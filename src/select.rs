//! `select` (TZ04): the arms' channel operations become one `SelectWait`, which waits for one of
//! them, and the arms' bodies become a `match` on what it gives, so that ownership, the flow
//! analyses and the code of the bodies are those of `match`.
use super::*;

/// The handles that a receive arm and a send arm take (TZ03 Phase 2 adds the direction handles).
const RECEIVERS: [&str; 2] = ["Channel.Channel", "ReceiveOnly.ReceiveOnly"];
const SENDERS: [&str; 2] = ["Channel.Channel", "SendOnly.SendOnly"];

impl Checker<'_> {
    /// Checks `select | head [when guard] -> body ...`.
    pub(super) fn select(
        &mut self,
        arms: &[SelectArm],
        expected: Option<&Type>,
        span: Span,
    ) -> Result<TypedExpr, Diagnostic> {
        let default = arms
            .iter()
            .position(|arm| matches!(arm.head, SelectHead::Default));
        let mut wait_arms = Vec::new();
        // Field 0 is the chosen arm; a receive arm with a name has a field of its own.
        let mut fields = vec![Type::I64];
        let mut field_of = vec![None; arms.len()];
        for (index, arm) in arms.iter().enumerate() {
            let (channel, value) = match &arm.head {
                SelectHead::Default => continue,
                SelectHead::Recv { channel, .. } => (channel, None),
                SelectHead::Send { value, channel } => (channel, Some(value)),
            };
            let guard = arm
                .guard
                .as_ref()
                .map(|guard| self.expression(guard, Some(&Type::Bool)))
                .transpose()?;
            let checked = Self::autoderef(self.expression(channel, None)?);
            if checked.ty.contains_error() {
                return Ok(TypedExpr::error(span));
            }
            let handles: &[&str] = if value.is_some() {
                &SENDERS
            } else {
                &RECEIVERS
            };
            let item = match &checked.ty {
                Type::Record(id, arguments)
                    if self.types.records[*id].origin == ModuleOrigin::Std
                        && handles.contains(&self.types.records[*id].name.as_str()) =>
                {
                    arguments[0].clone()
                }
                found => {
                    return Err(Diagnostic::of(
                        DiagnosticKind::TypeMismatch,
                        format!("expected Channel<…>, found {}", found.display(&self.types)),
                        checked.span,
                    ));
                }
            };
            let channel = Self::select_borrow(checked)?;
            let send = value
                .map(|value| self.expression(value, Some(&item)))
                .transpose()?;
            let field = match &arm.head {
                SelectHead::Recv { name, .. } if name.text != "_" => {
                    fields.push(self.names.std_type(
                        "Maybe",
                        "Maybe",
                        Box::new([item.clone()]),
                        arm.span,
                    )?);
                    Some(fields.len() - 1)
                }
                _ => None,
            };
            field_of[index] = field;
            wait_arms.push(SelectWaitArm {
                arm: index,
                guard,
                channel,
                send,
                item,
                field,
            });
        }
        if fields.len() == 1 {
            fields.push(Type::Unit);
        }
        let count = fields.len();
        let wait = TypedExpr {
            kind: TypedExprKind::SelectWait(Box::new(SelectWait {
                arms: wait_arms,
                default,
            })),
            ty: Type::Tuple(fields),
            span,
        };
        // `(arm, kept receives...)`: the last arm takes any number, so the match needs no trap.
        let last = arms.len() - 1;
        let pattern = |kind, span| Pattern {
            kind,
            span,
            depth: 1,
        };
        let match_arms: Vec<MatchArm> = arms
            .iter()
            .enumerate()
            .map(|(index, arm)| {
                let number = if index == last {
                    PatternKind::Wildcard
                } else {
                    PatternKind::Literal(Box::new(Expr {
                        kind: ExprKind::Integer(index as u128, Some("i64".into())),
                        span: arm.span,
                        depth: 1,
                    }))
                };
                let mut elements = vec![pattern(number, arm.span)];
                for field in 1..count {
                    elements.push(match (&arm.head, field_of[index] == Some(field)) {
                        (SelectHead::Recv { name, .. }, true) => {
                            pattern(PatternKind::Binding(name.clone()), name.span)
                        }
                        _ => pattern(PatternKind::Wildcard, arm.span),
                    });
                }
                MatchArm {
                    pattern: Pattern {
                        kind: PatternKind::Tuple(elements),
                        span: arm.span,
                        depth: 2,
                    },
                    guard: None,
                    body: arm.body.clone(),
                    span: arm.span,
                }
            })
            .collect();
        self.match_value(wait, &match_arms, expected, span, None)
    }

    /// The shared borrow of the channel that an arm names: a place is borrowed for the wait, and a
    /// reference is passed on. A temporary channel would be gone after the select, so it is refused.
    fn select_borrow(channel: TypedExpr) -> Result<TypedExpr, Diagnostic> {
        if let TypedExprKind::Dereference(inner) = &channel.kind
            && matches!(inner.ty, Type::Reference(..))
        {
            return Ok(*inner.clone());
        }
        if !control::is_place(&channel) {
            return Err(Diagnostic::of(
                DiagnosticKind::BorrowOfTemporary,
                "a select arm borrows its channel for the wait; bind the channel to a name first",
                channel.span,
            ));
        }
        let ty = Type::Reference(Box::new(channel.ty.clone()), false);
        let span = channel.span;
        Ok(TypedExpr {
            kind: TypedExprKind::Borrow(Box::new(channel), false),
            ty,
            span,
        })
    }
}
