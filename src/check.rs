use std::collections::{BTreeMap, BTreeSet};

use crate::diagnostic::{Diagnostic, DiagnosticSet, Diagnostics, MAX_UNIQUE_DIAGNOSTICS, Span};
pub use crate::syntax::Provenance;
use crate::syntax::*;

#[path = "closures.rs"]
mod closures;
#[path = "computation.rs"]
mod computation;
#[path = "constants.rs"]
mod constants;
#[path = "control.rs"]
mod control;
#[path = "derive.rs"]
mod deriving;
#[path = "exhaustiveness.rs"]
mod exhaustiveness;
#[path = "higher_kinds.rs"]
mod higher_kinds;
pub use higher_kinds::{Constructor, Partial};
#[path = "polymorph.rs"]
mod polymorph;
#[path = "recursion.rs"]
mod recursion;
#[path = "recursive.rs"]
mod recursive;
#[path = "regions.rs"]
mod regions;
#[path = "semantic.rs"]
pub mod semantic;
#[path = "warnings.rs"]
mod warnings;
use polymorph::{Classes, Constraint, Inference, Scheme};

pub const MAX_VALUE_BYTES: usize = 64 * 1024;

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
    List(Box<Type>),
    Vec(Box<Type>),
    Tuple(Vec<Type>),
    Task(Box<Type>),
    /// An `extern type`: an opaque host handle named by its qualified name.
    Handle(Box<str>),
    Function(Vec<Type>, Box<Type>),
    Reference(Box<Type>, bool),
}

impl Type {
    pub const I64: Self = Self::Integer(64, true);
    pub const F64: Self = Self::Binary(64);

    pub(crate) fn shared_array_element(&self) -> Option<&Type> {
        match self {
            Self::Reference(inner, false) => match inner.as_ref() {
                Self::Array(element) => Some(element),
                _ => None,
            },
            _ => None,
        }
    }

    pub(crate) fn function(mut parameters: Vec<Type>, mut result: Type) -> Self {
        if !parameters.is_empty() {
            while matches!(&result, Self::Function(more, _) if !more.is_empty()) {
                let Self::Function(more, tail) = result else {
                    unreachable!()
                };
                parameters.extend(more);
                result = *tail;
            }
        }
        Self::Function(parameters, Box::new(result))
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
            Self::function(parameters[count..].to_vec(), (**result).clone())
        }
    }

    pub fn display(&self, types: &TypeContext<'_>) -> String {
        match self {
            Self::Error => "an erroneous type".into(),
            Self::Variable(name) => format!("'{name}"),
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
                let mut text = match self {
                    Self::Record(..) => types.records[*id].name.clone(),
                    _ => types.unions[*id].name.clone(),
                };
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
            Self::Handle(name) => name.to_string(),
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
            | Self::List(ty)
            | Self::Vec(ty)
            | Self::Task(ty)
            | Self::Reference(ty, _) => ty.contains_error(),
            Self::Function(parameters, result) => {
                parameters.iter().any(Self::contains_error) || result.contains_error()
            }
            Self::Tuple(elements) => elements.iter().any(Self::contains_error),
            Self::Record(_, args) | Self::Union(_, args) => args.iter().any(Self::contains_error),
            _ => false,
        }
    }

    pub(crate) fn contains_constructor(&self) -> bool {
        match self {
            Self::Partial(_) | Self::Application(..) => true,
            Self::Array(ty)
            | Self::List(ty)
            | Self::Vec(ty)
            | Self::Task(ty)
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
            | Self::Reference(_, true)
            | Self::Partial(_)
            | Self::Application(..)
            | Self::Variable(_)
            | Self::Infer(_) => false,
            Self::Record(id, args) => types.record_fields_all(*id, args, |ty| ty.is_copy(types)),
            Self::Union(id, args) => types.union_payloads_all(*id, args, |ty| ty.is_copy(types)),
            Self::Array(element) | Self::List(element) => element.is_copy(types),
            Self::Tuple(elements) => elements.iter().all(|ty| ty.is_copy(types)),
            _ => true,
        }
    }

    pub fn needs_drop(&self, types: &TypeContext<'_>) -> bool {
        if types.recursive(self) {
            return true;
        }
        match self {
            Self::String | Self::Utf8String | Self::Function(..) | Self::Task(_) => true,
            Self::Record(id, args) => {
                !types.record_fields_all(*id, args, |ty| !ty.needs_drop(types))
            }
            Self::Union(id, args) => {
                !types.union_payloads_all(*id, args, |ty| !ty.needs_drop(types))
            }
            Self::Array(_) | Self::List(_) | Self::Vec(_) => true,
            Self::Tuple(elements) => elements.iter().any(|ty| ty.needs_drop(types)),
            _ => false,
        }
    }

    pub(crate) fn is_noncopy_record(&self, types: &TypeContext<'_>) -> bool {
        matches!(self, Self::Record(id, _) if types.records[*id].origin == ModuleOrigin::Std && matches!(types.records[*id].name.as_str(), "Seq.Seq" | "Gpu.Device" | "Gpu.Buffer"))
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
            Self::Array(element) | Self::List(element) | Self::Vec(element) => {
                element.contains_reference()
            }
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
            | Self::Vec(value) => value.contains_mutable_reference(),
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
            });
        }
        match self {
            Self::Reference(..) | Self::Function(..) => true,
            Self::Array(element) | Self::List(element) | Self::Vec(element) => {
                element.carries_loans(types)
            }
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
        !types.stored_all(self, |ty| !matches!(ty, Self::Reference(..)))
    }

    pub(crate) fn contains_stored_mutable_reference(&self, types: &TypeContext<'_>) -> bool {
        !types.stored_all(self, |ty| !ty.contains_mutable_reference())
    }

    pub(crate) fn can_capture(&self, types: &TypeContext<'_>) -> bool {
        if types.recursive(self) {
            return types.stored_all(self, |ty| {
                !matches!(
                    ty,
                    Type::Reference(_, true) | Type::Task(_) | Type::Handle(_)
                )
            });
        }
        match self {
            Self::Reference(_, true) | Self::Task(_) | Self::Handle(_) => false,
            Self::Array(element) | Self::List(element) | Self::Vec(element) => {
                element.can_capture(types)
            }
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

    pub(crate) fn can_send(&self, types: &TypeContext<'_>) -> bool {
        if types.recursive(self) {
            return types.stored_all(self, |ty| !matches!(ty, Type::Reference(..)));
        }
        match self {
            Self::Reference(..) => false,
            Self::Array(element) | Self::List(element) | Self::Vec(element) => {
                element.can_send(types)
            }
            Self::Tuple(elements) => elements.iter().all(|ty| ty.can_send(types)),
            Self::Record(id, args) => types.record_fields_all(*id, args, |ty| ty.can_send(types)),
            Self::Union(id, args) => types.union_payloads_all(*id, args, |ty| ty.can_send(types)),
            // Function environments are checked by ownership, not by their call signatures.
            _ => true,
        }
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

    fn record_fields_all(&self, id: usize, args: &[Type], test: impl Fn(&Type) -> bool) -> bool {
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

    fn union_payloads_all(&self, id: usize, args: &[Type], test: impl Fn(&Type) -> bool) -> bool {
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
    /// `unreachable : unit -> 'a` traps (GUIDE D-21).
    Unreachable,
    ToString,
    DebugPrintString,
    IOReadLine,
    IOWrite,
    Display,
    Parse,
    Default,
    Hash,
    HashMix,
    DisplayQuoted,
    ArraySet,
    ArrayUpdate,
    ArraySwap,
    ListCons,
    ListTail,
    ArrayConcat,
    ArrayToList,
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
    SimdExtract,
    SimdReplace,
    SimdLoad,
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
    /// A std-origin record or union, such as `Option.Option<'a>`.
    Std {
        module: &'static str,
        name: &'static str,
        args: Vec<BuiltinType>,
    },
    Array(Box<BuiltinType>),
    List(Box<BuiltinType>),
    Vec(Box<BuiltinType>),
    Tuple(Vec<BuiltinType>),
    Task(Box<BuiltinType>),
    Reference(Box<BuiltinType>, bool),
    Function(Vec<BuiltinType>, Box<BuiltinType>),
    /// The unsigned integer type of the same width.
    UnsignedOf(Box<BuiltinType>),
    /// The integer type of twice the width and the same signedness;
    /// undefined for 128-bit integers.
    WidenOf(Box<BuiltinType>),
    SimdLane(Box<BuiltinType>, Option<u16>),
    SimdMask(Box<BuiltinType>),
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
        Self::Unreachable,
        Self::ToString,
        Self::DebugPrintString,
        Self::IOReadLine,
        Self::IOWrite,
        Self::Display,
        Self::Parse,
        Self::Default,
        Self::Hash,
        Self::HashMix,
        Self::DisplayQuoted,
        Self::ArraySet,
        Self::ArrayUpdate,
        Self::ArraySwap,
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
        Self::SimdExtract,
        Self::SimdReplace,
        Self::SimdLoad,
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
            Self::Unreachable => "unreachable",
            Self::ToString => "to_string",
            Self::DebugPrintString => "Debug.__print_string",
            Self::IOReadLine => "IO.__read_line",
            Self::IOWrite => "IO.__write",
            Self::Display => "$builtin.display",
            Self::Parse => "$builtin.parse",
            Self::Default => "$builtin.default",
            Self::Hash => "$builtin.hash",
            Self::HashMix => "$builtin.hash_mix",
            Self::DisplayQuoted => "$builtin.display_quoted",
            Self::ArraySet => "Array.set",
            Self::ArrayUpdate => "Array.update",
            Self::ArraySwap => "Array.swap",
            Self::ListCons => "List.cons",
            Self::ListTail => "List.tail",
            Self::ArrayConcat => "Array.concat",
            Self::ArrayToList => "Array.to_list",
            Self::ArraySortBy => "Array.sort_by",
            Self::ListMap => "List.map",
            Self::ListMapRef => "List.map_ref",
            Self::ListReverse => "List.reverse",
            Self::ListToArray => "List.to_array",
            Self::ListFoldRef => "List.fold_ref",
            Self::VecEmpty => "Vec.empty",
            Self::SeqNext => "Seq.next",
            Self::SimdSplat => "Simd.splat",
            Self::SimdOfLanes2 => "Simd.of_lanes2",
            Self::SimdOfLanes4 => "Simd.of_lanes4",
            Self::SimdOfLanes8 => "Simd.of_lanes8",
            Self::SimdOfLanes16 => "Simd.of_lanes16",
            Self::SimdExtract => "Simd.extract",
            Self::SimdReplace => "Simd.replace",
            Self::SimdLoad => "Simd.load",
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
            Self::SimdSplat
            | Self::SimdOfLanes2
            | Self::SimdOfLanes4
            | Self::SimdOfLanes8
            | Self::SimdOfLanes16
            | Self::SimdExtract
            | Self::SimdReplace
            | Self::SimdLoad
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
                    Self::SimdSum | Self::SimdAll | Self::SimdAny => vec![a()],
                    Self::SimdSelect => vec![mask(), a(), a()],
                    _ if count.is_some() => vec![lane(); usize::from(count.unwrap())],
                    _ => vec![a(), a()],
                };
                let result = match self {
                    Self::SimdExtract | Self::SimdSum => lane(),
                    Self::SimdAll | Self::SimdAny => Concrete(Type::Bool),
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
                    module: "Option",
                    name: "Option",
                    args: vec![Concrete(Type::Utf8String)],
                },
                Vec::new(),
            ),
            Self::Utf8StringDecodeAt => (
                vec![
                    Reference(Box::new(Concrete(Type::Utf8String)), false),
                    Concrete(Type::I64),
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
                    module: "Option",
                    name: "Option",
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
                    module: "Option",
                    name: "Option",
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
            | Self::VecToArray => {
                let vector = || BuiltinType::Vec(Box::new(a()));
                let borrowed = || Reference(Box::new(vector()), false);
                let option = || BuiltinType::Std {
                    module: "Option",
                    name: "Option",
                    args: vec![a()],
                };
                let parameters = match self {
                    Self::VecEmpty => vec![],
                    Self::VecWithCapacity => vec![Concrete(Type::I64)],
                    Self::VecLength | Self::VecCapacity | Self::VecIsEmpty | Self::VecClone => {
                        vec![borrowed()]
                    }
                    Self::VecGet | Self::VecAt => vec![borrowed(), Concrete(Type::I64)],
                    Self::VecPush => vec![vector(), a()],
                    Self::VecReserve | Self::VecTruncate => vec![vector(), Concrete(Type::I64)],
                    Self::VecSet => vec![vector(), Concrete(Type::I64), a()],
                    Self::VecSwap => vec![vector(), Concrete(Type::I64), Concrete(Type::I64)],
                    Self::VecOfArray => vec![Array(Box::new(a()))],
                    _ => vec![vector()],
                };
                let result = match self {
                    Self::VecLength | Self::VecCapacity => Concrete(Type::I64),
                    Self::VecIsEmpty => Concrete(Type::Bool),
                    Self::VecPop => BuiltinType::Tuple(vec![vector(), option()]),
                    Self::VecGet => option(),
                    Self::VecAt => Reference(Box::new(a()), false),
                    Self::VecToArray => Array(Box::new(a())),
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
                            read_array(),
                            BuiltinType::Function(
                                vec![borrowed(), borrowed()],
                                Box::new(Concrete(Type::I64)),
                            ),
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
                                read_list(),
                                BuiltinType::Function(vec![input], Box::new(Var("b"))),
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
                            read_list(),
                            Var("state"),
                            BuiltinType::Function(
                                vec![Var("state"), borrowed()],
                                Box::new(Var("state")),
                            ),
                        ],
                        Var("state"),
                        Vec::new(),
                    ),
                    _ => unreachable!(),
                }
            }
            Self::ArraySet | Self::ArrayUpdate | Self::ArraySwap => {
                let third = match self {
                    Self::ArraySet => a(),
                    Self::ArrayUpdate => BuiltinType::Function(vec![a()], Box::new(a())),
                    _ => Concrete(Type::I64),
                };
                (
                    vec![Array(Box::new(a())), Concrete(Type::I64), third],
                    Array(Box::new(a())),
                    Vec::new(),
                )
            }
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
                        module: "Option",
                        name: "Option",
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
                    module: "Option",
                    name: "Option",
                    args: vec![a()],
                },
                Vec::new(),
            ),
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
            | Self::List(ty)
            | Self::Vec(ty)
            | Self::Task(ty)
            | Self::Reference(ty, _)
            | Self::UnsignedOf(ty)
            | Self::WidenOf(ty)
            | Self::SimdLane(ty, _)
            | Self::SimdMask(ty) => ty.variables(found),
            Self::Function(parameters, result) => {
                parameters.iter().for_each(|ty| ty.variables(found));
                result.variables(found);
            }
        }
    }
}

impl Builtin {
    pub(crate) fn is_parallel(self) -> bool {
        matches!(
            self,
            Self::ParallelInit | Self::ParallelMap | Self::ParallelMapRef | Self::ParallelReduce
        )
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
    fn as_type(&self) -> Type {
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
}

impl FunctionOrigin {
    /// A function written in a module of the given origin.
    pub(crate) fn source(module: ModuleOrigin) -> Self {
        Self {
            module,
            provenance: Provenance::User,
            parent: None,
            test: None,
        }
    }

    /// The origin of a helper that the function `parent` with this origin generates.
    pub(crate) fn generated(self, parent: usize) -> Self {
        Self {
            provenance: Provenance::Generated,
            parent: Some(parent),
            ..self
        }
    }
}

/// A parsed module and its origin, as `check_modules` receives it.
#[derive(Clone, Copy)]
pub struct ModuleInput<'a> {
    pub name: &'a str,
    pub program: &'a Program,
    pub origin: ModuleOrigin,
}

#[derive(Debug)]
pub struct CheckedModule {
    pub records: Vec<CheckedRecord>,
    pub unions: Vec<CheckedUnion>,
    pub functions: Vec<CheckedFunction>,
    pub entry: Option<usize>,
    pub tests: Vec<CheckedTest>,
    /// Warnings in source traversal order; they never fail a check or build.
    pub warnings: Vec<Diagnostic>,
}

#[derive(Clone, Debug)]
pub struct CheckedTest {
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

/// A union declaration. Cases keep declaration order, and a case's index is
/// its runtime `i32` tag.
#[derive(Clone, Debug)]
pub struct CheckedUnion {
    pub name: String,
    pub visibility: Visibility,
    pub origin: ModuleOrigin,
    pub parameters: Vec<String>,
    pub cases: Vec<(String, Option<Type>)>,
    pub span: Span,
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

#[derive(Clone, Debug)]
pub struct CheckedFunction {
    pub module: String,
    pub(crate) region_sources: Option<BTreeSet<usize>>,
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
}

impl CheckedFunction {
    pub fn qualified_name(&self) -> String {
        format!("{}.{}", self.module, self.name)
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
    /// Copy values are still copied, which keeps them independent of later
    /// writes to the matched storage.
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
    },
    Closure(usize, Vec<TypedExpr>),
    TaskRun(Box<TypedExpr>),
    TaskParallel(Box<TypedExpr>),
    TaskParallelResults(Box<TypedExpr>),
    Parallel(Builtin, Vec<TypedExpr>),
    StructuralCompare(BinaryOp, Vec<TypedExpr>),
    StructuralHash(Vec<TypedExpr>),
    StructuralDisplay(Vec<TypedExpr>),
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
    /// A case used as a function value, `Some : 'a -> Option<'a>`. After
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
}

impl TypedExpr {
    fn error(span: Span) -> Self {
        Self {
            kind: TypedExprKind::Error,
            ty: Type::Error,
            span,
        }
    }

    pub(crate) fn children(&self) -> Vec<&Self> {
        use TypedExprKind::*;
        match &self.kind {
            Unary(_, value)
            | Borrow(value, _)
            | BorrowOperand(value)
            | Dereference(value)
            | Cast(value)
            | Field(value, _)
            | Length(value)
            | StringLength(value)
            | TaskRun(value)
            | TaskParallel(value)
            | TaskParallelResults(value)
            | NewLiteral(value)
            | UnionTag(value)
            | UnionPayload { value, .. }
            | Construct {
                payload: Some(value),
                ..
            }
            | ListTail(value, _) => vec![value],
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
            Break | Continue => Vec::new(),
            _ => Vec::new(),
        }
    }

    pub(crate) fn children_mut(&mut self) -> Vec<&mut Self> {
        use TypedExprKind::*;
        match &mut self.kind {
            Unary(_, value)
            | Borrow(value, _)
            | BorrowOperand(value)
            | Dereference(value)
            | Cast(value)
            | Field(value, _)
            | Length(value)
            | StringLength(value)
            | TaskRun(value)
            | TaskParallel(value)
            | TaskParallelResults(value)
            | NewLiteral(value)
            | UnionTag(value)
            | UnionPayload { value, .. }
            | Construct {
                payload: Some(value),
                ..
            }
            | ListTail(value, _) => vec![value],
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
            Call(callee, arguments) => std::iter::once(callee.as_mut()).chain(arguments).collect(),
            Lambda { body, .. } => vec![body],
            If {
                condition,
                then_branch,
                else_branch,
            } => vec![condition, then_branch, else_branch],
            Match { value, arms, .. } => {
                let mut children = vec![value.as_mut()];
                for arm in arms {
                    for alternative in &mut arm.alternatives {
                        children.extend(
                            alternative
                                .steps
                                .iter_mut()
                                .map(PatternStep::expression_mut),
                        );
                        children.extend(alternative.bindings.iter_mut().map(|(_, value)| value));
                    }
                    children.extend(arm.guard.iter_mut());
                    children.push(&mut arm.body);
                }
                children
            }
            Block { bindings, result } => bindings
                .iter_mut()
                .map(|(_, value)| value)
                .chain(std::iter::once(result.as_mut()))
                .collect(),
            Record(fields) => fields.iter_mut().map(|(_, value)| value).collect(),
            Slice { value, start, end } => std::iter::once(value.as_mut())
                .chain(start.iter_mut().map(Box::as_mut))
                .chain(end.iter_mut().map(Box::as_mut))
                .collect(),
            RecordUpdate { base, fields } => std::iter::once(base.as_mut())
                .chain(fields.iter_mut().map(|(_, value)| value))
                .collect(),
            Array(values)
            | HostCall(_, values)
            | List(values)
            | Tuple(values)
            | Closure(_, values)
            | Parallel(_, values)
            | StructuralCompare(_, values)
            | StructuralHash(values)
            | StructuralDisplay(values) => values.iter_mut().collect(),
            Break | Continue => Vec::new(),
            _ => Vec::new(),
        }
    }
}

/// Checks one `Main` module without a standard library.
pub fn check(program: &Program) -> Result<CheckedModule, Diagnostic> {
    check_modules(&[ModuleInput {
        name: "Main",
        program,
        origin: ModuleOrigin::User,
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
        Diagnostic::new(
            "E1022",
            format!(
                "private {kind} '{}' is only visible inside module '{}'; expose a public wrapper or move the use into the same module",
                self.name, self.module
            ),
            span,
        )
    }
}

#[derive(Default)]
struct Names {
    warning_options: warnings::WarningOptions,
    modules: BTreeMap<String, ModuleOrigin>,
    builders: BTreeMap<String, BTreeSet<String>>,
    type_aliases: BTreeMap<String, (NameInfo, TypeAliasDecl)>,
    type_alias_names: BTreeMap<String, Vec<String>>,
    records: BTreeMap<String, NameInfo>,
    record_aliases: BTreeMap<String, Vec<String>>,
    /// `extern type` handles share the type namespace with records and unions.
    handles: BTreeMap<String, NameInfo>,
    handle_aliases: BTreeMap<String, Vec<String>>,
    /// Number of type parameters of each record declaration, by record id.
    record_arities: Vec<usize>,
    /// Unions share the type namespace with records and classes.
    unions: BTreeMap<String, NameInfo>,
    union_aliases: BTreeMap<String, Vec<String>>,
    union_arities: Vec<usize>,
    /// Union cases by qualified `Module.Case`; a module declares each case name once.
    cases: BTreeMap<String, CaseInfo>,
    case_aliases: BTreeMap<String, Vec<String>>,
    /// Built-in class names and qualified user class names, as in `Classes`.
    classes: BTreeSet<String>,
    /// Qualified user class names by unqualified name.
    class_aliases: BTreeMap<String, Vec<String>>,
    functions: BTreeMap<String, NameInfo>,
    constants: BTreeSet<usize>,
    active_patterns: BTreeMap<String, (NameInfo, ActiveCase)>,
    active_aliases: BTreeMap<String, Vec<String>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ActiveCase {
    TotalSingle,
    BoolPartial,
    OptionPartial,
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

impl Names {
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

    /// Chooses among same-named declarations of other modules, one tier of
    /// module origins at a time. Private declarations neither resolve nor
    /// make a name ambiguous.
    fn choose<T: Copy>(
        &self,
        requester: &str,
        candidates: &[T],
        origin: impl Fn(T) -> ModuleOrigin,
        visible: impl Fn(T) -> bool,
    ) -> Choice<T> {
        let mut hidden = None;
        for (rank, tier) in (1..).zip(self.tiers(requester)) {
            let declared: Vec<T> = candidates
                .iter()
                .copied()
                .filter(|candidate| origin(*candidate) == *tier)
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
            (Choice::Found(..), Choice::Found(named, _)) => Err(Diagnostic::new(
                "E1004",
                format!(
                    "'{}' names both a type class and the {} type '{}'; qualify the {} type or rename one of them",
                    head.text,
                    named.kind(),
                    named.info().name,
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
            _ => Err(Diagnostic::new(
                "E1004",
                format!(
                    "unknown type class, record type, or union type '{}'; declare it or check the spelling",
                    head.text
                ),
                head.span,
            )),
        }
    }

    fn record(&self, module: &str, name: &str, span: Span) -> Result<usize, Diagnostic> {
        match self.named_type(module, name, span)? {
            NamedType::Record(info) => Ok(info.id),
            NamedType::Union(info) => Err(Diagnostic::new(
                "E1004",
                format!(
                    "'{name}' names the union type '{}', not a record type; construct or match it with its cases",
                    info.name
                ),
                span,
            )),
            NamedType::Alias(info) => Err(Diagnostic::new(
                "E1004",
                format!(
                    "'{}' is a type alias; use the original record name to construct or match a value",
                    info.name
                ),
                span,
            )),
            NamedType::Handle(info) => Err(Diagnostic::new(
                "E1004",
                format!(
                    "'{name}' names the extern type '{}', not a record type; host handles have no fields and cannot be constructed",
                    info.name
                ),
                span,
            )),
        }
    }

    /// Resolves a record or union type name: the requester's own declaration,
    /// then an exact qualified name, then the unique visible declaration of
    /// user modules and then of std modules.
    fn named_type(
        &self,
        module: &str,
        name: &str,
        span: Span,
    ) -> Result<NamedType<'_>, Diagnostic> {
        self.type_result(self.type_choice(module, name), name, span)
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
        if self.searchable_path(module, name) {
            let exact = self
                .records
                .get(name)
                .map(NamedType::Record)
                .or_else(|| self.unions.get(name).map(NamedType::Union))
                .or_else(|| self.handles.get(name).map(NamedType::Handle))
                .or_else(|| {
                    self.type_aliases
                        .get(name)
                        .map(|(info, _)| NamedType::Alias(info))
                });
            if let Some(named) = exact {
                return if named.info().visible_from(module) {
                    Choice::Found(named, 0)
                } else {
                    Choice::Hidden(named)
                };
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
            |named| self.origin(&named.info().module),
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
            Choice::Missing => Err(Diagnostic::new(
                "E1004",
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
                Err(Diagnostic::new(
                    "E1004",
                    format!(
                        "ambiguous {kind} '{name}'; qualify it as {}",
                        visible
                            .iter()
                            .map(|named| named.info().name.as_str())
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
        let builtin = !name.contains('.');
        let exact = builtin.then(|| self.classes.get(name)).flatten();
        let exact = exact.or_else(|| self.classes.get(&format!("{module}.{name}")));
        let exact = exact.or_else(|| {
            self.searchable_path(module, name)
                .then(|| self.classes.get(name))
                .flatten()
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
            |class| self.origin(class.rsplit_once('.').map_or("", |(module, _)| module)),
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
            Choice::Ambiguous(classes, _) => Err(Diagnostic::new(
                "E1004",
                format!(
                    "ambiguous type class '{name}'; qualify it as {}",
                    classes.join(" or ")
                ),
                span,
            )),
            Choice::Hidden(_) | Choice::Missing => Ok(None),
        }
    }

    /// Resolves a class name to its key in `Classes::names`, or `None` when
    /// no class has the name.
    fn class(&self, module: &str, name: &str, span: Span) -> Result<Option<&str>, Diagnostic> {
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
            _ => Err(Diagnostic::new(
                "E1004",
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
            |case| self.origin(&case.info.module),
            |case| case.info.visible_from(module),
        ) {
            Choice::Found(case, _) => Ok(Some(case)),
            Choice::Hidden(case) => Err(case.info.private_error("union case", span)),
            Choice::Missing => Ok(None),
            Choice::Ambiguous(visible, _) => Err(Diagnostic::new(
                "E1004",
                format!(
                    "ambiguous union case '{name}'; qualify it as {}",
                    visible
                        .iter()
                        .map(|case| case.info.name.as_str())
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
                Diagnostic::new(
                    "E1002",
                    format!("union '{}' has no case '{name}'", info.name),
                    span,
                )
            })?;
        case.info.require_visible("union case", requester, span)?;
        Ok(Some(case))
    }

    /// Resolves a case path: `Case`, `Module.Case`, the requester's own
    /// `Union.Case`, or `Module.Union.Case`. `None` means the path names no
    /// case, so a caller can try functions, fields, or recognizers.
    fn case_path(
        &self,
        requester: &str,
        path: &str,
        span: Span,
    ) -> Result<Option<&CaseInfo>, Diagnostic> {
        let Some((prefix, name)) = path.rsplit_once('.') else {
            return self.case(requester, path, span);
        };
        let qualified = if self.searchable(requester, prefix) {
            let case = self.cases.get(path);
            if let Some(case) = case {
                case.info.require_visible("union case", requester, span)?;
            }
            case
        } else {
            None
        };
        let local = if let Some((module, union)) = prefix.rsplit_once('.') {
            self.union_case(requester, module, union, name, span)
        } else if self.unions.contains_key(&format!("{requester}.{prefix}")) {
            self.union_case(requester, requester, prefix, name, span)
        } else {
            Ok(None)
        };
        match (qualified, local) {
            (Some(case), Ok(Some(local))) if case.info.name != local.info.name => {
                Err(Self::ambiguous_path(path, span))
            }
            (Some(case), _) => Ok(Some(case)),
            (None, local) => local,
        }
    }

    fn ambiguous_path(path: &str, span: Span) -> Diagnostic {
        Diagnostic::new(
            "E1004",
            format!(
                "ambiguous path '{path}'; use Module.Union.Case when a module and a local union share the prefix"
            ),
            span,
        )
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
                return Err(Diagnostic::new(
                    "E1005",
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
            Diagnostic::new(
                "E1005",
                format!(
                    "{} does not satisfy '#{name}': module '{module}' has no function '{name}'",
                    receiver.display(types)
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
        if name.contains('.') {
            if self.searchable_path(requester, name) {
                if let Some((info, case)) = self.active_patterns.get(name) {
                    info.require_visible("active pattern", requester, span)?;
                    return Ok(Some((info.id, *case)));
                }
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
            |(info, _)| self.origin(&info.module),
            |(info, _)| info.visible_from(requester),
        ) {
            Choice::Found((info, case), _) => Ok(Some((info.id, *case))),
            Choice::Ambiguous(candidates, _) => Err(Diagnostic::new(
                "E1004",
                format!(
                    "ambiguous active pattern '{name}'; qualify one of {}",
                    candidates
                        .iter()
                        .map(|(info, _)| info.name.as_str())
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
                    return Err(Diagnostic::new(
                        "E1022",
                        format!(
                            "private type '{}' leaks from public {kind} '{owner}'; make the {kind} private or expose a public type",
                            info.name
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
            if head.text != "Vec"
                && !head.text.starts_with('\'')
                && let TypeHead::Type(named) = names.type_head(module, head)?
            {
                let info = named.info();
                if info.visibility == Visibility::Private {
                    let (kind, owner) = owner;
                    return Err(Diagnostic::new(
                        "E1022",
                        format!(
                            "private type '{}' leaks from public {kind} '{owner}'; make the {kind} private or expose a public type",
                            info.name
                        ),
                        head.span,
                    ));
                }
            }
            args.iter()
                .try_for_each(|arg| validate_public_type(arg, module, owner, names))
        }
        TypeExprKind::Variable(_) => Ok(()),
        TypeExprKind::Regions(inner, _)
        | TypeExprKind::Reference(inner, _)
        | TypeExprKind::Array(inner)
        | TypeExprKind::List(inner)
        | TypeExprKind::Task(inner) => validate_public_type(inner, module, owner, names),
        TypeExprKind::Tuple(elements) => elements
            .iter()
            .try_for_each(|element| validate_public_type(element, module, owner, names)),
        TypeExprKind::Function(parameters, result) => {
            for parameter in parameters {
                validate_public_type(parameter, module, owner, names)?;
            }
            validate_public_type(result, module, owner, names)
        }
    }
}

pub fn check_modules(modules: &[ModuleInput<'_>]) -> Result<CheckedModule, Diagnostic> {
    check_modules_all(modules).map_err(crate::first_error)
}

pub fn check_modules_all(modules: &[ModuleInput<'_>]) -> Result<CheckedModule, DiagnosticSet> {
    check_modules_indexed_all(modules, None)
}

pub(crate) fn check_modules_indexed_all(
    modules: &[ModuleInput<'_>],
    semantic: Option<&mut semantic::SemanticIndex>,
) -> Result<CheckedModule, DiagnosticSet> {
    let mut diagnostics = Diagnostics::new(0);
    match check_modules_collect(
        modules,
        &mut diagnostics,
        warnings::WarningOptions::default(),
        semantic,
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
    semantic: Option<&mut semantic::SemanticIndex>,
) -> Result<CheckedModule, Diagnostic> {
    let indexing = semantic.is_some();
    let mut names = Names {
        warning_options,
        ..Names::default()
    };
    let mut sources: BTreeMap<&str, (usize, ModuleOrigin)> = BTreeMap::new();
    for (source, module) in modules.iter().enumerate() {
        if diagnostics.is_full() {
            break;
        }
        let name = module.name;
        let reserved = |at: usize| {
            Diagnostic::new(
                "E1011",
                format!(
                    "module name '{name}' is reserved for the standard library; rename the file"
                ),
                Span::default().in_source(at),
            )
        };
        if name.len() > 255 || name.split('.').count() > 16 {
            diagnostics.push(Diagnostic::new(
                "E1017",
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
            crate::lexer::lex(name).is_ok_and(|tokens| {
                matches!(
                    tokens.as_slice(),
                    [Token { kind: TokenKind::Ident(text), .. }, Token { kind: TokenKind::End, .. }]
                        if text == name && name != "_" && name != "Task"
                )
            })
        });
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
            diagnostics.push(Diagnostic::new(
                "E1011",
                format!(
                    "invalid or duplicate module name '{name}'; each .tz, .tt, or .tc filename must have a unique ASCII identifier stem, not '_', 'Task', or a keyword"
                ),
                Span::default().in_source(source),
            ));
            continue;
        }
        names.modules.insert(name.to_owned(), module.origin);
    }
    diagnostics.check()?;
    names.builders = computation::collect_all(modules, diagnostics);
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
    for module in modules {
        for constant in &module.program.constants {
            constants::validate(&constant.value)?;
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
                return Err(Diagnostic::new(
                    "E1008",
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
                diagnostics.push(Diagnostic::new(
                    "E1018",
                    "embedded standard-library sources cannot declare tests",
                    test.name_span,
                ));
                continue;
            }
            let index = tests.len();
            let function = function_declarations.len();
            test_functions.insert(function, index);
            tests.push(CheckedTest {
                module: module.name.into(),
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
        if crate::numeric::primitive(&record.name.text).is_some()
            || record.name.text == "_"
            || matches!(record.name.text.as_str(), "Task" | "Vec")
            || polymorph::BUILTIN_CLASSES.contains(&record.name.text.as_str())
            || names.records.insert(qualified.clone(), info).is_some()
        {
            diagnostics.push(duplicate(&record.name));
            continue;
        }
        names
            .record_aliases
            .entry(record.name.text.clone())
            .or_default()
            .push(qualified);
        names.record_arities.push(record.parameters.len());
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
            if crate::numeric::primitive(&handle.name.text).is_some()
                || matches!(handle.name.text.as_str(), "_" | "Task" | "Vec")
                || polymorph::BUILTIN_CLASSES.contains(&handle.name.text.as_str())
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
                || matches!(class.name.text.as_str(), "_" | "Task" | "Vec")
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
            if crate::numeric::primitive(&alias.name.text).is_some()
                || matches!(alias.name.text.as_str(), "_" | "Task" | "Vec")
                || polymorph::BUILTIN_CLASSES.contains(&alias.name.text.as_str())
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
                diagnostics.push(Diagnostic::new(
                    "E1024",
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
                return Err(Diagnostic::new(
                    "E1024",
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
                return Err(Diagnostic::new(
                    "E1024",
                    format!(
                        "type parameter '{} is not used by the alias target; remove it or use it in the target",
                        parameter.text
                    ),
                    parameter.span,
                ));
            }
            if alias.visibility == Visibility::Public {
                validate_public_type(
                    &alias.target,
                    &info.module,
                    ("type alias", &info.name),
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
            let parameters = declared_parameters("record", &record.name.text, &record.parameters)?;
            let mut used = BTreeSet::new();
            let mut field_names = BTreeSet::new();
            let mut fields = Vec::new();
            for field in &record.fields {
                if !field_names.insert(&field.name.text) || field.name.text == "_" {
                    return Err(duplicate(&field.name));
                }
                reject_field_constraints(&field.ty, module, &names, "record")?;
                let ty = resolve_type(&field.ty, module, &names)?;
                if record.visibility == Visibility::Public
                    && !(names.origin(module) == ModuleOrigin::Std
                        && crate::stdlib::opaque_record(&qualified))
                {
                    validate_public_type(&field.ty, module, ("record", &qualified), &names)?;
                }
                polymorph::bounded_type(&ty, field.ty.span)?;
                for variable in polymorph::variables(&ty) {
                    if !parameters.contains(&variable) {
                        return Err(Diagnostic::new(
                            "E1024",
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
                    return Err(Diagnostic::new(
                        "E1013",
                        "record fields are immutable; use an owned update instead of a mutable field",
                        field.name.span,
                    ));
                }
                fields.push((field.name.text.clone(), ty));
            }
            if let Some(parameter) = record
                .parameters
                .iter()
                .find(|parameter| !used.contains(&parameter.text))
            {
                return Err(Diagnostic::new(
                    "E1024",
                    format!(
                        "type parameter '{} is not used by any field; remove it or add a field that mentions it",
                        parameter.text
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
    for record in &records {
        for (_, ty) in &record.fields {
            if ty.contains_stored_mutable_reference(&types) {
                diagnostics.push(Diagnostic::new(
                    "E1013",
                    "record fields cannot store mutable references; use a shared borrow",
                    record.span,
                ));
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
    let mut signatures = Vec::new();
    for (id, (module, function)) in function_declarations.iter().enumerate() {
        if diagnostics.is_full() {
            break;
        }
        if test_functions.contains_key(&id) {
            continue;
        }
        let qualified = format!("{module}.{}", function.name.text);
        let info = NameInfo {
            id,
            name: qualified.clone(),
            module: module.clone(),
            visibility: function.visibility,
        };
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
                return Err(Diagnostic::new(
                    "E1018",
                    "the standard library cannot export functions; std modules add no C/WASM exports",
                    function.name.span,
                ));
            }
            if function.exported && function.name.text.starts_with("$active.") {
                return Err(Diagnostic::new(
                    "E1008",
                    "active recognizers cannot be exported; expose an ordinary wrapper",
                    function.name.span,
                ));
            }
            if function.exported && !export_names.insert(&function.name.text) {
                return Err(Diagnostic::new(
                    "E1001",
                    format!(
                        "duplicate export 'tz_{}'; C/WASM export names must be unique across modules",
                        function.name.text
                    ),
                    function.name.span,
                ));
            }
            let mut parameter_names = BTreeSet::new();
            let mut parameters = Vec::new();
            let kinds = classes.constraint_kinds(&function.constraints, module, &names)?;
            for parameter in &function.parameters {
                if parameter.name.text != "_" && !parameter_names.insert(&parameter.name.text) {
                    return Err(duplicate(&parameter.name));
                }
                let ty = resolve_type_with_kinds(&parameter.ty, module, &names, &kinds)?;
                validate_size(&ty, &types, parameter.ty.span)?;
                parameters.push(ty);
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
                return Err(Diagnostic::new(
                    "E1008",
                    "exports support scalar values, borrowed i64/f64/ubyte arrays and string/utf8string inputs, owned buffer results, and scalar-only records with distinct C field names; mutable borrows, owned buffer inputs, wide numbers, and other aggregates are not supported",
                    function.name.span,
                ));
            }
            let signature = Signature { parameters, result };
            polymorph::bounded_type(&signature.as_type(), function.name.span)?;
            let variables = polymorph::variables(&signature.as_type());
            if external_functions.contains_key(&id) {
                if !variables.is_empty() {
                    return Err(Diagnostic::new(
                        "E1015",
                        "extern signatures must be concrete; provide a scalar ABI type",
                        function.name.span,
                    ));
                }
                if signature
                    .parameters
                    .iter()
                    .any(|ty| matches!(ty, Type::Function(..)) && !crate::abi::callback(ty))
                {
                    return Err(Diagnostic::new(
                        "E1008",
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
                    return Err(Diagnostic::new(
                        "E1008",
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
                        return Err(Diagnostic::new(
                            "E1008",
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
                    function.result.span,
                    "constant types must be concrete; remove type variables",
                ));
            }
            let mut constraints = Vec::new();
            let mut members = Vec::new();
            for constraint in &function.constraints {
                let ty = classes.constraint_type(constraint, module, &names, &kinds)?;
                if polymorph::variables(&ty)
                    .iter()
                    .any(|variable| !variables.contains(variable))
                {
                    return Err(Diagnostic::new(
                        "E1015",
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
                    ConstraintName::Function(name) => members.push(polymorph::MemberConstraint {
                        receiver: ty,
                        name: name.text.clone(),
                        module: module.clone(),
                        signature: None,
                        span: name.span,
                    }),
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
            Ok(Scheme {
                signature,
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
                diagnostics.push(Diagnostic::new(
                    "E1006",
                    "an active recognizer needs an input parameter",
                    active.cases[0].span,
                ));
                signatures[id] = Scheme::poisoned();
            }
            let option_result = matches!(&signatures[id].signature.result, Type::Union(union_id, _) if names.unions.get("Option.Option").is_some_and(|info| info.id == *union_id));
            if !signatures[id].is_poisoned()
                && active.partial
                && signatures[id].signature.result != Type::Bool
                && !option_result
            {
                diagnostics.push(Diagnostic::new(
                    "E1003",
                    "a partial active recognizer must return bool or Option payload",
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
                    Type::Union(..) => Some(Diagnostic::new(
                        "E1020",
                        "active pattern and backing union must have the same case count",
                        active.cases[0].span,
                    )),
                    _ => Some(Diagnostic::new(
                        "E1003",
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
                } else if option_result {
                    ActiveCase::OptionPartial
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
    for (id, (module, function)) in function_declarations.iter_mut().enumerate() {
        if diagnostics.is_full() {
            break;
        }
        let scheme = &signatures[id];
        if scheme.is_poisoned() {
            continue;
        }
        let signature = scheme.signature.clone();
        let mut checker = Checker::new(module, &names, types, &signatures, &classes);
        checker.indexing = indexing;
        if let Some(active) = active_results.get(&id) {
            checker.use_active_result(&signature.result, active);
        }
        let checked = (|| {
            let region_sources = regions::contract(function, module, &names, types)?;
            checker.type_parameters = scheme.variables.clone();
            checker.kinds = classes.constraint_kinds(&function.constraints, module, &names)?;
            checker.members = scheme.members.clone();
            let mut parameters = Vec::new();
            for (parameter, ty) in function.parameters.iter().zip(&signature.parameters) {
                parameters.push(checker.bind(&parameter.name, ty.clone(), parameter.mutable));
            }
            computation::expand(&mut function.body, &names)?;
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
                checker.expression(&function.body, Some(&signature.result))?
            };
            Ok(CheckedFunction {
                module: module.clone(),
                region_sources,
                origin: FunctionOrigin {
                    provenance: function.name.provenance,
                    test: test_functions.get(&id).copied(),
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
            })
        })();
        diagnostics.extend(
            std::mem::take(&mut checker.recovered)
                .into_iter()
                .map(|error| classes.derived_error(id, error)),
        );
        match checked {
            Ok(function) if !checker.poisoned => {
                functions.push(function);
                ids.push(id);
                pending.push(checker);
            }
            Ok(function) => recovered_functions.push((id, function, checker)),
            Err(error) => diagnostics.push(classes.derived_error(id, error)),
        }
    }
    let mut entry = modules
        .iter()
        .any(|module| {
            module.origin == ModuleOrigin::User
                && module.name == "Main"
                && matches!(module.program.source_kind, None | Some(SourceKind::Code))
        })
        .then(|| names.functions.get("Main.main").map(|info| info.id))
        .flatten()
        .filter(|id| !names.constants.contains(id));
    for &ModuleInput {
        name: module,
        program,
        origin,
    } in modules
    {
        if diagnostics.is_full() {
            break;
        }
        let Some(expression) = &program.entry else {
            continue;
        };
        if origin != ModuleOrigin::User || module != "Main" {
            diagnostics.push(Diagnostic::new(
                "E2004",
                "top-level execution is only allowed in Main.tz; other modules contain record and function declarations",
                expression.span,
            ));
            continue;
        }
        if entry.is_some() {
            diagnostics.push(Diagnostic::new(
                "E2004",
                "Main.tz must use either top-level entry-point code or 'fn main', not both",
                expression.span,
            ));
            continue;
        }
        let mut checker = Checker::new(module, &names, types, &signatures, &classes);
        checker.indexing = indexing;
        let checked = (|| {
            let mut expression = expression.clone();
            computation::expand(&mut expression, &names)?;
            checker.recovering = true;
            let body = checker.expression(&expression, None)?;
            Ok(CheckedFunction {
                module: module.to_owned(),
                region_sources: None,
                origin: FunctionOrigin {
                    provenance: Provenance::Generated,
                    ..FunctionOrigin::source(ModuleOrigin::User)
                },
                name: "$entry".into(),
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
    for (function, mut checker) in functions.iter_mut().zip(pending) {
        name_uses.push(std::mem::take(&mut checker.name_uses));
        let checked = (|| {
            checker.finish(&mut function.body)?;
            let coverage = checker.check_coverage()?;
            if function.origin.module == ModuleOrigin::User {
                warnings.extend(coverage);
                warnings.extend(warnings::unused_locals(function));
                warnings.append(&mut checker.shadowing_warnings);
            }
            if function.name == "$entry" {
                function.signature.result = function.body.ty.clone();
            }
            function.constraints.extend(checker.constraints);
            function.members = checker.members;
            Ok(())
        })();
        if let Err(error) = checked {
            // Unfinished types must not reach the ownership recovery pass.
            function.body = TypedExpr::error(function.span);
            let id = names
                .functions
                .get(&function.qualified_name())
                .map_or(usize::MAX, |info| info.id);
            diagnostics.push(classes.derived_error(id, error));
        }
    }
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
    if let Some(index) = semantic {
        *index = semantic::collect(modules, &functions, &name_uses, &names, types);
    }
    constants::fold(&mut functions, &names.constants)?;
    let references =
        recursion::check(&functions, &function_declarations, &classes, &names, &types)?;
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
    let module = CheckedModule {
        records,
        unions,
        functions,
        entry,
        tests,
        warnings,
    };
    validate_callbacks(&module)?;
    let copy_constraints = crate::ownership::infer_copy_all(&module).map_err(|errors| {
        let first = errors[0].clone();
        diagnostics.extend(errors);
        first
    })?;
    let recursive_functions = function_declarations
        .iter()
        .enumerate()
        .filter_map(|(id, (_, declaration))| declaration.recursion.is_some().then_some(id))
        .collect();
    let module = polymorph::specialize(
        module,
        &classes,
        &names,
        copy_constraints,
        &recursive_functions,
    )?;
    let module = closures::lower(module)?;
    diagnostics.extend(crate::ownership::check_all(&module));
    if let Err(error) = crate::gpu::validate_calls(&module) {
        diagnostics.push(error);
    }
    diagnostics.check()?;
    Ok(module)
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
        Diagnostic::new(
            "E1008",
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
                        return Err(Diagnostic::new(
                            "E1008",
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
            pending.extend(expression.children());
        }
    }
    Ok(())
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
            region_sources: None,
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
        warnings: Vec::new(),
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
    let id = names.functions[&format!("{module}.{}", function.name.text)].id;
    let scheme = &signatures[id];
    if scheme.variables.iter().any(|name| name == "T") {
        return Err(Diagnostic::new(
            "E1020",
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
    computation::expand(&mut source, names)?;
    let mut body = checker.expression(&source, Some(&result))?;
    checker.finish(&mut body)?;
    let result = checker.inference.resolve(&result);
    validate_size(&result, &types, function.result.span)?;
    Ok(Some(result))
}

fn duplicate(name: &Ident) -> Diagnostic {
    Diagnostic::new(
        "E1001",
        format!("duplicate or reserved name '{}'", name.text),
        name.span,
    )
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
            return Err(Diagnostic::new(
                "E1024",
                format!("'_ cannot name a {kind} type parameter; use a variable such as 'a"),
                parameter.span,
            ));
        }
        if declared.contains(&parameter.text) {
            return Err(Diagnostic::new(
                "E1024",
                format!(
                    "duplicate type parameter '{} in {kind} '{name}'; give each parameter a distinct name",
                    parameter.text
                ),
                parameter.span,
            ));
        }
        declared.push(parameter.text.clone());
    }
    Ok(declared)
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
                return Err(Diagnostic::new(
                    "E1024",
                    format!(
                        "union type '{}' must start with an uppercase ASCII letter",
                        name.text
                    ),
                    name.span,
                ));
            }
            let qualified = format!("{module}.{}", name.text);
            if matches!(name.text.as_str(), "Task" | "Vec")
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
                    return Err(Diagnostic::new(
                        "E1024",
                        format!(
                            "union case '{}' must start with an uppercase ASCII letter; lowercase names in patterns bind variables",
                            name.text
                        ),
                        name.span,
                    ));
                }
                if union.cases[..case]
                    .iter()
                    .any(|other| other.name.text == name.text)
                {
                    return Err(Diagnostic::new(
                        "E1024",
                        format!(
                            "duplicate case '{}' in union '{}'; give each case a distinct name",
                            name.text, union.name.text
                        ),
                        name.span,
                    ));
                }
                let qualified = format!("{module}.{}", name.text);
                if matches!(name.text.as_str(), "Task" | "Vec")
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
                return Err(Diagnostic::new(
                    "E1024",
                    format!(
                        "type variable '{variable} is not declared by union '{name}'; add it in angle brackets after the union name, as in 'union {name}<'{variable}> = ...'"
                    ),
                    expression.span,
                ));
            }
            used.insert(variable);
        }
        if ty.contains_reference() {
            return Err(Diagnostic::new(
                "E1013",
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
        return Err(Diagnostic::new(
            "E1024",
            format!(
                "type parameter '{} is not used by any case payload; remove it or add a payload that mentions it",
                parameter.text
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
        size: None,
        recursive: Default::default(),
    })
}

fn resolve_type(expression: &TypeExpr, module: &str, names: &Names) -> Result<Type, Diagnostic> {
    resolve_type_with_kinds(expression, module, names, &BTreeMap::new())
}

fn resolve_type_with_kinds(
    expression: &TypeExpr,
    module: &str,
    names: &Names,
    kinds: &BTreeMap<String, usize>,
) -> Result<Type, Diagnostic> {
    let resolve = |ty: &TypeExpr| resolve_type_with_kinds(ty, module, names, kinds);
    Ok(match &expression.kind {
        TypeExprKind::Regions(inner, _) => resolve(inner)?,
        TypeExprKind::Apply(head, args) if head.text.starts_with('\'') => {
            let variable = &head.text[1..];
            if kinds.get(variable) != Some(&args.len()) || args.is_empty() {
                return Err(Diagnostic::new(
                    "E1015",
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
                return Err(Diagnostic::new(
                    "E1004",
                    "Vec expects exactly one element type",
                    expression.span,
                ));
            };
            Type::Vec(Box::new(resolve(element)?))
        }
        TypeExprKind::Named(name) => match crate::numeric::primitive(name) {
            Some(ty) => ty,
            None => {
                let named = names.named_type(module, name, expression.span)?;
                record_arity(names, named, name, 0, expression.span).map_err(|mut error| {
                    if !matches!(named, NamedType::Alias(_)) {
                        error.code = "E1015";
                    }
                    error
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
                return Err(Diagnostic::new(
                    "E1004",
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
                        return Err(Diagnostic::new(
                            "E1016",
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
                    let args = args.iter().map(resolve).collect::<Result<_, _>>()?;
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
                return Err(Diagnostic::new(
                    "E1015",
                    "a type constructor must be fully applied in a value type",
                    expression.span,
                ));
            }
            Type::Variable(name.clone())
        }
        TypeExprKind::Reference(ty, mutable) => Type::Reference(Box::new(resolve(ty)?), *mutable),
        TypeExprKind::Array(element) => Type::Array(Box::new(resolve(element)?)),
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
    let code = if matches!(named, NamedType::Alias(_)) {
        "E1024"
    } else {
        "E1004"
    };
    Err(Diagnostic::new(code, message, span))
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
            return Err(Diagnostic::new(
                "E1017",
                "type alias expansion exceeds the compiler limit; simplify the aliases",
                expression.span,
            ));
        }
        let kind = match &expression.kind {
            TypeExprKind::Apply(head, args)
                if head.text == "Vec" || head.text.starts_with('\'') =>
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
                TypeExprKind::Named(named.info().name.clone())
            }
            TypeExprKind::Apply(head, args) if crate::numeric::primitive(&head.text).is_none() => {
                let name = match self.names.type_head(module, head)? {
                    TypeHead::Type(NamedType::Alias(info)) => {
                        return self.alias(info, args, module, expression.span, depth);
                    }
                    TypeHead::Type(named) => named.info().name.clone(),
                    TypeHead::Class => self
                        .names
                        .class(module, &head.text, head.span)?
                        .expect("a classified class has a resolved name")
                        .to_owned(),
                };
                TypeExprKind::Apply(
                    Box::new(Ident {
                        text: name,
                        span: head.span,
                        provenance: head.provenance,
                    }),
                    args.iter()
                        .map(|arg| self.expand(arg, module, depth + 1))
                        .collect::<Result<_, _>>()?,
                )
            }
            TypeExprKind::Reference(inner, mutable) => {
                TypeExprKind::Reference(Box::new(self.expand(inner, module, depth + 1)?), *mutable)
            }
            TypeExprKind::Regions(inner, regions) => TypeExprKind::Regions(
                Box::new(self.expand(inner, module, depth + 1)?),
                regions.clone(),
            ),
            TypeExprKind::Array(inner) => {
                TypeExprKind::Array(Box::new(self.expand(inner, module, depth + 1)?))
            }
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
        let arguments = args
            .iter()
            .map(|arg| self.expand(arg, module, depth + 1))
            .collect::<Result<Vec<_>, _>>()?;
        if self.active.contains(&info.name) {
            return Err(Diagnostic::new(
                "E1024",
                format!(
                    "cyclic type alias '{}'; remove the recursive alias reference",
                    info.name
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

    fn substitute(
        &mut self,
        expression: &mut TypeExpr,
        substitutions: &BTreeMap<String, TypeExpr>,
        depth: usize,
    ) -> Result<(), Diagnostic> {
        self.nodes += 1;
        if depth > MAX_NESTING || self.nodes > 4096 {
            return Err(Diagnostic::new(
                "E1017",
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
        } else {
            substitutions
        };
        let children: Vec<&mut TypeExpr> = match &mut expression.kind {
            TypeExprKind::Apply(_, args) => args.iter_mut().collect(),
            TypeExprKind::Regions(inner, _)
            | TypeExprKind::Reference(inner, _)
            | TypeExprKind::Array(inner)
            | TypeExprKind::List(inner)
            | TypeExprKind::Task(inner) => vec![inner],
            TypeExprKind::Tuple(elements) => elements.iter_mut().collect(),
            TypeExprKind::Function(parameters, result) => parameters
                .iter_mut()
                .chain(std::iter::once(result.as_mut()))
                .collect(),
            TypeExprKind::Named(_) | TypeExprKind::Variable(_) => Vec::new(),
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
            if matches!(names.type_head(module, head), Ok(TypeHead::Class)) {
                let parts = if owner == "union" {
                    "union payloads"
                } else {
                    "record fields"
                };
                return Err(Diagnostic::new(
                    "E1016",
                    format!(
                        "{parts} cannot constrain type parameters; put the constraint on the functions that use the {owner}"
                    ),
                    head.span,
                ));
            }
            args.iter().collect()
        }
        TypeExprKind::Regions(inner, _)
        | TypeExprKind::Reference(inner, _)
        | TypeExprKind::Array(inner)
        | TypeExprKind::List(inner)
        | TypeExprKind::Task(inner) => vec![inner],
        TypeExprKind::Tuple(elements) => elements.iter().collect(),
        TypeExprKind::Function(parameters, result) => parameters
            .iter()
            .chain(std::iter::once(result.as_ref()))
            .collect(),
        TypeExprKind::Named(_) | TypeExprKind::Variable(_) => Vec::new(),
    };
    children
        .into_iter()
        .try_for_each(|child| reject_expanded_field_constraints(child, module, names, owner))
}

fn size_error(span: Span) -> Diagnostic {
    Diagnostic::new(
        "E1010",
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
            return Err(Diagnostic::new(
                "E1010",
                format!(
                    "recursive value layout for '{}'; recursive heap types are not supported in 0.1",
                    record.name
                ),
                span,
            ));
        }
        if depth > MAX_NESTING {
            return Err(Diagnostic::new(
                "E1010",
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
            return Err(Diagnostic::new(
                "E1010",
                format!(
                    "recursive union layout for '{}'; recursive heap types are not supported in 0.1",
                    union.name
                ),
                span,
            ));
        }
        if depth > MAX_NESTING {
            return Err(Diagnostic::new(
                "E1010",
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
            Type::Simd(_) => 16,
            Type::Vec(element) => {
                self.size(element, depth, span)?;
                32
            }
            Type::Reference(_, false) if ty.shared_array_element().is_some() => 16,
            Type::Record(..) => self.record(ty, depth, span)?,
            Type::Union(..) => self.union(ty, depth, span)?,
            Type::Array(element) | Type::List(element) => {
                self.size(element, depth, span)?;
                16
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
            | Type::Utf8String => 16,
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

impl Validation<'_> {
    fn check(&mut self, ty: &Type, span: Span) -> Result<usize, Diagnostic> {
        let size = match ty {
            Type::Record(id, args) => {
                // Declared fields were validated with the record; an instance
                // only needs its substituted fields checked, once per call.
                if self.instances.insert(ty.clone()) {
                    let types = self.layouts.types;
                    let record = &types.records[*id];
                    for ((name, _), field) in
                        record.fields.iter().zip(types.record_fields(*id, args))
                    {
                        polymorph::bounded_type(&field, span)?;
                        if field.contains_stored_mutable_reference(&types) {
                            return Err(Diagnostic::new(
                                "E1013",
                                format!(
                                    "{} would store a mutable reference in field '{name}'; use a shared borrow",
                                    ty.display(&types)
                                ),
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
                            return Err(Diagnostic::new(
                                "E1013",
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
            Type::Array(element) | Type::List(element) | Type::Vec(element) => {
                if element.contains_stored_mutable_reference(&self.layouts.types) {
                    return Err(Diagnostic::new(
                        "E1005",
                        "array and list elements cannot contain mutable references; collections are deeply immutable",
                        span,
                    ));
                }
                self.check(element, span)?;
                if matches!(ty, Type::Vec(_)) { 32 } else { 16 }
            }
            Type::Function(parameters, result) => {
                for parameter in parameters {
                    self.check(parameter, span)?;
                }
                self.check(result, span)?;
                32
            }
            Type::Task(result) => {
                if result.contains_stored_reference(&self.layouts.types) {
                    return Err(Diagnostic::new(
                        "E1013",
                        "task results must be owned values, not references",
                        span,
                    ));
                }
                self.check(result, span)?;
                32
            }
            Type::Reference(value, _) => {
                self.check(value, span)?;
                if ty.shared_array_element().is_some() {
                    16
                } else {
                    8
                }
            }
            Type::Integer(128, _)
            | Type::Binary(128)
            | Type::Decimal(128)
            | Type::String
            | Type::Utf8String => 16,
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
    Record(usize),
    Union(usize),
    Field(usize, usize),
    Case(usize, usize),
    /// A later OR-pattern alternative binding an earlier alternative's local.
    Local(usize),
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
    /// Builtin result types that wait for a concrete integer argument type.
    families: Vec<polymorph::Family>,
    /// Explicit matches and function guards, in the order the checker reaches
    /// them; `check_coverage` inspects them once inference has finished.
    coverage: Vec<exhaustiveness::MatchCoverage>,
    shadowing_warnings: Vec<Diagnostic>,
    /// Locals that alias borrowed storage: `for...in` elements, match
    /// subjects bound to such a place, and pattern variables that may view it.
    borrowed: BTreeSet<usize>,
    poisoned: bool,
    recovering: bool,
    recovered: Vec<Diagnostic>,
    indexing: bool,
    name_uses: Vec<(Span, NameTarget)>,
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
            families: Vec::new(),
            coverage: Vec::new(),
            shadowing_warnings: Vec::new(),
            borrowed: BTreeSet::new(),
            poisoned: false,
            recovering: false,
            recovered: Vec::new(),
            indexing: false,
            name_uses: Vec::new(),
        }
    }

    /// Records the last segment of a possibly dotted name.
    #[inline(never)]
    fn note_name(&mut self, name: &Ident, target: NameTarget) {
        if self.indexing && name.provenance == Provenance::User {
            let last = name.text.rsplit('.').next().unwrap_or(&name.text);
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
        if let Some(qualifier) = segments.next().filter(|segment| Some(*segment) == short) {
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
        if Some(qualifier.text.as_str()) == short {
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
            self.shadowing_warnings.push(Diagnostic::warning(
                "W1004",
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
                return self.lambda(parameters, body, expected, expression.span, false);
            }
            ExprKind::Task(body) => {
                return self.lambda(&[], body, expected, expression.span, true);
            }
            ExprKind::Call(..) => return self.call_expression(expression, expected, false),
            ExprKind::Block { bindings, result } => {
                self.scopes.push(BTreeMap::new());
                let mut checked = Vec::new();
                for binding in bindings {
                    let annotation = self.binding_annotation(binding)?;
                    let value = self.expression(&binding.value, annotation.as_ref())?;
                    let ty = if annotation.as_ref().is_some_and(Type::contains_error) {
                        Type::Error
                    } else {
                        value.ty.clone()
                    };
                    let local = self.bind(&binding.name, ty, binding.mutable);
                    checked.push((local, value));
                }
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
        let callee = self.expression(callee, None)?;
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
            self.same(&result, &hint, expression.span)?;
        }
        if self.recovering && expected.is_some_and(|ty| matches!(ty, Type::Error)) {
            self.unknown_parameters(&mut parameters);
        }
        let mut arguments: Vec<_> = if matches!(&callee.kind, TypedExprKind::Function(FunctionRef::Builtin(instance)) if matches!(instance.builtin, Builtin::ParallelMap | Builtin::ParallelMapRef | Builtin::ParallelReduce) && arguments.len() == instance.builtin.scheme().parameters.len())
        {
            self.parallel_arguments(arguments, &parameters)?
        } else {
            arguments
                .iter()
                .zip(&parameters)
                .map(|(argument, parameter)| self.argument(argument, parameter))
                .collect::<Result<_, _>>()?
        };
        self.solve_families(false)?;
        self.constant_borrows(&callee, &mut arguments, &result, expression.span)?;
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
            let right = self.expression(right, None)?;
            let signature = self.call_signature(&right.ty, 1, right.span);
            let (parameters, result) = self.recover_signature(signature, 1, right.span)?;
            if let Some(expected) = expected {
                self.same(&result, expected, expression.span)?;
            }
            (self.argument(left, &parameters[0])?, right)
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
        self.finish_expression(
            TypedExprKind::Binary(*operator, Box::new(left), Box::new(right)),
            result,
            expected,
            expression.span,
        )
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
            if let Some(expected) = expected {
                self.same(&ty, expected, expression.span)?;
            }
            return Ok(TypedExpr {
                kind,
                ty: self.inference.resolve(&ty),
                span: expression.span,
            });
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
                        return Err(Diagnostic::new(
                            "E1009",
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
                return Err(Diagnostic::new(
                    "E1005",
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
                (TypedExprKind::Unary(*operator, Box::new(operand)), ty)
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
            ExprKind::Slice { value, start, end } => {
                self.slice(value, start.as_deref(), end.as_deref(), expression.span)?
            }
            ExprKind::Break | ExprKind::Continue => {
                let breaking = matches!(expression.kind, ExprKind::Break);
                if self.normal_loop_depth == 0 {
                    let keyword = if breaking { "break" } else { "continue" };
                    return Err(Diagnostic::new(
                        "E1023",
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
                    Diagnostic::new(
                        "E1004",
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
                let length = self.expression(length, Some(&Type::I64))?;
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
                let message = if module == "Task" {
                    format!(
                        "Task has no function '{}'; use Task.run, Task.parallel, or Task.parallel_results",
                        field.text
                    )
                } else {
                    format!(
                        "module '{}' has no function or union case '{}'",
                        module, field.text
                    )
                };
                return Err(Diagnostic::new("E1002", message, field.span));
            }
            ExprKind::Field(value, field) => self.field_access(value, field)?,
            ExprKind::Index(value, index) => {
                let value = Self::autoderef(self.expression(value, None)?);
                if value.ty.contains_error() {
                    return Ok(TypedExpr::error(expression.span));
                }
                let ty = match &value.ty {
                    Type::Array(element) | Type::List(element) | Type::Vec(element) => {
                        (**element).clone()
                    }
                    Type::String => Type::Integer(16, false),
                    Type::Utf8String => Type::Integer(8, false),
                    _ => {
                        return Err(Diagnostic::new(
                            "E1005",
                            "indexing requires an array, list, string, or utf8string",
                            value.span,
                        ));
                    }
                };
                let index = self.expression(index, Some(&Type::I64))?;
                (TypedExprKind::Index(Box::new(value), Box::new(index)), ty)
            }
            ExprKind::Borrow(value, mutable, notation) => {
                let value = match notation {
                    Notation::Symbol => {
                        let hint = match expected {
                            Some(Type::Reference(ty, expected_mutable))
                                if mutable == expected_mutable =>
                            {
                                Some(ty.as_ref())
                            }
                            Some(Type::Error) => expected,
                            _ => None,
                        };
                        self.expression(value, hint)?
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
                }
                let ty = Type::Reference(Box::new(value.ty.clone()), *mutable);
                (TypedExprKind::Borrow(Box::new(value), *mutable), ty)
            }
            ExprKind::Dereference(value, notation) => {
                let value = self.expression(value, None)?;
                if value.ty.contains_error() {
                    return Ok(TypedExpr::error(expression.span));
                }
                let Type::Reference(ty, _) = &value.ty else {
                    return Err(Diagnostic::new(
                        "E1005",
                        match notation {
                            Notation::Symbol => "dereference requires a reference",
                            Notation::Keyword => "'deref' requires a reference",
                        },
                        value.span,
                    ));
                };
                let ty = (**ty).clone();
                (TypedExprKind::Dereference(Box::new(value)), ty)
            }
            ExprKind::Assign(place, value) => {
                let place = self.expression(place, None)?;
                if place.ty.contains_error() {
                    return Ok(TypedExpr::error(expression.span));
                }
                Self::require_mutable_reference(&place)?;
                if !matches!(
                    place.kind,
                    TypedExprKind::Local(_) | TypedExprKind::Dereference(_)
                ) {
                    return Err(Diagnostic::new(
                        "E1012",
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
            | ExprKind::Field(..)
            | ExprKind::Index(..)
            | ExprKind::Dereference(..)
                if !expected.contains_error() =>
            {
                self.place_argument(expression, expected)
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

    fn argument_hint(&self, actual: &Type, expected: &Type) -> Type {
        let actual = self.inference.resolve(actual);
        let expected = self.inference.resolve(expected);
        match (&actual, &expected) {
            (_, Type::Infer(_) | Type::Error) => expected,
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
        mut value: TypedExpr,
        expected: &Type,
    ) -> Result<TypedExpr, Diagnostic> {
        let expected = self.inference.resolve(expected);
        if matches!(value.ty, Type::Infer(_)) {
            self.same(&value.ty, &expected, value.span)?;
            value.ty = self.inference.resolve(&value.ty);
        }
        if !matches!(value.ty, Type::Infer(_) | Type::Error) {
            match &expected {
                Type::Reference(inner, mutable) => {
                    if value.ty != **inner {
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

    fn require_mutable_reference(place: &TypedExpr) -> Result<(), Diagnostic> {
        if matches!(&place.kind, TypedExprKind::Dereference(reference) if matches!(reference.ty, Type::Reference(_, false)))
        {
            Err(Diagnostic::new(
                "E1014",
                "cannot mutate or exclusively reborrow through a shared reference",
                place.span,
            ))
        } else {
            Ok(())
        }
    }

    fn autoderef(mut value: TypedExpr) -> TypedExpr {
        while let Type::Reference(ty, _) = &value.ty {
            let ty = (**ty).clone();
            let span = value.span;
            value = TypedExpr {
                kind: TypedExprKind::Dereference(Box::new(value)),
                ty,
                span,
            };
        }
        value
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
                    Diagnostic::new(
                        "E1007",
                        format!("record '{}' has no field '{}'", record.name, field.text),
                        field.span,
                    )
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
            return Err(Diagnostic::new(
                "E1007",
                format!("missing fields: {}", missing.join(", ")),
                span,
            ));
        }
        let ty = self.inference.resolve(&Type::Record(id, args));
        validate_size(&ty, &self.types, span)?;
        Ok((TypedExprKind::Record(values), ty))
    }

    fn slice(
        &mut self,
        source: &Expr,
        start: Option<&Expr>,
        end: Option<&Expr>,
        span: Span,
    ) -> Result<(TypedExprKind, Type), Diagnostic> {
        let mut source = self.expression(source, None)?;
        source.ty = self.inference.resolve(&source.ty);
        let source = Self::autoderef(source);
        if source.ty == Type::Error {
            return Ok((TypedExprKind::Error, Type::Error));
        }
        if !matches!(source.ty, Type::Array(_)) {
            return Err(Diagnostic::new(
                "E1005",
                "a slice requires an array; lists and strings do not support array slicing",
                span,
            ));
        }
        let start = start
            .map(|value| self.expression(value, Some(&Type::I64)).map(Box::new))
            .transpose()?;
        let end = end
            .map(|value| self.expression(value, Some(&Type::I64)).map(Box::new))
            .transpose()?;
        let ty = Type::Reference(Box::new(source.ty.clone()), false);
        Ok((
            TypedExprKind::Slice {
                value: Box::new(source),
                start,
                end,
            },
            ty,
        ))
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
            return Err(Diagnostic::new(
                "E1005",
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
                    Diagnostic::new(
                        "E1007",
                        format!("record '{}' has no field '{}'", record.name, field.text),
                        field.span,
                    )
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
                        Diagnostic::new(
                            "E1007",
                            format!("unknown field '{}'", field.text),
                            field.span,
                        )
                    })?;
                let ty = self.types.record_field(*id, args, index);
                let id = *id;
                self.note_name(field, NameTarget::Field(id, index));
                Ok((TypedExprKind::Field(Box::new(value), index), ty))
            }
            Type::Array(_) | Type::List(_) | Type::Vec(_) if field.text == "length" => {
                Ok((TypedExprKind::Length(Box::new(value)), Type::I64))
            }
            Type::String | Type::Utf8String if field.text == "length" => {
                Ok((TypedExprKind::StringLength(Box::new(value)), Type::I64))
            }
            _ => Err(Diagnostic::new(
                "E1007",
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
            return Err(Diagnostic::new(
                "E1022",
                format!(
                    "the representation of '{}' is opaque; use its module API",
                    record.name
                ),
                span,
            ));
        }
        Ok(())
    }

    /// Keyword `ref r` on a reference `r` builds the same tree as the symbol reborrow `&*r`.
    fn reborrow_operand(value: TypedExpr) -> TypedExpr {
        let Type::Reference(ty, _) = &value.ty else {
            return value;
        };
        let ty = (**ty).clone();
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
        if suffix.is_none() && expected.is_some_and(polymorph::is_unknown) {
            let ty = expected.unwrap().clone();
            self.require(
                if negative { "SignedInteger" } else { "Integer" },
                ty.clone(),
                span,
            )?;
            self.inference.default_numeric(&ty, Type::I64);
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
            .unwrap_or(Type::I64);
        let Type::Integer(bits, signed) = ty else {
            return Err(Diagnostic::new(
                "E1009",
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
            return Err(Diagnostic::new(
                "E1009",
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
        self.names.searchable(self.module, name)
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
        if let Some(id) = self.names.function(self.module, &name.text, name.span)? {
            return Ok(self.function(id));
        }
        if let Some(builtin) = Builtin::ALL
            .iter()
            .find(|builtin| builtin.name() == name.text)
        {
            return self.builtin(*builtin, name.span);
        }
        Err(Diagnostic::new(
            "E1002",
            format!("unknown function '{}'", name.text),
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
        if let Some(id) = self.names.function(
            self.module,
            &format!("{}.{}", self.module, name.text),
            name.span,
        )? {
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
        Err(Diagnostic::new(
            "E1002",
            format!("unknown value '{}'", name.text),
            name.span,
        ))
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
        if let Some(id) = self.names.function(self.module, &path, span)? {
            return Ok(Some(self.function(id)));
        }
        Builtin::ALL
            .iter()
            .find(|builtin| builtin.name() == path)
            .map(|builtin| self.builtin(*builtin, expression.span))
            .transpose()
    }

    /// Resolves `Module.Case`, the current module's `Union.Case`, and
    /// `Module.Union.Case`. `None` leaves a path to module functions, class
    /// methods, and field access; a module function precedes a module case.
    fn case_reference(&self, expression: &Expr) -> Result<Option<(usize, usize)>, Diagnostic> {
        let Some(path) = self.value_path(expression) else {
            return Ok(None);
        };
        let span = expression.span;
        if let Some((prefix, name)) = path.rsplit_once('.') {
            let function = self.names.searchable(self.module, prefix)
                && self
                    .names
                    .functions
                    .get(&path)
                    .is_some_and(|info| info.visible_from(self.module));
            if function {
                let local = self
                    .names
                    .unions
                    .contains_key(&format!("{}.{prefix}", self.module))
                    && prefix != self.module
                    && matches!(
                        self.names
                            .union_case(self.module, self.module, prefix, name, span),
                        Ok(Some(_))
                    );
                if local {
                    return Err(Names::ambiguous_path(&path, span));
                }
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
    use super::Builtin;
    use crate::analyze;

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
            ("fn f(a: [i64; 4]) -> i64 { 0 }", "E0002"),
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
