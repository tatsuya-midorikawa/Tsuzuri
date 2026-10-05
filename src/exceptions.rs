//! Type checking of `try ... with ... finally`, unary `+`, and bigint literals.
//! `@checked` marks arithmetic in `Checker::binary_expression` and negation in
//! `value_expression`; a checked overflow raises an exception that the
//! innermost `try` of the same function body catches (`llvm` lowers both).
use super::*;

impl Checker<'_> {
    /// The expression forms of `_specs` kept out of `value_expression`'s frame:
    /// bigint literals, `@checked`, `try`, and unary `+`.
    #[inline(never)]
    pub(super) fn spec_expression(
        &mut self,
        expression: &Expr,
        expected: Option<&Type>,
    ) -> Result<TypedExpr, Diagnostic> {
        match &expression.kind {
            ExprKind::BigInt(digits) => {
                self.bigint_literal(digits, false, expected, expression.span)
            }
            ExprKind::Unary(UnaryOp::Negate, operand) => {
                let ExprKind::BigInt(digits) = &operand.kind else {
                    unreachable!("negated bigint literals only")
                };
                self.bigint_literal(digits, true, expected, expression.span)
            }
            ExprKind::Checked(value) => {
                let outer = std::mem::replace(&mut self.checked_arithmetic, true);
                let checked = self.expression(value, expected);
                self.checked_arithmetic = outer;
                checked
            }
            ExprKind::Try(handled) => self.try_expression(handled, expected, expression.span),
            ExprKind::Unary(UnaryOp::Plus, operand) => {
                self.unary_plus(operand, expected, expression.span)
            }
            _ => unreachable!("spec expression forms only"),
        }
    }

    /// `kind` checked for overflow under `@checked`.
    #[inline(never)]
    pub(super) fn checked_kind(
        kind: TypedExprKind,
        ty: &Type,
        span: Span,
    ) -> (TypedExprKind, Type) {
        (
            TypedExprKind::Checked(Box::new(TypedExpr {
                kind,
                ty: ty.clone(),
                span,
            })),
            ty.clone(),
        )
    }
    /// `+value`: a number unchanged; the operand must be numeric.
    pub(super) fn unary_plus(
        &mut self,
        operand: &Expr,
        expected: Option<&Type>,
        span: Span,
    ) -> Result<TypedExpr, Diagnostic> {
        let mut value = self.expression(operand, expected)?;
        if !self.is_bigint(&value.ty) {
            self.require("Numeric", value.ty.clone(), span)?;
        }
        value.span = span;
        Ok(value)
    }

    /// Whether `ty` is the standard `bigint` (the record `BigInt`).
    pub(super) fn is_bigint(&self, ty: &Type) -> bool {
        matches!(self.inference.resolve(ty), Type::Record(id, _)
            if self.names.records.get("BigInt.BigInt").is_some_and(|info| info.id == id))
    }

    /// `try body with arms [finally cleanup]` is `Result<T, E>`: `Ok` of the body's
    /// value, or `Error` of the first arm that matches the caught exception. An
    /// exception that no arm matches runs `finally` and propagates to the next `try`.
    #[inline(never)]
    pub(super) fn try_expression(
        &mut self,
        handled: &TryExpr,
        expected: Option<&Type>,
        span: Span,
    ) -> Result<TypedExpr, Diagnostic> {
        let expected = expected.map(|ty| self.inference.resolve(ty));
        let result_union = self.names.unions.get("Result.Result").map(|info| info.id);
        let (ok_hint, error_hint) = match (&expected, result_union) {
            (Some(Type::Union(id, args)), Some(result)) if *id == result && args.len() == 2 => {
                (Some(args[0].clone()), Some(args[1].clone()))
            }
            _ => (None, None),
        };
        // `break` and `continue` cannot leave a `try` whose `finally` must run first.
        let outer_loop = if handled.finally.is_some() {
            std::mem::take(&mut self.normal_loop_depth)
        } else {
            self.normal_loop_depth
        };
        let checked = self.try_parts(handled, ok_hint.as_ref(), error_hint.as_ref(), span);
        self.normal_loop_depth = outer_loop;
        let (body, handler) = checked?;
        let finally = handled
            .finally
            .as_ref()
            .map(|finally| self.expression(finally, Some(&Type::Unit)))
            .transpose()?;
        let error = self.inference.resolve(&handler.ty);
        self.require("Err", error.clone(), span)?;
        let ty = self.names.std_type(
            "Result",
            "Result",
            vec![body.ty.clone(), error].into(),
            span,
        )?;
        self.finish_expression(
            TypedExprKind::Try(Box::new(TypedTry {
                body,
                handler,
                finally,
            })),
            ty,
            expected.as_ref(),
            span,
        )
    }

    fn try_parts(
        &mut self,
        handled: &TryExpr,
        ok_hint: Option<&Type>,
        error_hint: Option<&Type>,
        span: Span,
    ) -> Result<(TypedExpr, TypedExpr), Diagnostic> {
        let body = self.expression(&handled.body, ok_hint)?;
        let exception = self
            .names
            .std_type("Exception", "Exception", Box::new([]), span)?;
        let make = self
            .names
            .functions
            .get("Exception.of_code")
            .map(|info| info.id)
            .ok_or_else(|| {
                Diagnostic::new(
                    "E1004",
                    "try needs the standard library function Exception.of_code",
                    span,
                )
            })?;
        let (kind, ty) = self.function(make);
        let callee = TypedExpr { kind, ty, span };
        let raised = TypedExpr {
            kind: TypedExprKind::RaisedException,
            ty: Type::Integer(32, true),
            span,
        };
        let caught = TypedExpr {
            kind: Self::call_kind(callee, vec![raised]),
            ty: exception,
            span,
        };
        let mut handler = self.match_value(caught, &handled.arms, error_hint, span, false)?;
        // An exception that no arm matches goes on to the next `try`.
        if let TypedExprKind::Match { arms, .. } = &mut handler.kind {
            arms.push(TypedMatchArm {
                alternatives: vec![PatternAlternative {
                    steps: Vec::new(),
                    bindings: Vec::new(),
                }],
                guard: None,
                body: TypedExpr {
                    kind: TypedExprKind::Reraise,
                    ty: handler.ty.clone(),
                    span,
                },
                borrowed: BTreeSet::new(),
            });
        }
        Ok((body, handler))
    }

    /// A bigint literal (`123I`, or an unsuffixed integer whose expected type is
    /// `bigint`) builds the standard `BigInt` from its base-10^9 digits.
    pub(super) fn bigint_literal(
        &mut self,
        digits: &str,
        negative: bool,
        expected: Option<&Type>,
        span: Span,
    ) -> Result<TypedExpr, Diagnostic> {
        let (radix, text) = match digits.get(..2) {
            Some("0x") => (16, &digits[2..]),
            Some("0b") => (2, &digits[2..]),
            _ => (10, digits),
        };
        // Least significant first; zero has no digits.
        let mut limbs: Vec<u64> = Vec::new();
        for character in text.chars() {
            let mut carry = u64::from(character.to_digit(radix).ok_or_else(|| {
                Diagnostic::new("E1009", "invalid digit in a bigint literal", span)
            })?);
            for limb in &mut limbs {
                let value = *limb * u64::from(radix) + carry;
                *limb = value % 1_000_000_000;
                carry = value / 1_000_000_000;
            }
            while carry > 0 {
                limbs.push(carry % 1_000_000_000);
                carry /= 1_000_000_000;
            }
        }
        let make = self
            .names
            .functions
            .get("BigInt.make")
            .map(|info| info.id)
            .ok_or_else(|| {
                Diagnostic::new(
                    "E1004",
                    "bigint literals need the standard library function BigInt.make",
                    span,
                )
            })?;
        let ty = self
            .names
            .std_type("BigInt", "BigInt", Box::default(), span)?;
        let (kind, callee_type) = self.function(make);
        let callee = TypedExpr {
            kind,
            ty: callee_type,
            span,
        };
        let sign = TypedExpr {
            kind: TypedExprKind::Bool(negative && !limbs.is_empty()),
            ty: Type::Bool,
            span,
        };
        let digits = TypedExpr {
            kind: TypedExprKind::Array(
                limbs
                    .iter()
                    .map(|limb| TypedExpr {
                        kind: TypedExprKind::Int(u128::from(*limb)),
                        ty: Type::I64,
                        span,
                    })
                    .collect(),
            ),
            ty: Type::Array(Box::new(Type::I64)),
            span,
        };
        self.finish_expression(
            Self::call_kind(callee, vec![sign, digits]),
            ty,
            expected,
            span,
        )
    }
}
