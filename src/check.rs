use std::collections::{BTreeMap, BTreeSet};

use crate::diagnostic::{Diagnostic, Span};
use crate::syntax::*;

#[path = "closures.rs"]
mod closures;
#[path = "computation.rs"]
mod computation;
#[path = "polymorph.rs"]
mod polymorph;
use polymorph::{Classes, Constraint, Inference, Scheme};

pub const MAX_VALUE_BYTES: usize = 64 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Type {
    Variable(String),
    Infer(usize),
    Integer(u16, bool),
    Binary(u16),
    Decimal(u16),
    Bool,
    Unit,
    String,
    Record(usize),
    Array(Box<Type>),
    List(Box<Type>),
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
        let Self::Function(parameters, result) = self else {
            unreachable!()
        };
        if count == parameters.len() {
            (**result).clone()
        } else {
            Self::function(parameters[count..].to_vec(), (**result).clone())
        }
    }

    pub fn display(&self, records: &[CheckedRecord]) -> String {
        match self {
            Self::Variable(name) => format!("'{name}"),
            Self::Infer(_) => "an undetermined type".into(),
            Self::Integer(bits, signed) => format!("i{bits}{}", if *signed { "" } else { "u" }),
            Self::Binary(bits) => format!("f{bits}"),
            Self::Decimal(bits) => format!("d{bits}"),
            Self::Bool => "bool".into(),
            Self::Unit => "unit".into(),
            Self::String => "string".into(),
            Self::Reference(ty, mutable) => {
                let inner = ty.display(records);
                let inner = if matches!(**ty, Self::Function(..)) {
                    format!("({inner})")
                } else {
                    inner
                };
                format!("&{}{inner}", if *mutable { "mut " } else { "" })
            }
            Self::Record(id) => records[*id].name.clone(),
            Self::Array(element) => {
                format!("[{}]", element.display(records))
            }
            Self::List(element) => format!("[|{}|]", element.display(records)),
            Self::Task(result) => {
                let inner = result.display(records);
                if matches!(**result, Self::Function(..)) {
                    format!("Task ({inner})")
                } else {
                    format!("Task {inner}")
                }
            }
            Self::Function(parameters, result) if parameters.is_empty() => {
                format!("fn() -> {}", result.display(records))
            }
            Self::Function(parameters, result) => {
                let mut parts: Vec<_> = parameters
                    .iter()
                    .map(|parameter| {
                        let text = parameter.display(records);
                        if matches!(parameter, Self::Function(..)) {
                            format!("({text})")
                        } else {
                            text
                        }
                    })
                    .collect();
                parts.push(result.display(records));
                parts.join(" -> ")
            }
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

    pub fn is_copy(&self, records: &[CheckedRecord]) -> bool {
        match self {
            Self::String
            | Self::Task(_)
            | Self::Reference(_, true)
            | Self::Variable(_)
            | Self::Infer(_) => false,
            Self::Record(id) => records[*id]
                .fields
                .iter()
                .all(|(_, ty)| ty.is_copy(records)),
            Self::Array(element) | Self::List(element) => element.is_copy(records),
            _ => true,
        }
    }

    pub fn needs_drop(&self, records: &[CheckedRecord]) -> bool {
        match self {
            Self::String | Self::Function(..) | Self::Task(_) => true,
            Self::Record(id) => records[*id]
                .fields
                .iter()
                .any(|(_, ty)| ty.needs_drop(records)),
            Self::Array(_) | Self::List(_) => true,
            _ => false,
        }
    }

    pub fn contains_reference(&self) -> bool {
        match self {
            Self::Reference(..) => true,
            Self::Array(element) | Self::List(element) => element.contains_reference(),
            _ => false,
        }
    }

    fn contains_mutable_reference(&self) -> bool {
        match self {
            Self::Reference(_, true) => true,
            Self::Reference(value, false) | Self::Array(value) | Self::List(value) => {
                value.contains_mutable_reference()
            }
            // Function signatures describe calls, not stored references; captures are checked separately.
            _ => false,
        }
    }

    pub(crate) fn carries_loans(&self, records: &[CheckedRecord]) -> bool {
        match self {
            Self::Reference(..) | Self::Function(..) => true,
            Self::Array(element) | Self::List(element) => element.carries_loans(records),
            Self::Record(id) => records[*id]
                .fields
                .iter()
                .any(|(_, ty)| ty.carries_loans(records)),
            _ => false,
        }
    }

    pub(crate) fn can_capture(&self, records: &[CheckedRecord]) -> bool {
        match self {
            Self::Reference(_, true) | Self::Task(_) => false,
            Self::Array(element) | Self::List(element) => element.can_capture(records),
            Self::Record(id) => records[*id]
                .fields
                .iter()
                .all(|(_, ty)| ty.can_capture(records)),
            _ => true,
        }
    }

    pub fn exportable(&self) -> bool {
        matches!(
            self,
            Self::Integer(8 | 16 | 32 | 64, _) | Self::Binary(32 | 64) | Self::Bool
        )
    }

    pub(crate) fn can_send(&self, records: &[CheckedRecord]) -> bool {
        match self {
            Self::Reference(..) => false,
            Self::Array(element) | Self::List(element) => element.can_send(records),
            Self::Record(id) => records[*id]
                .fields
                .iter()
                .all(|(_, ty)| ty.can_send(records)),
            // Function environments are checked by ownership, not by their call signatures.
            _ => true,
        }
    }
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
}

impl Builtin {
    pub const ALL: [Self; 10] = [
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
    ];

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
        }
    }

    pub fn signature(self) -> Signature {
        let (parameter, result) = match self {
            Self::ToFloat => (Type::I64, Type::F64),
            Self::ToInt => (Type::F64, Type::I64),
            Self::Assert => (Type::Bool, Type::Unit),
            Self::CloneString => (Type::Reference(Box::new(Type::String), false), Type::String),
            Self::TaskRun => {
                let ty = Type::Variable("a".into());
                (Type::Task(Box::new(ty.clone())), ty)
            }
            Self::TaskParallel => {
                let ty = Type::Variable("a".into());
                (
                    Type::Array(Box::new(Type::Task(Box::new(ty.clone())))),
                    Type::Task(Box::new(Type::Array(Box::new(ty)))),
                )
            }
            _ => (Type::F64, Type::F64),
        };
        Signature {
            parameters: vec![parameter],
            result,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FunctionRef {
    User(usize),
    Builtin(Builtin),
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

    fn validate_borrows(&self, records: &[CheckedRecord], span: Span) -> Result<(), Diagnostic> {
        if self.result.contains_reference()
            && self
                .parameters
                .iter()
                .filter(|ty| ty.carries_loans(records))
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

#[derive(Debug)]
pub struct CheckedModule {
    pub records: Vec<CheckedRecord>,
    pub functions: Vec<CheckedFunction>,
    pub entry: Option<usize>,
}

#[derive(Clone, Debug)]
pub struct CheckedRecord {
    pub name: String,
    pub fields: Vec<(String, Type)>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct CheckedFunction {
    pub module: String,
    pub name: String,
    pub exported: bool,
    pub parameters: Vec<Local>,
    pub signature: Signature,
    pub body: TypedExpr,
    pub span: Span,
    type_parameters: Vec<String>,
    constraints: Vec<Constraint>,
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
pub enum TypedExprKind {
    Int(u128),
    Float(String),
    String(String),
    Bool(bool),
    Unit,
    Local(usize),
    Function(FunctionRef),
    GenericFunction(usize, Vec<Type>),
    Method(usize, usize, Type),
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
    Block {
        bindings: Vec<(Local, TypedExpr)>,
        result: Box<TypedExpr>,
    },
    Record(Vec<(usize, TypedExpr)>),
    Array(Vec<TypedExpr>),
    List(Vec<TypedExpr>),
    NewArray(Box<TypedExpr>, Box<TypedExpr>),
    NewList(Box<TypedExpr>, Box<TypedExpr>),
    Field(Box<TypedExpr>, usize),
    Index(Box<TypedExpr>, Box<TypedExpr>),
    Length(Box<TypedExpr>),
    StringLength(Box<TypedExpr>),
    Borrow(Box<TypedExpr>, bool),
    Dereference(Box<TypedExpr>),
    Assign(Box<TypedExpr>, Box<TypedExpr>),
    Cast(Box<TypedExpr>),
}

pub fn check(program: &Program) -> Result<CheckedModule, Diagnostic> {
    check_modules(&[("Main", program)])
}

#[derive(Default)]
struct Names {
    modules: BTreeSet<String>,
    builders: BTreeMap<String, BTreeSet<String>>,
    records: BTreeMap<String, usize>,
    record_aliases: BTreeMap<String, Vec<String>>,
    functions: BTreeMap<String, usize>,
}

impl Names {
    fn record(&self, module: &str, name: &str, span: Span) -> Result<usize, Diagnostic> {
        if let Some(id) = self.records.get(&format!("{module}.{name}")) {
            return Ok(*id);
        }
        if let Some(id) = self.records.get(name) {
            return Ok(*id);
        }
        if let Some(aliases) = self.record_aliases.get(name) {
            if aliases.len() == 1 {
                return Ok(self.records[&aliases[0]]);
            }
            return Err(Diagnostic::new(
                "E1004",
                format!(
                    "ambiguous record type '{name}'; qualify it as {}",
                    aliases.join(" or ")
                ),
                span,
            ));
        }
        Err(Diagnostic::new(
            "E1004",
            format!("unknown record type '{name}'"),
            span,
        ))
    }
}

pub fn check_modules(modules: &[(&str, &Program)]) -> Result<CheckedModule, Diagnostic> {
    let mut names = Names::default();
    for (source, (name, _)) in modules.iter().copied().enumerate() {
        let valid = crate::lexer::lex(name).is_ok_and(|tokens| {
            matches!(
                tokens.as_slice(),
                [Token { kind: TokenKind::Ident(text), .. }, Token { kind: TokenKind::End, .. }]
                    if text == name && name != "_" && name != "Task"
            )
        });
        if !valid || !names.modules.insert(name.to_owned()) {
            return Err(Diagnostic::new(
                "E1011",
                format!(
                    "invalid or duplicate module name '{name}'; each .tz, .tt, or .tc filename must have a unique ASCII identifier stem, not '_', 'Task', or a keyword"
                ),
                Span::default().in_source(source),
            ));
        }
    }
    names.builders = computation::collect(modules)?;
    let record_declarations: Vec<_> = modules
        .iter()
        .flat_map(|(name, program)| program.records.iter().map(|record| (*name, record)))
        .collect();
    let mut function_declarations: Vec<_> = modules
        .iter()
        .flat_map(|(name, program)| {
            program
                .functions
                .iter()
                .map(|function| ((*name).to_owned(), function.clone()))
        })
        .collect();
    for (id, (module, record)) in record_declarations.iter().enumerate() {
        let qualified = format!("{module}.{}", record.name.text);
        if crate::numeric::primitive(&record.name.text).is_some()
            || record.name.text == "_"
            || record.name.text == "Task"
            || names.records.insert(qualified.clone(), id).is_some()
        {
            return Err(duplicate(&record.name));
        }
        names
            .record_aliases
            .entry(record.name.text.clone())
            .or_default()
            .push(qualified);
    }
    let mut records = Vec::new();
    for (module, record) in &record_declarations {
        let mut field_names = BTreeSet::new();
        let mut fields = Vec::new();
        for field in &record.fields {
            if !field_names.insert(&field.name.text) || field.name.text == "_" {
                return Err(duplicate(&field.name));
            }
            let ty = resolve_type(&field.ty, module, &names)?;
            polymorph::require_concrete(&ty, field.ty.span)?;
            if field.mutable || ty.contains_reference() {
                return Err(Diagnostic::new(
                    "E1013",
                    "record fields are immutable owned values; borrowed fields require lifetime parameters, which are not supported",
                    field.name.span,
                ));
            }
            fields.push((field.name.text.clone(), ty));
        }
        records.push(CheckedRecord {
            name: format!("{module}.{}", record.name.text),
            fields,
            span: record.name.span,
        });
    }
    let mut sizes = vec![None; records.len()];
    for id in 0..records.len() {
        record_size(id, &records, &mut sizes, &mut BTreeSet::new(), 0)?;
    }
    let record_sizes: Vec<usize> = sizes.into_iter().map(Option::unwrap).collect();
    for record in &records {
        for (_, ty) in &record.fields {
            validate_size(ty, &record_sizes, record.span)?;
        }
    }
    let mut export_names = BTreeSet::new();
    let mut classes = Classes::collect(modules, &names, &record_sizes)?;
    classes.instances(modules, &names, &records, &mut function_declarations)?;
    let mut signatures = Vec::new();
    for (id, (module, function)) in function_declarations.iter().enumerate() {
        if function.name.text == "_"
            || Builtin::ALL
                .iter()
                .any(|builtin| builtin.name() == function.name.text)
            || names
                .functions
                .insert(format!("{module}.{}", function.name.text), id)
                .is_some()
        {
            return Err(duplicate(&function.name));
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
            validate_size(&ty, &record_sizes, parameter.ty.span)?;
            parameters.push(ty);
        }
        let result = resolve_type(&function.result, module, &names)?;
        validate_size(&result, &record_sizes, function.result.span)?;
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
        signature.validate_borrows(&records, function.result.span)?;
        polymorph::bounded_type(&signature.as_type(), function.name.span)?;
        let variables = polymorph::variables(&signature.as_type());
        let mut constraints: Vec<_> = function
            .constraints
            .iter()
            .map(|constraint| {
                let class = classes.resolve(module, &constraint.class)?;
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
                Ok(Constraint {
                    class,
                    ty,
                    span: constraint.class.span,
                })
            })
            .collect::<Result<_, Diagnostic>>()?;
        for ty in function
            .parameters
            .iter()
            .map(|parameter| &parameter.ty)
            .chain(std::iter::once(&function.result))
        {
            constraints.extend(classes.inline_constraints(ty, module, &names)?);
        }
        signatures.push(Scheme {
            signature,
            variables,
            constraints,
        });
    }
    let mut functions = Vec::new();
    for (id, (module, function)) in function_declarations.iter_mut().enumerate() {
        let scheme = &signatures[id];
        let signature = scheme.signature.clone();
        let mut checker = Checker::new(
            module,
            &names,
            &records,
            &record_sizes,
            &signatures,
            &classes,
        );
        checker.type_parameters = scheme.variables.clone();
        let mut parameters = Vec::new();
        for (parameter, ty) in function.parameters.iter().zip(&signature.parameters) {
            parameters.push(checker.bind(&parameter.name, ty.clone(), parameter.mutable));
        }
        computation::expand(&mut function.body, &names)?;
        let mut body = checker.expression(&function.body, Some(&signature.result))?;
        checker.finish(&mut body)?;
        let mut constraints = scheme.constraints.clone();
        constraints.extend(checker.constraints);
        functions.push(CheckedFunction {
            module: module.clone(),
            name: function.name.text.clone(),
            exported: function.exported,
            parameters,
            signature,
            body,
            span: function.name.span,
            type_parameters: scheme.variables.clone(),
            constraints,
            capture_count: 0,
            is_task: false,
        });
    }
    let mut entry = modules
        .iter()
        .any(|(name, program)| {
            *name == "Main" && matches!(program.source_kind, None | Some(SourceKind::Code))
        })
        .then(|| names.functions.get("Main.main").copied())
        .flatten();
    for (module, program) in modules {
        let Some(expression) = &program.entry else {
            continue;
        };
        if *module != "Main" {
            return Err(Diagnostic::new(
                "E2004",
                "top-level execution is only allowed in Main.tz; other modules contain record and function declarations",
                expression.span,
            ));
        }
        if entry.is_some() {
            return Err(Diagnostic::new(
                "E2004",
                "Main.tz must use either top-level entry-point code or 'fn main', not both",
                expression.span,
            ));
        }
        let mut checker = Checker::new(
            module,
            &names,
            &records,
            &record_sizes,
            &signatures,
            &classes,
        );
        let mut expression = expression.clone();
        computation::expand(&mut expression, &names)?;
        let mut body = checker.expression(&expression, None)?;
        checker.finish(&mut body)?;
        entry = Some(functions.len());
        functions.push(CheckedFunction {
            module: (*module).to_owned(),
            name: "$entry".into(),
            exported: false,
            parameters: Vec::new(),
            signature: Signature {
                parameters: Vec::new(),
                result: body.ty.clone(),
            },
            body,
            span: expression.span,
            type_parameters: Vec::new(),
            constraints: checker.constraints,
            capture_count: 0,
            is_task: false,
        });
    }
    let module = CheckedModule {
        records,
        functions,
        entry,
    };
    let module = polymorph::specialize(module, &classes, &record_sizes)?;
    let module = closures::lower(module)?;
    for function in &module.functions {
        function
            .signature
            .validate_borrows(&module.records, function.span)?;
    }
    crate::ownership::check(&module)?;
    Ok(module)
}

fn duplicate(name: &Ident) -> Diagnostic {
    Diagnostic::new(
        "E1001",
        format!("duplicate or reserved name '{}'", name.text),
        name.span,
    )
}

fn resolve_type(expression: &TypeExpr, module: &str, names: &Names) -> Result<Type, Diagnostic> {
    Ok(match &expression.kind {
        TypeExprKind::Named(name) => match crate::numeric::primitive(name) {
            Some(ty) => ty,
            None => Type::Record(names.record(module, name, expression.span)?),
        },
        TypeExprKind::Variable(name) => Type::Variable(name.clone()),
        TypeExprKind::Constrained(_, ty) => resolve_type(ty, module, names)?,
        TypeExprKind::Reference(ty, mutable) => {
            Type::Reference(Box::new(resolve_type(ty, module, names)?), *mutable)
        }
        TypeExprKind::Array(element) => {
            Type::Array(Box::new(resolve_type(element, module, names)?))
        }
        TypeExprKind::List(element) => Type::List(Box::new(resolve_type(element, module, names)?)),
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

fn size_error(span: Span) -> Diagnostic {
    Diagnostic::new(
        "E1010",
        format!("value layout exceeds {MAX_VALUE_BYTES} bytes; use smaller value types"),
        span,
    )
}

fn record_size(
    id: usize,
    records: &[CheckedRecord],
    sizes: &mut [Option<usize>],
    visiting: &mut BTreeSet<usize>,
    depth: usize,
) -> Result<usize, Diagnostic> {
    if let Some(size) = sizes[id] {
        return Ok(size);
    }
    let record = &records[id];
    if !visiting.insert(id) {
        return Err(Diagnostic::new(
            "E1010",
            format!(
                "recursive value layout for '{}'; recursive heap types are not supported in 0.1",
                record.name
            ),
            record.span,
        ));
    }
    if depth > MAX_NESTING {
        return Err(Diagnostic::new(
            "E1010",
            format!("record nesting exceeds {MAX_NESTING}"),
            record.span,
        ));
    }
    let mut size: usize = 0;
    for (_, ty) in &record.fields {
        size = size.saturating_add(
            layout_size(ty, records, sizes, visiting, depth + 1)?.next_multiple_of(16),
        );
        if size > MAX_VALUE_BYTES {
            return Err(size_error(record.span));
        }
    }
    visiting.remove(&id);
    sizes[id] = Some(size);
    Ok(size)
}

fn layout_size(
    ty: &Type,
    records: &[CheckedRecord],
    sizes: &mut [Option<usize>],
    visiting: &mut BTreeSet<usize>,
    depth: usize,
) -> Result<usize, Diagnostic> {
    Ok(match ty {
        Type::Record(id) => record_size(*id, records, sizes, visiting, depth)?,
        Type::Array(element) | Type::List(element) => {
            layout_size(element, records, sizes, visiting, depth)?;
            16
        }
        Type::Integer(128, _) | Type::Binary(128) | Type::Decimal(128) | Type::String => 16,
        Type::Function(..) | Type::Task(_) => 32,
        // Small values conservatively occupy at least one pointer-sized slot.
        _ => 8,
    })
}

fn validate_size(ty: &Type, sizes: &[usize], span: Span) -> Result<usize, Diagnostic> {
    let size = match ty {
        Type::Record(id) => sizes[*id],
        Type::Array(element) | Type::List(element) => {
            if element.contains_mutable_reference() {
                return Err(Diagnostic::new(
                    "E1005",
                    "array and list elements cannot contain mutable references; collections are deeply immutable",
                    span,
                ));
            }
            validate_size(element, sizes, span)?;
            16
        }
        Type::Function(parameters, result) => {
            for parameter in parameters {
                validate_size(parameter, sizes, span)?;
            }
            validate_size(result, sizes, span)?;
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
            validate_size(result, sizes, span)?;
            32
        }
        Type::Reference(value, _) => {
            validate_size(value, sizes, span)?;
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

struct Checker<'a> {
    module: &'a str,
    names: &'a Names,
    records: &'a [CheckedRecord],
    record_sizes: &'a [usize],
    signatures: &'a [Scheme],
    classes: &'a Classes,
    inference: Inference,
    constraints: Vec<Constraint>,
    type_parameters: Vec<String>,
    scopes: Vec<BTreeMap<String, Local>>,
    next_local: usize,
}

impl<'a> Checker<'a> {
    fn new(
        module: &'a str,
        names: &'a Names,
        records: &'a [CheckedRecord],
        record_sizes: &'a [usize],
        signatures: &'a [Scheme],
        classes: &'a Classes,
    ) -> Self {
        Self {
            module,
            names,
            records,
            record_sizes,
            signatures,
            classes,
            inference: Inference::default(),
            constraints: Vec::new(),
            type_parameters: Vec::new(),
            scopes: vec![BTreeMap::new()],
            next_local: 0,
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
        self.inference.unify(actual, expected, self.records, span)
    }

    fn expression(
        &mut self,
        expression: &Expr,
        expected: Option<&Type>,
    ) -> Result<TypedExpr, Diagnostic> {
        // Continuations must not retain the large value-checking frame at every recursive step.
        match expression.kind {
            ExprKind::Lambda(..)
            | ExprKind::Task(_)
            | ExprKind::Call(..)
            | ExprKind::Block { .. } => self.composed_expression(expression, expected),
            _ => self.value_expression(expression, expected),
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
                let (parameters, result) =
                    self.call_signature(&callee.ty, arguments.len(), expression.span)?;
                if let Some(expected) = expected {
                    self.same(&result, expected, expression.span)?;
                }
                let arguments: Vec<_> = arguments
                    .iter()
                    .zip(&parameters)
                    .map(|(argument, parameter)| self.expression(argument, Some(parameter)))
                    .collect::<Result<_, _>>()?;
                (TypedExprKind::Call(Box::new(callee), arguments), result)
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
                        validate_size(ty, self.record_sizes, binding.name.span)?;
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
        if let Some(expected) = expected {
            self.same(&ty, expected, expression.span)?;
        }
        Ok(TypedExpr {
            kind,
            ty: self.inference.resolve(&ty),
            span: expression.span,
        })
    }

    fn value_expression(
        &mut self,
        expression: &Expr,
        expected: Option<&Type>,
    ) -> Result<TypedExpr, Diagnostic> {
        let expected = expected.map(|ty| self.inference.resolve(ty));
        let expected = expected.as_ref();
        let module_function = matches!(&expression.kind, ExprKind::Field(value, field)
            if matches!(&value.kind, ExprKind::Name(name)
                if self.local(&name.text).is_none()
                    && self.names.functions.contains_key(&format!("{}.{}", name.text, field.text))));
        let method = if module_function {
            None
        } else {
            self.classes
                .method(expression, self.module, |name| self.local(name).is_some())?
        };
        if let Some((class, method)) = method {
            let (kind, ty) = self.method(class, method, expression.span);
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
            ExprKind::Name(name) => self.name(name)?,
            ExprKind::QualifiedFunction(name) => {
                let id = self.names.functions.get(&name.text).ok_or_else(|| {
                    Diagnostic::new(
                        "E1002",
                        format!("unknown function '{}'", name.text),
                        name.span,
                    )
                })?;
                self.function(*id)
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
                let id = self.names.record(self.module, &name.text, name.span)?;
                let record = &self.records[id];
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
                    values.push((
                        index,
                        self.expression(value, Some(&record.fields[index].1))?,
                    ));
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
                        expression.span,
                    ));
                }
                (TypedExprKind::Record(values), Type::Record(id))
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
                validate_size(&ty, self.record_sizes, expression.span)?;
                (kind, ty)
            }
            ExprKind::NewArray(annotation, length, initializer)
            | ExprKind::NewList(annotation, length, initializer) => {
                let ty = self.annotation(annotation)?;
                validate_size(&ty, self.record_sizes, expression.span)?;
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
            ExprKind::Field(value, field)
                if matches!(&value.kind, ExprKind::Name(name)
                    if name.text == "Task" && self.local(&name.text).is_none()) =>
            {
                let builtin = Builtin::ALL
                    .iter()
                    .find(|builtin| builtin.name() == format!("Task.{}", field.text))
                    .ok_or_else(|| {
                        Diagnostic::new(
                            "E1002",
                            format!(
                                "Task has no function '{}'; use Task.run or Task.parallel",
                                field.text
                            ),
                            field.span,
                        )
                    })?;
                self.builtin(*builtin)
            }
            ExprKind::Field(value, field)
                if matches!(&value.kind, ExprKind::Name(name)
                    if self.local(&name.text).is_none() && self.names.modules.contains(&name.text)) =>
            {
                let ExprKind::Name(module) = &value.kind else {
                    unreachable!()
                };
                let qualified = format!("{}.{}", module.text, field.text);
                let id = self.names.functions.get(&qualified).ok_or_else(|| {
                    Diagnostic::new(
                        "E1002",
                        format!("module '{}' has no function '{}'", module.text, field.text),
                        field.span,
                    )
                })?;
                self.function(*id)
            }
            ExprKind::Field(value, field) => {
                let value = Self::autoderef(self.expression(value, None)?);
                match &value.ty {
                    Type::Record(id) => {
                        let index = self.records[*id]
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
                        let ty = self.records[*id].fields[index].1.clone();
                        (TypedExprKind::Field(Box::new(value), index), ty)
                    }
                    Type::Array(_) | Type::List(_) if field.text == "length" => {
                        (TypedExprKind::Length(Box::new(value)), Type::I64)
                    }
                    Type::String if field.text == "length" => {
                        (TypedExprKind::StringLength(Box::new(value)), Type::I64)
                    }
                    _ => {
                        return Err(Diagnostic::new(
                            "E1007",
                            "field access requires a record, or '.length' on an array, list, or string",
                            field.span,
                        ));
                    }
                }
            }
            ExprKind::Index(value, index) => {
                let value = Self::autoderef(self.expression(value, None)?);
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
            ExprKind::Borrow(value, mutable) => {
                let hint = match expected {
                    Some(Type::Reference(ty, expected_mutable)) if mutable == expected_mutable => {
                        Some(ty.as_ref())
                    }
                    _ => None,
                };
                let value = self.expression(value, hint)?;
                if *mutable {
                    Self::require_mutable_reference(&value)?;
                }
                let ty = Type::Reference(Box::new(value.ty.clone()), *mutable);
                (TypedExprKind::Borrow(Box::new(value), *mutable), ty)
            }
            ExprKind::Dereference(value) => {
                let value = self.expression(value, None)?;
                let Type::Reference(ty, _) = &value.ty else {
                    return Err(Diagnostic::new(
                        "E1005",
                        "dereference requires a reference",
                        value.span,
                    ));
                };
                let ty = (**ty).clone();
                (TypedExprKind::Dereference(Box::new(value)), ty)
            }
            ExprKind::Assign(place, value) => {
                let place = self.expression(place, None)?;
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
        if let Some(expected) = expected {
            self.same(&ty, expected, expression.span)?;
        }
        Ok(TypedExpr {
            kind,
            ty: self.inference.resolve(&ty),
            span: expression.span,
        })
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
        Self::integer_literal(value, suffix, expected, negative, span, self.records)
    }

    fn integer_literal(
        value: u128,
        suffix: Option<&str>,
        expected: Option<&Type>,
        negative: bool,
        span: Span,
        records: &[CheckedRecord],
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
                    ty.display(records)
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

    fn name(&mut self, name: &Ident) -> Result<(TypedExprKind, Type), Diagnostic> {
        if let Some(local) = self.local(&name.text) {
            return Ok((TypedExprKind::Local(local.id), local.ty.clone()));
        }
        if let Some(id) = self
            .names
            .functions
            .get(&format!("{}.{}", self.module, name.text))
        {
            return Ok(self.function(*id));
        }
        if let Some(builtin) = Builtin::ALL
            .iter()
            .find(|builtin| builtin.name() == name.text)
        {
            return Ok(self.builtin(*builtin));
        }
        Err(Diagnostic::new(
            "E1002",
            format!("unknown value '{}'", name.text),
            name.span,
        ))
    }

    fn binary_type(
        &mut self,
        operator: BinaryOp,
        left: &TypedExpr,
        right: &TypedExpr,
        span: Span,
    ) -> Result<Type, Diagnostic> {
        use BinaryOp::*;
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
    use crate::analyze;

    #[test]
    fn checks_value_types_higher_order_and_forward_recursion() {
        let source = "
            record Vec2 { x: f64, y: f64 }
            fn even(n: i64) -> bool { if n == 0 { true } else { odd(n - 1) } }
            fn odd(n: i64) -> bool { if n == 0 { false } else { even(n - 1) } }
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
