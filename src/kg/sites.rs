//! The sites of the facts (G26 phase 4) in the final functions: where a body may trap (appendix E3)
//! and where it allocates explicitly (Level C of `alloc_sites`). Copies come from `copies::sites`.

use crate::check::{Builtin, CheckedModule, FunctionRef, Type, TypedExpr, TypedExprKind};
use crate::diagnostic::Span;
use crate::syntax::BinaryOp;

/// A place where a body may trap, and whether a proof shows that it does not.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Trap {
    pub(crate) span: Span,
    pub(crate) kind: &'static str,
    pub(crate) proven: bool,
}

enum Frame<'a> {
    /// An expression, and whether it projects a pattern's value (no bounds site there).
    Visit(&'a TypedExpr, bool),
    Enter(&'a TypedExpr),
    Leave,
}

/// Whether `value` is the integer `-1` of `bits` bits.
fn minus_one(value: u128, bits: u16) -> bool {
    if bits >= 128 {
        value == u128::MAX
    } else {
        value == (1u128 << bits) - 1
    }
}

/// Whether an expression of this variant can bear a fact (appendix E1): a call, a host call, a
/// function or closure reference, a task, a branch, a loop, an allocation, an index or slice, a
/// division, or `@checked` arithmetic. The others are only traversed. Every variant is named, so
/// a new one does not go unnoticed.
pub(crate) fn bears_facts(kind: &TypedExprKind) -> bool {
    use TypedExprKind::*;
    match kind {
        Function(_)
        | GenericFunction(..)
        | Method(..)
        | TypeFunction { .. }
        | Binary(..)
        | Call(..)
        | HostCall(..)
        | Lambda { .. }
        | Closure(..)
        | TaskRun(_)
        | TaskParallel(_)
        | TaskParallelResults(_)
        | Parallel(..)
        | If { .. }
        | While { .. }
        | ForRange { .. }
        | ForEach { .. }
        | NewArray(..)
        | NewList(..)
        | NewLiteral(_)
        | Index(..)
        | Slice { .. }
        | Checked(_)
        | Match { .. } => true,
        Error
        | Int(_)
        | Float(_)
        | String(_)
        | Bool(_)
        | Unit
        | Break
        | Continue
        | Local(_)
        | Static(_)
        | GenericInteger(..)
        | GenericFloat(_)
        | Unary(..)
        | StructuralCompare(..)
        | StructuralHash(_)
        | StructuralDisplay(_)
        | Interpolated(_)
        | Block { .. }
        | Record(_)
        | RecordUpdate { .. }
        | Array(_)
        | List(_)
        | Tuple(_)
        | ListTail(..)
        | Field(..)
        | Length(_)
        | StringLength(_)
        | Borrow(..)
        | BorrowOperand(_)
        | Dereference(_)
        | Assign(..)
        | Cast(_)
        | SaturatingIndex(_)
        | CaseConstructor { .. }
        | Construct { .. }
        | UnionTag(_)
        | UnionPayload { .. }
        | Try(_)
        | RaisedException
        | Reraise
        | DynDispatch { .. } => false,
    }
}

/// The built-in function that a callee calls: the built-in itself, or the wrapper that
/// `closures::lower` generates for it in the module `$builtin`, whose body calls it.
pub(crate) fn called_builtin(module: &CheckedModule, callee: &TypedExpr) -> Option<Builtin> {
    match &callee.kind {
        TypedExprKind::Function(FunctionRef::Builtin(instance)) => Some(instance.builtin),
        TypedExprKind::Function(FunctionRef::User(id))
            if module.functions.get(*id)?.module == "$builtin" =>
        {
            match &module.functions[*id].body.kind {
                TypedExprKind::Call(inner, _) => match &inner.kind {
                    TypedExprKind::Function(FunctionRef::Builtin(instance)) => {
                        Some(instance.builtin)
                    }
                    _ => None,
                },
                _ => None,
            }
        }
        _ => None,
    }
}

/// Whether an indexed value is a sequence that the index checks against its length.
fn indexed(ty: &Type) -> bool {
    match ty {
        Type::Reference(inner, _) => indexed(inner),
        Type::Array(_)
        | Type::List(_)
        | Type::Vec(_)
        | Type::FixedArray(..)
        | Type::String
        | Type::Utf8String => true,
        _ => false,
    }
}

/// The trap and allocation sites of the final function `function`, visiting at most `limit` nodes
/// (taken from `budget`); the flag says the traversal stopped there.
pub(crate) fn sites(
    module: &CheckedModule,
    function: usize,
    limit: usize,
    budget: &mut usize,
) -> (Vec<Trap>, Vec<(Span, &'static str)>, bool) {
    let checked = &module.functions[function];
    let mut facts = crate::ranges::analyze(module, checked, false);
    let mut entered = Vec::new();
    let mut traps = Vec::new();
    let mut allocations = Vec::new();
    let mut stack = vec![Frame::Visit(&checked.body, false)];
    let mut visited = 0;
    let mut truncated = false;
    while let Some(frame) = stack.pop() {
        let (expression, projection) = match frame {
            Frame::Visit(expression, projection) => (expression, projection),
            Frame::Enter(condition) => {
                entered.push(facts.enter_condition(module, condition));
                continue;
            }
            Frame::Leave => {
                if let Some(facts_entered) = entered.pop() {
                    facts.leave_condition(facts_entered);
                }
                continue;
            }
        };
        if visited == limit || *budget == 0 {
            truncated = true;
            break;
        }
        visited += 1;
        *budget -= 1;
        if !bears_facts(&expression.kind) {
            stack.extend(
                expression
                    .children()
                    .into_iter()
                    .rev()
                    .map(|child| Frame::Visit(child, projection)),
            );
            continue;
        }
        let mut trap = |kind: &'static str, proven: bool| {
            traps.push(Trap {
                span: expression.span,
                kind,
                proven,
            })
        };
        match &expression.kind {
            TypedExprKind::Call(callee, _) => match called_builtin(module, callee) {
                Some(Builtin::Assert) => trap("assert", false),
                Some(Builtin::Unreachable) => trap("unreachable", false),
                _ => {}
            },
            TypedExprKind::Binary(BinaryOp::Divide | BinaryOp::Remainder, left, right) => {
                if let Type::Integer(bits, signed) = left.ty {
                    let constant = match right.kind {
                        TypedExprKind::Int(value) => Some(value),
                        _ => None,
                    };
                    trap(
                        "integer_division_by_zero",
                        constant.is_some_and(|value| value != 0),
                    );
                    if signed {
                        trap(
                            "integer_division_overflow",
                            constant.is_some_and(|value| !minus_one(value, bits)),
                        );
                    }
                }
            }
            TypedExprKind::Index(array, index) if !projection && indexed(&array.ty) => {
                trap("bounds_check", facts.index_in_bounds(array, index))
            }
            TypedExprKind::Slice { .. } if !projection => trap("bounds_check", false),
            TypedExprKind::ForRange { step, .. } => trap(
                "range_step_zero",
                matches!(step.kind, TypedExprKind::Int(value) if value != 0),
            ),
            TypedExprKind::Checked(_) => trap("overflow", false),
            TypedExprKind::NewArray(..) => allocations.push((expression.span, "new_array")),
            TypedExprKind::NewList(..) => allocations.push((expression.span, "new_list")),
            TypedExprKind::NewLiteral(_) => allocations.push((expression.span, "new_literal")),
            _ => {}
        }
        match &expression.kind {
            TypedExprKind::If {
                condition,
                then_branch,
                else_branch,
            } => {
                // Evaluation order: the condition, then the branch it proves facts for, then the
                // other branch (as `llvm.rs` emits them).
                stack.push(Frame::Visit(else_branch, projection));
                stack.push(Frame::Leave);
                stack.push(Frame::Visit(then_branch, projection));
                stack.push(Frame::Enter(condition));
                stack.push(Frame::Visit(condition, projection));
            }
            TypedExprKind::Match { value, arms, .. } => {
                let mut frames = vec![Frame::Visit(value, projection)];
                for arm in arms {
                    for alternative in &arm.alternatives {
                        frames.extend(
                            alternative
                                .steps
                                .iter()
                                .map(|step| Frame::Visit(step.expression(), true)),
                        );
                        frames.extend(
                            alternative
                                .bindings
                                .iter()
                                .map(|(_, value)| Frame::Visit(value, true)),
                        );
                    }
                    frames.extend(
                        arm.guard
                            .iter()
                            .map(|guard| Frame::Visit(guard, projection)),
                    );
                    frames.push(Frame::Visit(&arm.body, projection));
                }
                stack.extend(frames.into_iter().rev());
            }
            _ => stack.extend(
                expression
                    .children()
                    .into_iter()
                    .rev()
                    .map(|child| Frame::Visit(child, projection)),
            ),
        }
    }
    (traps, allocations, truncated)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minus_one_has_every_bit_of_its_width() {
        assert!(minus_one(u128::from(u64::MAX), 64));
        assert!(minus_one(0xff, 8));
        assert!(!minus_one(0x7f, 8));
        assert!(minus_one(u128::MAX, 128));
        assert!(!minus_one(1, 64));
    }
}
