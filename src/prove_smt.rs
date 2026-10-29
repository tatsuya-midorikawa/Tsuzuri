//! The translation of a woven, specialized function to an SMT-LIB script of `QF_BV` (A21
//! Phase 5, appendix C of A21). Values are `Bool` and bit vectors only; traps are not values but
//! obligations, each the condition under which the expression that may trap is reached and traps.

use std::collections::BTreeMap;

use crate::check::{
    Builtin, CheckedFunction, CheckedModule, FunctionRef, Type, TypeContext, TypedExpr,
    TypedExprKind,
};
use crate::diagnostic::Span;
use crate::syntax::{BinaryOp, UnaryOp};

/// The most definitions of one script (appendix C.10).
pub(crate) const MAX_DEFINITIONS: usize = 50_000;
/// How deep calls are inlined.
const MAX_INLINE: usize = 8;

/// A value of the script: SMT-LIB terms, with the widths of bit vectors.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Value {
    Unit,
    Bool(String),
    Bits(String, u16),
    Tuple(Vec<Value>),
}

/// Why a function cannot be proved: the English reason of appendix C.2 and C.3, and where.
#[derive(Clone, Debug)]
pub(crate) struct Unsupported {
    pub reason: String,
    pub span: Span,
}

/// A condition under which the program traps: the name of its definition and what traps.
#[derive(Clone, Debug)]
pub(crate) struct Obligation {
    pub name: String,
    pub what: &'static str,
}

/// The term of `value` (`width` bits) as SMT-LIB.
pub fn literal(value: u128, width: u16) -> String {
    let masked = if width >= 128 {
        value
    } else {
        value & ((1u128 << width) - 1)
    };
    format!("(_ bv{masked} {width})")
}

/// The term of `left operator right` on integers of `width` bits, signed or not, as the language
/// defines the operator (it wraps; the traps of `/` and `%` are obligations, not values). `None`
/// for an operator that is not an integer operation of the language.
pub fn binary_term(
    operator: BinaryOp,
    width: u16,
    signed: bool,
    left: &str,
    right: &str,
) -> Option<String> {
    let mask = || literal(u128::from(width - 1), width);
    Some(match operator {
        BinaryOp::Add => format!("(bvadd {left} {right})"),
        BinaryOp::Subtract => format!("(bvsub {left} {right})"),
        BinaryOp::Multiply => format!("(bvmul {left} {right})"),
        BinaryOp::Divide if signed => format!("(bvsdiv {left} {right})"),
        BinaryOp::Divide => format!("(bvudiv {left} {right})"),
        BinaryOp::Remainder if signed => format!("(bvsrem {left} {right})"),
        BinaryOp::Remainder => format!("(bvurem {left} {right})"),
        BinaryOp::Less if signed => format!("(bvslt {left} {right})"),
        BinaryOp::Less => format!("(bvult {left} {right})"),
        BinaryOp::LessEqual if signed => format!("(bvsle {left} {right})"),
        BinaryOp::LessEqual => format!("(bvule {left} {right})"),
        BinaryOp::Greater if signed => format!("(bvsgt {left} {right})"),
        BinaryOp::Greater => format!("(bvugt {left} {right})"),
        BinaryOp::GreaterEqual if signed => format!("(bvsge {left} {right})"),
        BinaryOp::GreaterEqual => format!("(bvuge {left} {right})"),
        BinaryOp::Equal => format!("(= {left} {right})"),
        BinaryOp::NotEqual => format!("(not (= {left} {right}))"),
        BinaryOp::BitAnd => format!("(bvand {left} {right})"),
        BinaryOp::BitOr => format!("(bvor {left} {right})"),
        BinaryOp::BitXor => format!("(bvxor {left} {right})"),
        BinaryOp::ShiftLeft => format!("(bvshl {left} (bvand {right} {}))", mask()),
        BinaryOp::ShiftRight if signed => format!("(bvashr {left} (bvand {right} {}))", mask()),
        BinaryOp::ShiftRight | BinaryOp::ShiftRightUnsigned => {
            format!("(bvlshr {left} (bvand {right} {}))", mask())
        }
        BinaryOp::And | BinaryOp::Or | BinaryOp::Power | BinaryOp::Pipe => return None,
    })
}

fn sort(width: u16) -> String {
    format!("(_ BitVec {width})")
}

/// One script under construction: declarations, definitions and obligations, in order.
pub(crate) struct Translator<'a> {
    module: &'a CheckedModule,
    types: TypeContext<'a>,
    pub(crate) declarations: Vec<String>,
    pub(crate) definitions: Vec<String>,
    pub(crate) obligations: Vec<Obligation>,
    /// The arguments' constants and types, for the counterexample.
    pub(crate) arguments: Vec<(Value, Type)>,
    /// Premises that the arguments' types give (a union's tag is below its case count).
    pub(crate) premises: Vec<String>,
    environment: BTreeMap<usize, Value>,
    inlined: Vec<usize>,
    next: usize,
    /// Inside the condition of a contract line: a division there is not supported.
    in_contract: bool,
}

type Translated = Result<Value, Unsupported>;

fn unsupported(reason: impl Into<String>, span: Span) -> Unsupported {
    Unsupported {
        reason: reason.into(),
        span,
    }
}

impl<'a> Translator<'a> {
    pub(crate) fn new(module: &'a CheckedModule) -> Self {
        Self {
            module,
            types: module.types(),
            declarations: Vec::new(),
            definitions: Vec::new(),
            obligations: Vec::new(),
            arguments: Vec::new(),
            premises: Vec::new(),
            environment: BTreeMap::new(),
            inlined: Vec::new(),
            next: 0,
            in_contract: false,
        }
    }

    fn define(&mut self, sort: &str, term: String, span: Span) -> Result<String, Unsupported> {
        if self.definitions.len() >= MAX_DEFINITIONS {
            return Err(unsupported("a proof script that is too large", span));
        }
        let name = format!("t{}", self.next);
        self.next += 1;
        self.definitions
            .push(format!("(define-fun {name} () {sort} {term})"));
        Ok(name)
    }

    fn boolean(&mut self, term: String, span: Span) -> Result<String, Unsupported> {
        if term.starts_with('(') {
            self.define("Bool", term, span)
        } else {
            Ok(term)
        }
    }

    fn bits(&mut self, term: String, width: u16, span: Span) -> Result<String, Unsupported> {
        if term.starts_with('(') && !term.starts_with("(_ bv") {
            self.define(&sort(width), term, span)
        } else {
            Ok(term)
        }
    }

    /// Records that the program traps when `condition` holds on `path`.
    fn obligation(&mut self, path: &str, condition: &str, what: &'static str) {
        let name = format!("o{}", self.obligations.len());
        let condition = if path == "true" {
            condition.to_owned()
        } else {
            format!("(and {path} {condition})")
        };
        self.definitions
            .push(format!("(define-fun {name} () Bool {condition})"));
        self.obligations.push(Obligation { name, what });
    }

    /// The value of a type: `None` with the reason when the type is not supported (C.2).
    fn shape(&self, ty: &Type) -> Result<Shape, String> {
        Ok(match ty {
            Type::Integer(width, _) => Shape::Bits(*width),
            Type::Bool => Shape::Bool,
            Type::Unit => Shape::Unit,
            Type::Reference(inner, false) => self.shape(inner)?,
            Type::Reference(_, true) => return Err("a mutable reference".into()),
            Type::Tuple(elements) => Shape::Tuple(
                elements
                    .iter()
                    .map(|element| self.shape(element))
                    .collect::<Result<_, _>>()?,
            ),
            Type::Record(id, arguments) => {
                if self.types.recursive(ty) {
                    return Err("a recursive type".into());
                }
                Shape::Tuple(
                    self.types
                        .record_fields(*id, arguments)
                        .iter()
                        .map(|field| self.shape(field))
                        .collect::<Result<_, _>>()?,
                )
            }
            Type::Union(id, arguments) => {
                let payloads = self.types.union_payloads(*id, arguments);
                if payloads.iter().any(Option::is_some) || self.types.recursive(ty) {
                    return Err("a union with a payload".into());
                }
                Shape::Tag(payloads.len())
            }
            Type::Binary(_) | Type::Decimal(_) | Type::Simd(_) => {
                return Err("a floating-point value".into());
            }
            Type::Char | Type::Utf8Char => return Err("a character".into()),
            Type::String | Type::Utf8String => return Err("a string".into()),
            Type::Array(_)
            | Type::ArrayView(_)
            | Type::FixedArray(..)
            | Type::List(_)
            | Type::Vec(_) => return Err("an array or list".into()),
            Type::Function(..) => return Err("a closure".into()),
            Type::Task(_) => return Err("a task".into()),
            Type::Dyn(_) => return Err("a dyn value".into()),
            Type::Shared(..) => return Err("a shared pointer".into()),
            Type::Handle(_) => return Err("an extern handle".into()),
            Type::Length(_)
            | Type::Application(..)
            | Type::Partial(_)
            | Type::Variable(_)
            | Type::Infer(_)
            | Type::Error => return Err("an unresolved type".into()),
        })
    }

    /// Declares the constants of an argument of type `ty` named `name`.
    fn declare(&mut self, name: &str, shape: &Shape) -> Value {
        match shape {
            Shape::Unit => Value::Unit,
            Shape::Bool => {
                self.declarations
                    .push(format!("(declare-const {name} Bool)"));
                Value::Bool(name.into())
            }
            Shape::Bits(width) => {
                self.declarations
                    .push(format!("(declare-const {name} {})", sort(*width)));
                Value::Bits(name.into(), *width)
            }
            Shape::Tag(count) => {
                self.declarations
                    .push(format!("(declare-const {name} {})", sort(32)));
                self.premises
                    .push(format!("(bvult {name} {})", literal(*count as u128, 32)));
                Value::Bits(name.into(), 32)
            }
            Shape::Tuple(elements) => Value::Tuple(
                elements
                    .iter()
                    .enumerate()
                    .map(|(index, element)| self.declare(&format!("{name}_{index}"), element))
                    .collect(),
            ),
        }
    }

    /// Binds the parameters of `function` to fresh constants `p0`, `p1`, ...
    pub(crate) fn parameters(&mut self, function: &CheckedFunction) -> Result<(), Unsupported> {
        for (index, parameter) in function.parameters.iter().enumerate() {
            let shape = self
                .shape(&parameter.ty)
                .map_err(|reason| unsupported(reason, parameter.span))?;
            let value = self.declare(&format!("p{index}"), &shape);
            self.environment.insert(parameter.id, value.clone());
            self.arguments.push((value, parameter.ty.clone()));
        }
        Ok(())
    }

    pub(crate) fn bind(&mut self, local: usize, value: Value) {
        self.environment.insert(local, value);
    }

    /// A contract line's condition, where a division is not supported (C.10).
    pub(crate) fn condition(
        &mut self,
        expression: &TypedExpr,
        path: &str,
    ) -> Result<String, Unsupported> {
        self.in_contract = true;
        let value = self.translate(expression, path);
        self.in_contract = false;
        match value? {
            Value::Bool(term) => Ok(term),
            _ => Err(unsupported("an unresolved expression", expression.span)),
        }
    }

    fn ite(&mut self, condition: &str, then: Value, otherwise: Value, span: Span) -> Translated {
        Ok(match (then, otherwise) {
            (Value::Unit, Value::Unit) => Value::Unit,
            (Value::Bool(a), Value::Bool(b)) => {
                Value::Bool(self.define("Bool", format!("(ite {condition} {a} {b})"), span)?)
            }
            (Value::Bits(a, width), Value::Bits(b, _)) => Value::Bits(
                self.define(&sort(width), format!("(ite {condition} {a} {b})"), span)?,
                width,
            ),
            (Value::Tuple(a), Value::Tuple(b)) => Value::Tuple(
                a.into_iter()
                    .zip(b)
                    .map(|(a, b)| self.ite(condition, a, b, span))
                    .collect::<Result<_, _>>()?,
            ),
            _ => return Err(unsupported("an unresolved expression", span)),
        })
    }

    fn equal(&mut self, left: &Value, right: &Value, span: Span) -> Result<String, Unsupported> {
        Ok(match (left, right) {
            (Value::Unit, Value::Unit) => "true".into(),
            (Value::Bool(a), Value::Bool(b)) | (Value::Bits(a, _), Value::Bits(b, _)) => {
                self.boolean(format!("(= {a} {b})"), span)?
            }
            (Value::Tuple(a), Value::Tuple(b)) => {
                let parts = a
                    .iter()
                    .zip(b)
                    .map(|(a, b)| self.equal(a, b, span))
                    .collect::<Result<Vec<_>, _>>()?;
                if parts.is_empty() {
                    "true".into()
                } else {
                    self.boolean(format!("(and {})", parts.join(" ")), span)?
                }
            }
            _ => return Err(unsupported("an unresolved expression", span)),
        })
    }

    /// The value of `expression`, evaluated where `path` holds (C.1, C.3).
    pub(crate) fn translate(&mut self, expression: &TypedExpr, path: &str) -> Translated {
        use TypedExprKind as K;
        let span = expression.span;
        match &expression.kind {
            K::Int(value) => match self.shape(&expression.ty) {
                Ok(Shape::Bits(width)) => Ok(Value::Bits(literal(*value, width), width)),
                _ => Err(unsupported("an unresolved expression", span)),
            },
            K::Bool(value) => Ok(Value::Bool(value.to_string())),
            K::Unit => Ok(Value::Unit),
            K::Local(id) => self
                .environment
                .get(id)
                .cloned()
                .ok_or_else(|| unsupported("an unresolved expression", span)),
            K::Unary(operator, value) => {
                let value = self.translate(value, path)?;
                match (operator, value) {
                    (UnaryOp::Not, Value::Bool(term)) => {
                        Ok(Value::Bool(self.boolean(format!("(not {term})"), span)?))
                    }
                    (UnaryOp::Negate, Value::Bits(term, width)) => Ok(Value::Bits(
                        self.bits(format!("(bvneg {term})"), width, span)?,
                        width,
                    )),
                    (UnaryOp::BitNot, Value::Bits(term, width)) => Ok(Value::Bits(
                        self.bits(format!("(bvnot {term})"), width, span)?,
                        width,
                    )),
                    (UnaryOp::Plus, value) => Ok(value),
                    _ => Err(unsupported("an unresolved expression", span)),
                }
            }
            K::Binary(operator, left, right) => self.binary(*operator, left, right, path, span),
            K::Call(callee, arguments) => self.call(callee, arguments, expression, path),
            K::If {
                condition,
                then_branch,
                else_branch,
            } => {
                let Value::Bool(condition) = self.translate(condition, path)? else {
                    return Err(unsupported("an unresolved expression", span));
                };
                let then_path = self.boolean(format!("(and {path} {condition})"), span)?;
                let then = self.translate(then_branch, &then_path)?;
                let else_path = self.boolean(format!("(and {path} (not {condition}))"), span)?;
                let otherwise = self.translate(else_branch, &else_path)?;
                self.ite(&condition, then, otherwise, span)
            }
            K::Block { bindings, result } => {
                for (local, value) in bindings {
                    let value = self.translate(value, path)?;
                    self.environment.insert(local.id, value);
                }
                self.translate(result, path)
            }
            K::Match { local, value, arms } => {
                let value = self.translate(value, path)?;
                self.environment.insert(local.id, value);
                let mut reached = path.to_owned();
                let mut results: Vec<(String, Value)> = Vec::new();
                for (index, arm) in arms.iter().enumerate() {
                    if arm.alternatives.len() > 1
                        && arm.alternatives.iter().any(|alternative| {
                            !alternative.bindings.is_empty()
                                || alternative
                                    .steps
                                    .iter()
                                    .any(|step| matches!(step, crate::check::PatternStep::Bind(..)))
                        })
                    {
                        return Err(unsupported("a pattern with alternatives that bind", span));
                    }
                    let mut alternatives = Vec::new();
                    for alternative in &arm.alternatives {
                        let mut tests = Vec::new();
                        for step in &alternative.steps {
                            match step {
                                crate::check::PatternStep::Test(test) => {
                                    let test_path = if tests.is_empty() {
                                        reached.clone()
                                    } else {
                                        self.boolean(
                                            format!("(and {reached} {})", tests.join(" ")),
                                            span,
                                        )?
                                    };
                                    let Value::Bool(term) = self.translate(test, &test_path)?
                                    else {
                                        return Err(unsupported("an unresolved expression", span));
                                    };
                                    tests.push(term);
                                }
                                crate::check::PatternStep::Bind(local, value) => {
                                    let value = self.translate(value, &reached)?;
                                    self.environment.insert(local.id, value);
                                }
                            }
                        }
                        for (local, value) in &alternative.bindings {
                            let value = self.translate(value, &reached)?;
                            self.environment.insert(local.id, value);
                        }
                        alternatives.push(match tests.len() {
                            0 => "true".to_owned(),
                            1 => tests.pop().unwrap(),
                            _ => self.boolean(format!("(and {})", tests.join(" ")), span)?,
                        });
                    }
                    let mut matched = match alternatives.len() {
                        0 => "true".to_owned(),
                        1 => alternatives.pop().unwrap(),
                        _ => self.boolean(format!("(or {})", alternatives.join(" ")), span)?,
                    };
                    if let Some(guard) = &arm.guard {
                        let guard_path =
                            self.boolean(format!("(and {reached} {matched})"), span)?;
                        let Value::Bool(term) = self.translate(guard, &guard_path)? else {
                            return Err(unsupported("an unresolved expression", span));
                        };
                        matched = self.boolean(format!("(and {matched} {term})"), span)?;
                    }
                    let last = index + 1 == arms.len();
                    let body_path = if last {
                        reached.clone()
                    } else {
                        self.boolean(format!("(and {reached} {matched})"), span)?
                    };
                    let body = self.translate(&arm.body, &body_path)?;
                    results.push((if last { "true".into() } else { matched.clone() }, body));
                    reached = self.boolean(format!("(and {reached} (not {matched}))"), span)?;
                }
                let Some((_, mut value)) = results.pop() else {
                    return Err(unsupported("an unresolved expression", span));
                };
                while let Some((condition, then)) = results.pop() {
                    value = self.ite(&condition, then, value, span)?;
                }
                Ok(value)
            }
            K::Record(fields) => {
                let mut values = vec![Value::Unit; fields.len()];
                for (index, value) in fields {
                    let value = self.translate(value, path)?;
                    if let Some(slot) = values.get_mut(*index) {
                        *slot = value;
                    }
                }
                Ok(Value::Tuple(values))
            }
            K::RecordUpdate { base, fields } => {
                let Value::Tuple(mut values) = self.translate(base, path)? else {
                    return Err(unsupported("an unresolved expression", span));
                };
                for (index, value) in fields {
                    let value = self.translate(value, path)?;
                    if let Some(slot) = values.get_mut(*index) {
                        *slot = value;
                    }
                }
                Ok(Value::Tuple(values))
            }
            K::Tuple(values) => Ok(Value::Tuple(
                values
                    .iter()
                    .map(|value| self.translate(value, path))
                    .collect::<Result<_, _>>()?,
            )),
            K::Field(value, index) => match self.translate(value, path)? {
                Value::Tuple(mut values) if *index < values.len() => Ok(values.swap_remove(*index)),
                _ => Err(unsupported("an unresolved expression", span)),
            },
            K::Borrow(value, false) | K::BorrowOperand(value) | K::Dereference(value) => {
                self.translate(value, path)
            }
            K::Borrow(_, true) => Err(unsupported("a mutable reference", span)),
            K::Cast(value) => {
                let (Type::Integer(to, _), Type::Integer(from, signed)) =
                    (&expression.ty, &value.ty)
                else {
                    return Err(unsupported(
                        format!("a cast of '{}'", value.ty.display(&self.types)),
                        span,
                    ));
                };
                let Value::Bits(term, _) = self.translate(value, path)? else {
                    return Err(unsupported("an unresolved expression", span));
                };
                let term = match to.cmp(from) {
                    std::cmp::Ordering::Equal => return Ok(Value::Bits(term, *to)),
                    std::cmp::Ordering::Less => format!("((_ extract {} 0) {term})", to - 1),
                    std::cmp::Ordering::Greater if *signed => {
                        format!("((_ sign_extend {}) {term})", to - from)
                    }
                    std::cmp::Ordering::Greater => {
                        format!("((_ zero_extend {}) {term})", to - from)
                    }
                };
                Ok(Value::Bits(self.define(&sort(*to), term, span)?, *to))
            }
            K::Construct {
                case_id,
                payload: None,
                ..
            } => Ok(Value::Bits(literal(*case_id as u128, 32), 32)),
            K::Construct {
                payload: Some(_), ..
            }
            | K::UnionPayload { .. } => Err(unsupported("a union with a payload", span)),
            K::UnionTag(value) => self.translate(value, path),
            K::StructuralCompare(operator, values) => match (operator, values.as_slice()) {
                (BinaryOp::Equal | BinaryOp::NotEqual, [left, right]) => {
                    let left = self.translate(left, path)?;
                    let right = self.translate(right, path)?;
                    let equal = self.equal(&left, &right, span)?;
                    Ok(Value::Bool(if *operator == BinaryOp::Equal {
                        equal
                    } else {
                        self.boolean(format!("(not {equal})"), span)?
                    }))
                }
                _ => Err(unsupported("a structural comparison", span)),
            },
            K::Function(_) => Err(unsupported("a function value", span)),
            K::Error
            | K::GenericFunction(..)
            | K::Method(..)
            | K::TypeFunction { .. }
            | K::GenericInteger(..)
            | K::GenericFloat(_)
            | K::CaseConstructor { .. }
            | K::SaturatingIndex(_) => Err(unsupported("an unresolved expression", span)),
            K::Float(_) => Err(unsupported("a floating-point value", span)),
            K::String(_) | K::StringLength(_) => Err(unsupported("a string", span)),
            K::StructuralHash(_) => Err(unsupported("a hash", span)),
            K::StructuralDisplay(_) | K::Interpolated(_) => {
                Err(unsupported("a text conversion", span))
            }
            K::Break | K::Continue | K::While { .. } | K::ForRange { .. } | K::ForEach { .. } => {
                Err(unsupported("a loop", span))
            }
            K::Lambda { .. } | K::Closure(..) => Err(unsupported("a closure", span)),
            K::TaskRun(_) | K::TaskParallel(_) | K::TaskParallelResults(_) | K::Parallel(..) => {
                Err(unsupported("a task", span))
            }
            K::HostCall(..) => Err(unsupported("an extern call", span)),
            K::Static(_) => Err(unsupported("a static value", span)),
            K::Array(_)
            | K::List(_)
            | K::ListTail(..)
            | K::NewArray(..)
            | K::NewList(..)
            | K::NewLiteral(_)
            | K::Index(..)
            | K::Slice { .. }
            | K::Length(_) => Err(unsupported("an array or list", span)),
            K::Assign(..) => Err(unsupported("an assignment", span)),
            K::Checked(_) => Err(unsupported("a '@checked' operation", span)),
            K::Try(_) | K::RaisedException | K::Reraise => Err(unsupported("an exception", span)),
            K::DynDispatch { .. } => Err(unsupported("a dyn call", span)),
        }
    }

    fn binary(
        &mut self,
        operator: BinaryOp,
        left: &TypedExpr,
        right: &TypedExpr,
        path: &str,
        span: Span,
    ) -> Translated {
        match operator {
            BinaryOp::And | BinaryOp::Or => {
                let Value::Bool(left) = self.translate(left, path)? else {
                    return Err(unsupported("an unresolved expression", span));
                };
                let right_path = if operator == BinaryOp::And {
                    self.boolean(format!("(and {path} {left})"), span)?
                } else {
                    self.boolean(format!("(and {path} (not {left}))"), span)?
                };
                let Value::Bool(right) = self.translate(right, &right_path)? else {
                    return Err(unsupported("an unresolved expression", span));
                };
                let word = if operator == BinaryOp::And {
                    "and"
                } else {
                    "or"
                };
                return Ok(Value::Bool(
                    self.boolean(format!("({word} {left} {right})"), span)?,
                ));
            }
            BinaryOp::Power => return Err(unsupported("a power operator", span)),
            BinaryOp::Pipe => return Err(unsupported("an unresolved pipe", span)),
            _ => {}
        }
        let left_value = self.translate(left, path)?;
        let right_value = self.translate(right, path)?;
        match (&left_value, &right_value) {
            (Value::Bits(a, width), Value::Bits(b, _)) => {
                let signed = matches!(left.ty, Type::Integer(_, true))
                    || matches!(&left.ty, Type::Reference(inner, _) if matches!(**inner, Type::Integer(_, true)));
                let width = *width;
                if matches!(operator, BinaryOp::Divide | BinaryOp::Remainder) {
                    if self.in_contract {
                        return Err(unsupported("a division in a contract", span));
                    }
                    let zero = literal(0, width);
                    self.obligation(path, &format!("(= {b} {zero})"), "integer division by zero");
                    if signed {
                        let minimum = literal(1u128 << (width - 1), width);
                        let minus_one = literal(u128::MAX, width);
                        self.obligation(
                            path,
                            &format!("(and (= {a} {minimum}) (= {b} {minus_one}))"),
                            "integer division overflow",
                        );
                    }
                }
                let term = binary_term(operator, width, signed, a, b)
                    .ok_or_else(|| unsupported("an unresolved expression", span))?;
                Ok(match operator {
                    BinaryOp::Less
                    | BinaryOp::LessEqual
                    | BinaryOp::Greater
                    | BinaryOp::GreaterEqual
                    | BinaryOp::Equal
                    | BinaryOp::NotEqual => Value::Bool(self.boolean(term, span)?),
                    _ => Value::Bits(self.define(&sort(width), term, span)?, width),
                })
            }
            (Value::Bool(a), Value::Bool(b)) => match operator {
                BinaryOp::Equal => Ok(Value::Bool(self.boolean(format!("(= {a} {b})"), span)?)),
                BinaryOp::NotEqual => Ok(Value::Bool(
                    self.boolean(format!("(not (= {a} {b}))"), span)?,
                )),
                _ => Err(unsupported("an unresolved expression", span)),
            },
            _ => Err(unsupported("an unresolved expression", span)),
        }
    }

    fn call(
        &mut self,
        callee: &TypedExpr,
        arguments: &[TypedExpr],
        expression: &TypedExpr,
        path: &str,
    ) -> Translated {
        let span = expression.span;
        match &callee.kind {
            TypedExprKind::Function(FunctionRef::User(id)) => {
                let function = &self.module.functions[*id];
                if self.inlined.contains(id) {
                    return Err(unsupported("a recursive call", span));
                }
                if self.inlined.len() >= MAX_INLINE {
                    return Err(unsupported(
                        format!("a call deeper than {MAX_INLINE}"),
                        span,
                    ));
                }
                if arguments.len() != function.parameters.len() {
                    return Err(unsupported("a function value", span));
                }
                let values = arguments
                    .iter()
                    .map(|argument| self.translate(argument, path))
                    .collect::<Result<Vec<_>, _>>()?;
                // The callee's locals are numbered in their own function, so they would overwrite
                // the caller's: the callee sees only its parameters, and the caller's environment
                // comes back whole.
                let mut callee = BTreeMap::new();
                for (parameter, value) in function.parameters.iter().zip(values) {
                    callee.insert(parameter.id, value);
                }
                let saved = std::mem::replace(&mut self.environment, callee);
                self.inlined.push(*id);
                let outer = std::mem::replace(&mut self.in_contract, false);
                let result = self.translate(&function.body, path);
                self.in_contract = outer;
                self.inlined.pop();
                self.environment = saved;
                result
            }
            TypedExprKind::Function(FunctionRef::Builtin(instance)) => {
                let values = arguments
                    .iter()
                    .map(|argument| self.translate(argument, path))
                    .collect::<Result<Vec<_>, _>>()?;
                let name = instance.builtin.name();
                let signed = arguments
                    .first()
                    .is_some_and(|argument| matches!(argument.ty, Type::Integer(_, true)));
                match (instance.builtin, values.as_slice()) {
                    (
                        Builtin::Assert | Builtin::ContractRequires | Builtin::ContractEnsures,
                        [Value::Bool(condition)],
                    ) => {
                        let what = match instance.builtin {
                            Builtin::ContractRequires => "contract requires failed",
                            Builtin::ContractEnsures => "contract ensures failed",
                            _ => "assertion failed",
                        };
                        self.obligation(path, &format!("(not {condition})"), what);
                        Ok(Value::Unit)
                    }
                    (Builtin::Unreachable, _) if expression.ty == Type::Unit => {
                        self.obligation(path, "true", "unreachable code reached");
                        Ok(Value::Unit)
                    }
                    (Builtin::Not, [Value::Bool(value)]) => {
                        Ok(Value::Bool(self.boolean(format!("(not {value})"), span)?))
                    }
                    (
                        Builtin::IntMin | Builtin::IntMax,
                        [Value::Bits(a, width), Value::Bits(b, _)],
                    ) => {
                        let compare = match (instance.builtin, signed) {
                            (Builtin::IntMin, true) => "bvsle",
                            (Builtin::IntMin, false) => "bvule",
                            (_, true) => "bvsge",
                            (_, false) => "bvuge",
                        };
                        let width = *width;
                        Ok(Value::Bits(
                            self.define(
                                &sort(width),
                                format!("(ite ({compare} {a} {b}) {a} {b})"),
                                span,
                            )?,
                            width,
                        ))
                    }
                    (Builtin::IntAbs, [Value::Bits(a, width)]) if signed => {
                        let width = *width;
                        let zero = literal(0, width);
                        Ok(Value::Bits(
                            self.define(
                                &sort(width),
                                format!("(ite (bvslt {a} {zero}) (bvneg {a}) {a})"),
                                span,
                            )?,
                            width,
                        ))
                    }
                    (
                        Builtin::IntWrappingAdd | Builtin::IntWrappingSub | Builtin::IntWrappingMul,
                        [Value::Bits(a, width), Value::Bits(b, _)],
                    ) => {
                        let operator = match instance.builtin {
                            Builtin::IntWrappingAdd => BinaryOp::Add,
                            Builtin::IntWrappingSub => BinaryOp::Subtract,
                            _ => BinaryOp::Multiply,
                        };
                        let width = *width;
                        let term =
                            binary_term(operator, width, signed, a, b).expect("integer arithmetic");
                        Ok(Value::Bits(self.bits(term, width, span)?, width))
                    }
                    (Builtin::IntWrappingNeg, [Value::Bits(a, width)]) => {
                        let width = *width;
                        Ok(Value::Bits(
                            self.bits(format!("(bvneg {a})"), width, span)?,
                            width,
                        ))
                    }
                    _ => Err(unsupported(format!("a call of '{name}'"), span)),
                }
            }
            TypedExprKind::Closure(..) | TypedExprKind::Local(_) => {
                Err(unsupported("a closure call", span))
            }
            _ => Err(unsupported("a closure call", span)),
        }
    }
}

/// How a value of a type is represented (C.2).
#[derive(Clone, Debug)]
enum Shape {
    Unit,
    Bool,
    Bits(u16),
    /// A union without payloads, with its number of cases.
    Tag(usize),
    Tuple(Vec<Shape>),
}

/// The constants of `value`, in order: what `get-value` asks for.
pub(crate) fn constants(value: &Value, into: &mut Vec<String>) {
    match value {
        Value::Unit => {}
        Value::Bool(term) | Value::Bits(term, _) => into.push(term.clone()),
        Value::Tuple(values) => values.iter().for_each(|value| constants(value, into)),
    }
}
