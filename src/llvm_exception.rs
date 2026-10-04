//! Lowering of `try ... with ... finally`, `@checked` arithmetic, and `**`.
//! A raised exception is an `i32` code. It jumps to the handler of the
//! innermost `try` of the same function body, dropping the scopes and
//! temporaries that the jump leaves, and traps when no `try` encloses it.
use super::*;
use crate::check::TypedTry;

/// The code of `OverflowException`: its case index in `Exception.ExceptionKind`.
const OVERFLOW: &str = "0";

impl FunctionEmitter<'_, '_> {
    /// Raises the exception `code` and continues in an unreachable block.
    pub(super) fn raise(&mut self, code: &str) {
        let Some(target) = self.try_targets.last() else {
            self.emit_trap(TrapKind::Overflow);
            self.instruction("unreachable");
            let dead = self.label();
            self.begin(&dead);
            return;
        };
        let handler = target.handler.clone();
        let slot = target.slot.clone();
        let scope_base = target.scope_base;
        let temporary_base = target.temporary_base;
        self.instruction(format!("store i32 {code}, ptr {slot}"));
        let temporaries = self.temporaries.clone();
        for (ty, value, frames) in temporaries[temporary_base..].iter().rev() {
            self.drop_framed(ty, value, frames);
        }
        for scope in self.scopes[scope_base..].to_vec().iter().rev() {
            self.drop_scope(scope);
        }
        self.temporaries = temporaries;
        self.jump(&handler);
        let dead = self.label();
        self.begin(&dead);
    }

    /// Raises `OverflowException` when `overflow` is true.
    fn raise_overflow_if(&mut self, overflow: &str) {
        let failure = self.label();
        let success = self.label();
        self.branch(overflow, &failure, &success);
        self.begin(&failure);
        self.raise(OVERFLOW);
        self.jump(&success);
        self.begin(&success);
    }

    /// `@checked` integer `+ - * **` and negation raise `OverflowException`
    /// instead of wrapping; other operations are unchanged.
    pub(super) fn checked(&mut self, value: &TypedExpr) -> String {
        let Type::Integer(bits, signed) = value.ty else {
            return self.expression(value);
        };
        match &value.kind {
            TypedExprKind::Binary(
                operator @ (BinaryOp::Add | BinaryOp::Subtract | BinaryOp::Multiply),
                left,
                right,
            ) => {
                let left = self.expression(left);
                let right = self.expression(right);
                let operation = match operator {
                    BinaryOp::Add => "add",
                    BinaryOp::Subtract => "sub",
                    _ => "mul",
                };
                let (result, overflow) =
                    self.checked_integer_arithmetic(bits, signed, operation, &left, &right);
                self.raise_overflow_if(&overflow);
                result
            }
            TypedExprKind::Binary(BinaryOp::Power, left, right) => {
                let left = self.expression(left);
                let right = self.expression(right);
                self.power(&value.ty, &left, &right, true)
            }
            TypedExprKind::Unary(UnaryOp::Negate, operand) => {
                let operand = self.expression(operand);
                let (result, overflow) =
                    self.checked_integer_arithmetic(bits, signed, "sub", "0", &operand);
                self.raise_overflow_if(&overflow);
                result
            }
            _ => self.expression(value),
        }
    }

    /// `base ** exponent`: `pow` for `f32` and `f64`, and square-and-multiply
    /// for integers, whose negative exponent traps. Checked products raise
    /// `OverflowException`; unchecked products wrap.
    pub(super) fn power(&mut self, ty: &Type, base: &str, exponent: &str, checked: bool) -> String {
        let llvm = self.ty(ty);
        let Type::Integer(bits, signed) = *ty else {
            let Type::Binary(bits) = *ty else {
                unreachable!("power is checked to integers, f32 and f64")
            };
            return self.value(format!(
                "call {llvm} @tz_math_pow_f{bits}({llvm} {base}, {llvm} {exponent})"
            ));
        };
        if signed {
            let valid = self.value(format!("icmp sge {llvm} {exponent}, 0"));
            self.guard(&valid, TrapKind::NumericRuntime);
        }
        let accumulator = self.spill(ty, "1");
        let factor = self.spill(ty, base);
        let remaining = self.spill(ty, exponent);
        let test = self.label();
        let work = self.label();
        let multiply = self.label();
        let advance = self.label();
        let square = self.label();
        let done = self.label();
        let multiplied = |emitter: &mut Self, left: &str, right: &str| {
            if checked {
                let (product, overflow) =
                    emitter.checked_integer_arithmetic(bits, signed, "mul", left, right);
                emitter.raise_overflow_if(&overflow);
                product
            } else {
                emitter.value(format!("mul {llvm} {left}, {right}"))
            }
        };
        self.jump(&test);
        self.begin(&test);
        let count = self.value(format!("load {llvm}, ptr {remaining}"));
        let more = self.value(format!("icmp ne {llvm} {count}, 0"));
        self.branch(&more, &work, &done);
        self.begin(&work);
        let low = self.value(format!("and {llvm} {count}, 1"));
        let odd = self.value(format!("icmp ne {llvm} {low}, 0"));
        self.branch(&odd, &multiply, &advance);
        self.begin(&multiply);
        let value = self.value(format!("load {llvm}, ptr {accumulator}"));
        let current = self.value(format!("load {llvm}, ptr {factor}"));
        let product = multiplied(self, &value, &current);
        self.instruction(format!("store {llvm} {product}, ptr {accumulator}"));
        self.jump(&advance);
        self.begin(&advance);
        let count = self.value(format!("lshr {llvm} {count}, 1"));
        self.instruction(format!("store {llvm} {count}, ptr {remaining}"));
        let more = self.value(format!("icmp ne {llvm} {count}, 0"));
        // The last factor is never squared, so only products that the result uses can overflow.
        self.branch(&more, &square, &done);
        self.begin(&square);
        let current = self.value(format!("load {llvm}, ptr {factor}"));
        let product = multiplied(self, &current, &current);
        self.instruction(format!("store {llvm} {product}, ptr {factor}"));
        self.jump(&test);
        self.begin(&done);
        self.value(format!("load {llvm}, ptr {accumulator}"))
    }

    /// `try body with arms [finally cleanup]`: `Ok` of the body, or `Error` of
    /// the handler arm that matched the raised exception. Every path, including
    /// an exception that leaves the handler, runs `finally` first.
    pub(super) fn try_expression(&mut self, handled: &TypedTry, ty: &Type) -> String {
        let Type::Union(id, _) = ty else {
            unreachable!("try is checked to Result")
        };
        let case = |name: &str| {
            self.module.unions[*id]
                .cases
                .iter()
                .position(|(case, _)| case == name)
                .expect("Result has Ok and Error")
        };
        let (ok, error) = (case("Ok"), case("Error"));
        let llvm = self.ty(ty);
        let code = self.slot(&Type::Integer(32, true));
        let result = self.slot(ty);
        let handler = self.label();
        let finish = self.label();
        // The body's and the handler's values move into `result`, so a later
        // raise must not drop them; they are also defined on different paths.
        let temporary_base = self.temporaries.len();
        let escape = handled.finally.as_ref().map(|_| {
            let flag = self.slot(&Type::Bool);
            self.instruction(format!("store i1 0, ptr {flag}"));
            (self.label(), self.slot(&Type::Integer(32, true)), flag)
        });
        self.try_targets.push(TryTarget {
            handler: handler.clone(),
            slot: code.clone(),
            scope_base: self.scopes.len(),
            temporary_base: self.temporaries.len(),
        });
        let value = self.expression(&handled.body);
        self.try_targets.pop();
        let body = self.ty(&handled.body.ty);
        let value = self.construct_value(ty, ok, Some((value, body)));
        self.temporaries.truncate(temporary_base);
        self.instruction(format!("store {llvm} {value}, ptr {result}"));
        self.jump(&finish);
        self.begin(&handler);
        self.caught.push(code);
        if let Some((label, slot, _)) = &escape {
            self.try_targets.push(TryTarget {
                handler: label.clone(),
                slot: slot.clone(),
                scope_base: self.scopes.len(),
                temporary_base: self.temporaries.len(),
            });
        }
        let caught = self.expression(&handled.handler);
        if escape.is_some() {
            self.try_targets.pop();
        }
        self.caught.pop();
        let payload = self.ty(&handled.handler.ty);
        let value = self.construct_value(ty, error, Some((caught, payload)));
        self.temporaries.truncate(temporary_base);
        self.instruction(format!("store {llvm} {value}, ptr {result}"));
        self.jump(&finish);
        if let (Some((label, slot, flag)), Some(finally)) = (&escape, &handled.finally) {
            self.begin(label);
            self.instruction(format!("store i1 1, ptr {flag}"));
            self.jump(&finish);
            self.begin(&finish);
            self.expression(finally);
            self.temporaries.truncate(temporary_base);
            let escaping = self.value(format!("load i1, ptr {flag}"));
            let rethrow = self.label();
            let done = self.label();
            self.branch(&escaping, &rethrow, &done);
            self.begin(&rethrow);
            let code = self.value(format!("load i32, ptr {slot}"));
            self.raise(&code);
            self.jump(&done);
            self.begin(&done);
        } else {
            self.begin(&finish);
        }
        self.value(format!("load {llvm}, ptr {result}"))
    }
}
