use std::collections::{BTreeMap, BTreeSet};

use crate::diagnostic::{Diagnostic, Span};
use crate::syntax::*;

pub const MAX_ARRAY_LENGTH: usize = 1024;
pub const MAX_VALUE_BYTES: usize = 64 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Type {
    Integer(u16, bool),
    Binary(u16),
    Decimal(u16),
    Bool,
    Unit,
    String,
    Record(usize),
    Array(Box<Type>, usize),
    Function(Vec<Type>, Box<Type>),
    Reference(Box<Type>, bool),
}

impl Type {
    pub const I64: Self = Self::Integer(64, true);
    pub const F64: Self = Self::Binary(64);

    pub fn display(&self, records: &[CheckedRecord]) -> String {
        match self {
            Self::Integer(bits, signed) => format!("i{bits}{}", if *signed { "" } else { "u" }),
            Self::Binary(bits) => format!("f{bits}"),
            Self::Decimal(bits) => format!("d{bits}"),
            Self::Bool => "bool".into(),
            Self::Unit => "unit".into(),
            Self::String => "string".into(),
            Self::Reference(ty, mutable) => format!(
                "&{}{}",
                if *mutable { "mut " } else { "" },
                ty.display(records)
            ),
            Self::Record(id) => records[*id].name.clone(),
            Self::Array(element, length) => {
                format!("[{}; {length}]", element.display(records))
            }
            Self::Function(parameters, result) => format!(
                "fn({}) -> {}",
                parameters
                    .iter()
                    .map(|parameter| parameter.display(records))
                    .collect::<Vec<_>>()
                    .join(", "),
                result.display(records),
            ),
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
            Self::String | Self::Reference(_, true) => false,
            Self::Record(id) => records[*id]
                .fields
                .iter()
                .all(|(_, ty)| ty.is_copy(records)),
            Self::Array(element, _) => element.is_copy(records),
            _ => true,
        }
    }

    pub fn needs_drop(&self, records: &[CheckedRecord]) -> bool {
        match self {
            Self::String => true,
            Self::Record(id) => records[*id]
                .fields
                .iter()
                .any(|(_, ty)| ty.needs_drop(records)),
            Self::Array(element, length) => *length != 0 && element.needs_drop(records),
            _ => false,
        }
    }

    pub fn contains_reference(&self) -> bool {
        match self {
            Self::Reference(..) => true,
            Self::Array(element, _) => element.contains_reference(),
            _ => false,
        }
    }

    pub fn exportable(&self) -> bool {
        matches!(
            self,
            Self::Integer(8 | 16 | 32 | 64, _) | Self::Binary(32 | 64) | Self::Bool
        )
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
}

impl Builtin {
    pub const ALL: [Self; 8] = [
        Self::Sqrt,
        Self::Floor,
        Self::Ceil,
        Self::Abs,
        Self::ToFloat,
        Self::ToInt,
        Self::Assert,
        Self::CloneString,
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
        }
    }

    pub fn signature(self) -> Signature {
        let (parameter, result) = match self {
            Self::ToFloat => (Type::I64, Type::F64),
            Self::ToInt => (Type::F64, Type::I64),
            Self::Assert => (Type::Bool, Type::Unit),
            Self::CloneString => (Type::Reference(Box::new(Type::String), false), Type::String),
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
        Type::Function(self.parameters.clone(), Box::new(self.result.clone()))
    }
}

#[derive(Debug)]
pub struct CheckedModule {
    pub records: Vec<CheckedRecord>,
    pub functions: Vec<CheckedFunction>,
    pub entry: Option<usize>,
}

#[derive(Debug)]
pub struct CheckedRecord {
    pub name: String,
    pub fields: Vec<(String, Type)>,
    pub span: Span,
}

#[derive(Debug)]
pub struct CheckedFunction {
    pub module: String,
    pub name: String,
    pub exported: bool,
    pub parameters: Vec<Local>,
    pub signature: Signature,
    pub body: TypedExpr,
    pub span: Span,
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

#[derive(Debug)]
pub struct TypedExpr {
    pub kind: TypedExprKind,
    pub ty: Type,
    pub span: Span,
}

#[derive(Debug)]
pub enum TypedExprKind {
    Int(u128),
    Float(String),
    String(String),
    Bool(bool),
    Unit,
    Local(usize),
    Function(FunctionRef),
    Unary(UnaryOp, Box<TypedExpr>),
    Binary(BinaryOp, Box<TypedExpr>, Box<TypedExpr>),
    Call(Box<TypedExpr>, Vec<TypedExpr>),
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
    Field(Box<TypedExpr>, usize),
    Index(Box<TypedExpr>, Box<TypedExpr>),
    Length(Box<TypedExpr>, usize),
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
                    if text == name && name != "_"
            )
        });
        if !valid || !names.modules.insert(name.to_owned()) {
            return Err(Diagnostic::new(
                "E1011",
                format!(
                    "invalid or duplicate module name '{name}'; each .tzr filename must be a unique ASCII identifier, not '_' or a keyword"
                ),
                Span::default().in_source(source),
            ));
        }
    }
    let record_declarations: Vec<_> = modules
        .iter()
        .flat_map(|(name, program)| program.records.iter().map(|record| (*name, record)))
        .collect();
    let function_declarations: Vec<_> = modules
        .iter()
        .flat_map(|(name, program)| program.functions.iter().map(|function| (*name, function)))
        .collect();
    for (id, (module, record)) in record_declarations.iter().enumerate() {
        let qualified = format!("{module}.{}", record.name.text);
        if crate::numeric::primitive(&record.name.text).is_some()
            || record.name.text == "_"
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
        if function.exported
            && (parameters.iter().any(|ty| !ty.exportable())
                || (!result.exportable() && result != Type::Unit))
        {
            return Err(Diagnostic::new(
                "E1008",
                "exports support 8/16/32/64-bit integers, f32, f64, bool, and unit results; keep wide/software numbers, strings, references, aggregates, and function values inside Tsuzuri",
                function.name.span,
            ));
        }
        if result.contains_reference()
            && parameters
                .iter()
                .filter(|ty| ty.contains_reference())
                .count()
                != 1
        {
            return Err(Diagnostic::new(
                "E1013",
                "a borrowed result requires exactly one borrowed input for lifetime elision",
                function.result.span,
            ));
        }
        signatures.push(Signature { parameters, result });
    }
    let mut functions = Vec::new();
    for (id, (module, function)) in function_declarations.iter().enumerate() {
        let signature = signatures[id].clone();
        let mut checker = Checker::new(module, &names, &records, &record_sizes, &signatures);
        let mut parameters = Vec::new();
        for (parameter, ty) in function.parameters.iter().zip(&signature.parameters) {
            parameters.push(checker.bind(&parameter.name, ty.clone(), parameter.mutable));
        }
        let body = checker.expression(&function.body, Some(&signature.result))?;
        functions.push(CheckedFunction {
            module: (*module).to_owned(),
            name: function.name.text.clone(),
            exported: function.exported,
            parameters,
            signature,
            body,
            span: function.name.span,
        });
    }
    let mut entry = names.functions.get("Main.main").copied();
    for (module, program) in modules {
        let Some(expression) = &program.entry else {
            continue;
        };
        if *module != "Main" {
            return Err(Diagnostic::new(
                "E2004",
                "top-level execution is only allowed in Main.tzr; other modules contain record and function declarations",
                expression.span,
            ));
        }
        if entry.is_some() {
            return Err(Diagnostic::new(
                "E2004",
                "Main.tzr must use either top-level entry-point code or 'fn main', not both",
                expression.span,
            ));
        }
        let mut checker = Checker::new(module, &names, &records, &record_sizes, &signatures);
        let body = checker.expression(expression, None)?;
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
        });
    }
    let module = CheckedModule {
        records,
        functions,
        entry,
    };
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
        TypeExprKind::Reference(ty, mutable) => {
            Type::Reference(Box::new(resolve_type(ty, module, names)?), *mutable)
        }
        TypeExprKind::Array(element, length) => {
            if *length > MAX_ARRAY_LENGTH {
                return Err(Diagnostic::new(
                    "E1004",
                    format!("fixed arrays are limited to {MAX_ARRAY_LENGTH} elements"),
                    expression.span,
                ));
            }
            Type::Array(Box::new(resolve_type(element, module, names)?), *length)
        }
        TypeExprKind::Function(parameters, result) => Type::Function(
            parameters
                .iter()
                .map(|parameter| resolve_type(parameter, module, names))
                .collect::<Result<_, _>>()?,
            Box::new(resolve_type(result, module, names)?),
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
        Type::Array(element, length) => {
            layout_size(element, records, sizes, visiting, depth)?.saturating_mul(*length)
        }
        Type::Integer(128, _) | Type::Binary(128) | Type::Decimal(128) | Type::String => 16,
        // Small values conservatively occupy at least one pointer-sized slot.
        _ => 8,
    })
}

fn validate_size(ty: &Type, sizes: &[usize], span: Span) -> Result<usize, Diagnostic> {
    let size = match ty {
        Type::Record(id) => sizes[*id],
        Type::Array(element, length) => {
            validate_size(element, sizes, span)?.saturating_mul(*length)
        }
        Type::Function(parameters, result) => {
            for parameter in parameters {
                validate_size(parameter, sizes, span)?;
            }
            validate_size(result, sizes, span)?;
            8
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
    signatures: &'a [Signature],
    scopes: Vec<BTreeMap<String, Local>>,
    next_local: usize,
}

impl<'a> Checker<'a> {
    fn new(
        module: &'a str,
        names: &'a Names,
        records: &'a [CheckedRecord],
        record_sizes: &'a [usize],
        signatures: &'a [Signature],
    ) -> Self {
        Self {
            module,
            names,
            records,
            record_sizes,
            signatures,
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

    fn same(&self, actual: &Type, expected: &Type, span: Span) -> Result<(), Diagnostic> {
        if actual == expected {
            Ok(())
        } else {
            Err(Diagnostic::new(
                "E1003",
                format!(
                    "expected {}, found {}",
                    expected.display(self.records),
                    actual.display(self.records)
                ),
                span,
            ))
        }
    }

    fn expression(
        &mut self,
        expression: &Expr,
        expected: Option<&Type>,
    ) -> Result<TypedExpr, Diagnostic> {
        let (kind, ty) = match &expression.kind {
            ExprKind::Integer(value, suffix) => {
                self.integer(*value, suffix.as_deref(), expected, false, expression.span)?
            }
            ExprKind::Float(value, suffix) => {
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
            ExprKind::String(text) => (TypedExprKind::String(text.clone()), Type::String),
            ExprKind::Bool(value) => (TypedExprKind::Bool(*value), Type::Bool),
            ExprKind::Unit => (TypedExprKind::Unit, Type::Unit),
            ExprKind::Name(name) => self.name(name)?,
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
                let valid = match operator {
                    UnaryOp::Negate => {
                        operand.ty.is_float() || matches!(operand.ty, Type::Integer(_, true))
                    }
                    UnaryOp::Not => operand.ty == Type::Bool,
                    UnaryOp::BitNot => operand.ty.is_integer(),
                };
                if !valid {
                    return Err(Diagnostic::new(
                        "E1005",
                        format!(
                            "invalid operand {} for {operator:?}",
                            operand.ty.display(self.records)
                        ),
                        expression.span,
                    ));
                }
                let ty = operand.ty.clone();
                (TypedExprKind::Unary(*operator, Box::new(operand)), ty)
            }
            ExprKind::Binary(operator, left, right) => {
                let (left, right) = if *operator == BinaryOp::Pipe {
                    let right = self.expression(right, None)?;
                    let hint = match &right.ty {
                        Type::Function(parameters, _) if parameters.len() == 1 => {
                            Some(&parameters[0])
                        }
                        _ => None,
                    };
                    (self.expression(left, hint)?, right)
                } else {
                    let hint = expected.filter(|ty| ty.is_numeric() || **ty == Type::String);
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
                    let Type::Function(parameters, result) = &right.ty else {
                        return Err(Diagnostic::new(
                            "E1005",
                            "the right side of '|>' must be a one-argument function",
                            right.span,
                        ));
                    };
                    if parameters.len() != 1 {
                        return Err(Diagnostic::new(
                            "E1006",
                            "'|>' requires a one-argument function",
                            right.span,
                        ));
                    }
                    self.same(&left.ty, &parameters[0], left.span)?;
                    (**result).clone()
                } else {
                    self.binary_type(*operator, &left, &right, expression.span)?
                };
                (
                    TypedExprKind::Binary(*operator, Box::new(left), Box::new(right)),
                    result,
                )
            }
            ExprKind::Call(callee, arguments) => {
                let callee = self.expression(callee, None)?;
                let Type::Function(parameters, result) = &callee.ty else {
                    return Err(Diagnostic::new(
                        "E1005",
                        "only a function value can be called",
                        callee.span,
                    ));
                };
                if parameters.len() != arguments.len() {
                    return Err(Diagnostic::new(
                        "E1006",
                        format!(
                            "expected {} arguments, found {}",
                            parameters.len(),
                            arguments.len()
                        ),
                        expression.span,
                    ));
                }
                let arguments = arguments
                    .iter()
                    .zip(parameters)
                    .map(|(argument, parameter)| self.expression(argument, Some(parameter)))
                    .collect::<Result<_, _>>()?;
                let result = (**result).clone();
                (TypedExprKind::Call(Box::new(callee), arguments), result)
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
            ExprKind::Block { bindings, result } => {
                self.scopes.push(BTreeMap::new());
                let mut checked = Vec::new();
                for binding in bindings {
                    let annotation = binding
                        .annotation
                        .as_ref()
                        .map(|ty| resolve_type(ty, self.module, self.names))
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
            ExprKind::Array(values) => {
                if values.len() > MAX_ARRAY_LENGTH {
                    return Err(Diagnostic::new(
                        "E1004",
                        format!("fixed arrays are limited to {MAX_ARRAY_LENGTH} elements"),
                        expression.span,
                    ));
                }
                let mut element_type = match expected {
                    Some(Type::Array(element, _)) => Some((**element).clone()),
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
                        "an empty array needs a type annotation, for example '[i64; 0]'",
                        expression.span,
                    )
                })?;
                let ty = Type::Array(Box::new(element), values.len());
                validate_size(&ty, self.record_sizes, expression.span)?;
                (TypedExprKind::Array(checked), ty)
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
                    Type::Array(_, length) if field.text == "length" => {
                        let length = *length;
                        (TypedExprKind::Length(Box::new(value), length), Type::I64)
                    }
                    Type::String if field.text == "length" => {
                        (TypedExprKind::StringLength(Box::new(value)), Type::I64)
                    }
                    _ => {
                        return Err(Diagnostic::new(
                            "E1007",
                            "field access requires a record, or '.length' on an array or string",
                            field.span,
                        ));
                    }
                }
            }
            ExprKind::Index(value, index) => {
                let value = Self::autoderef(self.expression(value, None)?);
                let ty = match &value.ty {
                    Type::Array(element, _) => (**element).clone(),
                    Type::String => Type::Integer(8, false),
                    _ => {
                        return Err(Diagnostic::new(
                            "E1005",
                            "indexing requires a fixed array or string",
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
                if !matches!(
                    place.kind,
                    TypedExprKind::Local(_) | TypedExprKind::Dereference(_)
                ) {
                    return Err(Diagnostic::new(
                        "E1012",
                        "assignment replaces a mutable binding; record fields and array elements are immutable",
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
                let ty = resolve_type(ty, self.module, self.names)?;
                if !value.ty.is_numeric() || !ty.is_numeric() {
                    return Err(Diagnostic::new(
                        "E1005",
                        "'as' converts between numeric types only",
                        expression.span,
                    ));
                }
                (TypedExprKind::Cast(Box::new(value)), ty)
            }
        };
        if let Some(expected) = expected {
            self.same(&ty, expected, expression.span)?;
        }
        Ok(TypedExpr {
            kind,
            ty,
            span: expression.span,
        })
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
        &self,
        value: u128,
        suffix: Option<&str>,
        expected: Option<&Type>,
        negative: bool,
        span: Span,
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
                    ty.display(self.records)
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

    fn function(&self, id: usize) -> (TypedExprKind, Type) {
        (
            TypedExprKind::Function(FunctionRef::User(id)),
            self.signatures[id].as_type(),
        )
    }

    fn name(&self, name: &Ident) -> Result<(TypedExprKind, Type), Diagnostic> {
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
            return Ok((
                TypedExprKind::Function(FunctionRef::Builtin(*builtin)),
                builtin.signature().as_type(),
            ));
        }
        Err(Diagnostic::new(
            "E1002",
            format!("unknown value '{}'", name.text),
            name.span,
        ))
    }

    fn binary_type(
        &self,
        operator: BinaryOp,
        left: &TypedExpr,
        right: &TypedExpr,
        span: Span,
    ) -> Result<Type, Diagnostic> {
        use BinaryOp::*;
        self.same(&right.ty, &left.ty, right.span)?;
        let valid = match operator {
            Add | Subtract | Multiply | Divide | Less | LessEqual | Greater | GreaterEqual => {
                left.ty.is_numeric() || (operator == Add && left.ty == Type::String)
            }
            Remainder | BitAnd | BitOr | BitXor | ShiftLeft | ShiftRight | ShiftRightUnsigned => {
                left.ty.is_integer()
            }
            And | Or => left.ty == Type::Bool,
            Equal | NotEqual => left.ty.is_scalar() || matches!(left.ty, Type::Unit | Type::String),
            Pipe => unreachable!("pipeline types are checked separately"),
        };
        if !valid {
            return Err(Diagnostic::new(
                "E1005",
                format!(
                    "operator {operator:?} does not accept {}",
                    left.ty.display(self.records)
                ),
                span,
            ));
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
                 fn f() -> i64 { let a: [i64; 0] = []; let _ = Empty {}; -9223372036854775808 + a.length }
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
            ("record A { a: [A; 0] }", "E1010"),
            ("fn f(a: [i64; 1025]) -> i64 { 0 }", "E1004"),
            ("record A { a: [[i64; 1024]; 1024] }", "E1010"),
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
        assert!(analyze("fn f(x: [i64; 1024]) -> i64 { x.length }").is_ok());
        assert!(analyze("fn f(x: [i64; 1025]) -> i64 { x.length }").is_err());
        assert!(analyze("record Full { values: [[i64; 1024]; 8] }").is_ok());
        assert_eq!(
            analyze("record TooLarge { values: [[i64; 1024]; 8], extra: i64 }")
                .unwrap_err()
                .code,
            "E1010"
        );
    }
}
