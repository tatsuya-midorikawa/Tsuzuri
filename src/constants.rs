use super::*;
use rustc_apfloat::ieee::{Double, Half, Quad, Single};
use rustc_apfloat::{Float, FloatConvert};
use std::rc::Rc;

#[path = "const_machine.rs"]
mod machine;
#[path = "const_rules.rs"]
mod rules;
#[path = "const_static.rs"]
mod statics;

use machine::{Machine, Value};
pub(super) use rules::check_signature;

pub(super) fn failure(kind: DiagnosticKind, span: Span, message: impl Into<String>) -> Diagnostic {
    Diagnostic::of(kind, message, span)
}

fn limit(span: Span) -> Diagnostic {
    failure(
        DiagnosticKind::ConstLimitExceeded,
        span,
        "const evaluation exceeds the compiler limit; split or simplify the constants",
    )
}

/// The name that a call's callee spells: the function itself, or the member of a module path.
fn callee_name(callee: &Expr) -> Option<&str> {
    match &callee.kind {
        ExprKind::Name(name) | ExprKind::QualifiedFunction(name) => {
            Some(name.text.rsplit(['.', ':']).next().unwrap_or(&name.text))
        }
        ExprKind::Field(_, name) => Some(&name.text),
        _ => None,
    }
}

/// Checks the initializer of a constant as written: a call is allowed only when it names a
/// `const def` (D11); whether the name resolves to one is decided after type checking.
pub(super) fn validate(
    expression: &Expr,
    const_names: &BTreeSet<String>,
) -> Result<(), Diagnostic> {
    use ExprKind as Expr;
    match &expression.kind {
        Expr::Integer(..)
        | Expr::Float(..)
        | Expr::String(_)
        | Expr::Char(_)
        | Expr::Utf8Char(_)
        | Expr::Bool(_)
        | Expr::Unit
        | Expr::Name(_)
        // A typed hole reaches the checker, which reports it with the constant's type (G25).
        | Expr::Hole(_) => Ok(()),
        Expr::Unary(_, value) | Expr::Cast(value, _) | Expr::Field(value, _) => {
            validate(value, const_names)
        }
        Expr::Binary(operator, left, right) if *operator != BinaryOp::Pipe => {
            validate(left, const_names)?;
            validate(right, const_names)
        }
        Expr::If {
            condition,
            then_branch,
            else_branch,
        } => {
            validate(condition, const_names)?;
            validate(then_branch, const_names)?;
            validate(else_branch, const_names)
        }
        Expr::Array(values) | Expr::Tuple(values) => values
            .iter()
            .try_for_each(|value| validate(value, const_names)),
        Expr::Record { fields, .. } => fields
            .iter()
            .try_for_each(|(_, value)| validate(value, const_names)),
        Expr::Block { bindings, result } if bindings.is_empty() => validate(result, const_names),
        Expr::Call(callee, arguments)
            if callee_name(callee).is_some_and(|name| const_names.contains(name)) =>
        {
            arguments
                .iter()
                .try_for_each(|argument| validate(argument, const_names))
        }
        _ => Err(failure(
            DiagnosticKind::ConstExpressionNotAllowed,
            expression.span,
            "this expression is not allowed in a phase-1 const; precompute it or use a runtime let",
        )),
    }
}

/// What the body of a function names, in the order a walk of it meets the names: a constant, with
/// the span of its use, or a `const def`.
#[derive(Clone, Copy)]
enum Named {
    Constant(usize, Span),
    ConstDef(usize),
}

/// The names in the body of `function`, not following the `const def`s it names: a constant that
/// the body calls is not walked into (its own initializer is a node of the dependency graph), and a
/// `const def` is a name of the body, expanded where it is first met by [`ConstDefWalk`].
fn named_in(
    functions: &[CheckedFunction],
    constants: &BTreeSet<usize>,
    const_functions: &BTreeSet<usize>,
    function: usize,
) -> Vec<Named> {
    let mut names = Vec::new();
    let mut expressions = vec![&functions[function].body];
    while let Some(expression) = expressions.pop() {
        if let Some(dependency) = constant_id(expression).filter(|id| constants.contains(id)) {
            names.push(Named::Constant(dependency, expression.span));
            continue;
        }
        if let TypedExprKind::Function(FunctionRef::User(callee)) = expression.kind
            && const_functions.contains(&callee)
        {
            names.push(Named::ConstDef(callee));
        }
        expression
            .try_for_each_child(&mut |child| {
                expressions.push(child);
                Ok(())
            })
            .expect("collecting dependencies cannot fail");
    }
    names
}

/// The names that the initializer of one constant reaches, directly and through the `const def`s
/// it names, with each `const def` expanded once, where it is first met: the order in which a walk
/// of the initializer that goes into each `const def` as it meets it would find them. The names of
/// one function are walked once, however many constants reach it, and the walk keeps no Rust stack
/// of its own depth.
struct ConstDefWalk<'a> {
    functions: &'a [CheckedFunction],
    constants: &'a BTreeSet<usize>,
    const_functions: &'a BTreeSet<usize>,
    names: BTreeMap<usize, Rc<Vec<Named>>>,
}

impl<'a> ConstDefWalk<'a> {
    fn new(
        functions: &'a [CheckedFunction],
        constants: &'a BTreeSet<usize>,
        const_functions: &'a BTreeSet<usize>,
    ) -> Self {
        Self {
            functions,
            constants,
            const_functions,
            names: BTreeMap::new(),
        }
    }

    fn names_of(&mut self, function: usize) -> Rc<Vec<Named>> {
        if let Some(names) = self.names.get(&function) {
            return Rc::clone(names);
        }
        let names = Rc::new(named_in(
            self.functions,
            self.constants,
            self.const_functions,
            function,
        ));
        self.names.insert(function, Rc::clone(&names));
        names
    }

    /// Calls `visit` with each name that the body of `root` reaches, in order; `visit` gets the
    /// `const def` that was expanded to find a name too, so that it can see the edge.
    fn walk(&mut self, root: usize, mut visit: impl FnMut(Named)) {
        let mut scanned = BTreeSet::new();
        let mut frames = vec![(self.names_of(root), 0)];
        while let Some((names, index)) = frames.pop() {
            let Some(&named) = names.get(index) else {
                continue;
            };
            frames.push((names, index + 1));
            match named {
                Named::Constant(..) => visit(named),
                Named::ConstDef(callee) => {
                    if scanned.insert(callee) {
                        visit(named);
                        frames.push((self.names_of(callee), 0));
                    }
                }
            }
        }
    }
}

/// The `const def`s that each constant's initializer reaches, directly or through other `const
/// def`s, as (constant, `const def`) pairs. `fold` replaces the initializers by their values, so
/// the unused-private-function warning takes these edges from here.
pub(super) fn const_def_uses(
    functions: &[CheckedFunction],
    constants: &BTreeSet<usize>,
    const_functions: &BTreeSet<usize>,
) -> Vec<(usize, usize)> {
    let mut uses = Vec::new();
    let mut walk = ConstDefWalk::new(functions, constants, const_functions);
    for &constant in constants {
        walk.walk(constant, |named| {
            if let Named::ConstDef(callee) = named {
                uses.push((constant, callee));
            }
        });
    }
    uses
}

/// Evaluates every constant, calls of `const def`s included (D11), and expands the references to
/// them with their values. The bodies of the `const def`s are checked first, so that one that no
/// constant calls is judged too. A constant of plain data that a use reads in place or borrows is
/// not expanded there; the use names its static data, which the result holds (D11 Phase 2).
pub(super) fn fold(
    functions: &mut [CheckedFunction],
    constants: &BTreeSet<usize>,
    const_functions: &BTreeSet<usize>,
    types: &TypeContext<'_>,
) -> Result<BTreeMap<usize, StaticData>, Diagnostic> {
    for &id in const_functions {
        rules::validate_function(id, functions, constants, const_functions, types)?;
    }
    let mut evaluator = Evaluator {
        functions,
        constants,
        const_functions,
        types: *types,
        walk: ConstDefWalk::new(functions, constants, const_functions),
        values: BTreeMap::new(),
        active: BTreeSet::new(),
        references: 0,
        remaining: 1 << 20,
    };
    evaluator.run()?;
    let values = evaluator.values;
    let eligible: BTreeSet<usize> = constants
        .iter()
        .copied()
        .filter(|&id| statics::eligible(&functions[id].signature.result, types))
        .collect();
    let mut used = BTreeSet::new();
    for (id, function) in functions.iter_mut().enumerate() {
        let mut remaining = 1 << 20;
        if constants.contains(&id) {
            inline(&mut function.body, &values, &mut remaining, 0)?;
            function.body = values[&id].expr.clone();
            function.visibility = Visibility::Private;
        } else {
            statics::mark_uses(&mut function.body, &eligible, &mut used);
            inline(&mut function.body, &values, &mut remaining, 0)?;
        }
    }
    statics::collect(&used, &values, types).map_err(|id| statics::too_big(functions[id].span))
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
        // Pure qualification casts do not change the lifetime of the borrowed operand.
        fn operand_kind(argument: &mut TypedExpr) -> &mut TypedExprKind {
            let mut kind = &mut argument.kind;
            while matches!(&*kind, TypedExprKind::Cast(value) if value.ty.function_purity().is_some())
            {
                let TypedExprKind::Cast(value) = kind else {
                    unreachable!("callable cast checked")
                };
                kind = &mut value.kind;
            }
            kind
        }
        fn make_operand(argument: &mut TypedExpr) {
            argument.kind = match std::mem::replace(&mut argument.kind, TypedExprKind::Error) {
                TypedExprKind::Borrow(value, false) => TypedExprKind::BorrowOperand(value),
                TypedExprKind::Cast(mut value) => {
                    make_operand(&mut value);
                    let TypedExprKind::BorrowOperand(operand) = value.kind else {
                        unreachable!("temporary borrow checked")
                    };
                    TypedExprKind::BorrowOperand(Box::new(TypedExpr {
                        kind: TypedExprKind::Cast(operand),
                        ty: argument.ty.dereferenced().expect("borrowed callable"),
                        span: argument.span,
                    }))
                }
                _ => unreachable!("temporary shared borrow checked"),
            };
        }
        let borrowed: Vec<_> = arguments
            .iter_mut()
            .enumerate()
            .filter_map(|(index, argument)| {
                if let TypedExprKind::Borrow(value, false) = operand_kind(argument)
                    && !is_place(value)
                {
                    Some(index)
                } else {
                    None
                }
            })
            .collect();
        // A constant of plain data is borrowed as static data, which `fold` rewrites, wherever
        // the call is applied and whatever it returns (D11 Phase 2).
        let (plain, borrowed): (Vec<_>, Vec<_>) = borrowed.into_iter().partition(|index| {
            matches!(operand_kind(&mut arguments[*index]), TypedExprKind::Borrow(value, _)
                if self.plain_constant(value))
        });
        for index in plain {
            make_operand(&mut arguments[index]);
        }
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
            // A class method (`Text.find "a" text`): the implementation that the instance chose takes its
            // arguments in stages of its own, as a user function does (`fn pick text = { ...; \n -> ... }`
            // runs its first stage before the second argument is evaluated), so the call applies them at once
            // only when that implementation names every argument. While the instance is unknown (an open type,
            // a type parameter of a generic body, a builtin instance) the rule of other callees applies.
            TypedExprKind::Method(class, method, ty) => self
                .classes
                .resolved_method(*class, *method, &self.inference.resolve(ty), &self.types)
                .map_or(arguments.len() == 1, |(function, _)| {
                    self.signatures[function].signature.parameters.len() == arguments.len()
                }),
            _ => arguments.len() == 1,
        };
        if !single_stage || !self.loan_free(result) {
            let constant = borrowed.iter().any(|index| {
                matches!(operand_kind(&mut arguments[*index]), TypedExprKind::Borrow(value, _)
                    if constant_id(value).is_some_and(|id| self.names.constants.contains(&id)))
            });
            if !constant {
                return Ok(());
            }
            return Err(Diagnostic::of(
                DiagnosticKind::ConstTemporaryBorrowEscapes,
                "a temporary const borrow cannot escape or cross application stages; bind the constant with let first",
                span,
            ));
        }
        for index in borrowed {
            make_operand(&mut arguments[index]);
        }
        Ok(())
    }

    /// The temporary borrows that `temporary_borrows` left alone because a type was still open when the call was
    /// checked, as in `let both = Array.sort >> Array.rev`: the composition applies `Array.rev` to the array that
    /// `Array.sort` returns before the element type is known. Once the function is finished every type is known, and
    /// the same rule settles them. A borrow that the rule still declines stays a borrow, and the ownership check
    /// rejects it as before, so this only accepts programs that were rejected for want of a type.
    pub(super) fn settle_temporary_borrows(
        &self,
        expression: &mut TypedExpr,
    ) -> Result<(), Diagnostic> {
        let span = expression.span;
        let result = expression.ty.clone();
        match &mut expression.kind {
            TypedExprKind::Call(callee, arguments) => {
                self.temporary_borrows(callee, arguments, &result, span)
            }
            TypedExprKind::Binary(BinaryOp::Pipe, argument, callee) => {
                self.temporary_borrows(callee, std::slice::from_mut(&mut **argument), &result, span)
            }
            _ => Ok(()),
        }
    }

    /// Whether `value` names a constant of plain data, which a borrow takes as static data.
    pub(super) fn plain_constant(&self, value: &TypedExpr) -> bool {
        constant_id(value)
            .filter(|id| self.names.constants.contains(id))
            .is_some_and(|id| statics::eligible(&self.signatures[id].signature.result, &self.types))
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
                | Type::Shared(ty, _)
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

/// Pays for one more copy of `value` whose root sits at `depth` of a body: its nodes come out of
/// the budget, and its tree may not nest deeper than the compiler allows.
fn charge(
    value: &Value,
    remaining: &mut usize,
    depth: usize,
    span: Span,
) -> Result<(), Diagnostic> {
    if depth + value.depth - 1 > MAX_NESTING || *remaining < value.size {
        return Err(limit(span));
    }
    *remaining -= value.size;
    Ok(())
}

fn inline(
    expression: &mut TypedExpr,
    values: &BTreeMap<usize, Value>,
    remaining: &mut usize,
    depth: usize,
) -> Result<(), Diagnostic> {
    if let Some(value) = constant_id(expression).and_then(|id| values.get(&id)) {
        charge(value, remaining, depth, expression.span)?;
        expression.kind = value.expr.kind.clone();
    } else {
        expression
            .try_for_each_child_mut(&mut |child| inline(child, values, remaining, depth + 1))?;
    }
    Ok(())
}

struct Evaluator<'a> {
    functions: &'a [CheckedFunction],
    constants: &'a BTreeSet<usize>,
    const_functions: &'a BTreeSet<usize>,
    types: TypeContext<'a>,
    walk: ConstDefWalk<'a>,
    values: BTreeMap<usize, Value>,
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
                    let value = self.evaluate(id)?;
                    charge(&value, &mut self.remaining, 0, value.expr.span)?;
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
                        DiagnosticKind::ConstCycle,
                        span,
                        "cyclic const definition; break the cycle or make one value a function",
                    ));
                }
                pending.push((id, true, depth, span));
                // The constants that the initializer names, and those that the `const def`s it
                // calls, or passes by name as in `new [T](n, f)`, name in turn.
                self.walk.walk(id, |name| {
                    if let Named::Constant(dependency, span) = name {
                        pending.push((dependency, false, depth + 1, span));
                    }
                });
            }
        }
        Ok(())
    }

    fn evaluate(&mut self, id: usize) -> Result<Value, Diagnostic> {
        let mut machine = Machine {
            functions: self.functions,
            constants: self.constants,
            const_functions: self.const_functions,
            values: &self.values,
            types: self.types,
            remaining: &mut self.remaining,
        };
        machine.run(&self.functions[id].body)
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
        (UnaryOp::Plus, Expr::Int(_) | Expr::Float(_), Type::Integer(..) | Type::Binary(_)) => {
            value.kind.clone()
        }
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
                DiagnosticKind::ConstUnsupportedOperation,
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
                    DiagnosticKind::ConstEvaluationTrapped,
                    span,
                    "const division trapped on zero or signed overflow; use a nonzero divisor without overflow",
                ));
            }
            if operator == Power && *is_signed && signed(*right, *bits) < 0 {
                return Err(failure(
                    DiagnosticKind::ConstEvaluationTrapped,
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
                        DiagnosticKind::ConstUnsupportedOperation,
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
            _ => Err(failure(
                DiagnosticKind::ConstUnsupportedOperation,
                span,
                "unsupported binary float width",
            )),
        },
        _ => Err(failure(
            DiagnosticKind::ConstUnsupportedOperation,
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
                DiagnosticKind::ConstUnsupportedOperation,
                span,
                "unsupported const floating-point operation; use a runtime let",
            ));
        }
    };
    // The 16- and 128-bit arithmetic of the program (`tz_soft_op` in numeric.c) answers an invalid
    // operation or a NaN operand with the canonical quiet NaN, signed by the left operand for `+` and
    // `-` and by the product of the signs for `*` and `/`. APFloat keeps the operand's own NaN or
    // makes a positive one, which `Math.copysign` could tell apart, so a constant takes the rule of
    // the program. `f32` and `f64` use the machine's instructions, whose NaN signs differ between
    // machines, so a constant of those types is only specified up to NaN.
    let value = if matches!(Binary::BITS, 16 | 128) && value.is_nan() {
        let negative = match operator {
            BinaryOp::Add | BinaryOp::Subtract => left.is_negative(),
            _ => left.is_negative() != right.is_negative(),
        };
        if negative {
            -Binary::qnan(None)
        } else {
            Binary::qnan(None)
        }
    } else {
        value
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
                DiagnosticKind::ConstUnsupportedOperation,
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
        DiagnosticKind::ConstUnsupportedOperation,
        span,
        "this const cast is unsupported; keep the conversion in runtime code",
    ))
}
