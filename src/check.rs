use std::collections::{BTreeMap, BTreeSet};

use crate::diagnostic::{Diagnostic, DiagnosticSet, Diagnostics, Span};
use crate::syntax::*;

#[path = "closures.rs"]
mod closures;
#[path = "computation.rs"]
mod computation;
#[path = "control.rs"]
mod control;
#[path = "exhaustiveness.rs"]
mod exhaustiveness;
#[path = "polymorph.rs"]
mod polymorph;
#[path = "recursion.rs"]
mod recursion;
use polymorph::{Classes, Constraint, Inference, Scheme};

pub const MAX_VALUE_BYTES: usize = 64 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Type {
    Error,
    Variable(String),
    Infer(usize),
    Integer(u16, bool),
    Binary(u16),
    Decimal(u16),
    Bool,
    Unit,
    String,
    /// A record declaration and its type arguments; non-generic records have
    /// none. The arguments are boxed so `Type` stays four words, which keeps
    /// every typed expression and the checker's recursive frames small.
    Record(usize, Box<[Type]>),
    /// A union declaration and its type arguments, boxed like `Record`.
    Union(usize, Box<[Type]>),
    Array(Box<Type>),
    List(Box<Type>),
    Tuple(Vec<Type>),
    Task(Box<Type>),
    Function(Vec<Type>, Box<Type>),
    Reference(Box<Type>, bool),
}

impl Type {
    pub const I64: Self = Self::Integer(64, true);
    pub const F64: Self = Self::Binary(64);

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
            Self::Integer(bits, signed) => format!("i{bits}{}", if *signed { "" } else { "u" }),
            Self::Binary(bits) => format!("f{bits}"),
            Self::Decimal(bits) => format!("d{bits}"),
            Self::Bool => "bool".into(),
            Self::Unit => "unit".into(),
            Self::String => "string".into(),
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
            Self::Array(ty) | Self::List(ty) | Self::Task(ty) | Self::Reference(ty, _) => {
                ty.contains_error()
            }
            Self::Function(parameters, result) => {
                parameters.iter().any(Self::contains_error) || result.contains_error()
            }
            Self::Tuple(elements) => elements.iter().any(Self::contains_error),
            Self::Record(_, args) | Self::Union(_, args) => args.iter().any(Self::contains_error),
            _ => false,
        }
    }

    pub fn is_scalar(&self) -> bool {
        self.is_numeric() || *self == Self::Bool
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

    pub fn is_copy(&self, types: &TypeContext<'_>) -> bool {
        match self {
            Self::String
            | Self::Task(_)
            | Self::Reference(_, true)
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
        match self {
            Self::String | Self::Function(..) | Self::Task(_) => true,
            Self::Record(id, args) => {
                !types.record_fields_all(*id, args, |ty| !ty.needs_drop(types))
            }
            Self::Union(id, args) => {
                !types.union_payloads_all(*id, args, |ty| !ty.needs_drop(types))
            }
            Self::Array(_) | Self::List(_) => true,
            Self::Tuple(elements) => elements.iter().any(|ty| ty.needs_drop(types)),
            _ => false,
        }
    }

    /// Record and union instances never store references: `validate_size`
    /// rejects type arguments that would put one into a field or payload.
    pub fn contains_reference(&self) -> bool {
        match self {
            Self::Reference(..) => true,
            Self::Array(element) | Self::List(element) => element.contains_reference(),
            Self::Tuple(elements) => elements.iter().any(Self::contains_reference),
            _ => false,
        }
    }

    fn contains_mutable_reference(&self) -> bool {
        match self {
            Self::Reference(_, true) => true,
            Self::Reference(value, false) | Self::Array(value) | Self::List(value) => {
                value.contains_mutable_reference()
            }
            Self::Tuple(elements) => elements.iter().any(Self::contains_mutable_reference),
            // Function signatures describe calls, not stored references; captures are checked separately.
            _ => false,
        }
    }

    pub(crate) fn carries_loans(&self, types: &TypeContext<'_>) -> bool {
        match self {
            Self::Reference(..) | Self::Function(..) => true,
            Self::Array(element) | Self::List(element) => element.carries_loans(types),
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

    pub(crate) fn can_capture(&self, types: &TypeContext<'_>) -> bool {
        match self {
            Self::Reference(_, true) | Self::Task(_) => false,
            Self::Array(element) | Self::List(element) => element.can_capture(types),
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
        match self {
            Self::Reference(..) => false,
            Self::Array(element) | Self::List(element) => element.can_send(types),
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
    ToFloat,
    ToInt,
    Assert,
    CloneString,
    TaskRun,
    TaskParallel,
    /// `unreachable : unit -> 'a` traps (GUIDE D-21).
    Unreachable,
    ToString,
    Display,
    Parse,
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
    /// A primitive, `bool`, `unit`, or `string` type; never a project id.
    Concrete(Type),
    /// A std-origin record or union, such as `Option.Option<'a>`.
    Std {
        module: &'static str,
        name: &'static str,
        args: Vec<BuiltinType>,
    },
    Array(Box<BuiltinType>),
    List(Box<BuiltinType>),
    Tuple(Vec<BuiltinType>),
    Task(Box<BuiltinType>),
    Reference(Box<BuiltinType>, bool),
    Function(Vec<BuiltinType>, Box<BuiltinType>),
    /// The unsigned integer type of the same width.
    UnsignedOf(Box<BuiltinType>),
    /// The integer type of twice the width and the same signedness;
    /// undefined for 128-bit integers.
    WidenOf(Box<BuiltinType>),
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
        Self::ToFloat,
        Self::ToInt,
        Self::Assert,
        Self::CloneString,
        Self::TaskRun,
        Self::TaskParallel,
        Self::Unreachable,
        Self::ToString,
        Self::Display,
        Self::Parse,
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
            Self::ToFloat => "to_float",
            Self::ToInt => "to_int",
            Self::Assert => "assert",
            Self::CloneString => "clone_string",
            Self::TaskRun => "Task.run",
            Self::TaskParallel => "Task.parallel",
            Self::Unreachable => "unreachable",
            Self::ToString => "to_string",
            Self::Display => "$builtin.display",
            Self::Parse => "$builtin.parse",
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
            Self::Sqrt | Self::Floor | Self::Ceil | Self::Abs => {
                (vec![Concrete(Type::F64)], Concrete(Type::F64), Vec::new())
            }
            Self::ToFloat => (vec![Concrete(Type::I64)], Concrete(Type::F64), Vec::new()),
            Self::ToInt => (vec![Concrete(Type::F64)], Concrete(Type::I64), Vec::new()),
            Self::Assert => (vec![Concrete(Type::Bool)], Concrete(Type::Unit), Vec::new()),
            Self::CloneString => (
                vec![Reference(Box::new(Concrete(Type::String)), false)],
                Concrete(Type::String),
                Vec::new(),
            ),
            Self::TaskRun => (vec![Task(Box::new(a()))], a(), Vec::new()),
            Self::TaskParallel => (
                vec![Array(Box::new(Task(Box::new(a()))))],
                Task(Box::new(Array(Box::new(a())))),
                Vec::new(),
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
            | Self::Task(ty)
            | Self::Reference(ty, _)
            | Self::UnsignedOf(ty)
            | Self::WidenOf(ty) => ty.variables(found),
            Self::Function(parameters, result) => {
                parameters.iter().for_each(|ty| ty.variables(found));
                result.variables(found);
            }
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
    fn as_type(&self) -> Type {
        Type::function(self.parameters.clone(), self.result.clone())
    }

    fn validate_borrows(&self, types: &TypeContext<'_>, span: Span) -> Result<(), Diagnostic> {
        if self.result.contains_reference()
            && self
                .parameters
                .iter()
                .filter(|ty| ty.carries_loans(types))
                .count()
                != 1
        {
            return Err(Diagnostic::new(
                "E1013",
                "a borrowed result requires exactly one borrowed input for lifetime elision",
                span,
            ));
        }
        Ok(())
    }
}

/// Whether a module comes from the user's project or the standard library.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModuleOrigin {
    User,
    Std,
}

/// Whether a function is written in source or generated by the compiler.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Provenance {
    User,
    Generated,
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
    /// Warnings in source traversal order; they never fail a check or build.
    pub warnings: Vec<Diagnostic>,
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
pub enum TypedExprKind {
    Error,
    Int(u128),
    Float(String),
    String(String),
    Bool(bool),
    Unit,
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
    Lambda {
        parameters: Vec<Local>,
        captures: Vec<Local>,
        body: Box<TypedExpr>,
    },
    Closure(usize, Vec<TypedExpr>),
    TaskRun(Box<TypedExpr>),
    TaskParallel(Box<TypedExpr>),
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
    Length(Box<TypedExpr>),
    StringLength(Box<TypedExpr>),
    Borrow(Box<TypedExpr>, bool),
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
            | Dereference(value)
            | Cast(value)
            | Field(value, _)
            | Length(value)
            | StringLength(value)
            | TaskRun(value)
            | TaskParallel(value)
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
            Array(values) | List(values) | Tuple(values) | Closure(_, values) => {
                values.iter().collect()
            }
            _ => Vec::new(),
        }
    }

    pub(crate) fn children_mut(&mut self) -> Vec<&mut Self> {
        use TypedExprKind::*;
        match &mut self.kind {
            Unary(_, value)
            | Borrow(value, _)
            | Dereference(value)
            | Cast(value)
            | Field(value, _)
            | Length(value)
            | StringLength(value)
            | TaskRun(value)
            | TaskParallel(value)
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
            Array(values) | List(values) | Tuple(values) | Closure(_, values) => {
                values.iter_mut().collect()
            }
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
    modules: BTreeMap<String, ModuleOrigin>,
    builders: BTreeMap<String, BTreeSet<String>>,
    records: BTreeMap<String, NameInfo>,
    record_aliases: BTreeMap<String, Vec<String>>,
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
    active_patterns: BTreeMap<String, (NameInfo, bool)>,
}

#[derive(Clone, Debug)]
struct CaseInfo {
    /// `id` is the union id, and `name` is the qualified `Module.Case`.
    info: NameInfo,
    case: usize,
    has_payload: bool,
}

/// A record or union type name.
#[derive(Clone, Copy)]
enum NamedType<'a> {
    Record(&'a NameInfo),
    Union(&'a NameInfo),
}

impl<'a> NamedType<'a> {
    fn info(self) -> &'a NameInfo {
        match self {
            Self::Record(info) | Self::Union(info) => info,
        }
    }

    fn kind(self) -> &'static str {
        match self {
            Self::Record(_) => "record",
            Self::Union(_) => "union",
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
            .split_once('.')
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
        if self.searchable_path(module, name) {
            let exact = self
                .records
                .get(name)
                .map(NamedType::Record)
                .or_else(|| self.unions.get(name).map(NamedType::Union));
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
                format!("unknown record or union type '{name}'"),
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
            |class| self.origin(class.split_once('.').map_or("", |(module, _)| module)),
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
        let segments: Vec<&str> = path.split('.').collect();
        match segments.as_slice() {
            [name] => self.case(requester, name, span),
            [prefix, name] => {
                let qualified = if self.searchable(requester, prefix) {
                    let case = self.cases.get(&format!("{prefix}.{name}"));
                    if let Some(case) = case {
                        case.info.require_visible("union case", requester, span)?;
                    }
                    case
                } else {
                    None
                };
                let local = if self.unions.contains_key(&format!("{requester}.{prefix}")) {
                    self.union_case(requester, requester, prefix, name, span)
                } else {
                    Ok(None)
                };
                match (qualified, local) {
                    (Some(case), Ok(Some(local))) if case.info.name != local.info.name => {
                        Err(Self::ambiguous_path(path, span))
                    }
                    // A missing case of a local union is reported only when
                    // the prefix does not name a module that has the case.
                    (Some(case), _) => Ok(Some(case)),
                    (None, local) => local,
                }
            }
            [module, union, name] if self.searchable(requester, module) => {
                self.union_case(requester, module, union, name, span)
            }
            _ => Ok(None),
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
        qualified: &str,
        span: Span,
    ) -> Result<Option<(usize, bool)>, Diagnostic> {
        if !self.searchable_path(requester, qualified) {
            return Ok(None);
        }
        let Some((info, partial)) = self.active_patterns.get(qualified) else {
            return Ok(None);
        };
        info.require_visible("active pattern", requester, span)?;
        Ok(Some((info.id, *partial)))
    }
}

fn display_function_name(module: &str, name: &str) -> String {
    match name
        .strip_prefix("$active.")
        .and_then(|name| name.split_once('.'))
    {
        Some((case, "partial")) => format!("{module}.(|{case}|_|)"),
        Some((case, _)) => format!("{module}.(|{case}|)"),
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
            if let TypeHead::Type(named) = names.type_head(module, head)? {
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
        TypeExprKind::Reference(inner, _)
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
    let mut diagnostics = Diagnostics::new(0);
    match check_modules_collect(modules, &mut diagnostics) {
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
) -> Result<CheckedModule, Diagnostic> {
    let mut names = Names::default();
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
        if module.origin == ModuleOrigin::User && crate::stdlib::is_reserved_module(name) {
            diagnostics.push(reserved(source));
            continue;
        }
        let valid = crate::lexer::lex(name).is_ok_and(|tokens| {
            matches!(
                tokens.as_slice(),
                [Token { kind: TokenKind::Ident(text), .. }, Token { kind: TokenKind::End, .. }]
                    if text == name && name != "_" && name != "Task"
            )
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
            || record.name.text == "Task"
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
                || names.classes.contains(&qualified)
                || polymorph::BUILTIN_CLASSES.contains(&class.name.text.as_str())
                || matches!(class.name.text.as_str(), "_" | "Task")
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
                if record.visibility == Visibility::Public {
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
                if field.mutable || ty.contains_reference() {
                    return Err(Diagnostic::new(
                        "E1013",
                        "record fields are immutable owned values; borrowed fields require lifetime parameters, which are not supported",
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
    for (module, function) in &function_declarations {
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
            for parameter in &function.parameters {
                if parameter.name.text != "_" && !parameter_names.insert(&parameter.name.text) {
                    return Err(duplicate(&parameter.name));
                }
                let ty = resolve_type(&parameter.ty, module, &names)?;
                validate_size(&ty, &types, parameter.ty.span)?;
                parameters.push(ty);
            }
            let result = resolve_type(&function.result, module, &names)?;
            validate_size(&result, &types, function.result.span)?;
            let public_type = Type::function(parameters.clone(), result.clone());
            let Type::Function(public_parameters, public_result) = &public_type else {
                unreachable!()
            };
            if function.exported
                && (public_parameters.iter().any(|ty| !ty.exportable())
                    || (!public_result.exportable() && **public_result != Type::Unit))
            {
                return Err(Diagnostic::new(
                    "E1008",
                    "exports support 8/16/32/64-bit integers, f32, f64, bool, and unit results; keep wide/software numbers, strings, references, aggregates, and function values inside Tsuzuri",
                    function.name.span,
                ));
            }
            let signature = Signature { parameters, result };
            signature.validate_borrows(&types, function.result.span)?;
            polymorph::bounded_type(&signature.as_type(), function.name.span)?;
            let variables = polymorph::variables(&signature.as_type());
            let mut constraints = Vec::new();
            let mut members = Vec::new();
            for constraint in &function.constraints {
                let ty = resolve_type(&constraint.ty, module, &names)?;
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
                    active.name.span,
                ));
                signatures[id] = Scheme::poisoned();
            }
            if !signatures[id].is_poisoned()
                && active.partial
                && signatures[id].signature.result != Type::Bool
            {
                diagnostics.push(Diagnostic::new(
                    "E1003",
                    "a partial active recognizer must return bool",
                    active.name.span,
                ));
                signatures[id] = Scheme::poisoned();
            }
            let qualified = format!("{module}.{}", active.name.text);
            let info = NameInfo {
                name: qualified.clone(),
                ..function
            };
            if names.cases.contains_key(&qualified)
                || names
                    .active_patterns
                    .insert(qualified, (info, active.partial))
                    .is_some()
            {
                diagnostics.push(duplicate(&active.name));
            }
        }
    }
    let mut functions = Vec::new();
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
        let checked = (|| {
            checker.type_parameters = scheme.variables.clone();
            checker.members = scheme.members.clone();
            let mut parameters = Vec::new();
            for (parameter, ty) in function.parameters.iter().zip(&signature.parameters) {
                parameters.push(checker.bind(&parameter.name, ty.clone(), parameter.mutable));
            }
            computation::expand(&mut function.body, &names)?;
            let body = checker.expression(&function.body, Some(&signature.result))?;
            Ok(CheckedFunction {
                module: module.clone(),
                origin: FunctionOrigin::source(names.origin(module)),
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
        if checker.poisoned {
            continue;
        }
        match checked {
            Ok(function) => {
                functions.push(function);
                pending.push(checker);
            }
            Err(error) => diagnostics.push(error),
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
        .flatten();
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
        let checked = (|| {
            let mut expression = expression.clone();
            computation::expand(&mut expression, &names)?;
            let body = checker.expression(&expression, None)?;
            Ok(CheckedFunction {
                module: module.to_owned(),
                origin: FunctionOrigin::source(ModuleOrigin::User),
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
        if checker.poisoned {
            continue;
        }
        match checked {
            Ok(function) => {
                entry = Some(functions.len());
                functions.push(function);
                pending.push(checker);
            }
            Err(error) => diagnostics.push(error),
        }
    }
    if diagnostics.check().is_ok() {
        polymorph::solve_members(&mut functions, &mut pending)?;
    }
    for (function, mut checker) in functions.iter_mut().zip(pending) {
        let checked = (|| {
            checker.finish(&mut function.body)?;
            warnings.extend(checker.check_coverage()?);
            if function.name == "$entry" {
                function.signature.result = function.body.ty.clone();
            }
            function.constraints.extend(checker.constraints);
            function.members = checker.members;
            Ok(())
        })();
        if let Err(error) = checked {
            diagnostics.push(error);
        }
    }
    diagnostics.check()?;
    recursion::check(&functions, &function_declarations, &classes, &names, &types)?;
    // Warnings follow source order rather than the order bodies are checked.
    warnings.sort_by_key(|warning: &Diagnostic| (warning.span.source, warning.span.start));
    let module = CheckedModule {
        records,
        unions,
        functions,
        entry,
        warnings,
    };
    let copy_constraints = crate::ownership::infer_copy_all(&module).map_err(|errors| {
        let first = errors[0].clone();
        diagnostics.extend(errors);
        first
    })?;
    let module = polymorph::specialize(module, &classes, &names, copy_constraints)?;
    let module = closures::lower(module)?;
    for function in &module.functions {
        if let Err(error) = function
            .signature
            .validate_borrows(&module.types(), function.span)
        {
            diagnostics.push(error);
        }
        if diagnostics.is_full() {
            break;
        }
    }
    diagnostics.check()?;
    diagnostics.extend(crate::ownership::check_all(&module));
    diagnostics.check()?;
    Ok(module)
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
            if name.text == "Task"
                || polymorph::BUILTIN_CLASSES.contains(&name.text.as_str())
                || names.records.contains_key(&qualified)
                || names.unions.contains_key(&qualified)
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
                if name.text == "Task"
                    || names.records.contains_key(&qualified)
                    || names.unions.contains_key(&qualified)
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
    })
}

fn resolve_type(expression: &TypeExpr, module: &str, names: &Names) -> Result<Type, Diagnostic> {
    Ok(match &expression.kind {
        TypeExprKind::Named(name) => match crate::numeric::primitive(name) {
            Some(ty) => ty,
            None => {
                let named = names.named_type(module, name, expression.span)?;
                record_arity(names, named, name, 0, expression.span)?;
                match named {
                    NamedType::Record(info) => Type::Record(info.id, Box::default()),
                    NamedType::Union(info) => Type::Union(info.id, Box::default()),
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
                    resolve_type(ty, module, names)?
                }
                TypeHead::Type(named) => {
                    record_arity(names, named, &head.text, args.len(), expression.span)?;
                    let args = args
                        .iter()
                        .map(|ty| resolve_type(ty, module, names))
                        .collect::<Result<_, _>>()?;
                    match named {
                        NamedType::Record(info) => Type::Record(info.id, args),
                        NamedType::Union(info) => Type::Union(info.id, args),
                    }
                }
            }
        }
        TypeExprKind::Variable(name) => Type::Variable(name.clone()),
        TypeExprKind::Reference(ty, mutable) => {
            Type::Reference(Box::new(resolve_type(ty, module, names)?), *mutable)
        }
        TypeExprKind::Array(element) => {
            Type::Array(Box::new(resolve_type(element, module, names)?))
        }
        TypeExprKind::List(element) => Type::List(Box::new(resolve_type(element, module, names)?)),
        TypeExprKind::Tuple(elements) => Type::Tuple(
            elements
                .iter()
                .map(|ty| resolve_type(ty, module, names))
                .collect::<Result<_, _>>()?,
        ),
        TypeExprKind::Task(result) => Type::Task(Box::new(resolve_type(result, module, names)?)),
        TypeExprKind::Function(parameters, result) => Type::function(
            parameters
                .iter()
                .map(|parameter| resolve_type(parameter, module, names))
                .collect::<Result<_, _>>()?,
            resolve_type(result, module, names)?,
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
    Err(Diagnostic::new("E1004", message, span))
}

/// Record fields and union payloads describe stored values, so they cannot
/// carry the inline class constraints that function signatures accept.
fn reject_field_constraints(
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
        TypeExprKind::Reference(inner, _)
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
        .try_for_each(|child| reject_field_constraints(child, module, names, owner))
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
            Type::Integer(128, _) | Type::Binary(128) | Type::Decimal(128) | Type::String => 16,
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
                if !args.is_empty() && self.instances.insert(ty.clone()) {
                    let types = self.layouts.types;
                    let record = &types.records[*id];
                    for ((name, _), field) in
                        record.fields.iter().zip(types.record_fields(*id, args))
                    {
                        polymorph::bounded_type(&field, span)?;
                        if field.contains_reference() {
                            return Err(Diagnostic::new(
                                "E1013",
                                format!(
                                    "record fields are immutable owned values; {} would store a reference in field '{name}', and borrowed fields require lifetime parameters, which are not supported",
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
                if !args.is_empty() && self.instances.insert(ty.clone()) {
                    let types = self.layouts.types;
                    let union = &types.unions[*id];
                    for ((name, _), payload) in
                        union.cases.iter().zip(types.union_payloads(*id, args))
                    {
                        let Some(payload) = payload else {
                            continue;
                        };
                        polymorph::bounded_type(&payload, span)?;
                        if payload.contains_reference() {
                            return Err(Diagnostic::new(
                                "E1013",
                                format!(
                                    "union payloads are owned values; {} would store a reference in case '{name}', and borrowed payloads require lifetime parameters, which are not supported",
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
            Type::Array(element) | Type::List(element) => {
                if element.contains_mutable_reference() {
                    return Err(Diagnostic::new(
                        "E1005",
                        "array and list elements cannot contain mutable references; collections are deeply immutable",
                        span,
                    ));
                }
                self.check(element, span)?;
                16
            }
            Type::Function(parameters, result) => {
                for parameter in parameters {
                    self.check(parameter, span)?;
                }
                self.check(result, span)?;
                32
            }
            Type::Task(result) => {
                if result.contains_reference() {
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
                8
            }
            Type::Integer(128, _) | Type::Binary(128) | Type::Decimal(128) | Type::String => 16,
            _ => 8,
        };
        if size > MAX_VALUE_BYTES {
            Err(size_error(span))
        } else {
            Ok(size)
        }
    }
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
    scopes: Vec<BTreeMap<String, Local>>,
    next_local: usize,
    /// Operands of keyword `ref` whose type was still unknown; `finish` rejects any that became references.
    undecided_borrows: Vec<(Type, Span)>,
    /// Builtin result types that wait for a concrete integer argument type.
    families: Vec<polymorph::Family>,
    /// Explicit matches and function guards, in the order the checker reaches
    /// them; `check_coverage` inspects them once inference has finished.
    coverage: Vec<exhaustiveness::MatchCoverage>,
    /// Locals that alias borrowed storage: `for...in` elements, match
    /// subjects bound to such a place, and pattern variables that may view it.
    borrowed: BTreeSet<usize>,
    poisoned: bool,
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
            scopes: vec![BTreeMap::new()],
            next_local: 0,
            undecided_borrows: Vec::new(),
            families: Vec::new(),
            coverage: Vec::new(),
            borrowed: BTreeSet::new(),
            poisoned: false,
        }
    }

    fn bind(&mut self, name: &Ident, ty: Type, mutable: bool) -> Local {
        let local = Local {
            id: self.next_local,
            ty,
            name: name.text.clone(),
            mutable,
            span: name.span,
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
            return Ok(TypedExpr::error(expression.span));
        }
        // Continuations must not retain the large value-checking frame at every recursive step.
        match expression.kind {
            ExprKind::While { .. } | ExprKind::For { .. } | ExprKind::Match { .. } => {
                self.control_expression(expression, expected)
            }
            ExprKind::Lambda(..)
            | ExprKind::Task(_)
            | ExprKind::Call(..)
            | ExprKind::Block { .. } => self.composed_expression(expression, expected),
            _ => self.value_expression(expression, expected),
        }
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
            return Ok(TypedExpr::error(span));
        }
        if let Some(expected) = expected {
            self.same(&value.ty, expected, span)?;
        }
        value.ty = self.inference.resolve(&value.ty);
        Ok(value)
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
            ExprKind::Call(callee, arguments) => {
                if let ExprKind::Call(inner, first) = &callee.kind {
                    if !first.is_empty() && !arguments.is_empty() {
                        let mut combined = first.clone();
                        combined.extend(arguments.iter().cloned());
                        let combined = Expr {
                            kind: ExprKind::Call(inner.clone(), combined),
                            span: expression.span,
                            depth: expression.depth,
                        };
                        return self.expression(&combined, expected);
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
                let (parameters, result) = self
                    .call_signature(&callee.ty, arguments.len(), expression.span)
                    .map_err(|error| Self::operator_spacing_hint(error, arguments))?;
                if let Some(expected) = expected {
                    self.same(&result, expected, expression.span)?;
                }
                let arguments: Vec<_> = arguments
                    .iter()
                    .zip(&parameters)
                    .map(|(argument, parameter)| self.expression(argument, Some(parameter)))
                    .collect::<Result<_, _>>()?;
                self.solve_families(false)?;
                (Self::call_kind(callee, arguments), result)
            }
            ExprKind::Block { bindings, result } => {
                self.scopes.push(BTreeMap::new());
                let mut checked = Vec::new();
                for binding in bindings {
                    let annotation = binding
                        .annotation
                        .as_ref()
                        .map(|ty| self.annotation(ty))
                        .transpose()?;
                    if let Some(ty) = &annotation {
                        validate_size(ty, &self.types, binding.name.span)?;
                    }
                    let value = self.expression(&binding.value, annotation.as_ref())?;
                    let local = self.bind(&binding.name, value.ty.clone(), binding.mutable);
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
        let namespace = match &expression.kind {
            ExprKind::Field(value, field) if case.is_none() => match &value.kind {
                ExprKind::Name(module) if self.local(&module.text).is_none() => {
                    let qualified = format!("{}.{}", module.text, field.text);
                    if let Some(id) = self.names.function(self.module, &qualified, field.span)? {
                        Some(self.function(id))
                    } else if let Some(builtin) = Builtin::ALL
                        .iter()
                        .find(|builtin| builtin.name() == qualified)
                    {
                        Some(self.builtin(*builtin, expression.span)?)
                    } else {
                        None
                    }
                }
                _ => None,
            },
            _ => None,
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
            ExprKind::String(text) => (TypedExprKind::String(text.clone()), Type::String),
            ExprKind::Bool(value) => (TypedExprKind::Bool(*value), Type::Bool),
            ExprKind::Unit => (TypedExprKind::Unit, Type::Unit),
            ExprKind::Tuple(values) => {
                let types = match expected {
                    Some(Type::Tuple(types)) if types.len() == values.len() => Some(types),
                    _ => None,
                };
                let values = values
                    .iter()
                    .enumerate()
                    .map(|(index, value)| self.expression(value, types.map(|types| &types[index])))
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
            ExprKind::QualifiedFunction(name) => {
                let id = self
                    .names
                    .function(self.module, &name.text, name.span)?
                    .ok_or_else(|| {
                        Diagnostic::new(
                            "E1002",
                            format!("unknown function '{}'", name.text),
                            name.span,
                        )
                    })?;
                self.function(id)
            }
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
            ExprKind::Binary(operator, left, right) => {
                let (left, right) = if *operator == BinaryOp::Pipe {
                    let right = self.expression(right, None)?;
                    let (parameters, result) = self.call_signature(&right.ty, 1, right.span)?;
                    if let Some(expected) = expected {
                        self.same(&result, expected, expression.span)?;
                    }
                    (self.expression(left, Some(&parameters[0]))?, right)
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
                        (self.expression(left, Some(&right.ty))?, right)
                    } else {
                        let left = self.expression(left, hint)?;
                        let right = self.expression(right, Some(&left.ty))?;
                        (left, right)
                    }
                };
                let result = if *operator == BinaryOp::Pipe {
                    let (parameters, result) = self.call_signature(&right.ty, 1, right.span)?;
                    self.same(&left.ty, &parameters[0], left.span)?;
                    self.solve_families(false)?;
                    result
                } else {
                    self.binary_type(*operator, &left, &right, expression.span)?
                };
                (
                    TypedExprKind::Binary(*operator, Box::new(left), Box::new(right)),
                    result,
                )
            }
            ExprKind::If {
                condition,
                then_branch,
                else_branch,
            } => {
                let condition = self.expression(condition, Some(&Type::Bool))?;
                let then_branch = self.expression(then_branch, expected)?;
                let else_branch = self.expression(else_branch, Some(&then_branch.ty))?;
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
            ExprKind::Array(values) | ExprKind::List(values) => {
                let list = matches!(expression.kind, ExprKind::List(_));
                let mut element_type = match (list, expected) {
                    (false, Some(Type::Array(element))) | (true, Some(Type::List(element))) => {
                        Some((**element).clone())
                    }
                    _ => None,
                };
                let mut checked = Vec::new();
                for value in values {
                    let value = self.expression(value, element_type.as_ref())?;
                    element_type = Some(value.ty.clone());
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
                self.case_value(union_id, case_id)
            }
            ExprKind::Field(value, field)
                if matches!(&value.kind, ExprKind::Name(name)
                    if self.local(&name.text).is_none() && self.is_namespace(&name.text)) =>
            {
                let ExprKind::Name(module) = &value.kind else {
                    unreachable!()
                };
                let message = if module.text == "Task" {
                    format!(
                        "Task has no function '{}'; use Task.run or Task.parallel",
                        field.text
                    )
                } else {
                    format!(
                        "module '{}' has no function or union case '{}'",
                        module.text, field.text
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
                    Type::Array(element) | Type::List(element) => (**element).clone(),
                    Type::String => Type::Integer(8, false),
                    _ => {
                        return Err(Diagnostic::new(
                            "E1005",
                            "indexing requires an array, list, or string",
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
            ExprKind::Lambda(..)
            | ExprKind::Task(_)
            | ExprKind::Call(..)
            | ExprKind::Block { .. } => unreachable!("composed expressions use their own checker"),
        };
        self.finish_expression(kind, ty, expected, expression.span)
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
        let types = self.types;
        let record = &types.records[id];
        // Type arguments come from the expected type when it names this
        // record; otherwise the field values determine them.
        let args = match expected.map(|ty| self.inference.resolve(ty)) {
            Some(Type::Record(expected_id, args)) if expected_id == id => args,
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
                Ok((TypedExprKind::Field(Box::new(value), index), ty))
            }
            Type::Array(_) | Type::List(_) if field.text == "length" => {
                Ok((TypedExprKind::Length(Box::new(value)), Type::I64))
            }
            Type::String if field.text == "length" => {
                Ok((TypedExprKind::StringLength(Box::new(value)), Type::I64))
            }
            _ => Err(Diagnostic::new(
                "E1007",
                "field access requires a record, or '.length' on an array, list, or string",
                field.span,
            )),
        }
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
        self.names.searchable(self.module, name)
            || crate::stdlib::is_reserved_module(name)
            || Builtin::ALL.iter().any(|builtin| {
                builtin
                    .name()
                    .split_once('.')
                    .is_some_and(|(prefix, _)| prefix == name)
            })
    }

    fn name(&mut self, name: &Ident) -> Result<(TypedExprKind, Type), Diagnostic> {
        if let Some(local) = self.local(&name.text) {
            return Ok((TypedExprKind::Local(local.id), local.ty.clone()));
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
        let args: Box<[Type]> = (0..arity).map(|_| self.inference.fresh()).collect();
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
        match &expression.kind {
            ExprKind::Name(name) => self.local(&name.text).is_none().then(|| name.text.clone()),
            ExprKind::Field(value, field) => {
                let mut path = self.value_path(value)?;
                path.push('.');
                path.push_str(&field.text);
                Some(path)
            }
            _ => None,
        }
    }

    /// Resolves `Module.Case`, the current module's `Union.Case`, and
    /// `Module.Union.Case`. `None` leaves a path to module functions, class
    /// methods, and field access; a module function precedes a module case.
    fn case_reference(&self, expression: &Expr) -> Result<Option<(usize, usize)>, Diagnostic> {
        let Some(path) = self.value_path(expression) else {
            return Ok(None);
        };
        let span = expression.span;
        if let Some((prefix, name)) = path.split_once('.') {
            let function = !name.contains('.')
                && self.names.searchable(self.module, prefix)
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
    fn call_kind(callee: TypedExpr, mut arguments: Vec<TypedExpr>) -> TypedExprKind {
        match callee.kind {
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
            // Arity 0 would make a builtin a value rather than a function.
            assert!(!scheme.parameters.is_empty(), "{}", builtin.name());
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
            ("fn f() -> bool { [1] == [1] }", "E1005"),
            ("record R { x: i64 } fn f() -> R { R {} }", "E1007"),
            ("record R { x: i64 } fn f() -> R { R { z: 1 } }", "E1007"),
            (
                "record R { x: i64 } fn f() -> i64 { R { x: 1 }.z }",
                "E1007",
            ),
            (
                "record R { x: i64 } export fn f(r: R) -> i64 { r.x }",
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
