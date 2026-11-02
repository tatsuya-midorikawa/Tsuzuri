//! `select` (TZ04): the arms' channel operations become one `SelectWait`, which waits for one of
//! them, and the arms' bodies become a `match` on what it gives, so that ownership, the flow
//! analyses and the code of the bodies are those of `match`.
use super::*;

/// The handles that a receive arm and a send arm take (TZ03 Phase 2 adds the direction handles).
const RECEIVERS: [&str; 2] = ["Channel.Channel", "ReceiveOnly.ReceiveOnly"];
const SENDERS: [&str; 2] = ["Channel.Channel", "SendOnly.SendOnly"];
/// The item types that the named receives of one select may have: each type is a field of what the
/// wait gives, and a pattern of n fields costs the checker more than n² steps.
const KEPT_TYPES: usize = 256;

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
        // Field 0 is the chosen arm; the named receives of one item type share a field (only the
        // chosen arm's is `Some`).
        let mut fields = vec![Type::I64];
        let mut kept: Vec<Type> = Vec::new();
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
                    let item = self.inference.resolve(&item);
                    match kept.iter().position(|kept| *kept == item) {
                        Some(at) => Some(at + 1),
                        None if kept.len() == KEPT_TYPES => {
                            return Err(Diagnostic::of(
                                DiagnosticKind::SelectArmLimit,
                                format!(
                                    "the named receives of a select have at most {KEPT_TYPES} item types; receive into '_' or split the select"
                                ),
                                arm.span,
                            ));
                        }
                        None => {
                            fields.push(self.names.std_type(
                                "Maybe",
                                "Maybe",
                                Box::new([item.clone()]),
                                arm.span,
                            )?);
                            kept.push(item);
                            Some(fields.len() - 1)
                        }
                    }
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
        // `(arm, kept receives...)` binds the arm's number and each kept receive to a local of its
        // own, and a match on the number runs the chosen arm's body, a kept receive's first binding
        // its name. Each arm's pattern is one number (the last arm takes any, so the match needs no
        // trap), so the lowering grows with the arms, not with their square.
        let hidden = |what: &str, index: usize| Ident {
            text: format!("$select_{what}_{index}_{}", span.start),
            span,
            provenance: Provenance::Generated,
        };
        let pattern = |kind, span| Pattern {
            kind,
            span,
            depth: 1,
        };
        let number = hidden("arm", 0);
        let mut elements = vec![pattern(PatternKind::Binding(number.clone()), span)];
        for field in 1..count {
            let kept = field_of.contains(&Some(field));
            elements.push(pattern(
                if kept {
                    PatternKind::Binding(hidden("item", field))
                } else {
                    PatternKind::Wildcard
                },
                span,
            ));
        }
        let last = arms.len() - 1;
        let depth = arms.iter().map(|arm| arm.body.depth).max().unwrap_or(0) + 3;
        let chosen: Vec<MatchArm> = arms
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
                let body = match (&arm.head, field_of[index]) {
                    (SelectHead::Recv { name, .. }, Some(field)) => Expr {
                        kind: ExprKind::Block {
                            bindings: vec![Binding {
                                name: name.clone(),
                                mutable: false,
                                using: false,
                                recursion: Recursion::None,
                                annotation: None,
                                value: Expr {
                                    kind: ExprKind::Name(hidden("item", field)),
                                    span: name.span,
                                    depth: 1,
                                },
                            }],
                            result: Box::new(arm.body.clone()),
                        },
                        span: arm.body.span,
                        depth: arm.body.depth + 1,
                    },
                    _ => arm.body.clone(),
                };
                MatchArm {
                    pattern: pattern(number, arm.span),
                    guard: None,
                    body,
                    span: arm.span,
                }
            })
            .collect();
        let body = Expr {
            kind: ExprKind::Match {
                value: Box::new(Expr {
                    kind: ExprKind::Name(number),
                    span,
                    depth: 1,
                }),
                arms: chosen,
                origin: MatchOrigin::ComputationDestructuring,
            },
            span,
            depth,
        };
        let unpack = MatchArm {
            pattern: Pattern {
                kind: PatternKind::Tuple(elements),
                span,
                depth: 2,
            },
            guard: None,
            body,
            span,
        };
        self.match_value(wait, &[unpack], expected, span, None)
    }

    /// The shared borrow of the channel that an arm names: a place is borrowed for the wait, and a
    /// shared reference is passed on (an exclusive one is borrowed again as shared). A temporary
    /// channel would be gone after the select, so it is refused.
    fn select_borrow(channel: TypedExpr) -> Result<TypedExpr, Diagnostic> {
        if let TypedExprKind::Dereference(inner) = &channel.kind
            && matches!(inner.ty, Type::Reference(_, false))
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
