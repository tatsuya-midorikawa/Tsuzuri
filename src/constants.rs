use super::*;
use rustc_apfloat::ieee::{Double, Half, Quad, Single};
use rustc_apfloat::{Float, FloatConvert};

pub(super) fn failure(span: Span, message: &str) -> Diagnostic {
    Diagnostic::new("E1026", message, span)
}

fn limit(span: Span) -> Diagnostic {
    failure(
        span,
        "const evaluation exceeds the compiler limit; split or simplify the constants",
    )
}

pub(super) fn validate(expression: &Expr) -> Result<(), Diagnostic> {
    use ExprKind as Expr;
    match &expression.kind {
        Expr::Integer(..)
        | Expr::Float(..)
        | Expr::String(_)
        | Expr::Char(_)
        | Expr::Utf8Char(_)
        | Expr::Bool(_)
        | Expr::Unit
        | Expr::Name(_) => Ok(()),
        Expr::Unary(_, value) | Expr::Cast(value, _) | Expr::Field(value, _) => validate(value),
        Expr::Binary(operator, left, right) if *operator != BinaryOp::Pipe => {
            validate(left)?;
            validate(right)
        }
        Expr::If {
            condition,
            then_branch,
            else_branch,
        } => {
            validate(condition)?;
            validate(then_branch)?;
            validate(else_branch)
        }
        Expr::Array(values) | Expr::Tuple(values) => values.iter().try_for_each(validate),
        Expr::Record { fields, .. } => fields.iter().try_for_each(|(_, value)| validate(value)),
        Expr::Block { bindings, result } if bindings.is_empty() => validate(result),
        _ => Err(failure(
            expression.span,
            "this expression is not allowed in a phase-1 const; precompute it or use a runtime let",
        )),
    }
}

pub(super) fn fold(
    functions: &mut [CheckedFunction],
    constants: &BTreeSet<usize>,
) -> Result<(), Diagnostic> {
    let mut evaluator = Evaluator {
        functions,
        constants,
        values: BTreeMap::new(),
        active: BTreeSet::new(),
        references: 0,
        remaining: 1 << 20,
    };
    evaluator.run()?;
    let values = evaluator.values;
    for (id, function) in functions.iter_mut().enumerate() {
        let mut remaining = 1 << 20;
        inline(&mut function.body, &values, &mut remaining, 0)?;
        if constants.contains(&id) {
            function.body = values[&id].clone();
            function.visibility = Visibility::Private;
        }
    }
    Ok(())
}

fn constant_id(expression: &TypedExpr) -> Option<usize> {
    if let TypedExprKind::Call(callee, arguments) = &expression.kind
        && arguments.is_empty()
        && let TypedExprKind::Function(FunctionRef::User(id)) = callee.kind
    {
        Some(id)
    } else {
        None
    }
}

impl Checker<'_> {
    /// A shared borrow of a temporary argument, such as a constant or the
    /// array in `Array.sum (Array.map double xs)`, borrows a value that lives
    /// until the call returns. The call must apply all of its parameters at
    /// once and return a value that holds no loan; otherwise the ownership
    /// check asks for a `let`.
    pub(super) fn temporary_borrows(
        &self,
        callee: &TypedExpr,
        arguments: &mut [TypedExpr],
        result: &Type,
        span: Span,
    ) -> Result<(), Diagnostic> {
        let borrowed: Vec<_> = arguments
            .iter()
            .enumerate()
            .filter_map(|(index, argument)| {
                if let TypedExprKind::Borrow(value, false) = &argument.kind
                    && !is_place(value)
                {
                    Some(index)
                } else {
                    None
                }
            })
            .collect();
        if borrowed.is_empty() {
            return Ok(());
        }
        let single_stage = match &callee.kind {
            TypedExprKind::Function(FunctionRef::User(id))
            | TypedExprKind::GenericFunction(id, _) => {
                self.signatures[*id].signature.parameters.len() == arguments.len()
            }
            TypedExprKind::Function(FunctionRef::Builtin(instance)) => {
                instance.builtin.scheme().parameters.len() == arguments.len()
            }
            _ => arguments.len() == 1,
        };
        if !single_stage || !self.loan_free(result) {
            let constant = borrowed.iter().any(|index| {
                matches!(&arguments[*index].kind, TypedExprKind::Borrow(value, _)
                    if constant_id(value).is_some_and(|id| self.names.constants.contains(&id)))
            });
            if !constant {
                return Ok(());
            }
            return Err(Diagnostic::new(
                "E1013",
                "a temporary const borrow cannot escape or cross application stages; bind the constant with let first",
                span,
            ));
        }
        for index in borrowed {
            if let TypedExprKind::Borrow(value, _) =
                std::mem::replace(&mut arguments[index].kind, TypedExprKind::Error)
            {
                arguments[index].kind = TypedExprKind::BorrowOperand(value);
            }
        }
        Ok(())
    }

    /// Whether a value of type `ty` surely holds no loan. A part that is still
    /// open, other than a number literal's type, could become a reference.
    fn loan_free(&self, ty: &Type) -> bool {
        fn open(checker: &Checker<'_>, ty: &Type) -> bool {
            match ty {
                Type::Infer(_) => !checker.inference.is_numeric_literal(ty),
                Type::Partial(_) | Type::Application(..) => true,
                Type::Array(ty)
                | Type::List(ty)
                | Type::Vec(ty)
                | Type::FixedArray(ty, _)
                | Type::Task(ty)
                | Type::Reference(ty, _) => open(checker, ty),
                Type::Function(parameters, result) => {
                    parameters.iter().any(|ty| open(checker, ty)) || open(checker, result)
                }
                Type::Tuple(types) => types.iter().any(|ty| open(checker, ty)),
                Type::Record(_, types) | Type::Union(_, types) => {
                    types.iter().any(|ty| open(checker, ty))
                }
                _ => false,
            }
        }
        let ty = self.inference.resolve(ty);
        !open(self, &ty) && !ty.carries_loans(&self.types)
    }
}

/// Whether a borrow of `expression` names storage that outlives the call: a
/// local, a dereference, or a part of one.
fn is_place(expression: &TypedExpr) -> bool {
    match &expression.kind {
        TypedExprKind::Local(_) | TypedExprKind::Dereference(_) => true,
        TypedExprKind::Field(value, _)
        | TypedExprKind::ListTail(value, _)
        | TypedExprKind::UnionPayload { value, .. } => is_place(value),
        TypedExprKind::Index(value, _) => {
            matches!(
                value.ty,
                Type::Array(_) | Type::List(_) | Type::Vec(_) | Type::FixedArray(..)
            ) && is_place(value)
        }
        _ => false,
    }
}

fn charge(value: &TypedExpr, remaining: &mut usize, depth: usize) -> Result<(), Diagnostic> {
    if depth > MAX_NESTING || *remaining == 0 {
        return Err(limit(value.span));
    }
    *remaining -= 1;
    for child in value.children() {
        charge(child, remaining, depth + 1)?;
    }
    Ok(())
}

fn inline(
    expression: &mut TypedExpr,
    values: &BTreeMap<usize, TypedExpr>,
    remaining: &mut usize,
    depth: usize,
) -> Result<(), Diagnostic> {
    if let Some(value) = constant_id(expression).and_then(|id| values.get(&id)) {
        charge(value, remaining, depth)?;
        expression.kind = value.kind.clone();
    } else {
        for child in expression.children_mut() {
            inline(child, values, remaining, depth + 1)?;
        }
    }
    Ok(())
}

struct Evaluator<'a> {
    functions: &'a [CheckedFunction],
    constants: &'a BTreeSet<usize>,
    values: BTreeMap<usize, TypedExpr>,
    active: BTreeSet<usize>,
    references: usize,
    remaining: usize,
}

impl Evaluator<'_> {
    fn run(&mut self) -> Result<(), Diagnostic> {
        for &root in self.constants {
            self.references = 0;
            let mut pending = vec![(root, false, 0, self.functions[root].span)];
            while let Some((id, finishing, depth, span)) = pending.pop() {
                if self.values.contains_key(&id) {
                    continue;
                }
                if finishing {
                    let value = self.evaluate(&self.functions[id].body, 0)?;
                    charge(&value, &mut self.remaining, 0)?;
                    self.active.remove(&id);
                    self.values.insert(id, value);
                    continue;
                }
                self.references += 1;
                if depth > MAX_NESTING || self.references > 1024 {
                    return Err(limit(span));
                }
                if !self.active.insert(id) {
                    return Err(failure(
                        span,
                        "cyclic const definition; break the cycle or make one value a function",
                    ));
                }
                pending.push((id, true, depth, span));
                let mut expressions = vec![&self.functions[id].body];
                while let Some(expression) = expressions.pop() {
                    if let Some(dependency) =
                        constant_id(expression).filter(|id| self.constants.contains(id))
                    {
                        pending.push((dependency, false, depth + 1, expression.span));
                    } else {
                        expressions.extend(expression.children());
                    }
                }
            }
        }
        Ok(())
    }

    fn constant(&mut self, id: usize, span: Span, depth: usize) -> Result<TypedExpr, Diagnostic> {
        if let Some(value) = self.values.get(&id) {
            charge(value, &mut self.remaining, depth)?;
            return Ok(value.clone());
        }
        Err(failure(
            span,
            "cyclic const definition; break the cycle or make one value a function",
        ))
    }

    fn evaluate(&mut self, expression: &TypedExpr, depth: usize) -> Result<TypedExpr, Diagnostic> {
        use TypedExprKind as Expr;
        let span = expression.span;
        if depth > MAX_NESTING || self.remaining == 0 {
            return Err(limit(span));
        }
        self.remaining -= 1;
        if let Some(id) = constant_id(expression).filter(|id| self.constants.contains(id)) {
            return self.constant(id, span, depth);
        }
        let kind = match &expression.kind {
            Expr::Int(_) | Expr::Float(_) | Expr::String(_) | Expr::Bool(_) | Expr::Unit => {
                expression.kind.clone()
            }
            Expr::Array(values) | Expr::Tuple(values) => {
                let values = values
                    .iter()
                    .map(|value| self.evaluate(value, depth + 1))
                    .collect::<Result<_, _>>()?;
                if matches!(expression.kind, Expr::Array(_)) {
                    Expr::Array(values)
                } else {
                    Expr::Tuple(values)
                }
            }
            Expr::Record(fields) => Expr::Record(
                fields
                    .iter()
                    .map(|(index, value)| Ok((*index, self.evaluate(value, depth + 1)?)))
                    .collect::<Result<_, Diagnostic>>()?,
            ),
            Expr::Unary(operator, value) => {
                let value = self.evaluate(value, depth + 1)?;
                unary(*operator, &value, span)?
            }
            Expr::Binary(operator, left, right) => {
                let left = self.evaluate(left, depth + 1)?;
                if matches!(
                    (operator, &left.kind),
                    (BinaryOp::And, Expr::Bool(false)) | (BinaryOp::Or, Expr::Bool(true))
                ) {
                    left.kind
                } else {
                    let right = self.evaluate(right, depth + 1)?;
                    binary(*operator, &left, &right, span)?
                }
            }
            Expr::If {
                condition,
                then_branch,
                else_branch,
            } => {
                let condition = self.evaluate(condition, depth + 1)?;
                let Expr::Bool(condition) = condition.kind else {
                    return Err(failure(span, "a const condition must be bool"));
                };
                self.evaluate(if condition { then_branch } else { else_branch }, depth + 1)?
                    .kind
            }
            Expr::Block { bindings, result } if bindings.is_empty() => {
                self.evaluate(result, depth + 1)?.kind
            }
            Expr::Cast(value) => {
                let value = self.evaluate(value, depth + 1)?;
                cast(&value, &expression.ty, span)?
            }
            _ => {
                return Err(failure(
                    span,
                    "this operation is not allowed in a phase-1 const; use a runtime let",
                ));
            }
        };
        Ok(TypedExpr {
            kind,
            ty: expression.ty.clone(),
            span,
        })
    }
}

fn mask(bits: u16) -> u128 {
    u128::MAX >> (128 - bits)
}

fn signed(value: u128, bits: u16) -> i128 {
    ((value << (128 - bits)) as i128) >> (128 - bits)
}

fn unary(operator: UnaryOp, value: &TypedExpr, span: Span) -> Result<TypedExprKind, Diagnostic> {
    use TypedExprKind as Expr;
    Ok(match (operator, &value.kind, &value.ty) {
        (UnaryOp::Not, Expr::Bool(value), _) => Expr::Bool(!value),
        (UnaryOp::Negate, Expr::Int(value), Type::Integer(bits, _)) => {
            Expr::Int(value.wrapping_neg() & mask(*bits))
        }
        (UnaryOp::BitNot, Expr::Int(value), Type::Integer(bits, _)) => {
            Expr::Int(!value & mask(*bits))
        }
        (UnaryOp::Negate, Expr::Float(value), Type::Binary(32 | 64)) => {
            let bits = u64::from_str_radix(&value[2..], 16).expect("checked float literal");
            Expr::Float(format!("0x{:016X}", bits ^ (1 << 63)))
        }
        (UnaryOp::Negate, Expr::Float(value), Type::Binary(bits) | Type::Decimal(bits)) => {
            Expr::Float(
                (value.parse::<u128>().expect("checked float literal") ^ (1 << (bits - 1)))
                    .to_string(),
            )
        }
        _ => {
            return Err(failure(
                span,
                "this const unary operation is unsupported; use a runtime let",
            ));
        }
    })
}

fn comparison(operator: BinaryOp, order: Option<std::cmp::Ordering>) -> bool {
    use BinaryOp::*;
    use std::cmp::Ordering;
    match operator {
        Equal => order == Some(Ordering::Equal),
        NotEqual => order != Some(Ordering::Equal),
        Less => order == Some(Ordering::Less),
        LessEqual => matches!(order, Some(Ordering::Less | Ordering::Equal)),
        Greater => order == Some(Ordering::Greater),
        GreaterEqual => matches!(order, Some(Ordering::Greater | Ordering::Equal)),
        _ => false,
    }
}

fn binary(
    operator: BinaryOp,
    left: &TypedExpr,
    right: &TypedExpr,
    span: Span,
) -> Result<TypedExprKind, Diagnostic> {
    use BinaryOp::*;
    use TypedExprKind as Expr;
    let comparing = matches!(
        operator,
        Equal | NotEqual | Less | LessEqual | Greater | GreaterEqual
    );
    match (&left.kind, &right.kind, &left.ty) {
        (Expr::Bool(left), Expr::Bool(right), _) => Ok(Expr::Bool(match operator {
            And => *left && *right,
            Or => *left || *right,
            _ => comparison(operator, Some(left.cmp(right))),
        })),
        (Expr::Unit, Expr::Unit, _) if comparing => Ok(Expr::Bool(operator == Equal)),
        (Expr::String(left), Expr::String(right), _) if comparing => {
            Ok(Expr::Bool(comparison(operator, Some(left.cmp(right)))))
        }
        (Expr::Int(left), Expr::Int(right), Type::Char | Type::Utf8Char) if comparing => {
            Ok(Expr::Bool(comparison(operator, Some(left.cmp(right)))))
        }
        (Expr::Int(left), Expr::Int(right), Type::Integer(bits, is_signed)) => {
            if comparing {
                return Ok(Expr::Bool(comparison(
                    operator,
                    Some(if *is_signed {
                        signed(*left, *bits).cmp(&signed(*right, *bits))
                    } else {
                        left.cmp(right)
                    }),
                )));
            }
            if matches!(operator, Divide | Remainder)
                && (*right == 0
                    || (*is_signed && *left == 1 << (bits - 1) && *right == mask(*bits)))
            {
                return Err(failure(
                    span,
                    "const division trapped on zero or signed overflow; use a nonzero divisor without overflow",
                ));
            }
            if operator == Power && *is_signed && signed(*right, *bits) < 0 {
                return Err(failure(
                    span,
                    "const power trapped on a negative exponent; use a nonnegative exponent",
                ));
            }
            let shift = (*right & u128::from(bits - 1)) as u32;
            let result = match operator {
                Add => left.wrapping_add(*right),
                Subtract => left.wrapping_sub(*right),
                Multiply => left.wrapping_mul(*right),
                Power => {
                    // Products modulo 2^128 keep their low `bits` bits exact.
                    let (mut base, mut exponent, mut product) = (*left, *right, 1u128);
                    while exponent != 0 {
                        if exponent & 1 == 1 {
                            product = product.wrapping_mul(base);
                        }
                        exponent >>= 1;
                        base = base.wrapping_mul(base);
                    }
                    product
                }
                Divide if *is_signed => (signed(*left, *bits) / signed(*right, *bits)) as u128,
                Divide => left / right,
                Remainder if *is_signed => (signed(*left, *bits) % signed(*right, *bits)) as u128,
                Remainder => left % right,
                BitAnd => left & right,
                BitOr => left | right,
                BitXor => left ^ right,
                ShiftLeft => left.wrapping_shl(shift),
                ShiftRight if *is_signed => (signed(*left, *bits) >> shift) as u128,
                ShiftRight | ShiftRightUnsigned => left >> shift,
                _ => {
                    return Err(failure(
                        span,
                        "unsupported const integer operation; use a runtime let",
                    ));
                }
            };
            Ok(Expr::Int(result & mask(*bits)))
        }
        (Expr::Float(left), Expr::Float(right), Type::Binary(bits)) => match bits {
            16 => float_binary::<Half>(operator, left, right, span),
            32 => float_binary::<Single>(operator, left, right, span),
            64 => float_binary::<Double>(operator, left, right, span),
            128 => float_binary::<Quad>(operator, left, right, span),
            _ => Err(failure(span, "unsupported binary float width")),
        },
        _ => Err(failure(
            span,
            "this const arithmetic is unsupported; keep decimal arithmetic and overloaded operations in runtime code",
        )),
    }
}

fn float_bits(text: &str, bits: u16) -> u128 {
    match bits {
        32 => {
            let value = Double::from_bits(
                u128::from_str_radix(&text[2..], 16).expect("checked float literal"),
            );
            let value: Single = value.convert(&mut false).value;
            value.to_bits()
        }
        64 => u128::from_str_radix(&text[2..], 16).expect("checked float literal"),
        _ => text.parse().expect("checked float literal"),
    }
}

fn float_text(value: u128, bits: u16) -> String {
    match bits {
        32 => {
            let widened: Double = Single::from_bits(value).convert(&mut false).value;
            format!("0x{:016X}", widened.to_bits())
        }
        64 => format!("0x{value:016X}"),
        _ => value.to_string(),
    }
}

fn float_binary<Binary: Float>(
    operator: BinaryOp,
    left: &str,
    right: &str,
    span: Span,
) -> Result<TypedExprKind, Diagnostic> {
    let left = Binary::from_bits(float_bits(left, Binary::BITS as u16));
    let right = Binary::from_bits(float_bits(right, Binary::BITS as u16));
    let value = match operator {
        BinaryOp::Add => (left + right).value,
        BinaryOp::Subtract => (left - right).value,
        BinaryOp::Multiply => (left * right).value,
        BinaryOp::Divide => (left / right).value,
        BinaryOp::Equal
        | BinaryOp::NotEqual
        | BinaryOp::Less
        | BinaryOp::LessEqual
        | BinaryOp::Greater
        | BinaryOp::GreaterEqual => {
            return Ok(TypedExprKind::Bool(comparison(
                operator,
                left.partial_cmp(&right),
            )));
        }
        _ => {
            return Err(failure(
                span,
                "unsupported const floating-point operation; use a runtime let",
            ));
        }
    };
    Ok(TypedExprKind::Float(float_text(
        value.to_bits(),
        Binary::BITS as u16,
    )))
}

fn exact_quad(text: &str, bits: u16) -> Quad {
    let value = float_bits(text, bits);
    match bits {
        16 => Half::from_bits(value).convert(&mut false).value,
        32 => Single::from_bits(value).convert(&mut false).value,
        64 => Double::from_bits(value).convert(&mut false).value,
        _ => Quad::from_bits(value),
    }
}

fn to_float<Binary: Float>(value: &TypedExpr, span: Span) -> Result<TypedExprKind, Diagnostic>
where
    Quad: FloatConvert<Binary>,
{
    let value = match (&value.kind, &value.ty) {
        (TypedExprKind::Int(value), Type::Integer(bits, true)) => {
            Binary::from_i128(signed(*value, *bits)).value
        }
        (TypedExprKind::Int(value), Type::Integer(_, false)) => Binary::from_u128(*value).value,
        (TypedExprKind::Float(value), Type::Binary(bits)) => {
            exact_quad(value, *bits).convert(&mut false).value
        }
        _ => {
            return Err(failure(
                span,
                "decimal const casts are unsupported; use a runtime conversion",
            ));
        }
    };
    Ok(TypedExprKind::Float(float_text(
        value.to_bits(),
        Binary::BITS as u16,
    )))
}

fn cast(value: &TypedExpr, target: &Type, span: Span) -> Result<TypedExprKind, Diagnostic> {
    if &value.ty == target {
        return Ok(value.kind.clone());
    }
    if let (TypedExprKind::Int(value), Type::Integer(bits, is_signed), Type::Integer(target, _)) =
        (&value.kind, &value.ty, target)
    {
        return Ok(TypedExprKind::Int(
            (if *is_signed {
                signed(*value, *bits) as u128
            } else {
                *value
            }) & mask(*target),
        ));
    }
    if let (TypedExprKind::Float(value), Type::Binary(bits), Type::Integer(target, is_signed)) =
        (&value.kind, &value.ty, target)
    {
        let value = exact_quad(value, *bits);
        let integer = if *is_signed {
            value.to_i128(usize::from(*target)).value as u128
        } else {
            value.to_u128(usize::from(*target)).value
        };
        return Ok(TypedExprKind::Int(integer & mask(*target)));
    }
    match target {
        Type::Binary(16) => return to_float::<Half>(value, span),
        Type::Binary(32) => return to_float::<Single>(value, span),
        Type::Binary(64) => return to_float::<Double>(value, span),
        Type::Binary(128) => return to_float::<Quad>(value, span),
        _ => {}
    }
    Err(failure(
        span,
        "this const cast is unsupported; keep the conversion in runtime code",
    ))
}
