use std::collections::{BTreeMap, BTreeSet};
use std::ops::ControlFlow;

use crate::diagnostic::{
    Applicability, Diagnostic, DiagnosticKind, DiagnosticSet, Diagnostics, Edit,
    MAX_UNIQUE_DIAGNOSTICS, RelatedRole, ScopeEntry, Span,
};
use crate::parallel::{Parallelism, ordered_for_each_owned};
pub use crate::syntax::Provenance;
use crate::syntax::*;

#[path = "closures.rs"]
mod closures;
use closures::LambdaKind;
#[path = "computation.rs"]
mod computation;
#[path = "constants.rs"]
mod constants;
#[path = "control.rs"]
mod control;
#[path = "derive.rs"]
mod deriving;
#[path = "exceptions.rs"]
mod exceptions;
#[path = "exhaustiveness.rs"]
mod exhaustiveness;
#[path = "higher_kinds.rs"]
mod higher_kinds;
pub use higher_kinds::{Constructor, Partial};
#[path = "kg_capture.rs"]
pub(crate) mod kg_capture;
#[path = "polymorph.rs"]
mod polymorph;
pub(crate) use polymorph::builtin_instance_by_type;
#[path = "pure_rules.rs"]
pub(crate) mod pure_rules;
#[path = "recursion.rs"]
mod recursion;
#[path = "recursive.rs"]
mod recursive;
#[path = "regions.rs"]
mod regions;
#[path = "semantic.rs"]
pub mod semantic;
#[path = "type_arena.rs"]
mod type_arena;
#[path = "warnings.rs"]
mod warnings;
use polymorph::{Classes, Constraint, Inference, Scheme};

pub const MAX_VALUE_BYTES: usize = 64 * 1024;
/// The most bytes that one function keeps in stack frames in all: its array and list literals, its
/// string buffers and its closure environments, by the same conservative estimates as the limit of
/// one literal. What comes after the limit in emission order goes to the heap.
pub const MAX_FRAME_BYTES: usize = 64 * 1024;
/// The longest fixed-length array `[T; N]` (A16): LLVM's translation time grows with large
/// first-class aggregates passed by value.
pub const MAX_FIXED_ARRAY_LENGTH: u64 = 1024;

/// A12: one bit per declared region of a record; bit i is the i-th declared region.
pub(crate) type RegionMask = u16;
pub(crate) const MAX_RECORD_REGIONS: usize = 16;
/// A12: per region slot of a function result, the (parameter index, parameter slot) pairs it may borrow from.
pub(crate) type RegionSources = Vec<BTreeSet<(usize, usize)>>;

/// A12 Phase 2: a parameter with a region-quantified function type `{r} A -> B`. Calls of the
/// parameter with `arity` arguments borrow only from the inputs that `sources` names.
#[derive(Clone, Debug)]
pub(crate) struct CallbackContract {
    pub parameter: usize,
    pub arity: usize,
    pub sources: RegionSources,
}

/// A13 Phase 2: the declared regions of a named function's parameter slots. A parameter or result
/// written `ref {r} T {s}` has two slots: the reference, and the borrows inside its target, which
/// the reference's value does not hold itself.
#[derive(Clone, Debug)]
pub(crate) struct RegionSlots {
    /// Per parameter and slot, the index of the declared region that the slot names.
    pub parameters: Vec<Vec<Option<usize>>>,
    /// Per parameter, whether it is a reference with a named target region.
    pub targets: Vec<bool>,
    pub result_target: bool,
    /// A call may store some input in the target of an exclusive reference, so the function only
    /// runs in direct calls with every argument.
    pub writes: bool,
}

impl RegionSlots {
    /// The slots of parameter `index`, of type `ty`, that hold exclusive references with a named
    /// target region, each with the declared regions of the target's borrows.
    pub(crate) fn written_targets(
        &self,
        index: usize,
        ty: &Type,
        types: TypeContext<'_>,
    ) -> Vec<(usize, BTreeSet<usize>)> {
        let regions = &self.parameters[index];
        let declared = |mask: RegionMask| -> BTreeSet<usize> {
            (0..regions.len())
                .filter(|slot| mask & (1 << slot) != 0)
                .filter_map(|slot| regions[slot])
                .collect()
        };
        match ty {
            Type::Reference(_, true) if self.targets[index] => vec![(0, declared(2))],
            Type::Record(id, _) if types.records[*id].region_count == regions.len() => {
                let record = &types.records[*id];
                (0..regions.len())
                    .filter(|slot| record.exclusive_regions & (1 << slot) != 0)
                    .filter_map(|slot| {
                        let targets = record.region_targets.get(slot).copied().unwrap_or(0);
                        (targets != 0).then(|| (slot, declared(targets)))
                    })
                    .collect()
            }
            _ => Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Type {
    Error,
    Variable(String),
    Infer(usize),
    Partial(Box<Partial>),
    Application(Box<Type>, Box<[Type]>),
    Integer(u16, bool),
    Binary(u16),
    Decimal(u16),
    Simd(crate::simd::SimdType),
    Bool,
    Unit,
    Char,
    Utf8Char,
    String,
    Utf8String,
    /// A record declaration and its type arguments; non-generic records have
    /// none. The arguments are boxed so `Type` stays four words, which keeps
    /// every typed expression and the checker's recursive frames small.
    Record(usize, Box<[Type]>),
    /// A union declaration and its type arguments, boxed like `Record`.
    Union(usize, Box<[Type]>),
    Array(Box<Type>),
    /// `[T..]`, the fixed-length target of an exclusive slice `ref mut [T..]` (C08). It appears
    /// only as `Reference(ArrayView(T), true)`, never as a value, field, or element type.
    ArrayView(Box<Type>),
    /// `[T; N]` (A16): N elements stored inline, a value like a tuple. The length is a `Length`,
    /// or a length parameter `Variable("#N")` or inference variable until it is substituted.
    FixedArray(Box<Type>, Box<Type>),
    /// A length at the type level: of a fixed-length array, or a length argument (A16). It is
    /// never the type of a value.
    Length(u64),
    List(Box<Type>),
    Vec(Box<Type>),
    Tuple(Vec<Type>),
    Task(Box<Type>),
    /// An `extern type`: an opaque host handle named by its qualified name.
    Handle(Box<str>),
    Function(Box<FunctionParameters>, Box<Type>),
    Reference(Box<Type>, bool),
    /// `dyn C`: an owned value of some type with instances of the classes, stored as
    /// `{ data, vtable }` (A14). A leaf whose box keeps `Type` at four words.
    Dyn(Box<DynType>),
    /// `Rc<T>`, `Rc.Weak<T>`, `Arc<T>`, or `Arc.Weak<T>` (C10 Phase 2): a pointer to a
    /// reference-counted heap block `{ i64 strong, i64 weak, T value }`.
    Shared(Box<Type>, SharedKind),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ParameterConvention {
    Own,
    Read,
    Mut,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct FunctionParameters {
    pub types: Vec<Type>,
    pub conventions: Box<[ParameterConvention]>,
    pub pure: bool,
}

impl std::ops::Deref for FunctionParameters {
    type Target = Vec<Type>;

    fn deref(&self) -> &Self::Target {
        &self.types
    }
}

impl std::ops::DerefMut for FunctionParameters {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.types
    }
}

impl FunctionParameters {
    pub fn convention(&self, index: usize) -> ParameterConvention {
        self.conventions
            .get(index)
            .copied()
            .unwrap_or(ParameterConvention::Own)
    }
}

/// The pointer kind of `Type::Shared` (C10 Phase 2). Strong pointers own the value together;
/// weak pointers keep only the block. `Arc` counts with atomic operations.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum SharedKind {
    Rc,
    RcWeak,
    Arc,
    ArcWeak,
}

impl SharedKind {
    pub const ALL: [Self; 4] = [Self::Rc, Self::RcWeak, Self::Arc, Self::ArcWeak];

    pub fn atomic(self) -> bool {
        matches!(self, Self::Arc | Self::ArcWeak)
    }

    pub fn weak(self) -> bool {
        matches!(self, Self::RcWeak | Self::ArcWeak)
    }

    /// The type name as source code writes it.
    pub fn name(self) -> &'static str {
        match self {
            Self::Rc => "Rc",
            Self::RcWeak => "Rc.Weak",
            Self::Arc => "Arc",
            Self::ArcWeak => "Arc.Weak",
        }
    }

    /// The kind that a type name such as `Rc.Weak` or `std::Arc` names.
    pub fn named(name: &str) -> Option<Self> {
        let name = name
            .strip_prefix(crate::stdlib::NAMESPACE)
            .and_then(|rest| rest.strip_prefix("::"))
            .unwrap_or(name);
        Self::ALL.into_iter().find(|kind| kind.name() == name)
    }

    /// The strong kind of the same module.
    pub fn strong(self) -> Self {
        if self.atomic() { Self::Arc } else { Self::Rc }
    }

    /// The weak kind of the same module.
    pub fn weakened(self) -> Self {
        if self.atomic() {
            Self::ArcWeak
        } else {
            Self::RcWeak
        }
    }
}

/// The classes and markers of a `dyn` type (A14).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct DynType {
    /// The dispatched classes by their keys in `Classes::names`, sorted and without duplicates.
    pub classes: Box<[Box<str>]>,
    /// `Copy` among the classes: the vtable clones the stored value, so the dyn value is Copy.
    pub copy: bool,
    /// `Send` among the classes: the stored value is Send, and so is the dyn value.
    pub send: bool,
    /// Written with a region, `dyn C {r}`: the stored value may hold shared borrows.
    pub borrowed: bool,
}

impl DynType {
    /// The type whose vtables serve this one: `Send` and the region change no slot.
    pub(crate) fn vtable_key(&self) -> Self {
        Self {
            classes: self.classes.clone(),
            copy: self.copy,
            send: false,
            borrowed: false,
        }
    }

    pub fn display(&self) -> String {
        let mut names: Vec<String> = self.classes.iter().map(|name| type_display(name)).collect();
        if self.copy {
            names.push("Copy".into());
        }
        if self.send {
            names.push("Send".into());
        }
        let mut text = match names.as_slice() {
            [name] => format!("dyn {name}"),
            _ => format!("dyn ({})", names.join(", ")),
        };
        if self.borrowed {
            text.push_str(" {_}");
        }
        text
    }
}

impl Type {
    pub const I32: Self = Self::Integer(32, true);
    pub const I64: Self = Self::Integer(64, true);
    pub const F64: Self = Self::Binary(64);

    /// The element type of a slice, which is a `%tz.array` value: a shared `ref [T]` or an
    /// exclusive `ref mut [T..]` (C08).
    pub(crate) fn slice_element(&self) -> Option<&Type> {
        match self {
            Self::Reference(inner, false) => match inner.as_ref() {
                Self::Array(element) => Some(element),
                _ => None,
            },
            Self::Reference(inner, true) => match inner.as_ref() {
                Self::ArrayView(element) => Some(element),
                _ => None,
            },
            _ => None,
        }
    }

    /// An exclusive slice `ref mut [T..]` (C08).
    pub(crate) fn is_view(&self) -> bool {
        matches!(self, Self::Reference(inner, true) if matches!(**inner, Self::ArrayView(_)))
    }

    /// The length of a fixed-length array whose length is known (A16).
    pub(crate) fn fixed_length(&self) -> Option<u64> {
        match self {
            Self::FixedArray(_, length) => match **length {
                Self::Length(length) => Some(length),
                _ => None,
            },
            _ => None,
        }
    }

    /// The type that dereferencing a reference of this type reads: the target, except that the
    /// elements of an exclusive slice read as the array `[T]`.
    pub(crate) fn dereferenced(&self) -> Option<Type> {
        match self {
            Self::Reference(inner, _) => Some(match inner.as_ref() {
                Self::ArrayView(element) => Self::Array(element.clone()),
                inner => inner.clone(),
            }),
            _ => None,
        }
    }

    pub fn function(parameters: Vec<Type>, result: Type) -> Self {
        Self::function_with_conventions(parameters, result, Vec::new(), false)
    }

    pub(crate) fn function_purity(&self) -> Option<bool> {
        match self {
            Self::Function(parameters, _) => Some(parameters.pure),
            Self::Reference(inner, false) => inner.function_purity(),
            _ => None,
        }
    }

    pub(crate) fn set_function_purity(&mut self, pure: bool) {
        match self {
            Self::Function(parameters, _) => parameters.pure = pure,
            Self::Reference(inner, false) => inner.set_function_purity(pure),
            _ => {}
        }
    }

    pub(crate) fn function_with_conventions(
        mut parameters: Vec<Type>,
        mut result: Type,
        mut conventions: Vec<ParameterConvention>,
        pure: bool,
    ) -> Self {
        if conventions
            .iter()
            .all(|mode| *mode == ParameterConvention::Own)
        {
            conventions.clear();
        }
        if !pure && !parameters.is_empty() {
            while matches!(&result, Self::Function(more, _) if !more.is_empty() && !more.pure) {
                let Self::Function(more, tail) = result else {
                    unreachable!()
                };
                if !more.conventions.is_empty() {
                    conventions.resize(parameters.len(), ParameterConvention::Own);
                    conventions.extend_from_slice(&more.conventions);
                } else if !conventions.is_empty() {
                    conventions.resize(parameters.len() + more.len(), ParameterConvention::Own);
                }
                parameters.extend(more.types);
                result = *tail;
            }
        }
        Self::Function(
            Box::new(FunctionParameters {
                types: parameters,
                conventions: conventions.into_boxed_slice(),
                pure,
            }),
            Box::new(result),
        )
    }

    pub(crate) fn after_arguments(&self, count: usize) -> Self {
        if self.contains_error() {
            return Self::Error;
        }
        let Self::Function(parameters, result) = self else {
            unreachable!()
        };
        if count == parameters.len() {
            (**result).clone()
        } else {
            Self::function_with_conventions(
                parameters[count..].to_vec(),
                (**result).clone(),
                parameters.conventions.get(count..).unwrap_or(&[]).to_vec(),
                parameters.pure,
            )
        }
    }

    pub fn display(&self, types: &TypeContext<'_>) -> String {
        match self {
            Self::Error => "an erroneous type".into(),
            // A length parameter `#N` (A16) reads as its declared name.
            Self::Variable(name) => match name.strip_prefix('#') {
                Some(length) => length.into(),
                None => format!("'{name}"),
            },
            Self::Infer(_) => "an undetermined type".into(),
            Self::Partial(partial) => partial.display(types),
            Self::Application(head, arguments) => format!(
                "{}<{}>",
                head.display(types),
                arguments
                    .iter()
                    .map(|ty| ty.display(types))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Self::Integer(bits, signed) => format!("i{bits}{}", if *signed { "" } else { "u" }),
            Self::Binary(bits) => format!("f{bits}"),
            Self::Decimal(bits) => format!("d{bits}"),
            Self::Simd(ty) => ty.name(),
            Self::Bool => "bool".into(),
            Self::Char => "char".into(),
            Self::Utf8Char => "utf8char".into(),
            Self::Unit => "unit".into(),
            Self::String => "string".into(),
            Self::Utf8String => "utf8string".into(),
            Self::Reference(ty, mutable) => {
                let inner = ty.display(types);
                let inner = if matches!(**ty, Self::Function(..)) {
                    format!("({inner})")
                } else {
                    inner
                };
                format!("ref {}{inner}", if *mutable { "mut " } else { "" })
            }
            Self::Record(id, args) | Self::Union(id, args) => {
                let mut text = type_display(match self {
                    Self::Record(..) => &types.records[*id].name,
                    _ => &types.unions[*id].name,
                });
                if !args.is_empty() {
                    text.push('<');
                    text.push_str(
                        &args
                            .iter()
                            .map(|arg| arg.display(types))
                            .collect::<Vec<_>>()
                            .join(", "),
                    );
                    text.push('>');
                }
                text
            }
            Self::Vec(element) => format!("Vec<{}>", element.display(types)),
            Self::Array(element) => {
                format!("[{}]", element.display(types))
            }
            Self::ArrayView(element) => format!("[{}..]", element.display(types)),
            Self::FixedArray(element, length) => {
                format!("[{}; {}]", element.display(types), length.display(types))
            }
            Self::Length(length) => length.to_string(),
            Self::List(element) => format!("[|{}|]", element.display(types)),
            Self::Tuple(elements) => format!(
                "({})",
                elements
                    .iter()
                    .map(|ty| {
                        let text = ty.display(types);
                        if matches!(ty, Self::Function(..)) {
                            format!("({text})")
                        } else {
                            text
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(" * ")
            ),
            Self::Task(result) => format!("Task<{}>", result.display(types)),
            Self::Handle(name) => type_display(name),
            Self::Dyn(dyn_type) => dyn_type.display(),
            Self::Shared(value, kind) => format!("{}<{}>", kind.name(), value.display(types)),
            Self::Function(parameters, _) if parameters.pure => {
                let mut ordinary = self.clone();
                let Self::Function(parameters, _) = &mut ordinary else {
                    unreachable!()
                };
                parameters.pure = false;
                format!("pure ({})", ordinary.display(types))
            }
            Self::Function(parameters, result) if parameters.is_empty() => {
                format!("fn() -> {}", result.display(types))
            }
            Self::Function(parameters, result) => {
                let mut parts: Vec<_> = parameters
                    .iter()
                    .map(|parameter| {
                        let text = parameter.display(types);
                        if matches!(parameter, Self::Function(..)) {
                            format!("({text})")
                        } else {
                            text
                        }
                    })
                    .collect();
                parts.push(result.display(types));
                parts.join(" -> ")
            }
        }
    }

    pub fn contains_error(&self) -> bool {
        match self {
            Self::Error => true,
            Self::Partial(partial) => partial.trailing.iter().any(Self::contains_error),
            Self::Application(head, arguments) => {
                head.contains_error() || arguments.iter().any(Self::contains_error)
            }
            Self::Array(ty)
            | Self::ArrayView(ty)
            | Self::List(ty)
            | Self::Vec(ty)
            | Self::Task(ty)
            | Self::Shared(ty, _)
            | Self::Reference(ty, _) => ty.contains_error(),
            Self::Function(parameters, result) => {
                parameters.iter().any(Self::contains_error) || result.contains_error()
            }
            Self::Tuple(elements) => elements.iter().any(Self::contains_error),
            Self::FixedArray(element, length) => {
                element.contains_error() || length.contains_error()
            }
            Self::Record(_, args) | Self::Union(_, args) => args.iter().any(Self::contains_error),
            _ => false,
        }
    }

    pub(crate) fn contains_constructor(&self) -> bool {
        match self {
            Self::Partial(_) | Self::Application(..) => true,
            Self::Array(ty)
            | Self::ArrayView(ty)
            | Self::FixedArray(ty, _)
            | Self::List(ty)
            | Self::Vec(ty)
            | Self::Task(ty)
            | Self::Shared(ty, _)
            | Self::Reference(ty, _) => ty.contains_constructor(),
            Self::Function(parameters, result) => {
                parameters.iter().any(Self::contains_constructor) || result.contains_constructor()
            }
            Self::Tuple(elements) => elements.iter().any(Self::contains_constructor),
            Self::Record(_, arguments) | Self::Union(_, arguments) => {
                arguments.iter().any(Self::contains_constructor)
            }
            _ => false,
        }
    }

    pub fn is_scalar(&self) -> bool {
        self.is_numeric() || matches!(self, Self::Bool | Self::Char | Self::Utf8Char)
    }

    pub fn is_numeric(&self) -> bool {
        matches!(self, Self::Integer(..) | Self::Binary(_) | Self::Decimal(_))
    }

    pub fn is_integer(&self) -> bool {
        matches!(self, Self::Integer(..))
    }

    pub fn is_float(&self) -> bool {
        matches!(self, Self::Binary(_) | Self::Decimal(_))
    }

    pub fn is_string(&self) -> bool {
        matches!(self, Self::String | Self::Utf8String)
    }

    pub fn is_copy(&self, types: &TypeContext<'_>) -> bool {
        if types.recursive(self) || self.is_noncopy_record(types) {
            return false;
        }
        match self {
            Self::String
            | Self::Utf8String
            | Self::Vec(_)
            | Self::Task(_)
            | Self::Handle(_)
            // Sharing is explicit (`Rc.share`), never an implicit copy (C10).
            | Self::Shared(..)
            | Self::Reference(_, true)
            | Self::Partial(_)
            | Self::Application(..)
            | Self::Variable(_)
            | Self::Infer(_) => false,
            Self::Record(id, args) => types.record_fields_all(*id, args, |ty| ty.is_copy(types)),
            Self::Union(id, args) => types.union_payloads_all(*id, args, |ty| ty.is_copy(types)),
            // A dyn value owns its data; only a vtable with a clone slot copies it (A14).
            Self::Dyn(dyn_type) => dyn_type.copy,
            Self::Array(element) | Self::List(element) | Self::FixedArray(element, _) => {
                element.is_copy(types)
            }
            Self::Tuple(elements) => elements.iter().all(|ty| ty.is_copy(types)),
            _ => true,
        }
    }

    pub fn needs_drop(&self, types: &TypeContext<'_>) -> bool {
        if types.recursive(self) || self.has_user_drop(types) {
            return true;
        }
        match self {
            Self::String
            | Self::Utf8String
            | Self::Function(..)
            | Self::Task(_)
            | Self::Dyn(_)
            | Self::Shared(..) => true,
            Self::Record(id, args) => {
                !types.record_fields_all(*id, args, |ty| !ty.needs_drop(types))
            }
            Self::Union(id, args) => {
                !types.union_payloads_all(*id, args, |ty| !ty.needs_drop(types))
            }
            Self::Array(_) | Self::List(_) | Self::Vec(_) => true,
            // A fixed-length array owns nothing but its elements (A16).
            Self::FixedArray(element, _) => element.needs_drop(types),
            Self::Tuple(elements) => elements.iter().any(|ty| ty.needs_drop(types)),
            _ => false,
        }
    }

    pub(crate) fn is_noncopy_record(&self, types: &TypeContext<'_>) -> bool {
        self.has_user_drop(types)
            || matches!(self, Self::Record(id, _) if types.records[*id].origin == ModuleOrigin::Std && matches!(types.records[*id].name.as_str(), "Seq.Seq" | "Gpu.Device" | "Gpu.Buffer" | "Owned.Function" | "Regex.Regex" | "Async.Async" | "Async.Next" | "Matrix.Matrix" | "Tensor.Tensor" | "Tensor.View" | "Atomic.Atomic" | "Mutex.Mutex"))
    }

    /// The std `Atomic.Atomic` and `Mutex.Mutex`, whose cell changes under a shared borrow (F10).
    /// Copying one would make an independent cell, so neither is Copy or Capture.
    pub(crate) fn is_shared_cell(&self, types: &TypeContext<'_>) -> bool {
        matches!(self, Self::Record(id, _) if types.records[*id].origin == ModuleOrigin::Std && matches!(types.records[*id].name.as_str(), "Atomic.Atomic" | "Mutex.Mutex"))
    }

    /// The std `Mutex.Mutex`. `Mutex.create` needs `Send` of its value, so a `Mutex<T>` exists
    /// only for a `T` that may move between threads, which is all that `Sync` asks of it.
    fn is_mutex(&self, types: &TypeContext<'_>) -> bool {
        matches!(self, Self::Record(id, _) if types.records[*id].origin == ModuleOrigin::Std && types.records[*id].name == "Mutex.Mutex")
    }

    /// Std records that borrows must not share between threads: a lazy sequence and an async
    /// computation hold function values whose environments are not visible, and GPU handles are
    /// not thread-safe (F10 D6).
    fn is_unsync_record(&self, types: &TypeContext<'_>) -> bool {
        matches!(self, Self::Record(id, _) if types.records[*id].origin == ModuleOrigin::Std && matches!(types.records[*id].name.as_str(), "Seq.Seq" | "Gpu.Device" | "Gpu.Buffer" | "Async.Async" | "Async.Next" | "Owned.Function"))
    }

    /// The std `Async.Async`, whose values hold no loans (B08 D6).
    pub(crate) fn is_async(&self, types: &TypeContext<'_>) -> bool {
        matches!(self, Self::Record(id, _) if types.records[*id].origin == ModuleOrigin::Std && types.records[*id].name == "Async.Async")
    }

    /// The std `Owned.Function`, whose environment has no clone (B07).
    pub(crate) fn is_owned_function(&self, types: &TypeContext<'_>) -> bool {
        matches!(self, Self::Record(id, _) if types.records[*id].origin == ModuleOrigin::Std && types.records[*id].name == "Owned.Function")
    }

    /// A record or union with a user `Drop` instance, whatever its type arguments (B07).
    pub(crate) fn has_user_drop(&self, types: &TypeContext<'_>) -> bool {
        match self {
            Self::Record(id, _) => types.records[*id].user_drop,
            Self::Union(id, _) => types.unions[*id].user_drop,
            _ => false,
        }
    }

    /// Whether destroying a value of this type runs a user `Drop`: the type or a type that it
    /// stores (not through a reference) has one.
    pub(crate) fn holds_user_drop(&self, types: &TypeContext<'_>) -> bool {
        !types.stored_all(self, |part| !part.has_user_drop(types))
    }

    pub(crate) fn sequence_element(&self, types: &TypeContext<'_>) -> Option<&Type> {
        match self {
            Self::Record(id, arguments)
                if types.records[*id].origin == ModuleOrigin::Std
                    && types.records[*id].name == "Seq.Seq" =>
            {
                arguments.first()
            }
            _ => None,
        }
    }

    pub fn contains_reference(&self) -> bool {
        match self {
            Self::Reference(..) => true,
            // `dyn C {r}` may hold shared borrows (A14 Phase 2).
            Self::Dyn(dyn_type) => dyn_type.borrowed,
            Self::Array(element)
            | Self::List(element)
            | Self::Vec(element)
            | Self::FixedArray(element, _)
            | Self::Shared(element, _) => element.contains_reference(),
            Self::Tuple(elements) => elements.iter().any(Self::contains_reference),
            Self::Union(_, arguments) => arguments.iter().any(Self::contains_reference),
            _ => false,
        }
    }

    fn contains_mutable_reference(&self) -> bool {
        match self {
            Self::Reference(_, true) => true,
            Self::Reference(value, false)
            | Self::Array(value)
            | Self::List(value)
            | Self::Vec(value)
            | Self::FixedArray(value, _)
            | Self::Shared(value, _) => value.contains_mutable_reference(),
            Self::Tuple(elements) => elements.iter().any(Self::contains_mutable_reference),
            Self::Union(_, arguments) => arguments.iter().any(Self::contains_mutable_reference),
            // Function signatures describe calls, not stored references; captures are checked separately.
            _ => false,
        }
    }

    pub(crate) fn carries_loans(&self, types: &TypeContext<'_>) -> bool {
        if types.recursive(self) {
            return !types.stored_all(self, |ty| {
                !matches!(ty, Type::Reference(..) | Type::Function(..))
                    && !matches!(ty, Type::Dyn(dyn_type) if dyn_type.borrowed)
            });
        }
        match self {
            Self::Reference(..) | Self::Function(..) => true,
            Self::Dyn(dyn_type) => dyn_type.borrowed,
            Self::Array(element)
            | Self::List(element)
            | Self::Vec(element)
            | Self::FixedArray(element, _)
            | Self::Shared(element, _) => element.carries_loans(types),
            Self::Tuple(elements) => elements.iter().any(|ty| ty.carries_loans(types)),
            Self::Record(id, args) => {
                !types.record_fields_all(*id, args, |ty| !ty.carries_loans(types))
            }
            Self::Union(id, args) => {
                !types.union_payloads_all(*id, args, |ty| !ty.carries_loans(types))
            }
            _ => false,
        }
    }

    pub(crate) fn contains_stored_reference(&self, types: &TypeContext<'_>) -> bool {
        !types.stored_all(self, |ty| {
            !matches!(ty, Self::Reference(..))
                && !matches!(ty, Self::Dyn(dyn_type) if dyn_type.borrowed)
        })
    }

    /// A SIMD vector wider than 128 bits is part of the value itself rather than behind a
    /// pointer (F08 Phase 3). A union counts its payloads although they may share storage.
    pub(crate) fn holds_wide_vector(&self, types: &TypeContext<'_>) -> bool {
        if types.recursive(self) {
            return false;
        }
        match self {
            Self::Simd(vector) => vector.width > 128,
            Self::Tuple(elements) => elements.iter().any(|ty| ty.holds_wide_vector(types)),
            Self::FixedArray(element, _) => element.holds_wide_vector(types),
            Self::Record(id, args) => {
                !types.record_fields_all(*id, args, |ty| !ty.holds_wide_vector(types))
            }
            Self::Union(id, args) => {
                !types.union_payloads_all(*id, args, |ty| !ty.holds_wide_vector(types))
            }
            _ => false,
        }
    }

    pub(crate) fn contains_stored_mutable_reference(&self, types: &TypeContext<'_>) -> bool {
        !types.stored_all(self, |ty| !ty.contains_mutable_reference())
    }

    /// A13: an exclusive reference is reachable through stored values, records, or references.
    pub(crate) fn reaches_exclusive(&self, types: &TypeContext<'_>) -> bool {
        types.reaches(self, |ty| matches!(ty, Type::Reference(_, true)))
    }

    /// A13: a record with exclusive regions is reachable through stored values or references.
    pub(crate) fn holds_exclusive_record(&self, types: &TypeContext<'_>) -> bool {
        types.reaches(
            self,
            |ty| matches!(ty, Type::Record(id, _) if types.records[*id].exclusive_regions != 0),
        )
    }

    pub(crate) fn can_capture(&self, types: &TypeContext<'_>) -> bool {
        // Copying a function value copies its captures, which would drop a resource twice, and
        // would make a second, independent counter or lock out of an `Atomic` or `Mutex`.
        if self.has_user_drop(types) || self.is_owned_function(types) || self.is_shared_cell(types)
        {
            return false;
        }
        if types.recursive(self) {
            // The value of an `Arc` is shared, not copied, so what it holds is judged by `Sync` at
            // the pointer and a cell behind it may be reached by every copy (as for a type that
            // is not recursive); only a cell that the value owns outright is copied.
            return types.stored_all_closed(
                self,
                |ty| {
                    !matches!(
                        ty,
                        Type::Reference(_, true) | Type::Task(_) | Type::Handle(_)
                    ) && !matches!(ty, Type::Dyn(dyn_type) if !dyn_type.copy)
                        && !matches!(ty, Type::Shared(_, kind) if !kind.atomic())
                        && !matches!(ty, Type::Shared(value, kind) if !value.can_sync(types) && kind.atomic())
                        && !ty.has_user_drop(types)
                        && !ty.is_owned_function(types)
                        && !ty.is_shared_cell(types)
                },
                |ty| matches!(ty, Type::Shared(..)),
            );
        }
        match self {
            Self::Reference(_, true) | Self::Task(_) | Self::Handle(_) => false,
            // Function values are Send whatever they capture, so they hold only values that may
            // move to another task: an `Arc` whose value tasks may share, never an `Rc` (C10).
            Self::Shared(value, kind) => kind.atomic() && value.can_sync(types),
            // Copying a function value clones its captures, which needs the vtable's clone slot.
            Self::Dyn(dyn_type) => dyn_type.copy,
            Self::Array(element)
            | Self::List(element)
            | Self::Vec(element)
            | Self::FixedArray(element, _) => element.can_capture(types),
            Self::Tuple(elements) => elements.iter().all(|ty| ty.can_capture(types)),
            Self::Record(id, args) => {
                types.record_fields_all(*id, args, |ty| ty.can_capture(types))
            }
            Self::Union(id, args) => {
                types.union_payloads_all(*id, args, |ty| ty.can_capture(types))
            }
            _ => true,
        }
    }

    pub fn exportable(&self) -> bool {
        matches!(
            self,
            Self::Integer(8 | 16 | 32 | 64, _) | Self::Binary(32 | 64) | Self::Bool
        )
    }

    /// Whether this type stands for a type that is not known yet: a type variable, an inference
    /// variable, or a constructor variable applied to arguments (`'f<i64>`) or applied in part.
    /// What it stands for is judged where it is substituted, not where it is written.
    fn is_unresolved(&self) -> bool {
        matches!(
            self,
            Self::Variable(_) | Self::Infer(_) | Self::Partial(_) | Self::Application(..)
        )
    }

    pub(crate) fn can_send(&self, types: &TypeContext<'_>) -> bool {
        if types.recursive(self) {
            return types.stored_all(self, |ty| {
                !ty.is_unresolved()
                    && !matches!(ty, Type::Reference(..))
                    && !matches!(ty, Type::Dyn(dyn_type) if !dyn_type.send || dyn_type.borrowed)
                    && !matches!(ty, Type::Shared(value, kind) if !kind.atomic() || !value.can_sync(types))
            });
        }
        match self {
            // A type variable has no known value yet, nor has a constructor variable applied to
            // arguments (`'f<i64>`). `false` keeps `Send<'a>` as a constraint on a generic
            // function, which is checked again for every type that it is used at (as `Sync<'a>`
            // is); `true` would discharge it where the function is written and let an `Rc` through
            // a wrapper of `Channel.bounded`, `Mutex.create` or `Parallel.init`. Every kind of type
            // is named here and below, so that a new one has to be judged.
            Self::Variable(_)
            | Self::Infer(_)
            | Self::Partial(_)
            | Self::Application(..)
            | Self::Reference(..) => false,
            Self::Dyn(dyn_type) => dyn_type.send && !dyn_type.borrowed,
            // Rc counts are not atomic. As in Rust, an `Arc<T>` is Send when `T` is Send and
            // Sync: every owner reads the value through a shared borrow, and the last one drops it
            // (C10, F10).
            Self::Shared(value, kind) => {
                kind.atomic() && value.can_send(types) && value.can_sync(types)
            }
            Self::Array(element)
            | Self::List(element)
            | Self::Vec(element)
            | Self::FixedArray(element, _) => element.can_send(types),
            Self::Tuple(elements) => elements.iter().all(|ty| ty.can_send(types)),
            Self::Record(id, args) => types.record_fields_all(*id, args, |ty| ty.can_send(types)),
            Self::Union(id, args) => types.union_payloads_all(*id, args, |ty| ty.can_send(types)),
            // Plain data, a task (its result was judged where the task was made) and a function
            // value, whose environment is checked by ownership, not by its call signature.
            Self::Error
            | Self::Integer(..)
            | Self::Binary(_)
            | Self::Decimal(_)
            | Self::Simd(_)
            | Self::Bool
            | Self::Unit
            | Self::Char
            | Self::Utf8Char
            | Self::String
            | Self::Utf8String
            | Self::ArrayView(_)
            | Self::Length(_)
            | Self::Task(_)
            | Self::Handle(_)
            | Self::Function(..) => true,
        }
    }

    /// Whether a value of this type stores a value of a recursive type, which may hold a shared
    /// pointer to a block of this type again (C10).
    pub(crate) fn reaches_recursive(&self, types: &TypeContext<'_>) -> bool {
        !types.stored_all(self, |ty| !types.recursive(ty))
    }

    /// Whether a value of this type owns an `Rc` or `Rc.Weak`, whose counts are not atomic (C10).
    pub(crate) fn holds_rc(&self, types: &TypeContext<'_>) -> bool {
        !types.stored_all(
            self,
            |ty| !matches!(ty, Type::Shared(_, kind) if !kind.atomic()),
        )
    }

    /// Whether several threads may hold shared borrows of a value of this type at once, which is
    /// what a task scope asks of the value it shares and what an `Arc` asks of its value (F10).
    ///
    /// Immutable data is shared freely. Memory changes under a shared borrow in three places:
    /// an `Atomic` (Sync), a `Mutex<T>` (Sync: `Mutex.create` admits only a `T` that may move
    /// between threads, so the type stands for `T` without being looked into), and the counts of
    /// shared pointers (an `Arc` counts atomically and is Sync when its value is; an `Rc` counts
    /// through `Rc.share`'s shared borrow and is never Sync). Function values are Sync: they
    /// capture only values that tasks may hold, and `Task.scope` checks that the environment of
    /// a function it shares is proven owned. Not Sync are an extern handle (host libraries are
    /// rarely thread-safe), a dyn value that is not Copy (it may hide one), a task, an exclusive
    /// reference, an `Owned.Function`, a lazy sequence, an async computation and the GPU handles.
    pub(crate) fn can_sync(&self, types: &TypeContext<'_>) -> bool {
        types.stored_all_closed(self, |ty| ty.sync_here(types), |ty| ty.sync_closed(types))
    }

    fn sync_here(&self, types: &TypeContext<'_>) -> bool {
        match self {
            // A type variable has no known value yet, nor has a constructor variable applied to
            // arguments (`'f<i64>`). `false` keeps `Sync<'a>` as a constraint on a generic
            // function, which is checked again for every type that it is used at. Every kind of
            // type is named here, so that a new one has to be judged.
            Self::Variable(_) | Self::Infer(_) | Self::Partial(_) | Self::Application(..) => false,
            Self::Reference(_, true) | Self::Task(_) | Self::Handle(_) => false,
            Self::Reference(value, false) => value.can_sync(types),
            // A Copy dyn value holds plain data; any other may hide a host handle.
            Self::Dyn(dyn_type) => dyn_type.copy && !dyn_type.borrowed,
            Self::Shared(_, kind) => kind.atomic(),
            Self::Record(..) => !self.is_unsync_record(types),
            // Plain data, and the types whose parts the walk of `can_sync` judges in turn.
            Self::Error
            | Self::Integer(..)
            | Self::Binary(_)
            | Self::Decimal(_)
            | Self::Simd(_)
            | Self::Bool
            | Self::Unit
            | Self::Char
            | Self::Utf8Char
            | Self::String
            | Self::Utf8String
            | Self::Union(..)
            | Self::Array(_)
            | Self::ArrayView(_)
            | Self::FixedArray(..)
            | Self::Length(_)
            | Self::List(_)
            | Self::Vec(_)
            | Self::Tuple(_)
            | Self::Function(..) => true,
        }
    }

    /// The types whose contents `sync_here` has already judged, or that need no judgement.
    fn sync_closed(&self, types: &TypeContext<'_>) -> bool {
        matches!(self, Self::Reference(..)) || self.is_mutex(types)
    }

    /// Whether a shared borrow of a value of this type reaches memory that other threads change:
    /// an `Atomic` or a `Mutex`, however deep (F10). A reference to a type without it never sees
    /// a change under it, which is what `noalias readonly` on such a parameter asks (PR01).
    #[allow(dead_code)]
    pub(crate) fn has_interior_mutability(&self, types: &TypeContext<'_>) -> bool {
        types.reaches(self, |ty| ty.is_shared_cell(types))
    }

    /// Whether reachable memory is proven immutable under a shared borrow.
    /// Unknown types and callable/host environments cannot supply this proof.
    pub(crate) fn is_frozen(&self, types: &TypeContext<'_>) -> bool {
        !types.reaches(self, |ty| {
            ty.is_shared_cell(types)
                || matches!(ty, Self::Record(id, _) if types.records[*id].opaque())
                || matches!(
                    ty,
                    Self::Error
                        | Self::Variable(_)
                        | Self::Infer(_)
                        | Self::Partial(_)
                        | Self::Application(..)
                        | Self::Shared(..)
                        | Self::Handle(_)
                        | Self::Function(..)
                        | Self::Task(_)
                        | Self::Dyn(_)
                        | Self::Reference(_, true)
                )
        })
    }

    /// Whether a value of this type owns an `Atomic` or `Mutex` outright, not behind an `Arc`.
    pub(crate) fn holds_cell(&self, types: &TypeContext<'_>) -> bool {
        !types.stored_all_closed(
            self,
            |ty| !ty.is_shared_cell(types),
            |ty| matches!(ty, Type::Shared(..)),
        )
    }

    /// Whether a value of this type owns an `Arc` or `Arc.Weak` whose value tasks may not share
    /// (C10, F10).
    pub(crate) fn holds_unsync_arc(&self, types: &TypeContext<'_>) -> bool {
        !types.stored_all(self, |ty| {
            !matches!(ty, Type::Shared(value, kind) if kind.atomic() && !value.can_sync(types))
        })
    }

    /// Whether a value of this type reaches what makes a host call unsafe to share: an extern
    /// handle, a dyn value that is not Copy, or an `Owned.Function` (C10).
    pub(crate) fn holds_host_state(&self, types: &TypeContext<'_>) -> bool {
        types.reaches(self, |ty| {
            matches!(ty, Type::Handle(_))
                || matches!(ty, Type::Dyn(dyn_type) if !dyn_type.copy)
                || ty.is_owned_function(types)
        })
    }
}

/// The declarations that give meaning to named types. Type properties,
/// layouts, and code generation read record and union instances through this
/// context so that field and payload types are always substituted with the
/// instance's arguments.
#[derive(Clone, Copy)]
pub struct TypeContext<'a> {
    pub records: &'a [CheckedRecord],
    pub unions: &'a [CheckedUnion],
}

impl TypeContext<'_> {
    /// Field types of `Record(id, args)` in declaration order. `args` must
    /// already match the declaration's arity.
    pub fn record_fields(&self, id: usize, args: &[Type]) -> Vec<Type> {
        let record = &self.records[id];
        if args.is_empty() {
            return record.fields.iter().map(|(_, ty)| ty.clone()).collect();
        }
        let substitutions = type_parameter_substitutions(&record.parameters, args);
        record
            .fields
            .iter()
            .map(|(_, ty)| substitute_type_parameters(ty, &substitutions))
            .collect()
    }

    pub fn record_field(&self, id: usize, args: &[Type], field: usize) -> Type {
        let record = &self.records[id];
        let ty = &record.fields[field].1;
        if args.is_empty() {
            return ty.clone();
        }
        substitute_type_parameters(ty, &type_parameter_substitutions(&record.parameters, args))
    }

    pub(crate) fn record_fields_all(
        &self,
        id: usize,
        args: &[Type],
        test: impl Fn(&Type) -> bool,
    ) -> bool {
        let record = &self.records[id];
        if args.is_empty() {
            record.fields.iter().all(|(_, ty)| test(ty))
        } else {
            self.record_fields(id, args).iter().all(test)
        }
    }

    /// Payload types of `Union(id, args)` in case (tag) order; nullary
    /// cases have none. `args` must already match the declaration's arity.
    pub fn union_payloads(&self, id: usize, args: &[Type]) -> Vec<Option<Type>> {
        let union = &self.unions[id];
        if args.is_empty() {
            return union.cases.iter().map(|(_, ty)| ty.clone()).collect();
        }
        let substitutions = type_parameter_substitutions(&union.parameters, args);
        union
            .cases
            .iter()
            .map(|(_, ty)| {
                ty.as_ref()
                    .map(|ty| substitute_type_parameters(ty, &substitutions))
            })
            .collect()
    }

    pub fn union_payload(&self, id: usize, args: &[Type], case: usize) -> Option<Type> {
        let union = &self.unions[id];
        let ty = union.cases[case].1.as_ref()?;
        if args.is_empty() {
            return Some(ty.clone());
        }
        Some(substitute_type_parameters(
            ty,
            &type_parameter_substitutions(&union.parameters, args),
        ))
    }

    pub(crate) fn union_payloads_all(
        &self,
        id: usize,
        args: &[Type],
        test: impl Fn(&Type) -> bool,
    ) -> bool {
        let union = &self.unions[id];
        if args.is_empty() {
            union
                .cases
                .iter()
                .filter_map(|(_, ty)| ty.as_ref())
                .all(test)
        } else {
            self.union_payloads(id, args).iter().flatten().all(test)
        }
    }
}

pub(crate) fn type_parameter_substitutions(
    parameters: &[String],
    args: &[Type],
) -> BTreeMap<String, Type> {
    debug_assert_eq!(parameters.len(), args.len());
    parameters
        .iter()
        .cloned()
        .zip(args.iter().cloned())
        .collect()
}

pub(crate) fn substitute_type_parameters(
    ty: &Type,
    substitutions: &BTreeMap<String, Type>,
) -> Type {
    polymorph::substitute(ty, substitutions)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Builtin {
    Sqrt,
    Floor,
    Ceil,
    Abs,
    MathSqrt,
    MathFloor,
    MathCeil,
    MathTrunc,
    MathRound,
    MathRoundEven,
    MathAbs,
    MathMin,
    MathMax,
    MathClamp,
    MathFma,
    MathCopysign,
    MathIsNan,
    MathIsInfinite,
    MathIsFinite,
    MathPi,
    MathE,
    MathSin,
    MathCos,
    MathTan,
    MathAsin,
    MathAcos,
    MathAtan,
    MathAtan2,
    MathExp,
    MathExp2,
    MathLog,
    MathLog2,
    MathLog10,
    MathPow,
    MathCbrt,
    MathHypot,
    ToFloat,
    ToInt,
    Assert,
    CloneString,
    CloneUtf8String,
    StringFromUtf8,
    Utf8StringFromString,
    StringIsWellFormed,
    StringToWellFormed,
    TaskRun,
    TaskParallel,
    TaskParallelResults,
    ParallelInit,
    ParallelMap,
    ParallelMapRef,
    ParallelReduce,
    /// `Parallel.for_each_chunk :: Send<'a> => i64 -> (i64 -> ref mut ['a..] -> unit) -> ref mut ['a..] -> unit`
    /// runs the callback on disjoint chunks of an exclusive slice in parallel (C08 Phase 2).
    ParallelForEachChunk,
    /// `unreachable : unit -> 'a` traps (GUIDE D-21).
    Unreachable,
    ToString,
    DebugPrintString,
    IOReadLine,
    IOWrite,
    /// `Os.__read :: i32 -> ref utf8string -> (i64 * [ubyte])` (E08): reads a file, a directory, the current directory, or an environment variable.
    OsRead,
    /// `Os.__args :: unit -> (i64 * [ubyte])`: the NUL-terminated command-line arguments after the program name.
    OsArgs,
    /// `Os.__write :: i32 -> ref utf8string -> ref [ubyte] -> i64`: writes a file, or creates or removes a file or directory.
    OsWrite,
    /// `Os.__random :: i64 -> (i64 * [ubyte])`: operating-system random bytes.
    OsRandom,
    /// `Os.__clock :: i32 -> i64`: a monotonic or Unix clock in nanoseconds.
    OsClock,
    /// `Os.__sleep :: i64 -> i64`: sleeps for milliseconds.
    OsSleep,
    /// `Os.__open :: i32 -> ref utf8string -> i64`: opens a file; a negative result is the negated error status.
    OsOpen,
    /// `Os.__handle :: i32 -> i64 -> i64 -> ref [ubyte] -> (i64 * [ubyte])`: reads, writes, or flushes an open file.
    OsHandle,
    /// `Os.__close :: i64 -> i64`: closes an open file.
    OsClose,
    /// `Os.__spawn :: ref utf8string -> ref [ubyte] -> ref [ubyte] -> (i64 * [ubyte])`: runs a program without a
    /// shell, given its NUL-terminated arguments and its input, and returns the encoded outcome.
    OsSpawn,
    /// `Net.__resolve :: ref utf8string -> i64 -> (i64 * [ubyte])` (E09): host names to 20-byte address records.
    NetResolve,
    /// `Net.__open :: i32 -> i64 -> i64 -> i64 -> i64 -> (i64 * [ubyte])`: connects, listens, or binds UDP; a
    /// positive handle or the negated status, and the local and peer address records.
    NetOpen,
    /// `Net.__accept :: i64 -> i64 -> (i64 * [ubyte])`: a new handle (or the negated status) and its two address records.
    NetAccept,
    /// `Net.__read :: i32 -> i64 -> i64 -> i64 -> (i64 * [ubyte])`: one TCP receive or one datagram (with its source first).
    NetRead,
    /// `Net.__write :: i32 -> i64 -> ref [ubyte] -> i64 -> i64 -> i64 -> i64 -> i64`: all of a TCP stream or one datagram.
    NetWrite,
    /// `Net.__close :: i32 -> i64 -> i64`: closes a socket or shuts one direction down.
    NetClose,
    /// `Net.__classify :: i32 -> i64`: an `errno` as the declaration-order index of `Net.ErrorKind`.
    NetClassify,
    /// `Net.__watch :: i64 -> i64 -> i32 -> i64 -> unit` (E09 Phase 2): starts a wait of an async operation (its id)
    /// for a socket to be readable (1) or writable (2) within a timeout in milliseconds (-1: none). The runtime
    /// completes the operation: 0 when ready, otherwise a status.
    NetWatch,
    /// `Net.__unwatch :: i64 -> unit`: takes the operation back; a connect that is under way is closed.
    NetUnwatch,
    /// `Net.__connect :: i64 -> i64 -> i64 -> i64 -> i64 -> unit`: starts a connect for an async operation (id, address,
    /// timeout); the runtime completes it with a new handle or the negated status.
    NetConnect,
    /// `Net.__names :: i64 -> (i64 * [ubyte])`: the 40 bytes of local and peer address records of an open socket.
    NetNames,
    /// `Net.__send :: i32 -> i64 -> ref [ubyte] -> i64 -> i64 -> i64 -> i64 -> i64`: sends without waiting from an
    /// offset: the bytes sent, or the negated status (the "would wait" status among them).
    NetSend,
    /// `Unicode.__table_length :: i64 -> i64`: the number of entries of a generated Unicode table
    /// (`src/runtime/unicode.ll`). Private to the std Unicode and Regex modules (D09).
    UnicodeTableLength,
    /// `Unicode.__table_entry :: i64 -> i64 -> i64`: one entry of a generated Unicode table; traps
    /// when the table or the index is out of range.
    UnicodeTableEntry,
    Display,
    Parse,
    Default,
    Hash,
    HashMix,
    DisplayQuoted,
    /// `Array.__set :: i64 -> 'a -> ['a] -> ['a]` replaces one element of an owned array and returns it. It is private
    /// to the std `Array` module, whose `update_at` forwards to it. Like every array function, it takes the array last.
    ArraySet,
    /// `Array.update :: i64 -> ('a -> 'a) -> ['a] -> ['a]` replaces one element of an owned array with the result of
    /// the function and returns the array.
    ArrayUpdate,
    /// `Array.swap :: i64 -> i64 -> ['a] -> ['a]` exchanges two elements of an owned array and returns it.
    ArraySwap,
    /// `Array.write :: i64 -> 'a -> ref mut ['a..] -> unit` replaces one element in place (C08).
    ArrayWrite,
    /// `Array.swap_in :: i64 -> i64 -> ref mut ['a..] -> unit` exchanges two elements in place.
    ArraySwapIn,
    /// `Array.split_at_mut :: i64 -> ref mut ['a..] -> (ref mut ['a..] * ref mut ['a..])`.
    ArraySplitAtMut,
    /// `FixedArray.init :: (i64 -> 'a) -> ['a; N]`, whose length comes from the expected type (A16).
    FixedArrayInit,
    /// `Dyn.of :: 'a -> 'b`, where the expected type `'b` is a `dyn` type whose classes `'a`
    /// implements (A14). Only a direct application with one argument type-checks.
    DynOf,
    ListCons,
    ListTail,
    ArrayConcat,
    ArrayToList,
    /// `Array.__sort_with :: Copy<'a> => (ref 'a -> ref 'a -> i64) -> ref ['a] -> ['a]` is the stable merge sort behind
    /// the std `Array` sorts, which give it by-value comparers and keys. It is private to the std `Array` module.
    ArraySortBy,
    ListMap,
    ListMapRef,
    ListReverse,
    ListToArray,
    ListFoldRef,
    VecEmpty,
    SeqNext,
    SimdSplat,
    SimdOfLanes2,
    SimdOfLanes4,
    SimdOfLanes8,
    SimdOfLanes16,
    SimdOfLanes32,
    SimdExtract,
    SimdReplace,
    SimdLoad,
    SimdStore,
    SimdSum,
    SimdEq,
    SimdNe,
    SimdLt,
    SimdLe,
    SimdGt,
    SimdGe,
    SimdSelect,
    SimdAll,
    SimdAny,
    VecWithCapacity,
    VecLength,
    VecCapacity,
    VecIsEmpty,
    VecPush,
    VecPushMut,
    VecPop,
    VecReserve,
    VecTruncate,
    VecClear,
    VecSet,
    VecSwap,
    VecGet,
    VecAt,
    VecClone,
    VecOfArray,
    VecToArray,
    /// `Vec.__view :: ref mut Vec<'a> -> ref mut ['a..]` borrows the initialized elements of a vector as an exclusive
    /// slice, so the in-place `Array` algorithms run on its buffer and its capacity stays. Private to the std `Vec`
    /// module.
    VecView,
    /// `Vec.__set_capacity :: i64 -> Vec<'a> -> Vec<'a>` reallocates the buffer to exactly the given capacity, which
    /// must not be less than the length. Private to the std `Vec` module (the `Capacity` setter of .NET's `List<T>`).
    VecSetCapacity,
    /// `Vec.__insert :: i64 -> 'a -> Vec<'a> -> Vec<'a>` moves the elements from the index on one slot up and stores
    /// the new element there (`List<T>.Insert`). Private to the std `Vec` module.
    VecInsert,
    /// `Vec.__splice :: i64 -> i64 -> ['a] -> Vec<'a> -> Vec<'a>` drops `count` elements at the index, then moves the
    /// elements of an owned array into their place (`RemoveRange` and `InsertRange` of .NET's `List<T>`). Private to
    /// the std `Vec` module.
    VecSplice,
    CharToU16,
    CharOfU16,
    Utf8CharToU32,
    Utf8CharOfU32,
    Utf8CharOfU32Unchecked,
    StringToCodeUnits,
    StringFromCodeUnits,
    Utf8StringToBytes,
    Utf8StringFromBytes,
    Utf8StringDecodeAt,
    StringCompare,
    Utf8StringCompare,
    IntMin,
    IntMax,
    IntClamp,
    IntAbs,
    IntUnsignedAbs,
    IntAbsDiff,
    IntCountOnes,
    IntLeadingZeros,
    IntTrailingZeros,
    IntRotateLeft,
    IntRotateRight,
    IntSwapBytes,
    IntReverseBits,
    IntIsPowerOfTwo,
    IntCheckedAdd,
    IntCheckedSub,
    IntCheckedMul,
    IntCheckedDiv,
    IntCheckedRem,
    IntCheckedNeg,
    IntSaturatingAdd,
    IntSaturatingSub,
    IntSaturatingMul,
    IntWrappingPow,
    IntCheckedPow,
    IntWideningMul,
    /// `Owned.drop :: 'a -> unit` drops its argument now (B07).
    OwnedDrop,
    /// `Owned.function :: ('a -> 'b) -> Owned.Function<'a, 'b>` (B07).
    OwnedFunction,
    /// `Owned.call :: ref Owned.Function<'a, 'b> -> 'a -> 'b` (B07).
    OwnedCall,
    /// `not :: bool -> bool`; a full application is the `!` operator.
    Not,
    /// `ignore :: 'a -> unit` drops its argument, as in `do! action |> ignore`.
    Ignore,
    /// `Arena.__next_id :: i64` takes the next arena id from a process-wide atomic counter (C10).
    /// Only the std `Arena` module may call it.
    ArenaNextId,
    /// `Bench.now :: i64`, called as `Bench.now()`: nanoseconds of a monotonic clock from an
    /// arbitrary origin. Only the `tsuzuri bench` runner defines its clock (G18 D3).
    BenchNow,
    /// `Bench.consume :: 'a -> unit` drops its argument after an optimization barrier, so the
    /// value and the memory it reaches count as observed (G18 D6).
    BenchConsume,
    /// `Alloc.__counter :: i64 -> i64` reads a counting-heap counter (F14).
    /// Only the standard Alloc module may call it.
    AllocCounter,
    /// `Gen.__seed :: i64u`, called as `Gen.__seed()`: the seed of property tests, from
    /// `tsuzuri test --seed` or `llvm::DEFAULT_PROPERTY_SEED`. Only the std `Gen` module may
    /// call it (G18 Phase 3).
    GenSeed,
    /// `Gpu.__open :: i32 -> i32 -> i32` opens the process-wide device of a backend (the tag of
    /// `Gpu.Backend`) with the device features the program's kernels need, and returns a status:
    /// 0 for success. It is `tsuzuri_gpu_open` natively, the import `tsuzuri_gpu.open` under
    /// `--wasm-feature webgpu`, and the constant 1 (unavailable) otherwise. Only the std `Gpu`
    /// module may call it (F09 Phase 2).
    GpuOpen,
    /// `Gpu.__features :: i32`, called as `Gpu.__features()`: the device features that every
    /// kernel of the program needs (F09 Phase 2).
    GpuFeatures,
    /// `Gpu.__run :: i32 -> i64 -> i32 -> ref ['a] -> i64 -> ['b]` runs the kernel of the program's
    /// kernel table on a device, with the lanes of a borrowed input array (or none for `init`),
    /// and returns the new output array; a failing run traps (F09 Phase 2).
    GpuRun,
    /// `Gpu.__select :: i32 -> i64 -> i32 -> i64 -> i32` takes the tag of a `Gpu.Backend`, the
    /// kernel number, the mode (0 map, 1 init), and the lane count of a device call. It returns the
    /// tag of the backend that serves the call and records it for `Gpu.__last`: a named backend
    /// serves it, and `Gpu.Auto` picks the CPU reference (0) or a device by the runtime's cost
    /// rule (F09 Phase 3).
    GpuSelect,
    /// `Gpu.__last :: i32`, called as `Gpu.__last()`: the backend tag that `Gpu.__select` chose
    /// for the most recent device call (0 before any) (F09 Phase 3).
    GpuLast,
    /// `Rc.new :: 'a -> Rc<'a>` moves a value into a new reference-counted block (C10 Phase 2).
    RcNew,
    /// `Rc.share :: ref Rc<'a> -> Rc<'a>` adds a strong pointer to the same block.
    RcShare,
    /// `Rc.get :: ref Rc<'a> -> ref 'a` borrows the shared value.
    RcGet,
    /// `Rc.strong_count :: ref Rc<'a> -> i64`.
    RcStrongCount,
    /// `Rc.weak_count :: ref Rc<'a> -> i64`, the number of `Rc.Weak` pointers.
    RcWeakCount,
    /// `Rc.try_unwrap :: Rc<'a> -> Result<'a, Rc<'a>>` moves the value out of the last strong pointer.
    RcTryUnwrap,
    /// `Rc.downgrade :: ref Rc<'a> -> Rc.Weak<'a>`.
    RcDowngrade,
    /// `Rc.upgrade :: ref Rc.Weak<'a> -> Maybe<Rc<'a>>`, `None` once the value is dropped.
    RcUpgrade,
    /// `Rc.ptr_eq :: ref Rc<'a> -> ref Rc<'a> -> bool`: whether both point to one block.
    RcPtrEq,
    /// The `Arc` versions, which count with atomic operations.
    ArcNew,
    ArcShare,
    ArcGet,
    ArcStrongCount,
    ArcWeakCount,
    ArcTryUnwrap,
    ArcDowngrade,
    ArcUpgrade,
    ArcPtrEq,
    /// `Async.__resume :: Async.Next<'a, 'b> -> 'a -> 'b` calls a continuation once, moving
    /// its environment into the call instead of copying it (B08). Only the std `Async`
    /// module may call it.
    AsyncResume,
    /// `Async.__take :: Maybe<'a>`, called as `Async.__take()`: takes the host executor's state of
    /// type `'a` out of its per-thread slot and locks it until `Async.__put`; taking a locked
    /// slot traps on re-entry (B08 Phase 2). Only the std `Async` module may call it.
    AsyncTake,
    /// `Async.__put :: Maybe<'a> -> unit` stores the state of a slot that `Async.__take`
    /// locked, or leaves it empty, and unlocks it; putting into an unlocked slot traps.
    AsyncPut,
    /// `Async.__next_id :: i64`, called as `Async.__next_id()`: the next host operation id from a
    /// per-thread counter, tagged with the thread's never-reused index.
    AsyncNextId,
    /// `Async.__clock :: i64`, called as `Async.__clock()`: milliseconds of the monotonic clock
    /// of `Async.block_on` (B08 Phase 3): `src/runtime/async.c` natively, and the import
    /// `tsuzuri_async.clock` under `--wasm-feature jspi`.
    AsyncClock,
    /// `Async.__wait :: i64 -> unit` blocks until the clock reaches the deadline or a host
    /// operation completes: natively in `src/runtime/async.c`, and under `--wasm-feature jspi`
    /// through the import `tsuzuri_async.wait`, which suspends the WebAssembly stack.
    AsyncWait,
    /// `Async.__posted :: (i64 * i64)`, called as `Async.__posted()`: the earliest completion
    /// that another thread posted with `tsuzuri_async_post`, or `(0, 0)`. WASM has none.
    AsyncPosted,
    /// Removes a completed or cancelled operation from the native reactor's mailbox.
    AsyncRetire,
    /// `Task.scope :: (Sync<'s>, Send<'a>) => ref 's -> i64 -> (ref 's -> i64 -> 'a) -> ['a]`
    /// runs the callback once per index below a count, in parallel, lending every child the
    /// same shared borrow, and returns the results in index order (F10).
    TaskScope,
    /// `Atomic.create :: AtomicValue<'a> => 'a -> Atomic<'a>` (F10).
    AtomicCreate,
    /// `Atomic.load :: AtomicValue<'a> => ref Atomic<'a> -> 'a`.
    AtomicLoad,
    /// `Atomic.store :: AtomicValue<'a> => ref Atomic<'a> -> 'a -> unit`.
    AtomicStore,
    /// `Atomic.swap :: AtomicValue<'a> => ref Atomic<'a> -> 'a -> 'a`.
    AtomicSwap,
    /// `Atomic.compare_exchange cell expected desired`: `Ok previous` after writing `desired`,
    /// or `Error current` without writing.
    AtomicCompareExchange,
    /// `Atomic.fetch_add :: (AtomicValue<'a>, Integer<'a>) => ref Atomic<'a> -> 'a -> 'a`
    /// returns the value before it wrapped around; `fetch_sub`, `fetch_and`, `fetch_or` and
    /// `fetch_xor` likewise.
    AtomicFetchAdd,
    AtomicFetchSub,
    AtomicFetchAnd,
    AtomicFetchOr,
    AtomicFetchXor,
    /// `Atomic.into_inner :: Atomic<'a> -> 'a`.
    AtomicIntoInner,
    /// `Mutex.create :: Send<'a> => 'a -> Mutex<'a>`.
    MutexCreate,
    /// `Mutex.with_lock :: Send<'b> => ref Mutex<'a> -> (ref mut 'a -> 'b) -> 'b` runs the
    /// callback once while the calling thread holds the lock (`with` is a keyword, which cannot
    /// follow a dot, so the name is `with_lock` as `Bench.with_input` is).
    MutexWith,
    /// `Mutex.into_inner :: Mutex<'a> -> 'a`.
    MutexIntoInner,
    /// `Channel.bounded :: Send<'a> => i64 -> (Channel.Sender<'a> * Channel.Receiver<'a>)`
    /// makes a channel that holds up to the given number of items (F10 Phase 2).
    ChannelBounded,
    /// `Channel.send :: ref Channel.Sender<'a> -> 'a -> Result<unit, 'a>`: waits while the channel
    /// is full; `Error item` when no receiver is left.
    ChannelSend,
    /// `Channel.recv :: ref Channel.Receiver<'a> -> Maybe<'a>`: waits while the channel is empty
    /// and a sender is left; `None` when it is empty and closed.
    ChannelRecv,
    /// `Channel.clone_sender :: ref Channel.Sender<'a> -> Channel.Sender<'a>`.
    ChannelCloneSender,
    /// `Channel.__close_sender` and `__close_receiver`, the bodies of the `Drop` instances in
    /// `std/Channel.tz`: private to that module.
    ChannelCloseSender,
    ChannelCloseReceiver,
    /// Test-only `Int.test_add : Integer<'a> => 'a -> 'a -> 'a` exercises
    /// multi-argument, constrained builtins.
    #[cfg(test)]
    TestAdd,
    /// Test-only `Int.test_unsigned : Integer<'a> => 'a -> UnsignedOf<'a>`.
    #[cfg(test)]
    TestUnsigned,
    /// Test-only `Int.test_widen : Integer<'a> => 'a -> WidenOf<'a>`.
    #[cfg(test)]
    TestWiden,
}

/// A type in a builtin's scheme. Project-specific types are named through
/// `Std` and resolved against each program's standard library (GUIDE D-07).
#[derive(Clone, Debug)]
pub enum BuiltinType {
    Var(&'static str),
    /// A primitive type; never a project id.
    Concrete(Type),
    /// A std-origin record or union by module and name, such as `Maybe<'a>`.
    Std {
        module: &'static str,
        name: &'static str,
        args: Vec<BuiltinType>,
    },
    Array(Box<BuiltinType>),
    /// The `[T..]` target of an exclusive slice (C08); only inside `Reference(_, true)`.
    ArrayView(Box<BuiltinType>),
    List(Box<BuiltinType>),
    Vec(Box<BuiltinType>),
    Tuple(Vec<BuiltinType>),
    Task(Box<BuiltinType>),
    /// `Rc<T>` and the other shared pointers (C10).
    Shared(Box<BuiltinType>, SharedKind),
    Reference(Box<BuiltinType>, bool),
    Function(Vec<BuiltinType>, Box<BuiltinType>),
    /// The unsigned integer type of the same width.
    UnsignedOf(Box<BuiltinType>),
    /// The integer type of twice the width and the same signedness;
    /// undefined for 128-bit integers.
    WidenOf(Box<BuiltinType>),
    SimdLane(Box<BuiltinType>, Option<u16>),
    SimdMask(Box<BuiltinType>),
    /// A fixed-length array of the element, whose length the expected type decides (A16).
    FixedArrayOf(Box<BuiltinType>),
}

#[derive(Clone, Debug)]
pub struct BuiltinScheme {
    pub parameters: Vec<BuiltinType>,
    pub result: BuiltinType,
    pub variables: Vec<&'static str>,
    pub constraints: Vec<BuiltinConstraint>,
}

#[derive(Clone, Debug)]
pub struct BuiltinConstraint {
    pub class: &'static str,
    pub ty: BuiltinType,
}

/// A builtin at the concrete or inferred types of its scheme variables, in
/// `BuiltinScheme::variables` order.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct BuiltinInstance {
    pub builtin: Builtin,
    pub types: Vec<Type>,
}

impl Builtin {
    pub const ALL: &'static [Self] = &[
        Self::Sqrt,
        Self::Floor,
        Self::Ceil,
        Self::Abs,
        Self::MathSqrt,
        Self::MathFloor,
        Self::MathCeil,
        Self::MathTrunc,
        Self::MathRound,
        Self::MathRoundEven,
        Self::MathAbs,
        Self::MathMin,
        Self::MathMax,
        Self::MathClamp,
        Self::MathFma,
        Self::MathCopysign,
        Self::MathIsNan,
        Self::MathIsInfinite,
        Self::MathIsFinite,
        Self::MathPi,
        Self::MathE,
        Self::MathSin,
        Self::MathCos,
        Self::MathTan,
        Self::MathAsin,
        Self::MathAcos,
        Self::MathAtan,
        Self::MathAtan2,
        Self::MathExp,
        Self::MathExp2,
        Self::MathLog,
        Self::MathLog2,
        Self::MathLog10,
        Self::MathPow,
        Self::MathCbrt,
        Self::MathHypot,
        Self::ToFloat,
        Self::ToInt,
        Self::Assert,
        Self::CloneString,
        Self::CloneUtf8String,
        Self::StringFromUtf8,
        Self::Utf8StringFromString,
        Self::StringIsWellFormed,
        Self::StringToWellFormed,
        Self::TaskRun,
        Self::TaskParallel,
        Self::TaskParallelResults,
        Self::ParallelInit,
        Self::ParallelMap,
        Self::ParallelMapRef,
        Self::ParallelReduce,
        Self::ParallelForEachChunk,
        Self::Unreachable,
        Self::ToString,
        Self::DebugPrintString,
        Self::IOReadLine,
        Self::IOWrite,
        Self::OsRead,
        Self::OsArgs,
        Self::OsWrite,
        Self::OsRandom,
        Self::OsClock,
        Self::OsSleep,
        Self::OsOpen,
        Self::OsHandle,
        Self::OsClose,
        Self::OsSpawn,
        Self::NetResolve,
        Self::NetOpen,
        Self::NetAccept,
        Self::NetRead,
        Self::NetWrite,
        Self::NetClose,
        Self::NetClassify,
        Self::NetWatch,
        Self::NetUnwatch,
        Self::NetConnect,
        Self::NetNames,
        Self::NetSend,
        Self::UnicodeTableLength,
        Self::UnicodeTableEntry,
        Self::Display,
        Self::Parse,
        Self::Default,
        Self::Hash,
        Self::HashMix,
        Self::DisplayQuoted,
        Self::ArraySet,
        Self::ArrayUpdate,
        Self::ArraySwap,
        Self::ArrayWrite,
        Self::ArraySwapIn,
        Self::ArraySplitAtMut,
        Self::FixedArrayInit,
        Self::DynOf,
        Self::ListCons,
        Self::ListTail,
        Self::ArrayConcat,
        Self::ArrayToList,
        Self::ArraySortBy,
        Self::ListMap,
        Self::ListMapRef,
        Self::ListReverse,
        Self::ListToArray,
        Self::ListFoldRef,
        Self::VecEmpty,
        Self::SeqNext,
        Self::SimdSplat,
        Self::SimdOfLanes2,
        Self::SimdOfLanes4,
        Self::SimdOfLanes8,
        Self::SimdOfLanes16,
        Self::SimdOfLanes32,
        Self::SimdExtract,
        Self::SimdReplace,
        Self::SimdLoad,
        Self::SimdStore,
        Self::SimdSum,
        Self::SimdEq,
        Self::SimdNe,
        Self::SimdLt,
        Self::SimdLe,
        Self::SimdGt,
        Self::SimdGe,
        Self::SimdSelect,
        Self::SimdAll,
        Self::SimdAny,
        Self::VecWithCapacity,
        Self::VecLength,
        Self::VecCapacity,
        Self::VecIsEmpty,
        Self::VecPush,
        Self::VecPushMut,
        Self::VecPop,
        Self::VecReserve,
        Self::VecTruncate,
        Self::VecClear,
        Self::VecSet,
        Self::VecSwap,
        Self::VecGet,
        Self::VecAt,
        Self::VecClone,
        Self::VecOfArray,
        Self::VecToArray,
        Self::VecView,
        Self::VecSetCapacity,
        Self::VecInsert,
        Self::VecSplice,
        Self::CharToU16,
        Self::CharOfU16,
        Self::Utf8CharToU32,
        Self::Utf8CharOfU32,
        Self::Utf8CharOfU32Unchecked,
        Self::StringToCodeUnits,
        Self::StringFromCodeUnits,
        Self::Utf8StringToBytes,
        Self::Utf8StringFromBytes,
        Self::Utf8StringDecodeAt,
        Self::StringCompare,
        Self::Utf8StringCompare,
        Self::IntMin,
        Self::IntMax,
        Self::IntClamp,
        Self::IntAbs,
        Self::IntUnsignedAbs,
        Self::IntAbsDiff,
        Self::IntCountOnes,
        Self::IntLeadingZeros,
        Self::IntTrailingZeros,
        Self::IntRotateLeft,
        Self::IntRotateRight,
        Self::IntSwapBytes,
        Self::IntReverseBits,
        Self::IntIsPowerOfTwo,
        Self::IntCheckedAdd,
        Self::IntCheckedSub,
        Self::IntCheckedMul,
        Self::IntCheckedDiv,
        Self::IntCheckedRem,
        Self::IntCheckedNeg,
        Self::IntSaturatingAdd,
        Self::IntSaturatingSub,
        Self::IntSaturatingMul,
        Self::IntWrappingPow,
        Self::IntCheckedPow,
        Self::IntWideningMul,
        Self::OwnedDrop,
        Self::OwnedFunction,
        Self::OwnedCall,
        Self::Not,
        Self::Ignore,
        Self::ArenaNextId,
        Self::BenchNow,
        Self::BenchConsume,
        Self::AllocCounter,
        Self::GenSeed,
        Self::GpuOpen,
        Self::GpuFeatures,
        Self::GpuRun,
        Self::GpuSelect,
        Self::GpuLast,
        Self::RcNew,
        Self::RcShare,
        Self::RcGet,
        Self::RcStrongCount,
        Self::RcWeakCount,
        Self::RcTryUnwrap,
        Self::RcDowngrade,
        Self::RcUpgrade,
        Self::RcPtrEq,
        Self::ArcNew,
        Self::ArcShare,
        Self::ArcGet,
        Self::ArcStrongCount,
        Self::ArcWeakCount,
        Self::ArcTryUnwrap,
        Self::ArcDowngrade,
        Self::ArcUpgrade,
        Self::ArcPtrEq,
        Self::AsyncResume,
        Self::AsyncTake,
        Self::AsyncPut,
        Self::AsyncNextId,
        Self::AsyncClock,
        Self::AsyncWait,
        Self::AsyncPosted,
        Self::AsyncRetire,
        Self::TaskScope,
        Self::AtomicCreate,
        Self::AtomicLoad,
        Self::AtomicStore,
        Self::AtomicSwap,
        Self::AtomicCompareExchange,
        Self::AtomicFetchAdd,
        Self::AtomicFetchSub,
        Self::AtomicFetchAnd,
        Self::AtomicFetchOr,
        Self::AtomicFetchXor,
        Self::AtomicIntoInner,
        Self::MutexCreate,
        Self::MutexWith,
        Self::MutexIntoInner,
        Self::ChannelBounded,
        Self::ChannelSend,
        Self::ChannelRecv,
        Self::ChannelCloneSender,
        Self::ChannelCloseSender,
        Self::ChannelCloseReceiver,
        #[cfg(test)]
        Self::TestAdd,
        #[cfg(test)]
        Self::TestUnsigned,
        #[cfg(test)]
        Self::TestWiden,
    ];

    /// The name used in source: unqualified for the original builtins, and
    /// `Module.name` for namespace functions.
    pub fn name(self) -> &'static str {
        match self {
            Self::Sqrt => "sqrt",
            Self::Floor => "floor",
            Self::Ceil => "ceil",
            Self::Abs => "abs",
            Self::MathSqrt => "Math.sqrt",
            Self::MathFloor => "Math.floor",
            Self::MathCeil => "Math.ceil",
            Self::MathTrunc => "Math.trunc",
            Self::MathRound => "Math.round",
            Self::MathRoundEven => "Math.round_even",
            Self::MathAbs => "Math.abs",
            Self::MathMin => "Math.min",
            Self::MathMax => "Math.max",
            Self::MathClamp => "Math.clamp",
            Self::MathFma => "Math.fma",
            Self::MathCopysign => "Math.copysign",
            Self::MathIsNan => "Math.is_nan",
            Self::MathIsInfinite => "Math.is_infinite",
            Self::MathIsFinite => "Math.is_finite",
            Self::MathPi => "Math.pi",
            Self::MathE => "Math.e",
            Self::MathSin => "Math.sin",
            Self::MathCos => "Math.cos",
            Self::MathTan => "Math.tan",
            Self::MathAsin => "Math.asin",
            Self::MathAcos => "Math.acos",
            Self::MathAtan => "Math.atan",
            Self::MathAtan2 => "Math.atan2",
            Self::MathExp => "Math.exp",
            Self::MathExp2 => "Math.exp2",
            Self::MathLog => "Math.log",
            Self::MathLog2 => "Math.log2",
            Self::MathLog10 => "Math.log10",
            Self::MathPow => "Math.pow",
            Self::MathCbrt => "Math.cbrt",
            Self::MathHypot => "Math.hypot",
            Self::ToFloat => "to_float",
            Self::ToInt => "to_int",
            Self::Assert => "assert",
            Self::CloneString => "clone_string",
            Self::CloneUtf8String => "Utf8String.clone",
            Self::StringFromUtf8 => "String.from_utf8",
            Self::Utf8StringFromString => "Utf8String.from_string",
            Self::StringIsWellFormed => "String.is_well_formed",
            Self::StringToWellFormed => "String.to_well_formed",
            Self::TaskRun => "Task.run",
            Self::TaskParallel => "Task.parallel",
            Self::TaskParallelResults => "Task.parallel_results",
            Self::ParallelInit => "Parallel.init",
            Self::ParallelMap => "Parallel.map",
            Self::ParallelMapRef => "Parallel.map_ref",
            Self::ParallelReduce => "Parallel.reduce",
            Self::ParallelForEachChunk => "Parallel.for_each_chunk",
            Self::Unreachable => "unreachable",
            Self::ToString => "to_string",
            Self::DebugPrintString => "Debug.__print_string",
            Self::IOReadLine => "IO.__read_line",
            Self::IOWrite => "IO.__write",
            Self::OsRead => "Os.__read",
            Self::OsArgs => "Os.__args",
            Self::OsWrite => "Os.__write",
            Self::OsRandom => "Os.__random",
            Self::OsClock => "Os.__clock",
            Self::OsSleep => "Os.__sleep",
            Self::OsOpen => "Os.__open",
            Self::OsHandle => "Os.__handle",
            Self::OsClose => "Os.__close",
            Self::OsSpawn => "Os.__spawn",
            Self::NetResolve => "Net.__resolve",
            Self::NetOpen => "Net.__open",
            Self::NetAccept => "Net.__accept",
            Self::NetRead => "Net.__read",
            Self::NetWrite => "Net.__write",
            Self::NetClose => "Net.__close",
            Self::NetClassify => "Net.__classify",
            Self::NetWatch => "Net.__watch",
            Self::NetUnwatch => "Net.__unwatch",
            Self::NetConnect => "Net.__connect",
            Self::NetNames => "Net.__names",
            Self::NetSend => "Net.__send",
            Self::UnicodeTableLength => "Unicode.__table_length",
            Self::UnicodeTableEntry => "Unicode.__table_entry",
            Self::Display => "$builtin.display",
            Self::Parse => "$builtin.parse",
            Self::Default => "$builtin.default",
            Self::Hash => "$builtin.hash",
            Self::HashMix => "$builtin.hash_mix",
            Self::DisplayQuoted => "$builtin.display_quoted",
            Self::ArraySet => "Array.__set",
            Self::ArrayUpdate => "Array.update",
            Self::ArraySwap => "Array.swap",
            Self::ArrayWrite => "Array.write",
            Self::ArraySwapIn => "Array.swap_in",
            Self::ArraySplitAtMut => "Array.split_at_mut",
            Self::FixedArrayInit => "FixedArray.init",
            Self::DynOf => "Dyn.of",
            Self::ListCons => "List.cons",
            Self::ListTail => "List.tail",
            Self::ArrayConcat => "Array.concat",
            Self::ArrayToList => "Array.to_list",
            Self::ArraySortBy => "Array.__sort_with",
            Self::ListMap => "List.map",
            Self::ListMapRef => "List.mapr",
            Self::ListReverse => "List.rev",
            Self::ListToArray => "List.to_array",
            Self::ListFoldRef => "List.foldr",
            Self::VecEmpty => "Vec.empty",
            Self::SeqNext => "Seq.next",
            Self::SimdSplat => "Simd.splat",
            Self::SimdOfLanes2 => "Simd.of_lanes2",
            Self::SimdOfLanes4 => "Simd.of_lanes4",
            Self::SimdOfLanes8 => "Simd.of_lanes8",
            Self::SimdOfLanes16 => "Simd.of_lanes16",
            Self::SimdOfLanes32 => "Simd.of_lanes32",
            Self::SimdExtract => "Simd.extract",
            Self::SimdReplace => "Simd.replace",
            Self::SimdLoad => "Simd.load",
            Self::SimdStore => "Simd.store",
            Self::SimdSum => "Simd.sum_lanes",
            Self::SimdEq => "Simd.eq",
            Self::SimdNe => "Simd.ne",
            Self::SimdLt => "Simd.lt",
            Self::SimdLe => "Simd.le",
            Self::SimdGt => "Simd.gt",
            Self::SimdGe => "Simd.ge",
            Self::SimdSelect => "Simd.select",
            Self::SimdAll => "Simd.all",
            Self::SimdAny => "Simd.any",
            Self::VecWithCapacity => "Vec.with_capacity",
            Self::VecLength => "Vec.length",
            Self::VecCapacity => "Vec.capacity",
            Self::VecIsEmpty => "Vec.is_empty",
            Self::VecPush => "Vec.push",
            Self::VecPushMut => "Vec.push_mut",
            Self::VecPop => "Vec.pop",
            Self::VecReserve => "Vec.reserve",
            Self::VecTruncate => "Vec.truncate",
            Self::VecClear => "Vec.clear",
            Self::VecSet => "Vec.set",
            Self::VecSwap => "Vec.swap",
            Self::VecGet => "Vec.get",
            Self::VecAt => "Vec.at",
            Self::VecClone => "Vec.clone",
            Self::VecOfArray => "Vec.of_array",
            Self::VecToArray => "Vec.to_array",
            Self::VecView => "Vec.__view",
            Self::VecSetCapacity => "Vec.__set_capacity",
            Self::VecInsert => "Vec.__insert",
            Self::VecSplice => "Vec.__splice",
            Self::CharToU16 => "Char.to_u16",
            Self::CharOfU16 => "Char.of_u16",
            Self::Utf8CharToU32 => "Utf8Char.to_u32",
            Self::Utf8CharOfU32 => "Utf8Char.of_u32",
            Self::Utf8CharOfU32Unchecked => "Utf8Char.of_u32_unchecked",
            Self::StringToCodeUnits => "String.to_code_units",
            Self::StringFromCodeUnits => "String.from_code_units",
            Self::Utf8StringToBytes => "Utf8String.to_bytes",
            Self::Utf8StringFromBytes => "Utf8String.from_bytes",
            Self::Utf8StringDecodeAt => "Utf8String.decode_at",
            Self::StringCompare => "String.compare",
            Self::Utf8StringCompare => "Utf8String.compare",
            Self::IntMin => "Int.min",
            Self::IntMax => "Int.max",
            Self::IntClamp => "Int.clamp",
            Self::IntAbs => "Int.abs",
            Self::IntUnsignedAbs => "Int.unsigned_abs",
            Self::IntAbsDiff => "Int.abs_diff",
            Self::IntCountOnes => "Int.count_ones",
            Self::IntLeadingZeros => "Int.leading_zeros",
            Self::IntTrailingZeros => "Int.trailing_zeros",
            Self::IntRotateLeft => "Int.rotate_left",
            Self::IntRotateRight => "Int.rotate_right",
            Self::IntSwapBytes => "Int.swap_bytes",
            Self::IntReverseBits => "Int.reverse_bits",
            Self::IntIsPowerOfTwo => "Int.is_power_of_two",
            Self::IntCheckedAdd => "Int.checked_add",
            Self::IntCheckedSub => "Int.checked_sub",
            Self::IntCheckedMul => "Int.checked_mul",
            Self::IntCheckedDiv => "Int.checked_div",
            Self::IntCheckedRem => "Int.checked_rem",
            Self::IntCheckedNeg => "Int.checked_neg",
            Self::IntSaturatingAdd => "Int.saturating_add",
            Self::IntSaturatingSub => "Int.saturating_sub",
            Self::IntSaturatingMul => "Int.saturating_mul",
            Self::IntWrappingPow => "Int.wrapping_pow",
            Self::IntCheckedPow => "Int.checked_pow",
            Self::IntWideningMul => "Int.widening_mul",
            Self::OwnedDrop => "Owned.drop",
            Self::OwnedFunction => "Owned.function",
            Self::OwnedCall => "Owned.call",
            Self::Not => "not",
            Self::Ignore => "ignore",
            Self::ArenaNextId => "Arena.__next_id",
            Self::AsyncResume => "Async.__resume",
            Self::AsyncTake => "Async.__take",
            Self::AsyncPut => "Async.__put",
            Self::AsyncNextId => "Async.__next_id",
            Self::AsyncClock => "Async.__clock",
            Self::AsyncWait => "Async.__wait",
            Self::AsyncPosted => "Async.__posted",
            Self::AsyncRetire => "Async.__retire",
            Self::TaskScope => "Task.scope",
            Self::AtomicCreate => "Atomic.create",
            Self::AtomicLoad => "Atomic.load",
            Self::AtomicStore => "Atomic.store",
            Self::AtomicSwap => "Atomic.swap",
            Self::AtomicCompareExchange => "Atomic.compare_exchange",
            Self::AtomicFetchAdd => "Atomic.fetch_add",
            Self::AtomicFetchSub => "Atomic.fetch_sub",
            Self::AtomicFetchAnd => "Atomic.fetch_and",
            Self::AtomicFetchOr => "Atomic.fetch_or",
            Self::AtomicFetchXor => "Atomic.fetch_xor",
            Self::AtomicIntoInner => "Atomic.into_inner",
            Self::MutexCreate => "Mutex.create",
            Self::MutexWith => "Mutex.with_lock",
            Self::MutexIntoInner => "Mutex.into_inner",
            Self::ChannelBounded => "Channel.bounded",
            Self::ChannelSend => "Channel.send",
            Self::ChannelRecv => "Channel.recv",
            Self::ChannelCloneSender => "Channel.clone_sender",
            Self::ChannelCloseSender => "Channel.__close_sender",
            Self::ChannelCloseReceiver => "Channel.__close_receiver",
            Self::BenchNow => "Bench.now",
            Self::BenchConsume => "Bench.consume",
            Self::AllocCounter => "Alloc.__counter",
            Self::GenSeed => "Gen.__seed",
            Self::GpuOpen => "Gpu.__open",
            Self::GpuFeatures => "Gpu.__features",
            Self::GpuRun => "Gpu.__run",
            Self::GpuSelect => "Gpu.__select",
            Self::GpuLast => "Gpu.__last",
            Self::RcNew => "Rc.new",
            Self::RcShare => "Rc.share",
            Self::RcGet => "Rc.get",
            Self::RcStrongCount => "Rc.strong_count",
            Self::RcWeakCount => "Rc.weak_count",
            Self::RcTryUnwrap => "Rc.try_unwrap",
            Self::RcDowngrade => "Rc.downgrade",
            Self::RcUpgrade => "Rc.upgrade",
            Self::RcPtrEq => "Rc.ptr_eq",
            Self::ArcNew => "Arc.new",
            Self::ArcShare => "Arc.share",
            Self::ArcGet => "Arc.get",
            Self::ArcStrongCount => "Arc.strong_count",
            Self::ArcWeakCount => "Arc.weak_count",
            Self::ArcTryUnwrap => "Arc.try_unwrap",
            Self::ArcDowngrade => "Arc.downgrade",
            Self::ArcUpgrade => "Arc.upgrade",
            Self::ArcPtrEq => "Arc.ptr_eq",
            #[cfg(test)]
            Self::TestAdd => "Int.test_add",
            #[cfg(test)]
            Self::TestUnsigned => "Int.test_unsigned",
            #[cfg(test)]
            Self::TestWiden => "Int.test_widen",
        }
    }

    pub fn scheme(self) -> BuiltinScheme {
        use BuiltinType::{Array, Concrete, Reference, Task, Var};
        let a = || Var("a");
        #[cfg(test)]
        let integer = || BuiltinConstraint {
            class: "Integer",
            ty: a(),
        };
        let (parameters, result, constraints) = match self {
            Self::IOReadLine => (
                vec![Concrete(Type::Unit)],
                BuiltinType::Tuple(vec![
                    Concrete(Type::Integer(32, true)),
                    Array(Box::new(Concrete(Type::Integer(8, false)))),
                ]),
                Vec::new(),
            ),
            Self::IOWrite => (
                vec![
                    Concrete(Type::Integer(32, true)),
                    Reference(Box::new(Concrete(Type::Utf8String)), false),
                ],
                Concrete(Type::Integer(32, true)),
                Vec::new(),
            ),
            Self::OsRead | Self::OsArgs | Self::OsRandom => {
                let owned = || {
                    BuiltinType::Tuple(vec![
                        Concrete(Type::I64),
                        Array(Box::new(Concrete(Type::Integer(8, false)))),
                    ])
                };
                let parameters = match self {
                    Self::OsRead => vec![
                        Concrete(Type::Integer(32, true)),
                        Reference(Box::new(Concrete(Type::Utf8String)), false),
                    ],
                    Self::OsArgs => vec![Concrete(Type::Unit)],
                    _ => vec![Concrete(Type::I64)],
                };
                (parameters, owned(), Vec::new())
            }
            Self::OsWrite => (
                vec![
                    Concrete(Type::Integer(32, true)),
                    Reference(Box::new(Concrete(Type::Utf8String)), false),
                    Reference(
                        Box::new(Array(Box::new(Concrete(Type::Integer(8, false))))),
                        false,
                    ),
                ],
                Concrete(Type::I64),
                Vec::new(),
            ),
            Self::OsClock => (
                vec![Concrete(Type::Integer(32, true))],
                Concrete(Type::I64),
                Vec::new(),
            ),
            Self::OsSleep | Self::OsClose => {
                (vec![Concrete(Type::I64)], Concrete(Type::I64), Vec::new())
            }
            Self::OsOpen => (
                vec![
                    Concrete(Type::Integer(32, true)),
                    Reference(Box::new(Concrete(Type::Utf8String)), false),
                ],
                Concrete(Type::I64),
                Vec::new(),
            ),
            Self::OsSpawn => (
                vec![
                    Reference(Box::new(Concrete(Type::Utf8String)), false),
                    Reference(
                        Box::new(Array(Box::new(Concrete(Type::Integer(8, false))))),
                        false,
                    ),
                    Reference(
                        Box::new(Array(Box::new(Concrete(Type::Integer(8, false))))),
                        false,
                    ),
                ],
                BuiltinType::Tuple(vec![
                    Concrete(Type::I64),
                    Array(Box::new(Concrete(Type::Integer(8, false)))),
                ]),
                Vec::new(),
            ),
            Self::OsHandle => (
                vec![
                    Concrete(Type::Integer(32, true)),
                    Concrete(Type::I64),
                    Concrete(Type::I64),
                    Reference(
                        Box::new(Array(Box::new(Concrete(Type::Integer(8, false))))),
                        false,
                    ),
                ],
                BuiltinType::Tuple(vec![
                    Concrete(Type::I64),
                    Array(Box::new(Concrete(Type::Integer(8, false)))),
                ]),
                Vec::new(),
            ),
            Self::NetResolve | Self::NetOpen | Self::NetAccept | Self::NetRead | Self::NetNames => {
                let integers = |count: usize| vec![Concrete(Type::I64); count];
                let parameters = match self {
                    Self::NetResolve => vec![
                        Reference(Box::new(Concrete(Type::Utf8String)), false),
                        Concrete(Type::I64),
                    ],
                    Self::NetOpen => {
                        [vec![Concrete(Type::Integer(32, true))], integers(4)].concat()
                    }
                    Self::NetAccept => integers(2),
                    Self::NetNames => integers(1),
                    _ => [vec![Concrete(Type::Integer(32, true))], integers(3)].concat(),
                };
                (
                    parameters,
                    BuiltinType::Tuple(vec![
                        Concrete(Type::I64),
                        Array(Box::new(Concrete(Type::Integer(8, false)))),
                    ]),
                    Vec::new(),
                )
            }
            Self::NetWrite | Self::NetSend => (
                vec![
                    Concrete(Type::Integer(32, true)),
                    Concrete(Type::I64),
                    Reference(
                        Box::new(Array(Box::new(Concrete(Type::Integer(8, false))))),
                        false,
                    ),
                    Concrete(Type::I64),
                    Concrete(Type::I64),
                    Concrete(Type::I64),
                    Concrete(Type::I64),
                ],
                Concrete(Type::I64),
                Vec::new(),
            ),
            Self::NetWatch => (
                vec![
                    Concrete(Type::I64),
                    Concrete(Type::I64),
                    Concrete(Type::Integer(32, true)),
                    Concrete(Type::I64),
                ],
                Concrete(Type::Unit),
                Vec::new(),
            ),
            Self::NetUnwatch => (vec![Concrete(Type::I64)], Concrete(Type::Unit), Vec::new()),
            Self::NetConnect => (
                vec![Concrete(Type::I64); 5],
                Concrete(Type::Unit),
                Vec::new(),
            ),
            Self::NetClose => (
                vec![Concrete(Type::Integer(32, true)), Concrete(Type::I64)],
                Concrete(Type::I64),
                Vec::new(),
            ),
            Self::NetClassify => (
                vec![Concrete(Type::Integer(32, true))],
                Concrete(Type::I64),
                Vec::new(),
            ),
            Self::SimdSplat
            | Self::SimdOfLanes2
            | Self::SimdOfLanes4
            | Self::SimdOfLanes8
            | Self::SimdOfLanes16
            | Self::SimdOfLanes32
            | Self::SimdExtract
            | Self::SimdReplace
            | Self::SimdLoad
            | Self::SimdStore
            | Self::SimdSum
            | Self::SimdEq
            | Self::SimdNe
            | Self::SimdLt
            | Self::SimdLe
            | Self::SimdGt
            | Self::SimdGe
            | Self::SimdSelect
            | Self::SimdAll
            | Self::SimdAny => {
                let count = match self {
                    Self::SimdOfLanes2 => Some(2),
                    Self::SimdOfLanes4 => Some(4),
                    Self::SimdOfLanes8 => Some(8),
                    Self::SimdOfLanes16 => Some(16),
                    Self::SimdOfLanes32 => Some(32),
                    _ => None,
                };
                let lane = || BuiltinType::SimdLane(Box::new(a()), count);
                let mask = || BuiltinType::SimdMask(Box::new(a()));
                let parameters = match self {
                    Self::SimdSplat => vec![lane()],
                    Self::SimdExtract => vec![a(), Concrete(Type::I64)],
                    Self::SimdReplace => vec![a(), Concrete(Type::I64), lane()],
                    Self::SimdLoad => vec![
                        Reference(Box::new(Array(Box::new(lane()))), false),
                        Concrete(Type::I64),
                    ],
                    Self::SimdStore => vec![
                        Reference(Box::new(BuiltinType::ArrayView(Box::new(lane()))), true),
                        Concrete(Type::I64),
                        a(),
                    ],
                    Self::SimdSum | Self::SimdAll | Self::SimdAny => vec![a()],
                    Self::SimdSelect => vec![mask(), a(), a()],
                    _ if count.is_some() => vec![lane(); usize::from(count.unwrap())],
                    _ => vec![a(), a()],
                };
                let result = match self {
                    Self::SimdExtract | Self::SimdSum => lane(),
                    Self::SimdAll | Self::SimdAny => Concrete(Type::Bool),
                    Self::SimdStore => Concrete(Type::Unit),
                    Self::SimdEq
                    | Self::SimdNe
                    | Self::SimdLt
                    | Self::SimdLe
                    | Self::SimdGt
                    | Self::SimdGe => mask(),
                    _ => a(),
                };
                let class = match self {
                    Self::SimdAll | Self::SimdAny => "SimdMask",
                    Self::SimdLoad
                    | Self::SimdStore
                    | Self::SimdSum
                    | Self::SimdEq
                    | Self::SimdNe
                    | Self::SimdLt
                    | Self::SimdLe
                    | Self::SimdGt
                    | Self::SimdGe => "SimdNumeric",
                    _ => "SimdVector",
                };
                (
                    parameters,
                    result,
                    vec![BuiltinConstraint { class, ty: a() }],
                )
            }
            Self::StringToCodeUnits => (
                vec![Concrete(Type::String)],
                Array(Box::new(Concrete(Type::Integer(16, false)))),
                Vec::new(),
            ),
            Self::StringFromCodeUnits => (
                vec![Array(Box::new(Concrete(Type::Integer(16, false))))],
                Concrete(Type::String),
                Vec::new(),
            ),
            Self::Utf8StringToBytes => (
                vec![Concrete(Type::Utf8String)],
                Array(Box::new(Concrete(Type::Integer(8, false)))),
                Vec::new(),
            ),
            Self::Utf8StringFromBytes => (
                vec![Array(Box::new(Concrete(Type::Integer(8, false))))],
                BuiltinType::Std {
                    module: "Maybe",
                    name: "Maybe",
                    args: vec![Concrete(Type::Utf8String)],
                },
                Vec::new(),
            ),
            Self::Utf8StringDecodeAt => (
                vec![
                    Concrete(Type::I64),
                    Reference(Box::new(Concrete(Type::Utf8String)), false),
                ],
                BuiltinType::Tuple(vec![Concrete(Type::Utf8Char), Concrete(Type::I64)]),
                Vec::new(),
            ),
            Self::StringCompare | Self::Utf8StringCompare => {
                let ty = if self == Self::StringCompare {
                    Type::String
                } else {
                    Type::Utf8String
                };
                (
                    vec![
                        Reference(Box::new(Concrete(ty.clone())), false),
                        Reference(Box::new(Concrete(ty)), false),
                    ],
                    Concrete(Type::I64),
                    Vec::new(),
                )
            }
            Self::CharToU16 => (
                vec![Concrete(Type::Char)],
                Concrete(Type::Integer(16, false)),
                Vec::new(),
            ),
            Self::CharOfU16 => (
                vec![Concrete(Type::Integer(16, false))],
                Concrete(Type::Char),
                Vec::new(),
            ),
            Self::Utf8CharToU32 => (
                vec![Concrete(Type::Utf8Char)],
                Concrete(Type::Integer(32, false)),
                Vec::new(),
            ),
            Self::Utf8CharOfU32Unchecked => (
                vec![Concrete(Type::Integer(32, false))],
                Concrete(Type::Utf8Char),
                Vec::new(),
            ),
            Self::Utf8CharOfU32 => (
                vec![Concrete(Type::Integer(32, false))],
                BuiltinType::Std {
                    module: "Maybe",
                    name: "Maybe",
                    args: vec![Concrete(Type::Utf8Char)],
                },
                Vec::new(),
            ),
            Self::SeqNext => {
                let sequence = BuiltinType::Std {
                    module: "Seq",
                    name: "Seq",
                    args: vec![a()],
                };
                let option = BuiltinType::Std {
                    module: "Maybe",
                    name: "Maybe",
                    args: vec![a()],
                };
                (
                    vec![sequence.clone()],
                    BuiltinType::Tuple(vec![sequence, option]),
                    Vec::new(),
                )
            }
            Self::VecEmpty
            | Self::VecWithCapacity
            | Self::VecLength
            | Self::VecCapacity
            | Self::VecIsEmpty
            | Self::VecPush
            | Self::VecPushMut
            | Self::VecPop
            | Self::VecReserve
            | Self::VecTruncate
            | Self::VecClear
            | Self::VecSet
            | Self::VecSwap
            | Self::VecGet
            | Self::VecAt
            | Self::VecClone
            | Self::VecOfArray
            | Self::VecToArray
            | Self::VecView
            | Self::VecSetCapacity
            | Self::VecInsert
            | Self::VecSplice => {
                let vector = || BuiltinType::Vec(Box::new(a()));
                let borrowed = || Reference(Box::new(vector()), false);
                let option = || BuiltinType::Std {
                    module: "Maybe",
                    name: "Maybe",
                    args: vec![a()],
                };
                let parameters = match self {
                    Self::VecEmpty => vec![],
                    Self::VecWithCapacity => vec![Concrete(Type::I64)],
                    Self::VecLength | Self::VecCapacity | Self::VecIsEmpty | Self::VecClone => {
                        vec![borrowed()]
                    }
                    Self::VecGet | Self::VecAt => vec![Concrete(Type::I64), borrowed()],
                    Self::VecPush => vec![a(), vector()],
                    Self::VecPushMut => vec![a(), Reference(Box::new(vector()), true)],
                    Self::VecReserve | Self::VecTruncate | Self::VecSetCapacity => {
                        vec![Concrete(Type::I64), vector()]
                    }
                    Self::VecSet | Self::VecInsert => {
                        vec![Concrete(Type::I64), a(), vector()]
                    }
                    Self::VecSwap => vec![Concrete(Type::I64), Concrete(Type::I64), vector()],
                    Self::VecSplice => vec![
                        Concrete(Type::I64),
                        Concrete(Type::I64),
                        Array(Box::new(a())),
                        vector(),
                    ],
                    Self::VecView => vec![Reference(Box::new(vector()), true)],
                    Self::VecOfArray => vec![Array(Box::new(a()))],
                    _ => vec![vector()],
                };
                let result = match self {
                    Self::VecLength | Self::VecCapacity => Concrete(Type::I64),
                    Self::VecIsEmpty => Concrete(Type::Bool),
                    Self::VecPushMut => Concrete(Type::Unit),
                    Self::VecPop => BuiltinType::Tuple(vec![vector(), option()]),
                    Self::VecGet => option(),
                    Self::VecAt => Reference(Box::new(a()), false),
                    Self::VecToArray => Array(Box::new(a())),
                    Self::VecView => {
                        Reference(Box::new(BuiltinType::ArrayView(Box::new(a()))), true)
                    }
                    _ => vector(),
                };
                let constraints = if matches!(self, Self::VecGet | Self::VecClone) {
                    vec![BuiltinConstraint {
                        class: "Copy",
                        ty: a(),
                    }]
                } else {
                    Vec::new()
                };
                (parameters, result, constraints)
            }
            Self::ArrayConcat
            | Self::ArrayToList
            | Self::ArraySortBy
            | Self::ListMap
            | Self::ListMapRef
            | Self::ListReverse
            | Self::ListToArray
            | Self::ListFoldRef => {
                let list = || BuiltinType::List(Box::new(a()));
                let array = || Array(Box::new(a()));
                let borrowed = || Reference(Box::new(a()), false);
                let read_list = || Reference(Box::new(list()), false);
                let read_array = || Reference(Box::new(array()), false);
                let copy = vec![BuiltinConstraint {
                    class: "Copy",
                    ty: a(),
                }];
                match self {
                    Self::ArrayConcat => (
                        vec![Reference(Box::new(Array(Box::new(array()))), false)],
                        array(),
                        copy,
                    ),
                    Self::ArrayToList => (vec![read_array()], list(), copy),
                    Self::ArraySortBy => (
                        vec![
                            BuiltinType::Function(
                                vec![borrowed(), borrowed()],
                                Box::new(Concrete(Type::I64)),
                            ),
                            read_array(),
                        ],
                        array(),
                        copy,
                    ),
                    Self::ListMap | Self::ListMapRef => {
                        let input = if self == Self::ListMap {
                            a()
                        } else {
                            borrowed()
                        };
                        (
                            vec![
                                BuiltinType::Function(vec![input], Box::new(Var("b"))),
                                read_list(),
                            ],
                            BuiltinType::List(Box::new(Var("b"))),
                            if self == Self::ListMap {
                                copy
                            } else {
                                Vec::new()
                            },
                        )
                    }
                    Self::ListReverse => (vec![read_list()], list(), copy),
                    Self::ListToArray => (vec![read_list()], array(), copy),
                    Self::ListFoldRef => (
                        vec![
                            BuiltinType::Function(
                                vec![Var("state"), borrowed()],
                                Box::new(Var("state")),
                            ),
                            Var("state"),
                            read_list(),
                        ],
                        Var("state"),
                        Vec::new(),
                    ),
                    _ => unreachable!(),
                }
            }
            Self::ArraySet | Self::ArrayUpdate | Self::ArraySwap => {
                let second = match self {
                    Self::ArraySet => a(),
                    Self::ArrayUpdate => BuiltinType::Function(vec![a()], Box::new(a())),
                    _ => Concrete(Type::I64),
                };
                // The array comes last, so that a pipeline or a composition can pass it on.
                (
                    vec![Concrete(Type::I64), second, Array(Box::new(a()))],
                    Array(Box::new(a())),
                    Vec::new(),
                )
            }
            Self::ArrayWrite | Self::ArraySwapIn | Self::ArraySplitAtMut => {
                let view = || Reference(Box::new(BuiltinType::ArrayView(Box::new(a()))), true);
                match self {
                    Self::ArrayWrite => (
                        vec![Concrete(Type::I64), a(), view()],
                        Concrete(Type::Unit),
                        Vec::new(),
                    ),
                    Self::ArraySwapIn => (
                        vec![Concrete(Type::I64), Concrete(Type::I64), view()],
                        Concrete(Type::Unit),
                        Vec::new(),
                    ),
                    _ => (
                        vec![Concrete(Type::I64), view()],
                        BuiltinType::Tuple(vec![view(), view()]),
                        Vec::new(),
                    ),
                }
            }
            Self::FixedArrayInit => (
                vec![BuiltinType::Function(
                    vec![Concrete(Type::I64)],
                    Box::new(a()),
                )],
                BuiltinType::FixedArrayOf(Box::new(a())),
                Vec::new(),
            ),
            // The classes of the expected `dyn` type become constraints in `Checker::dyn_of`.
            Self::DynOf => (vec![a()], Var("b"), Vec::new()),
            Self::ListCons => (
                vec![a(), BuiltinType::List(Box::new(a()))],
                BuiltinType::List(Box::new(a())),
                Vec::new(),
            ),
            Self::ListTail => (
                vec![BuiltinType::List(Box::new(a()))],
                BuiltinType::List(Box::new(a())),
                Vec::new(),
            ),
            Self::IntMin
            | Self::IntMax
            | Self::IntClamp
            | Self::IntAbs
            | Self::IntUnsignedAbs
            | Self::IntAbsDiff
            | Self::IntCountOnes
            | Self::IntLeadingZeros
            | Self::IntTrailingZeros
            | Self::IntRotateLeft
            | Self::IntRotateRight
            | Self::IntSwapBytes
            | Self::IntReverseBits
            | Self::IntIsPowerOfTwo
            | Self::IntCheckedAdd
            | Self::IntCheckedSub
            | Self::IntCheckedMul
            | Self::IntCheckedDiv
            | Self::IntCheckedRem
            | Self::IntCheckedNeg
            | Self::IntSaturatingAdd
            | Self::IntSaturatingSub
            | Self::IntSaturatingMul
            | Self::IntWrappingPow
            | Self::IntCheckedPow
            | Self::IntWideningMul => {
                let parameters = match self {
                    Self::IntAbs
                    | Self::IntUnsignedAbs
                    | Self::IntCountOnes
                    | Self::IntLeadingZeros
                    | Self::IntTrailingZeros
                    | Self::IntSwapBytes
                    | Self::IntReverseBits
                    | Self::IntIsPowerOfTwo
                    | Self::IntCheckedNeg => vec![a()],
                    Self::IntRotateLeft
                    | Self::IntRotateRight
                    | Self::IntWrappingPow
                    | Self::IntCheckedPow => vec![a(), Concrete(Type::I64)],
                    Self::IntClamp => vec![a(), a(), a()],
                    _ => vec![a(), a()],
                };
                let result = match self {
                    Self::IntCountOnes | Self::IntLeadingZeros | Self::IntTrailingZeros => {
                        Concrete(Type::I64)
                    }
                    Self::IntIsPowerOfTwo => Concrete(Type::Bool),
                    Self::IntUnsignedAbs | Self::IntAbsDiff => {
                        BuiltinType::UnsignedOf(Box::new(a()))
                    }
                    Self::IntWideningMul => BuiltinType::WidenOf(Box::new(a())),
                    Self::IntCheckedAdd
                    | Self::IntCheckedSub
                    | Self::IntCheckedMul
                    | Self::IntCheckedDiv
                    | Self::IntCheckedRem
                    | Self::IntCheckedNeg
                    | Self::IntCheckedPow => BuiltinType::Std {
                        module: "Maybe",
                        name: "Maybe",
                        args: vec![a()],
                    },
                    _ => a(),
                };
                let class = match self {
                    Self::IntAbs | Self::IntUnsignedAbs | Self::IntCheckedNeg => "SignedInteger",
                    Self::IntIsPowerOfTwo => "UnsignedInteger",
                    _ => "Integer",
                };
                (
                    parameters,
                    result,
                    vec![BuiltinConstraint { class, ty: a() }],
                )
            }
            Self::MathSqrt
            | Self::MathFloor
            | Self::MathCeil
            | Self::MathTrunc
            | Self::MathRound
            | Self::MathRoundEven
            | Self::MathAbs
            | Self::MathMin
            | Self::MathMax
            | Self::MathClamp
            | Self::MathFma
            | Self::MathCopysign
            | Self::MathIsNan
            | Self::MathIsInfinite
            | Self::MathIsFinite
            | Self::MathPi
            | Self::MathE => {
                let count = match self {
                    Self::MathPi | Self::MathE => 0,
                    Self::MathMin | Self::MathMax | Self::MathCopysign => 2,
                    Self::MathClamp | Self::MathFma => 3,
                    _ => 1,
                };
                let result = if matches!(
                    self,
                    Self::MathIsNan | Self::MathIsInfinite | Self::MathIsFinite
                ) {
                    Concrete(Type::Bool)
                } else {
                    a()
                };
                (
                    vec![a(); count],
                    result,
                    vec![BuiltinConstraint {
                        class: "Float",
                        ty: a(),
                    }],
                )
            }
            Self::MathSin
            | Self::MathCos
            | Self::MathTan
            | Self::MathAsin
            | Self::MathAcos
            | Self::MathAtan
            | Self::MathAtan2
            | Self::MathExp
            | Self::MathExp2
            | Self::MathLog
            | Self::MathLog2
            | Self::MathLog10
            | Self::MathPow
            | Self::MathCbrt
            | Self::MathHypot => {
                let count = if matches!(self, Self::MathAtan2 | Self::MathPow | Self::MathHypot) {
                    2
                } else {
                    1
                };
                (
                    vec![a(); count],
                    a(),
                    vec![BuiltinConstraint {
                        class: "Elementary",
                        ty: a(),
                    }],
                )
            }
            Self::Sqrt | Self::Floor | Self::Ceil | Self::Abs => {
                (vec![Concrete(Type::F64)], Concrete(Type::F64), Vec::new())
            }
            Self::UnicodeTableLength => {
                (vec![Concrete(Type::I64)], Concrete(Type::I64), Vec::new())
            }
            Self::UnicodeTableEntry => (
                vec![Concrete(Type::I64), Concrete(Type::I64)],
                Concrete(Type::I64),
                Vec::new(),
            ),
            Self::ToFloat => (vec![Concrete(Type::I64)], Concrete(Type::F64), Vec::new()),
            Self::ToInt => (vec![Concrete(Type::F64)], Concrete(Type::I64), Vec::new()),
            Self::Assert => (vec![Concrete(Type::Bool)], Concrete(Type::Unit), Vec::new()),
            Self::DebugPrintString => (
                vec![Concrete(Type::String)],
                Concrete(Type::Unit),
                Vec::new(),
            ),
            Self::CloneString | Self::StringToWellFormed => (
                vec![Reference(Box::new(Concrete(Type::String)), false)],
                Concrete(Type::String),
                Vec::new(),
            ),
            Self::CloneUtf8String => (
                vec![Reference(Box::new(Concrete(Type::Utf8String)), false)],
                Concrete(Type::Utf8String),
                Vec::new(),
            ),
            Self::StringFromUtf8 => (
                vec![Reference(Box::new(Concrete(Type::Utf8String)), false)],
                Concrete(Type::String),
                Vec::new(),
            ),
            Self::Utf8StringFromString => (
                vec![Reference(Box::new(Concrete(Type::String)), false)],
                Concrete(Type::Utf8String),
                Vec::new(),
            ),
            Self::StringIsWellFormed => (
                vec![Reference(Box::new(Concrete(Type::String)), false)],
                Concrete(Type::Bool),
                Vec::new(),
            ),
            Self::TaskRun => (vec![Task(Box::new(a()))], a(), Vec::new()),
            Self::TaskParallel => (
                vec![Array(Box::new(Task(Box::new(a()))))],
                Task(Box::new(Array(Box::new(a())))),
                Vec::new(),
            ),
            Self::TaskParallelResults => {
                let result = |value| BuiltinType::Std {
                    module: "Result",
                    name: "Result",
                    args: vec![value, Var("e")],
                };
                (
                    vec![Array(Box::new(Task(Box::new(result(a())))))],
                    Task(Box::new(result(Array(Box::new(a()))))),
                    Vec::new(),
                )
            }
            Self::ParallelInit => (
                vec![
                    Concrete(Type::I64),
                    BuiltinType::Function(vec![Concrete(Type::I64)], Box::new(a())),
                ],
                Array(Box::new(a())),
                vec![BuiltinConstraint {
                    class: "Send",
                    ty: a(),
                }],
            ),
            Self::ParallelMap | Self::ParallelMapRef => {
                let input = if self == Self::ParallelMapRef {
                    Reference(Box::new(a()), false)
                } else {
                    a()
                };
                let mut constraints = vec![
                    BuiltinConstraint {
                        class: "Send",
                        ty: a(),
                    },
                    BuiltinConstraint {
                        class: "Send",
                        ty: BuiltinType::Var("b"),
                    },
                ];
                if self == Self::ParallelMap {
                    constraints.push(BuiltinConstraint {
                        class: "Copy",
                        ty: a(),
                    });
                }
                (
                    vec![
                        BuiltinType::Function(vec![input], Box::new(BuiltinType::Var("b"))),
                        Reference(Box::new(Array(Box::new(a()))), false),
                    ],
                    Array(Box::new(BuiltinType::Var("b"))),
                    constraints,
                )
            }
            Self::ParallelReduce => (
                vec![
                    a(),
                    BuiltinType::Function(vec![a(), a()], Box::new(a())),
                    Reference(Box::new(Array(Box::new(a()))), false),
                ],
                a(),
                vec![
                    BuiltinConstraint {
                        class: "Copy",
                        ty: a(),
                    },
                    BuiltinConstraint {
                        class: "Send",
                        ty: a(),
                    },
                ],
            ),
            Self::ParallelForEachChunk => {
                let view = || Reference(Box::new(BuiltinType::ArrayView(Box::new(a()))), true);
                (
                    vec![
                        Concrete(Type::I64),
                        BuiltinType::Function(
                            vec![Concrete(Type::I64), view()],
                            Box::new(Concrete(Type::Unit)),
                        ),
                        view(),
                    ],
                    Concrete(Type::Unit),
                    vec![BuiltinConstraint {
                        class: "Send",
                        ty: a(),
                    }],
                )
            }
            Self::Unreachable => (vec![Concrete(Type::Unit)], a(), Vec::new()),
            Self::ToString => (
                vec![a()],
                Concrete(Type::String),
                vec![BuiltinConstraint {
                    class: "Display",
                    ty: a(),
                }],
            ),
            Self::Display => (
                vec![Reference(Box::new(a()), false)],
                Concrete(Type::String),
                Vec::new(),
            ),
            Self::Default => (Vec::new(), a(), Vec::new()),
            Self::Hash => (
                vec![Reference(Box::new(a()), false)],
                Concrete(Type::Integer(64, false)),
                Vec::new(),
            ),
            Self::HashMix => (
                vec![Concrete(Type::Integer(64, false)); 2],
                Concrete(Type::Integer(64, false)),
                Vec::new(),
            ),
            Self::DisplayQuoted => (
                vec![Reference(Box::new(a()), false)],
                Concrete(Type::String),
                vec![BuiltinConstraint {
                    class: "Display",
                    ty: a(),
                }],
            ),
            Self::Parse => (
                vec![Reference(Box::new(Concrete(Type::String)), false)],
                BuiltinType::Std {
                    module: "Maybe",
                    name: "Maybe",
                    args: vec![a()],
                },
                Vec::new(),
            ),
            Self::OwnedDrop | Self::Ignore | Self::BenchConsume => {
                (vec![a()], Concrete(Type::Unit), Vec::new())
            }
            Self::ArenaNextId | Self::BenchNow | Self::AsyncNextId | Self::AsyncClock => {
                (Vec::new(), Concrete(Type::I64), Vec::new())
            }
            Self::AsyncWait | Self::AsyncRetire => {
                (vec![Concrete(Type::I64)], Concrete(Type::Unit), Vec::new())
            }
            Self::AsyncPosted => (
                Vec::new(),
                Concrete(Type::Tuple(vec![Type::I64, Type::I64])),
                Vec::new(),
            ),
            Self::AsyncTake | Self::AsyncPut => {
                let state = BuiltinType::Std {
                    module: "Maybe",
                    name: "Maybe",
                    args: vec![a()],
                };
                if self == Self::AsyncTake {
                    (Vec::new(), state, Vec::new())
                } else {
                    (vec![state], Concrete(Type::Unit), Vec::new())
                }
            }
            Self::AllocCounter => (vec![Concrete(Type::I64)], Concrete(Type::I64), Vec::new()),
            Self::GenSeed => (Vec::new(), Concrete(Type::Integer(64, false)), Vec::new()),
            Self::GpuOpen => (
                vec![Concrete(Type::Integer(32, true)); 2],
                Concrete(Type::Integer(32, true)),
                Vec::new(),
            ),
            Self::GpuFeatures => (Vec::new(), Concrete(Type::Integer(32, true)), Vec::new()),
            Self::GpuRun => (
                vec![
                    Concrete(Type::Integer(32, true)),
                    Concrete(Type::I64),
                    Concrete(Type::Integer(32, true)),
                    Reference(Box::new(Array(Box::new(a()))), false),
                    Concrete(Type::I64),
                ],
                Array(Box::new(Var("b"))),
                Vec::new(),
            ),
            Self::GpuSelect => (
                vec![
                    Concrete(Type::Integer(32, true)),
                    Concrete(Type::I64),
                    Concrete(Type::Integer(32, true)),
                    Concrete(Type::I64),
                ],
                Concrete(Type::Integer(32, true)),
                Vec::new(),
            ),
            Self::GpuLast => (Vec::new(), Concrete(Type::Integer(32, true)), Vec::new()),
            Self::RcNew
            | Self::RcShare
            | Self::RcGet
            | Self::RcStrongCount
            | Self::RcWeakCount
            | Self::RcTryUnwrap
            | Self::RcDowngrade
            | Self::RcUpgrade
            | Self::RcPtrEq
            | Self::ArcNew
            | Self::ArcShare
            | Self::ArcGet
            | Self::ArcStrongCount
            | Self::ArcWeakCount
            | Self::ArcTryUnwrap
            | Self::ArcDowngrade
            | Self::ArcUpgrade
            | Self::ArcPtrEq => {
                let kind = self.shared_kind().expect("a shared pointer builtin");
                let strong = || BuiltinType::Shared(Box::new(a()), kind);
                let borrowed = || Reference(Box::new(strong()), false);
                match self.shared_operation() {
                    SharedOperation::New => (vec![a()], strong(), Vec::new()),
                    SharedOperation::Share => (vec![borrowed()], strong(), Vec::new()),
                    SharedOperation::Get => (
                        vec![borrowed()],
                        Reference(Box::new(a()), false),
                        Vec::new(),
                    ),
                    SharedOperation::StrongCount | SharedOperation::WeakCount => {
                        (vec![borrowed()], Concrete(Type::I64), Vec::new())
                    }
                    SharedOperation::TryUnwrap => (
                        vec![strong()],
                        BuiltinType::Std {
                            module: "Result",
                            name: "Result",
                            args: vec![a(), strong()],
                        },
                        Vec::new(),
                    ),
                    SharedOperation::Downgrade => (
                        vec![borrowed()],
                        BuiltinType::Shared(Box::new(a()), kind.weakened()),
                        Vec::new(),
                    ),
                    SharedOperation::Upgrade => (
                        vec![Reference(
                            Box::new(BuiltinType::Shared(Box::new(a()), kind.weakened())),
                            false,
                        )],
                        BuiltinType::Std {
                            module: "Maybe",
                            name: "Maybe",
                            args: vec![strong()],
                        },
                        Vec::new(),
                    ),
                    SharedOperation::PtrEq => (
                        vec![borrowed(), borrowed()],
                        Concrete(Type::Bool),
                        Vec::new(),
                    ),
                }
            }
            Self::Not => (vec![Concrete(Type::Bool)], Concrete(Type::Bool), Vec::new()),
            Self::AsyncResume => (
                vec![
                    BuiltinType::Std {
                        module: "Async",
                        name: "Next",
                        args: vec![a(), Var("b")],
                    },
                    a(),
                ],
                Var("b"),
                Vec::new(),
            ),
            Self::OwnedFunction | Self::OwnedCall => {
                let run = BuiltinType::Function(vec![a()], Box::new(Var("b")));
                let owned = BuiltinType::Std {
                    module: "Owned",
                    name: "Function",
                    args: vec![a(), Var("b")],
                };
                if self == Self::OwnedFunction {
                    (vec![run], owned, Vec::new())
                } else {
                    (
                        vec![Reference(Box::new(owned), false), a()],
                        Var("b"),
                        Vec::new(),
                    )
                }
            }
            Self::TaskScope
            | Self::AtomicCreate
            | Self::AtomicLoad
            | Self::AtomicStore
            | Self::AtomicSwap
            | Self::AtomicCompareExchange
            | Self::AtomicFetchAdd
            | Self::AtomicFetchSub
            | Self::AtomicFetchAnd
            | Self::AtomicFetchOr
            | Self::AtomicFetchXor
            | Self::AtomicIntoInner
            | Self::MutexCreate
            | Self::MutexWith
            | Self::MutexIntoInner
            | Self::ChannelBounded
            | Self::ChannelSend
            | Self::ChannelRecv
            | Self::ChannelCloneSender
            | Self::ChannelCloseSender
            | Self::ChannelCloseReceiver => self.sync_scheme(),
            #[cfg(test)]
            Self::TestAdd => (vec![a(), a()], a(), vec![integer()]),
            #[cfg(test)]
            Self::TestUnsigned => (
                vec![a()],
                BuiltinType::UnsignedOf(Box::new(a())),
                vec![integer()],
            ),
            #[cfg(test)]
            Self::TestWiden => (
                vec![a()],
                BuiltinType::WidenOf(Box::new(a())),
                vec![integer()],
            ),
        };
        let mut variables = Vec::new();
        for ty in parameters.iter().chain([&result]) {
            ty.variables(&mut variables);
        }
        BuiltinScheme {
            parameters,
            result,
            variables,
            constraints,
        }
    }
}

impl BuiltinType {
    /// Appends the scheme variables in order of first appearance.
    fn variables(&self, found: &mut Vec<&'static str>) {
        match self {
            Self::Var(name) => {
                if !found.contains(name) {
                    found.push(name);
                }
            }
            Self::Concrete(_) => {}
            Self::Std { args: types, .. } | Self::Tuple(types) => {
                types.iter().for_each(|ty| ty.variables(found))
            }
            Self::Array(ty)
            | Self::ArrayView(ty)
            | Self::List(ty)
            | Self::Vec(ty)
            | Self::Task(ty)
            | Self::Shared(ty, _)
            | Self::Reference(ty, _)
            | Self::UnsignedOf(ty)
            | Self::WidenOf(ty)
            | Self::SimdLane(ty, _)
            | Self::SimdMask(ty)
            | Self::FixedArrayOf(ty) => ty.variables(found),
            Self::Function(parameters, result) => {
                parameters.iter().for_each(|ty| ty.variables(found));
                result.variables(found);
            }
        }
    }
}

/// What a shared pointer builtin does, the same for `Rc` and `Arc` (C10 Phase 2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SharedOperation {
    New,
    Share,
    Get,
    StrongCount,
    WeakCount,
    TryUnwrap,
    Downgrade,
    Upgrade,
    PtrEq,
}

impl Builtin {
    /// The strong pointer kind of an `Rc` or `Arc` builtin.
    pub(crate) fn shared_kind(self) -> Option<SharedKind> {
        if self.name().starts_with("Rc.") {
            Some(SharedKind::Rc)
        } else if self.name().starts_with("Arc.") {
            Some(SharedKind::Arc)
        } else {
            None
        }
    }

    pub(crate) fn shared_operation(self) -> SharedOperation {
        match self {
            Self::RcNew | Self::ArcNew => SharedOperation::New,
            Self::RcShare | Self::ArcShare => SharedOperation::Share,
            Self::RcGet | Self::ArcGet => SharedOperation::Get,
            Self::RcStrongCount | Self::ArcStrongCount => SharedOperation::StrongCount,
            Self::RcWeakCount | Self::ArcWeakCount => SharedOperation::WeakCount,
            Self::RcTryUnwrap | Self::ArcTryUnwrap => SharedOperation::TryUnwrap,
            Self::RcDowngrade | Self::ArcDowngrade => SharedOperation::Downgrade,
            Self::RcUpgrade | Self::ArcUpgrade => SharedOperation::Upgrade,
            Self::RcPtrEq | Self::ArcPtrEq => SharedOperation::PtrEq,
            _ => unreachable!("not a shared pointer builtin"),
        }
    }

    pub(crate) fn is_parallel(self) -> bool {
        matches!(
            self,
            Self::ParallelInit
                | Self::ParallelMap
                | Self::ParallelMapRef
                | Self::ParallelReduce
                | Self::ParallelForEachChunk
                | Self::TaskScope
        )
    }

    /// The argument of a parallel operation that holds the callback.
    pub(crate) fn parallel_callback(self) -> usize {
        match self {
            Self::TaskScope => 2,
            Self::ParallelInit | Self::ParallelReduce | Self::ParallelForEachChunk => 1,
            _ => 0,
        }
    }

    /// Whether this builtin is an `Atomic` operation, which the LLVM lowering emits inline as
    /// one atomic instruction (F10).
    pub(crate) fn is_atomic(self) -> bool {
        self.name().starts_with("Atomic.")
    }

    /// Whether this builtin is a `Channel` operation, lowered to calls of the channel runtime
    /// (F10 Phase 2).
    pub(crate) fn is_channel(self) -> bool {
        self.name().starts_with("Channel.")
    }

    /// The schemes of `Task.scope`, `Atomic.*`, and `Mutex.*` (F10), kept out of `scheme` so
    /// that its frame stays small.
    #[inline(never)]
    fn sync_scheme(self) -> (Vec<BuiltinType>, BuiltinType, Vec<BuiltinConstraint>) {
        use BuiltinType::{Array, Concrete, Reference, Var};
        let a = || Var("a");
        let constraint = |class, ty| BuiltinConstraint { class, ty };
        let atomic = || BuiltinType::Std {
            module: "Atomic",
            name: "Atomic",
            args: vec![a()],
        };
        let mutex = || BuiltinType::Std {
            module: "Mutex",
            name: "Mutex",
            args: vec![a()],
        };
        let sender = || BuiltinType::Std {
            module: "Channel",
            name: "Sender",
            args: vec![a()],
        };
        let receiver = || BuiltinType::Std {
            module: "Channel",
            name: "Receiver",
            args: vec![a()],
        };
        let atomic_value = || vec![constraint("AtomicValue", a())];
        let borrowed = |ty| Reference(Box::new(ty), false);
        match self {
            Self::TaskScope => {
                let shared = || Reference(Box::new(Var("s")), false);
                (
                    vec![
                        shared(),
                        Concrete(Type::I64),
                        BuiltinType::Function(vec![shared(), Concrete(Type::I64)], Box::new(a())),
                    ],
                    Array(Box::new(a())),
                    vec![constraint("Sync", Var("s")), constraint("Send", a())],
                )
            }
            Self::AtomicCreate => (vec![a()], atomic(), atomic_value()),
            Self::AtomicLoad => (vec![borrowed(atomic())], a(), atomic_value()),
            Self::AtomicStore => (
                vec![borrowed(atomic()), a()],
                Concrete(Type::Unit),
                atomic_value(),
            ),
            Self::AtomicSwap => (vec![borrowed(atomic()), a()], a(), atomic_value()),
            Self::AtomicCompareExchange => (
                vec![borrowed(atomic()), a(), a()],
                BuiltinType::Std {
                    module: "Result",
                    name: "Result",
                    args: vec![a(), a()],
                },
                atomic_value(),
            ),
            Self::AtomicFetchAdd
            | Self::AtomicFetchSub
            | Self::AtomicFetchAnd
            | Self::AtomicFetchOr
            | Self::AtomicFetchXor => {
                let mut constraints = atomic_value();
                constraints.push(constraint("Integer", a()));
                (vec![borrowed(atomic()), a()], a(), constraints)
            }
            Self::AtomicIntoInner => (vec![atomic()], a(), atomic_value()),
            Self::MutexCreate => (vec![a()], mutex(), vec![constraint("Send", a())]),
            Self::MutexWith => (
                vec![
                    borrowed(mutex()),
                    BuiltinType::Function(vec![Reference(Box::new(a()), true)], Box::new(Var("b"))),
                ],
                Var("b"),
                vec![constraint("Send", Var("b"))],
            ),
            Self::MutexIntoInner => (vec![mutex()], a(), Vec::new()),
            Self::ChannelBounded => (
                vec![Concrete(Type::I64)],
                BuiltinType::Tuple(vec![sender(), receiver()]),
                vec![constraint("Send", a())],
            ),
            Self::ChannelSend => (
                vec![borrowed(sender()), a()],
                BuiltinType::Std {
                    module: "Result",
                    name: "Result",
                    args: vec![Concrete(Type::Unit), a()],
                },
                Vec::new(),
            ),
            Self::ChannelRecv => (
                vec![borrowed(receiver())],
                BuiltinType::Std {
                    module: "Maybe",
                    name: "Maybe",
                    args: vec![a()],
                },
                Vec::new(),
            ),
            Self::ChannelCloneSender => (vec![borrowed(sender())], sender(), Vec::new()),
            Self::ChannelCloseSender => {
                (vec![borrowed(sender())], Concrete(Type::Unit), Vec::new())
            }
            Self::ChannelCloseReceiver => {
                (vec![borrowed(receiver())], Concrete(Type::Unit), Vec::new())
            }
            _ => unreachable!("not a concurrency builtin"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FunctionRef {
    User(usize),
    Builtin(BuiltinInstance),
}

#[derive(Clone, Debug)]
pub struct Signature {
    pub parameters: Vec<Type>,
    pub result: Type,
}

impl Signature {
    pub(crate) fn as_type(&self) -> Type {
        Type::function(self.parameters.clone(), self.result.clone())
    }
}

/// Whether a module comes from the user's project or the standard library.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModuleOrigin {
    User,
    Std,
}

/// Where a function comes from (GUIDE D-22). Generated helpers inherit the
/// module origin and test of the function that created them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FunctionOrigin {
    pub module: ModuleOrigin,
    pub provenance: Provenance,
    pub parent: Option<usize>,
    pub test: Option<usize>,
    /// The `bench` declaration that the function belongs to (G18).
    pub bench: Option<usize>,
    /// The CPU levels that `@cpu` also compiles the function for, bit `L` for level `L` of
    /// `syntax::CPU_TARGETS` (F08 Phase 3). Generated helpers have none.
    pub cpu: u8,
    pub pure: bool,
    pub pure_values: bool,
}

impl FunctionOrigin {
    /// A function written in a module of the given origin.
    pub(crate) fn source(module: ModuleOrigin) -> Self {
        Self {
            module,
            provenance: Provenance::User,
            parent: None,
            test: None,
            bench: None,
            cpu: 0,
            pure: false,
            pure_values: false,
        }
    }

    /// The origin of a helper that the function `parent` with this origin generates.
    pub(crate) fn generated(self, parent: usize) -> Self {
        Self {
            provenance: Provenance::Generated,
            parent: Some(parent),
            cpu: 0,
            pure: false,
            ..self
        }
    }
}

/// A parsed module and its origin, as `check_modules` receives it.
#[derive(Clone, Copy)]
pub struct ModuleInput<'a> {
    /// The module key that qualifies its declarations inside the compiler.
    pub name: &'a str,
    pub program: &'a Program,
    pub origin: ModuleOrigin,
    /// The namespace that, with the file stem, forms the module's full name; empty for std.
    pub namespace: &'a str,
    /// The root package's `Main.tz`, the only module that may hold the entry point.
    pub entry: bool,
    /// The edition that the module's source was read with; its name is an identifier of it.
    pub edition: crate::syntax::Edition,
    /// The dependency package that owns the module, `None` for the root package, for sources
    /// without a manifest, and for std (W1005).
    pub package: Option<&'a crate::package::PackageId>,
}

#[derive(Debug)]
pub struct CheckedModule {
    pub records: Vec<CheckedRecord>,
    pub unions: Vec<CheckedUnion>,
    pub functions: Vec<CheckedFunction>,
    pub entry: Option<usize>,
    pub tests: Vec<CheckedTest>,
    /// The `bench` declarations, whose functions take an iteration count and return nanoseconds.
    pub benches: Vec<CheckedBench>,
    /// Warnings in source traversal order; they never fail a check or build.
    pub warnings: Vec<Diagnostic>,
    /// Concrete Drop type -> specialized `Drop.drop` function id. Empty without Drop instances.
    pub user_drops: BTreeMap<Type, usize>,
    /// The `user_drops` functions of types that only bench code holds; only the bench runner
    /// emits them (G18).
    pub bench_drops: BTreeSet<usize>,
    /// A14: the slot functions of each vtable in slot order, by vtable key
    /// (`DynType::vtable_key`) and stored type. Specialization fills it.
    pub vtables: Vtables,
    /// A14: the layout of each vtable key that a vtable or an upcast uses.
    pub dyn_layouts: BTreeMap<DynType, DynLayout>,
    /// A14: some module writes a `dyn` type, so the IR defines `%tz.dyn`.
    pub uses_dyn: bool,
    /// F09 Phase 2: the kernels that the program embeds for non-CPU backends. Empty unless the
    /// program builds a non-CPU `Gpu.Backend`.
    pub gpu: crate::gpu_devices::GpuProgram,
    /// D11 Phase 2: the plain constants that the program reads in place or borrows, by the id
    /// of the constant's declaration. `TypedExprKind::Static(id)` names one.
    pub statics: BTreeMap<usize, StaticData>,
}

/// The value of a constant that the program keeps once, as read-only static data (D11 Phase 2).
#[derive(Clone, Debug)]
pub struct StaticData {
    /// The literal tree that the constant evaluates to; its type is the constant's.
    pub value: TypedExpr,
}

/// The slot functions of each vtable in slot order, by vtable key and stored type (A14).
pub type Vtables = BTreeMap<(DynType, Type), Vec<usize>>;

/// The shape of the vtables of one vtable key (A14).
#[derive(Clone, Debug, Default)]
pub struct DynLayout {
    /// The number of method slots.
    pub slots: usize,
    /// The vtable keys that values of this key are upcast to without being stored again, in
    /// the order of the vtable's upcast table (Phase 2).
    pub upcasts: Vec<DynType>,
}

#[derive(Clone, Debug)]
pub struct CheckedTest {
    /// The module as source code writes it, as `Geometry::Point`.
    pub module: String,
    pub name: String,
    pub index: usize,
    pub function: usize,
    pub span: Span,
}

/// A `bench` declaration (G18). Its function `$bench.<index>` has type `i64 -> i64`.
#[derive(Clone, Debug)]
pub struct CheckedBench {
    /// The module as source code writes it, as `Geometry::Point`.
    pub module: String,
    pub name: String,
    pub index: usize,
    pub function: usize,
    pub span: Span,
}

impl CheckedModule {
    pub fn types(&self) -> TypeContext<'_> {
        TypeContext {
            records: &self.records,
            unions: &self.unions,
        }
    }
}

#[derive(Clone, Debug)]
pub struct CheckedRecord {
    pub name: String,
    pub visibility: Visibility,
    pub origin: ModuleOrigin,
    pub parameters: Vec<String>,
    pub fields: Vec<(String, Type)>,
    pub span: Span,
    /// Every instance of the record has a user `Drop` instance (B07).
    pub user_drop: bool,
    /// Declared region count. 0 and 1 both mean one shared region (A09).
    pub(crate) region_count: usize,
    /// Only when `region_count >= 2`: per field in declaration order, the record-region mask of
    /// each region slot of the field type. A single entry applies to every slot of the field type.
    pub(crate) field_regions: Vec<Vec<RegionMask>>,
    /// A13: the regions that hold an exclusive borrow, a direct `ref mut {r} T` field or a nested
    /// record's exclusive region. Bit 0 stands for the only region of a record with one.
    pub(crate) exclusive_regions: RegionMask,
    /// A13 Phase 2, only when `region_count >= 2`: per field written `ref {r} T {s}`, the regions of
    /// the borrows inside its target, and 0 for the other fields.
    pub(crate) field_targets: Vec<RegionMask>,
    /// A13 Phase 2, only when `region_count >= 2`: per region, the regions of the borrows inside
    /// the targets of the references in that region, including those of nested records.
    pub(crate) region_targets: Vec<RegionMask>,
    /// Value layout size of a non-generic record; generic instances are
    /// measured per concrete type.
    size: Option<usize>,
    recursive: recursive::Cache,
}

impl CheckedRecord {
    pub(crate) fn opaque(&self) -> bool {
        self.origin == ModuleOrigin::Std && crate::stdlib::opaque_record(&self.name)
    }
}

/// A union declaration. Cases keep declaration order, and a case's index is its tag. The tag is
/// stored in the fewest of 1, 2 and 4 bytes that hold every index, and read as an `i32`.
#[derive(Clone, Debug)]
pub struct CheckedUnion {
    pub name: String,
    pub visibility: Visibility,
    pub origin: ModuleOrigin,
    pub parameters: Vec<String>,
    pub cases: Vec<(String, Option<Type>)>,
    pub span: Span,
    /// Every instance of the union has a user `Drop` instance (B07).
    pub user_drop: bool,
    /// Value layout size of a non-generic union, like `CheckedRecord::size`.
    size: Option<usize>,
    recursive: recursive::Cache,
}

impl CheckedUnion {
    /// The module that declares the union.
    pub fn module(&self) -> &str {
        self.name.rsplit_once('.').map_or("", |(module, _)| module)
    }
}

fn read_by_default(ty: &Type, types: &TypeContext<'_>) -> bool {
    !matches!(
        ty,
        Type::Reference(..) | Type::Infer(_) | Type::Task(_) | Type::Dyn(_) | Type::Simd(_)
    ) && (ty.needs_drop(types)
        || types.reaches(ty, |part| {
            matches!(part, Type::Variable(_) | Type::Function(..))
        }))
}

pub(crate) fn has_explicit_parameter_regions(ty: &TypeExpr) -> bool {
    regions::has_regions(ty)
}

pub(crate) fn read_only_hint(error: &mut Diagnostic, name: &str, ty: &str, pure: bool) {
    let context = if pure { "pure def" } else { "edition 2027" };
    error.message.push_str(&format!(
        "; parameter '{name}' is read-only by default ({context}); write 'own {ty}' in the signature to take ownership"
    ));
}

#[derive(Clone, Debug)]
pub struct CheckedFunction {
    pub module: String,
    pub(crate) read_by_default: Vec<bool>,
    pub(crate) region_sources: Option<RegionSources>,
    /// The parameters with region-quantified function types; such a function is only called directly.
    pub(crate) callback_contracts: Vec<CallbackContract>,
    /// A13 Phase 2: the declared regions of the parameters' slots of a function with named regions.
    pub(crate) region_slots: Option<RegionSlots>,
    pub origin: FunctionOrigin,
    pub name: String,
    pub visibility: Visibility,
    pub exported: bool,
    pub parameters: Vec<Local>,
    pub signature: Signature,
    pub body: TypedExpr,
    pub span: Span,
    type_parameters: Vec<String>,
    constraints: Vec<Constraint>,
    members: Vec<polymorph::MemberConstraint>,
    pub(crate) capture_count: usize,
    pub(crate) is_task: bool,
    /// A lambda passed directly to `Owned.function` (B07); its body borrows the captures.
    pub(crate) owned_captures: bool,
}

impl CheckedFunction {
    pub fn qualified_name(&self) -> String {
        format!("{}.{}", self.module, self.name)
    }

    /// Whether the calls of the function depend only on its declared signature: it has no type
    /// parameters, whose inferred constraints and ownership requirements the signature does not
    /// show, and no named-region contracts.
    pub(crate) fn plain_signature(&self) -> bool {
        self.type_parameters.is_empty()
            && self.region_sources.is_none()
            && self.region_slots.is_none()
            && self.callback_contracts.is_empty()
    }
}

#[derive(Clone, Debug)]
pub struct Local {
    pub id: usize,
    pub ty: Type,
    pub name: String,
    pub mutable: bool,
    pub span: Span,
    pub provenance: Provenance,
}

#[derive(Clone, Debug)]
pub struct TypedExpr {
    pub kind: TypedExprKind,
    pub ty: Type,
    pub span: Span,
}

/// `$"a{x}b"` after type checking: the texts and one entry per hole.
#[derive(Clone, Debug)]
pub struct TypedInterpolation {
    pub texts: Vec<StringLiteral>,
    pub holes: Vec<TypedHole>,
}

#[derive(Clone, Debug)]
pub struct TypedHole {
    /// A `ref U` value: a borrowed place, a reference, or a `BorrowOperand` of a temporary.
    pub operand: TypedExpr,
    /// `Display.display` for `U`, or `Format.format` when `custom`. `None` for a
    /// string of the literal's own type and for numbers formatted straight from
    /// their spec.
    pub method: Option<TypedExpr>,
    pub spec: Option<FormatSpec>,
    /// `U` is a record or union shown by its `Format` instance, which receives
    /// the spec text and does its own padding.
    pub custom: bool,
}

#[derive(Clone, Debug)]
pub struct PatternAlternative {
    pub steps: Vec<PatternStep>,
    pub bindings: Vec<(Local, TypedExpr)>,
}

#[derive(Clone, Debug)]
pub enum PatternStep {
    Test(TypedExpr),
    Bind(Local, TypedExpr),
}

impl PatternStep {
    pub(crate) fn expression(&self) -> &TypedExpr {
        match self {
            Self::Test(value) | Self::Bind(_, value) => value,
        }
    }

    pub(crate) fn expression_mut(&mut self) -> &mut TypedExpr {
        match self {
            Self::Test(value) | Self::Bind(_, value) => value,
        }
    }
}

#[derive(Clone, Debug)]
pub struct TypedMatchArm {
    pub alternatives: Vec<PatternAlternative>,
    pub guard: Option<TypedExpr>,
    pub body: TypedExpr,
    /// Bindings that some alternative reaches through a reference, a
    /// collection element or tail, or another view. Those with a non-Copy
    /// type that carries no loans stay read-only views of the matched storage
    /// in the body instead of being moved out of it (`TypedMatchArm::views`).
    pub borrowed: BTreeSet<usize>,
}

impl TypedMatchArm {
    /// Whether the body sees `binding` as a view rather than an owned value.
    /// Copy values are independent owned values. Emission copies them unless
    /// the scrutinee is owned and dead afterwards and the projections do not overlap.
    pub fn views(&self, binding: &Local, types: &TypeContext<'_>) -> bool {
        self.borrowed.contains(&binding.id)
            && !binding.ty.is_copy(types)
            && !binding.ty.carries_loans(types)
    }
}

#[derive(Clone, Debug)]
pub struct HostImport {
    pub native_symbol: String,
    /// The WASM import module: `tsuzuri` unless the extern names one.
    pub wasm_module: String,
    pub wasm_name: String,
    /// Declared with a link name: the symbol is the host's own, so the header omits its prototype.
    pub explicit: bool,
    /// Takes a function-typed parameter: a callback the host calls through a C function pointer.
    pub callbacks: bool,
}

#[derive(Clone, Debug)]
pub enum TypedExprKind {
    Error,
    Int(u128),
    Float(String),
    String(StringLiteral),
    Bool(bool),
    Unit,
    Break,
    Continue,
    Local(usize),
    /// The read-only static data `CheckedModule::statics[id]` (D11 Phase 2). It is only the
    /// operand of a shared `Borrow`, which `constants::fold` builds for a constant of plain data
    /// that the program reads in place or borrows. The value is never moved, written, or dropped.
    Static(usize),
    Function(FunctionRef),
    GenericFunction(usize, Vec<Type>),
    Method(usize, usize, Type),
    TypeFunction {
        receiver: Type,
        name: String,
        module: String,
    },
    GenericInteger(u128, bool),
    GenericFloat(String),
    Unary(UnaryOp, Box<TypedExpr>),
    Binary(BinaryOp, Box<TypedExpr>, Box<TypedExpr>),
    Call(Box<TypedExpr>, Vec<TypedExpr>),
    HostCall(Box<HostImport>, Vec<TypedExpr>),
    Lambda {
        parameters: Vec<Local>,
        captures: Vec<Local>,
        body: Box<TypedExpr>,
        /// Passed directly to `Owned.function` (B07); the body borrows the captures.
        owned: bool,
        /// The functions of the `let rec` group that binds this lambda, in binding order and this
        /// one included; empty for any other lambda (B09). Their names are in scope in the body
        /// without being captured: `closures::lower` turns the uses into direct calls.
        recursive: Vec<Local>,
    },
    Closure(usize, Vec<TypedExpr>),
    TaskRun(Box<TypedExpr>),
    TaskParallel(Box<TypedExpr>),
    TaskParallelResults(Box<TypedExpr>),
    Parallel(Builtin, Vec<TypedExpr>),
    StructuralCompare(BinaryOp, Vec<TypedExpr>),
    StructuralHash(Vec<TypedExpr>),
    StructuralDisplay(Vec<TypedExpr>),
    Interpolated(Box<TypedInterpolation>),
    If {
        condition: Box<TypedExpr>,
        then_branch: Box<TypedExpr>,
        else_branch: Box<TypedExpr>,
    },
    While {
        condition: Box<TypedExpr>,
        body: Box<TypedExpr>,
    },
    ForRange {
        local: Local,
        start: Box<TypedExpr>,
        step: Box<TypedExpr>,
        finish: Box<TypedExpr>,
        body: Box<TypedExpr>,
        /// `for i = start downto finish`: the counter moves down by `step`, which is the literal
        /// `1`, so that an unsigned counter can count down too. A range `start .. step .. finish`
        /// is never descending; a negative step moves its counter down.
        descending: bool,
    },
    ForEach {
        owner: Local,
        local: Local,
        source: Box<TypedExpr>,
        body: Box<TypedExpr>,
    },
    Match {
        local: Local,
        value: Box<TypedExpr>,
        arms: Vec<TypedMatchArm>,
    },
    Block {
        bindings: Vec<(Local, TypedExpr)>,
        result: Box<TypedExpr>,
    },
    Record(Vec<(usize, TypedExpr)>),
    RecordUpdate {
        base: Box<TypedExpr>,
        fields: Vec<(usize, TypedExpr)>,
    },
    Array(Vec<TypedExpr>),
    List(Vec<TypedExpr>),
    Tuple(Vec<TypedExpr>),
    ListTail(Box<TypedExpr>, usize),
    NewArray(Box<TypedExpr>, Box<TypedExpr>),
    NewList(Box<TypedExpr>, Box<TypedExpr>),
    /// A collection literal written with `new`; it is always heap-allocated.
    NewLiteral(Box<TypedExpr>),
    Field(Box<TypedExpr>, usize),
    Index(Box<TypedExpr>, Box<TypedExpr>),
    Slice {
        value: Box<TypedExpr>,
        start: Option<Box<TypedExpr>>,
        end: Option<Box<TypedExpr>>,
    },
    Length(Box<TypedExpr>),
    StringLength(Box<TypedExpr>),
    Borrow(Box<TypedExpr>, bool),
    BorrowOperand(Box<TypedExpr>),
    Dereference(Box<TypedExpr>),
    Assign(Box<TypedExpr>, Box<TypedExpr>),
    Cast(Box<TypedExpr>),
    /// An index, a slice bound or an array length of a 128-bit integer type or a type parameter,
    /// saturated to the `i64` range: a value that `i64` cannot hold becomes `i64::MIN` or
    /// `i64::MAX`, which no collection reaches (`Checker::index_operand`). Its type is `i64`.
    SaturatingIndex(Box<TypedExpr>),
    /// A case used as a function value, `Some : 'a -> Maybe<'a>`. After
    /// specialization `closures::lower` replaces it with a generated function.
    CaseConstructor {
        union_id: usize,
        case_id: usize,
        args: Box<[Type]>,
    },
    /// A union value; its type carries the union's type arguments.
    Construct {
        union_id: usize,
        case_id: usize,
        payload: Option<Box<TypedExpr>>,
    },
    /// The `i32` tag of a union value.
    UnionTag(Box<TypedExpr>),
    /// The payload of a union value whose tag has already been tested.
    UnionPayload {
        value: Box<TypedExpr>,
        case_id: usize,
    },
    /// `@checked` integer arithmetic: an overflowing `+ - * **` or negation raises
    /// OverflowException instead of wrapping. The child is the arithmetic node.
    Checked(Box<TypedExpr>),
    /// `try ... with`: a `Result` that is `Ok body` or `Error handler`.
    Try(Box<TypedTry>),
    /// The `i32` code of the exception that the enclosing `try` caught; only in its handler.
    RaisedException,
    /// Raises the exception that the enclosing handler did not match to the next `try`.
    Reraise,
    /// The whole body of a method of a generated instance for a `dyn` type (A14): calls slot
    /// `slot` of the receiver's vtable, which has `slots` slots, with the other parameters.
    DynDispatch {
        slot: u32,
        slots: u32,
    },
}

/// A checked `try ... with ... [finally ...]`. `handler` binds the caught exception
/// from `RaisedException`, matches the arms, and ends in `Reraise` when no arm matches.
#[derive(Clone, Debug)]
pub struct TypedTry {
    pub body: TypedExpr,
    pub handler: TypedExpr,
    pub finally: Option<TypedExpr>,
}

#[derive(Clone, Copy)]
enum LocalAccess {
    Consume,
    Read,
    Write,
}

impl TypedExpr {
    fn error(span: Span) -> Self {
        Self {
            kind: TypedExprKind::Error,
            ty: Type::Error,
            span,
        }
    }

    /// Whether code generation can address the value in place.
    pub(crate) fn is_place(&self) -> bool {
        match &self.kind {
            TypedExprKind::Local(_) | TypedExprKind::Dereference(_) => true,
            TypedExprKind::Field(value, _)
            | TypedExprKind::ListTail(value, _)
            | TypedExprKind::UnionPayload { value, .. } => value.is_place(),
            TypedExprKind::Index(value, _) => {
                matches!(
                    value.ty,
                    Type::Array(_) | Type::List(_) | Type::Vec(_) | Type::FixedArray(..)
                ) && value.is_place()
            }
            _ => false,
        }
    }

    /// The first use that consumes or mutates `local`; without one the local can stay borrowed.
    pub(crate) fn consuming_use(&self, local: usize, types: &TypeContext<'_>) -> Option<Span> {
        self.local_use(local, LocalAccess::Consume, types, false)
    }

    /// `consuming_use` for a `let rec` function that borrows `local` (B09): a `match` only reads
    /// the place that it matches, and a binding that takes a non-Copy value out of it moves it. A
    /// closure that takes the local is the use, not the declaration of the local.
    pub(crate) fn moving_use(&self, local: usize, types: &TypeContext<'_>) -> Option<Span> {
        self.local_use(local, LocalAccess::Consume, types, true)
    }

    fn local_use(
        &self,
        local: usize,
        access: LocalAccess,
        types: &TypeContext<'_>,
        exact: bool,
    ) -> Option<Span> {
        use TypedExprKind::*;
        let read = |value: &Self| value.local_use(local, LocalAccess::Read, types, exact);
        let consume = |value: &Self| value.local_use(local, LocalAccess::Consume, types, exact);
        match &self.kind {
            Local(id) if *id == local => match access {
                LocalAccess::Consume if self.ty.is_copy(types) => None,
                LocalAccess::Read => None,
                LocalAccess::Consume | LocalAccess::Write => Some(self.span),
            },
            Borrow(value, mutable) => value.local_use(
                local,
                if *mutable {
                    LocalAccess::Write
                } else {
                    LocalAccess::Read
                },
                types,
                exact,
            ),
            BorrowOperand(value) => read(value),
            Slice { value, start, end } => value
                .local_use(
                    local,
                    if self.ty.is_view() {
                        LocalAccess::Write
                    } else {
                        LocalAccess::Read
                    },
                    types,
                    exact,
                )
                .or_else(|| start.iter().chain(end).find_map(|bound| consume(bound))),
            Assign(place, value) => place
                .local_use(local, LocalAccess::Write, types, exact)
                .or_else(|| consume(value)),
            Field(value, _) | Index(value, _) | UnionPayload { value, .. } if self.is_place() => {
                let access = match access {
                    LocalAccess::Consume if self.ty.is_copy(types) => LocalAccess::Read,
                    other => other,
                };
                value
                    .local_use(local, access, types, exact)
                    .or_else(|| match &self.kind {
                        Index(_, index) => consume(index),
                        _ => None,
                    })
            }
            Length(value) | StringLength(value) | UnionTag(value) => read(value),
            Closure(_, values) if exact => values.iter().find_map(consume).map(|_| self.span),
            Match { value, arms, .. } if exact => self.match_use(value, arms, local, access, types),
            Binary(
                BinaryOp::Equal
                | BinaryOp::NotEqual
                | BinaryOp::Less
                | BinaryOp::LessEqual
                | BinaryOp::Greater
                | BinaryOp::GreaterEqual,
                left,
                right,
            ) => read(left).or_else(|| read(right)),
            _ => self.children().into_iter().find_map(consume),
        }
    }

    /// `local_use` of the `match` `self` for `moving_use`: the matched place is read, and only a
    /// binding that takes a non-Copy value out of it moves it. The guards and bodies consume.
    fn match_use(
        &self,
        value: &Self,
        arms: &[TypedMatchArm],
        local: usize,
        access: LocalAccess,
        types: &TypeContext<'_>,
    ) -> Option<Span> {
        let moves = arms.iter().any(|arm| {
            arm.alternatives.iter().any(|alternative| {
                alternative
                    .bindings
                    .iter()
                    .any(|(binding, _)| !binding.ty.is_copy(types) && !arm.views(binding, types))
            })
        });
        let access = if moves { access } else { LocalAccess::Read };
        value.local_use(local, access, types, true).or_else(|| {
            let consume = |child: &Self| child.local_use(local, LocalAccess::Consume, types, true);
            self.children().into_iter().skip(1).find_map(consume)
        })
    }

    /// The direct subexpressions of this expression.
    pub fn children(&self) -> Vec<&Self> {
        use TypedExprKind::*;
        match &self.kind {
            Unary(_, value)
            | Borrow(value, _)
            | BorrowOperand(value)
            | Dereference(value)
            | Cast(value)
            | SaturatingIndex(value)
            | Field(value, _)
            | Length(value)
            | StringLength(value)
            | TaskRun(value)
            | TaskParallel(value)
            | TaskParallelResults(value)
            | NewLiteral(value)
            | UnionTag(value)
            | Checked(value)
            | UnionPayload { value, .. }
            | Construct {
                payload: Some(value),
                ..
            }
            | ListTail(value, _) => vec![value],
            Try(handled) => std::iter::once(&handled.body)
                .chain(std::iter::once(&handled.handler))
                .chain(handled.finally.iter())
                .collect(),
            Binary(_, a, b)
            | Assign(a, b)
            | Index(a, b)
            | NewArray(a, b)
            | NewList(a, b)
            | While {
                condition: a,
                body: b,
            }
            | ForEach {
                source: a, body: b, ..
            } => vec![a, b],
            ForRange {
                start,
                step,
                finish,
                body,
                ..
            } => vec![start, step, finish, body],
            Call(callee, arguments) => std::iter::once(callee.as_ref()).chain(arguments).collect(),
            Lambda { body, .. } => vec![body],
            If {
                condition,
                then_branch,
                else_branch,
            } => vec![condition, then_branch, else_branch],
            Match { value, arms, .. } => {
                let mut children = vec![value.as_ref()];
                for arm in arms {
                    for alternative in &arm.alternatives {
                        children.extend(alternative.steps.iter().map(PatternStep::expression));
                        children.extend(alternative.bindings.iter().map(|(_, value)| value));
                    }
                    children.extend(arm.guard.iter());
                    children.push(&arm.body);
                }
                children
            }
            Block { bindings, result } => bindings
                .iter()
                .map(|(_, value)| value)
                .chain(std::iter::once(result.as_ref()))
                .collect(),
            Record(fields) => fields.iter().map(|(_, value)| value).collect(),
            Slice { value, start, end } => std::iter::once(value.as_ref())
                .chain(start.iter().map(Box::as_ref))
                .chain(end.iter().map(Box::as_ref))
                .collect(),
            RecordUpdate { base, fields } => std::iter::once(base.as_ref())
                .chain(fields.iter().map(|(_, value)| value))
                .collect(),
            Array(values)
            | HostCall(_, values)
            | List(values)
            | Tuple(values)
            | Closure(_, values)
            | Parallel(_, values)
            | StructuralCompare(_, values)
            | StructuralHash(values)
            | StructuralDisplay(values) => values.iter().collect(),
            Interpolated(interpolation) => interpolation
                .holes
                .iter()
                .flat_map(|hole| std::iter::once(&hole.operand).chain(hole.method.iter()))
                .collect(),
            Break | Continue => Vec::new(),
            _ => Vec::new(),
        }
    }

    /// Visits children in `children` order without allocating a temporary vector.
    pub fn try_for_each_child<'a>(
        &'a self,
        f: &mut dyn FnMut(&'a Self) -> Result<(), Diagnostic>,
    ) -> Result<(), Diagnostic> {
        use TypedExprKind::*;
        match &self.kind {
            Unary(_, value)
            | Borrow(value, _)
            | BorrowOperand(value)
            | Dereference(value)
            | Cast(value)
            | SaturatingIndex(value)
            | Field(value, _)
            | Length(value)
            | StringLength(value)
            | TaskRun(value)
            | TaskParallel(value)
            | TaskParallelResults(value)
            | NewLiteral(value)
            | UnionTag(value)
            | Checked(value)
            | UnionPayload { value, .. }
            | Construct {
                payload: Some(value),
                ..
            }
            | ListTail(value, _) => f(value),
            Try(handled) => {
                f(&handled.body)?;
                f(&handled.handler)?;
                if let Some(finally) = &handled.finally {
                    f(finally)?;
                }
                Ok(())
            }
            Binary(_, a, b)
            | Assign(a, b)
            | Index(a, b)
            | NewArray(a, b)
            | NewList(a, b)
            | While {
                condition: a,
                body: b,
            }
            | ForEach {
                source: a, body: b, ..
            } => {
                f(a)?;
                f(b)
            }
            ForRange {
                start,
                step,
                finish,
                body,
                ..
            } => {
                f(start)?;
                f(step)?;
                f(finish)?;
                f(body)
            }
            Call(callee, arguments) => {
                f(callee)?;
                for argument in arguments {
                    f(argument)?;
                }
                Ok(())
            }
            Lambda { body, .. } => f(body),
            If {
                condition,
                then_branch,
                else_branch,
            } => {
                f(condition)?;
                f(then_branch)?;
                f(else_branch)
            }
            Match { value, arms, .. } => {
                f(value)?;
                for arm in arms {
                    for alternative in &arm.alternatives {
                        for step in &alternative.steps {
                            f(step.expression())?;
                        }
                        for (_, value) in &alternative.bindings {
                            f(value)?;
                        }
                    }
                    if let Some(guard) = &arm.guard {
                        f(guard)?;
                    }
                    f(&arm.body)?;
                }
                Ok(())
            }
            Block { bindings, result } => {
                for (_, value) in bindings {
                    f(value)?;
                }
                f(result)
            }
            Record(fields) => {
                for (_, value) in fields {
                    f(value)?;
                }
                Ok(())
            }
            Slice { value, start, end } => {
                f(value)?;
                if let Some(start) = start {
                    f(start)?;
                }
                if let Some(end) = end {
                    f(end)?;
                }
                Ok(())
            }
            RecordUpdate { base, fields } => {
                f(base)?;
                for (_, value) in fields {
                    f(value)?;
                }
                Ok(())
            }
            Array(values)
            | HostCall(_, values)
            | List(values)
            | Tuple(values)
            | Closure(_, values)
            | Parallel(_, values)
            | StructuralCompare(_, values)
            | StructuralHash(values)
            | StructuralDisplay(values) => {
                for value in values {
                    f(value)?;
                }
                Ok(())
            }
            Interpolated(interpolation) => {
                for hole in &interpolation.holes {
                    f(&hole.operand)?;
                    if let Some(method) = &hole.method {
                        f(method)?;
                    }
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    /// Visits children in `children` order without allocating a temporary vector.
    pub(crate) fn try_for_each_child_mut<'a>(
        &'a mut self,
        f: &mut dyn FnMut(&'a mut Self) -> Result<(), Diagnostic>,
    ) -> Result<(), Diagnostic> {
        use TypedExprKind::*;
        match &mut self.kind {
            Unary(_, value)
            | Borrow(value, _)
            | BorrowOperand(value)
            | Dereference(value)
            | Cast(value)
            | SaturatingIndex(value)
            | Field(value, _)
            | Length(value)
            | StringLength(value)
            | TaskRun(value)
            | TaskParallel(value)
            | TaskParallelResults(value)
            | NewLiteral(value)
            | UnionTag(value)
            | Checked(value)
            | UnionPayload { value, .. }
            | Construct {
                payload: Some(value),
                ..
            }
            | ListTail(value, _) => f(value),
            Try(handled) => {
                let TypedTry {
                    body,
                    handler,
                    finally,
                } = handled.as_mut();
                std::iter::once(body)
                    .chain(std::iter::once(handler))
                    .chain(finally.iter_mut())
                    .try_for_each(f)
            }
            Binary(_, a, b)
            | Assign(a, b)
            | Index(a, b)
            | NewArray(a, b)
            | NewList(a, b)
            | While {
                condition: a,
                body: b,
            }
            | ForEach {
                source: a, body: b, ..
            } => [a.as_mut(), b.as_mut()].into_iter().try_for_each(f),
            ForRange {
                start,
                step,
                finish,
                body,
                ..
            } => [
                start.as_mut(),
                step.as_mut(),
                finish.as_mut(),
                body.as_mut(),
            ]
            .into_iter()
            .try_for_each(f),
            Call(callee, arguments) => std::iter::once(callee.as_mut())
                .chain(arguments.iter_mut())
                .try_for_each(f),
            Lambda { body, .. } => f(body),
            If {
                condition,
                then_branch,
                else_branch,
            } => [
                condition.as_mut(),
                then_branch.as_mut(),
                else_branch.as_mut(),
            ]
            .into_iter()
            .try_for_each(f),
            Match { value, arms, .. } => visit_match_children_mut(value, arms, f),
            Block { bindings, result } => bindings
                .iter_mut()
                .map(|(_, value)| value)
                .chain(std::iter::once(result.as_mut()))
                .try_for_each(f),
            Record(fields) => fields.iter_mut().map(|(_, value)| value).try_for_each(f),
            Slice { value, start, end } => std::iter::once(value.as_mut())
                .chain(start.iter_mut().map(Box::as_mut))
                .chain(end.iter_mut().map(Box::as_mut))
                .try_for_each(f),
            RecordUpdate { base, fields } => std::iter::once(base.as_mut())
                .chain(fields.iter_mut().map(|(_, value)| value))
                .try_for_each(f),
            Array(values)
            | HostCall(_, values)
            | List(values)
            | Tuple(values)
            | Closure(_, values)
            | Parallel(_, values)
            | StructuralCompare(_, values)
            | StructuralHash(values)
            | StructuralDisplay(values) => values.iter_mut().try_for_each(f),
            Interpolated(interpolation) => interpolation
                .holes
                .iter_mut()
                .flat_map(|hole| std::iter::once(&mut hole.operand).chain(hole.method.iter_mut()))
                .try_for_each(f),
            _ => Ok(()),
        }
    }
}

fn visit_match_children_mut<'a>(
    value: &'a mut TypedExpr,
    arms: &'a mut [TypedMatchArm],
    f: &mut dyn FnMut(&'a mut TypedExpr) -> Result<(), Diagnostic>,
) -> Result<(), Diagnostic> {
    f(value)?;
    for arm in arms {
        for alternative in &mut arm.alternatives {
            for step in &mut alternative.steps {
                f(step.expression_mut())?;
            }
            for (_, value) in &mut alternative.bindings {
                f(value)?;
            }
        }
        if let Some(guard) = &mut arm.guard {
            f(guard)?;
        }
        f(&mut arm.body)?;
    }
    Ok(())
}

/// Checks one `Main` module without a standard library, preserving 2026 conventions.
/// Use [`check_modules`] and an explicit [`ModuleInput::edition`] for newer conventions.
pub fn check(program: &Program) -> Result<CheckedModule, Diagnostic> {
    let declared = program.namespace.as_ref().map(|namespace| &namespace.path);
    let (name, namespace) = crate::module_identity("Main", declared, "")?;
    check_modules(&[ModuleInput {
        name: &name,
        program,
        origin: ModuleOrigin::User,
        namespace: &namespace,
        entry: true,
        edition: crate::syntax::Edition::E2026,
        package: None,
    }])
}

#[derive(Clone, Debug)]
struct NameInfo {
    id: usize,
    name: String,
    module: String,
    visibility: Visibility,
}

impl NameInfo {
    fn visible_from(&self, requester: &str) -> bool {
        self.visibility == Visibility::Public || self.module == requester
    }

    fn require_visible(&self, kind: &str, requester: &str, span: Span) -> Result<(), Diagnostic> {
        if self.visible_from(requester) {
            return Ok(());
        }
        Err(self.private_error(kind, span))
    }

    fn private_error(&self, kind: &str, span: Span) -> Diagnostic {
        let name = if kind == "type" {
            type_display(&self.name)
        } else {
            declaration_display(&self.name)
        };
        Diagnostic::of(
            DiagnosticKind::PrivateNameAccess,
            format!(
                "private {kind} '{name}' is only visible inside module '{}'; expose a public wrapper or move the use into the same module",
                namespace_display(&self.module)
            ),
            span,
        )
    }
}

#[path = "intern.rs"]
mod intern;

/// Where a function is declared: its name and the spans of its parameters' types, for the
/// notes of diagnostics that point at the signature.
#[derive(Clone, Default)]
struct DeclaredFunction {
    name: String,
    parameters: Vec<Span>,
}

#[derive(Default)]
struct Names {
    /// By function id; the declarations of tests and benches stay empty.
    declared: Vec<DeclaredFunction>,
    warning_options: warnings::WarningOptions,
    borrowing_modules: BTreeSet<String>,
    modules: BTreeMap<String, ModuleOrigin>,
    /// Module keys by full name (`Namespace.File`), and each module's namespace by key.
    full_modules: BTreeMap<String, String>,
    module_namespaces: BTreeMap<String, String>,
    /// The namespaces that each module's `using` declarations import, by key.
    module_usings: BTreeMap<String, Vec<String>>,
    builders: BTreeMap<String, BTreeSet<String>>,
    /// The keys of the builders that declare each `@alias` name (`.tc` files, in module order).
    builder_aliases: BTreeMap<String, Vec<String>>,
    type_aliases: BTreeMap<String, (NameInfo, TypeAliasDecl)>,
    type_alias_names: BTreeMap<String, Vec<String>>,
    records: BTreeMap<String, NameInfo>,
    record_aliases: BTreeMap<String, Vec<String>>,
    /// `extern type` handles share the type namespace with records and unions.
    handles: BTreeMap<String, NameInfo>,
    handle_aliases: BTreeMap<String, Vec<String>>,
    /// Number of type parameters of each record declaration, by record id.
    record_arities: Vec<usize>,
    /// Whether each type parameter of a record is a length parameter `const N: i64` (A16).
    record_lengths: Vec<Box<[bool]>>,
    /// Unions share the type namespace with records and classes.
    unions: BTreeMap<String, NameInfo>,
    union_aliases: BTreeMap<String, Vec<String>>,
    union_arities: Vec<usize>,
    union_lengths: Vec<Box<[bool]>>,
    /// Union cases by qualified `Module.Case`; a module declares each case name once.
    cases: BTreeMap<String, CaseInfo>,
    case_aliases: BTreeMap<String, Vec<String>>,
    /// Built-in class names and qualified user class names, as in `Classes`.
    classes: BTreeSet<String>,
    /// Qualified user class names by unqualified name.
    class_aliases: BTreeMap<String, Vec<String>>,
    functions: BTreeMap<String, NameInfo>,
    symbols: intern::Interner,
    function_ids: std::collections::HashMap<(intern::Symbol, intern::Symbol), usize>,
    constants: BTreeSet<usize>,
    /// The functions declared `const def` (D11), by function id.
    const_functions: BTreeSet<usize>,
    pure_functions: BTreeSet<usize>,
    /// The values of the `i64` constants with an integer literal value, which can be lengths (A16).
    length_constants: BTreeMap<usize, u64>,
    active_patterns: BTreeMap<String, (NameInfo, ActiveCase)>,
    active_aliases: BTreeMap<String, Vec<String>>,
}

/// How the name before `{` of a computation expression resolves.
enum BuilderName<'a> {
    /// The key of the builder module that the name, or its `@alias`, names.
    Builder(&'a str),
    /// An `@alias` that these builders declare, all in sight.
    Ambiguous(Vec<&'a str>),
    /// No builder.
    Missing,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ActiveCase {
    TotalSingle,
    BoolPartial,
    MaybePartial,
    TotalCase { index: usize, count: usize },
}

#[derive(Clone, Debug)]
struct CaseInfo {
    /// `id` is the union id, and `name` is the qualified `Module.Case`.
    info: NameInfo,
    case: usize,
    has_payload: bool,
}

/// A record, union, alias or extern type name.
#[derive(Clone, Copy)]
enum NamedType<'a> {
    Record(&'a NameInfo),
    Union(&'a NameInfo),
    Alias(&'a NameInfo),
    Handle(&'a NameInfo),
}

impl<'a> NamedType<'a> {
    fn info(self) -> &'a NameInfo {
        match self {
            Self::Record(info) | Self::Union(info) | Self::Alias(info) | Self::Handle(info) => info,
        }
    }

    fn kind(self) -> &'static str {
        match self {
            Self::Record(_) => "record",
            Self::Union(_) => "union",
            Self::Alias(_) => "type alias",
            Self::Handle(_) => "extern type",
        }
    }
}

/// What the head of a type application such as `Add<'a>` or `Pair<i64, string>` names.
enum TypeHead<'a> {
    Class,
    Type(NamedType<'a>),
}

/// The outcome of looking a name up among the declarations of all modules.
enum Choice<T> {
    /// The declaration and its rank: 0 for a built-in class, the requester's
    /// own declaration, or an exact qualified name, and 1 + tier for the
    /// unique visible declaration of a tier.
    Found(T, u8),
    /// The visible declarations of the first tier that has several, and its rank.
    Ambiguous(Vec<T>, u8),
    /// Only private declarations of other modules have the name.
    Hidden(T),
    Missing,
}

impl<T> Choice<T> {
    fn rank(&self) -> Option<u8> {
        match self {
            Self::Found(_, rank) | Self::Ambiguous(_, rank) => Some(*rank),
            Self::Hidden(_) | Self::Missing => None,
        }
    }
}

/// Starts a name that the compiler builds from a resolved declaration, such
/// as `::Geometry.Point.Point`: a module key, `.`, and the declaration's
/// name, which namespace resolution leaves as it is. Source paths never
/// start with `::`.
const KEY_PATH: &str = "::";

fn key_path(qualified: &str) -> String {
    if qualified.contains('.') {
        format!("{KEY_PATH}{qualified}")
    } else {
        qualified.to_owned()
    }
}

/// A dotted full name or namespace as source code writes it, as
/// `Sample::Features::Shape` for `Sample.Features.Shape`.
fn namespace_display(dotted: &str) -> String {
    dotted.replace('.', "::")
}

/// `$bench.<index> ($iterations: i64) -> i64 = { let $case: i64 -> i64 = body; $case $iterations }`
/// (G18 D4): a body of another type reports E1003 where it is written, and the runner calls the
/// function directly. Names that start with `$` cannot hide the user's names.
fn bench_function(bench: &crate::syntax::BenchDecl, index: usize) -> FunctionDecl {
    let span = bench.body.span;
    let named = |text: &str| Ident {
        text: text.into(),
        span,
        provenance: Provenance::Generated,
    };
    let i64_type = || TypeExpr {
        kind: TypeExprKind::Named("i64".into()),
        span,
    };
    let name = |text: &str| Expr {
        kind: ExprKind::Name(named(text)),
        span,
        depth: 1,
    };
    let call = Expr {
        kind: ExprKind::Call(Box::new(name("$case")), vec![name("$iterations")]),
        span,
        depth: 2,
    };
    FunctionDecl {
        doc: None,
        name: Ident {
            text: format!("$bench.{index}"),
            span: bench.name_span,
            provenance: Provenance::Generated,
        },
        regions: Vec::new(),
        recursion: None,
        visibility: Visibility::Private,
        exported: false,
        constant: false,
        pure: false,
        parameters: vec![Parameter {
            name: named("$iterations"),
            ty: i64_type(),
            mutable: false,
            json: None,
        }],
        result: i64_type(),
        constraints: Vec::new(),
        body: Expr {
            kind: ExprKind::Block {
                bindings: vec![Binding {
                    name: named("$case"),
                    mutable: false,
                    using: false,
                    recursion: Recursion::None,
                    annotation: Some(TypeExpr {
                        kind: TypeExprKind::Function(vec![i64_type()], Box::new(i64_type())),
                        span,
                    }),
                    value: bench.body.clone(),
                }],
                result: Box::new(call),
            },
            span,
            depth: bench.body.depth.max(2) + 1,
        },
    }
}

/// A key-qualified declaration as source code writes it from the root
/// namespace, as `Geometry::Point.distance` for `Geometry.Point.distance`.
fn declaration_display(qualified: &str) -> String {
    match qualified.rsplit_once('.') {
        Some((key, name)) => format!("{}.{name}", namespace_display(key)),
        None => qualified.to_owned(),
    }
}

/// A key-qualified type as source code writes it from the root namespace. A
/// type named after its module has the module's full name, as
/// `Geometry::Point` for `Geometry.Point.Point`.
fn type_display(qualified: &str) -> String {
    match qualified.rsplit_once('.') {
        Some((key, name)) if module_stem(key) == name => namespace_display(key),
        _ => declaration_display(qualified),
    }
}

/// The file stem that ends the module key or full name `module`.
fn module_stem(module: &str) -> &str {
    module.rsplit('.').next().unwrap_or(module)
}

/// The legacy, default-read and explicitly owning entry points across editions.
pub(crate) const BORROWING_APIS: &[(&str, &str, &str)] = &[
    ("Array.map", "Array.mapr", "Array.map_owned"),
    ("Array.mapi", "Array.mapir", "Array.mapi_owned"),
    ("Array.iter", "Array.iterr", "Array.iter_owned"),
    ("Array.iteri", "Array.iterir", "Array.iteri_owned"),
    ("Array.fold", "Array.foldr", "Array.fold_owned"),
    (
        "Array.fold_back",
        "Array.foldr_back",
        "Array.fold_back_owned",
    ),
    ("List.map", "List.mapr", "List.map_owned"),
    ("List.mapi", "List.mapir", "List.mapi_owned"),
    ("List.iter", "List.iterr", "List.iter_owned"),
    ("List.iteri", "List.iterir", "List.iteri_owned"),
    ("List.fold", "List.foldr", "List.fold_owned"),
    ("List.fold_back", "List.foldr_back", "List.fold_back_owned"),
    ("Vec.push", "Vec.push_mut", "Vec.push_owned"),
];

/// The builtin module function that `path` names, as `Task.run` or, with the
/// std namespace, `std::Task.run`.
fn qualified_builtin(path: &str) -> Option<Builtin> {
    let path = path
        .strip_prefix(crate::stdlib::NAMESPACE)
        .and_then(|rest| rest.strip_prefix("::"))
        .filter(|member| member.contains('.'))
        .unwrap_or(path);
    let path = BORROWING_APIS
        .iter()
        .find(|(_, _, owned)| *owned == path)
        .map_or(path, |(value, _, _)| *value);
    Builtin::ALL
        .iter()
        .copied()
        .find(|builtin| builtin.name() == path)
}

/// Whether `signature` is one that an application's `main` may have: `unit -> i32`,
/// or `Array<string> -> i32` to receive the command-line arguments.
fn entry_signature(signature: &Signature) -> bool {
    signature.result == Type::I32
        && match signature.parameters.as_slice() {
            [Type::Unit] => true,
            [Type::Array(element)] => **element == Type::String,
            _ => false,
        }
}

impl Names {
    fn local_function(&self, module: &str, name: &str) -> Option<usize> {
        let module = self.symbols.lookup(module)?;
        let name = self.symbols.lookup(name)?;
        self.function_ids.get(&(module, name)).copied()
    }

    fn origin(&self, module: &str) -> ModuleOrigin {
        self.modules
            .get(module)
            .copied()
            .unwrap_or(ModuleOrigin::User)
    }

    /// The module origins whose declarations `requester` searches, in order;
    /// std code never sees user declarations (GUIDE D-07).
    fn tiers(&self, requester: &str) -> &'static [ModuleOrigin] {
        match self.origin(requester) {
            ModuleOrigin::User => &[ModuleOrigin::User, ModuleOrigin::Std],
            ModuleOrigin::Std => &[ModuleOrigin::Std],
        }
    }

    /// Whether code in `requester` may name the declarations of `module`.
    fn searchable(&self, requester: &str, module: &str) -> bool {
        self.modules
            .get(module)
            .is_some_and(|origin| self.tiers(requester).contains(origin))
    }

    /// Whether `requester` may name the module of a qualified `Module.name`.
    fn searchable_path(&self, requester: &str, qualified: &str) -> bool {
        qualified
            .rsplit_once('.')
            .is_some_and(|(module, _)| self.searchable(requester, module))
    }

    /// The key of the module that `path` (`Name` or `A::B::Name`) names from
    /// `requester`. The requester's namespace, then for a bare name the
    /// namespaces of its `using` declarations, and then each enclosing
    /// namespace qualify `path`, innermost first; a bare name may also be a
    /// module key. A bare name that several `using` namespaces hold names
    /// nothing (see `check_module`).
    fn module_path(&self, requester: &str, path: &str) -> Option<&str> {
        if path.contains('.') || path.starts_with(KEY_PATH) {
            return None;
        }
        let bare = !path.contains("::");
        let dotted = if bare {
            std::borrow::Cow::Borrowed(path)
        } else {
            std::borrow::Cow::Owned(path.replace("::", "."))
        };
        let mut scope = self
            .module_namespaces
            .get(requester)
            .map_or("", String::as_str);
        let mut usings = self.module_usings.get(requester).filter(|_| bare);
        loop {
            let found = if scope.is_empty() {
                self.full_modules.get(dotted.as_ref())
            } else {
                self.full_modules.get(&format!("{scope}.{dotted}"))
            };
            if let Some(key) = found {
                return Some(key);
            }
            if let Some(namespaces) = usings.take() {
                let mut imported = self.imported(namespaces, path);
                match (imported.next(), imported.next()) {
                    (Some(key), None) => return Some(key),
                    (Some(_), Some(_)) => return None,
                    _ => {}
                }
            }
            if scope.is_empty() {
                break;
            }
            scope = scope.rsplit_once('.').map_or("", |(parent, _)| parent);
        }
        bare.then(|| {
            self.modules
                .get_key_value(path)
                .map(|(key, _)| key.as_str())
        })
        .flatten()
    }

    /// The keys of the modules named `name` in `namespaces`.
    fn imported<'s>(
        &'s self,
        namespaces: &'s [String],
        name: &str,
    ) -> impl Iterator<Item = &'s str> {
        namespaces.iter().filter_map(move |namespace| {
            self.full_modules
                .get(&format!("{namespace}.{name}"))
                .map(String::as_str)
        })
    }

    /// Rejects a module name that the `using` declarations of `requester`
    /// make ambiguous: several of their namespaces hold a module named
    /// `name`, and the requester's own namespace holds none.
    fn check_module(&self, requester: &str, name: &str, span: Span) -> Result<(), Diagnostic> {
        let Some(namespaces) = self
            .module_usings
            .get(requester)
            .filter(|_| !name.contains(['.', ':']))
        else {
            return Ok(());
        };
        let scope = self
            .module_namespaces
            .get(requester)
            .map_or("", String::as_str);
        let own = if scope.is_empty() {
            name.to_owned()
        } else {
            format!("{scope}.{name}")
        };
        if self.full_modules.contains_key(&own) || self.imported(namespaces, name).nth(1).is_none()
        {
            return Ok(());
        }
        let modules = namespaces
            .iter()
            .map(|namespace| format!("{namespace}.{name}"))
            .filter(|full| self.full_modules.contains_key(full))
            .map(|full| format!("'{}'", namespace_display(&full)))
            .collect::<Vec<_>>()
            .join(", ");
        Err(Diagnostic::of(
            DiagnosticKind::AmbiguousName,
            format!(
                "'{name}' is ambiguous: using declarations make the modules {modules} visible; qualify it with its namespace"
            ),
            span,
        ))
    }

    /// Rejects `path` when the `using` declarations make the module name that
    /// starts it ambiguous. A namespace path such as `A::B.f` and a key path
    /// do not search `using` namespaces.
    fn check_path(&self, requester: &str, path: &str, span: Span) -> Result<(), Diagnostic> {
        if path.contains("::") {
            return Ok(());
        }
        let Some((first, _)) = path.split_once('.') else {
            return Ok(());
        };
        self.check_module(requester, first, span)
    }

    /// The namespace that `path` (`A::B`) names from `requester`, as `using
    /// path` imports it: `path` in the requester's namespace or an enclosing
    /// one, innermost first, when it holds a module that `requester` may search.
    fn namespace_path(&self, requester: &str, path: &str) -> Option<String> {
        if path.contains('.') || path.starts_with(KEY_PATH) {
            return None;
        }
        let path = path.replace("::", ".");
        let mut scope = self
            .module_namespaces
            .get(requester)
            .map_or("", String::as_str);
        loop {
            let namespace = if scope.is_empty() {
                path.clone()
            } else {
                format!("{scope}.{path}")
            };
            let prefix = format!("{namespace}.");
            if self
                .full_modules
                .range(prefix.clone()..)
                .take_while(|(full, _)| full.starts_with(&prefix))
                .any(|(_, key)| self.searchable(requester, key))
            {
                return Some(namespace);
            }
            if scope.is_empty() {
                return None;
            }
            scope = scope.rsplit_once('.').map_or("", |(parent, _)| parent);
        }
    }

    /// `path` with its module part, up to the first `.` after any namespace
    /// path, replaced by that module's key, so that `Sample::Shape.area` and
    /// `Shape.area` in namespace `Sample` agree. A key path is already
    /// qualified, and a bare name or a module path without members stays as
    /// written. `None` means that the module part names no module.
    fn canonical<'p>(&self, requester: &str, path: &'p str) -> Option<std::borrow::Cow<'p, str>> {
        if let Some(qualified) = path.strip_prefix(KEY_PATH) {
            return Some(std::borrow::Cow::Borrowed(qualified));
        }
        let start = path.rfind("::").map_or(0, |at| at + 2);
        let Some(dot) = path[start..].find('.').map(|at| start + at) else {
            return Some(std::borrow::Cow::Borrowed(path));
        };
        let key = self.module_path(requester, &path[..dot])?;
        Some(if key == &path[..dot] {
            std::borrow::Cow::Borrowed(path)
        } else {
            std::borrow::Cow::Owned(format!("{key}{}", &path[dot..]))
        })
    }

    /// `path` written with `::` after the namespaces that its leading dotted
    /// segments name, as `Sample::Shape.area` for `Sample.Shape.area`, and
    /// `Sample::Point` for the record `Sample.Point.Point`.
    fn namespace_spelling(&self, requester: &str, path: &str) -> Option<String> {
        if path.contains(':') {
            return None;
        }
        let segments: Vec<&str> = path.split('.').collect();
        (1..segments.len()).find_map(|end| {
            let module = segments[..=end].join("::");
            let key = self
                .namespace_path(requester, &segments[..end].join("::"))
                .and_then(|_| self.module_path(requester, &module))?;
            let mut members = &segments[end + 1..];
            if members.len() == 1
                && members[0] == segments[end]
                && self.type_kind(&format!("{key}.{}", members[0])).is_some()
            {
                members = &[];
            }
            Some(
                std::iter::once(module.as_str())
                    .chain(members.iter().copied())
                    .collect::<Vec<_>>()
                    .join("."),
            )
        })
    }

    /// How source code names the key-qualified declaration `qualified`: its
    /// module's full name, then `.` and the declaration, as `Sample::Point.distance`.
    fn spelling(&self, qualified: &str) -> String {
        match qualified.rsplit_once('.') {
            Some((key, member)) => format!("{}.{member}", self.module_display(key)),
            None => qualified.to_owned(),
        }
    }

    /// `spelling` for a type: a type named after its module has the module's
    /// full name, as `Sample::Point`.
    fn type_spelling(&self, qualified: &str) -> String {
        match qualified.rsplit_once('.') {
            Some((key, name)) if module_stem(key) == name => self.module_display(key),
            _ => self.spelling(qualified),
        }
    }

    /// What kind of type the key-qualified `qualified` names, if any.
    fn type_kind(&self, qualified: &str) -> Option<&'static str> {
        if self.records.contains_key(qualified) {
            Some("record")
        } else if self.unions.contains_key(qualified) {
            Some("union")
        } else if self.handles.contains_key(qualified) {
            Some("extern type")
        } else if self.type_aliases.contains_key(qualified) {
            Some("type alias")
        } else {
            None
        }
    }

    /// Rejects a type path that repeats its module's name, as
    /// `Sample::Point.Point`: a type named after its module has the module's
    /// full name, `Sample::Point`.
    fn check_module_type(&self, requester: &str, path: &str, span: Span) -> Result<(), Diagnostic> {
        let Some((written, name)) = path.rsplit_once('.') else {
            return Ok(());
        };
        if path.starts_with(KEY_PATH)
            || written.contains('.')
            || written.rsplit("::").next() != Some(name)
        {
            return Ok(());
        }
        match self
            .module_path(requester, written)
            .and_then(|key| self.type_kind(&format!("{key}.{name}")))
        {
            Some(kind) => Err(Self::repeated_module(path, kind, name, written, span)),
            None => Ok(()),
        }
    }

    fn repeated_module(
        path: &str,
        kind: &str,
        name: &str,
        written: &str,
        span: Span,
    ) -> Diagnostic {
        Diagnostic::of(
            DiagnosticKind::RepeatedModuleName,
            format!(
                "'{path}' repeats the module name; the {kind} '{name}' shares its module's name, so write '{written}'"
            ),
            span,
        )
    }

    /// The full name of the module `key` as source code writes it.
    fn module_display(&self, key: &str) -> String {
        let stem = key.rsplit('.').next().unwrap_or(key);
        match self.module_namespaces.get(key).map(String::as_str) {
            None | Some("") => namespace_display(key),
            Some(namespace) => namespace_display(&format!("{namespace}.{stem}")),
        }
    }

    /// The computation builder that `name` names from `requester`: a builder module's name, which
    /// may be qualified by namespaces, or one of its `@alias` names. An alias is another spelling
    /// of its builder's name, so it is in sight wherever that name is. User builders take an alias
    /// before std ones, as for declarations, and the opt-in rule for std types does not apply: the
    /// std builder `Async` is reached as `async` once the program writes it (D-43).
    fn builder_name(&self, requester: &str, name: &str) -> BuilderName<'_> {
        if let Some(key) = self
            .module_path(requester, name)
            .filter(|key| self.builders.contains_key(*key))
        {
            return BuilderName::Builder(key);
        }
        // An alias is a lowercase word. The lexer reads `::` before one as list cons, so no
        // namespace path leads to it; a builder's own name does.
        let Some(candidates) = self.builder_aliases.get(name) else {
            return BuilderName::Missing;
        };
        for tier in self.tiers(requester) {
            let visible: Vec<&str> = candidates
                .iter()
                .map(String::as_str)
                .filter(|key| self.origin(key) == *tier && self.builder_in_sight(requester, key))
                .collect();
            match visible.as_slice() {
                [] => {}
                [key] => return BuilderName::Builder(key),
                _ => return BuilderName::Ambiguous(visible),
            }
        }
        BuilderName::Missing
    }

    /// Whether the bare name of the builder module `key` is in sight of `requester`: it names
    /// `key`, or several `using` namespaces hold a module of that name and `key` is one of them.
    /// The second case keeps same-named builders of two namespaces from hiding each other's alias.
    fn builder_in_sight(&self, requester: &str, key: &str) -> bool {
        let stem = module_stem(key);
        match self.module_path(requester, stem) {
            Some(found) => found == key,
            None => self.module_usings.get(requester).is_some_and(|namespaces| {
                self.imported(namespaces, stem).any(|found| found == key)
            }),
        }
    }

    /// Whether `name {}` is an empty computation expression, not an empty record: `name` is a
    /// builder or an alias in sight, or a lowercase word that no record is named, which can only
    /// be an alias that nobody declared (`computation::lower` says so).
    fn empty_block_is_computation(&self, requester: &str, name: &str) -> bool {
        !matches!(self.builder_name(requester, name), BuilderName::Missing)
            || (name.starts_with(|first: char| first.is_ascii_lowercase())
                && !self.record_aliases.contains_key(name))
    }

    /// The error for an alias that several builders in sight declare.
    fn ambiguous_alias(&self, written: &str, keys: &[&str], span: Span) -> Diagnostic {
        let builders = keys
            .iter()
            .map(|key| format!("'{}'", self.module_display(key)))
            .collect::<Vec<_>>()
            .join(", ");
        Diagnostic::of(
            DiagnosticKind::AmbiguousBuilderAlias,
            format!(
                "builder alias '{written}' is ambiguous: the builders {builders} declare it; write the name of the builder you mean, or rename an alias"
            ),
            span,
        )
    }

    /// Chooses among same-named declarations of other modules, one tier of
    /// module origins at a time. Private declarations neither resolve nor
    /// make a name ambiguous.
    fn choose<'m, T: Copy>(
        &self,
        requester: &str,
        candidates: &[T],
        module: impl Fn(T) -> &'m str,
        visible: impl Fn(T) -> bool,
    ) -> Choice<T> {
        let mut hidden = None;
        let user = self.origin(requester) == ModuleOrigin::User;
        for (rank, tier) in (1..).zip(self.tiers(requester)) {
            let declared: Vec<T> = candidates
                .iter()
                .copied()
                .filter(|candidate| {
                    let module = module(*candidate);
                    self.origin(module) == *tier
                        // User code names the declarations of an opt-in std module only
                        // qualified, so loading it never changes a bare name (D-40).
                        && !(user && *tier == ModuleOrigin::Std && crate::stdlib::is_opt_in(module))
                })
                .collect();
            let shown: Vec<T> = declared
                .iter()
                .copied()
                .filter(|candidate| visible(*candidate))
                .collect();
            match shown.as_slice() {
                [candidate] => return Choice::Found(*candidate, rank),
                [] => hidden = hidden.or(declared.first().copied()),
                _ => return Choice::Ambiguous(shown, rank),
            }
        }
        hidden.map_or(Choice::Missing, Choice::Hidden)
    }

    /// Classifies an applied name. The better-ranked meaning wins, and a
    /// class and a record or union type of equal rank are ambiguous rather
    /// than silently preferring either meaning.
    fn type_head(&self, module: &str, head: &Ident) -> Result<TypeHead<'_>, Diagnostic> {
        self.check_type_path(module, &head.text, head.span)?;
        let class = self.class_choice(module, &head.text);
        let named = self.type_choice(module, &head.text);
        let (class_rank, type_rank) = (class.rank(), named.rank());
        if class_rank.is_some() && (type_rank.is_none() || class_rank < type_rank) {
            return self
                .class_result(class, &head.text, head.span)
                .map(|_| TypeHead::Class);
        }
        if type_rank.is_some() && (class_rank.is_none() || type_rank < class_rank) {
            return self
                .type_result(named, &head.text, head.span)
                .map(TypeHead::Type);
        }
        match (class, named) {
            (Choice::Found(..), Choice::Found(named, _)) => Err(Diagnostic::of(
                DiagnosticKind::AmbiguousName,
                format!(
                    "'{}' names both a type class and the {} type '{}'; qualify the {} type or rename one of them",
                    head.text,
                    named.kind(),
                    self.type_spelling(&named.info().name),
                    named.kind()
                ),
                head.span,
            )),
            (class @ Choice::Ambiguous(..), _) => self
                .class_result(class, &head.text, head.span)
                .map(|_| TypeHead::Class),
            (_, named @ (Choice::Ambiguous(..) | Choice::Hidden(_))) => self
                .type_result(named, &head.text, head.span)
                .map(TypeHead::Type),
            _ => Err(Diagnostic::of(
                DiagnosticKind::UnknownTypeName,
                format!(
                    "unknown type class, record type, or union type '{}'; {}",
                    head.text,
                    self.namespace_spelling(module, &head.text).map_or_else(
                        || "declare it or check the spelling".to_owned(),
                        |spelling| format!(
                            "join namespaces and modules with '::', as in '{spelling}'"
                        )
                    )
                ),
                head.span,
            )),
        }
    }

    fn record(&self, module: &str, name: &str, span: Span) -> Result<usize, Diagnostic> {
        match self.named_type(module, name, span)? {
            NamedType::Record(info) => Ok(info.id),
            NamedType::Union(info) => Err(Diagnostic::of(
                DiagnosticKind::NotARecordType,
                format!(
                    "'{name}' names the union type '{}', not a record type; construct or match it with its cases",
                    self.type_spelling(&info.name)
                ),
                span,
            )),
            NamedType::Alias(info) => Err(Diagnostic::of(
                DiagnosticKind::NotARecordType,
                format!(
                    "'{}' is a type alias; use the original record name to construct or match a value",
                    self.type_spelling(&info.name)
                ),
                span,
            )),
            NamedType::Handle(info) => Err(Diagnostic::of(
                DiagnosticKind::NotARecordType,
                format!(
                    "'{name}' names the extern type '{}', not a record type; host handles have no fields and cannot be constructed",
                    self.type_spelling(&info.name)
                ),
                span,
            )),
        }
    }

    /// Resolves a record or union type name: the requester's own declaration,
    /// then an exact qualified name or the type of the module that the name
    /// names, then the unique visible declaration of user modules and then of
    /// std modules.
    fn named_type(
        &self,
        module: &str,
        name: &str,
        span: Span,
    ) -> Result<NamedType<'_>, Diagnostic> {
        self.check_type_path(module, name, span)?;
        let choice = self.type_choice(module, name);
        if matches!(choice, Choice::Missing)
            && let Some(spelling) = self.namespace_spelling(module, name)
        {
            return Err(Diagnostic::of(
                DiagnosticKind::UnknownTypeName,
                format!(
                    "unknown record, union, or type alias '{name}'; join namespaces and modules with '::', as in '{spelling}'"
                ),
                span,
            ));
        }
        self.type_result(choice, name, span)
    }

    /// `check_path` for a type name. A bare name is also a module name, so
    /// `using` declarations can make it ambiguous unless the requester
    /// declares that type or class, or it is a built-in class.
    fn check_type_path(&self, module: &str, name: &str, span: Span) -> Result<(), Diagnostic> {
        if name.contains(['.', ':']) {
            self.check_path(module, name, span)?;
            return self.check_module_type(module, name, span);
        }
        let own = format!("{module}.{name}");
        if self.records.contains_key(&own)
            || self.unions.contains_key(&own)
            || self.type_aliases.contains_key(&own)
            || self.handles.contains_key(&own)
            || self.classes.contains(&own)
            || self.classes.contains(name)
        {
            return Ok(());
        }
        self.check_module(module, name, span)
    }

    fn type_choice(&self, module: &str, name: &str) -> Choice<NamedType<'_>> {
        let own = format!("{module}.{name}");
        if let Some(info) = self.records.get(&own) {
            return Choice::Found(NamedType::Record(info), 0);
        }
        if let Some(info) = self.unions.get(&own) {
            return Choice::Found(NamedType::Union(info), 0);
        }
        if let Some((info, _)) = self.type_aliases.get(&own) {
            return Choice::Found(NamedType::Alias(info), 0);
        }
        if let Some(info) = self.handles.get(&own) {
            return Choice::Found(NamedType::Handle(info), 0);
        }
        let exact = |qualified: &str| {
            self.records
                .get(qualified)
                .map(NamedType::Record)
                .or_else(|| self.unions.get(qualified).map(NamedType::Union))
                .or_else(|| self.handles.get(qualified).map(NamedType::Handle))
                .or_else(|| {
                    self.type_aliases
                        .get(qualified)
                        .map(|(info, _)| NamedType::Alias(info))
                })
                .map(|named| {
                    if named.info().visible_from(module) {
                        Choice::Found(named, 0)
                    } else {
                        Choice::Hidden(named)
                    }
                })
        };
        if let Some(canonical) = self.canonical(module, name)
            && self.searchable_path(module, &canonical)
            && let Some(choice) = exact(&canonical)
        {
            return choice;
        }
        // A path to a module names the type that shares the module's name, whose
        // full name it is: `Sample::Point` is the record `Point` of the module
        // `Sample::Point`, and a bare `Maybe` is the std union even when another
        // module declares a `Maybe`. It ranks with its module's origin ahead of
        // the search below; private types keep that search.
        if !name.contains('.')
            && let Some(key) = self.module_path(module, name)
            && self.searchable(module, key)
            && let Some(choice) = exact(&format!("{key}.{}", module_stem(key)))
        {
            if name.contains("::") {
                return choice;
            }
            if let Choice::Found(named, _) = choice {
                let origin = self.origin(key);
                let tier = self.tiers(module).iter().position(|tier| *tier == origin);
                return Choice::Found(named, 1 + tier.unwrap_or(0) as u8);
            }
        }
        let candidates: Vec<NamedType<'_>> = self
            .record_aliases
            .get(name)
            .into_iter()
            .flatten()
            .map(|qualified| NamedType::Record(&self.records[qualified]))
            .chain(
                self.union_aliases
                    .get(name)
                    .into_iter()
                    .flatten()
                    .map(|qualified| NamedType::Union(&self.unions[qualified])),
            )
            .chain(
                self.handle_aliases
                    .get(name)
                    .into_iter()
                    .flatten()
                    .map(|qualified| NamedType::Handle(&self.handles[qualified])),
            )
            .chain(
                self.type_alias_names
                    .get(name)
                    .into_iter()
                    .flatten()
                    .map(|qualified| NamedType::Alias(&self.type_aliases[qualified].0)),
            )
            .collect();
        self.choose(
            module,
            &candidates,
            |named| named.info().module.as_str(),
            |named| named.info().visible_from(module),
        )
    }

    fn type_result<'n>(
        &self,
        choice: Choice<NamedType<'n>>,
        name: &str,
        span: Span,
    ) -> Result<NamedType<'n>, Diagnostic> {
        match choice {
            Choice::Found(named, _) => Ok(named),
            Choice::Hidden(named) => Err(named.info().private_error("type", span)),
            Choice::Missing => Err(Diagnostic::of(
                DiagnosticKind::UnknownTypeName,
                format!("unknown record, union, or type alias '{name}'"),
                span,
            )),
            Choice::Ambiguous(visible, _) => {
                let kind = if visible
                    .iter()
                    .all(|named| matches!(named, NamedType::Record(_)))
                {
                    "record type"
                } else if visible
                    .iter()
                    .all(|named| matches!(named, NamedType::Union(_)))
                {
                    "union type"
                } else {
                    "type"
                };
                Err(Diagnostic::of(
                    DiagnosticKind::AmbiguousName,
                    format!(
                        "ambiguous {kind} '{name}'; qualify it as {}",
                        visible
                            .iter()
                            .map(|named| self.type_spelling(&named.info().name))
                            .collect::<Vec<_>>()
                            .join(" or ")
                    ),
                    span,
                ))
            }
        }
    }

    /// Finds a class: a built-in class, the requester's own class, an exact
    /// qualified name, then the unique class of user modules and then of std
    /// modules. Classes have no visibility.
    fn class_choice(&self, module: &str, name: &str) -> Choice<&str> {
        let builtin = !name.contains(['.', ':']);
        let exact = builtin.then(|| self.classes.get(name)).flatten();
        let exact = exact.or_else(|| self.classes.get(&format!("{module}.{name}")));
        let exact = exact.or_else(|| {
            self.canonical(module, name)
                .filter(|canonical| self.searchable_path(module, canonical))
                .and_then(|canonical| self.classes.get(canonical.as_ref()))
        });
        if let Some(class) = exact {
            return Choice::Found(class, 0);
        }
        let candidates: Vec<&str> = self
            .class_aliases
            .get(name)
            .into_iter()
            .flatten()
            .map(String::as_str)
            .collect();
        self.choose(
            module,
            &candidates,
            |class| class.rsplit_once('.').map_or("", |(module, _)| module),
            |_| true,
        )
    }

    fn class_result<'n>(
        &self,
        choice: Choice<&'n str>,
        name: &str,
        span: Span,
    ) -> Result<Option<&'n str>, Diagnostic> {
        match choice {
            Choice::Found(class, _) => Ok(Some(class)),
            Choice::Ambiguous(classes, _) => Err(Diagnostic::of(
                DiagnosticKind::AmbiguousName,
                format!(
                    "ambiguous type class '{name}'; qualify it as {}",
                    classes
                        .iter()
                        .map(|class| self.spelling(class))
                        .collect::<Vec<_>>()
                        .join(" or ")
                ),
                span,
            )),
            Choice::Hidden(_) | Choice::Missing => Ok(None),
        }
    }

    /// Resolves a class name to its key in `Classes::names`, or `None` when
    /// no class has the name.
    fn class(&self, module: &str, name: &str, span: Span) -> Result<Option<&str>, Diagnostic> {
        self.check_path(module, name, span)?;
        self.class_result(self.class_choice(module, name), name, span)
    }

    /// Resolves the std-origin record or union that a builtin scheme names.
    fn std_type(
        &self,
        module: &str,
        name: &str,
        args: Box<[Type]>,
        span: Span,
    ) -> Result<Type, Diagnostic> {
        let qualified = format!("{module}.{name}");
        let std = self.modules.get(module) == Some(&ModuleOrigin::Std);
        let found = if !std {
            None
        } else if let Some(info) = self.records.get(&qualified) {
            Some((
                self.record_arities[info.id],
                Type::Record(info.id, args.clone()),
            ))
        } else {
            self.unions.get(&qualified).map(|info| {
                (
                    self.union_arities[info.id],
                    Type::Union(info.id, args.clone()),
                )
            })
        };
        match found {
            Some((arity, ty)) if arity == args.len() => Ok(ty),
            _ => Err(Diagnostic::of(
                DiagnosticKind::StdItemMissing,
                format!(
                    "this builtin needs the standard library type '{qualified}' with {} type argument(s)",
                    args.len()
                ),
                span,
            )),
        }
    }

    /// Resolves an unqualified case: the requester's own module first, then
    /// the unique visible case of user modules and then of std modules.
    fn case(&self, module: &str, name: &str, span: Span) -> Result<Option<&CaseInfo>, Diagnostic> {
        if let Some(case) = self.cases.get(&format!("{module}.{name}")) {
            return Ok(Some(case));
        }
        let candidates: Vec<&CaseInfo> = self
            .case_aliases
            .get(name)
            .into_iter()
            .flatten()
            .map(|qualified| &self.cases[qualified])
            .collect();
        match self.choose(
            module,
            &candidates,
            |case| case.info.module.as_str(),
            |case| case.info.visible_from(module),
        ) {
            Choice::Found(case, _) => Ok(Some(case)),
            Choice::Hidden(case) => Err(case.info.private_error("union case", span)),
            Choice::Missing => Ok(None),
            Choice::Ambiguous(visible, _) => Err(Diagnostic::of(
                DiagnosticKind::AmbiguousName,
                format!(
                    "ambiguous union case '{name}'; qualify it as {}",
                    visible
                        .iter()
                        .map(|case| self.spelling(&case.info.name))
                        .collect::<Vec<_>>()
                        .join(" or ")
                ),
                span,
            )),
        }
    }

    /// Resolves `Union.Case` for a union declared in `module`.
    fn union_case(
        &self,
        requester: &str,
        module: &str,
        union: &str,
        name: &str,
        span: Span,
    ) -> Result<Option<&CaseInfo>, Diagnostic> {
        if !self.searchable(requester, module) {
            return Ok(None);
        }
        let Some(info) = self.unions.get(&format!("{module}.{union}")) else {
            return Ok(None);
        };
        let case = self
            .cases
            .get(&format!("{module}.{name}"))
            .filter(|case| case.info.id == info.id)
            .ok_or_else(|| {
                Diagnostic::of(
                    DiagnosticKind::UnknownUnionCase,
                    format!("union '{}' has no case '{name}'", type_display(&info.name)),
                    span,
                )
            })?;
        case.info.require_visible("union case", requester, span)?;
        Ok(Some(case))
    }

    /// Resolves a case path: `Case`, `Module.Case`, the requester's own
    /// `Union.Case`, or `Module.Union.Case`, where `Module` may be a namespace
    /// path. A module's case comes before a case of the requester's own union
    /// that shares the module's name, which stays `Case` or `Module.Union.Case`
    /// with the requester's module. A std module comes after the requester's
    /// own declarations instead: its own `union Maybe` makes `Maybe.Some` its
    /// own case, and `std::Maybe.Some` names the std case. `None` means the
    /// path names no case, so a caller can try functions, fields, or recognizers.
    fn case_path(
        &self,
        requester: &str,
        path: &str,
        span: Span,
    ) -> Result<Option<&CaseInfo>, Diagnostic> {
        self.check_path(requester, path, span)?;
        let source = path;
        let Some(canonical) = self.canonical(requester, path) else {
            // No module starts the path, so only the requester's own `Union.Case` remains.
            return match path.split_once('.') {
                Some((union, name))
                    if !union.contains(':')
                        && !name.contains('.')
                        && self.unions.contains_key(&format!("{requester}.{union}")) =>
                {
                    self.union_case(requester, requester, union, name, span)
                }
                _ => Ok(None),
            };
        };
        let path = canonical.as_ref();
        let Some((prefix, name)) = path.rsplit_once('.') else {
            return self.case(requester, path, span);
        };
        if self.origin(requester) == ModuleOrigin::User
            && self.origin(prefix) == ModuleOrigin::Std
            && source
                .split_once('.')
                .is_some_and(|(written, member)| written == prefix && !member.contains('.'))
            && self.unions.contains_key(&format!("{requester}.{prefix}"))
        {
            return self
                .union_case(requester, requester, prefix, name, span)
                .map_err(|mut error| {
                    if self.cases.contains_key(path) {
                        error.message.push_str(&format!(
                            "; write '{}::{path}' for the standard library's case",
                            crate::stdlib::NAMESPACE
                        ));
                    }
                    error
                });
        }
        let qualified = if self.searchable(requester, prefix) {
            let case = self.cases.get(path);
            if let Some(case) = case {
                case.info.require_visible("union case", requester, span)?;
            }
            case
        } else {
            None
        };
        if qualified.is_some() {
            return Ok(qualified);
        }
        if let Some((module, union)) = prefix.rsplit_once('.') {
            match source
                .rsplit_once('.')
                .and_then(|(head, _)| head.rsplit_once('.'))
            {
                Some((written, _))
                    if module_stem(module) == union
                        && !source.starts_with(KEY_PATH)
                        && self.unions.contains_key(prefix) =>
                {
                    Err(Self::repeated_module(
                        source,
                        "union",
                        union,
                        &format!("{written}.{name}"),
                        span,
                    ))
                }
                _ => self.union_case(requester, module, union, name, span),
            }
        } else if self.unions.contains_key(&format!("{requester}.{prefix}")) {
            self.union_case(requester, requester, prefix, name, span)
        } else {
            Ok(None)
        }
    }

    fn function(
        &self,
        requester: &str,
        qualified: &str,
        span: Span,
    ) -> Result<Option<usize>, Diagnostic> {
        if !self.searchable_path(requester, qualified) {
            return Ok(None);
        }
        let Some(info) = self.functions.get(qualified) else {
            return Ok(None);
        };
        info.require_visible("name", requester, span)?;
        Ok(Some(info.id))
    }

    /// The constant that a length name `N` or `Module.N` refers to (A16 Phase 2), with its value
    /// when it is an `i64` constant with an integer literal value.
    pub(crate) fn length_constant(
        &self,
        module: &str,
        name: &str,
        span: Span,
    ) -> Option<(usize, Option<u64>)> {
        let qualified = if name.contains(['.', ':']) {
            self.canonical(module, name)?
        } else {
            std::borrow::Cow::Owned(format!("{module}.{name}"))
        };
        let id = self.function(module, &qualified, span).ok().flatten()?;
        self.constants
            .contains(&id)
            .then(|| (id, self.length_constants.get(&id).copied()))
    }

    /// Whether each parameter of a record, union, or alias is a length parameter (A16 Phase 2).
    fn length_parameters(&self, named: NamedType<'_>) -> Vec<bool> {
        match named {
            NamedType::Record(info) => self.record_lengths[info.id].to_vec(),
            NamedType::Union(info) => self.union_lengths[info.id].to_vec(),
            NamedType::Alias(info) => self.type_aliases[&info.name]
                .1
                .parameters
                .iter()
                .map(|parameter| parameter.text.starts_with('#'))
                .collect(),
            NamedType::Handle(_) => Vec::new(),
        }
    }

    fn member_function(
        &self,
        requester: &str,
        receiver: &Type,
        name: &str,
        types: &TypeContext<'_>,
        span: Span,
    ) -> Result<usize, Diagnostic> {
        let qualified_type = match receiver {
            Type::Record(id, _) => &types.records[*id].name,
            Type::Union(id, _) => &types.unions[*id].name,
            _ => {
                return Err(Diagnostic::of(
                    DiagnosticKind::FunctionConstraintUnresolvable,
                    format!(
                        "{} has no declaring module for function constraint '#{name}'",
                        receiver.display(types)
                    ),
                    span,
                ));
            }
        };
        let (module, _) = qualified_type.rsplit_once('.').unwrap();
        let qualified = format!("{module}.{name}");
        self.function(requester, &qualified, span)?.ok_or_else(|| {
            Diagnostic::of(
                DiagnosticKind::FunctionConstraintUnsatisfied,
                format!(
                    "{} does not satisfy '#{name}': module '{}' has no function '{name}'",
                    receiver.display(types),
                    self.module_display(module)
                ),
                span,
            )
        })
    }

    fn has_active_pattern(&self, requester: &str, qualified: &str) -> bool {
        self.searchable_path(requester, qualified) && self.active_patterns.contains_key(qualified)
    }

    fn active_pattern(
        &self,
        requester: &str,
        name: &str,
        span: Span,
    ) -> Result<Option<(usize, ActiveCase)>, Diagnostic> {
        if let Some((info, case)) = self.active_patterns.get(&format!("{requester}.{name}")) {
            return Ok(Some((info.id, *case)));
        }
        if name.contains(['.', ':']) {
            self.check_path(requester, name, span)?;
            if let Some(qualified) = self.canonical(requester, name)
                && self.searchable_path(requester, &qualified)
                && let Some((info, case)) = self.active_patterns.get(qualified.as_ref())
            {
                info.require_visible("active pattern", requester, span)?;
                return Ok(Some((info.id, *case)));
            }
            return Ok(None);
        }
        let candidates: Vec<_> = self
            .active_aliases
            .get(name)
            .into_iter()
            .flatten()
            .map(|qualified| &self.active_patterns[qualified])
            .collect();
        match self.choose(
            requester,
            &candidates,
            |(info, _)| info.module.as_str(),
            |(info, _)| info.visible_from(requester),
        ) {
            Choice::Found((info, case), _) => Ok(Some((info.id, *case))),
            Choice::Ambiguous(candidates, _) => Err(Diagnostic::of(
                DiagnosticKind::AmbiguousName,
                format!(
                    "ambiguous active pattern '{name}'; qualify one of {}",
                    candidates
                        .iter()
                        .map(|(info, _)| self.spelling(&info.name))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                span,
            )),
            Choice::Hidden((info, _)) => Err(info.private_error("active pattern", span)),
            Choice::Missing => Ok(None),
        }
    }
}

/// A format spec in the canonical spelling the lexer accepts, for diagnostics.
pub(crate) fn spec_text(spec: &FormatSpec) -> String {
    let mut text = String::new();
    if let Some(align) = spec.align {
        if spec.fill != ' ' {
            text.push(spec.fill);
        }
        text.push(match align {
            FormatAlign::Left => '<',
            FormatAlign::Right => '>',
            FormatAlign::Center => '^',
        });
    }
    if spec.plus {
        text.push('+');
    }
    if spec.width > 0 {
        text.push_str(&spec.width.to_string());
    }
    if let Some(precision) = spec.precision {
        text.push('.');
        text.push_str(&precision.to_string());
    }
    if let Some(kind) = spec.kind {
        text.push(match kind {
            FormatKind::LowerHex => 'x',
            FormatKind::UpperHex => 'X',
            FormatKind::Octal => 'o',
            FormatKind::Binary => 'b',
            FormatKind::Exponent => 'e',
            FormatKind::Fixed => 'f',
        });
    }
    text
}

fn display_function_name(module: &str, name: &str) -> String {
    match name
        .strip_prefix("$active.")
        .and_then(|name| name.rsplit_once('.'))
    {
        Some((case, "partial")) => format!("{module}.(|{case}|_|)"),
        Some((cases, _)) => format!("{module}.(|{}|)", cases.replace('.', "|")),
        None => format!("{module}.{name}"),
    }
}

/// Rejects private named types reachable from a public declaration, reporting
/// the span of the leaked type reference instead of the declaration name.
fn validate_public_type(
    expression: &TypeExpr,
    module: &str,
    owner: (&str, &str),
    names: &Names,
) -> Result<(), Diagnostic> {
    match &expression.kind {
        TypeExprKind::Named(name) => {
            if crate::numeric::primitive(name).is_none() {
                let info = names.named_type(module, name, expression.span)?.info();
                if info.visibility == Visibility::Private {
                    let (kind, owner) = owner;
                    return Err(Diagnostic::of(
                        DiagnosticKind::PrivateTypeInPublicApi,
                        format!(
                            "private type '{}' leaks from public {kind} '{owner}'; make the {kind} private or expose a public type",
                            names.type_spelling(&info.name)
                        ),
                        expression.span,
                    ));
                }
            }
            Ok(())
        }
        TypeExprKind::Apply(head, _) if crate::numeric::primitive(&head.text).is_some() => {
            // `resolve_type` reports arguments applied to a primitive type.
            Ok(())
        }
        TypeExprKind::Apply(head, args) => {
            let mut lengths = Vec::new();
            if !builtin_type_head(&head.text)
                && !head.text.starts_with('\'')
                && let TypeHead::Type(named) = names.type_head(module, head)?
            {
                let info = named.info();
                if info.visibility == Visibility::Private {
                    let (kind, owner) = owner;
                    return Err(Diagnostic::of(
                        DiagnosticKind::PrivateTypeInPublicApi,
                        format!(
                            "private type '{}' leaks from public {kind} '{owner}'; make the {kind} private or expose a public type",
                            names.type_spelling(&info.name)
                        ),
                        head.span,
                    ));
                }
                lengths = names.length_parameters(named);
            }
            // Length arguments (A16) name no types.
            args.iter()
                .enumerate()
                .filter(|(index, _)| !lengths.get(*index).copied().unwrap_or(false))
                .try_for_each(|(_, arg)| validate_public_type(arg, module, owner, names))
        }
        // Classes have no visibility, so a dyn type leaks nothing (A14).
        TypeExprKind::Variable(_) | TypeExprKind::Length(_) | TypeExprKind::Dyn(_) => Ok(()),
        TypeExprKind::Regions(inner, _)
        | TypeExprKind::Own(inner)
        | TypeExprKind::Mut(inner)
        | TypeExprKind::Pure(inner)
        | TypeExprKind::Quantified(_, inner)
        | TypeExprKind::Reference(inner, _)
        | TypeExprKind::Array(inner)
        | TypeExprKind::ArrayView(inner)
        | TypeExprKind::FixedArray(inner, _)
        | TypeExprKind::List(inner)
        | TypeExprKind::Task(inner) => validate_public_type(inner, module, owner, names),
        TypeExprKind::Tuple(elements) => elements
            .iter()
            .try_for_each(|element| validate_public_type(element, module, owner, names)),
        TypeExprKind::Function(parameters, result) => {
            for parameter in parameters.iter() {
                validate_public_type(parameter, module, owner, names)?;
            }
            validate_public_type(result, module, owner, names)
        }
    }
}

pub fn check_modules(modules: &[ModuleInput<'_>]) -> Result<CheckedModule, Diagnostic> {
    check_modules_all(modules).map_err(crate::first_error)
}

/// The functions that one worker takes before a stage of `check_modules_collect` uses another
/// thread, and how many a worker takes at a time.
const FUNCTIONS_PER_WORKER: usize = 64;
const FUNCTION_CHUNK: usize = 8;

pub fn check_modules_all(modules: &[ModuleInput<'_>]) -> Result<CheckedModule, DiagnosticSet> {
    check_modules_indexed_all(modules, None, Parallelism::library())
}

pub(crate) fn check_modules_indexed_all(
    modules: &[ModuleInput<'_>],
    semantic: Option<&mut semantic::SemanticIndex>,
    parallelism: Parallelism,
) -> Result<CheckedModule, DiagnosticSet> {
    check_modules_with_capture(modules, semantic, None, parallelism)
}

/// `check_modules_indexed_all` that also fills `capture`, the declaration view of the knowledge
/// graph (G26). The capture only reads the checker's state: the result and the diagnostics are
/// those of the same analysis without it.
pub(crate) fn check_modules_captured(
    modules: &[ModuleInput<'_>],
    capture: &mut kg_capture::Capture,
    parallelism: Parallelism,
) -> Result<CheckedModule, DiagnosticSet> {
    check_modules_with_capture(modules, None, Some(capture), parallelism)
}

fn check_modules_with_capture(
    modules: &[ModuleInput<'_>],
    semantic: Option<&mut semantic::SemanticIndex>,
    capture: Option<&mut kg_capture::Capture>,
    parallelism: Parallelism,
) -> Result<CheckedModule, DiagnosticSet> {
    let mut diagnostics = Diagnostics::new(0);
    match check_modules_collect(
        modules,
        &mut diagnostics,
        warnings::WarningOptions::default(),
        semantic,
        capture,
        parallelism,
    ) {
        Ok(module) => Ok(module),
        Err(error) => {
            diagnostics.push(error);
            Err(diagnostics.finish())
        }
    }
}

fn check_modules_collect(
    modules: &[ModuleInput<'_>],
    diagnostics: &mut Diagnostics,
    warning_options: warnings::WarningOptions,
    mut semantic: Option<&mut semantic::SemanticIndex>,
    mut capture: Option<&mut kg_capture::Capture>,
    parallelism: Parallelism,
) -> Result<CheckedModule, Diagnostic> {
    let check_time = crate::timings::Phase::start("check");
    // A `@deprecated` declaration needs the references that the semantic index records (W1005),
    // so a project without one never collects them unless the caller asked for the index.
    let deprecations = warnings::deprecations(modules);
    let indexing = semantic.is_some() || capture.is_some() || !deprecations.is_empty();
    let mut names = Names {
        warning_options,
        ..Names::default()
    };
    let module_editions: BTreeMap<_, _> = modules
        .iter()
        .map(|input| (input.name, input.edition))
        .collect();
    names.borrowing_modules = module_editions
        .iter()
        .filter(|(_, edition)| edition.borrows_parameters())
        .map(|(module, _)| (*module).to_owned())
        .collect();
    let mut sources: BTreeMap<&str, (usize, ModuleOrigin)> = BTreeMap::new();
    for (source, module) in modules.iter().enumerate() {
        if diagnostics.is_full() {
            break;
        }
        let name = module.name;
        let reserved = |at: usize| {
            let message = match &module.program.namespace {
                Some(declared) if module.origin == ModuleOrigin::User && name.contains('.') => {
                    format!(
                        "namespace '{}' puts module '{}' under '{}', a name reserved for the standard library; choose another namespace",
                        declared.path.text,
                        name.rsplit('.').next().unwrap_or(name),
                        name.split('.').next().unwrap_or(name)
                    )
                }
                _ => format!(
                    "module name '{name}' is reserved for the standard library; rename the file"
                ),
            };
            Diagnostic::of(
                DiagnosticKind::ReservedStdNamespace,
                message,
                Span::default().in_source(at),
            )
        };
        if name.len() > 255 || name.split('.').count() > 16 {
            diagnostics.push(Diagnostic::of(
                DiagnosticKind::ModulePathLimit,
                "module paths are limited to 16 segments and 255 bytes",
                Span::default().in_source(source),
            ));
            continue;
        }
        if module.origin == ModuleOrigin::User
            && crate::stdlib::is_reserved_module(name.split('.').next().unwrap_or(name))
        {
            diagnostics.push(reserved(source));
            continue;
        }
        let valid = name.split('.').all(|name| {
            crate::lexer::lex(name, module.edition).is_ok_and(|tokens| {
                matches!(
                    tokens.as_slice(),
                    [Token { kind: TokenKind::Ident(text), .. }, Token { kind: TokenKind::End, .. }]
                        if text == name && name != "_" && name != "Task"
                )
            })
        });
        let stem = name.rsplit('.').next().unwrap_or(name);
        if valid
            && module.origin == ModuleOrigin::User
            && !stem.starts_with(|first: char| first.is_ascii_uppercase())
        {
            let suggestion = stem
                .chars()
                .next()
                .filter(char::is_ascii_lowercase)
                .map(|first| {
                    format!(
                        ", for example to '{}{}'",
                        first.to_ascii_uppercase(),
                        &stem[1..]
                    )
                })
                .unwrap_or_default();
            diagnostics.push(Diagnostic::of(DiagnosticKind::InvalidModuleName,
                format!(
                    "module name '{stem}' must start with an uppercase ASCII letter; rename the file{suggestion}"
                ),
                Span::default().in_source(source),
            ));
            continue;
        }
        let full = if module.namespace.is_empty() {
            name.to_owned()
        } else {
            format!("{}.{stem}", module.namespace)
        };
        let span = module
            .program
            .namespace
            .as_ref()
            .map_or(Span::default(), |declared| declared.path.span);
        if module.origin == ModuleOrigin::User
            && full.split('.').next() == Some(crate::stdlib::NAMESPACE)
        {
            diagnostics.push(Diagnostic::of(DiagnosticKind::ReservedStdNamespace,
                format!(
                    "module '{}' is in the namespace 'std', which is reserved for the standard library; choose another namespace",
                    namespace_display(&full)
                ),
                span.in_source(source),
            ));
            continue;
        }
        if valid && names.full_modules.contains_key(&full) {
            diagnostics.push(Diagnostic::of(DiagnosticKind::DuplicateModule,
                format!(
                    "module '{}' is declared by more than one file; rename a file or change its namespace",
                    namespace_display(&full)
                ),
                span.in_source(source),
            ));
            continue;
        }
        let previous = sources.insert(name, (source, module.origin));
        if let Some((previous, origin)) = previous
            && (origin == ModuleOrigin::Std || module.origin == ModuleOrigin::Std)
        {
            // A user module that collides with a std module is reported at the user file.
            let at = if module.origin == ModuleOrigin::User {
                source
            } else {
                previous
            };
            diagnostics.push(reserved(at));
            continue;
        }
        if !valid || previous.is_some() {
            diagnostics.push(Diagnostic::of(DiagnosticKind::InvalidModuleName,
                format!(
                    "invalid or duplicate module name '{name}'; each .tz, .tt, or .tc filename must have a unique ASCII identifier stem, not '_', 'Task', or a keyword"
                ),
                Span::default().in_source(source),
            ));
            continue;
        }
        names.modules.insert(name.to_owned(), module.origin);
        names.full_modules.insert(full, name.to_owned());
        names
            .module_namespaces
            .insert(name.to_owned(), module.namespace.to_owned());
    }
    diagnostics.check()?;
    for (source, module) in modules.iter().enumerate() {
        let mut imported: Vec<String> = Vec::new();
        for using in &module.program.usings {
            let path = &using.path.text;
            let message = match names.namespace_path(module.name, path) {
                Some(namespace) if !imported.contains(&namespace) => {
                    imported.push(namespace);
                    continue;
                }
                Some(namespace) => format!(
                    "duplicate using '{}'; remove one of them",
                    namespace_display(&namespace)
                ),
                None if names.module_path(module.name, path).is_some() => format!(
                    "'{path}' is a module, not a namespace; using takes the namespace that holds modules, and a module's members are reached as '{path}.name'"
                ),
                None => format!(
                    "using '{path}' names no namespace with modules; check the spelling or the namespace declarations"
                ),
            };
            diagnostics.push(Diagnostic::of(
                DiagnosticKind::InvalidUsingDeclaration,
                message,
                using.path.span.in_source(source),
            ));
        }
        if !imported.is_empty() {
            names.module_usings.insert(module.name.to_owned(), imported);
        }
    }
    diagnostics.check()?;
    (names.builders, names.builder_aliases) = computation::collect_all(modules, diagnostics);
    diagnostics.check()?;
    let record_declarations: Vec<_> = modules
        .iter()
        .flat_map(|module| {
            module
                .program
                .records
                .iter()
                .map(|record| (module.name, record))
        })
        .collect();
    let mut function_declarations: Vec<_> = modules
        .iter()
        .flat_map(|module| {
            module
                .program
                .functions
                .iter()
                .map(|function| (module.name.to_owned(), function.clone()))
        })
        .collect();
    // F08 Phase 3: the `@cpu` levels of each function declaration.
    let mut cpu_levels = BTreeMap::new();
    for module in modules {
        for attribute in &module.program.cpu_attributes {
            if let Some(id) = function_declarations.iter().position(|(owner, function)| {
                owner == module.name && function.name.text == attribute.function.text
            }) {
                cpu_levels.insert(id, attribute.levels);
            }
        }
    }
    // The names that a const initializer may apply: `const def`s, and union cases to build the
    // arguments of a call (D11).
    let const_names: BTreeSet<String> = modules
        .iter()
        .flat_map(|module| {
            let functions = module
                .program
                .functions
                .iter()
                .filter(|function| function.constant)
                .map(|function| function.name.text.clone());
            let cases = module
                .program
                .unions
                .iter()
                .flat_map(|union| &union.cases)
                .map(|case| case.name.text.clone());
            functions.chain(cases)
        })
        .collect();
    for module in modules {
        for constant in &module.program.constants {
            constants::validate(&constant.value, &const_names)?;
            // An `i64` constant with an integer literal value can name a length (A16 Phase 2).
            if let (TypeExprKind::Named(ty), ExprKind::Integer(value, suffix)) =
                (&constant.ty.kind, &constant.value.kind)
                && ty == "i64"
                && suffix.as_deref().is_none_or(|suffix| suffix == "i64")
            {
                names.length_constants.insert(
                    function_declarations.len(),
                    u64::try_from(*value).unwrap_or(u64::MAX),
                );
            }
            names.constants.insert(function_declarations.len());
            let mut name = constant.name.clone();
            name.provenance = Provenance::Generated;
            function_declarations.push((
                module.name.to_owned(),
                FunctionDecl {
                    doc: None,
                    name,
                    regions: Vec::new(),
                    recursion: None,
                    visibility: constant.visibility,
                    exported: false,
                    constant: false,
                    pure: false,
                    parameters: Vec::new(),
                    result: constant.ty.clone(),
                    constraints: Vec::new(),
                    body: constant.value.clone(),
                },
            ));
        }
    }
    let mut external_functions = BTreeMap::new();
    let mut external_symbols = BTreeSet::new();
    for module in modules {
        for external in &module.program.externs {
            for ty in external.parameters.iter().chain([&external.result]) {
                regions::reject_local(ty)?;
            }
            let import = if let Some(link) = &external.link {
                if let Some(error) = crate::abi::link_name_error(link) {
                    return Err(error);
                }
                HostImport {
                    native_symbol: link.symbol.clone(),
                    wasm_module: link
                        .module
                        .as_ref()
                        .map_or_else(|| "tsuzuri".to_owned(), |(module, _)| module.clone()),
                    wasm_name: link.symbol.clone(),
                    explicit: true,
                    callbacks: false,
                }
            } else {
                let native_symbol = format!(
                    "tsuzuri_host_{}_{}",
                    module.name.replace('.', "_"),
                    external.name.text
                );
                if !external_symbols.insert(native_symbol.clone()) {
                    return Err(duplicate(&external.name));
                }
                HostImport {
                    native_symbol,
                    wasm_module: "tsuzuri".to_owned(),
                    wasm_name: format!("{}.{}", module.name, external.name.text),
                    explicit: false,
                    callbacks: false,
                }
            };
            if !external.constraints.is_empty() || !external.regions.is_empty() {
                return Err(Diagnostic::of(
                    DiagnosticKind::ExternDeclarationConstraints,
                    "extern declarations cannot have constraints or named regions",
                    external.name.span,
                ));
            }
            external_functions.insert(function_declarations.len(), import);
            function_declarations.push((
                module.name.to_owned(),
                FunctionDecl {
                    doc: None,
                    name: external.name.clone(),
                    regions: Vec::new(),
                    recursion: None,
                    visibility: external.visibility,
                    exported: false,
                    constant: false,
                    pure: false,
                    parameters: external
                        .parameters
                        .iter()
                        .enumerate()
                        .map(|(index, ty)| Parameter {
                            name: Ident {
                                text: format!("$host_arg{index}"),
                                span: ty.span,
                                provenance: Provenance::Generated,
                            },
                            mutable: false,
                            ty: ty.clone(),
                            json: None,
                        })
                        .collect(),
                    result: external.result.clone(),
                    constraints: Vec::new(),
                    body: Expr {
                        kind: ExprKind::Unit,
                        span: external.name.span,
                        depth: 1,
                    },
                },
            ));
        }
    }
    let mut tests = Vec::new();
    let mut test_functions = BTreeMap::new();
    for module in modules {
        for test in &module.program.tests {
            if module.origin == ModuleOrigin::Std {
                diagnostics.push(Diagnostic::of(
                    DiagnosticKind::StdDeclaresTest,
                    "embedded standard-library sources cannot declare tests",
                    test.name_span,
                ));
                continue;
            }
            let index = tests.len();
            let function = function_declarations.len();
            test_functions.insert(function, index);
            tests.push(CheckedTest {
                module: namespace_display(module.name),
                name: test.name.clone(),
                index,
                function,
                span: test.name_span,
            });
            function_declarations.push((
                module.name.into(),
                FunctionDecl {
                    doc: None,
                    name: Ident {
                        text: format!("$test.{index}"),
                        span: test.name_span,
                        provenance: Provenance::Generated,
                    },
                    regions: Vec::new(),
                    recursion: None,
                    visibility: Visibility::Private,
                    exported: false,
                    constant: false,
                    pure: false,
                    parameters: Vec::new(),
                    result: TypeExpr {
                        kind: TypeExprKind::Named("unit".into()),
                        span: test.body.span,
                    },
                    constraints: Vec::new(),
                    body: test.body.clone(),
                },
            ));
        }
    }
    let mut benches = Vec::new();
    let mut bench_functions = BTreeMap::new();
    for module in modules {
        for bench in &module.program.benches {
            if module.origin == ModuleOrigin::Std {
                diagnostics.push(Diagnostic::of(
                    DiagnosticKind::StdDeclaresTest,
                    "embedded standard-library sources cannot declare benchmarks",
                    bench.name_span,
                ));
                continue;
            }
            let index = benches.len();
            let function = function_declarations.len();
            bench_functions.insert(function, index);
            benches.push(CheckedBench {
                module: namespace_display(module.name),
                name: bench.name.clone(),
                index,
                function,
                span: bench.name_span,
            });
            function_declarations.push((module.name.into(), bench_function(bench, index)));
        }
    }
    for (id, (module, record)) in record_declarations.iter().enumerate() {
        if diagnostics.is_full() {
            break;
        }
        let qualified = format!("{module}.{}", record.name.text);
        let info = NameInfo {
            id,
            name: qualified.clone(),
            module: (*module).to_owned(),
            visibility: record.visibility,
        };
        if reserved_type_name(&record.name.text)
            || names.records.insert(qualified.clone(), info).is_some()
        {
            let first = record_declarations[..id]
                .iter()
                .filter(|_| !reserved_type_name(&record.name.text))
                .find(|(other_module, other)| {
                    other_module == module && other.name.text == record.name.text
                })
                .map(|(_, other)| other.name.span);
            diagnostics.push(duplicate_of(&record.name, first));
            continue;
        }
        names
            .record_aliases
            .entry(record.name.text.clone())
            .or_default()
            .push(qualified);
        names.record_arities.push(record.parameters.len());
        names.record_lengths.push(
            record
                .parameters
                .iter()
                .map(|parameter| parameter.text.starts_with('#'))
                .collect(),
        );
    }
    for module in modules {
        for handle in &module.program.extern_types {
            if diagnostics.is_full() {
                break;
            }
            let qualified = format!("{}.{}", module.name, handle.name.text);
            let info = NameInfo {
                id: names.handles.len(),
                name: qualified.clone(),
                module: module.name.to_owned(),
                visibility: handle.visibility,
            };
            if reserved_type_name(&handle.name.text)
                || names.records.contains_key(&qualified)
                || names.handles.insert(qualified.clone(), info).is_some()
            {
                diagnostics.push(duplicate(&handle.name));
                continue;
            }
            names
                .handle_aliases
                .entry(handle.name.text.clone())
                .or_default()
                .push(qualified);
        }
    }
    diagnostics.check()?;
    let union_declarations: Vec<_> = modules
        .iter()
        .flat_map(|module| {
            module
                .program
                .unions
                .iter()
                .map(|union| (module.name, union))
        })
        .collect();
    collect_unions(&union_declarations, &mut names, diagnostics)?;
    // Classes, records, and unions share one type namespace, so `Name<'a>` always has one meaning.
    names.classes.extend(
        polymorph::BUILTIN_CLASSES
            .iter()
            .map(|name| (*name).to_owned()),
    );
    for module in modules {
        for class in &module.program.classes {
            let qualified = format!("{}.{}", module.name, class.name.text);
            if names.records.contains_key(&qualified)
                || names.unions.contains_key(&qualified)
                || names.handles.contains_key(&qualified)
                || names.classes.contains(&qualified)
                || polymorph::BUILTIN_CLASSES.contains(&class.name.text.as_str())
                || matches!(
                    class.name.text.as_str(),
                    "_" | "Array" | "Task" | "Vec" | "Rc" | "Arc"
                )
            {
                diagnostics.push(duplicate(&class.name));
                continue;
            }
            names.classes.insert(qualified.clone());
            names
                .class_aliases
                .entry(class.name.text.clone())
                .or_default()
                .push(qualified);
        }
    }
    diagnostics.check()?;
    for module in modules {
        for alias in &module.program.type_aliases {
            if diagnostics.is_full() {
                break;
            }
            let qualified = format!("{}.{}", module.name, alias.name.text);
            if reserved_type_name(&alias.name.text)
                || names.records.contains_key(&qualified)
                || names.unions.contains_key(&qualified)
                || names.handles.contains_key(&qualified)
                || names.cases.contains_key(&qualified)
                || names.classes.contains(&qualified)
                || names.type_aliases.contains_key(&qualified)
            {
                diagnostics.push(duplicate(&alias.name));
                continue;
            }
            if !starts_uppercase(&alias.name.text) {
                diagnostics.push(Diagnostic::of(
                    DiagnosticKind::TypeNameCase,
                    "a type alias name must start with an uppercase ASCII letter",
                    alias.name.span,
                ));
                continue;
            }
            names.type_aliases.insert(
                qualified.clone(),
                (
                    NameInfo {
                        id: names.type_aliases.len(),
                        name: qualified.clone(),
                        module: module.name.to_owned(),
                        visibility: alias.visibility,
                    },
                    alias.clone(),
                ),
            );
            names
                .type_alias_names
                .entry(alias.name.text.clone())
                .or_default()
                .push(qualified);
        }
    }
    diagnostics.check()?;
    for (info, alias) in names.type_aliases.values() {
        if diagnostics.is_full() {
            break;
        }
        let checked = (|| {
            let parameters =
                declared_parameters("type alias", &alias.name.text, &alias.parameters)?;
            let ty = resolve_type(&alias.target, &info.module, &names)?;
            polymorph::bounded_type(&ty, alias.target.span)?;
            let used = polymorph::variables(&ty);
            if let Some(variable) = used.iter().find(|variable| !parameters.contains(variable)) {
                if let Some(length) = variable.strip_prefix('#') {
                    return Err(undeclared_length(
                        "type alias",
                        &alias.name.text,
                        length,
                        alias.target.span,
                    ));
                }
                return Err(Diagnostic::of(
                    DiagnosticKind::UndeclaredTypeVariable,
                    format!(
                        "type variable '{variable} is not declared by type alias '{}'; add it to the alias parameters",
                        alias.name.text
                    ),
                    alias.target.span,
                ));
            }
            if let Some(parameter) = alias
                .parameters
                .iter()
                .find(|parameter| !used.contains(&parameter.text))
            {
                return Err(Diagnostic::of(
                    DiagnosticKind::UnusedTypeParameter,
                    format!(
                        "{} is not used by the alias target; remove it or use it in the target",
                        parameter_name(&parameter.text)
                    ),
                    parameter.span,
                ));
            }
            if alias.visibility == Visibility::Public {
                validate_public_type(
                    &alias.target,
                    &info.module,
                    ("type alias", &names.type_spelling(&info.name)),
                    &names,
                )?;
            }
            Ok(())
        })();
        if let Err(error) = checked {
            diagnostics.push(error);
        }
    }
    diagnostics.check()?;
    let mut records = Vec::new();
    for (module, record) in &record_declarations {
        if diagnostics.is_full() {
            break;
        }
        let checked = (|| {
            let qualified = format!("{module}.{}", record.name.text);
            // Opaque std records may carry phantom type parameters, as `Arena.Handle<'a>` does (C10).
            let opaque = names.origin(module) == ModuleOrigin::Std
                && crate::stdlib::opaque_record(&qualified);
            let parameters = declared_parameters("record", &record.name.text, &record.parameters)?;
            let regions::RecordRegions {
                count: region_count,
                fields: field_regions,
                targets: field_targets,
            } = regions::field_regions(record)?;
            let mut used = BTreeSet::new();
            let mut field_names = BTreeSet::new();
            let mut fields = Vec::new();
            for field in &record.fields {
                if !field_names.insert(&field.name.text) || field.name.text == "_" {
                    // The earlier fields only; `_` is reserved and has no first definition.
                    let first = record
                        .fields
                        .iter()
                        .take_while(|other| other.name.span != field.name.span)
                        .filter(|_| field.name.text != "_")
                        .find(|other| other.name.text == field.name.text);
                    return Err(duplicate_of(
                        &field.name,
                        first.map(|other| other.name.span),
                    ));
                }
                reject_field_constraints(&field.ty, module, &names, "record")?;
                let ty = resolve_type(&field.ty, module, &names)?;
                if record.visibility == Visibility::Public && !opaque {
                    validate_public_type(&field.ty, module, ("record", &qualified), &names)?;
                }
                polymorph::bounded_type(&ty, field.ty.span)?;
                for variable in polymorph::variables(&ty) {
                    if !parameters.contains(&variable) {
                        if let Some(length) = variable.strip_prefix('#') {
                            return Err(undeclared_length(
                                "record",
                                &record.name.text,
                                length,
                                field.ty.span,
                            ));
                        }
                        return Err(Diagnostic::of(
                            DiagnosticKind::UndeclaredTypeVariable,
                            format!(
                                "type variable '{variable} is not declared by record '{}'; add it in angle brackets after the record name, as in 'record {}<'{variable}> {{ ... }}'",
                                record.name.text, record.name.text
                            ),
                            field.ty.span,
                        ));
                    }
                    used.insert(variable);
                }
                if field.mutable {
                    return Err(Diagnostic::of(
                        DiagnosticKind::MutableRecordField,
                        "record fields are immutable; use an owned update instead of a mutable field",
                        field.name.span,
                    ));
                }
                fields.push((field.name.text.clone(), ty));
            }
            if let Some(parameter) = record
                .parameters
                .iter()
                .find(|parameter| !opaque && !used.contains(&parameter.text))
            {
                return Err(Diagnostic::of(
                    DiagnosticKind::UnusedTypeParameter,
                    format!(
                        "{} is not used by any field; remove it or add a field that mentions it",
                        parameter_name(&parameter.text)
                    ),
                    parameter.span,
                ));
            }
            Ok(CheckedRecord {
                name: qualified,
                visibility: record.visibility,
                origin: names.origin(module),
                parameters,
                fields,
                span: record.name.span,
                user_drop: false,
                region_count,
                field_regions,
                exclusive_regions: 0,
                region_targets: vec![0; if region_count >= 2 { region_count } else { 0 }],
                field_targets,
                size: None,
                recursive: Default::default(),
            })
        })();
        match checked {
            Ok(record) => records.push(record),
            Err(error) => diagnostics.push(error),
        }
    }
    let mut unions = Vec::new();
    for (module, union) in &union_declarations {
        if diagnostics.is_full() {
            break;
        }
        match check_union(module, union, &names) {
            Ok(union) => unions.push(union),
            Err(error) => diagnostics.push(error),
        }
    }
    diagnostics.check()?;
    mark_exclusive_regions(&mut records);
    // Generic declarations are measured with their own parameters as opaque
    // leaves, which rejects every recursive layout before any instance exists.
    let mut layouts = Layouts::new(TypeContext {
        records: &records,
        unions: &unions,
    });
    for (id, record) in records.iter().enumerate() {
        let parameters = record
            .parameters
            .iter()
            .cloned()
            .map(Type::Variable)
            .collect();
        if let Err(error) = layouts.size(&Type::Record(id, parameters), 0, record.span) {
            diagnostics.push(error);
            layouts.visiting.clear();
        }
        if diagnostics.is_full() {
            break;
        }
    }
    for (id, union) in unions.iter().enumerate() {
        let parameters = union
            .parameters
            .iter()
            .cloned()
            .map(Type::Variable)
            .collect();
        if let Err(error) = layouts.size(&Type::Union(id, parameters), 0, union.span) {
            diagnostics.push(error);
            layouts.visiting.clear();
        }
        if diagnostics.is_full() {
            break;
        }
    }
    diagnostics.check()?;
    let record_sizes: Vec<_> = (0..records.len())
        .map(|id| {
            layouts
                .sizes
                .get(&Type::Record(id, Box::default()))
                .copied()
        })
        .collect();
    let union_sizes: Vec<_> = (0..unions.len())
        .map(|id| layouts.sizes.get(&Type::Union(id, Box::default())).copied())
        .collect();
    for (record, size) in records.iter_mut().zip(record_sizes) {
        record.size = size;
    }
    for (union, size) in unions.iter_mut().zip(union_sizes) {
        union.size = size;
    }
    let types = TypeContext {
        records: &records,
        unions: &unions,
    };
    for (record, (_, declaration)) in records.iter().zip(&record_declarations) {
        for (index, (_, ty)) in record.fields.iter().enumerate() {
            let span = declaration.fields[index].ty.span;
            if let Err(error) = validate_exclusive_field(record, index, &types, span) {
                diagnostics.push(error);
                continue;
            }
            if let Err(error) = validate_size(ty, &types, record.span) {
                diagnostics.push(error);
            }
        }
        if diagnostics.is_full() {
            break;
        }
    }
    for union in &unions {
        for ty in union.cases.iter().filter_map(|(_, ty)| ty.as_ref()) {
            if let Err(error) = validate_size(ty, &types, union.span) {
                diagnostics.push(error);
            }
        }
        if diagnostics.is_full() {
            break;
        }
    }
    diagnostics.check()?;
    let mut export_names = BTreeSet::new();
    // The function type and WASM module of each explicit extern symbol; a symbol may be declared again only identically.
    let mut explicit_symbols: BTreeMap<String, (Type, String)> = BTreeMap::new();
    regions::validate_modules(modules, &names, types)?;
    let mut classes = Classes::collect(modules, &names, &types, diagnostics)?;
    classes.instances(
        modules,
        &names,
        &types,
        &mut function_declarations,
        diagnostics,
    )?;
    for head in classes.drop_heads() {
        match head {
            Type::Record(id, _) => records[*id].user_drop = true,
            Type::Union(id, _) => unions[*id].user_drop = true,
            _ => unreachable!("Drop instances are validated to name records and unions"),
        }
    }
    let types = TypeContext {
        records: &records,
        unions: &unions,
    };
    classes.check_superclasses(&types, diagnostics)?;
    let mut signatures = Vec::new();
    names.declared = vec![DeclaredFunction::default(); function_declarations.len()];
    let mut default_reads = BTreeMap::new();
    for (id, (module, function)) in function_declarations.iter().enumerate() {
        if diagnostics.is_full() {
            break;
        }
        if test_functions.contains_key(&id) || bench_functions.contains_key(&id) {
            continue;
        }
        names.declared[id] = DeclaredFunction {
            name: function.name.text.clone(),
            parameters: function
                .parameters
                .iter()
                .map(|parameter| parameter.ty.span)
                .collect(),
        };
        let qualified = format!("{module}.{}", function.name.text);
        let module_symbol = names.symbols.intern(module);
        let name_symbol = names.symbols.intern(&function.name.text);
        names.function_ids.insert((module_symbol, name_symbol), id);
        let info = NameInfo {
            id,
            name: qualified.clone(),
            module: module.clone(),
            visibility: function.visibility,
        };
        if function.constant {
            names.const_functions.insert(id);
        }
        if function.pure {
            names.pure_functions.insert(id);
        }
        if function.name.text == "_"
            || Builtin::ALL
                .iter()
                .any(|builtin| builtin.name() == function.name.text || builtin.name() == qualified)
            || names.cases.contains_key(&qualified)
            || names.functions.insert(qualified.clone(), info).is_some()
        {
            diagnostics.push(duplicate(&function.name));
        }
    }
    diagnostics.check()?;
    let entry_module = modules
        .iter()
        .find(|module| {
            module.origin == ModuleOrigin::User
                && module.entry
                && matches!(module.program.source_kind, None | Some(SourceKind::Code))
        })
        .map(|module| module.name);
    for (id, (module, function)) in function_declarations.iter().enumerate() {
        if diagnostics.is_full() {
            diagnostics.check()?;
        }
        let checked = (|| {
            // Instance methods are reached only through class dispatch, so an
            // instance for a private type does not publish that type.
            let public = function.visibility == Visibility::Public
                && !function.name.text.starts_with("$instance.");
            let owner = display_function_name(module, &function.name.text);
            if public {
                for ty in function
                    .parameters
                    .iter()
                    .map(|parameter| &parameter.ty)
                    .chain(std::iter::once(&function.result))
                    .chain(function.constraints.iter().map(|constraint| &constraint.ty))
                {
                    validate_public_type(ty, module, ("function", &owner), &names)?;
                }
            }
            if function.exported && names.origin(module) == ModuleOrigin::Std {
                return Err(Diagnostic::of(
                    DiagnosticKind::StdExportsFunction,
                    "the standard library cannot export functions; std modules add no C/WASM exports",
                    function.name.span,
                ));
            }
            if function.exported && function.name.text.starts_with("$active.") {
                return Err(Diagnostic::of(
                    DiagnosticKind::ExportActiveRecognizer,
                    "active recognizers cannot be exported; expose an ordinary wrapper",
                    function.name.span,
                ));
            }
            if function.exported && !export_names.insert(&function.name.text) {
                return Err(Diagnostic::of(
                    DiagnosticKind::DuplicateExportName,
                    format!(
                        "duplicate export 'tz_{}'; C/WASM export names must be unique across modules",
                        function.name.text
                    ),
                    function.name.span,
                ));
            }
            let mut parameter_names = BTreeSet::new();
            let mut parameters = Vec::new();
            let mut reads = Vec::new();
            let borrow_defaults = module_editions
                .get(module.as_str())
                .is_some_and(|edition| edition.borrows_parameters())
                && !external_functions.contains_key(&id)
                && !names.const_functions.contains(&id)
                && !function.pure
                && !function.name.text.starts_with("$instance.")
                && !(entry_module == Some(module.as_str()) && function.name.text == "main");
            let kinds = classes.constraint_kinds(&function.constraints, module, &names)?;
            for parameter in &function.parameters {
                if parameter.name.text != "_" && !parameter_names.insert(&parameter.name.text) {
                    let first = function
                        .parameters
                        .iter()
                        .find(|other| other.name.text == parameter.name.text);
                    return Err(duplicate_of(
                        &parameter.name,
                        first.map(|other| other.name.span),
                    ));
                }
                let mutable = matches!(parameter.ty.kind, TypeExprKind::Mut(_));
                if mutable
                    && !function.name.text.starts_with("$instance.")
                    && !module_editions
                        .get(module.as_str())
                        .is_some_and(|edition| edition.borrows_parameters())
                {
                    return Err(Diagnostic::of(
                        DiagnosticKind::MutParameterNeedsEdition2027,
                        "'mut T' parameter conventions require edition 2027",
                        parameter.ty.span,
                    ));
                }
                let expression = match &parameter.ty.kind {
                    TypeExprKind::Own(inner) | TypeExprKind::Mut(inner) => inner.as_ref(),
                    _ => &parameter.ty,
                };
                let ty = resolve_type_with_kinds(expression, module, &names, &kinds)?;
                validate_size(&ty, &types, parameter.ty.span)?;
                let owned = matches!(parameter.ty.kind, TypeExprKind::Own(_));
                if function.pure {
                    pure_rules::check_parameter(parameter, &ty, owned, &types)?;
                }
                let read = (borrow_defaults
                    && !parameter.mutable
                    && !matches!(
                        parameter.ty.kind,
                        TypeExprKind::Own(_) | TypeExprKind::Mut(_)
                    )
                    && !regions::has_regions(&parameter.ty)
                    && read_by_default(&ty, &types))
                    || (function.pure && pure_rules::borrow_parameter(&ty, owned, &types));
                let ty = if read || mutable {
                    Type::Reference(Box::new(ty), mutable)
                } else {
                    ty
                };
                parameters.push(ty);
                reads.push(read || classes.reads_parameter(&function.name.text, reads.len()));
            }
            let result = resolve_type_with_kinds(&function.result, module, &names, &kinds)?;
            validate_size(&result, &types, function.result.span)?;
            let public_type = Type::function(parameters.clone(), result.clone());
            let Type::Function(public_parameters, public_result) = &public_type else {
                unreachable!()
            };
            if function.exported
                && (public_parameters
                    .iter()
                    .any(|ty| !crate::abi::parameter(ty, &types))
                    || !crate::abi::result(public_result, &types))
            {
                return Err(Diagnostic::of(
                    DiagnosticKind::UnsupportedExportSignature,
                    "exports support scalar values, borrowed i64/f64/ubyte arrays and string/utf8string inputs, owned buffer results, and scalar-only records (scalars, scalar fixed-length arrays, and such records as fields) with distinct C field names; mutable borrows, owned buffer inputs, wide numbers, and other aggregates are not supported",
                    function.name.span,
                ));
            }
            let errors = try_errors(
                function,
                &parameters,
                &result,
                module,
                &names,
                &classes,
                &kinds,
            )?;
            let result = polymorph::substitute(&result, &errors);
            let signature = Signature { parameters, result };
            if entry_module == Some(module.as_str())
                && function.name.text == "main"
                && !names.constants.contains(&id)
                && !entry_signature(&signature)
            {
                return Err(Diagnostic::of(
                    DiagnosticKind::InvalidMainSignature,
                    "the entry point must be 'def main :: unit -> i32' or 'def main :: Array<string> -> i32'; main returns the process exit code",
                    function.name.span,
                ));
            }
            polymorph::bounded_type(&signature.as_type(), function.name.span)?;
            let variables = polymorph::variables(&signature.as_type());
            if external_functions.contains_key(&id) {
                if !variables.is_empty() {
                    return Err(Diagnostic::of(
                        DiagnosticKind::ExternSignatureNotConcrete,
                        "extern signatures must be concrete; provide a scalar ABI type",
                        function.name.span,
                    ));
                }
                if signature
                    .parameters
                    .iter()
                    .any(|ty| matches!(ty, Type::Function(..)) && !crate::abi::callback(ty))
                {
                    return Err(Diagnostic::of(
                        DiagnosticKind::UnsupportedExternSignature,
                        "extern callback parameters support functions over scalar, unit and extern handle types; buffers, records and nested functions are not supported",
                        function.name.span,
                    ));
                }
                if signature.parameters.iter().any(|ty| {
                    !crate::abi::parameter(ty, &types)
                        && *ty != Type::Unit
                        && !matches!(ty, Type::Function(..))
                }) || !crate::abi::result(&signature.result, &types)
                {
                    return Err(Diagnostic::of(
                        DiagnosticKind::UnsupportedExternSignature,
                        "extern signatures support scalar/unit, shared ABI buffers and scalar records, and owned ABI buffer or scalar record results",
                        function.name.span,
                    ));
                }
                let import = &external_functions[&id];
                if import.explicit {
                    let declared = (signature.as_type(), import.wasm_module.clone());
                    let known = explicit_symbols
                        .entry(import.native_symbol.clone())
                        .or_insert_with(|| declared.clone());
                    if *known != declared {
                        return Err(Diagnostic::of(
                            DiagnosticKind::ConflictingExternDeclaration,
                            format!(
                                "extern symbol '{}' is declared with different signatures or import modules; declare it once and call it from other modules",
                                import.native_symbol
                            ),
                            function.name.span,
                        ));
                    }
                }
            }
            if names.constants.contains(&id) && !variables.is_empty() {
                return Err(constants::failure(
                    DiagnosticKind::ConstantTypeNotConcrete,
                    function.result.span,
                    "constant types must be concrete; remove type variables",
                ));
            }
            if names.const_functions.contains(&id) {
                constants::check_signature(function, &signature, &variables, &types)?;
            }
            let mut constraints = Vec::new();
            let mut members = Vec::new();
            for constraint in &function.constraints {
                let ty = classes.constraint_type(constraint, module, &names, &kinds)?;
                if matches!(&ty, Type::Variable(variable) if errors.contains_key(variable)) {
                    // `@'E : Err` on the caught exception's type, which implements Err.
                    continue;
                }
                if polymorph::variables(&ty)
                    .iter()
                    .any(|variable| !variables.contains(variable))
                {
                    return Err(Diagnostic::of(
                        DiagnosticKind::TypeVariableAbsentFromSignature,
                        "constraint mentions a type variable absent from the signature",
                        constraint.ty.span,
                    ));
                }
                match &constraint.name {
                    ConstraintName::Class(name) => constraints.push(Constraint {
                        class: classes.resolve(&names, module, name)?,
                        ty,
                        span: name.span,
                    }),
                    ConstraintName::Function(name, annotation) => {
                        let signature = annotation
                            .as_ref()
                            .map(|annotation| {
                                let ty =
                                    resolve_type_with_kinds(annotation, module, &names, &kinds)?;
                                if polymorph::variables(&ty)
                                    .iter()
                                    .any(|variable| !variables.contains(variable))
                                {
                                    return Err(Diagnostic::of(DiagnosticKind::TypeVariableAbsentFromSignature,
                                        "the function constraint's type mentions a type variable absent from the signature",
                                        annotation.span,
                                    ));
                                }
                                Ok(ty)
                            })
                            .transpose()?;
                        members.push(polymorph::MemberConstraint {
                            receiver: ty,
                            name: name.text.clone(),
                            module: module.clone(),
                            signature,
                            declared: true,
                            span: name.span,
                        })
                    }
                }
            }
            for ty in function
                .parameters
                .iter()
                .map(|parameter| &parameter.ty)
                .chain(std::iter::once(&function.result))
            {
                constraints.extend(classes.inline_constraints(ty, module, &names)?);
            }
            default_reads.insert(id, reads.clone());
            Ok(Scheme {
                pure: function.pure,
                signature,
                parameter_conventions: reads
                    .into_iter()
                    .zip(&function.parameters)
                    .map(|(read, parameter)| {
                        if matches!(parameter.ty.kind, TypeExprKind::Mut(_)) {
                            ParameterConvention::Mut
                        } else if read {
                            ParameterConvention::Read
                        } else {
                            ParameterConvention::Own
                        }
                    })
                    .collect(),
                variables,
                constraints,
                members,
            })
        })();
        signatures.push(match checked {
            Ok(scheme) => scheme,
            Err(error) => {
                diagnostics.push(error);
                Scheme::poisoned()
            }
        });
    }
    let mut active_results = BTreeMap::new();
    for &ModuleInput {
        name: module,
        program,
        ..
    } in modules
    {
        for active in &program.active_patterns {
            if diagnostics.is_full() {
                diagnostics.check()?;
            }
            let function = names.functions[&format!("{module}.{}", active.function)].clone();
            let id = function.id;
            let signature = &signatures[id].signature;
            if !signatures[id].is_poisoned() && signature.parameters.is_empty() {
                diagnostics.push(Diagnostic::of(
                    DiagnosticKind::ActiveRecognizerParameters,
                    "an active recognizer needs an input parameter",
                    active.cases[0].span,
                ));
                signatures[id] = Scheme::poisoned();
            }
            let maybe_result = matches!(&signatures[id].signature.result, Type::Union(union_id, _) if names.unions.get("Maybe.Maybe").is_some_and(|info| info.id == *union_id));
            if !signatures[id].is_poisoned()
                && active.partial
                && signatures[id].signature.result != Type::Bool
                && !maybe_result
            {
                diagnostics.push(Diagnostic::of(
                    DiagnosticKind::ActiveRecognizerResult,
                    "a partial active recognizer must return bool or Maybe payload",
                    active.cases[0].span,
                ));
                signatures[id] = Scheme::poisoned();
            }
            if active.cases.len() > 1 && !signatures[id].is_poisoned() {
                active_results.insert(id, active);
                let error = match &signatures[id].signature.result {
                    Type::Union(union_id, _)
                        if unions[*union_id].cases.len() == active.cases.len() =>
                    {
                        None
                    }
                    Type::Union(..) => Some(Diagnostic::of(
                        DiagnosticKind::ActivePatternCaseCount,
                        "active pattern and backing union must have the same case count",
                        active.cases[0].span,
                    )),
                    _ => Some(Diagnostic::of(
                        DiagnosticKind::ActiveRecognizerResult,
                        "a multi-case active recognizer must return a union",
                        active.cases[0].span,
                    )),
                };
                if let Some(error) = error {
                    diagnostics.push(error);
                    signatures[id] = Scheme::poisoned();
                }
            }
            for (index, case) in active.cases.iter().enumerate() {
                let qualified = format!("{module}.{}", case.text);
                let kind = if active.cases.len() > 1 {
                    ActiveCase::TotalCase {
                        index,
                        count: active.cases.len(),
                    }
                } else if !active.partial {
                    ActiveCase::TotalSingle
                } else if maybe_result {
                    ActiveCase::MaybePartial
                } else {
                    ActiveCase::BoolPartial
                };
                let info = NameInfo {
                    name: qualified.clone(),
                    ..function.clone()
                };
                if names.cases.contains_key(&qualified)
                    || names.records.contains_key(&qualified)
                    || names.unions.contains_key(&qualified)
                    || names.handles.contains_key(&qualified)
                    || names.type_aliases.contains_key(&qualified)
                    || names.active_patterns.contains_key(&qualified)
                {
                    diagnostics.push(duplicate(case));
                    continue;
                }
                names
                    .active_patterns
                    .insert(qualified.clone(), (info, kind));
                names
                    .active_aliases
                    .entry(case.text.clone())
                    .or_default()
                    .push(qualified);
            }
        }
    }
    for (&id, active) in &active_results {
        if signatures[id].is_poisoned() {
            continue;
        }
        let (module, function) = &function_declarations[id];
        match infer_active_result(
            module,
            function,
            &names,
            types,
            &signatures,
            &classes,
            active,
        ) {
            Ok(Some(result)) => {
                signatures[id].signature.result = result;
                signatures[id].variables =
                    polymorph::variables(&signatures[id].signature.as_type());
            }
            Ok(None) => {}
            Err(error) => {
                diagnostics.push(error);
                signatures[id] = Scheme::poisoned();
            }
        }
    }
    let declaration_count = function_declarations.len();
    let mut functions = Vec::new();
    let mut ids = Vec::new();
    let mut recovered_functions = Vec::new();
    let mut pending = Vec::new();
    let mut warnings = Vec::new();
    // The bodies of different functions are checked in parallel: each reads the shared tables and
    // changes only its own declaration (`computation::expand`). The loop that collects the results
    // runs in declaration order, stops once `diagnostics` is full, and a stage on one thread does
    // not even check the later bodies, so the outcome does not depend on the number of threads.
    let workers = parallelism.workers(function_declarations.len(), FUNCTIONS_PER_WORKER);
    ordered_for_each_owned(
        function_declarations.iter_mut(),
        workers,
        FUNCTION_CHUNK,
        None,
        |id, (module, function)| {
            let scheme = &signatures[id];
            if scheme.is_poisoned() {
                return None;
            }
            let signature = scheme.signature.clone();
            let mut checker = Checker::new(module, &names, types, &signatures, &classes);
            checker.indexing = indexing;
            if let Some(active) = active_results.get(&id) {
                checker.use_active_result(&signature.result, active);
            }
            let checked = (|| {
                let reads = default_reads.get(&id).cloned().unwrap_or_default();
                for (index, parameter) in function.parameters.iter_mut().enumerate() {
                    if let TypeExprKind::Own(inner) = &parameter.ty.kind {
                        parameter.ty = (**inner).clone();
                    } else if let TypeExprKind::Mut(inner) = &parameter.ty.kind {
                        parameter.ty = TypeExpr {
                            kind: TypeExprKind::Reference(inner.clone(), true),
                            span: parameter.ty.span,
                        };
                    } else if reads.get(index) == Some(&true) {
                        parameter.ty = TypeExpr {
                            kind: TypeExprKind::Reference(Box::new(parameter.ty.clone()), false),
                            span: parameter.ty.span,
                        };
                    }
                }
                let contracts = regions::contract(function, module, &names, types)?;
                checker.type_parameters = scheme.variables.clone();
                checker.kinds = classes.constraint_kinds(&function.constraints, module, &names)?;
                checker.members = scheme.members.clone();
                let mut parameters = Vec::new();
                for (parameter, ty) in function.parameters.iter().zip(&signature.parameters) {
                    let local = checker.bind(&parameter.name, ty.clone(), parameter.mutable);
                    if scheme.parameter_conventions.get(parameters.len())
                        == Some(&ParameterConvention::Mut)
                    {
                        checker.read_value_parameters.insert(local.id);
                    }
                    if reads.get(parameters.len()) == Some(&true)
                        && let Type::Reference(inner, _) = ty
                    {
                        checker.default_reads.insert(
                            local.id,
                            (local.name.clone(), inner.display(&types), function.pure),
                        );
                        if types.reaches(inner, |part| {
                            matches!(part, Type::Variable(_) | Type::Function(..))
                        }) {
                            checker.read_value_parameters.insert(local.id);
                        }
                    }
                    parameters.push(local);
                }
                computation::expand(&mut function.body, &names, module)?;
                checker.recovering = true;
                let body = if let Some(import) = external_functions.get(&id) {
                    let mut import = import.clone();
                    import.callbacks = signature
                        .parameters
                        .iter()
                        .any(|ty| matches!(ty, Type::Function(..)));
                    TypedExpr {
                        kind: TypedExprKind::HostCall(
                            Box::new(import),
                            parameters
                                .iter()
                                .map(|local| TypedExpr {
                                    kind: TypedExprKind::Local(local.id),
                                    ty: local.ty.clone(),
                                    span: local.span,
                                })
                                .collect(),
                        ),
                        ty: signature.result.clone(),
                        span: function.name.span,
                    }
                } else {
                    let from = checker.recovered.len();
                    let mut body = checker.expression(&function.body, Some(&signature.result));
                    // Generated functions (tests, benchmarks) have no written result type.
                    if (body.is_err() || checker.recovered.len() > from)
                        && function.name.provenance == Provenance::User
                        && !function.name.text.starts_with('$')
                    {
                        checker.expected_because(
                            body.as_mut().err(),
                            from,
                            &tail_spans(&function.body),
                            &signature.result,
                            &format!(
                                "the result type of '{}' is declared here",
                                function.name.text
                            ),
                            function.result.span,
                        );
                    }
                    body?
                };
                Ok(CheckedFunction {
                    module: module.clone(),
                    read_by_default: reads,
                    region_sources: contracts.result,
                    callback_contracts: contracts.callbacks,
                    region_slots: contracts.slots,
                    origin: FunctionOrigin {
                        provenance: function.name.provenance,
                        test: test_functions.get(&id).copied(),
                        bench: bench_functions.get(&id).copied(),
                        cpu: cpu_levels.get(&id).copied().unwrap_or(0),
                        pure: function.pure,
                        pure_values: checker.pure_values,
                        ..FunctionOrigin::source(names.origin(module))
                    },
                    name: function.name.text.clone(),
                    visibility: function.visibility,
                    exported: function.exported,
                    parameters,
                    signature,
                    body,
                    span: function.name.span,
                    type_parameters: scheme.variables.clone(),
                    constraints: scheme.constraints.clone(),
                    members: Vec::new(),
                    capture_count: 0,
                    is_task: false,
                    owned_captures: false,
                })
            })();
            let recovered: Vec<_> = std::mem::take(&mut checker.recovered)
                .into_iter()
                .map(|error| classes.derived_error(id, error))
                .collect();
            let checked = checked.map_err(|error| classes.derived_error(id, error));
            Some((recovered, checked, checker))
        },
        |id, outcome| {
            if diagnostics.is_full() {
                return ControlFlow::Break(());
            }
            let Some((recovered, checked, checker)) = outcome.get() else {
                return ControlFlow::Continue(());
            };
            diagnostics.extend(recovered);
            match checked {
                Ok(function) if !checker.poisoned => {
                    functions.push(function);
                    ids.push(id);
                    pending.push(checker);
                }
                Ok(function) => recovered_functions.push((id, function, checker)),
                Err(error) => diagnostics.push(error),
            }
            ControlFlow::Continue(())
        },
    );
    let mut entry = modules
        .iter()
        .find(|module| {
            module.origin == ModuleOrigin::User
                && module.entry
                && matches!(module.program.source_kind, None | Some(SourceKind::Code))
        })
        .and_then(|module| names.local_function(module.name, "main"))
        .filter(|id| !names.constants.contains(id));
    for &ModuleInput {
        name: module,
        program,
        origin,
        entry: is_entry,
        ..
    } in modules
    {
        if diagnostics.is_full() {
            break;
        }
        let Some(expression) = &program.entry else {
            continue;
        };
        if origin != ModuleOrigin::User || !is_entry {
            diagnostics.push(Diagnostic::of(DiagnosticKind::ToplevelCodeOutsideMain,
                "top-level execution is only allowed in Main.tz; other modules contain record and function declarations",
                expression.span,
            ));
            continue;
        }
        if entry.is_some() {
            diagnostics.push(Diagnostic::of(
                DiagnosticKind::AmbiguousEntryPoint,
                "Main.tz must use either top-level entry-point code or 'def main', not both",
                expression.span,
            ));
            continue;
        }
        let mut checker = Checker::new(module, &names, types, &signatures, &classes);
        checker.indexing = indexing;
        let checked = (|| {
            let mut expression = expression.clone();
            computation::expand(&mut expression, &names, module)?;
            computation::direct_entry(&mut expression);
            checker.recovering = true;
            let body = checker.expression(&expression, None)?;
            Ok(CheckedFunction {
                module: module.to_owned(),
                region_sources: None,
                callback_contracts: Vec::new(),
                region_slots: None,
                origin: FunctionOrigin {
                    pure_values: checker.pure_values,
                    provenance: Provenance::Generated,
                    ..FunctionOrigin::source(ModuleOrigin::User)
                },
                name: "$entry".into(),
                read_by_default: Vec::new(),
                visibility: Visibility::Public,
                exported: false,
                parameters: Vec::new(),
                signature: Signature {
                    parameters: Vec::new(),
                    result: body.ty.clone(),
                },
                body,
                span: expression.span,
                type_parameters: Vec::new(),
                constraints: Vec::new(),
                members: Vec::new(),
                capture_count: 0,
                is_task: false,
                owned_captures: false,
            })
        })();
        diagnostics.extend(std::mem::take(&mut checker.recovered));
        match checked {
            Ok(function) if !checker.poisoned => {
                entry = Some(functions.len());
                functions.push(function);
                ids.push(declaration_count);
                pending.push(checker);
            }
            Ok(function) => recovered_functions.push((declaration_count, function, checker)),
            Err(error) => diagnostics.push(error),
        }
    }
    if diagnostics.check().is_ok() {
        polymorph::solve_members(&mut functions, &mut pending)?;
    }
    let mut name_uses = Vec::new();
    // Finishing a function reads the shared tables and changes only that function and its checker,
    // so the functions finish in parallel; what each leaves for the shared lists is applied in
    // function order.
    let workers = parallelism.workers(functions.len(), FUNCTIONS_PER_WORKER);
    let function_names = warnings::function_names(&names);
    ordered_for_each_owned(
        functions.iter_mut().zip(pending),
        workers,
        FUNCTION_CHUNK,
        None,
        |_, (function, mut checker)| {
            let name_use = std::mem::take(&mut checker.name_uses);
            let mut function_warnings = Vec::new();
            let checked = (|| {
                checker.finish(&mut function.body)?;
                let coverage = checker.check_coverage()?;
                if function.origin.module == ModuleOrigin::User {
                    function_warnings.extend(coverage);
                    function_warnings.extend(warnings::unused_locals(function, &function_names));
                    function_warnings.append(&mut checker.shadowing_warnings);
                }
                if function.name == "$entry" {
                    entry_result(&mut function.body, &names, &classes, &types);
                    function.signature.result = function.body.ty.clone();
                }
                function.constraints.extend(checker.constraints);
                function.members = checker.members;
                Ok(())
            })();
            let error = checked.err().map(|error| {
                // Unfinished types must not reach the ownership recovery pass.
                function.body = TypedExpr::error(function.span);
                let id = names
                    .functions
                    .get(&function.qualified_name())
                    .map_or(usize::MAX, |info| info.id);
                classes.derived_error(id, error)
            });
            (name_use, function_warnings, error)
        },
        |_, outcome| {
            let (name_use, function_warnings, error) = outcome.get();
            name_uses.push(name_use);
            warnings.extend(function_warnings);
            if let Some(error) = error {
                diagnostics.push(error);
            }
            ControlFlow::Continue(())
        },
    );
    if let Err(error) = diagnostics.check() {
        if !diagnostics.is_full() {
            let recovered: Vec<_> = recovered_functions
                .into_iter()
                .map(|(id, mut function, mut checker)| {
                    if checker.finish(&mut function.body).is_err() {
                        function.body = TypedExpr::error(function.span);
                    }
                    (id, function)
                })
                .collect();
            let module = recovery_module(
                records,
                unions,
                &function_declarations,
                ids.into_iter().zip(functions).chain(recovered),
            );
            diagnostics.extend(crate::ownership::check_recovered(&module));
        }
        return Err(error);
    }
    debug_assert_eq!(
        ids[..declaration_count],
        (0..declaration_count).collect::<Vec<_>>()
    );
    diagnostics.check()?;
    let resolve_api = semantic
        .as_ref()
        .is_some_and(|index| index.resolved_api.is_some());
    if indexing {
        let index = semantic::collect(modules, &functions, &name_uses, &names, types);
        if !deprecations.is_empty() {
            warnings.extend(warnings::deprecated_uses(
                modules,
                &index,
                &names,
                &deprecations,
            ));
        }
        if let Some(capture) = capture.as_deref_mut() {
            capture.semantic = Some(index.clone());
        }
        if let Some(requested) = semantic.as_deref_mut() {
            *requested = index;
        }
    }
    let const_uses =
        constants::const_def_uses(&functions, &names.constants, &names.const_functions);
    let statics = constants::fold(
        &mut functions,
        &names.constants,
        &names.const_functions,
        &types,
    )?;
    let mut references =
        recursion::check(&functions, &function_declarations, &classes, &names, &types)?;
    // A `const def` that a constant calls is used, although `fold` replaced the call by a value.
    for (constant, callee) in const_uses {
        references[constant].push(callee);
    }
    warnings.extend(warnings::unused_private(
        &functions,
        &function_declarations,
        modules,
        &names,
        types,
        &references,
        entry,
    ));
    // Warnings follow source order rather than the order bodies are checked.
    warnings.sort_by_key(|warning: &Diagnostic| (warning.span.source, warning.span.start));
    let mut module = CheckedModule {
        records,
        unions,
        functions,
        entry,
        tests,
        benches,
        warnings,
        user_drops: BTreeMap::new(),
        bench_drops: BTreeSet::new(),
        vtables: BTreeMap::new(),
        dyn_layouts: BTreeMap::new(),
        uses_dyn: modules
            .iter()
            .any(|input| !input.program.dyn_types.is_empty()),
        gpu: Default::default(),
        statics,
    };
    validate_callbacks(&module)?;
    diagnostics.extend(pure_rules::validate_templates(&module));
    diagnostics.check()?;
    regions::validate_contract_calls(&module)?;
    // F09 Phase 2: a program that names a non-CPU `Gpu.Backend` calls the device-aware std functions.
    crate::gpu_devices::retarget_calls(&mut module);
    let copy_constraints =
        crate::ownership::infer_copy_all(&module, parallelism).map_err(|errors| {
            let first = errors[0].clone();
            diagnostics.extend(errors);
            first
        })?;
    let recursive_functions = function_declarations
        .iter()
        .enumerate()
        .filter_map(|(id, (_, declaration))| declaration.recursion.is_some().then_some(id))
        .collect();
    drop(check_time);
    // G26: the declaration view of the knowledge graph, while the module still has the abstract
    // bodies; its own phase, so that the phases of `TSUZURI_TIME_PASSES` do not overlap.
    if let Some(capture) = capture {
        let _kg_time = crate::timings::Phase::start("kg-capture");
        kg_capture::collect_abstract(
            capture,
            &kg_capture::Abstract {
                modules,
                module: &module,
                names: &names,
                classes: &classes,
                references: &references,
            },
        );
    }
    let specialize_time = crate::timings::Phase::start("specialize");
    let module = polymorph::specialize(
        module,
        &classes,
        &names,
        copy_constraints,
        &recursive_functions,
        |module| {
            if resolve_api && let Some(index) = semantic {
                index.resolved_api = Some(semantic::collect_api(modules, module, &names, &classes));
            }
            resolve_api
        },
    );
    drop(specialize_time);
    let module = module?;
    let closures_time = crate::timings::Phase::start("closures");
    let module = closures::lower(module);
    drop(closures_time);
    let mut module = module?;
    if module.functions.iter().any(|function| {
        function.module == "$builtin"
            && function
                .region_slots
                .as_ref()
                .is_some_and(|slots| slots.writes)
    }) {
        regions::validate_contract_calls(&module)?;
    }
    let ownership_time = crate::timings::Phase::start("ownership");
    diagnostics.extend(crate::ownership::check_all_with(&module, parallelism));
    drop(ownership_time);
    if let Err(error) = crate::gpu::validate_calls(&module) {
        diagnostics.push(error);
    }
    diagnostics.extend(pure_rules::validate(&module));
    if let Err(error) = validate_cpu_functions(&module) {
        diagnostics.push(error);
    }
    diagnostics.check()?;
    // F09 Phase 2: the calls are validated, so their callbacks are known functions.
    crate::gpu_devices::lower_kernels(&mut module)?;
    Ok(module)
}

/// F08 Phase 3: the versions of a `@cpu` function run with different instruction sets, which
/// pass vectors wider than 128 bits in different registers. No such vector may cross its
/// signature, and it calls no function value that passes one; named functions that pass one
/// get versions of their own.
fn validate_cpu_functions(module: &CheckedModule) -> Result<(), Diagnostic> {
    let types = module.types();
    for function in module
        .functions
        .iter()
        .filter(|function| function.origin.cpu != 0)
    {
        let signature = &function.signature;
        if signature
            .parameters
            .iter()
            .chain([&signature.result])
            .any(|ty| ty.holds_wide_vector(&types))
        {
            return Err(Diagnostic::of(
                DiagnosticKind::CpuFunctionWideVectorBoundary,
                "a '@cpu' function cannot take or return a SIMD vector wider than 128 bits, also inside a record, union, tuple, or fixed-length array; pass a slice or return a scalar",
                function.span,
            ));
        }
        let mut pending = vec![&function.body];
        while let Some(expression) = pending.pop() {
            if let TypedExprKind::Call(callee, _) = &expression.kind
                && !matches!(callee.kind, TypedExprKind::Function(_))
                && let Type::Function(parameters, result) = &callee.ty
                && parameters
                    .iter()
                    .chain([result.as_ref()])
                    .any(|ty| ty.holds_wide_vector(&types))
            {
                return Err(Diagnostic::of(
                    DiagnosticKind::CpuFunctionWideVectorBoundary,
                    "a '@cpu' function cannot call a function value that takes or returns a SIMD vector wider than 128 bits; call a named function",
                    expression.span,
                ));
            }
            expression.try_for_each_child(&mut |child| {
                pending.push(child);
                Ok(())
            })?;
        }
    }
    Ok(())
}

/// Callback externs run only through direct calls with every argument, and each
/// callback argument names a top-level user function: no lambda, local, partial
/// application, generic function, std function, or extern. The host gets a
/// static C function pointer, so nothing needs a context or a lifetime.
fn validate_callbacks(module: &CheckedModule) -> Result<(), Diagnostic> {
    let is_callback_extern = |id: usize| matches!(&module.functions[id].body.kind, TypedExprKind::HostCall(import, _) if import.callbacks);
    if !(0..module.functions.len()).any(is_callback_extern) {
        return Ok(());
    }
    let callback_extern = |expression: &TypedExpr| match &expression.kind {
        TypedExprKind::Function(FunctionRef::User(id)) if is_callback_extern(*id) => Some(*id),
        _ => None,
    };
    let direct_only = |id: usize, span: Span| {
        Diagnostic::of(
            DiagnosticKind::ExternCallbackCallForm,
            format!(
                "extern '{}' takes a callback and must be called directly with all arguments",
                module.functions[id].name
            ),
            span,
        )
    };
    let plain_function = |argument: &TypedExpr| match &argument.kind {
        TypedExprKind::Function(FunctionRef::User(id)) => {
            let function = &module.functions[*id];
            function.origin.module == ModuleOrigin::User
                && function.origin.provenance == Provenance::User
                && function.origin.parent.is_none()
                && function.origin.test.is_none()
                && function.type_parameters.is_empty()
                && !matches!(function.body.kind, TypedExprKind::HostCall(..))
        }
        _ => false,
    };
    for function in &module.functions {
        let mut pending = vec![&function.body];
        while let Some(expression) = pending.pop() {
            if let TypedExprKind::Call(callee, arguments) = &expression.kind
                && let Some(id) = callback_extern(callee)
            {
                let target = &module.functions[id];
                if arguments.len() != target.parameters.len() {
                    return Err(direct_only(id, expression.span));
                }
                for (parameter, argument) in target.signature.parameters.iter().zip(arguments) {
                    if matches!(parameter, Type::Function(..)) && !plain_function(argument) {
                        return Err(Diagnostic::of(
                            DiagnosticKind::ExternCallbackArgument,
                            "callback arguments must name a top-level user function without captures; move the logic into a 'def' and pass its name",
                            argument.span,
                        ));
                    }
                }
                // The callee is the extern itself; only the arguments can hold other uses.
                pending.extend(arguments.iter());
                continue;
            }
            if let Some(id) = callback_extern(expression) {
                return Err(direct_only(id, expression.span));
            }
            expression.try_for_each_child(&mut |child| {
                pending.push(child);
                Ok(())
            })?;
        }
    }
    Ok(())
}

/// Top-level code prints a number, bool, character, or string and runs an
/// `IO` action. Any other value prints through its `Display` instance, or is
/// dropped when it has none, as `safe_add 2147483647 1` drops its `Result`.
fn entry_result(
    body: &mut TypedExpr,
    names: &Names,
    classes: &polymorph::Classes,
    types: &TypeContext<'_>,
) {
    let ty = body.ty.clone();
    let io = names.records.get("IO.IO").map(|info| info.id);
    if ty.is_scalar()
        || matches!(
            ty,
            Type::Unit | Type::String | Type::Utf8String | Type::Error
        )
        || matches!(ty, Type::Record(id, _) if Some(id) == io)
    {
        return;
    }
    let (builtin, result) = if classes.holds("Display", &ty, types) {
        (Builtin::ToString, Type::String)
    } else {
        (Builtin::Ignore, Type::Unit)
    };
    let span = body.span;
    let callee = TypedExpr {
        kind: TypedExprKind::Function(FunctionRef::Builtin(BuiltinInstance {
            builtin,
            types: vec![ty.clone()],
        })),
        ty: Type::function(vec![ty], result.clone()),
        span,
    };
    let value = std::mem::replace(body, TypedExpr::error(span));
    *body = TypedExpr {
        kind: TypedExprKind::Call(Box::new(callee), vec![value]),
        ty: result,
        span,
    };
}

/// The error type that a `try ... with` function leaves open. In a result
/// `Result<'T, 'E>` whose `'E` no parameter mentions, `'E` is the caught
/// `Exception` when it is constrained by `Err` or the body is a `try`.
fn try_errors(
    function: &FunctionDecl,
    parameters: &[Type],
    result: &Type,
    module: &str,
    names: &Names,
    classes: &polymorph::Classes,
    kinds: &BTreeMap<String, usize>,
) -> Result<BTreeMap<String, Type>, Diagnostic> {
    let result_union = names.unions.get("Result.Result").map(|info| info.id);
    let Type::Union(id, arguments) = result else {
        return Ok(BTreeMap::new());
    };
    let (Some(true), [_, Type::Variable(variable)]) =
        (result_union.map(|union| union == *id), arguments.as_ref())
    else {
        return Ok(BTreeMap::new());
    };
    if parameters
        .iter()
        .any(|ty| polymorph::variables(ty).contains(variable))
    {
        return Ok(BTreeMap::new());
    }
    fn ends_in_try(expression: &Expr) -> bool {
        match &expression.kind {
            ExprKind::Try(_) => true,
            ExprKind::Block { result, .. } => ends_in_try(result),
            _ => false,
        }
    }
    let mut constrained = false;
    for constraint in &function.constraints {
        if let ConstraintName::Class(name) = &constraint.name
            && name.text == "Err"
            && matches!(classes.constraint_type(constraint, module, names, kinds)?,
                Type::Variable(other) if other == *variable)
        {
            constrained = true;
        }
    }
    if !constrained && !ends_in_try(&function.body) {
        return Ok(BTreeMap::new());
    }
    let exception = names.std_type("Exception", "Exception", Box::new([]), function.name.span)?;
    Ok(BTreeMap::from([(variable.clone(), exception)]))
}

// The ownership checker indexes functions by declaration ID.
fn recovery_module(
    records: Vec<CheckedRecord>,
    unions: Vec<CheckedUnion>,
    declarations: &[(String, FunctionDecl)],
    checked: impl Iterator<Item = (usize, CheckedFunction)>,
) -> CheckedModule {
    let mut functions: Vec<_> = declarations
        .iter()
        .map(|(module, declaration)| CheckedFunction {
            module: module.clone(),
            read_by_default: Vec::new(),
            region_sources: None,
            callback_contracts: Vec::new(),
            region_slots: None,
            origin: FunctionOrigin::source(ModuleOrigin::User),
            name: declaration.name.text.clone(),
            visibility: declaration.visibility,
            exported: false,
            parameters: Vec::new(),
            signature: Signature {
                parameters: Vec::new(),
                result: Type::Error,
            },
            body: TypedExpr::error(declaration.body.span),
            span: declaration.name.span,
            type_parameters: Vec::new(),
            constraints: Vec::new(),
            members: Vec::new(),
            capture_count: 0,
            is_task: false,
            owned_captures: false,
        })
        .collect();
    for (id, function) in checked {
        if id == functions.len() {
            functions.push(function);
        } else {
            functions[id] = function;
        }
    }
    CheckedModule {
        records,
        unions,
        functions,
        entry: None,
        tests: Vec::new(),
        benches: Vec::new(),
        warnings: Vec::new(),
        user_drops: BTreeMap::new(),
        bench_drops: BTreeSet::new(),
        vtables: BTreeMap::new(),
        dyn_layouts: BTreeMap::new(),
        uses_dyn: false,
        gpu: Default::default(),
        statics: BTreeMap::new(),
    }
}

fn infer_active_result(
    module: &str,
    function: &FunctionDecl,
    names: &Names,
    types: TypeContext<'_>,
    signatures: &[Scheme],
    classes: &Classes,
    active: &crate::syntax::ActivePattern,
) -> Result<Option<Type>, Diagnostic> {
    let id = names.local_function(module, &function.name.text).unwrap();
    let scheme = &signatures[id];
    if scheme.variables.iter().any(|name| name == "T") {
        return Err(Diagnostic::of(
            DiagnosticKind::ReservedActiveResultVariable,
            "'T is reserved for the implicit active-pattern result, not an input type variable",
            function.result.span,
        ));
    }
    let Type::Union(union_id, arguments) = &scheme.signature.result else {
        return Ok(None);
    };
    if arguments.is_empty() {
        return Ok(None);
    }
    let mut checker = Checker::new(module, names, types, signatures, classes);
    checker.type_parameters = scheme.variables.clone();
    checker.kinds = classes.constraint_kinds(&function.constraints, module, names)?;
    checker.members = scheme.members.clone();
    for (parameter, ty) in function.parameters.iter().zip(&scheme.signature.parameters) {
        checker.bind(&parameter.name, ty.clone(), parameter.mutable);
    }
    let result = Type::Union(
        *union_id,
        arguments
            .iter()
            .map(|_| checker.inference.fresh())
            .collect(),
    );
    checker.use_active_result(&result, active);
    let mut source = function.body.clone();
    computation::expand(&mut source, names, module)?;
    let mut body = checker.expression(&source, Some(&result))?;
    checker.finish(&mut body)?;
    let result = checker.inference.resolve(&result);
    validate_size(&result, &types, function.result.span)?;
    Ok(Some(result))
}

/// The spans of the positions that an expected type is passed down to: the expression, and the
/// result of a block, the branches of an `if`, and the arms of a `match` in it, recursively. An
/// operator is left out: it checks its operands against types of its own and reports them at its
/// own span (`!`, `&&`), so a mismatch there is not explained by the declaration.
fn tail_spans(expression: &Expr) -> Vec<Span> {
    let mut spans = Vec::new();
    let mut pending = vec![expression];
    while let Some(expression) = pending.pop() {
        if !matches!(
            expression.kind,
            ExprKind::Unary(..)
                | ExprKind::Binary(..)
                | ExprKind::Range { .. }
                | ExprKind::Cast(..)
                | ExprKind::Interpolated(_)
                | ExprKind::Assign(..)
        ) {
            spans.push(expression.span);
        }
        match &expression.kind {
            ExprKind::Block { result, .. } => pending.push(result),
            ExprKind::If {
                then_branch,
                else_branch,
                ..
            } => pending.extend([&**then_branch, &**else_branch]),
            ExprKind::Match { arms, .. } => pending.extend(arms.iter().map(|arm| &arm.body)),
            _ => {}
        }
    }
    spans
}

/// `duplicate` with the first declaration of the name, when it is known.
fn duplicate_of(name: &Ident, first: Option<Span>) -> Diagnostic {
    let error = duplicate(name);
    match first {
        Some(first) => error.related(RelatedRole::FirstDefinedHere, "first defined here", first),
        None => error,
    }
}

/// `help` of an unknown field: the fields that the record has, at most 16.
fn field_names_help(record: &str, fields: &[(String, Type)]) -> String {
    let shown: Vec<&str> = fields
        .iter()
        .take(16)
        .map(|(name, _)| name.as_str())
        .collect();
    let more = if fields.len() > 16 { ", ..." } else { "" };
    format!("the fields of '{record}' are: {}{more}", shown.join(", "))
}

fn duplicate(name: &Ident) -> Diagnostic {
    Diagnostic::of(
        DiagnosticKind::DuplicateName,
        format!("duplicate or reserved name '{}'", name.text),
        name.span,
    )
}

/// How many bindings the `let rec` group at the start of `bindings` has: the `let rec` binding and
/// the `and` bindings that follow it (B09).
pub(super) fn recursive_group_len(bindings: &[Binding]) -> usize {
    1 + bindings[1..]
        .iter()
        .take_while(|binding| binding.recursion == Recursion::And)
        .count()
}

/// Whether no record, extern type, or type alias may be named `name` (`E1001`):
/// primitive and SIMD type names, `_`, the built-in `Array`, `Task`, and `Vec`,
/// and the built-in type classes. `tsuzuri bindgen` renames generated types with it.
pub fn reserved_type_name(name: &str) -> bool {
    crate::numeric::primitive(name).is_some()
        || matches!(name, "_" | "Array" | "Task" | "Vec" | "Rc" | "Arc")
        || polymorph::BUILTIN_CLASSES.contains(&name)
}

/// Checks the type parameters that a record or union declares.
fn declared_parameters(
    kind: &str,
    name: &str,
    parameters: &[Ident],
) -> Result<Vec<String>, Diagnostic> {
    let mut declared: Vec<String> = Vec::new();
    for parameter in parameters {
        if parameter.text == "_" {
            return Err(Diagnostic::of(
                DiagnosticKind::InvalidTypeParameterName,
                format!("'_ cannot name a {kind} type parameter; use a variable such as 'a"),
                parameter.span,
            ));
        }
        if declared.contains(&parameter.text) {
            return Err(Diagnostic::of(
                DiagnosticKind::DuplicateTypeParameter,
                format!(
                    "duplicate {} in {kind} '{name}'; give each parameter a distinct name",
                    parameter_name(&parameter.text)
                ),
                parameter.span,
            ));
        }
        declared.push(parameter.text.clone());
    }
    Ok(declared)
}

/// A declared parameter in diagnostics: `type parameter 'a` or `length parameter N` (A16).
fn parameter_name(text: &str) -> String {
    match text.strip_prefix('#') {
        Some(length) => format!("length parameter {length}"),
        None => format!("type parameter '{text}"),
    }
}

/// A length name in a declaration that is neither a declared length parameter nor a constant.
fn undeclared_length(kind: &str, name: &str, length: &str, span: Span) -> Diagnostic {
    Diagnostic::of(
        DiagnosticKind::UndeclaredLengthParameter,
        format!(
            "length '{length}' is not declared by {kind} '{name}'; add 'const {length}: i64' in angle brackets after the name, or define an 'i64' constant '{length}'"
        ),
        span,
    )
}

fn starts_uppercase(name: &str) -> bool {
    name.starts_with(|first: char| first.is_ascii_uppercase())
}

/// Registers union type names, then their cases. A case shares its module's
/// value namespace with functions, and its type namespace with records and
/// unions, so a `Module.Name` path never has two meanings in one module.
fn collect_unions(
    declarations: &[(&str, &UnionDecl)],
    names: &mut Names,
    diagnostics: &mut Diagnostics,
) -> Result<(), Diagnostic> {
    for (id, (module, union)) in declarations.iter().enumerate() {
        if diagnostics.is_full() {
            break;
        }
        let registered = (|| {
            let name = &union.name;
            if !starts_uppercase(&name.text) {
                return Err(Diagnostic::of(
                    DiagnosticKind::TypeNameCase,
                    format!(
                        "union type '{}' must start with an uppercase ASCII letter",
                        name.text
                    ),
                    name.span,
                ));
            }
            let qualified = format!("{module}.{}", name.text);
            if matches!(name.text.as_str(), "Array" | "Task" | "Vec" | "Rc" | "Arc")
                || polymorph::BUILTIN_CLASSES.contains(&name.text.as_str())
                || names.records.contains_key(&qualified)
                || names.unions.contains_key(&qualified)
                || names.handles.contains_key(&qualified)
            {
                return Err(duplicate(name));
            }
            names.unions.insert(
                qualified.clone(),
                NameInfo {
                    id,
                    name: qualified.clone(),
                    module: (*module).to_owned(),
                    visibility: union.visibility,
                },
            );
            names
                .union_aliases
                .entry(name.text.clone())
                .or_default()
                .push(qualified);
            names.union_arities.push(union.parameters.len());
            names.union_lengths.push(
                union
                    .parameters
                    .iter()
                    .map(|parameter| parameter.text.starts_with('#'))
                    .collect(),
            );
            Ok(())
        })();
        if let Err(error) = registered {
            diagnostics.push(error);
        }
    }
    diagnostics.check()?;
    for (id, (module, union)) in declarations.iter().enumerate() {
        for (case, declaration) in union.cases.iter().enumerate() {
            if diagnostics.is_full() {
                break;
            }
            let registered = (|| {
                let name = &declaration.name;
                if !starts_uppercase(&name.text) {
                    return Err(Diagnostic::of(
                        DiagnosticKind::UnionCaseNameCase,
                        format!(
                            "union case '{}' must start with an uppercase ASCII letter; lowercase names in patterns bind variables",
                            name.text
                        ),
                        name.span,
                    ));
                }
                if let Some(first) = union.cases[..case]
                    .iter()
                    .find(|other| other.name.text == name.text)
                {
                    return Err(Diagnostic::of(
                        DiagnosticKind::DuplicateUnionCase,
                        format!(
                            "duplicate case '{}' in union '{}'; give each case a distinct name",
                            name.text, union.name.text
                        ),
                        name.span,
                    )
                    .related(
                        RelatedRole::FirstDefinedHere,
                        "first defined here",
                        first.name.span,
                    ));
                }
                let qualified = format!("{module}.{}", name.text);
                if matches!(name.text.as_str(), "Array" | "Task" | "Vec" | "Rc" | "Arc")
                    || names.records.contains_key(&qualified)
                    || names.unions.contains_key(&qualified)
                    || names.handles.contains_key(&qualified)
                    || names.cases.contains_key(&qualified)
                {
                    return Err(duplicate(name));
                }
                names.cases.insert(
                    qualified.clone(),
                    CaseInfo {
                        info: NameInfo {
                            id,
                            name: qualified.clone(),
                            module: (*module).to_owned(),
                            visibility: union.visibility,
                        },
                        case,
                        has_payload: declaration.payload.is_some(),
                    },
                );
                names
                    .case_aliases
                    .entry(name.text.clone())
                    .or_default()
                    .push(qualified);
                Ok(())
            })();
            if let Err(error) = registered {
                diagnostics.push(error);
            }
        }
        if diagnostics.is_full() {
            break;
        }
    }
    diagnostics.check()
}

/// Resolves the payload types of a union declaration.
fn check_union(module: &str, union: &UnionDecl, names: &Names) -> Result<CheckedUnion, Diagnostic> {
    let name = &union.name.text;
    let qualified = format!("{module}.{name}");
    let parameters = declared_parameters("union", name, &union.parameters)?;
    let mut used = BTreeSet::new();
    let mut cases = Vec::new();
    for case in &union.cases {
        let Some(expression) = &case.payload else {
            cases.push((case.name.text.clone(), None));
            continue;
        };
        reject_field_constraints(expression, module, names, "union")?;
        let ty = resolve_type(expression, module, names)?;
        if union.visibility == Visibility::Public {
            validate_public_type(expression, module, ("union", &qualified), names)?;
        }
        polymorph::bounded_type(&ty, expression.span)?;
        for variable in polymorph::variables(&ty) {
            if !parameters.contains(&variable) {
                if let Some(length) = variable.strip_prefix('#') {
                    return Err(undeclared_length("union", name, length, expression.span));
                }
                return Err(Diagnostic::of(
                    DiagnosticKind::UndeclaredTypeVariable,
                    format!(
                        "type variable '{variable} is not declared by union '{name}'; add it in angle brackets after the union name, as in 'union {name}<'{variable}> = ...'"
                    ),
                    expression.span,
                ));
            }
            used.insert(variable);
        }
        if ty.contains_reference() {
            return Err(Diagnostic::of(
                DiagnosticKind::BorrowedUnionPayload,
                "union payloads are owned values; borrowed payloads require lifetime parameters, which are not supported",
                expression.span,
            ));
        }
        cases.push((case.name.text.clone(), Some(ty)));
    }
    if let Some(parameter) = union
        .parameters
        .iter()
        .find(|parameter| !used.contains(&parameter.text))
    {
        return Err(Diagnostic::of(
            DiagnosticKind::UnusedTypeParameter,
            format!(
                "{} is not used by any case payload; remove it or add a payload that mentions it",
                parameter_name(&parameter.text)
            ),
            parameter.span,
        ));
    }
    Ok(CheckedUnion {
        name: qualified,
        visibility: union.visibility,
        origin: names.origin(module),
        parameters,
        cases,
        span: union.name.span,
        user_drop: false,
        size: None,
        recursive: Default::default(),
    })
}

fn resolve_type(expression: &TypeExpr, module: &str, names: &Names) -> Result<Type, Diagnostic> {
    resolve_type_with_kinds(expression, module, names, &BTreeMap::new())
}

/// Whether an applied type name is a builtin type constructor rather than a declaration:
/// `Vec`, or a shared pointer such as `Rc` or `Arc.Weak` (C10).
fn builtin_type_head(name: &str) -> bool {
    name == "Vec" || SharedKind::named(name).is_some()
}

fn resolve_type_with_kinds(
    expression: &TypeExpr,
    module: &str,
    names: &Names,
    kinds: &BTreeMap<String, usize>,
) -> Result<Type, Diagnostic> {
    let resolve = |ty: &TypeExpr| resolve_type_with_kinds(ty, module, names, kinds);
    Ok(match &expression.kind {
        TypeExprKind::Regions(inner, _) | TypeExprKind::Quantified(_, inner) => resolve(inner)?,
        TypeExprKind::Dyn(dyn_type) => resolve_dyn(dyn_type, module, names)?,
        TypeExprKind::Apply(head, args) if head.text.starts_with('\'') => {
            let variable = &head.text[1..];
            if kinds.get(variable) != Some(&args.len()) || args.is_empty() {
                return Err(Diagnostic::of(
                    DiagnosticKind::ConstructorVariableKindAnnotation,
                    "type constructor variable requires a matching explicit class kind annotation",
                    head.span,
                ));
            }
            Type::Application(
                Box::new(Type::Variable(variable.into())),
                args.iter().map(resolve).collect::<Result<_, _>>()?,
            )
        }
        TypeExprKind::Apply(head, args) if head.text == "Vec" => {
            let [element] = &**args else {
                return Err(Diagnostic::of(
                    DiagnosticKind::TypeArgumentCount,
                    "Vec expects exactly one element type",
                    expression.span,
                ));
            };
            Type::Vec(Box::new(resolve(element)?))
        }
        TypeExprKind::Apply(head, args) if SharedKind::named(&head.text).is_some() => {
            let kind = SharedKind::named(&head.text).expect("checked by the guard");
            let [value] = &**args else {
                return Err(Diagnostic::of(
                    DiagnosticKind::TypeArgumentCount,
                    format!(
                        "{} expects exactly one value type, as in '{}<i64>'",
                        kind.name(),
                        kind.name()
                    ),
                    expression.span,
                ));
            };
            Type::Shared(Box::new(resolve(value)?), kind)
        }
        TypeExprKind::Named(name) if SharedKind::named(name).is_some() => {
            let kind = SharedKind::named(name).expect("checked by the guard");
            return Err(Diagnostic::of(
                DiagnosticKind::TypeArgumentCount,
                format!(
                    "{} expects exactly one value type, as in '{}<i64>'",
                    kind.name(),
                    kind.name()
                ),
                expression.span,
            ));
        }
        TypeExprKind::Named(name) => match crate::numeric::primitive(name) {
            Some(ty) => ty,
            // `bigint` names the standard arbitrary-precision integer.
            None if name == "bigint" => {
                names.std_type("BigInt", "BigInt", Box::default(), expression.span)?
            }
            None => {
                let named = names.named_type(module, name, expression.span)?;
                record_arity(names, named, name, 0, expression.span).map_err(|error| {
                    if matches!(named, NamedType::Alias(_)) {
                        error
                    } else {
                        error.recode(DiagnosticKind::MissingTypeArguments)
                    }
                })?;
                match named {
                    NamedType::Record(info) => Type::Record(info.id, Box::default()),
                    NamedType::Union(info) => Type::Union(info.id, Box::default()),
                    NamedType::Handle(info) => Type::Handle(info.name.as_str().into()),
                    NamedType::Alias(_) => {
                        let expanded = expand_type_aliases(expression, module, names)?;
                        let ty = resolve(&expanded)?;
                        polymorph::bounded_type(&ty, expression.span)?;
                        ty
                    }
                }
            }
        },
        TypeExprKind::Apply(head, args) => {
            if crate::numeric::primitive(&head.text).is_some() {
                return Err(Diagnostic::of(
                    DiagnosticKind::TypeArgumentCount,
                    format!(
                        "type '{}' takes no type arguments; remove the arguments after it",
                        head.text
                    ),
                    expression.span,
                ));
            }
            match names.type_head(module, head)? {
                // `Add<'a>` constrains its argument; `Classes::inline_constraints` records the constraint.
                TypeHead::Class => {
                    let [ty] = &**args else {
                        return Err(Diagnostic::of(
                            DiagnosticKind::ClassConstraintArity,
                            format!(
                                "type class '{}' constrains exactly one type, as in '{}<'a>'",
                                head.text, head.text
                            ),
                            expression.span,
                        ));
                    };
                    resolve(ty)?
                }
                TypeHead::Type(named) => {
                    record_arity(names, named, &head.text, args.len(), expression.span)?;
                    if matches!(named, NamedType::Alias(_)) {
                        let expanded = expand_type_aliases(expression, module, names)?;
                        let ty = resolve(&expanded)?;
                        polymorph::bounded_type(&ty, expression.span)?;
                        return Ok(ty);
                    }
                    let lengths: &[bool] = match named {
                        NamedType::Record(info) => &names.record_lengths[info.id],
                        NamedType::Union(info) => &names.union_lengths[info.id],
                        _ => &[],
                    };
                    let args = args
                        .iter()
                        .enumerate()
                        .map(|(index, arg)| {
                            resolve_argument(
                                arg,
                                lengths.get(index).copied().unwrap_or(false),
                                module,
                                names,
                                kinds,
                            )
                        })
                        .collect::<Result<_, _>>()?;
                    match named {
                        NamedType::Record(info) => Type::Record(info.id, args),
                        NamedType::Union(info) => Type::Union(info.id, args),
                        // `record_arity` rejected the arguments of an extern type.
                        NamedType::Handle(_) => unreachable!("extern types take no arguments"),
                        NamedType::Alias(_) => unreachable!(),
                    }
                }
            }
        }
        TypeExprKind::Variable(name) => {
            if kinds.get(name).is_some_and(|arity| *arity > 0) {
                return Err(Diagnostic::of(
                    DiagnosticKind::TypeConstructorNotApplied,
                    "a type constructor must be fully applied in a value type",
                    expression.span,
                ));
            }
            Type::Variable(name.clone())
        }
        TypeExprKind::Reference(ty, mutable) => match &ty.kind {
            TypeExprKind::ArrayView(element) if *mutable => {
                Type::Reference(Box::new(Type::ArrayView(Box::new(resolve(element)?))), true)
            }
            _ => Type::Reference(Box::new(resolve(ty)?), *mutable),
        },
        TypeExprKind::Own(_) => {
            return Err(Diagnostic::of(
                DiagnosticKind::MisplacedOwnConvention,
                "'own' is only valid on the type of a parameter in a 'def' signature",
                expression.span,
            ));
        }
        TypeExprKind::Mut(_) => {
            return Err(Diagnostic::of(
                DiagnosticKind::MisplacedMutConvention,
                "'mut' is only valid on the type of a parameter in a 'def' signature",
                expression.span,
            ));
        }
        TypeExprKind::Pure(inner) => {
            let mut ty = if let TypeExprKind::Function(parameters, result) = &inner.kind {
                Type::function_with_conventions(
                    parameters.iter().map(resolve).collect::<Result<_, _>>()?,
                    resolve(result)?,
                    Vec::new(),
                    true,
                )
            } else {
                resolve(inner)?
            };
            let Type::Function(parameters, _) = &mut ty else {
                return Err(Diagnostic::of(
                    DiagnosticKind::PureOnNonFunctionType,
                    "'pure' qualifies a function type; write 'pure (T -> U)'",
                    expression.span,
                ));
            };
            parameters.pure = true;
            ty
        }
        TypeExprKind::ArrayView(_) => {
            return Err(Diagnostic::of(
                DiagnosticKind::UnsizedSliceTypePosition,
                "'[T..]' is only valid directly after 'ref mut'; write 'ref mut [T..]' for an exclusive slice, 'ref [T]' for a shared slice, or '[T]' for an owned array",
                expression.span,
            ));
        }
        TypeExprKind::Array(element) => Type::Array(Box::new(resolve(element)?)),
        TypeExprKind::FixedArray(element, length) => {
            let element = resolve(element)?;
            Type::FixedArray(
                Box::new(element),
                Box::new(resolve_length(length, module, names, expression.span)?),
            )
        }
        TypeExprKind::Length(length) => {
            return Err(Diagnostic::of(
                DiagnosticKind::LengthUsedAsType,
                format!(
                    "expected a type, found the length {length}; a length belongs in '[T; N]' or a length parameter 'const N: i64'"
                ),
                expression.span,
            ));
        }
        TypeExprKind::List(element) => Type::List(Box::new(resolve(element)?)),
        TypeExprKind::Tuple(elements) => {
            Type::Tuple(elements.iter().map(resolve).collect::<Result<_, _>>()?)
        }
        TypeExprKind::Task(result) => Type::Task(Box::new(resolve(result)?)),
        TypeExprKind::Function(parameters, result) => Type::function(
            parameters.iter().map(resolve).collect::<Result<_, _>>()?,
            resolve(result)?,
        ),
    })
}

/// The type of `dyn` and its classes (A14): `Copy` and `Send` become markers, and every other
/// name must be a type class. The classes are sorted, so their order in the source never matters.
#[inline(never)]
fn resolve_dyn(expression: &DynTypeExpr, module: &str, names: &Names) -> Result<Type, Diagnostic> {
    let mut classes = BTreeSet::new();
    let (mut copy, mut send) = (false, false);
    for name in &expression.classes {
        let Some(key) = names.class(module, &name.text, name.span)? else {
            let kind = if names.named_type(module, &name.text, name.span).is_ok() {
                format!("'{}' is not a type class", name.text)
            } else {
                format!("unknown type class '{}'", name.text)
            };
            return Err(Diagnostic::of(
                DiagnosticKind::DynNeedsTypeClass,
                format!("{kind}; dyn needs a type class such as dyn Shapes.Shape"),
                name.span,
            ));
        };
        match key {
            "Copy" => copy = true,
            "Send" => send = true,
            key => {
                classes.insert(Box::<str>::from(key));
            }
        }
    }
    Ok(Type::Dyn(Box::new(DynType {
        classes: classes.into_iter().collect(),
        copy,
        send,
        borrowed: expression.borrowed,
    })))
}

/// A type argument, or the length argument of a length parameter `const N: i64` (A16 Phase 2).
fn resolve_argument(
    argument: &TypeExpr,
    length: bool,
    module: &str,
    names: &Names,
    kinds: &BTreeMap<String, usize>,
) -> Result<Type, Diagnostic> {
    if length {
        return resolve_length(argument, module, names, argument.span);
    }
    resolve_type_with_kinds(argument, module, names, kinds)
}

/// The length of `[T; N]` or a length argument (A16): a literal, an `i64` constant with an
/// integer literal value, or a length parameter, which stays `Variable("#N")` until it is
/// substituted. `span` locates the limit error.
fn resolve_length(
    expression: &TypeExpr,
    module: &str,
    names: &Names,
    span: Span,
) -> Result<Type, Diagnostic> {
    let length = match &expression.kind {
        TypeExprKind::Length(length) => *length,
        TypeExprKind::Variable(name) if name.starts_with('#') => {
            return Ok(Type::Variable(name.clone()));
        }
        TypeExprKind::Named(name) => match names.length_constant(module, name, expression.span) {
            Some((_, Some(length))) => length,
            Some((_, None)) => {
                return Err(Diagnostic::of(
                    DiagnosticKind::InvalidLengthConstant,
                    format!(
                        "constant '{name}' cannot be a length; a length constant has type 'i64' and an integer literal value"
                    ),
                    expression.span,
                ));
            }
            None if crate::numeric::primitive(name).is_some()
                || name == "bigint"
                || names.named_type(module, name, expression.span).is_ok() =>
            {
                return Err(Diagnostic::of(
                    DiagnosticKind::TypeUsedAsLength,
                    format!("expected a length, found the type '{name}'"),
                    expression.span,
                ));
            }
            None if !name.contains(['.', ':']) => {
                return Ok(Type::Variable(format!("#{name}")));
            }
            None => {
                return Err(Diagnostic::of(
                    DiagnosticKind::UnknownLengthConstant,
                    format!(
                        "unknown constant '{name}'; a length is an integer literal, an 'i64' constant, or a length parameter"
                    ),
                    expression.span,
                ));
            }
        },
        _ => {
            return Err(Diagnostic::of(
                DiagnosticKind::ExpectedLength,
                "expected a length, such as '3', an 'i64' constant, or a length parameter 'N'",
                expression.span,
            ));
        }
    };
    if length > MAX_FIXED_ARRAY_LENGTH {
        return Err(Diagnostic::of(
            DiagnosticKind::FixedArrayTooLong,
            format!(
                "fixed array length {length} exceeds {MAX_FIXED_ARRAY_LENGTH} elements; use '[T]' for larger arrays"
            ),
            span,
        ));
    }
    Ok(Type::Length(length))
}

fn record_arity(
    names: &Names,
    named: NamedType<'_>,
    name: &str,
    found: usize,
    span: Span,
) -> Result<(), Diagnostic> {
    let expected = match named {
        NamedType::Record(info) => names.record_arities[info.id],
        NamedType::Union(info) => names.union_arities[info.id],
        NamedType::Alias(info) => names.type_aliases[&info.name].1.parameters.len(),
        NamedType::Handle(_) => 0,
    };
    if expected == found {
        return Ok(());
    }
    let message = if expected == 0 {
        format!(
            "type '{name}' takes no type arguments, found {found}; remove the arguments after it"
        )
    } else {
        format!(
            "type '{name}' expects {expected} type argument{}, found {found}; write comma-separated types in '<...>' after the name",
            if expected == 1 { "" } else { "s" }
        )
    };
    let kind = if matches!(named, NamedType::Alias(_)) {
        DiagnosticKind::AliasArgumentCount
    } else {
        DiagnosticKind::TypeArgumentCount
    };
    Err(Diagnostic::of(kind, message, span))
}

fn expand_type_aliases(
    expression: &TypeExpr,
    module: &str,
    names: &Names,
) -> Result<TypeExpr, Diagnostic> {
    TypeAliasExpansion {
        names,
        active: Vec::new(),
        nodes: 0,
    }
    .expand(expression, module, 0)
}

struct TypeAliasExpansion<'a> {
    names: &'a Names,
    active: Vec<String>,
    nodes: usize,
}

impl TypeAliasExpansion<'_> {
    fn expand(
        &mut self,
        expression: &TypeExpr,
        module: &str,
        depth: usize,
    ) -> Result<TypeExpr, Diagnostic> {
        self.nodes += 1;
        if depth > MAX_NESTING || self.nodes > 4096 {
            return Err(Diagnostic::of(
                DiagnosticKind::TypeAliasExpansionLimit,
                "type alias expansion exceeds the compiler limit; simplify the aliases",
                expression.span,
            ));
        }
        let kind = match &expression.kind {
            TypeExprKind::Apply(head, args)
                if builtin_type_head(&head.text) || head.text.starts_with('\'') =>
            {
                TypeExprKind::Apply(
                    head.clone(),
                    args.iter()
                        .map(|arg| self.expand(arg, module, depth + 1))
                        .collect::<Result<_, _>>()?,
                )
            }
            TypeExprKind::Named(name) if crate::numeric::primitive(name).is_none() => {
                let named = self.names.named_type(module, name, expression.span)?;
                if let NamedType::Alias(info) = named {
                    return self.alias(info, &[], module, expression.span, depth);
                }
                TypeExprKind::Named(key_path(&named.info().name))
            }
            TypeExprKind::Apply(head, args) if crate::numeric::primitive(&head.text).is_none() => {
                let (name, lengths) = match self.names.type_head(module, head)? {
                    TypeHead::Type(NamedType::Alias(info)) => {
                        return self.alias(info, args, module, expression.span, depth);
                    }
                    TypeHead::Type(named) => (
                        named.info().name.clone(),
                        self.names.length_parameters(named),
                    ),
                    TypeHead::Class => (
                        self.names
                            .class(module, &head.text, head.span)?
                            .expect("a classified class has a resolved name")
                            .to_owned(),
                        Vec::new(),
                    ),
                };
                TypeExprKind::Apply(
                    Box::new(Ident {
                        text: key_path(&name),
                        span: head.span,
                        provenance: head.provenance,
                    }),
                    self.arguments(args, &lengths, module, depth)?.into(),
                )
            }
            TypeExprKind::Reference(inner, mutable) => {
                TypeExprKind::Reference(Box::new(self.expand(inner, module, depth + 1)?), *mutable)
            }
            TypeExprKind::Own(inner) => {
                TypeExprKind::Own(Box::new(self.expand(inner, module, depth + 1)?))
            }
            TypeExprKind::Mut(inner) => {
                TypeExprKind::Mut(Box::new(self.expand(inner, module, depth + 1)?))
            }
            TypeExprKind::Pure(inner) => {
                TypeExprKind::Pure(Box::new(self.expand(inner, module, depth + 1)?))
            }
            TypeExprKind::Regions(inner, regions) => TypeExprKind::Regions(
                Box::new(self.expand(inner, module, depth + 1)?),
                regions.clone(),
            ),
            TypeExprKind::Quantified(regions, inner) => TypeExprKind::Quantified(
                regions.clone(),
                Box::new(self.expand(inner, module, depth + 1)?),
            ),
            TypeExprKind::Array(inner) => {
                TypeExprKind::Array(Box::new(self.expand(inner, module, depth + 1)?))
            }
            TypeExprKind::ArrayView(inner) => {
                TypeExprKind::ArrayView(Box::new(self.expand(inner, module, depth + 1)?))
            }
            // A length names a constant or a length parameter, never an alias.
            TypeExprKind::FixedArray(element, length) => TypeExprKind::FixedArray(
                Box::new(self.expand(element, module, depth + 1)?),
                length.clone(),
            ),
            TypeExprKind::List(inner) => {
                TypeExprKind::List(Box::new(self.expand(inner, module, depth + 1)?))
            }
            TypeExprKind::Task(inner) => {
                TypeExprKind::Task(Box::new(self.expand(inner, module, depth + 1)?))
            }
            TypeExprKind::Tuple(elements) => TypeExprKind::Tuple(
                elements
                    .iter()
                    .map(|element| self.expand(element, module, depth + 1))
                    .collect::<Result<_, _>>()?,
            ),
            TypeExprKind::Function(parameters, result) => TypeExprKind::Function(
                parameters
                    .iter()
                    .map(|parameter| self.expand(parameter, module, depth + 1))
                    .collect::<Result<_, _>>()?,
                Box::new(self.expand(result, module, depth + 1)?),
            ),
            _ => expression.kind.clone(),
        };
        Ok(TypeExpr {
            kind,
            span: expression.span,
        })
    }

    fn alias(
        &mut self,
        info: &NameInfo,
        args: &[TypeExpr],
        module: &str,
        span: Span,
        depth: usize,
    ) -> Result<TypeExpr, Diagnostic> {
        record_arity(
            self.names,
            NamedType::Alias(info),
            &info.name,
            args.len(),
            span,
        )?;
        let arguments = self.arguments(
            args,
            &self.names.length_parameters(NamedType::Alias(info)),
            module,
            depth,
        )?;
        if self.active.contains(&info.name) {
            return Err(Diagnostic::of(
                DiagnosticKind::CyclicTypeAlias,
                format!(
                    "cyclic type alias '{}'; remove the recursive alias reference",
                    self.names.type_spelling(&info.name)
                ),
                span,
            ));
        }
        self.active.push(info.name.clone());
        let alias = &self.names.type_aliases[&info.name].1;
        let result = self.expand(&alias.target, &info.module, depth + 1);
        self.active.pop();
        let mut result = result?;
        let substitutions = alias
            .parameters
            .iter()
            .map(|parameter| parameter.text.clone())
            .zip(arguments)
            .collect();
        self.substitute(&mut result, &substitutions, depth)?;
        Ok(result)
    }

    /// Expands type arguments; a length argument (A16) names a constant or a length parameter,
    /// never an alias, so it stays as written.
    fn arguments(
        &mut self,
        args: &[TypeExpr],
        lengths: &[bool],
        module: &str,
        depth: usize,
    ) -> Result<Vec<TypeExpr>, Diagnostic> {
        args.iter()
            .enumerate()
            .map(|(index, arg)| {
                if lengths.get(index).copied().unwrap_or(false) {
                    Ok(arg.clone())
                } else {
                    self.expand(arg, module, depth + 1)
                }
            })
            .collect()
    }

    fn substitute(
        &mut self,
        expression: &mut TypeExpr,
        substitutions: &BTreeMap<String, TypeExpr>,
        depth: usize,
    ) -> Result<(), Diagnostic> {
        self.nodes += 1;
        if depth > MAX_NESTING || self.nodes > 4096 {
            return Err(Diagnostic::of(
                DiagnosticKind::TypeAliasExpansionLimit,
                "type alias expansion exceeds the compiler limit; simplify the aliases",
                expression.span,
            ));
        }
        let empty = BTreeMap::new();
        let substitutions = if let TypeExprKind::Variable(name) = &expression.kind
            && let Some(argument) = substitutions.get(name)
        {
            *expression = argument.clone();
            &empty
        } else if let TypeExprKind::Named(name) = &expression.kind
            && let Some(argument) = substitutions.get(&format!("#{name}"))
        {
            // The alias's length parameter `const N: i64` is written `N` (A16).
            *expression = argument.clone();
            &empty
        } else {
            substitutions
        };
        let children: Vec<&mut TypeExpr> = match &mut expression.kind {
            TypeExprKind::Apply(_, args) => args.iter_mut().collect(),
            TypeExprKind::Regions(inner, _)
            | TypeExprKind::Own(inner)
            | TypeExprKind::Mut(inner)
            | TypeExprKind::Pure(inner)
            | TypeExprKind::Quantified(_, inner)
            | TypeExprKind::Reference(inner, _)
            | TypeExprKind::Array(inner)
            | TypeExprKind::ArrayView(inner)
            | TypeExprKind::List(inner)
            | TypeExprKind::Task(inner) => vec![inner],
            TypeExprKind::FixedArray(element, length) => vec![element, length],
            TypeExprKind::Tuple(elements) => elements.iter_mut().collect(),
            TypeExprKind::Function(parameters, result) => parameters
                .iter_mut()
                .chain(std::iter::once(result.as_mut()))
                .collect(),
            TypeExprKind::Named(_)
            | TypeExprKind::Variable(_)
            | TypeExprKind::Length(_)
            | TypeExprKind::Dyn(_) => Vec::new(),
        };
        for child in children {
            self.substitute(child, substitutions, depth + 1)?;
        }
        Ok(())
    }
}

/// Record fields and union payloads describe stored values, so they cannot
/// carry the inline class constraints that function signatures accept.
fn reject_field_constraints(
    expression: &TypeExpr,
    module: &str,
    names: &Names,
    owner: &str,
) -> Result<(), Diagnostic> {
    if names.type_aliases.is_empty() {
        return reject_expanded_field_constraints(expression, module, names, owner);
    }
    let expanded = expand_type_aliases(expression, module, names)?;
    reject_expanded_field_constraints(&expanded, module, names, owner)
}

fn reject_expanded_field_constraints(
    expression: &TypeExpr,
    module: &str,
    names: &Names,
    owner: &str,
) -> Result<(), Diagnostic> {
    let children: Vec<&TypeExpr> = match &expression.kind {
        TypeExprKind::Apply(head, args) => {
            if !builtin_type_head(&head.text)
                && matches!(names.type_head(module, head), Ok(TypeHead::Class))
            {
                let parts = if owner == "union" {
                    "union payloads"
                } else {
                    "record fields"
                };
                return Err(Diagnostic::of(
                    DiagnosticKind::FieldTypeConstraint,
                    format!(
                        "{parts} cannot constrain type parameters; put the constraint on the functions that use the {owner}"
                    ),
                    head.span,
                ));
            }
            args.iter().collect()
        }
        TypeExprKind::Regions(inner, _)
        | TypeExprKind::Own(inner)
        | TypeExprKind::Mut(inner)
        | TypeExprKind::Pure(inner)
        | TypeExprKind::Quantified(_, inner)
        | TypeExprKind::Reference(inner, _)
        | TypeExprKind::Array(inner)
        | TypeExprKind::ArrayView(inner)
        | TypeExprKind::FixedArray(inner, _)
        | TypeExprKind::List(inner)
        | TypeExprKind::Task(inner) => vec![inner],
        TypeExprKind::Tuple(elements) => elements.iter().collect(),
        TypeExprKind::Function(parameters, result) => parameters
            .iter()
            .chain(std::iter::once(result.as_ref()))
            .collect(),
        TypeExprKind::Named(_)
        | TypeExprKind::Variable(_)
        | TypeExprKind::Length(_)
        | TypeExprKind::Dyn(_) => Vec::new(),
    };
    children
        .into_iter()
        .try_for_each(|child| reject_expanded_field_constraints(child, module, names, owner))
}

fn size_error(span: Span) -> Diagnostic {
    Diagnostic::of(
        DiagnosticKind::ValueLayoutTooLarge,
        format!("value layout exceeds {MAX_VALUE_BYTES} bytes; use smaller value types"),
        span,
    )
}

/// Measures value layouts per concrete type. Non-generic records are
/// measured once when they are declared; a generic record instance is
/// measured when it is used because its size depends on its arguments.
/// Heap elements are visited to reject recursive types, but function,
/// task, and reference targets do not belong to the layout.
struct Layouts<'a> {
    types: TypeContext<'a>,
    sizes: BTreeMap<Type, usize>,
    visiting: BTreeSet<Type>,
}

impl<'a> Layouts<'a> {
    fn new(types: TypeContext<'a>) -> Self {
        Self {
            types,
            sizes: BTreeMap::new(),
            visiting: BTreeSet::new(),
        }
    }

    fn record(&mut self, ty: &Type, depth: usize, span: Span) -> Result<usize, Diagnostic> {
        self.types.recursive_checked(ty, span)?;
        let Type::Record(id, args) = ty else {
            unreachable!()
        };
        let record = &self.types.records[*id];
        if args.is_empty() {
            if let Some(size) = record.size {
                return Ok(size);
            }
        }
        if let Some(size) = self.sizes.get(ty) {
            return Ok(*size);
        }
        let span = if args.is_empty() { record.span } else { span };
        if !self.visiting.insert(ty.clone()) {
            return Err(Diagnostic::of(
                DiagnosticKind::InfiniteRecursiveLayout,
                format!(
                    "recursive value layout for '{}'; recursive heap types are not supported in 0.1",
                    record.name
                ),
                span,
            ));
        }
        if depth > MAX_NESTING {
            return Err(Diagnostic::of(
                DiagnosticKind::LayoutNestingLimit,
                format!("record nesting exceeds {MAX_NESTING}"),
                span,
            ));
        }
        let mut size: usize = 0;
        for field in self.types.record_fields(*id, args) {
            // Substitution can grow types; bound them before measuring.
            polymorph::bounded_type(&field, span)?;
            size = size.saturating_add(self.size(&field, depth + 1, span)?.next_multiple_of(16));
            if size > MAX_VALUE_BYTES {
                return Err(size_error(span));
            }
        }
        self.visiting.remove(ty);
        self.sizes.insert(ty.clone(), size);
        Ok(size)
    }

    /// A union value is its tag and the largest payload: an enum takes one
    /// pointer-sized slot, and payload storage adds 16 tag bytes to the
    /// largest payload rounded up to 16 bytes.
    fn union(&mut self, ty: &Type, depth: usize, span: Span) -> Result<usize, Diagnostic> {
        if self.types.recursive_checked(ty, span)? {
            return Ok(8);
        }
        let Type::Union(id, args) = ty else {
            unreachable!()
        };
        let union = &self.types.unions[*id];
        if args.is_empty() {
            if let Some(size) = union.size {
                return Ok(size);
            }
        }
        if let Some(size) = self.sizes.get(ty) {
            return Ok(*size);
        }
        let span = if args.is_empty() { union.span } else { span };
        if !self.visiting.insert(ty.clone()) {
            return Err(Diagnostic::of(
                DiagnosticKind::InfiniteRecursiveLayout,
                format!(
                    "recursive union layout for '{}'; recursive heap types are not supported in 0.1",
                    union.name
                ),
                span,
            ));
        }
        if depth > MAX_NESTING {
            return Err(Diagnostic::of(
                DiagnosticKind::LayoutNestingLimit,
                format!("union nesting exceeds {MAX_NESTING}"),
                span,
            ));
        }
        let mut payload = None;
        for ty in self.types.union_payloads(*id, args).into_iter().flatten() {
            polymorph::bounded_type(&ty, span)?;
            let size = self.size(&ty, depth + 1, span)?;
            payload = Some(payload.unwrap_or(0).max(size));
        }
        let size = payload.map_or(8, |size: usize| {
            16usize.saturating_add(size.next_multiple_of(16))
        });
        if size > MAX_VALUE_BYTES {
            return Err(size_error(span));
        }
        self.visiting.remove(ty);
        self.sizes.insert(ty.clone(), size);
        Ok(size)
    }

    fn size(&mut self, ty: &Type, depth: usize, span: Span) -> Result<usize, Diagnostic> {
        if ty.contains_error() {
            return Ok(0);
        }
        Ok(match ty {
            // A 256-bit vector fills 32 bytes (F08); a mask's packed bits count as a small value.
            Type::Simd(vector) => vector.bytes().max(8),
            Type::Vec(element) => {
                self.size(element, depth, span)?;
                32
            }
            // The value lives in the shared block, which may hold the type being measured (C10).
            Type::Shared(..) => 8,
            Type::Reference(..) if ty.slice_element().is_some() => 16,
            Type::Record(..) => self.record(ty, depth, span)?,
            Type::Union(..) => self.union(ty, depth, span)?,
            Type::Array(element) | Type::List(element) => {
                self.size(element, depth, span)?;
                16
            }
            // Elements are inline at their own stride, so no per-element rounding (A16); a
            // length parameter counts as empty until specialization substitutes it.
            Type::FixedArray(element, _) => {
                let length = usize::try_from(ty.fixed_length().unwrap_or(0)).unwrap_or(usize::MAX);
                let size = self.size(element, depth + 1, span)?.saturating_mul(length);
                if size > MAX_VALUE_BYTES {
                    return Err(size_error(span));
                }
                size
            }
            Type::Tuple(elements) => {
                let mut size = 0usize;
                for ty in elements {
                    size =
                        size.saturating_add(self.size(ty, depth + 1, span)?.next_multiple_of(16));
                    if size > MAX_VALUE_BYTES {
                        return Err(size_error(span));
                    }
                }
                size
            }
            Type::Integer(128, _)
            | Type::Binary(128)
            | Type::Decimal(128)
            | Type::String
            | Type::Utf8String
            | Type::Dyn(_) => 16,
            Type::Function(..) | Type::Task(_) => 32,
            // Small values conservatively occupy at least one pointer-sized slot.
            _ => 8,
        })
    }
}

/// Validates a type at a construction boundary: value size, deep collection
/// immutability, owned task results, and record instances whose type
/// arguments would put a reference into a field.
fn validate_size(ty: &Type, types: &TypeContext<'_>, span: Span) -> Result<usize, Diagnostic> {
    if ty.contains_error() {
        return Ok(0);
    }
    Validation {
        layouts: Layouts::new(*types),
        instances: BTreeSet::new(),
    }
    .check(ty, span)
}

struct Validation<'a> {
    layouts: Layouts<'a>,
    instances: BTreeSet<Type>,
}

/// A13: the regions of each record that hold an exclusive borrow and the regions of the borrows
/// behind each region's references, to a fixed point over nested records. A pass only adds bits,
/// so the loop ends even before layouts reject by-value cycles.
fn mark_exclusive_regions(records: &mut [CheckedRecord]) {
    let bits = |mask: RegionMask| (0..MAX_RECORD_REGIONS).filter(move |bit| mask & (1 << bit) != 0);
    loop {
        let mut changed = false;
        for id in 0..records.len() {
            let record = &records[id];
            let mut exclusive = record.exclusive_regions;
            let mut targets = record.region_targets.clone();
            for (index, (_, ty)) in record.fields.iter().enumerate() {
                let (slots, inner) = match ty {
                    Type::Reference(_, true) => (1, None),
                    Type::Record(inner, _) => (
                        records[*inner].exclusive_regions,
                        Some(&records[*inner].region_targets),
                    ),
                    _ => (0, None),
                };
                exclusive |= field_slot_regions(record, index, slots);
                if record.region_count < 2 {
                    continue;
                }
                let masks = &record.field_regions[index];
                for region in bits(masks[0]) {
                    targets[region] |= record.field_targets[index];
                }
                if let Some(inner) = inner
                    && masks.len() >= 2
                    && masks.len() == inner.len()
                {
                    for (slot, inner_targets) in inner.iter().enumerate() {
                        let mapped =
                            bits(*inner_targets).fold(0, |all, target| all | masks[target]);
                        for region in bits(masks[slot]) {
                            targets[region] |= mapped;
                        }
                    }
                }
            }
            if exclusive != record.exclusive_regions || targets != record.region_targets {
                changed = true;
                records[id].exclusive_regions = exclusive;
                records[id].region_targets = targets;
            }
        }
        if !changed {
            return;
        }
    }
}

/// The regions of `record` that hold the region slots `slots` of the value of its field `index`.
fn field_slot_regions(record: &CheckedRecord, index: usize, slots: RegionMask) -> RegionMask {
    match record.region_count {
        _ if slots == 0 => 0,
        0 => 0,
        1 => 1,
        _ => {
            let masks = &record.field_regions[index];
            masks
                .iter()
                .enumerate()
                .filter(|(slot, _)| masks.len() < 2 || slots & (1 << slot) != 0)
                .fold(0, |all, (_, mask)| all | mask)
        }
    }
}

/// A13: a record field holds an exclusive borrow only as a direct `ref mut {r} T` to data without
/// borrows or as a record with exclusive regions; `regions::validate_modules` checks the regions.
fn validate_exclusive_field(
    record: &CheckedRecord,
    index: usize,
    types: &TypeContext<'_>,
    span: Span,
) -> Result<(), Diagnostic> {
    let (name, ty) = &record.fields[index];
    let named_target = record
        .field_targets
        .get(index)
        .is_some_and(|mask| *mask != 0);
    let message = match ty {
        Type::Reference(target, true) if target.carries_loans(types) && !named_target => format!(
            "exclusive borrow field '{name}' must point to data without borrows; store borrowed parts in separate fields, or name the target's region as in 'ref mut {{r}} T {{s}}'"
        ),
        Type::Reference(_, true) => return Ok(()),
        Type::Record(id, _) if types.records[*id].exclusive_regions != 0 => return Ok(()),
        _ if ty.contains_stored_mutable_reference(types) || ty.reaches_exclusive(types) => {
            "record fields can hold an exclusive reference only as a direct 'ref mut {r} T' field or inside a record field; arrays, lists, Vec, tuples, union payloads, and shared references cannot hold one".to_owned()
        }
        _ => return Ok(()),
    };
    Err(Diagnostic::of(
        DiagnosticKind::ExclusiveBorrowFieldShape,
        message,
        span,
    ))
}

/// A13: why the field `index` of the record instance `owner` cannot hold its substituted type
/// `field`, given the declared type `declared`.
#[inline(never)]
fn instance_field_error(
    owner: &Type,
    record: &CheckedRecord,
    index: usize,
    field: &Type,
    types: &TypeContext<'_>,
) -> Option<String> {
    let (name, declared) = &record.fields[index];
    let named_target = record
        .field_targets
        .get(index)
        .is_some_and(|mask| *mask != 0);
    match (declared, field) {
        (Type::Reference(_, true), Type::Reference(target, true))
            if named_target && target.reaches_exclusive(types) =>
        {
            Some(format!(
                "{} would store exclusive borrows behind the exclusive field '{name}'; the target of a field with a named target region holds shared borrows only",
                owner.display(types)
            ))
        }
        (Type::Reference(_, true), Type::Reference(target, true)) => {
            (!named_target && target.carries_loans(types)).then(|| format!(
                "{} would store borrowed data behind the exclusive field '{name}'; exclusive borrow fields must point to data without borrows",
                owner.display(types)
            ))
        }
        (Type::Record(id, _), _) if types.records[*id].exclusive_regions != 0 => None,
        _ if field.contains_stored_mutable_reference(types) || field.reaches_exclusive(types) => {
            Some(format!(
                "{} would store a mutable reference in field '{name}'; use a shared borrow",
                owner.display(types)
            ))
        }
        _ => None,
    }
}

impl Validation<'_> {
    fn check(&mut self, ty: &Type, span: Span) -> Result<usize, Diagnostic> {
        let size = match ty {
            Type::Record(id, args) => {
                // Declared fields were validated with the record; an instance
                // only needs its substituted fields checked, once per call.
                if self.instances.insert(ty.clone()) {
                    let types = self.layouts.types;
                    let record = &types.records[*id];
                    for (index, field) in types.record_fields(*id, args).into_iter().enumerate() {
                        polymorph::bounded_type(&field, span)?;
                        if let Some(message) =
                            instance_field_error(ty, record, index, &field, &types)
                        {
                            return Err(Diagnostic::of(
                                DiagnosticKind::ExclusiveBorrowFieldShape,
                                message,
                                span,
                            ));
                        }
                        self.check(&field, span)?;
                    }
                }
                self.layouts.record(ty, 0, span)?
            }
            Type::Union(id, args) => {
                if self.instances.insert(ty.clone()) {
                    let types = self.layouts.types;
                    let union = &types.unions[*id];
                    for ((name, _), payload) in
                        union.cases.iter().zip(types.union_payloads(*id, args))
                    {
                        let Some(payload) = payload else {
                            continue;
                        };
                        polymorph::bounded_type(&payload, span)?;
                        if payload.contains_stored_mutable_reference(&types) {
                            return Err(Diagnostic::of(
                                DiagnosticKind::UnionPayloadExclusiveBorrow,
                                format!(
                                    "union payloads cannot store mutable references; {} would store one in case '{name}'; use a shared borrow",
                                    ty.display(&types)
                                ),
                                span,
                            ));
                        }
                        self.check(&payload, span)?;
                    }
                }
                self.layouts.union(ty, 0, span)?
            }
            Type::Tuple(elements) => {
                let mut size = 0usize;
                for ty in elements {
                    size = size.saturating_add(self.check(ty, span)?.next_multiple_of(16));
                }
                size
            }
            Type::Array(element)
            | Type::ArrayView(element)
            | Type::FixedArray(element, _)
            | Type::List(element)
            | Type::Vec(element) => {
                if element.contains_stored_mutable_reference(&self.layouts.types) {
                    return Err(Diagnostic::of(
                        DiagnosticKind::CollectionElementExclusiveBorrow,
                        "array and list elements cannot contain mutable references; collections cannot hold exclusive borrows, so keep exclusive slices in locals or split them with 'Array.split_at_mut'",
                        span,
                    ));
                }
                let element_size = self.check(element, span)?;
                match ty {
                    Type::Vec(_) => 32,
                    Type::FixedArray(..) => {
                        let length =
                            usize::try_from(ty.fixed_length().unwrap_or(0)).unwrap_or(usize::MAX);
                        let size = element_size.saturating_mul(length);
                        if size > MAX_VALUE_BYTES {
                            return Err(size_error(span));
                        }
                        size
                    }
                    _ => 16,
                }
            }
            Type::Function(parameters, result) => {
                if parameters.pure {
                    pure_rules::check_function_type(ty, &self.layouts.types, span)?;
                }
                for parameter in parameters.iter() {
                    self.check(parameter, span)?;
                }
                self.check(result, span)?;
                32
            }
            Type::Shared(value, kind) => {
                if value.contains_stored_mutable_reference(&self.layouts.types) {
                    return Err(Diagnostic::of(
                        DiagnosticKind::SharedContainerExclusiveBorrow,
                        format!(
                            "{} cannot hold a mutable reference; its owners only read the shared value, so share the data itself or keep the exclusive borrow in a local",
                            kind.name()
                        ),
                        span,
                    ));
                }
                self.check(value, span)?;
                8
            }
            Type::Task(result) => {
                if result.contains_stored_reference(&self.layouts.types) {
                    return Err(Diagnostic::of(
                        DiagnosticKind::TaskResultIsReference,
                        "task results must be owned values, not references",
                        span,
                    ));
                }
                self.check(result, span)?;
                32
            }
            Type::Reference(value, _) => {
                self.check(value, span)?;
                if ty.slice_element().is_some() { 16 } else { 8 }
            }
            // The same sizes as `Layouts::size`, which fixed-length arrays multiply (A16).
            Type::Simd(vector) => vector.bytes().max(8),
            Type::Integer(128, _)
            | Type::Binary(128)
            | Type::Decimal(128)
            | Type::String
            | Type::Utf8String
            | Type::Dyn(_) => 16,
            _ => 8,
        };
        if size > MAX_VALUE_BYTES {
            Err(size_error(span))
        } else {
            Ok(size)
        }
    }
}

fn expression_path(expression: &Expr) -> Option<(String, &Ident)> {
    let mut names = Vec::new();
    let mut current = expression;
    while let ExprKind::Field(value, field) = &current.kind {
        names.push(field.text.as_str());
        current = value;
    }
    let ExprKind::Name(root) = &current.kind else {
        return None;
    };
    names.push(&root.text);
    names.reverse();
    Some((names.join("."), root))
}

struct ActiveResult {
    union_id: usize,
    arguments: Box<[Type]>,
    cases: BTreeMap<String, usize>,
}

#[derive(Clone, Copy)]
struct RecoveryMark {
    scopes: usize,
    normal_loop_depth: usize,
    computation_depth: usize,
}

/// A source name that the typed tree does not keep, recorded for the semantic index.
#[derive(Clone, Copy, Debug)]
pub(crate) enum NameTarget {
    Function(usize),
    Record(usize),
    Union(usize),
    Field(usize, usize),
    Case(usize, usize),
    /// A later OR-pattern alternative binding an earlier alternative's local.
    Local(usize),
    Builtin(Builtin),
}

struct Checker<'a> {
    module: &'a str,
    names: &'a Names,
    types: TypeContext<'a>,
    signatures: &'a [Scheme],
    classes: &'a Classes,
    inference: Inference,
    constraints: Vec<Constraint>,
    members: Vec<polymorph::MemberConstraint>,
    type_parameters: Vec<String>,
    active_result: Option<Box<ActiveResult>>,
    scopes: Vec<BTreeMap<String, Local>>,
    kinds: BTreeMap<String, usize>,
    next_local: usize,
    normal_loop_depth: usize,
    computation_depth: usize,
    /// Operands of keyword `ref` whose type was still unknown; `finish` rejects any that became references.
    undecided_borrows: Vec<(Type, Span)>,
    /// Format specs of interpolation holes; `finish` checks them against the final hole types.
    format_specs: Vec<(Type, FormatSpec)>,
    /// Values of `use` bindings whose `Drop` requirement waits for inference.
    use_bindings: Vec<Span>,
    /// The type of the value that `xs |> f a` passes to the call `f a`.
    piped: Option<Type>,
    /// Spans of unsuffixed integer literals whose type waits for inference.
    integer_literals: Vec<Span>,
    /// Builtin result types that wait for a concrete integer argument type.
    families: Vec<polymorph::Family>,
    /// Explicit matches and function guards, in the order the checker reaches
    /// them; `check_coverage` inspects them once inference has finished.
    coverage: Vec<exhaustiveness::MatchCoverage>,
    shadowing_warnings: Vec<Diagnostic>,
    /// Locals that alias borrowed storage: `for...in` elements, match
    /// subjects bound to such a place, and pattern variables that may view it.
    borrowed: BTreeSet<usize>,
    default_reads: BTreeMap<usize, (String, String, bool)>,
    read_value_parameters: BTreeSet<usize>,
    read_conventions: bool,
    pure_values: bool,
    poisoned: bool,
    recovering: bool,
    recovered: Vec<Diagnostic>,
    indexing: bool,
    name_uses: Vec<(Span, NameTarget)>,
    /// Inside `@checked`: integer `+ - * **` and negation raise OverflowException.
    checked_arithmetic: bool,
    /// The callee being checked is `Dyn.of` applied to one argument, the only place it may appear (A14).
    dyn_callee: bool,
}

impl<'a> Checker<'a> {
    fn new(
        module: &'a str,
        names: &'a Names,
        types: TypeContext<'a>,
        signatures: &'a [Scheme],
        classes: &'a Classes,
    ) -> Self {
        Self {
            module,
            names,
            types,
            signatures,
            classes,
            inference: Inference::default(),
            constraints: Vec::new(),
            members: Vec::new(),
            type_parameters: Vec::new(),
            active_result: None,
            scopes: vec![BTreeMap::new()],
            kinds: BTreeMap::new(),
            next_local: 0,
            normal_loop_depth: 0,
            computation_depth: 0,
            undecided_borrows: Vec::new(),
            format_specs: Vec::new(),
            use_bindings: Vec::new(),
            piped: None,
            integer_literals: Vec::new(),
            families: Vec::new(),
            coverage: Vec::new(),
            shadowing_warnings: Vec::new(),
            borrowed: BTreeSet::new(),
            default_reads: BTreeMap::new(),
            read_value_parameters: BTreeSet::new(),
            read_conventions: names.borrowing_modules.contains(module),
            pure_values: false,
            poisoned: false,
            recovering: false,
            recovered: Vec::new(),
            indexing: false,
            name_uses: Vec::new(),
            checked_arithmetic: false,
            dyn_callee: false,
        }
    }

    /// Records the last segment of a possibly qualified name.
    #[inline(never)]
    fn note_name(&mut self, name: &Ident, target: NameTarget) {
        if self.indexing && name.provenance == Provenance::User {
            let last = name.text.rsplit(['.', ':']).next().unwrap_or(&name.text);
            let start = name.span.end.saturating_sub(last.len());
            self.name_uses.push((Span { start, ..name.span }, target));
        }
    }

    /// Records a case name and a `Union.Case` qualifier spelled in one dotted name.
    #[inline(never)]
    fn note_case(&mut self, name: &Ident, union: usize, case: usize) {
        if !self.indexing || name.provenance == Provenance::Generated {
            return;
        }
        self.note_name(name, NameTarget::Case(union, case));
        let mut segments = name.text.rsplit('.');
        let last = segments.next().unwrap_or_default();
        let short = self.types.unions[union].name.rsplit('.').next();
        if let Some(qualifier) = segments
            .next()
            .and_then(|segment| segment.rsplit(':').next())
            .filter(|segment| Some(*segment) == short)
        {
            let end = name.span.end.saturating_sub(last.len() + 1);
            let start = end.saturating_sub(qualifier.len());
            let span = Span {
                start,
                end,
                ..name.span
            };
            self.name_uses.push((span, NameTarget::Union(union)));
        }
    }

    /// Records `Case`, `Union.Case`, and `Module.Union.Case` written as field paths.
    #[inline(never)]
    fn note_case_path(&mut self, expression: &Expr, union: usize, case: usize) {
        let ExprKind::Field(value, field) = &expression.kind else {
            return;
        };
        self.note_name(field, NameTarget::Case(union, case));
        let short = self.types.unions[union].name.rsplit('.').next();
        let qualifier = match &value.kind {
            ExprKind::Name(name) | ExprKind::Field(_, name) => name,
            _ => return,
        };
        if qualifier.text.rsplit(':').next() == short {
            self.note_name(qualifier, NameTarget::Union(union));
        }
    }

    fn use_active_result(&mut self, result: &Type, active: &crate::syntax::ActivePattern) {
        if let Type::Union(union_id, arguments) = result {
            self.active_result = Some(Box::new(ActiveResult {
                union_id: *union_id,
                arguments: arguments.clone(),
                cases: active
                    .cases
                    .iter()
                    .enumerate()
                    .map(|(index, case)| (case.text.clone(), index))
                    .collect(),
            }));
        }
    }

    fn bind(&mut self, name: &Ident, ty: Type, mutable: bool) -> Local {
        if self.names.warning_options.shadowing
            && name.provenance == Provenance::User
            && !name.text.starts_with('_')
            && self
                .scopes
                .last()
                .and_then(|scope| scope.get(&name.text))
                .is_some_and(|previous| previous.provenance == Provenance::User)
        {
            self.shadowing_warnings.push(Diagnostic::of(
                DiagnosticKind::ShadowedLocal,
                format!(
                    "local '{}' shadows an earlier binding in the same scope",
                    name.text
                ),
                name.span,
            ));
        }
        let local = Local {
            id: self.next_local,
            ty,
            name: name.text.clone(),
            mutable,
            span: name.span,
            provenance: name.provenance,
        };
        self.next_local += 1;
        if name.text != "_" {
            self.scopes
                .last_mut()
                .unwrap()
                .insert(name.text.clone(), local.clone());
        }
        local
    }

    /// Adds `ExpectedBecause` to the `E1003` mismatches that checking one expression reported, in
    /// `error` and in `self.recovered[from..]`, when they are at one of `tails`: the expression
    /// itself or a position that its expected type is passed down to. The callers call it only
    /// when the check reported an error, and keep the checked expression out of it, so that the
    /// recursive checks keep their frames.
    #[cold]
    #[inline(never)]
    fn expected_because(
        &mut self,
        error: Option<&mut Diagnostic>,
        from: usize,
        tails: &[Span],
        declared: &Type,
        message: &str,
        because: Span,
    ) {
        // The declaration supplied the expected type only when the mismatch expects that type
        // (or, for a reference, its target, which an argument is borrowed for): an operator in
        // the tail position expects its own operand types, as `!` does in `let x: i64 = !1i64`.
        let declared = self.inference.resolve(declared);
        let mut types = vec![declared.display(&self.types)];
        if let Type::Reference(target, _) | Type::Shared(target, _) = &declared {
            types.push(self.inference.resolve(target).display(&self.types));
        }
        let note = |diagnostic: &mut Diagnostic| {
            if diagnostic.code == "E1003"
                && diagnostic
                    .detail
                    .expected
                    .as_ref()
                    .is_some_and(|expected| types.contains(expected))
                && diagnostic.related.is_empty()
                && because.end > because.start
                && tails.contains(&diagnostic.span)
            {
                diagnostic.add_related(RelatedRole::ExpectedBecause, message, because);
            }
        };
        for diagnostic in self.recovered.iter_mut().skip(from) {
            note(diagnostic);
        }
        if let Some(diagnostic) = error {
            note(diagnostic);
        }
    }

    /// `expected_because` for the value of a `let` with a type annotation.
    #[cold]
    #[inline(never)]
    fn annotation_because(
        &mut self,
        binding: &Binding,
        annotation: Option<&Type>,
        from: usize,
        error: Option<&mut Diagnostic>,
    ) {
        // A generated binding's annotation is not written in the source.
        if let (Some(written), Some(annotation)) = (&binding.annotation, annotation)
            && binding.name.provenance == Provenance::User
            && !binding.name.text.starts_with('$')
        {
            self.expected_because(
                error,
                from,
                &tail_spans(&binding.value),
                annotation,
                "the type annotation declares this type",
                written.span,
            );
        }
    }

    /// `expected_because` for an argument of a call: the declared type of the parameter of a
    /// named function.
    #[cold]
    #[inline(never)]
    fn argument_because(
        &mut self,
        callee: &TypedExpr,
        index: usize,
        argument: &Expr,
        parameter: &Type,
        from: usize,
        error: Option<&mut Diagnostic>,
    ) {
        if let Some((message, span)) = self.declared_parameter(callee, index) {
            self.expected_because(
                error,
                from,
                &tail_spans(argument),
                parameter,
                &format!("{message} is declared here"),
                span,
            );
        }
    }

    /// `parameter {n} of '{name}'` and the span of that parameter's declared type, when `callee`
    /// is a named function.
    fn declared_parameter(&self, callee: &TypedExpr, index: usize) -> Option<(String, Span)> {
        let (TypedExprKind::Function(FunctionRef::User(id))
        | TypedExprKind::GenericFunction(id, _)) = &callee.kind
        else {
            return None;
        };
        let declared = self.names.declared.get(*id)?;
        let span = *declared.parameters.get(index)?;
        (!declared.name.starts_with('$')).then(|| {
            (
                format!("parameter {} of '{}'", index + 1, declared.name),
                span,
            )
        })
    }

    /// An `E1003` for a call whose value is a function because it left out trailing arguments:
    /// it passes fewer arguments than `callee` takes. A call that passes them all and returns a
    /// function, or a function without parameters (`fn() -> T`), needs the result called instead.
    #[cold]
    #[inline(never)]
    fn partial_application(
        &self,
        mut error: Diagnostic,
        callee: &Type,
        passed: usize,
        result: &Type,
        hint: &Type,
    ) -> Diagnostic {
        if error.code == "E1003"
            && matches!(self.inference.resolve(callee), Type::Function(declared, _) if declared.len() > passed)
            && let Type::Function(missing, _) = self.inference.resolve(result)
            && !missing.is_empty()
            && !matches!(
                self.inference.resolve(hint),
                Type::Function(..) | Type::Infer(_) | Type::Error
            )
        {
            error.help = Some(format!(
                "a call with too few arguments evaluates to a function; add the missing argument(s) ({} more needed)",
                missing.len()
            ));
        }
        error
    }

    /// The locals in scope, innermost first, at most `MAX_SCOPE`.
    fn scope_entries(&self) -> Vec<ScopeEntry> {
        let mut seen = BTreeSet::new();
        let mut entries = Vec::new();
        for scope in self.scopes.iter().rev() {
            for (name, local) in scope {
                if entries.len() == crate::diagnostic::MAX_SCOPE {
                    return entries;
                }
                if name == "_"
                    || name.starts_with('$')
                    || local.provenance != Provenance::User
                    || !seen.insert(name.clone())
                {
                    continue;
                }
                entries.push(ScopeEntry {
                    name: name.clone(),
                    ty: self.inference.resolve(&local.ty).display(&self.types),
                });
            }
        }
        entries.truncate(crate::diagnostic::MAX_SCOPE);
        entries
    }

    fn same(&mut self, actual: &Type, expected: &Type, span: Span) -> Result<(), Diagnostic> {
        self.inference.unify(actual, expected, &self.types, span)
    }

    fn expression(
        &mut self,
        expression: &Expr,
        expected: Option<&Type>,
    ) -> Result<TypedExpr, Diagnostic> {
        if expected.is_some_and(Type::contains_error) {
            self.poisoned = true;
            // During recovery an erroneous context is unknown: still check the operand unless its type needs that context.
            if !self.recovering
                || Self::untyped_number(expression)
                || matches!(expression.kind, ExprKind::Computation(..))
            {
                return Ok(TypedExpr::error(expression.span));
            }
        }
        let mark = RecoveryMark {
            scopes: self.scopes.len(),
            normal_loop_depth: self.normal_loop_depth,
            computation_depth: self.computation_depth,
        };
        // Continuations must not retain the large value-checking frame at every recursive step.
        let result = match expression.kind {
            ExprKind::Computation(ref builder, ref body) => {
                computation::check_implicit(self, builder, body, expected)
            }
            ExprKind::ComputationBoundary(ref body) => {
                let outer = std::mem::take(&mut self.normal_loop_depth);
                let result = self.expression(body, expected);
                self.normal_loop_depth = outer;
                result
            }
            ExprKind::While { .. } | ExprKind::For { .. } | ExprKind::Match { .. } => {
                self.control_expression(expression, expected)
            }
            ExprKind::Binary(..) => self.binary_expression(expression, expected),
            ExprKind::Lambda(..)
            | ExprKind::Task(_)
            | ExprKind::Call(..)
            | ExprKind::Block { .. } => self.composed_expression(expression, expected),
            _ => self.value_expression(expression, expected),
        };
        let result = result.map_err(|mut error| {
            if error.code == "E1003"
                && let Some(expected) = expected
                && let ExprKind::Name(root) = &expression.kind
                && let Some(local) = self
                    .scopes
                    .iter()
                    .rev()
                    .find_map(|scope| scope.get(&root.text))
                && let Type::Reference(inner, false) = self.inference.resolve(&local.ty)
                && *inner == self.inference.resolve(expected)
                && let Some((name, ty, pure)) = self.default_reads.get(&local.id)
            {
                read_only_hint(&mut error, name, ty, *pure);
            }
            error
        });
        self.finish_recovery(result, mark, expression.span)
    }

    fn finish_recovery(
        &mut self,
        result: Result<TypedExpr, Diagnostic>,
        mark: RecoveryMark,
        span: Span,
    ) -> Result<TypedExpr, Diagnostic> {
        match result {
            Err(error) if self.recovering => {
                self.scopes.truncate(mark.scopes);
                self.normal_loop_depth = mark.normal_loop_depth;
                self.computation_depth = mark.computation_depth;
                self.recover_expression(error, span)
            }
            result => result,
        }
    }

    // A broken sibling keeps a valid hint, otherwise it passes its Error on as an unknown context.
    fn sibling_hint<'t>(&self, ty: &'t Type, hint: Option<&'t Type>) -> Option<&'t Type> {
        if self.recovering && ty.contains_error() {
            hint.or(Some(ty))
        } else {
            Some(ty)
        }
    }

    // A non-callable target still lets its operands report their own errors.
    fn recover_signature(
        &mut self,
        signature: Result<(Vec<Type>, Type), Diagnostic>,
        count: usize,
        span: Span,
    ) -> Result<(Vec<Type>, Type), Diagnostic> {
        match signature {
            Err(error) if self.recovering => {
                self.recover_expression(error, span)?;
                Ok((vec![Type::Error; count], Type::Error))
            }
            signature => signature,
        }
    }

    // Parameters that only the lost context would have typed stay unknown.
    fn unknown_parameters(&self, parameters: &mut [Type]) {
        for parameter in parameters {
            if polymorph::is_unknown(&self.inference.resolve(parameter)) {
                *parameter = Type::Error;
            }
        }
    }

    #[cold]
    #[inline(never)]
    fn recover_expression(
        &mut self,
        error: Diagnostic,
        span: Span,
    ) -> Result<TypedExpr, Diagnostic> {
        if error.code == "E1017" || self.recovered.len() >= MAX_UNIQUE_DIAGNOSTICS {
            return Err(error);
        }
        self.recovered.push(error);
        self.poisoned = true;
        Ok(TypedExpr::error(span))
    }

    fn finish_expression(
        &mut self,
        kind: TypedExprKind,
        ty: Type,
        expected: Option<&Type>,
        span: Span,
    ) -> Result<TypedExpr, Diagnostic> {
        let mut value = TypedExpr { kind, ty, span };
        if matches!(value.kind, TypedExprKind::Local(id) if self.read_value_parameters.contains(&id))
            && let Some(ty) = value.ty.dereferenced()
        {
            value = TypedExpr {
                kind: TypedExprKind::Dereference(Box::new(value)),
                ty,
                span,
            };
        }
        if self.poisoned
            && (self.inference.resolve(&value.ty).contains_error()
                || value
                    .children()
                    .iter()
                    .any(|child| child.ty.contains_error()))
        {
            if self.recovering && matches!(value.kind, TypedExprKind::Block { .. }) {
                value.ty = Type::Error;
                return Ok(value);
            }
            return Ok(TypedExpr::error(span));
        }
        if let Some(expected) = expected {
            value = self.callable_effect(value, expected)?;
            if self.read_conventions
                && matches!(expected, Type::Function(..) | Type::Infer(_))
                && let target @ Type::Function(..) = self.inference.resolve(expected)
            {
                value = self.callback_adapter(value, &target)?;
            }
            if value.ty.function_purity() == Some(true) && expected.function_purity() == Some(false)
            {
                let mut ty = value.ty.clone();
                ty.set_function_purity(false);
                value = TypedExpr {
                    span,
                    kind: TypedExprKind::Cast(Box::new(value)),
                    ty,
                };
            }
            self.same(&value.ty, expected, span)?;
        }
        value.ty = self.inference.resolve(&value.ty);
        Ok(value)
    }

    fn binding_annotation(&mut self, binding: &Binding) -> Result<Option<Type>, Diagnostic> {
        let annotation = (|| {
            let annotation = binding
                .annotation
                .as_ref()
                .map(|ty| self.annotation(ty))
                .transpose()?;
            if let Some(ty) = &annotation {
                validate_size(ty, &self.types, binding.name.span)?;
            }
            Ok(annotation)
        })();
        match annotation {
            Err(error) if self.recovering => {
                self.recover_expression(error, binding.name.span)?;
                Ok(Some(Type::Error))
            }
            annotation => annotation,
        }
    }

    fn composed_expression(
        &mut self,
        expression: &Expr,
        expected: Option<&Type>,
    ) -> Result<TypedExpr, Diagnostic> {
        let expected = expected.map(|ty| self.inference.resolve(ty));
        let expected = expected.as_ref();
        let (kind, ty) = match &expression.kind {
            ExprKind::Lambda(parameters, body) => {
                return self.lambda(
                    parameters,
                    body,
                    expected,
                    expression.span,
                    LambdaKind::Function,
                    &[],
                );
            }
            ExprKind::Task(body) => {
                return self.lambda(&[], body, expected, expression.span, LambdaKind::Task, &[]);
            }
            ExprKind::Call(..) => return self.call_expression(expression, expected, false),
            ExprKind::Block { bindings, result } => {
                self.scopes.push(BTreeMap::new());
                let checked = self.block_bindings(bindings)?;
                let result = self.expression(result, expected)?;
                self.scopes.pop();
                let ty = result.ty.clone();
                (
                    TypedExprKind::Block {
                        bindings: checked,
                        result: Box::new(result),
                    },
                    ty,
                )
            }
            _ => unreachable!("composed expression kinds are checked by expression"),
        };
        self.finish_expression(kind, ty, expected, expression.span)
    }

    /// Checks the bindings of a block in order, a `let rec` group (B09) as one. It is a function
    /// of its own so that its locals are not in the frame of `composed_expression`, which every
    /// level of an expression pays for.
    fn block_bindings(
        &mut self,
        bindings: &[Binding],
    ) -> Result<Vec<(Local, TypedExpr)>, Diagnostic> {
        let mut checked = Vec::new();
        let mut index = 0;
        while index < bindings.len() {
            let binding = &bindings[index];
            if binding.recursion != Recursion::None {
                let count = recursive_group_len(&bindings[index..]);
                checked.extend(self.let_rec_group(&bindings[index..index + count])?);
                index += count;
                continue;
            }
            index += 1;
            let annotation = self.binding_annotation(binding)?;
            let from = self.recovered.len();
            let value = match self.expression(&binding.value, annotation.as_ref()) {
                Ok(value) => value,
                Err(mut error) => {
                    self.annotation_because(binding, annotation.as_ref(), from, Some(&mut error));
                    return Err(error);
                }
            };
            if self.recovered.len() > from {
                self.annotation_because(binding, annotation.as_ref(), from, None);
            }
            let ty = if annotation.as_ref().is_some_and(Type::contains_error) {
                Type::Error
            } else {
                value.ty.clone()
            };
            self.require_use(binding, &ty)?;
            let local = self.bind(&binding.name, ty, binding.mutable);
            checked.push((local, value));
        }
        Ok(checked)
    }

    /// A `let rec` group (B09). Every name is bound before any function is checked, so each body
    /// can call itself and the other members, and the lambdas capture none of them: their uses
    /// become direct calls when `closures::lower` lifts the group.
    pub(super) fn let_rec_group(
        &mut self,
        group: &[Binding],
    ) -> Result<Vec<(Local, TypedExpr)>, Diagnostic> {
        let mut locals: Vec<Local> = Vec::with_capacity(group.len());
        for binding in group {
            if binding.name.text != "_"
                && locals.iter().any(|local| local.name == binding.name.text)
            {
                return Err(duplicate(&binding.name));
            }
            let ty = match self.binding_annotation(binding)? {
                Some(ty) => ty,
                None => self.inference.fresh(),
            };
            locals.push(self.bind(&binding.name, ty, false));
        }
        let mut checked = Vec::with_capacity(group.len());
        for (binding, local) in group.iter().zip(&locals) {
            let ExprKind::Lambda(names, body) = &binding.value.kind else {
                return Err(Diagnostic::of(
                    DiagnosticKind::InternalRecBindingNotLambda,
                    "internal compiler error: a 'let rec' binding is not a lambda",
                    binding.value.span,
                ));
            };
            let value = self.lambda(
                names,
                body,
                Some(&local.ty),
                binding.value.span,
                LambdaKind::Function,
                &locals,
            )?;
            checked.push((local.clone(), value));
        }
        Ok(checked)
    }

    /// A `use` binding is a `let` whose value type must implement `Drop` (B07).
    pub(super) fn require_use(&mut self, binding: &Binding, ty: &Type) -> Result<(), Diagnostic> {
        if !binding.using {
            return Ok(());
        }
        let span = binding.value.span;
        self.use_bindings.push(span);
        self.require("Drop", ty.clone(), span)
            .map_err(|error| self.use_error(error, ty, span))
    }

    pub(super) fn use_error(&self, error: Diagnostic, ty: &Type, span: Span) -> Diagnostic {
        if error.code != "E1005" {
            return error;
        }
        Diagnostic::of(
            DiagnosticKind::UseRequiresDrop,
            format!(
                "'use' needs a value whose type implements Drop; {} does not, so bind it with 'let'",
                self.inference.resolve(ty).display(&self.types)
            ),
            span,
        )
    }

    fn call_expression(
        &mut self,
        expression: &Expr,
        expected: Option<&Type>,
        argument: bool,
    ) -> Result<TypedExpr, Diagnostic> {
        let ExprKind::Call(callee, arguments) = &expression.kind else {
            unreachable!("call expressions only")
        };
        if let ExprKind::Call(inner, first) = &callee.kind {
            if !first.is_empty() && !arguments.is_empty() {
                let mut combined = first.clone();
                combined.extend(arguments.iter().cloned());
                let combined = Expr {
                    kind: ExprKind::Call(inner.clone(), combined),
                    span: expression.span,
                    depth: expression.depth,
                };
                return self.call_expression(&combined, expected, argument);
            }
        }
        // `xs |> f a` passes `xs` last; its type fixes `f`'s next parameter before
        // the arguments, such as a lambda, are checked.
        let piped = self.piped.take();
        self.dyn_callee = arguments.len() == 1 && piped.is_none() && self.names_dyn_of(callee);
        let callee = self.expression(callee, None);
        self.dyn_callee = false;
        let callee = callee?;
        let arguments = if let [
            Expr {
                kind: ExprKind::Tuple(values),
                ..
            },
        ] = arguments.as_slice()
        {
            match &callee.ty {
                Type::Function(parameters, _)
                    if parameters.len() >= values.len()
                        && !matches!(
                            parameters[0],
                            Type::Tuple(_) | Type::Variable(_) | Type::Infer(_)
                        ) =>
                {
                    values
                }
                _ => arguments,
            }
        } else {
            arguments
        };
        let signature = self
            .call_signature(&callee.ty, arguments.len(), expression.span)
            .map_err(|error| Self::operator_spacing_hint(error, arguments));
        let (mut parameters, result) =
            self.recover_signature(signature, arguments.len(), expression.span)?;
        if let Some(expected) = expected {
            let hint = if argument {
                self.argument_hint(&result, expected)
            } else {
                expected.clone()
            };
            self.same(&result, &hint, expression.span)
                .map_err(|error| {
                    self.partial_application(error, &callee.ty, arguments.len(), &result, &hint)
                })?;
        }
        if self.recovering && expected.is_some_and(|ty| matches!(ty, Type::Error)) {
            self.unknown_parameters(&mut parameters);
        }
        if piped.is_some() {
            self.apply_piped(piped, &result, expression.span)?;
        }
        let owned = matches!(&callee.kind, TypedExprKind::Function(FunctionRef::Builtin(instance)) if instance.builtin == Builtin::OwnedFunction);
        let mut arguments: Vec<_> = if matches!(&callee.kind, TypedExprKind::Function(FunctionRef::Builtin(instance)) if matches!(instance.builtin, Builtin::ParallelMap | Builtin::ParallelMapRef | Builtin::ParallelReduce | Builtin::ParallelForEachChunk) && arguments.len() == instance.builtin.scheme().parameters.len())
        {
            self.parallel_arguments(arguments, &parameters)?
        } else {
            // A plain loop keeps iterator adapter frames off the recursion through nested calls.
            // Lambdas are checked after the other arguments, whose types often fix the
            // lambdas' parameters, as in `Array.map (\text -> text.length) (ref texts)`.
            let mut checked = Vec::with_capacity(arguments.len());
            let mut deferred = false;
            let mut callbacks = Vec::new();
            for (index, (argument, parameter)) in arguments.iter().zip(&parameters).enumerate() {
                checked.push(match &argument.kind {
                    _ if Self::deferred_lambda(arguments, index) => {
                        deferred = true;
                        TypedExpr::error(argument.span)
                    }
                    ExprKind::Lambda(names, body) if owned && index == 0 => {
                        self.owned_lambda(names, body, argument.span, parameter)?
                    }
                    _ if self.read_conventions
                        && Self::callback_parameter(
                            &self.inference.resolve(parameter),
                            matches!(&callee.ty, Type::Function(parameters, _)
                                if parameters.convention(index) != ParameterConvention::Own),
                        ) =>
                    {
                        callbacks.push(index);
                        TypedExpr::error(argument.span)
                    }
                    _ => {
                        let from = self.recovered.len();
                        let checked = match self.argument(argument, parameter) {
                            Ok(checked) => checked,
                            Err(mut error) => {
                                self.argument_because(
                                    &callee,
                                    index,
                                    argument,
                                    parameter,
                                    from,
                                    Some(&mut error),
                                );
                                return Err(error);
                            }
                        };
                        if self.recovered.len() > from {
                            self.argument_because(&callee, index, argument, parameter, from, None);
                        }
                        checked
                    }
                });
            }
            for index in callbacks {
                checked[index] =
                    self.noted_argument(&callee, index, &arguments[index], &parameters[index])?;
            }
            if deferred {
                self.lambda_arguments(&callee, arguments, &parameters, &mut checked)?;
            }
            checked
        };
        self.solve_families(false)?;
        self.temporary_borrows(&callee, &mut arguments, &result, expression.span)?;
        if matches!(&callee.kind, TypedExprKind::Function(FunctionRef::Builtin(instance)) if instance.builtin == Builtin::DynOf)
        {
            self.dyn_of(&callee, expression.span)?;
        }
        let value = self.finish_expression(
            Self::call_kind(callee, arguments),
            result,
            if argument { None } else { expected },
            expression.span,
        )?;
        if let (true, Some(expected)) = (argument, expected) {
            self.coerce_argument(value, expected)
        } else {
            Ok(value)
        }
    }

    fn binary_expression(
        &mut self,
        expression: &Expr,
        expected: Option<&Type>,
    ) -> Result<TypedExpr, Diagnostic> {
        let ExprKind::Binary(operator, left, right) = &expression.kind else {
            unreachable!("binary expressions only")
        };
        let expected = expected.map(|ty| self.inference.resolve(ty));
        let expected = expected.as_ref();
        let (left, right) = if *operator == BinaryOp::Pipe {
            // The piped value is checked first, as it runs first.
            let open = self.inference.fresh();
            let left = self.argument(left, &open)?;
            if matches!(right.kind, ExprKind::Call(..)) {
                self.piped = Some(left.ty.clone());
            }
            let right = self.expression(right, None);
            self.piped = None;
            let right = right?;
            let signature = self.call_signature(&right.ty, 1, right.span);
            let (parameters, result) = self.recover_signature(signature, 1, right.span)?;
            if let Some(expected) = expected {
                self.same(&result, expected, expression.span)?;
            }
            (self.coerce_argument(left, &parameters[0])?, right)
        } else {
            let hint = expected.filter(|_| {
                !matches!(
                    operator,
                    BinaryOp::Equal
                        | BinaryOp::NotEqual
                        | BinaryOp::Less
                        | BinaryOp::LessEqual
                        | BinaryOp::Greater
                        | BinaryOp::GreaterEqual
                        | BinaryOp::And
                        | BinaryOp::Or
                )
            });
            if Self::untyped_number(left) && !Self::untyped_number(right) {
                let right = self.expression(right, hint)?;
                let expected = self.sibling_hint(&right.ty, hint);
                (self.expression(left, expected)?, right)
            } else {
                let left = self.expression(left, hint)?;
                let expected = self.sibling_hint(&left.ty, hint);
                let right = self.expression(right, expected)?;
                (left, right)
            }
        };
        let result = if *operator == BinaryOp::Pipe {
            let signature = self.call_signature(&right.ty, 1, right.span);
            let (parameters, result) = self.recover_signature(signature, 1, right.span)?;
            self.same(&left.ty, &parameters[0], left.span)?;
            self.solve_families(false)?;
            result
        } else {
            self.binary_type(*operator, &left, &right, expression.span)?
        };
        self.finish_binary(*operator, left, right, result, expected, expression.span)
    }

    /// The typed node of a checked binary expression: a pipe may borrow its
    /// temporary or become `!` for `x |> not`, and `@checked` marks arithmetic.
    #[inline(never)]
    fn finish_binary(
        &mut self,
        operator: BinaryOp,
        mut left: TypedExpr,
        right: TypedExpr,
        result: Type,
        expected: Option<&Type>,
        span: Span,
    ) -> Result<TypedExpr, Diagnostic> {
        if operator == BinaryOp::Pipe {
            self.temporary_borrows(&right, std::slice::from_mut(&mut left), &result, span)?;
        }
        let kind = match &right.kind {
            TypedExprKind::Function(FunctionRef::Builtin(instance))
                if operator == BinaryOp::Pipe && instance.builtin == Builtin::Not =>
            {
                TypedExprKind::Unary(UnaryOp::Not, Box::new(left))
            }
            _ => TypedExprKind::Binary(operator, Box::new(left), Box::new(right)),
        };
        let (kind, result) = if self.checked_arithmetic
            && matches!(
                operator,
                BinaryOp::Add | BinaryOp::Subtract | BinaryOp::Multiply | BinaryOp::Power
            ) {
            Self::checked_kind(kind, &result, span)
        } else {
            (kind, result)
        };
        self.finish_expression(kind, result, expected, span)
    }

    fn value_expression(
        &mut self,
        expression: &Expr,
        expected: Option<&Type>,
    ) -> Result<TypedExpr, Diagnostic> {
        let expected = expected.map(|ty| self.inference.resolve(ty));
        let expected = expected.as_ref();
        let case = match &expression.kind {
            ExprKind::Field(..) => self.case_reference(expression)?,
            _ => None,
        };
        // `Module.name` names a module function or a qualified builtin such
        // as `Task.run` before any class method.
        let namespace = if case.is_none() && matches!(expression.kind, ExprKind::Field(..)) {
            self.namespace_value(expression)?
        } else {
            None
        };
        let method = if namespace.is_some() || case.is_some() {
            None
        } else {
            self.classes
                .method(expression, self.module, self.names, |name| {
                    self.local(name).is_some()
                })?
        };
        let resolved = if namespace.is_some() {
            namespace
        } else {
            method
                .map(|(class, method)| self.method(class, method, expression.span))
                .transpose()?
        };
        if let Some((kind, ty)) = resolved {
            return self.finish_expression(kind, ty, expected, expression.span);
        }
        let (kind, ty) = match &expression.kind {
            ExprKind::Integer(value, suffix) => {
                self.integer(*value, suffix.as_deref(), expected, false, expression.span)?
            }
            ExprKind::Float(value, suffix) => {
                if suffix.is_none() && expected.is_some_and(polymorph::is_unknown) {
                    let ty = expected.unwrap().clone();
                    self.require("Float", ty.clone(), expression.span)?;
                    self.inference.default_numeric(&ty, Type::F64);
                    (TypedExprKind::GenericFloat(value.clone()), ty)
                } else {
                    let ty = suffix
                        .as_deref()
                        .and_then(crate::numeric::primitive)
                        .or_else(|| expected.filter(|ty| ty.is_float()).cloned())
                        .unwrap_or(Type::F64);
                    if !ty.is_float() {
                        return Err(Diagnostic::of(
                            DiagnosticKind::FloatLiteralType,
                            "a floating-point literal needs a binary or decimal floating-point type",
                            expression.span,
                        ));
                    }
                    (
                        TypedExprKind::Float(crate::numeric::float_literal(
                            value,
                            &ty,
                            expression.span,
                        )?),
                        ty,
                    )
                }
            }
            ExprKind::String(text) => (
                TypedExprKind::String(text.clone()),
                match text {
                    StringLiteral::Utf16(_) => Type::String,
                    StringLiteral::Utf8(_) => Type::Utf8String,
                },
            ),
            ExprKind::Bool(value) => (TypedExprKind::Bool(*value), Type::Bool),
            ExprKind::Unit => (TypedExprKind::Unit, Type::Unit),
            // The body of a generated `dyn` instance method has the method's result type (A14).
            ExprKind::DynDispatch { slot, slots } => {
                let ty = expected.cloned().ok_or_else(|| {
                    Diagnostic::of(
                        DiagnosticKind::DynDispatchResultType,
                        "a dyn dispatch needs the method's result type",
                        expression.span,
                    )
                })?;
                (
                    TypedExprKind::DynDispatch {
                        slot: *slot,
                        slots: *slots,
                    },
                    ty,
                )
            }
            ExprKind::Interpolated(interpolation) => {
                return self.interpolation(interpolation, expression.span, expected);
            }
            ExprKind::Tuple(values) => {
                let types = match expected {
                    Some(Type::Tuple(types)) if types.len() == values.len() => Some(types),
                    _ => None,
                };
                let unknown = expected.filter(|ty| matches!(ty, Type::Error));
                let values = values
                    .iter()
                    .enumerate()
                    .map(|(index, value)| {
                        self.expression(value, types.map(|types| &types[index]).or(unknown))
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let ty = Type::Tuple(values.iter().map(|value| value.ty.clone()).collect());
                validate_size(&ty, &self.types, expression.span)?;
                (TypedExprKind::Tuple(values), ty)
            }
            ExprKind::Range { .. } => {
                return Err(Diagnostic::of(
                    DiagnosticKind::RangeAsValue,
                    "a range is an enumerable expression for 'for...in', not a stored value",
                    expression.span,
                ));
            }
            ExprKind::While { .. } | ExprKind::For { .. } | ExprKind::Match { .. } => {
                unreachable!("control expressions use their own checker")
            }
            ExprKind::Name(name) => self.name(name)?,
            ExprKind::TypeFunction(variable, name) => self.type_function(variable, name)?,
            ExprKind::QualifiedFunction(name) => self.qualified_function(name)?,
            ExprKind::TaskRun(value) => {
                let result = expected.cloned().unwrap_or_else(|| self.inference.fresh());
                let value = self.expression(value, Some(&Type::Task(Box::new(result.clone()))))?;
                (TypedExprKind::TaskRun(Box::new(value)), result)
            }
            ExprKind::Computation(..) => unreachable!("computations expand before type checking"),
            ExprKind::BigInt(_)
            | ExprKind::Checked(_)
            | ExprKind::Try(_)
            | ExprKind::Unary(UnaryOp::Plus, _) => {
                return self.spec_expression(expression, expected);
            }
            ExprKind::Unary(UnaryOp::Negate, operand)
                if matches!(operand.kind, ExprKind::BigInt(_)) =>
            {
                return self.spec_expression(expression, expected);
            }
            ExprKind::Unary(UnaryOp::Negate, operand)
                if matches!(operand.kind, ExprKind::Integer(..)) =>
            {
                let ExprKind::Integer(value, suffix) = &operand.kind else {
                    unreachable!()
                };
                self.integer(*value, suffix.as_deref(), expected, true, expression.span)?
            }
            ExprKind::Unary(operator, operand) => {
                let operand = self.expression(operand, expected)?;
                if *operator == UnaryOp::Not {
                    self.same(&operand.ty, &Type::Bool, expression.span)?;
                } else {
                    self.require(
                        if *operator == UnaryOp::Negate {
                            "Neg"
                        } else {
                            "Bits"
                        },
                        operand.ty.clone(),
                        expression.span,
                    )?;
                }
                let ty = operand.ty.clone();
                let kind = TypedExprKind::Unary(*operator, Box::new(operand));
                if *operator == UnaryOp::Negate && self.checked_arithmetic {
                    Self::checked_kind(kind, &ty, expression.span)
                } else {
                    (kind, ty)
                }
            }
            ExprKind::If {
                condition,
                then_branch,
                else_branch,
            } => {
                let condition = self.expression(condition, Some(&Type::Bool))?;
                let then_branch = self.expression(then_branch, expected)?;
                let expected = self.sibling_hint(&then_branch.ty, expected);
                let else_branch = self.expression(else_branch, expected)?;
                let ty = then_branch.ty.clone();
                (
                    TypedExprKind::If {
                        condition: Box::new(condition),
                        then_branch: Box::new(then_branch),
                        else_branch: Box::new(else_branch),
                    },
                    ty,
                )
            }
            ExprKind::Record { name, fields } => {
                self.record_literal(name, fields, expected, expression.span)?
            }
            ExprKind::RecordUpdate { base, fields } => {
                self.record_update(base, fields, expression.span)?
            }
            ExprKind::Char(value) => (TypedExprKind::Int(u128::from(*value)), Type::Char),
            ExprKind::Utf8Char(value) => (TypedExprKind::Int(u128::from(*value)), Type::Utf8Char),
            ExprKind::Slice {
                value,
                start,
                end,
                mutable,
            } => self.slice(
                value,
                start.as_deref(),
                end.as_deref(),
                *mutable,
                expression.span,
            )?,
            ExprKind::Break | ExprKind::Continue => {
                let breaking = matches!(expression.kind, ExprKind::Break);
                if self.normal_loop_depth == 0 {
                    let keyword = if breaking { "break" } else { "continue" };
                    return Err(Diagnostic::of(
                        DiagnosticKind::JumpOutsideLoop,
                        format!(
                            "{keyword} can only target an enclosing for or while loop in the same function, task, or computation; return a value across boundaries"
                        ),
                        expression.span,
                    ));
                }
                (
                    if breaking {
                        TypedExprKind::Break
                    } else {
                        TypedExprKind::Continue
                    },
                    Type::Unit,
                )
            }
            ExprKind::Array(values) | ExprKind::List(values) => {
                let list = matches!(expression.kind, ExprKind::List(_));
                // A16: a literal is a fixed-length array only where one is expected.
                if !list
                    && let Some(Type::FixedArray(element, length)) =
                        expected.map(|ty| self.inference.resolve(ty))
                {
                    let (kind, ty) =
                        self.fixed_array_literal(values, *element, *length, expression.span)?;
                    return self.finish_expression(kind, ty, expected, expression.span);
                }
                let mut element_type = match (list, expected) {
                    (false, Some(Type::Array(element))) | (true, Some(Type::List(element))) => {
                        Some((**element).clone())
                    }
                    (_, Some(Type::Error)) => Some(Type::Error),
                    _ => None,
                };
                let mut checked = Vec::new();
                for value in values {
                    let value = self.expression(value, element_type.as_ref())?;
                    if !element_type.as_ref().is_some_and(Type::contains_error) {
                        element_type = self.sibling_hint(&value.ty, element_type.as_ref()).cloned();
                    }
                    checked.push(value);
                }
                let element = element_type.ok_or_else(|| {
                    Diagnostic::of(
                        DiagnosticKind::EmptyListNeedsAnnotation,
                        if list {
                            "an empty list needs a type annotation, for example '[|i64|]'"
                        } else {
                            "an empty array needs a type annotation, for example '[i64]'"
                        },
                        expression.span,
                    )
                })?;
                let (kind, ty) = if list {
                    (TypedExprKind::List(checked), Type::List(Box::new(element)))
                } else {
                    (
                        TypedExprKind::Array(checked),
                        Type::Array(Box::new(element)),
                    )
                };
                validate_size(&ty, &self.types, expression.span)?;
                (kind, ty)
            }
            ExprKind::NewArray(annotation, length, initializer)
            | ExprKind::NewList(annotation, length, initializer) => {
                let ty = self.annotation(annotation)?;
                validate_size(&ty, &self.types, expression.span)?;
                let (Type::Array(element) | Type::List(element)) = &ty else {
                    unreachable!("the parser requires a collection type after 'new'")
                };
                let length = self.index_operand(length)?;
                let initializer_type = Type::function(vec![Type::I64], (**element).clone());
                let initializer = self.expression(initializer, Some(&initializer_type))?;
                let kind = if matches!(ty, Type::List(_)) {
                    TypedExprKind::NewList(Box::new(length), Box::new(initializer))
                } else {
                    TypedExprKind::NewArray(Box::new(length), Box::new(initializer))
                };
                (kind, ty)
            }
            ExprKind::NewLiteral(literal) => {
                let literal = self.expression(literal, expected)?;
                let ty = literal.ty.clone();
                if matches!(self.inference.resolve(&ty), Type::FixedArray(..)) {
                    return Err(Diagnostic::of(
                        DiagnosticKind::NewOnFixedArray,
                        "'new' creates a heap array or list; remove 'new' to create a fixed-length array value",
                        expression.span,
                    ));
                }
                (TypedExprKind::NewLiteral(Box::new(literal)), ty)
            }
            ExprKind::Field(..) if case.is_some() => {
                let (union_id, case_id) = case.unwrap();
                self.note_case_path(expression, union_id, case_id);
                self.case_value(union_id, case_id)
            }
            ExprKind::Field(value, field)
                if self
                    .value_path(value)
                    .is_some_and(|name| self.is_namespace(&name)) =>
            {
                let module = self.value_path(value).unwrap();
                let spelled = namespace_display(&module);
                let message = if module == "Task" {
                    format!(
                        "Task has no function '{}'; use Task.run, Task.parallel, or Task.parallel_results",
                        field.text
                    )
                } else if self.names.module_path(self.module, &spelled).is_none()
                    && self.names.namespace_path(self.module, &spelled).is_some()
                {
                    format!(
                        "'{spelled}' is a namespace; write '::' between a namespace and the names in it, as in '{spelled}::{}'",
                        field.text
                    )
                } else {
                    format!(
                        "module '{spelled}' has no function or union case '{}'",
                        field.text
                    )
                };
                return Err(Diagnostic::of(
                    DiagnosticKind::UnknownModuleMember,
                    message,
                    field.span,
                ));
            }
            ExprKind::Field(value, field) => self.field_access(value, field)?,
            ExprKind::Index(value, index) => {
                let value = Self::autoderef(self.expression(value, None)?);
                if value.ty.contains_error() {
                    return Ok(TypedExpr::error(expression.span));
                }
                let ty = match &value.ty {
                    Type::Array(element)
                    | Type::List(element)
                    | Type::Vec(element)
                    | Type::FixedArray(element, _) => (**element).clone(),
                    Type::String => Type::Integer(16, false),
                    Type::Utf8String => Type::Integer(8, false),
                    _ => {
                        return Err(Diagnostic::of(
                            DiagnosticKind::IndexNonIndexable,
                            "indexing requires an array, list, string, or utf8string",
                            value.span,
                        ));
                    }
                };
                let index = self.index_operand(index)?;
                (TypedExprKind::Index(Box::new(value), Box::new(index)), ty)
            }
            ExprKind::Borrow(value, mutable, notation) => {
                let value = match notation {
                    Notation::Symbol => {
                        let hint = match expected {
                            Some(reference @ Type::Reference(_, expected_mutable))
                                if mutable == expected_mutable =>
                            {
                                reference.dereferenced()
                            }
                            Some(Type::Error) => Some(Type::Error),
                            _ => None,
                        };
                        self.expression(value, hint.as_ref())?
                    }
                    Notation::Keyword => {
                        let value = self.expression(value, None)?;
                        if matches!(value.ty, Type::Infer(_)) {
                            self.undecided_borrows.push((value.ty.clone(), value.span));
                        }
                        Self::reborrow_operand(value)
                    }
                };
                if *mutable {
                    Self::require_mutable_reference(&value)?;
                    Self::exclusive_borrow(value, expected)?
                } else {
                    let ty = Type::Reference(Box::new(value.ty.clone()), false);
                    (TypedExprKind::Borrow(Box::new(value), false), ty)
                }
            }
            ExprKind::Dereference(value, notation) => {
                let value = self.expression(value, None)?;
                if value.ty.contains_error() {
                    return Ok(TypedExpr::error(expression.span));
                }
                let Some(ty) = value.ty.dereferenced() else {
                    return Err(Diagnostic::of(
                        DiagnosticKind::DerefNonReference,
                        match notation {
                            Notation::Symbol => "dereference requires a reference",
                            Notation::Keyword => "'deref' requires a reference",
                        },
                        value.span,
                    ));
                };
                (TypedExprKind::Dereference(Box::new(value)), ty)
            }
            ExprKind::Assign(place, value) => {
                let place = self.expression(place, None)?;
                if place.ty.contains_error() {
                    return Ok(TypedExpr::error(expression.span));
                }
                Self::require_mutable_reference(&place)?;
                if matches!(&place.kind, TypedExprKind::Dereference(reference) if reference.ty.is_view())
                {
                    return Err(Diagnostic::of(
                        DiagnosticKind::ReplaceExclusiveSlice,
                        "cannot replace an exclusive slice as a whole because its length is fixed; write elements with 'Array.write' or replace the array through its owner ('ref mut [T]')",
                        expression.span,
                    ));
                }
                let projection = matches!(place.kind, TypedExprKind::Field(..))
                    || matches!(&place.kind, TypedExprKind::Index(base, _) if matches!(
                        base.ty,
                        Type::Array(_) | Type::List(_) | Type::Vec(_) | Type::FixedArray(..)
                    ));
                if !matches!(
                    place.kind,
                    TypedExprKind::Local(_) | TypedExprKind::Dereference(_)
                ) && !(self.read_conventions && projection)
                {
                    return Err(Diagnostic::of(
                        DiagnosticKind::AssignToImmutablePlace,
                        "assignment replaces a mutable binding; record fields, array elements, and list elements are immutable",
                        place.span,
                    ));
                }
                let value = self.expression(value, Some(&place.ty))?;
                (
                    TypedExprKind::Assign(Box::new(place), Box::new(value)),
                    Type::Unit,
                )
            }
            ExprKind::Cast(value, ty) => {
                let value = self.expression(value, None)?;
                if value.ty.contains_error() {
                    return Ok(TypedExpr::error(expression.span));
                }
                let ty = self.annotation(ty)?;
                self.require("Numeric", value.ty.clone(), expression.span)?;
                self.require("Numeric", ty.clone(), expression.span)?;
                (TypedExprKind::Cast(Box::new(value)), ty)
            }
            ExprKind::Binary(..)
            | ExprKind::Lambda(..)
            | ExprKind::Task(_)
            | ExprKind::Call(..)
            | ExprKind::ComputationBoundary(_)
            | ExprKind::Block { .. } => unreachable!("composed expressions use their own checker"),
        };
        self.finish_expression(kind, ty, expected, expression.span)
    }

    /// The lambda written directly as the argument of `Owned.function` (B07).
    fn owned_lambda(
        &mut self,
        names: &[(Ident, bool)],
        body: &Expr,
        span: Span,
        expected: &Type,
    ) -> Result<TypedExpr, Diagnostic> {
        if expected.contains_error() {
            self.poisoned = true;
            if !self.recovering {
                return Ok(TypedExpr::error(span));
            }
        }
        let mark = RecoveryMark {
            scopes: self.scopes.len(),
            normal_loop_depth: self.normal_loop_depth,
            computation_depth: self.computation_depth,
        };
        let result = self.lambda(names, body, Some(expected), span, LambdaKind::Owned, &[]);
        self.finish_recovery(result, mark, span)
    }

    fn argument(&mut self, expression: &Expr, expected: &Type) -> Result<TypedExpr, Diagnostic> {
        let mark = RecoveryMark {
            scopes: self.scopes.len(),
            normal_loop_depth: self.normal_loop_depth,
            computation_depth: self.computation_depth,
        };
        let result = match expression.kind {
            ExprKind::Call(..) if !expected.contains_error() => {
                self.call_expression(expression, Some(expected), true)
            }
            ExprKind::Name(_)
            | ExprKind::QualifiedFunction(_)
            | ExprKind::TypeFunction(..)
            | ExprKind::Field(..)
            | ExprKind::Index(..)
            | ExprKind::Dereference(..)
                if !expected.contains_error() =>
            {
                self.place_argument(expression, expected)
            }
            // A temporary is borrowed like a call's result:
            // `xs |> Array.map f |> Array.sum`, `String.length "text"` and `List.length [|1, 2|]`.
            ExprKind::Binary(..)
            | ExprKind::Unary(..)
            | ExprKind::Cast(..)
            | ExprKind::Checked(_)
            | ExprKind::String(_)
            | ExprKind::Interpolated(_)
            | ExprKind::Array(_)
            | ExprKind::List(_)
            | ExprKind::Tuple(_)
                if self.shared_reference(expected) =>
            {
                self.temporary_argument(expression, expected)
            }
            ExprKind::Lambda(..)
                if self.shared_reference(expected) && expected.function_purity() == Some(true) =>
            {
                self.temporary_argument(expression, expected)
            }
            // A branch gives a value or a reference, so the argument is checked by itself first: a
            // value is borrowed as a temporary, and a reference is reborrowed as a place's is.
            ExprKind::If { .. }
            | ExprKind::Match { .. }
            | ExprKind::Try(_)
            | ExprKind::Block { .. }
                if self.shared_reference(expected) =>
            {
                self.place_argument(expression, expected)
            }
            ExprKind::Integer(..)
            | ExprKind::Float(..)
            | ExprKind::BigInt(_)
            | ExprKind::Bool(_)
            | ExprKind::Unit
            | ExprKind::Char(_)
            | ExprKind::Utf8Char(_)
            | ExprKind::Lambda(..)
            | ExprKind::Record { .. }
            | ExprKind::RecordUpdate { .. }
            | ExprKind::NewArray(..)
            | ExprKind::NewList(..)
            | ExprKind::NewLiteral(_)
                if self.read_conventions && self.shared_reference(expected) =>
            {
                self.temporary_argument(expression, expected)
            }
            _ => self.expression(expression, Some(expected)),
        };
        self.finish_recovery(result, mark, expression.span)
    }

    fn place_argument(
        &mut self,
        expression: &Expr,
        expected: &Type,
    ) -> Result<TypedExpr, Diagnostic> {
        let value = self.expression(expression, None)?;
        self.coerce_argument(value, expected)
    }

    /// Whether the lambda argument at `index` waits for a later non-lambda
    /// argument, whose type often fixes its parameters, as in
    /// `Array.map (\text -> text.length) (ref texts)`.
    fn deferred_lambda(arguments: &[Expr], index: usize) -> bool {
        matches!(arguments[index].kind, ExprKind::Lambda(..))
            && arguments[index + 1..]
                .iter()
                .any(|later| !matches!(later.kind, ExprKind::Lambda(..)))
    }

    /// The deferred lambda arguments of a call; `checked` holds placeholders at
    /// their positions. An owned lambda comes first only in `Owned.function`,
    /// which takes one argument, so it is never deferred.
    #[inline(never)]
    fn lambda_arguments(
        &mut self,
        callee: &TypedExpr,
        arguments: &[Expr],
        parameters: &[Type],
        checked: &mut [TypedExpr],
    ) -> Result<(), Diagnostic> {
        for (index, (argument, parameter)) in arguments.iter().zip(parameters).enumerate() {
            if Self::deferred_lambda(arguments, index) {
                checked[index] = self.noted_argument(callee, index, argument, parameter)?;
            }
        }
        Ok(())
    }

    /// `argument` for an argument that is checked after the others, with the declared parameter
    /// type in its mismatches (`argument_because`).
    #[inline(never)]
    fn noted_argument(
        &mut self,
        callee: &TypedExpr,
        index: usize,
        argument: &Expr,
        parameter: &Type,
    ) -> Result<TypedExpr, Diagnostic> {
        let from = self.recovered.len();
        let checked = match self.argument(argument, parameter) {
            Ok(checked) => checked,
            Err(mut error) => {
                self.argument_because(callee, index, argument, parameter, from, Some(&mut error));
                return Err(error);
            }
        };
        if self.recovered.len() > from {
            self.argument_because(callee, index, argument, parameter, from, None);
        }
        Ok(checked)
    }

    /// `xs |> f a` fixes the next parameter of `f a` to the type of `xs`.
    #[inline(never)]
    fn apply_piped(
        &mut self,
        piped: Option<Type>,
        result: &Type,
        span: Span,
    ) -> Result<(), Diagnostic> {
        if let Some(piped) = piped
            && let Type::Function(remaining, _) = self.inference.resolve(result)
        {
            // An array that passes on as an exclusive slice (`ref mut xs |> Array.write 0 1`) is sliced by
            // `coerce_slice_argument` once the call is complete; here only its elements are known to agree.
            if let Type::Reference(view, true) = self.inference.resolve(&remaining[0])
                && let Type::ArrayView(wanted) = *view
                && let Some(element) = match self.inference.resolve(&piped) {
                    Type::Reference(inner, true) => match *inner {
                        Type::Array(element) => Some(*element),
                        _ => None,
                    },
                    Type::Array(element) => Some(*element),
                    _ => None,
                }
            {
                return self.same(&element, &wanted, span);
            }
            let hint = self.argument_hint(&piped, &remaining[0]);
            self.same(&piped, &hint, span)?;
        }
        Ok(())
    }

    /// A temporary argument for a shared reference parameter, checked against
    /// the referenced type and then borrowed.
    #[inline(never)]
    fn temporary_argument(
        &mut self,
        expression: &Expr,
        expected: &Type,
    ) -> Result<TypedExpr, Diagnostic> {
        let Type::Reference(inner, false) = self.inference.resolve(expected) else {
            unreachable!("shared reference checked")
        };
        let value = self.expression(expression, Some(&inner))?;
        self.coerce_argument(value, expected)
    }

    fn shared_reference(&self, expected: &Type) -> bool {
        matches!(self.inference.resolve(expected), Type::Reference(_, false))
    }

    fn argument_hint(&self, actual: &Type, expected: &Type) -> Type {
        let actual = self.inference.resolve(actual);
        let expected = self.inference.resolve(expected);
        match (&actual, &expected) {
            (_, Type::Infer(_) | Type::Error) => expected,
            // A16: a fixed-length array keeps its type; `coerce_argument` slices it.
            (Type::FixedArray(..), Type::Reference(inner, false))
                if matches!(**inner, Type::Array(_)) =>
            {
                actual
            }
            (Type::Reference(target, _), Type::Reference(inner, false))
                if matches!(**target, Type::FixedArray(..))
                    && matches!(**inner, Type::Array(_)) =>
            {
                actual
            }
            (Type::Reference(_, mutable), Type::Reference(inner, _)) => {
                if actual == **inner {
                    actual
                } else {
                    Type::Reference(inner.clone(), *mutable)
                }
            }
            (Type::Reference(inner, mutable), _) => {
                Type::Reference(Box::new(self.argument_hint(inner, &expected)), *mutable)
            }
            (Type::Infer(_) | Type::Error, _) => expected,
            (_, Type::Reference(inner, _)) => (**inner).clone(),
            _ => expected,
        }
    }

    fn coerce_argument(
        &mut self,
        value: TypedExpr,
        expected: &Type,
    ) -> Result<TypedExpr, Diagnostic> {
        let expected = self.inference.resolve(expected);
        let mut value = match self.coerce_slice_argument(value, &expected)? {
            Ok(converted) => return Ok(converted),
            Err(value) => value,
        };
        value = self.callable_effect(value, &expected)?;
        if self.read_conventions {
            let target = match &expected {
                Type::Reference(inner, false) if matches!(**inner, Type::Function(..)) => {
                    inner.as_ref()
                }
                _ => &expected,
            };
            if matches!(target, Type::Function(..)) {
                value = self.callback_adapter(value, target)?;
            }
        }
        let literal = self.inference.is_numeric_literal(&value.ty);
        if matches!(value.ty, Type::Infer(_)) && !literal {
            self.same(&value.ty, &expected, value.span)?;
            value.ty = self.inference.resolve(&value.ty);
        }
        if literal || !matches!(value.ty, Type::Infer(_) | Type::Error) {
            match &expected {
                Type::Reference(inner, mutable) => {
                    if value.ty != **inner && !self.literal_match(&value.ty, inner) {
                        value = Self::reborrow_operand(value);
                    }
                    if *mutable {
                        Self::require_mutable_reference(&value)?;
                    }
                    let ty = Type::Reference(Box::new(value.ty.clone()), *mutable);
                    value = TypedExpr {
                        span: value.span,
                        kind: TypedExprKind::Borrow(Box::new(value), *mutable),
                        ty,
                    };
                }
                Type::Infer(_) | Type::Error => {}
                _ => value = Self::autoderef(value),
            }
        }
        self.finish_expression(value.kind, value.ty, Some(&expected), value.span)
    }

    fn callback_parameter(ty: &Type, borrowed: bool) -> bool {
        match ty {
            Type::Function(parameters, _) => parameters
                .conventions
                .iter()
                .any(|convention| *convention != ParameterConvention::Own),
            Type::Reference(inner, false) => {
                (borrowed && matches!(**inner, Type::Function(..)))
                    || Self::callback_parameter(inner, false)
            }
            _ => false,
        }
    }

    fn callback_local(&mut self, ty: Type, span: Span) -> Local {
        let id = self.next_local;
        self.next_local += 1;
        Local {
            id,
            ty,
            name: format!("$convention.{id}"),
            mutable: false,
            span,
            provenance: Provenance::Generated,
        }
    }

    fn callable_effect(
        &mut self,
        mut value: TypedExpr,
        expected: &Type,
    ) -> Result<TypedExpr, Diagnostic> {
        let actual = self.inference.resolve(&value.ty);
        let expected = self.inference.resolve(expected);
        let (Some(actual), Some(expected)) = (actual.function_purity(), expected.function_purity())
        else {
            return Ok(value);
        };
        if actual == expected || !expected {
            return Ok(value);
        }
        if expected
            && !matches!(
                value.kind,
                TypedExprKind::Function(_)
                    | TypedExprKind::GenericFunction(..)
                    | TypedExprKind::Lambda { .. }
                    | TypedExprKind::Closure(..)
                    | TypedExprKind::Method(..)
                    | TypedExprKind::TypeFunction { .. }
            )
        {
            return Err(Diagnostic::of(
                DiagnosticKind::ImpureCallbackForPureContract,
                "an unknown ordinary callback cannot satisfy a pure function contract; pass a pure callback or a closed function",
                value.span,
            ));
        }
        value.ty = self.inference.resolve(&value.ty);
        value.ty.set_function_purity(expected);
        self.pure_values = true;
        Ok(value)
    }

    fn callback_adapter(
        &mut self,
        value: TypedExpr,
        expected: &Type,
    ) -> Result<TypedExpr, Diagnostic> {
        let actual = self.inference.resolve(&value.ty);
        let Type::Function(actual_parameters, actual_result) = &actual else {
            return Ok(value);
        };
        let Type::Function(expected_parameters, expected_result) = expected else {
            return Ok(value);
        };
        let mut exact = self.inference.clone();
        if exact
            .unify(&actual, expected, &self.types, value.span)
            .is_ok()
        {
            self.inference = exact;
            return Ok(value);
        }
        if actual_parameters.len() != expected_parameters.len() {
            return Ok(value);
        }
        let mut trial = self.inference.clone();
        let mut adapted = false;
        for (actual, expected) in actual_parameters.iter().zip(expected_parameters.iter()) {
            let (actual, expected) = match (actual, expected) {
                (Type::Reference(inner, false), expected)
                    if !matches!(expected, Type::Reference(..)) =>
                {
                    adapted = true;
                    (inner.as_ref(), expected)
                }
                (actual, Type::Reference(inner, false))
                    if !matches!(actual, Type::Reference(..)) =>
                {
                    adapted = true;
                    (actual, inner.as_ref())
                }
                _ => (actual, expected),
            };
            if trial
                .unify(actual, expected, &self.types, value.span)
                .is_err()
            {
                return Ok(value);
            }
        }
        if !adapted
            || trial
                .unify(actual_result, expected_result, &self.types, value.span)
                .is_err()
        {
            return Ok(value);
        }
        self.inference = trial;
        let span = value.span;
        let captured = self.callback_local(self.inference.resolve(&value.ty), span);
        let mut parameters = Vec::new();
        let mut arguments = Vec::new();
        for (parameter, actual) in expected_parameters.iter().zip(actual_parameters.iter()) {
            let parameter = self.callback_local(self.inference.resolve(parameter), span);
            let value = TypedExpr {
                kind: TypedExprKind::Local(parameter.id),
                ty: parameter.ty.clone(),
                span,
            };
            arguments.push(self.coerce_argument(value, actual)?);
            parameters.push(parameter);
        }
        let result = self.inference.resolve(expected_result);
        let callback = TypedExpr {
            kind: TypedExprKind::Local(captured.id),
            ty: captured.ty.clone(),
            span,
        };
        let lambda = TypedExpr {
            kind: TypedExprKind::Lambda {
                parameters,
                captures: vec![captured.clone()],
                body: Box::new(TypedExpr {
                    kind: TypedExprKind::Call(Box::new(callback), arguments),
                    ty: result,
                    span,
                }),
                owned: false,
                recursive: Vec::new(),
            },
            ty: self.inference.resolve(expected),
            span,
        };
        Ok(TypedExpr {
            ty: lambda.ty.clone(),
            kind: TypedExprKind::Block {
                bindings: vec![(captured, value)],
                result: Box::new(lambda),
            },
            span,
        })
    }

    /// Whether `actual`, open only in number literal types (as `ref {integer}`
    /// is), can still become `expected`; such a value is not a reference.
    fn literal_match(&self, actual: &Type, expected: &Type) -> bool {
        fn closed(checker: &Checker<'_>, ty: &Type, open: &mut bool) -> bool {
            match ty {
                Type::Infer(_) => {
                    *open = true;
                    checker.inference.is_numeric_literal(ty)
                }
                Type::Partial(_) | Type::Application(..) => false,
                Type::Array(ty)
                | Type::ArrayView(ty)
                | Type::FixedArray(ty, _)
                | Type::List(ty)
                | Type::Vec(ty)
                | Type::Task(ty)
                | Type::Shared(ty, _)
                | Type::Reference(ty, _) => closed(checker, ty, open),
                Type::Function(parameters, result) => {
                    parameters.iter().all(|ty| closed(checker, ty, open))
                        && closed(checker, result, open)
                }
                Type::Tuple(types) => types.iter().all(|ty| closed(checker, ty, open)),
                Type::Record(_, types) | Type::Union(_, types) => {
                    types.iter().all(|ty| closed(checker, ty, open))
                }
                _ => true,
            }
        }
        let actual = self.inference.resolve(actual);
        let mut open = false;
        if !closed(self, &actual, &mut open) || !open {
            return false;
        }
        let mut trial = self.inference.clone();
        trial
            .unify(&actual, expected, &self.types, Span::default())
            .is_ok()
    }

    fn require_mutable_reference(place: &TypedExpr) -> Result<(), Diagnostic> {
        if matches!(&place.kind, TypedExprKind::Dereference(reference) if matches!(reference.ty, Type::Reference(_, false)))
        {
            Err(Diagnostic::of(
                DiagnosticKind::MutateThroughSharedReference,
                "cannot mutate or exclusively reborrow through a shared reference",
                place.span,
            ))
        } else {
            Ok(())
        }
    }

    fn autoderef(mut value: TypedExpr) -> TypedExpr {
        while let Some(ty) = value.ty.dereferenced() {
            let span = value.span;
            value = TypedExpr {
                kind: TypedExprKind::Dereference(Box::new(value)),
                ty,
                span,
            };
        }
        value
    }

    /// Types `$"a{x}b"`: every hole is borrowed, never consumed, and shown
    /// by `Display.display`; the pieces are joined into one string at run time.
    #[inline(never)]
    fn interpolation(
        &mut self,
        interpolation: &Interpolation,
        span: Span,
        expected: Option<&Type>,
    ) -> Result<TypedExpr, Diagnostic> {
        let utf8 = matches!(interpolation.texts[0], StringLiteral::Utf8(_));
        let result = if utf8 { Type::Utf8String } else { Type::String };
        let mut holes = Vec::with_capacity(interpolation.holes.len());
        for hole in &interpolation.holes {
            holes.push(self.interpolation_hole(hole, utf8)?);
        }
        let kind = TypedExprKind::Interpolated(Box::new(TypedInterpolation {
            texts: interpolation.texts.clone(),
            holes,
        }));
        self.finish_expression(kind, result, expected, span)
    }

    #[inline(never)]
    fn interpolation_hole(
        &mut self,
        hole: &InterpolationHole,
        utf8: bool,
    ) -> Result<TypedHole, Diagnostic> {
        let expression = &hole.value;
        let value = self.expression(expression, None)?;
        let operand = self.hole_operand(expression, value)?;
        let Type::Reference(inner, _) = &operand.ty else {
            // Error recovery left a hole of unknown type; nothing is shown.
            return Ok(TypedHole {
                operand,
                method: None,
                spec: hole.spec,
                custom: false,
            });
        };
        let inner = (**inner).clone();
        let own_text = if utf8 { Type::Utf8String } else { Type::String };
        let formatted = hole
            .spec
            .is_some_and(|spec| spec.plus || spec.precision.is_some() || spec.kind.is_some());
        // A spec on a record or union belongs to the type's `Format` instance.
        // Without an instance, a sign, precision, or type is an error and a
        // width and alignment pad the `Display` text.
        let resolved = self.inference.resolve(&inner);
        let custom = hole.spec.is_some()
            && matches!(resolved, Type::Record(..) | Type::Union(..))
            && (formatted || self.has_format_instance(&resolved));
        let direct = !formatted && resolved == own_text;
        let method = if custom {
            let (kind, ty) = self.format_method(expression.span)?;
            let shown = Type::function(
                vec![
                    Type::Reference(Box::new(inner.clone()), false),
                    Type::Reference(Box::new(Type::String), false),
                ],
                Type::String,
            );
            self.same(&ty, &shown, expression.span)?;
            Some(TypedExpr {
                kind,
                ty: self.inference.resolve(&ty),
                span: expression.span,
            })
        } else if direct || formatted {
            None
        } else {
            let (kind, ty) = self.display_method(expression.span)?;
            let shown = Type::function(
                vec![Type::Reference(Box::new(inner.clone()), false)],
                Type::String,
            );
            self.same(&ty, &shown, expression.span)?;
            Some(TypedExpr {
                kind,
                ty: self.inference.resolve(&ty),
                span: expression.span,
            })
        };
        if let Some(spec) = hole.spec.filter(|_| !custom) {
            self.format_specs.push((inner, spec));
        }
        Ok(TypedHole {
            operand,
            method,
            spec: hole.spec,
            custom,
        })
    }

    /// The borrow a hole shows: a place is borrowed as an argument of a
    /// `ref` parameter is, a reference is used as it is, and any other value
    /// is a temporary that the hole drops after copying its text.
    fn hole_operand(
        &mut self,
        expression: &Expr,
        value: TypedExpr,
    ) -> Result<TypedExpr, Diagnostic> {
        // A bare name is a place unless it builds a value: a union case such as
        // `Light`, or a call of a function without parameters. Projections use
        // their typed value: `s.length`, `s[0]`, and `(f x).name` are not places.
        let place = match &expression.kind {
            ExprKind::Name(_) => !matches!(
                value.kind,
                TypedExprKind::Construct { .. } | TypedExprKind::Call(..)
            ),
            ExprKind::Field(..) | ExprKind::Index(..) | ExprKind::Dereference(..) => {
                self.hole_place(&value)
            }
            _ => false,
        };
        if place {
            let expected = Type::Reference(Box::new(self.inference.fresh()), false);
            return self.coerce_argument(value, &expected);
        }
        let span = value.span;
        Ok(match self.inference.resolve(&value.ty) {
            Type::Reference(inner, false) => TypedExpr {
                ty: Type::Reference(inner, false),
                ..value
            },
            Type::Reference(_, true) => {
                let operand = Self::reborrow_operand(value);
                TypedExpr {
                    ty: Type::Reference(Box::new(operand.ty.clone()), false),
                    kind: TypedExprKind::Borrow(Box::new(operand), false),
                    span,
                }
            }
            ty => TypedExpr {
                kind: TypedExprKind::BorrowOperand(Box::new(value)),
                ty: Type::Reference(Box::new(ty), false),
                span,
            },
        })
    }

    /// Keep parts of plain constants borrowed: folding turns them into static places.
    #[inline(never)]
    fn hole_place(&self, value: &TypedExpr) -> bool {
        value.is_place() || self.constant_part(value)
    }

    #[inline(never)]
    fn constant_part(&self, value: &TypedExpr) -> bool {
        match &value.kind {
            TypedExprKind::Field(base, _) => self.plain_constant(base) || self.constant_part(base),
            TypedExprKind::Index(base, _) => {
                matches!(
                    base.ty,
                    Type::Array(_) | Type::List(_) | Type::Vec(_) | Type::FixedArray(..)
                ) && (self.plain_constant(base) || self.constant_part(base))
            }
            _ => false,
        }
    }

    /// Checks the format specs of this body's holes against the final types.
    pub(super) fn check_format_specs(&mut self) -> Result<(), Diagnostic> {
        for (ty, spec) in std::mem::take(&mut self.format_specs) {
            let ty = self.inference.resolve(&ty);
            if ty.contains_error() {
                continue;
            }
            let spelled = spec_text(&spec);
            let needs_number = spec.plus || spec.precision.is_some() || spec.kind.is_some();
            if needs_number && matches!(ty, Type::Infer(_) | Type::Variable(_)) {
                return Err(Diagnostic::of(
                    DiagnosticKind::FormatSpecTypeUnknown,
                    format!(
                        "format spec '{spelled}' needs a concrete numeric type, found {}; annotate the value's type",
                        ty.display(&self.types)
                    ),
                    spec.span,
                ));
            }
            let reason = if spec.plus && !ty.is_numeric() {
                Some("'+' needs a number")
            } else if spec.precision.is_some() && !ty.is_float() {
                Some("precision needs a floating-point or decimal number")
            } else if matches!(
                spec.kind,
                Some(
                    FormatKind::LowerHex
                        | FormatKind::UpperHex
                        | FormatKind::Octal
                        | FormatKind::Binary
                )
            ) && !ty.is_integer()
            {
                Some("'x', 'X', 'o' and 'b' need an integer")
            } else {
                None
            };
            if let Some(reason) = reason {
                return Err(Diagnostic::of(
                    DiagnosticKind::FormatSpecTypeMismatch,
                    format!(
                        "format spec '{spelled}' does not apply to {}; {reason}",
                        ty.display(&self.types)
                    ),
                    spec.span,
                ));
            }
        }
        Ok(())
    }

    /// Record literals are checked outside `value_expression` so their
    /// locals do not enlarge the frame retained by every nested expression.
    fn record_literal(
        &mut self,
        name: &Ident,
        fields: &[(Ident, Expr)],
        expected: Option<&Type>,
        span: Span,
    ) -> Result<(TypedExprKind, Type), Diagnostic> {
        let id = self.names.record(self.module, &name.text, name.span)?;
        self.record_storage(id, name.span)?;
        self.note_name(name, NameTarget::Record(id));
        let types = self.types;
        let record = &types.records[id];
        // Type arguments come from the expected type when it names this
        // record; otherwise the field values determine them.
        let args = match expected.map(|ty| self.inference.resolve(ty)) {
            Some(Type::Record(expected_id, args)) if expected_id == id => args,
            Some(Type::Error) => record.parameters.iter().map(|_| Type::Error).collect(),
            _ => record
                .parameters
                .iter()
                .map(|_| self.inference.fresh())
                .collect(),
        };
        let field_types = types.record_fields(id, &args);
        let mut seen = BTreeSet::new();
        let mut values = Vec::new();
        for (field, value) in fields {
            if !seen.insert(&field.text) {
                return Err(duplicate(field));
            }
            let index = record
                .fields
                .iter()
                .position(|(name, _)| name == &field.text)
                .ok_or_else(|| {
                    Diagnostic::of(
                        DiagnosticKind::RecordHasNoField,
                        format!("record '{}' has no field '{}'", record.name, field.text),
                        field.span,
                    )
                    .with_help(field_names_help(&record.name, &record.fields))
                })?;
            self.note_name(field, NameTarget::Field(id, index));
            values.push((index, self.expression(value, Some(&field_types[index]))?));
        }
        let missing: Vec<_> = record
            .fields
            .iter()
            .filter(|(name, _)| !seen.contains(name))
            .map(|(name, _)| name.as_str())
            .collect();
        if !missing.is_empty() {
            return Err(Diagnostic::of(
                DiagnosticKind::MissingRecordFields,
                format!("missing fields: {}", missing.join(", ")),
                span,
            ));
        }
        let ty = self.inference.resolve(&Type::Record(id, args));
        validate_size(&ty, &self.types, span)?;
        Ok((TypedExprKind::Record(values), ty))
    }

    /// An index, a slice bound or an array length (`new [T](length, f)`) in any integer type, as
    /// the `i64` that addresses the collection. An `i64` stays as it is, and an unsuffixed literal
    /// or an undecided type becomes one. A narrower type widens; `i64u` keeps its bits, so a value
    /// above `i64.max` reads as negative; a 128-bit type or a type parameter saturates to `i64`
    /// (`SaturatingIndex`). A value that `i64` cannot hold therefore becomes one that no collection
    /// reaches, and it traps where an `i64` index would. Up to 64 bits the conversion is `as i64`,
    /// so `xs[i]` and `xs[i as i64]` compile to the same code.
    fn index_operand(&mut self, index: &Expr) -> Result<TypedExpr, Diagnostic> {
        let index = self.expression(index, None)?;
        let span = index.span;
        let cast = |value: TypedExpr, ty: Type| TypedExpr {
            kind: TypedExprKind::Cast(Box::new(value)),
            ty,
            span,
        };
        match self.inference.resolve(&index.ty) {
            ty if ty.contains_error() => Ok(TypedExpr::error(span)),
            Type::I64 => Ok(index),
            Type::Integer(bits, _) if bits <= 64 => Ok(cast(index, Type::I64)),
            ty @ Type::Infer(_) => {
                self.same(&ty, &Type::I64, span)?;
                Ok(index)
            }
            ty @ (Type::Integer(..) | Type::Variable(_)) => {
                self.require("Integer", ty, span)?;
                Ok(TypedExpr {
                    kind: TypedExprKind::SaturatingIndex(Box::new(index)),
                    ty: Type::I64,
                    span,
                })
            }
            ty => Err(Diagnostic::of(
                DiagnosticKind::IndexNotInteger,
                format!(
                    "an index must have an integer type (i8 to i128 or i8u to i128u), found {}",
                    ty.display(&self.types)
                ),
                span,
            )),
        }
    }

    fn slice(
        &mut self,
        source: &Expr,
        start: Option<&Expr>,
        end: Option<&Expr>,
        mutable: bool,
        span: Span,
    ) -> Result<(TypedExprKind, Type), Diagnostic> {
        let mut source = self.expression(source, None)?;
        source.ty = self.inference.resolve(&source.ty);
        let source = Self::autoderef(source);
        if source.ty == Type::Error {
            return Ok((TypedExprKind::Error, Type::Error));
        }
        let element = match &source.ty {
            Type::Array(element) => element.clone(),
            // A16: the shared slice of a fixed-length array borrows its storage in place.
            Type::FixedArray(element, _) if !mutable => {
                self.fixed_slice_source(&source)?;
                element.clone()
            }
            Type::FixedArray(..) => {
                return Err(Diagnostic::of(
                    DiagnosticKind::SliceOfFixedArray,
                    "a fixed-length array has no exclusive slices; replace the whole value through 'let mut', or use '[T]'",
                    span,
                ));
            }
            _ => {
                return Err(Diagnostic::of(
                    DiagnosticKind::SliceNonArray,
                    "a slice requires an array; lists and strings do not support array slicing",
                    span,
                ));
            }
        };
        if mutable {
            Self::require_mutable_reference(&source)?;
            Self::named_slice_source(&source, span)?;
        }
        let start = start
            .map(|value| self.index_operand(value).map(Box::new))
            .transpose()?;
        let end = end
            .map(|value| self.index_operand(value).map(Box::new))
            .transpose()?;
        let ty = if mutable {
            Type::Reference(Box::new(Type::ArrayView(element)), true)
        } else {
            Type::Reference(Box::new(Type::Array(element)), false)
        };
        Ok((
            TypedExprKind::Slice {
                value: Box::new(source),
                start,
                end,
            },
            ty,
        ))
    }

    /// An exclusive slice borrows a named array: a local, a field path, or a dereference.
    fn named_slice_source(source: &TypedExpr, span: Span) -> Result<(), Diagnostic> {
        if source.is_place() {
            Ok(())
        } else {
            Err(Diagnostic::of(
                DiagnosticKind::ExclusiveSliceSource,
                "an exclusive slice must borrow a named array; bind the value with 'let mut' first",
                span,
            ))
        }
    }

    /// A slice of a fixed-length array borrows a named value, so the borrow cannot outlive the
    /// slot of a temporary (A16 D7). A constant of plain data is named static data (D11 Phase 2).
    fn fixed_slice_source(&self, source: &TypedExpr) -> Result<(), Diagnostic> {
        if source.is_place() || self.plain_constant(source) {
            Ok(())
        } else {
            Err(Diagnostic::of(
                DiagnosticKind::FixedSliceSource,
                "slicing a fixed-length array requires a named value; bind it with 'let' first",
                source.span,
            ))
        }
    }

    /// A literal where a fixed-length array is expected (A16): exactly its length of elements, in
    /// order, as a tuple-like aggregate.
    #[inline(never)]
    fn fixed_array_literal(
        &mut self,
        values: &[Expr],
        element: Type,
        length: Type,
        span: Span,
    ) -> Result<(TypedExprKind, Type), Diagnostic> {
        let found = values.len() as u64;
        let length = match length {
            // An open length, as of a generic `[T; N]` parameter, is the literal's.
            Type::Infer(_) => {
                self.same(&Type::Length(found), &length, span)?;
                Type::Length(found)
            }
            length => length,
        };
        let ty = Type::FixedArray(Box::new(element.clone()), Box::new(length.clone()));
        if length != Type::Length(found) && !length.contains_error() {
            return Err(Diagnostic::of(
                DiagnosticKind::FixedArrayLiteralLength,
                format!(
                    "expected {} elements for '{}', found {found}",
                    length.display(&self.types),
                    self.inference.resolve(&ty).display(&self.types)
                ),
                span,
            ));
        }
        let mut checked = Vec::with_capacity(values.len());
        for value in values {
            let value = self.expression(value, Some(&element))?;
            self.same(&value.ty, &element, value.span)?;
            checked.push(value);
        }
        validate_size(&self.inference.resolve(&ty), &self.types, span)?;
        Ok((TypedExprKind::Tuple(checked), ty))
    }

    /// The exclusive slice of the whole array `source`, a place of type `[T]` (C08).
    fn whole_view(source: TypedExpr) -> (TypedExprKind, Type) {
        let element = match &source.ty {
            Type::Array(element) => element.clone(),
            _ => unreachable!("a whole view slices an array place"),
        };
        (
            TypedExprKind::Slice {
                value: Box::new(source),
                start: None,
                end: None,
            },
            Type::Reference(Box::new(Type::ArrayView(element)), true),
        )
    }

    /// `ref mut value` after the operand is checked: a reborrow of an exclusive slice stays an
    /// exclusive slice, and the whole array becomes one where an exclusive slice is expected.
    #[inline(never)]
    fn exclusive_borrow(
        value: TypedExpr,
        expected: Option<&Type>,
    ) -> Result<(TypedExprKind, Type), Diagnostic> {
        let through_view =
            matches!(&value.kind, TypedExprKind::Dereference(inner) if inner.ty.is_view());
        if through_view {
            if let Some(Type::Reference(inner, true)) = expected
                && !matches!(**inner, Type::ArrayView(_) | Type::Infer(_) | Type::Error)
            {
                return Err(Self::whole_array_error(value.span));
            }
            return Ok(Self::whole_view(value));
        }
        if expected.is_some_and(Type::is_view) && matches!(value.ty, Type::Array(_)) {
            Self::named_slice_source(&value, value.span)?;
            return Ok(Self::whole_view(value));
        }
        let ty = Type::Reference(Box::new(value.ty.clone()), true);
        Ok((TypedExprKind::Borrow(Box::new(value), true), ty))
    }

    fn whole_array_error(span: Span) -> Diagnostic {
        Diagnostic::of(
            DiagnosticKind::ExclusiveSliceWholeArrayParameter,
            "an exclusive slice cannot be passed as 'ref mut [T]' because that parameter may replace the whole array; declare the parameter as 'ref mut [T..]'",
            span,
        )
    }

    /// The call argument conversions of exclusive slices (C08 type rule 6); `Err` hands the value
    /// back when no conversion applies.
    #[inline(never)]
    fn coerce_slice_argument(
        &mut self,
        value: TypedExpr,
        expected: &Type,
    ) -> Result<Result<TypedExpr, TypedExpr>, Diagnostic> {
        if let Type::Reference(target, false) = expected
            && let Type::Array(element) = &**target
        {
            return self.fixed_array_view(value, element, expected);
        }
        let Type::Reference(target, true) = expected else {
            return Ok(Err(value));
        };
        let actual = self.inference.resolve(&value.ty);
        let mut value = value;
        value.ty = actual.clone();
        let span = value.span;
        let converted = if matches!(**target, Type::ArrayView(_)) {
            match &actual {
                ty if ty.is_view() => Self::whole_view(Self::reborrow_operand(value)),
                Type::Reference(inner, true) if matches!(**inner, Type::Array(_)) => {
                    Self::whole_view(Self::reborrow_operand(value))
                }
                Type::Reference(inner, false) if matches!(**inner, Type::Array(_)) => {
                    return Err(Diagnostic::of(
                        DiagnosticKind::MutateThroughSharedReference,
                        "cannot mutate or exclusively reborrow through a shared reference",
                        span,
                    ));
                }
                Type::Array(_) => {
                    Self::require_mutable_reference(&value)?;
                    Self::named_slice_source(&value, span)?;
                    Self::whole_view(value)
                }
                _ => return Ok(Err(value)),
            }
        } else if actual.is_view() && !matches!(**target, Type::Error) {
            return Err(Self::whole_array_error(span));
        } else {
            return Ok(Err(value));
        };
        let (kind, ty) = converted;
        self.finish_expression(kind, ty, Some(expected), span)
            .map(Ok)
    }

    /// A16 D7: a fixed-length array place, or `ref [T; N]`, passes to `ref [T]` as the shared
    /// slice of all its elements; `Err` hands any other value back.
    fn fixed_array_view(
        &mut self,
        value: TypedExpr,
        element: &Type,
        expected: &Type,
    ) -> Result<Result<TypedExpr, TypedExpr>, Diagnostic> {
        let actual = self.inference.resolve(&value.ty);
        let fixed = match &actual {
            Type::FixedArray(..) => true,
            Type::Reference(inner, _) => matches!(**inner, Type::FixedArray(..)),
            _ => false,
        };
        if !fixed {
            return Ok(Err(value));
        }
        let mut value = value;
        value.ty = actual;
        let source = Self::autoderef(value);
        self.fixed_slice_source(&source)?;
        let Type::FixedArray(actual_element, _) = &source.ty else {
            unreachable!("autoderef reaches the fixed-length array")
        };
        let span = source.span;
        self.same(&actual_element.clone(), element, span)?;
        let kind = TypedExprKind::Slice {
            value: Box::new(source),
            start: None,
            end: None,
        };
        let ty = Type::Reference(Box::new(Type::Array(Box::new(element.clone()))), false);
        self.finish_expression(kind, ty, Some(expected), span)
            .map(Ok)
    }

    fn record_update(
        &mut self,
        base: &Expr,
        fields: &[(Ident, Expr)],
        span: Span,
    ) -> Result<(TypedExprKind, Type), Diagnostic> {
        let mut base = self.expression(base, None)?;
        base.ty = self.inference.resolve(&base.ty);
        let base = Self::autoderef(base);
        if base.ty == Type::Error {
            return Ok((TypedExprKind::Error, Type::Error));
        }
        let Type::Record(id, args) = &base.ty else {
            return Err(Diagnostic::of(
                DiagnosticKind::RecordUpdateNonRecord,
                "record update requires a record value",
                span,
            ));
        };
        self.record_storage(*id, span)?;
        let id = *id;
        let types = self.types;
        let record = &types.records[id];
        let field_types = types.record_fields(id, args);
        let ty = base.ty.clone();
        let mut values = Vec::new();
        let mut seen = BTreeSet::new();
        for (field, value) in fields {
            if !seen.insert(&field.text) {
                return Err(duplicate(field));
            }
            let index = record
                .fields
                .iter()
                .position(|(name, _)| name == &field.text)
                .ok_or_else(|| {
                    Diagnostic::of(
                        DiagnosticKind::RecordHasNoField,
                        format!("record '{}' has no field '{}'", record.name, field.text),
                        field.span,
                    )
                    .with_help(field_names_help(&record.name, &record.fields))
                })?;
            self.note_name(field, NameTarget::Field(id, index));
            values.push((index, self.expression(value, Some(&field_types[index]))?));
        }
        Ok((
            TypedExprKind::RecordUpdate {
                base: Box::new(base),
                fields: values,
            },
            ty,
        ))
    }

    fn field_access(
        &mut self,
        value: &Expr,
        field: &Ident,
    ) -> Result<(TypedExprKind, Type), Diagnostic> {
        let mut value = self.expression(value, None)?;
        value.ty = self.inference.resolve(&value.ty);
        let value = Self::autoderef(value);
        match &value.ty {
            Type::Error => Ok((TypedExprKind::Error, Type::Error)),
            Type::Record(id, args) => {
                self.record_storage(*id, field.span)?;
                let index = self.types.records[*id]
                    .fields
                    .iter()
                    .position(|(name, _)| name == &field.text)
                    .ok_or_else(|| {
                        Diagnostic::of(
                            DiagnosticKind::UnknownField,
                            format!("unknown field '{}'", field.text),
                            field.span,
                        )
                        .with_help(field_names_help(
                            &self.types.records[*id].name,
                            &self.types.records[*id].fields,
                        ))
                    })?;
                let ty = self.types.record_field(*id, args, index);
                let id = *id;
                self.note_name(field, NameTarget::Field(id, index));
                Ok((TypedExprKind::Field(Box::new(value), index), ty))
            }
            Type::Array(_) | Type::List(_) | Type::Vec(_) | Type::FixedArray(..)
                if field.text == "length" =>
            {
                Ok((TypedExprKind::Length(Box::new(value)), Type::I64))
            }
            Type::String | Type::Utf8String if field.text == "length" => {
                Ok((TypedExprKind::StringLength(Box::new(value)), Type::I64))
            }
            _ => Err(Diagnostic::of(
                DiagnosticKind::FieldAccessNonRecord,
                "field access requires a record, or '.length' on an array, list, string, or utf8string",
                field.span,
            )),
        }
    }

    fn record_storage(&self, id: usize, span: Span) -> Result<(), Diagnostic> {
        let record = &self.types.records[id];
        if record.opaque()
            && record
                .name
                .rsplit_once('.')
                .is_some_and(|(module, _)| module != self.module)
        {
            return Err(Diagnostic::of(
                DiagnosticKind::OpaqueTypeRepresentation,
                format!(
                    "the representation of '{}' is opaque; use its module API",
                    type_display(&record.name)
                ),
                span,
            ));
        }
        Ok(())
    }

    /// Keyword `ref r` on a reference `r` builds the same tree as the symbol reborrow `&*r`.
    fn reborrow_operand(value: TypedExpr) -> TypedExpr {
        let Some(ty) = value.ty.dereferenced() else {
            return value;
        };
        let span = value.span;
        TypedExpr {
            kind: TypedExprKind::Dereference(Box::new(value)),
            ty,
            span,
        }
    }

    /// `a *b` and `a &b` pass prefix arguments; point to the spaced binary form when the call fails.
    fn operator_spacing_hint(mut error: Diagnostic, arguments: &[Expr]) -> Diagnostic {
        if !matches!(error.code, "E1005" | "E1006") {
            return error;
        }
        let symbol = arguments.iter().find_map(|argument| match &argument.kind {
            ExprKind::Dereference(value, Notation::Symbol)
                if value.span.start == argument.span.start + 1 =>
            {
                Some('*')
            }
            ExprKind::Borrow(value, false, Notation::Symbol)
                if value.span.start == argument.span.start + 1 =>
            {
                Some('&')
            }
            _ => None,
        });
        if let Some(symbol) = symbol {
            error.message.push_str(&format!(
                "; '{symbol}' written directly before an operand starts a prefix argument, so write 'a {symbol} b' with spaces on both sides for the binary operator"
            ));
        }
        error
    }

    fn untyped_number(expression: &Expr) -> bool {
        match &expression.kind {
            ExprKind::Integer(_, None) | ExprKind::Float(_, None) => true,
            ExprKind::Unary(_, value) => Self::untyped_number(value),
            _ => false,
        }
    }

    fn integer(
        &mut self,
        value: u128,
        suffix: Option<&str>,
        expected: Option<&Type>,
        negative: bool,
        span: Span,
    ) -> Result<(TypedExprKind, Type), Diagnostic> {
        if suffix.is_none()
            && let Some(ty) = expected.filter(|ty| ty.is_float())
        {
            // A float context reads an unsuffixed integer as that float: `let r: f64 = 2`.
            let float = TypedExpr {
                kind: TypedExprKind::Float(crate::numeric::float_literal(
                    &value.to_string(),
                    ty,
                    span,
                )?),
                ty: ty.clone(),
                span,
            };
            let kind = if negative {
                TypedExprKind::Unary(UnaryOp::Negate, Box::new(float))
            } else {
                float.kind
            };
            return Ok((kind, ty.clone()));
        }
        if suffix.is_none() && expected.is_some_and(|ty| self.is_bigint(ty)) {
            let value = self.bigint_literal(&value.to_string(), negative, expected, span)?;
            return Ok((value.kind, value.ty));
        }
        if suffix.is_none() && expected.is_none_or(polymorph::is_unknown) {
            // Like Rust, an unsuffixed integer takes the type that its uses
            // require, and `i32` when nothing does.
            let ty = expected.cloned().unwrap_or_else(|| self.inference.fresh());
            self.require(
                if negative { "SignedInteger" } else { "Integer" },
                ty.clone(),
                span,
            )?;
            self.inference.default_numeric(&ty, Type::I32);
            self.integer_literals.push(span);
            return Ok((TypedExprKind::GenericInteger(value, negative), ty));
        }
        Self::integer_literal(value, suffix, expected, negative, span, &self.types)
    }

    fn integer_literal(
        value: u128,
        suffix: Option<&str>,
        expected: Option<&Type>,
        negative: bool,
        span: Span,
        types: &TypeContext<'_>,
    ) -> Result<(TypedExprKind, Type), Diagnostic> {
        let ty = suffix
            .and_then(crate::numeric::primitive)
            .or_else(|| expected.filter(|ty| ty.is_integer()).cloned())
            .unwrap_or(Type::I32);
        let Type::Integer(bits, signed) = ty else {
            return Err(Diagnostic::of(
                DiagnosticKind::IntegerLiteralType,
                "an integer literal needs an integer type",
                span,
            ));
        };
        let maximum = if signed {
            (1u128 << (bits - 1)) - u128::from(!negative)
        } else if bits == 128 {
            u128::MAX
        } else {
            (1u128 << bits) - 1
        };
        if value > maximum || (negative && !signed) {
            return Err(Diagnostic::of(
                DiagnosticKind::IntegerLiteralOutOfRange,
                format!(
                    "integer literal is outside the range of {}",
                    ty.display(types)
                ),
                span,
            ));
        }
        let value = if negative {
            value.wrapping_neg()
        } else {
            value
        };
        let value = if bits == 128 {
            value
        } else {
            value & ((1u128 << bits) - 1)
        };
        Ok((TypedExprKind::Int(value), ty))
    }

    fn local(&self, name: &str) -> Option<&Local> {
        self.scopes.iter().rev().find_map(|scope| scope.get(name))
    }

    /// Whether `name.x` can only mean a function or case of the namespace
    /// `name`: a visible module, a std module, or a builtin prefix such as `Task`.
    fn is_namespace(&self, name: &str) -> bool {
        let prefix = format!("{name}.");
        self.names
            .module_path(self.module, name)
            .is_some_and(|key| self.names.searchable(self.module, key))
            || self.names.namespace_path(self.module, name).is_some()
            || self
                .names
                .modules
                .range(prefix.clone()..)
                .next()
                .is_some_and(|(module, _)| {
                    module.starts_with(&prefix) && self.names.searchable(self.module, module)
                })
            || crate::stdlib::is_reserved_module(name)
            || Builtin::ALL.iter().any(|builtin| {
                builtin
                    .name()
                    .split_once('.')
                    .is_some_and(|(prefix, _)| prefix == name)
            })
    }

    fn qualified_function(&mut self, name: &Ident) -> Result<(TypedExprKind, Type), Diagnostic> {
        // Compiler-generated calls (such as `IO.run` for a directly run `let!`) may name
        // private std functions that user source cannot.
        if name.provenance == Provenance::Generated
            && let Some(info) = self.names.functions.get(&name.text)
        {
            return Ok(self.function(info.id));
        }
        self.names.check_path(self.module, &name.text, name.span)?;
        if let Some(path) = self.names.canonical(self.module, &name.text)
            && let Some(id) = self.names.function(self.module, &path, name.span)?
        {
            return Ok(self.function(id));
        }
        if let Some(builtin) = qualified_builtin(&name.text) {
            return self.builtin(builtin, name.span);
        }
        let hint = self
            .names
            .namespace_spelling(self.module, &name.text)
            .map_or_else(String::new, |spelling| {
                format!("; join namespaces and modules with '::', as in '{spelling}'")
            });
        Err(Diagnostic::of(
            DiagnosticKind::UnknownFunction,
            format!("unknown function '{}'{hint}", name.text),
            name.span,
        ))
    }

    fn name(&mut self, name: &Ident) -> Result<(TypedExprKind, Type), Diagnostic> {
        if let Some(local) = self.local(&name.text) {
            return Ok((TypedExprKind::Local(local.id), local.ty.clone()));
        }
        if let Some(active) = &self.active_result {
            if let Some(&case_id) = active.cases.get(&name.text) {
                return Ok(self.case_value(active.union_id, case_id));
            }
        }
        if let Some(id) = self.names.local_function(self.module, &name.text) {
            return Ok(self.function(id));
        }
        if let Some(builtin) = Builtin::ALL
            .iter()
            .find(|builtin| builtin.name() == name.text)
        {
            return self.builtin(*builtin, name.span);
        }
        if let Some(case) = self.names.case(self.module, &name.text, name.span)? {
            let (union_id, case_id) = (case.info.id, case.case);
            self.note_name(name, NameTarget::Case(union_id, case_id));
            return Ok(self.case_value(union_id, case_id));
        }
        Err(self.unknown_value(name))
    }

    /// The `E1002` of an unknown value, with the locals in scope.
    #[cold]
    #[inline(never)]
    fn unknown_value(&self, name: &Ident) -> Diagnostic {
        let scope = self.scope_entries();
        let names: Vec<&str> = scope
            .iter()
            .take(8)
            .map(|entry| entry.name.as_str())
            .collect();
        let help = if names.is_empty() {
            "check the spelling, or define the name with 'let' or 'def'".to_owned()
        } else {
            format!(
                "check the spelling, or define the name with 'let' or 'def'; names in scope: {}",
                names.join(", ")
            )
        };
        Diagnostic::of(
            DiagnosticKind::UnknownValue,
            format!("unknown value '{}'", name.text),
            name.span,
        )
        .with_help(help)
        .with_scope(scope)
    }

    /// A case used as a value: a nullary case is a union value, and a case
    /// with a payload is a one-argument constructor function.
    fn case_value(&mut self, union_id: usize, case_id: usize) -> (TypedExprKind, Type) {
        let arity = self.types.unions[union_id].parameters.len();
        let args: Box<[Type]> = match &self.active_result {
            Some(active) if active.union_id == union_id => active.arguments.clone(),
            _ => (0..arity).map(|_| self.inference.fresh()).collect(),
        };
        let union = Type::Union(union_id, args.clone());
        match self.types.union_payload(union_id, &args, case_id) {
            None => (
                TypedExprKind::Construct {
                    union_id,
                    case_id,
                    payload: None,
                },
                union,
            ),
            Some(payload) => (
                TypedExprKind::CaseConstructor {
                    union_id,
                    case_id,
                    args,
                },
                Type::function(vec![payload], union),
            ),
        }
    }

    /// The dotted path of names that `expression` spells, when its root is
    /// not a local binding.
    fn value_path(&self, expression: &Expr) -> Option<String> {
        let (path, root) = expression_path(expression)?;
        self.local(&root.text).is_none().then_some(path)
    }

    /// Whether `callee` names `Dyn.of` (A14).
    fn names_dyn_of(&self, callee: &Expr) -> bool {
        matches!(callee.kind, ExprKind::Field(..))
            && self
                .value_path(callee)
                .is_some_and(|path| qualified_builtin(&path) == Some(Builtin::DynOf))
    }

    /// The class constraints of `Dyn.of value` (A14 D9): the expected type is a `dyn` type, and
    /// the value's type implements its classes and markers. A stored dyn value of a type that
    /// dispatches the same classes is the value itself, so it satisfies them through the
    /// generated instances.
    #[inline(never)]
    fn dyn_of(&mut self, callee: &TypedExpr, span: Span) -> Result<(), Diagnostic> {
        let TypedExprKind::Function(FunctionRef::Builtin(instance)) = &callee.kind else {
            unreachable!("Dyn.of is a builtin")
        };
        let value = instance.types[0].clone();
        let dyn_type = match self.inference.resolve(&instance.types[1]) {
            Type::Dyn(dyn_type) => dyn_type,
            Type::Error => return Ok(()),
            _ => {
                return Err(Diagnostic::of(
                    DiagnosticKind::DynOfNeedsExpectedType,
                    "Dyn.of needs an expected dyn type here; annotate the binding, parameter, or field, for example let shape: dyn Shapes.Shape = Dyn.of value",
                    span,
                ));
            }
        };
        for class in &dyn_type.classes {
            self.require(class, value.clone(), span)?;
        }
        if dyn_type.copy {
            self.require("Copy", value.clone(), span)?;
        }
        if dyn_type.send {
            self.require("Send", value, span)?;
        }
        Ok(())
    }

    fn namespace_value(
        &mut self,
        expression: &Expr,
    ) -> Result<Option<(TypedExprKind, Type)>, Diagnostic> {
        let Some(path) = self.value_path(expression) else {
            return Ok(None);
        };
        let span = match &expression.kind {
            ExprKind::Field(_, field) => field.span,
            _ => expression.span,
        };
        self.names.check_path(self.module, &path, span)?;
        if let Some(canonical) = self.names.canonical(self.module, &path) {
            let standard = canonical.split_once('.').is_some_and(|(module, _)| {
                self.names.modules.get(module) == Some(&ModuleOrigin::Std)
            });
            let owning = standard
                .then(|| {
                    BORROWING_APIS
                        .iter()
                        .find(|(_, _, owned)| *owned == canonical)
                })
                .flatten();
            let function_name = owning.map_or(canonical.as_ref(), |(value, _, _)| *value);
            if let Some(id) = self.names.function(self.module, function_name, span)? {
                if self.indexing
                    && standard
                    && BORROWING_APIS
                        .iter()
                        .any(|(value, _, owned)| *value == canonical || *owned == canonical)
                {
                    self.name_uses.push((span, NameTarget::Function(id)));
                }
                let read_name = (self.read_conventions && standard && owning.is_none())
                    .then(|| {
                        BORROWING_APIS
                            .iter()
                            .find(|(value, _, _)| *value == canonical)
                            .map(|(_, read, _)| *read)
                    })
                    .flatten();
                if let Some(read_name) = read_name {
                    let read =
                        if let Some(read_id) = self.names.function(self.module, read_name, span)? {
                            Some(self.function(read_id))
                        } else {
                            qualified_builtin(read_name)
                                .map(|builtin| self.builtin(builtin, expression.span))
                                .transpose()?
                        };
                    if let Some((kind, mut ty)) = read {
                        Self::read_callback_type(&mut ty);
                        return Ok(Some((kind, ty)));
                    }
                }
                return Ok(Some(self.function(id)));
            }
        }
        qualified_builtin(&path)
            .map(|builtin| {
                if self.indexing {
                    self.name_uses
                        .push((expression.span, NameTarget::Builtin(builtin)));
                }
                let spelling = path
                    .strip_prefix(crate::stdlib::NAMESPACE)
                    .and_then(|rest| rest.strip_prefix("::"))
                    .unwrap_or(&path);
                let owning = BORROWING_APIS
                    .iter()
                    .any(|(_, _, owned)| *owned == spelling);
                let read = (self.read_conventions && !owning)
                    .then(|| {
                        BORROWING_APIS
                            .iter()
                            .find(|(value, _, _)| *value == builtin.name())
                            .and_then(|(_, read, _)| qualified_builtin(read))
                    })
                    .flatten();
                let builtin = read.unwrap_or(builtin);
                let borrowed = read.is_some() && builtin != Builtin::VecPushMut;
                let (kind, mut ty) = self.builtin(builtin, expression.span)?;
                if borrowed {
                    Self::read_callback_type(&mut ty);
                }
                if builtin == Builtin::VecPushMut
                    && let Type::Function(parameters, _) = &mut ty
                {
                    parameters.conventions =
                        vec![ParameterConvention::Own, ParameterConvention::Mut].into_boxed_slice();
                }
                Ok((kind, ty))
            })
            .transpose()
    }

    fn read_callback_type(ty: &mut Type) {
        if let Type::Function(parameters, _) = ty
            && let Some(Type::Function(callback, _)) = parameters.types.first_mut()
        {
            callback.conventions = callback
                .iter()
                .map(|ty| {
                    if matches!(ty, Type::Reference(_, false)) {
                        ParameterConvention::Read
                    } else {
                        ParameterConvention::Own
                    }
                })
                .collect();
        }
    }

    /// Resolves `Module.Case`, the current module's `Union.Case`, and
    /// `Module.Union.Case`. `None` leaves a path to module functions, class
    /// methods, and field access; a module function precedes a module case
    /// and a case of the current module's union that shares the module's name.
    fn case_reference(&self, expression: &Expr) -> Result<Option<(usize, usize)>, Diagnostic> {
        let Some(path) = self.value_path(expression) else {
            return Ok(None);
        };
        let span = expression.span;
        if path.contains('.') {
            self.names.check_path(self.module, &path, span)?;
            let function = self
                .names
                .canonical(self.module, &path)
                .is_some_and(|canonical| {
                    self.names.searchable_path(self.module, &canonical)
                        && self
                            .names
                            .functions
                            .get(canonical.as_ref())
                            .is_some_and(|info| info.visible_from(self.module))
                });
            if function {
                return Ok(None);
            }
        }
        Ok(self
            .names
            .case_path(self.module, &path, span)?
            .map(|case| (case.info.id, case.case)))
    }

    /// A fully applied case constructor builds its union value directly,
    /// without a call.
    fn parallel_arguments(
        &mut self,
        arguments: &[Expr],
        parameters: &[Type],
    ) -> Result<Vec<TypedExpr>, Diagnostic> {
        let last = arguments.len() - 1;
        let input = self.argument(&arguments[last], &parameters[last])?;
        let mut checked = arguments[..last]
            .iter()
            .zip(&parameters[..last])
            .map(|(argument, parameter)| self.argument(argument, parameter))
            .collect::<Result<Vec<_>, _>>()?;
        checked.push(input);
        Ok(checked)
    }

    fn call_kind(callee: TypedExpr, mut arguments: Vec<TypedExpr>) -> TypedExprKind {
        match callee.kind {
            TypedExprKind::Function(FunctionRef::Builtin(instance))
                if instance.builtin.is_parallel()
                    && arguments.len() == instance.builtin.scheme().parameters.len() =>
            {
                TypedExprKind::Parallel(instance.builtin, arguments)
            }
            TypedExprKind::Function(FunctionRef::Builtin(instance))
                if instance.builtin == Builtin::Not && arguments.len() == 1 =>
            {
                TypedExprKind::Unary(UnaryOp::Not, Box::new(arguments.remove(0)))
            }
            TypedExprKind::CaseConstructor {
                union_id, case_id, ..
            } if arguments.len() == 1 => TypedExprKind::Construct {
                union_id,
                case_id,
                payload: arguments.pop().map(Box::new),
            },
            kind => TypedExprKind::Call(Box::new(TypedExpr { kind, ..callee }), arguments),
        }
    }

    fn binary_type(
        &mut self,
        operator: BinaryOp,
        left: &TypedExpr,
        right: &TypedExpr,
        span: Span,
    ) -> Result<Type, Diagnostic> {
        use BinaryOp::*;
        if left.ty.contains_error() || right.ty.contains_error() {
            return Ok(Type::Error);
        }
        self.same(&right.ty, &left.ty, right.span)?;
        if matches!(operator, And | Or) {
            self.same(&left.ty, &Type::Bool, span)?;
        } else {
            self.require(
                polymorph::binary_class(operator),
                self.inference.resolve(&left.ty),
                span,
            )?;
        }
        Ok(match operator {
            Equal | NotEqual | Less | LessEqual | Greater | GreaterEqual | And | Or => Type::Bool,
            _ => left.ty.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{Builtin, ModuleOrigin, Type};
    use crate::analyze;

    #[test]
    fn interior_mutability_reaches_atomics_and_mutexes_at_any_depth() {
        let module = analyze(
            "record Plain { count: i64, names: [string] }\n\
             record Counter { hits: Atomic<i64> }\n\
             record Nested { inner: Counter, name: string }\n\
             record Locked { value: Mutex<[i64]> }\n\
             record Behind { counter: Arc<Atomic<i64>> }\n\
             record Linked { value: i64, next: Maybe<Arc<Mutex<Linked>>> }\n\
             0",
        )
        .unwrap();
        let types = module.types();
        let record = |name: &str| {
            let id = module
                .records
                .iter()
                .position(|record| {
                    record.origin == ModuleOrigin::User
                        && record.name.rsplit('.').next() == Some(name)
                })
                .unwrap();
            Type::Record(id, Box::default())
        };
        for (ty, expected) in [
            (Type::I64, false),
            (Type::String, false),
            (record("Plain"), false),
            (Type::Array(Box::new(record("Plain"))), false),
            (Type::Reference(Box::new(record("Plain")), false), false),
            (record("Counter"), true),
            (Type::Reference(Box::new(record("Counter")), false), true),
            (Type::Array(Box::new(record("Counter"))), true),
            (record("Nested"), true),
            (record("Locked"), true),
            (record("Behind"), true),
            (record("Linked"), true),
        ] {
            assert_eq!(ty.has_interior_mutability(&types), expected, "{ty:?}");
        }
    }

    #[test]
    fn string_comparison_borrows_do_not_make_pipelines_nonconsuming() {
        for literal in [r#""owned""#, r#"u8"owned""#] {
            let prefix = format!("let text = {literal}\nlet result = text |> to_string\n");
            let error = analyze(&format!("{prefix}text.length")).unwrap_err();
            assert_eq!(error.code, "E1012", "{}", error.message);
            let module = analyze(&format!("{prefix}result.length")).unwrap();
            for wasm in [false, true] {
                crate::llvm::emit_target(&module, crate::llvm::Entry::Library, wasm).unwrap();
            }
        }
    }

    #[test]
    fn rejects_std_definitions_of_qualified_builtins() {
        let error = crate::analyze_modules_with_std(
            &[("Main", "0")],
            &[(
                "std/Int.tz",
                "def test_add :: i64 -> i64 -> i64\nfn test_add x y = x",
            )],
        )
        .unwrap_err();
        assert_eq!(error.code, "E1001", "{}", error.message);
        assert_eq!(error.span.source, Some(1));
    }

    #[test]
    fn describes_every_builtin_with_a_parameterized_scheme() {
        let mut names = std::collections::BTreeSet::new();
        for builtin in Builtin::ALL {
            let scheme = builtin.scheme();
            assert!(
                names.insert(builtin.name()),
                "{} is listed twice",
                builtin.name()
            );
            if *builtin == Builtin::VecEmpty {
                assert!(scheme.parameters.is_empty());
                assert_eq!(scheme.variables, ["a"]);
            }
            let mut variables = Vec::new();
            for constraint in &scheme.constraints {
                constraint.ty.variables(&mut variables);
            }
            assert!(
                variables.iter().all(|name| scheme.variables.contains(name)),
                "{} constrains an unused variable",
                builtin.name()
            );
        }
    }

    #[test]
    fn checks_value_types_higher_order_and_forward_recursion() {
        let source = "
            record Vec2 { x: f64, y: f64 }
            fn rec even(n: i64) -> bool { if n == 0 { true } else { odd(n - 1) } }
            fn rec odd(n: i64) -> bool { if n == 0 { false } else { even(n - 1) } }
            fn apply(f: fn(i64) -> i64, n: i64) -> i64 { f(n) }
            fn square(n: i64) -> i64 { n * n }
            export fn answer() -> i64 {
                let points = [Vec2 { y: 4.0, x: 3.0 }, Vec2 { x: 0.0, y: 1.0 }];
                let first = points[0];
                let distance = sqrt(first.x * first.x + first.y * first.y);
                let x = apply(square, 6);
                let x = x + to_int(distance);
                if even(10) { x + 1 } else { points.length }
            }";
        assert!(analyze(source).is_ok());
    }

    #[test]
    fn accepts_minimum_integer_and_annotated_empty_values() {
        assert!(
            analyze(
                "record Empty {}
                 fn f() -> i64 { let a: [i64] = []; let _ = Empty {}; -9223372036854775808 + a.length }
                 export fn empty() -> unit {}"
            )
            .is_ok()
        );
    }

    #[test]
    fn rejects_invalid_programs_with_stable_codes() {
        for (source, code) in [
            ("fn f() -> i64 { true }", "E1003"),
            ("fn f(x: i64, x: i64) -> i64 { x }", "E1001"),
            ("fn sqrt() -> i64 { 1 }", "E1001"),
            ("fn f() -> i64 { missing }", "E1002"),
            ("fn f() -> Missing { 1 }", "E1004"),
            ("fn f() -> i64 { 1 + 1.0 }", "E1003"),
            ("fn f() -> i64 { 1(true) }", "E1005"),
            ("fn f() -> i64 { f(1) }", "E1006"),
            ("fn f() -> i64 { 1 |> f }", "E1006"),
            ("fn f() -> i64 { 9223372036854775808 }", "E1009"),
            ("fn f() -> i64 { -9223372036854775809 }", "E1009"),
            ("fn f() -> i64 { let xs = []; 0 }", "E1004"),
            ("fn f() -> i64 { let xs = [1, true]; 0 }", "E1003"),
            (
                "record R { x: i64 } fn f() -> bool { [R { x: 1 }] == [R { x: 1 }] }",
                "E1005",
            ),
            ("record R { x: i64 } fn f() -> R { R {} }", "E1007"),
            ("record R { x: i64 } fn f() -> R { R { z: 1 } }", "E1007"),
            (
                "record R { x: i64 } fn f() -> i64 { R { x: 1 }.z }",
                "E1007",
            ),
            (
                "record R { x: string } export fn f(r: R) -> i64 { r.x.length }",
                "E1008",
            ),
            ("export fn f(x: unit) -> i64 { 1 }", "E1008"),
            ("record A { b: B } record B { a: A }", "E1010"),
            ("record A { a: [A] }", "E1010"),
            ("fn f(a: [i64; -1]) -> i64 { 0 }", "E0002"),
        ] {
            let error = analyze(source).expect_err(source);
            assert_eq!(error.code, code, "{source}: {}", error.message);
        }
    }

    #[test]
    fn checks_all_branches_and_respects_lexical_scope() {
        assert!(analyze("fn f() -> i64 { if true { 1 } else { false } }").is_err());
        assert!(analyze("fn f() -> i64 { let x = { let y = 1; y }; y }").is_err());
        assert!(analyze("fn f() -> i64 { let x = 1; { let x = x + 1; x } }").is_ok());
    }

    #[test]
    fn type_stays_four_words() {
        // Fixed-length arrays (A16) must not grow every type and typed expression.
        assert!(std::mem::size_of::<super::Type>() <= 32);
    }

    #[test]
    fn accepts_exact_value_limits_and_rejects_the_next_scalar() {
        assert!(analyze("fn f(x: [i64]) -> i64 { x.length }").is_ok());
        assert!(analyze("record Nested { values: [[i64]] }").is_ok());
        let fields = (0..4096)
            .map(|index| format!("f{index}: i64"))
            .collect::<Vec<_>>()
            .join(", ");
        assert!(analyze(&format!("record Full {{ {fields} }}")).is_ok());
        assert_eq!(
            analyze(&format!("record TooLarge {{ {fields}, extra: i64 }}"))
                .unwrap_err()
                .code,
            "E1010"
        );
    }
}

#[cfg(test)]
mod allocation_tests {
    use super::*;

    fn child_signature(expression: &TypedExpr) -> (Span, String) {
        (expression.span, format!("{:?}", expression.kind))
    }

    fn verify_children(expression: &mut TypedExpr) -> Result<(), Diagnostic> {
        let expected: Vec<_> = expression
            .children()
            .into_iter()
            .map(child_signature)
            .collect();
        let mut shared = Vec::new();
        expression.try_for_each_child(&mut |child| {
            shared.push(child_signature(child));
            Ok(())
        })?;
        assert_eq!(shared, expected);
        let mut mutable = Vec::new();
        expression.try_for_each_child_mut(&mut |child| {
            mutable.push(child_signature(child));
            Ok(())
        })?;
        assert_eq!(mutable, expected);
        if !expected.is_empty() {
            let mut visited = 0;
            let failure = expression.try_for_each_child_mut(&mut |child| {
                visited += 1;
                Err(Diagnostic::of(
                    DiagnosticKind::PolymorphicTypeExpansionLimit,
                    "visitor stopped",
                    child.span,
                ))
            });
            assert_eq!(visited, 1);
            assert_eq!(failure.unwrap_err().span, expected[0].0);
        }
        expression.try_for_each_child_mut(&mut verify_children)
    }

    #[test]
    fn allocation_free_visitors_preserve_child_order_and_short_circuit() {
        for fixture in [
            "control",
            "computations",
            "exceptions",
            "string_interpolation",
            "local_recursion",
            "const_functions",
            "dyn_dispatch",
            "fixed_arrays",
        ] {
            let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures")
                .join(fixture);
            let mut module = crate::driver::Project::load(&path)
                .unwrap()
                .analyze_all()
                .unwrap();
            for function in &mut module.functions {
                verify_children(&mut function.body).unwrap();
            }
        }
    }

    #[test]
    fn frozen_types_reject_nested_mutability_and_hidden_opaque_state() {
        let module = crate::analyze(
            "record Plain { values: [i64] }\n\
             record Nested { cells: [Atomic<i64>] }\n\
             def plain :: ref Plain -> unit = \\_value -> ()\n\
             def atomic :: ref Atomic<i64> -> unit = \\_value -> ()\n\
             def nested :: ref Nested -> unit = \\_value -> ()\n\
             def mutex :: ref Mutex<i64> -> unit = \\_value -> ()\n\
             def sender :: ref Channel.Sender<i64> -> unit = \\_value -> ()\n\
             def receiver :: ref Channel.Receiver<i64> -> unit = \\_value -> ()",
        )
        .unwrap();
        for function in &module.functions {
            let frozen = function.parameters[0].ty.is_frozen(&module.types());
            assert_eq!(frozen, function.name == "plain", "{}", function.name);
        }
        for ty in [
            Type::Error,
            Type::Variable("a".into()),
            Type::Infer(0),
            Type::Reference(Box::new(Type::Integer(64, true)), true),
            Type::Shared(Box::new(Type::Integer(64, true)), SharedKind::Arc),
            Type::function(vec![], Type::Unit),
        ] {
            assert!(!ty.is_frozen(&module.types()), "{ty:?}");
        }
    }
}

/// The frontend shares these types between threads (PB04). Putting an `Rc`, a `Cell` or a
/// `RefCell` into one of them stops this test from compiling.
#[cfg(test)]
mod thread_safety {
    use super::*;

    fn send<T: Send>() {}
    fn sync<T: Sync>() {}

    #[test]
    fn frontend_data_crosses_threads() {
        send::<Result<Program, Vec<Diagnostic>>>();
        sync::<Program>();
        sync::<crate::SourceInput<'static>>();
        sync::<ModuleInput<'static>>();
        send::<crate::frontend_cache::Parsed>();
        sync::<crate::frontend_cache::FrontendCache>();
        send::<CheckedModule>();
        sync::<CheckedModule>();
        sync::<CheckedRecord>();
        sync::<CheckedUnion>();
        send::<CheckedFunction>();
        sync::<CheckedFunction>();
        send::<TypedExpr>();
        send::<TypeContext<'static>>();
        sync::<TypeContext<'static>>();
        sync::<Names>();
        sync::<polymorph::Classes>();
        sync::<polymorph::Scheme>();
        send::<polymorph::Inference>();
        send::<Checker<'static>>();
    }
}
