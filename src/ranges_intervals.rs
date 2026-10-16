//! Intervals of integer expressions (PR02): the values that an expression can take, which decide
//! the overflow flags of `+ - *`. Every answer contains all the values the expression can have,
//! whatever the program does. The whole range of the type is always a correct answer, and it is
//! what anything unknown, too deep or past the step budget gets, so a limit never changes what a
//! program computes, only how much is proved. This is the exception to the rule that a program
//! beyond a limit is an error (D-47): a proof that is missing costs a flag, not a check.

use super::*;
use crate::interval::{self, Flags, Int, Interval, Operation};
use crate::syntax::UnaryOp;

/// Typed nodes that the interval proofs of one function may visit.
pub(super) const STEP_BUDGET: usize = 1 << 20;
/// The nesting of expressions that one answer follows.
const MAX_DEPTH: u32 = 32;
/// How often the comparisons of one condition are applied again to use each other's results.
const MAX_PASSES: usize = 3;

/// The integer type of at most 64 bits that `ty` is.
fn integer(ty: &Type) -> Option<Int> {
    match ty {
        Type::Integer(bits, signed) => Int::new(*bits, *signed),
        _ => None,
    }
}

/// The value of an integer literal, which the checker keeps as the bit pattern of its type.
fn literal(ty: Int, pattern: u128) -> Option<Interval> {
    let modulus = 1_u128 << ty.bits;
    if pattern >= modulus {
        return None;
    }
    let value = pattern as i128;
    let negative = ty.signed && pattern >= modulus >> 1;
    Some(Interval::point(if negative {
        value - modulus as i128
    } else {
        value
    }))
}

fn operation(operator: BinaryOp) -> Option<Operation> {
    match operator {
        BinaryOp::Add => Some(Operation::Add),
        BinaryOp::Subtract => Some(Operation::Subtract),
        BinaryOp::Multiply => Some(Operation::Multiply),
        _ => None,
    }
}

/// The comparison that holds with its operands swapped.
fn mirror(operator: BinaryOp) -> Option<BinaryOp> {
    match operator {
        BinaryOp::Less => Some(BinaryOp::Greater),
        BinaryOp::LessEqual => Some(BinaryOp::GreaterEqual),
        BinaryOp::Greater => Some(BinaryOp::Less),
        BinaryOp::GreaterEqual => Some(BinaryOp::LessEqual),
        BinaryOp::Equal | BinaryOp::NotEqual => Some(operator),
        _ => None,
    }
}

impl RangeFacts {
    /// The values of `expression` where the emitter is now, when it is an integer of at most 64 bits.
    #[cfg(test)]
    pub(crate) fn interval(
        &mut self,
        module: &CheckedModule,
        expression: &TypedExpr,
    ) -> Option<Interval> {
        let ty = integer(&expression.ty)?;
        Some(self.value(module, expression, ty, 0))
    }

    /// The overflow flags that `left operator right` is proved to have. There are none unless it is
    /// an integer `+`, `-` or `*` of at most 64 bits whose operands are both of that type.
    #[allow(dead_code)]
    pub(crate) fn integer_flags(
        &mut self,
        module: &CheckedModule,
        operator: BinaryOp,
        left: &TypedExpr,
        right: &TypedExpr,
    ) -> Flags {
        let Some(operation) = operation(operator) else {
            return Flags::default();
        };
        let Some(ty) = integer(&left.ty).filter(|ty| integer(&right.ty) == Some(*ty)) else {
            return Flags::default();
        };
        let left = self.value(module, left, ty, 0);
        let right = self.value(module, right, ty, 0);
        interval::arithmetic(operation, ty, left, right).flags
    }

    /// Records the values of the immutable integer local that `value` initializes, for the rest of
    /// the function: it is bound once and never changes.
    pub(crate) fn bind(&mut self, module: &CheckedModule, local: &Local, value: &TypedExpr) {
        if let Some(ty) = self.trackable(local) {
            let known = self.value(module, value, ty, 0);
            self.values.insert(local.id, known);
        }
    }

    /// Records the values of the counter of a `for` range loop whose bounds and step are given: it
    /// starts at `start`, moves in the direction of the step and stops at `finish`.
    pub(crate) fn enter_counter(
        &mut self,
        module: &CheckedModule,
        local: &Local,
        start: &TypedExpr,
        step: &TypedExpr,
        finish: &TypedExpr,
    ) {
        let Some(ty) = self.trackable(local) else {
            return;
        };
        let first = self.value(module, start, ty, 0);
        let last = self.value(module, finish, ty, 0);
        let step = self.value(module, step, ty, 0);
        let (low, high) = if !ty.signed || step.low > 0 {
            (first.low, last.high)
        } else if step.high < 0 {
            (last.low, first.high)
        } else {
            (first.low.min(last.low), first.high.max(last.high))
        };
        if low <= high {
            self.values.insert(local.id, Interval::new(low, high));
        }
    }

    /// The integer type of a local that never changes after it is bound, so that what is known of
    /// it holds wherever it is read.
    fn trackable(&self, local: &Local) -> Option<Int> {
        let ty = integer(&local.ty)?;
        self.fixed.contains(&local.id).then_some(ty)
    }

    /// The values that `condition` proves its integer locals have where it holds, and how many
    /// narrowings it pushed. A comparison can use what an earlier one in the same conjunction
    /// proved, whichever comes first, so the terms are applied until nothing changes.
    pub(super) fn refine(
        &mut self,
        module: &CheckedModule,
        condition: &TypedExpr,
        depth: u32,
    ) -> usize {
        let mut comparisons = Vec::new();
        let mut terms = vec![condition];
        while let Some(term) = terms.pop() {
            let TypedExprKind::Binary(operator, left, right) = &term.kind else {
                continue;
            };
            if *operator == BinaryOp::And {
                terms.push(right);
                terms.push(left);
            } else if let Some(ty) = integer(&left.ty).filter(|ty| integer(&right.ty) == Some(*ty))
            {
                comparisons.push((*operator, left.as_ref(), right.as_ref(), ty));
            }
        }
        let mut refined = 0;
        for _ in 0..MAX_PASSES {
            let before = refined;
            for &(operator, left, right, ty) in &comparisons {
                if let TypedExprKind::Local(id) = left.kind {
                    let other = self.value(module, right, ty, depth);
                    refined += self.narrow(id, ty, operator, other);
                }
                if let (TypedExprKind::Local(id), Some(operator)) = (&right.kind, mirror(operator))
                {
                    let other = self.value(module, left, ty, depth);
                    refined += self.narrow(*id, ty, operator, other);
                }
            }
            if refined == before {
                break;
            }
        }
        refined
    }

    /// Narrows the values of `id` to those for which `id operator other` can hold, until the
    /// condition is left; returns 1 if that changed anything.
    fn narrow(&mut self, id: usize, ty: Int, operator: BinaryOp, other: Interval) -> usize {
        if !self.fixed.contains(&id) {
            return 0;
        }
        let known = self.values.get(&id).copied().unwrap_or_else(|| ty.full());
        let (low, high) = match operator {
            BinaryOp::Less => (known.low, known.high.min(other.high - 1)),
            BinaryOp::LessEqual => (known.low, known.high.min(other.high)),
            BinaryOp::Greater => (known.low.max(other.low + 1), known.high),
            BinaryOp::GreaterEqual => (known.low.max(other.low), known.high),
            BinaryOp::Equal => (known.low.max(other.low), known.high.min(other.high)),
            BinaryOp::NotEqual if other.low == other.high && other.low == known.low => {
                (known.low + 1, known.high)
            }
            BinaryOp::NotEqual if other.low == other.high && other.low == known.high => {
                (known.low, known.high - 1)
            }
            _ => return 0,
        };
        // A condition that cannot hold leaves the branch unreachable, and whatever is known is fine.
        if low > high || (low, high) == (known.low, known.high) {
            return 0;
        }
        self.undo.push((id, self.values.get(&id).copied()));
        self.values.insert(id, Interval::new(low, high));
        1
    }

    /// Puts back what the entries of `undo` from `mark` on replaced.
    pub(super) fn restore(&mut self, mark: usize) {
        while self.undo.len() > mark {
            let (id, previous) = self.undo.pop().unwrap();
            match previous {
                Some(values) => self.values.insert(id, values),
                None => self.values.remove(&id),
            };
        }
    }

    /// The values of `expression`, which has the integer type `ty`.
    fn value(
        &mut self,
        module: &CheckedModule,
        expression: &TypedExpr,
        ty: Int,
        depth: u32,
    ) -> Interval {
        if depth >= MAX_DEPTH || self.steps == 0 || integer(&expression.ty) != Some(ty) {
            return ty.full();
        }
        self.steps -= 1;
        self.compute(module, expression, ty, depth + 1)
            .filter(|known| known.within(ty.full()))
            .unwrap_or_else(|| ty.full())
    }

    fn compute(
        &mut self,
        module: &CheckedModule,
        expression: &TypedExpr,
        ty: Int,
        depth: u32,
    ) -> Option<Interval> {
        match &expression.kind {
            TypedExprKind::Int(pattern) => literal(ty, *pattern),
            TypedExprKind::Local(id) => self.values.get(id).copied(),
            TypedExprKind::Binary(operator, left, right) => {
                self.binary_value(module, *operator, left, right, ty, depth)
            }
            TypedExprKind::Unary(operator, operand) => {
                let operand = self.value(module, operand, ty, depth);
                match operator {
                    UnaryOp::Negate => Some(
                        interval::arithmetic(Operation::Subtract, ty, Interval::point(0), operand)
                            .result,
                    ),
                    UnaryOp::BitNot => Some(interval::complement(ty, operand)),
                    UnaryOp::Plus => Some(operand),
                    UnaryOp::Not => None,
                }
            }
            TypedExprKind::Cast(operand) => {
                let from = integer(&operand.ty)?;
                Some(interval::cast(ty, self.value(module, operand, from, depth)))
            }
            // A collection cannot have more elements than an `i64` counts.
            TypedExprKind::Length(_) | TypedExprKind::StringLength(_) => {
                Some(Interval::new(0, i128::from(i64::MAX)))
            }
            TypedExprKind::Call(callee, arguments) if arguments.len() == 1 => {
                let TypedExprKind::Function(FunctionRef::User(id)) = callee.kind else {
                    return None;
                };
                is_array_length(module, id).then(|| Interval::new(0, i128::from(i64::MAX)))
            }
            TypedExprKind::If {
                condition,
                then_branch,
                else_branch,
            } => {
                let mark = self.undo.len();
                self.refine(module, condition, depth);
                let then_value = self.value(module, then_branch, ty, depth);
                self.restore(mark);
                Some(then_value.hull(self.value(module, else_branch, ty, depth)))
            }
            TypedExprKind::Block { bindings, result } => {
                let mark = self.undo.len();
                for (local, value) in bindings {
                    if let Some(local_ty) = self.trackable(local) {
                        let known = self.value(module, value, local_ty, depth);
                        self.undo
                            .push((local.id, self.values.get(&local.id).copied()));
                        self.values.insert(local.id, known);
                    }
                }
                let value = self.value(module, result, ty, depth);
                self.restore(mark);
                Some(value)
            }
            _ => None,
        }
    }

    fn binary_value(
        &mut self,
        module: &CheckedModule,
        operator: BinaryOp,
        left: &TypedExpr,
        right: &TypedExpr,
        ty: Int,
        depth: u32,
    ) -> Option<Interval> {
        use BinaryOp::*;
        if integer(&left.ty) != Some(ty) || integer(&right.ty) != Some(ty) {
            return None;
        }
        if matches!(operator, ShiftLeft | ShiftRight | ShiftRightUnsigned) {
            // A shift count is cut to the width, which is a power of two.
            let TypedExprKind::Int(count) = right.kind else {
                return None;
            };
            if !ty.bits.is_power_of_two() {
                return None;
            }
            let count = u32::try_from(count & u128::from(ty.bits - 1)).ok()?;
            let left = self.value(module, left, ty, depth);
            return Some(match operator {
                ShiftLeft => interval::shift_left(ty, left, count),
                ShiftRight => interval::shift_right(left, count),
                _ => interval::shift_right_logical(ty, left, count),
            });
        }
        if !matches!(
            operator,
            Add | Subtract | Multiply | BitAnd | Divide | Remainder
        ) {
            return None;
        }
        let left = self.value(module, left, ty, depth);
        let right = self.value(module, right, ty, depth);
        Some(match operator {
            Add | Subtract | Multiply => {
                interval::arithmetic(operation(operator)?, ty, left, right).result
            }
            BitAnd => interval::bit_and(ty, left, right),
            Divide => interval::divide(ty, left, right),
            _ => interval::remainder(ty, left, right),
        })
    }
}
