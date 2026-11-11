use super::exhaustiveness::{Constructor, CoverageArm, CoveragePat, MatchCoverage};
use super::*;

pub(super) const MAX_PATTERN_ALTERNATIVES: usize = 1024;

#[derive(Clone, Default)]
struct Alternative {
    steps: Vec<PatternStep>,
    bindings: Vec<(Ident, TypedExpr)>,
    /// The narrowings of the alternative, in an arm without a guard (A25).
    narrowed: Vec<Narrowing>,
}

/// `:? W as x` where `W` has fewer members than the matched anonymous union (A25), in an arm
/// without a guard. The alternative binds the matched value to the hidden name `hidden`, and
/// `match_value` binds `name` around the arm's body to that value as a value of `to`, which
/// `retag` makes from `cases`.
#[derive(Clone)]
struct Narrowing {
    name: Ident,
    hidden: String,
    to: Type,
    cases: Vec<(usize, usize)>,
    /// Whether the hidden binding views the matched value where a binding would move it: every
    /// member of `to` is Copy, so the narrowing copies the payload and leaves the value whole.
    view: bool,
    /// The whole pattern.
    span: Span,
}

/// The name that hides the matched value of a narrowing into `name`. No source spells it.
fn narrowed_name(name: &str) -> String {
    format!("$narrow.{name}")
}

fn value(kind: TypedExprKind, ty: Type, span: Span) -> TypedExpr {
    TypedExpr { kind, ty, span }
}

fn local_value(local: &Local) -> TypedExpr {
    value(TypedExprKind::Local(local.id), local.ty.clone(), local.span)
}

/// Matches the place classification of ownership and LLVM lowering, which
/// view such a match subject in place instead of binding a temporary.
pub(super) fn is_place(expression: &TypedExpr) -> bool {
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

fn combine(
    left: Vec<Alternative>,
    right: Vec<Alternative>,
    span: Span,
) -> Result<Vec<Alternative>, Diagnostic> {
    if left.len().saturating_mul(right.len()) > MAX_PATTERN_ALTERNATIVES {
        return Err(Diagnostic::of(
            DiagnosticKind::PatternAlternativesLimit,
            "pattern alternatives exceed the compiler limit; split the match",
            span,
        ));
    }
    let mut result = Vec::new();
    for left in left {
        for right in &right {
            let mut combined = left.clone();
            combined.steps.extend(right.steps.iter().cloned());
            combined.bindings.extend(right.bindings.iter().cloned());
            combined.narrowed.extend(right.narrowed.iter().cloned());
            result.push(combined);
        }
    }
    Ok(result)
}

impl Checker<'_> {
    pub(super) fn control_expression(
        &mut self,
        expression: &Expr,
        expected: Option<&Type>,
    ) -> Result<TypedExpr, Diagnostic> {
        let kind = match &expression.kind {
            ExprKind::While { condition, body } => {
                let condition = self.expression(condition, Some(&Type::Bool))?;
                self.normal_loop_depth += 1;
                let body = self.expression(body, Some(&Type::Unit));
                self.normal_loop_depth -= 1;
                TypedExprKind::While {
                    condition: Box::new(condition),
                    body: Box::new(body?),
                }
            }
            ExprKind::For {
                pattern,
                source,
                body,
            } => {
                let (source, range, element) = if let ExprKind::Range {
                    start,
                    step,
                    finish,
                    counted: _,
                    descending,
                    op,
                } = &source.kind
                {
                    // Both forms count in any integer type; an unsuffixed literal counter is `i32`.
                    let ty = self.inference.fresh();
                    let start = self.expression(start, Some(&ty))?;
                    let step = if let Some(step) = step {
                        self.expression(step, Some(&ty))?
                    } else {
                        // `downto` steps down by `1` (`descending`), so its step fits an unsigned type.
                        let (kind, ty) = self.integer(1, None, Some(&ty), false, source.span)?;
                        value(kind, ty, source.span)
                    };
                    let finish = self.expression(finish, Some(&ty))?;
                    self.require("Integer", ty.clone(), source.span)?;
                    (
                        None,
                        Some((start, step, finish, *descending, *op == RangeOp::Open)),
                        self.inference.resolve(&ty),
                    )
                } else {
                    let source = Self::autoderef(self.expression(source, None)?);
                    if source.ty.contains_error() {
                        return Ok(TypedExpr::error(expression.span));
                    }
                    if source.ty.sequence_element(&self.types).is_some() {
                        if let Some(expected) = expected {
                            self.same(&Type::Unit, expected, expression.span)?;
                        }
                        return self.sequence_loop(source, pattern, body, expression.span);
                    }
                    if let Some((receive, _)) = source.ty.channel_element(&self.types) {
                        if let Some(expected) = expected {
                            self.same(&Type::Unit, expected, expression.span)?;
                        }
                        return self.channel_loop(source, receive, pattern, body, expression.span);
                    }
                    let element = match &source.ty {
                        Type::Array(element) | Type::List(element) | Type::Vec(element) => {
                            (**element).clone()
                        }
                        Type::String => Type::Integer(16, false),
                        Type::Utf8String => Type::Integer(8, false),
                        Type::FixedArray(..) => {
                            return Err(Diagnostic::of(
                                DiagnosticKind::ForOverFixedArray,
                                "a fixed-length array is not enumerable; iterate 'for i in 0 .. xs.length - 1' and index it",
                                source.span,
                            ));
                        }
                        _ => {
                            return Err(Diagnostic::of(
                                DiagnosticKind::ForSourceNotIterable,
                                "for...in expects an array, list, string, utf8string, integer range, Seq, Channel, or ReceiveOnly; call Module.iter explicitly for user-defined types",
                                source.span,
                            ));
                        }
                    };
                    (Some(source), None, element)
                };
                self.scopes.push(BTreeMap::new());
                let simple_binding = matches!(&pattern.kind, PatternKind::Binding(name)
                    if !name.text.contains(['.', ':'])
                        && self.active_recognizer(name)?.is_none()
                        && matches!(self.names.case_path(self.module, &name.text, name.span), Ok(None)));
                let name = match &pattern.kind {
                    PatternKind::Binding(name) if simple_binding => name.clone(),
                    _ => Ident {
                        text: format!("$iteration{}", pattern.span.start),
                        span: pattern.span,
                        provenance: Provenance::Generated,
                    },
                };
                let local = self.bind(&name, element, false);
                if source.is_some() {
                    self.borrowed.insert(local.id);
                }
                self.normal_loop_depth += 1;
                let body = if simple_binding || matches!(pattern.kind, PatternKind::Wildcard) {
                    self.expression(body, Some(&Type::Unit))?
                } else {
                    self.match_value(
                        local_value(&local),
                        &[MatchArm {
                            pattern: (**pattern).clone(),
                            guard: None,
                            body: (**body).clone(),
                            span: body.span,
                        }],
                        Some(&Type::Unit),
                        expression.span,
                        None,
                    )?
                };
                self.normal_loop_depth -= 1;
                self.scopes.pop();
                if let Some((start, step, finish, descending, exclusive)) = range {
                    TypedExprKind::ForRange {
                        local,
                        start: Box::new(start),
                        step: Box::new(step),
                        finish: Box::new(finish),
                        body: Box::new(body),
                        descending,
                        exclusive,
                    }
                } else {
                    let source = source.unwrap();
                    let owner = self.bind(
                        &Ident {
                            text: "$enumerable".into(),
                            span: source.span,
                            provenance: Provenance::Generated,
                        },
                        source.ty.clone(),
                        false,
                    );
                    TypedExprKind::ForEach {
                        owner,
                        local,
                        source: Box::new(source),
                        body: Box::new(body),
                    }
                }
            }
            ExprKind::Match {
                value,
                arms,
                origin,
            } => {
                let value = self.expression(value, None)?;
                return self.match_value(
                    value,
                    arms,
                    expected,
                    expression.span,
                    origin.checks_coverage().then_some(*origin),
                );
            }
            _ => unreachable!("control expression kinds checked by expression"),
        };
        if let Some(expected) = expected {
            self.same(&Type::Unit, expected, expression.span)?;
        }
        Ok(value(kind, Type::Unit, expression.span))
    }

    /// `for x in ch` over a `Channel` or a `ReceiveOnly` (TZ03), Go's `for x := range ch`, with the
    /// `receive` function of its type: receives until the channel
    /// is closed and empty. Like `sequence_loop`, it checks a generated `while`, so `break` and
    /// `continue` keep their meaning:
    ///
    /// ```text
    /// while true do
    ///     match Channel.recv <channel> with
    ///     | Maybe.Some $item -> match $item with | <pattern> -> <body>
    ///     | Maybe.None -> break
    /// ```
    ///
    /// `<channel>` is a local of the loop: a shared borrow of a channel in a place, which stays
    /// usable after the loop, or a temporary channel, which the loop holds and drops after it.
    fn channel_loop(
        &mut self,
        source: TypedExpr,
        receive: &str,
        pattern: &Pattern,
        body: &Expr,
        span: Span,
    ) -> Result<TypedExpr, Diagnostic> {
        let depth = body.depth.max(pattern.depth) + 12 * (self.normal_loop_depth + 1);
        if depth > MAX_NESTING {
            return Err(Diagnostic::of(
                DiagnosticKind::SequenceLoopDepthLimit,
                "sequence loop expansion exceeds the compiler depth limit; extract the body into a function",
                span,
            ));
        }
        let ident = |name: &str| Ident {
            text: format!("$channel_{name}_{}", span.start),
            span,
            provenance: Provenance::Generated,
        };
        let named = |text: &str| Ident {
            text: text.into(),
            span,
            provenance: Provenance::Generated,
        };
        let make = |kind| Expr { kind, span, depth };
        let owner_name = ident("owner");
        let item_name = ident("item");
        let channel = make(ExprKind::Name(owner_name.clone()));
        let item_body = make(ExprKind::Match {
            value: Box::new(make(ExprKind::Name(item_name.clone()))),
            arms: vec![MatchArm {
                pattern: pattern.clone(),
                guard: None,
                body: body.clone(),
                span: body.span,
            }],
            origin: MatchOrigin::ComputationDestructuring,
        });
        let receive = make(ExprKind::Call(
            Box::new(make(ExprKind::QualifiedFunction(named(receive)))),
            vec![channel],
        ));
        let arms = vec![
            MatchArm {
                pattern: Pattern {
                    kind: PatternKind::Apply(
                        named("::Maybe.Maybe.Some"),
                        vec![Pattern {
                            kind: PatternKind::Binding(item_name),
                            span,
                            depth: 1,
                        }],
                    ),
                    span,
                    depth: 2,
                },
                guard: None,
                body: item_body,
                span,
            },
            MatchArm {
                pattern: Pattern {
                    kind: PatternKind::Binding(named("::Maybe.Maybe.None")),
                    span,
                    depth: 1,
                },
                guard: None,
                body: make(ExprKind::Break),
                span,
            },
        ];
        let loop_expression = make(ExprKind::While {
            condition: Box::new(make(ExprKind::Bool(true))),
            body: Box::new(make(ExprKind::Match {
                value: Box::new(receive),
                arms,
                origin: MatchOrigin::ComputationDestructuring,
            })),
        });
        let source = if is_place(&source) {
            let ty = Type::Reference(Box::new(source.ty.clone()), false);
            let span = source.span;
            value(TypedExprKind::Borrow(Box::new(source), false), ty, span)
        } else {
            source
        };
        self.scopes.push(BTreeMap::new());
        let owner = self.bind(&owner_name, source.ty.clone(), false);
        let result = self.expression(&loop_expression, Some(&Type::Unit));
        self.scopes.pop();
        Ok(value(
            TypedExprKind::Block {
                bindings: vec![(owner, source)],
                result: Box::new(result?),
            },
            Type::Unit,
            span,
        ))
    }

    fn sequence_loop(
        &mut self,
        source: TypedExpr,
        pattern: &Pattern,
        body: &Expr,
        span: Span,
    ) -> Result<TypedExpr, Diagnostic> {
        let depth = body.depth.max(pattern.depth) + 12 * (self.normal_loop_depth + 1);
        if depth > MAX_NESTING {
            return Err(Diagnostic::of(
                DiagnosticKind::SequenceLoopDepthLimit,
                "sequence loop expansion exceeds the compiler depth limit; extract the body into a function",
                span,
            ));
        }
        let ident = |name: &str| Ident {
            text: format!("$sequence_{name}_{}", span.start),
            span,
            provenance: Provenance::Generated,
        };
        let owner_name = ident("owner");
        let next_name = ident("next");
        let item_name = ident("item");
        let make = |kind| Expr { kind, span, depth };
        let binding_pattern = |name: Ident| Pattern {
            kind: PatternKind::Binding(name),
            span,
            depth: 1,
        };
        let named = |text: &str| Ident {
            text: text.into(),
            span,
            provenance: Provenance::Generated,
        };
        let advance = make(ExprKind::Assign(
            Box::new(make(ExprKind::Name(owner_name.clone()))),
            Box::new(make(ExprKind::Name(next_name.clone()))),
        ));
        let item_body = make(ExprKind::Match {
            value: Box::new(make(ExprKind::Name(item_name.clone()))),
            arms: vec![MatchArm {
                pattern: pattern.clone(),
                guard: None,
                body: body.clone(),
                span: body.span,
            }],
            origin: MatchOrigin::ComputationDestructuring,
        });
        let after_advance = |result| {
            make(ExprKind::Block {
                bindings: vec![Binding {
                    name: ident("advance"),
                    mutable: false,
                    using: false,
                    recursion: Recursion::None,
                    annotation: None,
                    value: advance.clone(),
                }],
                result: Box::new(result),
            })
        };
        let step = make(ExprKind::Call(
            Box::new(make(ExprKind::QualifiedFunction(named("Seq.next")))),
            vec![make(ExprKind::Name(owner_name.clone()))],
        ));
        let arms = vec![
            MatchArm {
                pattern: Pattern {
                    kind: PatternKind::Tuple(vec![
                        binding_pattern(next_name.clone()),
                        Pattern {
                            kind: PatternKind::Apply(
                                named("::Maybe.Maybe.Some"),
                                vec![binding_pattern(item_name)],
                            ),
                            span,
                            depth: 3,
                        },
                    ]),
                    span,
                    depth: 4,
                },
                guard: None,
                body: after_advance(item_body),
                span,
            },
            MatchArm {
                pattern: Pattern {
                    kind: PatternKind::Tuple(vec![
                        binding_pattern(next_name),
                        binding_pattern(named("::Maybe.Maybe.None")),
                    ]),
                    span,
                    depth: 3,
                },
                guard: None,
                body: after_advance(make(ExprKind::Break)),
                span,
            },
        ];
        let body = make(ExprKind::Match {
            value: Box::new(step),
            arms,
            origin: MatchOrigin::ComputationDestructuring,
        });
        let loop_expression = make(ExprKind::While {
            condition: Box::new(make(ExprKind::Bool(true))),
            body: Box::new(body),
        });
        self.scopes.push(BTreeMap::new());
        let owner = self.bind(&owner_name, source.ty.clone(), true);
        let result = self.expression(&loop_expression, Some(&Type::Unit));
        self.scopes.pop();
        Ok(value(
            TypedExprKind::Block {
                bindings: vec![(owner, source)],
                result: Box::new(result?),
            },
            Type::Unit,
            span,
        ))
    }

    /// Whether `place` lies behind a reference, inside a collection element
    /// or tail, or within a local that already aliases such storage. A match
    /// cannot move a non-Copy value out of it, so a pattern variable bound
    /// there may instead view it (`TypedMatchArm::views`).
    fn borrowed_place(&self, place: &TypedExpr) -> bool {
        match &place.kind {
            TypedExprKind::Local(id) => self.borrowed.contains(id),
            TypedExprKind::Dereference(_) => true,
            TypedExprKind::Index(value, _) | TypedExprKind::ListTail(value, _) => {
                matches!(
                    value.ty,
                    Type::Array(_) | Type::List(_) | Type::Vec(_) | Type::FixedArray(..)
                ) && is_place(value)
            }
            TypedExprKind::Field(value, _) | TypedExprKind::UnionPayload { value, .. } => {
                self.borrowed_place(value)
            }
            _ => false,
        }
    }

    /// The local that holds the subject `matched` of a match at `span`, bound in the innermost
    /// scope, which the caller has pushed for the match. A subject in borrowed storage is itself
    /// borrowed, so that the bindings below it are (`borrowed_place`).
    fn match_subject(&mut self, matched: &TypedExpr, span: Span) -> Local {
        let subject = self.bind(
            &Ident {
                text: format!("$match{}", span.start),
                span,
                provenance: Provenance::Generated,
            },
            matched.ty.clone(),
            false,
        );
        if self.borrowed_place(matched) {
            self.borrowed.insert(subject.id);
        }
        subject
    }

    /// `subset` as a value of the anonymous union `to` (A25): member `i` of the anonymous union
    /// that `subset` has is member `members[i]` of `to`. See `retag`.
    #[inline(never)]
    pub(super) fn widen_union(
        &mut self,
        subset: TypedExpr,
        to: &Type,
        members: &[usize],
    ) -> TypedExpr {
        let cases: Vec<(usize, usize)> = members.iter().copied().enumerate().collect();
        let span = subset.span;
        self.retag(subset, span, to, &cases)
    }

    /// A value of the union type `to` made from the union value `matched` (A25): for each
    /// `(from, into)` of `cases`, case `from` of `matched` becomes case `into` of `to` with the
    /// same payload. Both cases carry a payload of the same type, and `cases` names a case of
    /// `matched` at most once; a value of a case that `cases` does not name traps as a match
    /// without an arm for it does. This is the typed match of
    ///
    /// ```text
    /// match matched with
    /// | <case from0> x0 -> <case into0 of to> x0
    /// | <case from1> x1 -> <case into1 of to> x1
    /// ```
    ///
    /// in the shape that `match_value` gives it, so that ownership, last uses, effects and code
    /// generation treat it as the match that a program writes: `matched` is evaluated once, a
    /// tag test selects the arm (`switch_plan` switches on it), and the payload goes through the
    /// arm's binding, so that ownership sees it leave `matched`. No binding is a view (`borrowed`
    /// is empty): a payload in borrowed storage is taken out of it, which copies a Copy payload
    /// and which ownership rejects for any other, as it does for `Some s`.
    ///
    /// Everything has the span `span` (a widening gives the span of `matched`, a narrowing the
    /// span of its pattern), except what follows. The reads of the subject and
    /// of the bindings have the empty span at its end, so that a hover or a definition request
    /// on `matched` finds `matched` and not a local that no source spells. A binding whose type
    /// is never both Copy and in need of a drop also has that empty span: `LastUses` keeps one
    /// decision for each binding span, a binding of such a type always says "copied", and no
    /// one reads its decision, so with a span of its own it cannot take the move from the
    /// binding of an array member, which reports its copies at `span`.
    fn retag(
        &mut self,
        mut matched: TypedExpr,
        span: Span,
        to: &Type,
        cases: &[(usize, usize)],
    ) -> TypedExpr {
        matched.ty = self.inference.resolve(&matched.ty);
        let (Type::Union(from_id, from_arguments), Type::Union(to_id, _)) = (&matched.ty, to)
        else {
            unreachable!("a union value is given the cases of another union")
        };
        let (from_id, from_arguments, to_id) = (*from_id, from_arguments.clone(), *to_id);
        let read = Span {
            start: span.end,
            ..span
        };
        let tag = Type::Integer(32, true);
        self.scopes.push(BTreeMap::new());
        let subject = self.match_subject(&matched, span);
        let read_subject = || value(TypedExprKind::Local(subject.id), subject.ty.clone(), read);
        let mut arms = Vec::with_capacity(cases.len());
        for &(from, into) in cases {
            let payload = self
                .types
                .union_payload(from_id, &from_arguments, from)
                .expect("the case carries a payload");
            let payload = self.inference.resolve(&payload);
            // A type variable may stand for an array, so its binding is not set apart.
            let mut open = false;
            polymorph::map_type(&payload, &mut |leaf| {
                open |= polymorph::is_unknown(leaf);
                leaf.clone()
            });
            let copied = open || payload.is_copy(&self.types) && payload.needs_drop(&self.types);
            let binding = self.bind(
                &Ident {
                    text: format!("$retag{}.{from}", span.start),
                    span: if copied { span } else { read },
                    provenance: Provenance::Generated,
                },
                payload.clone(),
                false,
            );
            let test = value(
                TypedExprKind::Binary(
                    BinaryOp::Equal,
                    Box::new(value(
                        TypedExprKind::UnionTag(Box::new(read_subject())),
                        tag.clone(),
                        span,
                    )),
                    Box::new(value(TypedExprKind::Int(from as u128), tag.clone(), span)),
                ),
                Type::Bool,
                span,
            );
            let projection = value(
                TypedExprKind::UnionPayload {
                    value: Box::new(read_subject()),
                    case_id: from,
                },
                payload.clone(),
                span,
            );
            let body = value(
                TypedExprKind::Construct {
                    union_id: to_id,
                    case_id: into,
                    payload: Some(Box::new(value(
                        TypedExprKind::Local(binding.id),
                        payload,
                        read,
                    ))),
                },
                to.clone(),
                span,
            );
            arms.push(TypedMatchArm {
                alternatives: vec![PatternAlternative {
                    steps: vec![PatternStep::Test(test)],
                    bindings: vec![(binding, projection)],
                }],
                guard: None,
                body,
                borrowed: BTreeSet::new(),
            });
        }
        self.scopes.pop();
        value(
            TypedExprKind::Match {
                local: subject,
                value: Box::new(matched),
                arms,
            },
            to.clone(),
            span,
        )
    }

    /// Checks the arms of a match. With `coverage`, the arms' coverage is
    /// recorded so that `check_coverage` can reject a non-exhaustive match and
    /// warn about unreachable arms; destructuring keeps its runtime trap.
    pub(super) fn match_value(
        &mut self,
        matched: TypedExpr,
        arms: &[MatchArm],
        expected: Option<&Type>,
        span: Span,
        coverage: Option<MatchOrigin>,
    ) -> Result<TypedExpr, Diagnostic> {
        if matched.ty.contains_error() {
            return Ok(TypedExpr::error(span));
        }
        let slot = coverage.map(|origin| {
            self.coverage.push(MatchCoverage {
                span,
                scrutinee: matched.ty.clone(),
                arms: Vec::new(),
                append_at: arms
                    .last()
                    .filter(|_| origin == MatchOrigin::Explicit)
                    .map(|arm| arm.body.span.end),
            });
            self.coverage.len() - 1
        });
        let mut coverage = Vec::new();
        self.scopes.push(BTreeMap::new());
        let subject = self.match_subject(&matched, span);
        let mut checked = Vec::new();
        let mut result = expected.cloned();
        for arm in arms {
            self.scopes.push(BTreeMap::new());
            // A pattern in the arm's own expressions (a recognizer argument) is of another match.
            let outer = std::mem::replace(&mut self.guarded_arm, arm.guard.is_some());
            let alternatives = self.pattern_alternatives(&arm.pattern, local_value(&subject));
            self.guarded_arm = outer;
            let (alternatives, pattern) = alternatives?;
            coverage.push(CoverageArm {
                pattern,
                guarded: arm.guard.is_some(),
                span: arm.pattern.span,
            });
            let mut locals: BTreeMap<String, Local> = BTreeMap::new();
            let mut plans = Vec::new();
            let mut borrowed = BTreeSet::new();
            // The narrowings of the arm (A25): those of the first alternative, which every
            // other alternative repeats.
            let mut narrowed: Vec<Narrowing> = Vec::new();
            for (index, alternative) in alternatives.into_iter().enumerate() {
                for (at, narrowing) in alternative.narrowed.iter().enumerate() {
                    let name = &narrowing.name;
                    if alternative.narrowed[..at]
                        .iter()
                        .any(|other| other.name.text == name.text)
                        || alternative
                            .bindings
                            .iter()
                            .any(|(other, _)| other.text == name.text)
                    {
                        return Err(duplicate(name));
                    }
                    if index == 0 {
                        continue;
                    }
                    // The hidden bindings below report a name that only one side narrows.
                    if let Some(first) = narrowed.iter().find(|first| first.name.text == name.text)
                    {
                        self.same(&narrowing.to, &first.to, name.span)?;
                    }
                }
                if index == 0 {
                    narrowed.clone_from(&alternative.narrowed);
                }
                let mut seen = BTreeSet::new();
                let mut bindings = Vec::new();
                for (name, projection) in alternative.bindings {
                    if !seen.insert(name.text.clone()) {
                        return Err(duplicate(&name));
                    }
                    let local = if index == 0 {
                        let local = self.bind(&name, projection.ty.clone(), false);
                        locals.insert(name.text.clone(), local.clone());
                        local
                    } else {
                        let hidden = name.provenance == Provenance::Generated
                            && alternative
                                .narrowed
                                .iter()
                                .any(|narrowing| narrowing.hidden == name.text);
                        let unlike = || {
                            Diagnostic::of(
                                DiagnosticKind::OrPatternBindingMismatch,
                                "both sides of an OR pattern must narrow the same anonymous union with the same ':?' test to bind one name",
                                name.span,
                            )
                        };
                        let local = locals
                            .get(&name.text)
                            .ok_or_else(|| {
                                if hidden
                                    || narrowed
                                        .iter()
                                        .any(|narrowing| narrowing.name.text == name.text)
                                {
                                    return unlike();
                                }
                                Diagnostic::of(
                                    DiagnosticKind::OrPatternBindingMismatch,
                                    "both sides of an OR pattern must bind the same names",
                                    name.span,
                                )
                            })?
                            .clone();
                        if hidden {
                            // One retagging serves the arm: every side narrows the same union.
                            if self.inference.resolve(&projection.ty)
                                != self.inference.resolve(&local.ty)
                            {
                                return Err(unlike());
                            }
                        } else {
                            self.same(&projection.ty, &local.ty, name.span)?;
                        }
                        self.note_name(&name, NameTarget::Local(local.id));
                        local
                    };
                    if self.borrowed_place(&projection)
                        || alternative
                            .narrowed
                            .iter()
                            .any(|narrowing| narrowing.view && narrowing.hidden == local.name)
                    {
                        borrowed.insert(local.id);
                    }
                    bindings.push((local, projection));
                }
                if seen.len() != locals.len() {
                    return Err(Diagnostic::of(
                        DiagnosticKind::OrPatternBindingMismatch,
                        "both sides of an OR pattern must bind the same names",
                        arm.pattern.span,
                    ));
                }
                plans.push(PatternAlternative {
                    steps: alternative.steps,
                    bindings,
                });
            }
            self.borrowed.extend(&borrowed);
            // The body of a narrowing arm sees the narrowed names; an arm with a guard has none.
            let narrowed: Vec<(Local, Local, Narrowing)> = narrowed
                .into_iter()
                .map(|narrowing| {
                    let hidden = locals[&narrowing.hidden].clone();
                    let local = self.bind(&narrowing.name, narrowing.to.clone(), false);
                    (hidden, local, narrowing)
                })
                .collect();
            let guard = arm
                .guard
                .as_ref()
                .map(|guard| self.expression(guard, Some(&Type::Bool)))
                .transpose()?;
            let mut body = self.expression(&arm.body, result.as_ref())?;
            if !result.as_ref().is_some_and(Type::contains_error) {
                result = self.sibling_hint(&body.ty, result.as_ref()).cloned();
            }
            if !narrowed.is_empty() {
                body = self.narrowed_body(narrowed, body);
            }
            self.scopes.pop();
            checked.push(TypedMatchArm {
                alternatives: plans,
                guard,
                body,
                borrowed,
            });
        }
        self.scopes.pop();
        if let Some(slot) = slot {
            self.coverage[slot].arms = coverage;
        }
        Ok(value(
            TypedExprKind::Match {
                local: subject,
                value: Box::new(matched),
                arms: checked,
            },
            self.inference
                .resolve(&result.expect("match has at least one arm")),
            span,
        ))
    }

    /// The body of an arm that narrows (A25): a block that binds each narrowed name to the
    /// hidden binding of its matched value, retagged as the narrowed union, around `body`. An
    /// owned payload moves from the hidden binding into the narrowed value; a hidden binding
    /// that views borrowed storage has only Copy payloads to give (`narrowing_pattern`).
    #[inline(never)]
    fn narrowed_body(
        &mut self,
        narrowed: Vec<(Local, Local, Narrowing)>,
        body: TypedExpr,
    ) -> TypedExpr {
        let bindings = narrowed
            .into_iter()
            .map(|(hidden, local, narrowing)| {
                // Read at the empty span after the pattern, like the reads that `retag` makes.
                let read = Span {
                    start: narrowing.span.end,
                    ..narrowing.span
                };
                let matched = value(TypedExprKind::Local(hidden.id), hidden.ty.clone(), read);
                let narrow = self.retag(matched, narrowing.span, &narrowing.to, &narrowing.cases);
                (local, narrow)
            })
            .collect();
        let (ty, span) = (body.ty.clone(), body.span);
        value(
            TypedExprKind::Block {
                bindings,
                result: Box::new(body),
            },
            ty,
            span,
        )
    }

    /// Resolves a pattern into lowering alternatives and, from the same
    /// resolution, the pattern's coverage for the exhaustiveness check.
    fn pattern_alternatives(
        &mut self,
        pattern: &Pattern,
        mut matched: TypedExpr,
    ) -> Result<(Vec<Alternative>, CoveragePat), Diagnostic> {
        matched.ty = self.inference.resolve(&matched.ty);
        if matches!(
            pattern.kind,
            PatternKind::Literal(_)
                | PatternKind::Tuple(_)
                | PatternKind::Record(..)
                | PatternKind::Array(_)
                | PatternKind::List(_)
                | PatternKind::Cons(..)
        ) {
            matched = Self::autoderef(matched);
        }
        let mut alternatives = vec![Alternative::default()];
        let coverage = match &pattern.kind {
            PatternKind::Wildcard => CoveragePat::Wildcard,
            PatternKind::Binding(name) => {
                if let Some(case) = self.pattern_case(name)? {
                    return self.case_pattern(name, case, &[], matched);
                }
                if self.active_recognizer(name)?.is_some() {
                    return self.active_pattern(name, &[], matched);
                }
                if name.text.contains(['.', ':']) {
                    return Err(Diagnostic::of(
                        DiagnosticKind::QualifiedPatternBinding,
                        "a pattern binding cannot be a qualified name",
                        name.span,
                    ));
                }
                alternatives[0].bindings.push((name.clone(), matched));
                CoveragePat::Wildcard
            }
            PatternKind::Apply(name, arguments) => {
                if let Some(case) = self.pattern_case(name)? {
                    return self.case_pattern(name, case, arguments, matched);
                }
                return self.active_pattern(name, arguments, matched);
            }
            PatternKind::Argument(_) => {
                return Err(Diagnostic::of(
                    DiagnosticKind::ExpressionInPattern,
                    "expressions are only allowed as recognizer arguments; use 'when' for a guard",
                    pattern.span,
                ));
            }
            PatternKind::Literal(literal) => {
                let literal = self.expression(literal, Some(&matched.ty))?;
                self.require("Eq", matched.ty.clone(), pattern.span)?;
                let coverage = CoveragePat::Literal(Box::new(literal.clone()));
                alternatives[0].steps.push(PatternStep::Test(value(
                    TypedExprKind::Binary(BinaryOp::Equal, Box::new(matched), Box::new(literal)),
                    Type::Bool,
                    pattern.span,
                )));
                coverage
            }
            PatternKind::TypeTest(tested, binder) => {
                return self.type_test_pattern(pattern, tested, binder.as_ref(), matched);
            }
            PatternKind::Annotated(inner, annotation) => {
                let ty = self.annotation(annotation)?;
                self.same(&matched.ty, &ty, pattern.span)?;
                matched.ty = self.inference.resolve(&ty);
                return self.pattern_alternatives(inner, matched);
            }
            PatternKind::As(inner, name) => {
                let (mut alternatives, coverage) =
                    self.pattern_alternatives(inner, matched.clone())?;
                for alternative in &mut alternatives {
                    alternative.bindings.push((name.clone(), matched.clone()));
                }
                return Ok((alternatives, coverage));
            }
            PatternKind::Or(left, right) => {
                let (mut alternatives, left) = self.pattern_alternatives(left, matched.clone())?;
                let (others, right) = self.pattern_alternatives(right, matched)?;
                alternatives.extend(others);
                if alternatives.len() > MAX_PATTERN_ALTERNATIVES {
                    return Err(Diagnostic::of(
                        DiagnosticKind::OrPatternAlternativesLimit,
                        "too many OR pattern alternatives",
                        pattern.span,
                    ));
                }
                return Ok((
                    alternatives,
                    CoveragePat::Or(Box::new(left), Box::new(right)),
                ));
            }
            PatternKind::And(left, right) => {
                let (left, left_coverage) = self.pattern_alternatives(left, matched.clone())?;
                let (right, right_coverage) = self.pattern_alternatives(right, matched)?;
                return Ok((
                    combine(left, right, pattern.span)?,
                    CoveragePat::And(Box::new(left_coverage), Box::new(right_coverage)),
                ));
            }
            PatternKind::Tuple(patterns) => {
                let types: Vec<_> = patterns.iter().map(|_| self.inference.fresh()).collect();
                self.same(&matched.ty, &Type::Tuple(types.clone()), pattern.span)?;
                matched.ty = self.inference.resolve(&matched.ty);
                let mut elements = Vec::new();
                for (index, (pattern, ty)) in patterns.iter().zip(types).enumerate() {
                    let projection = value(
                        TypedExprKind::Field(Box::new(matched.clone()), index),
                        self.inference.resolve(&ty),
                        pattern.span,
                    );
                    let (next, coverage) = self.pattern_alternatives(pattern, projection)?;
                    alternatives = combine(alternatives, next, pattern.span)?;
                    elements.push(coverage);
                }
                CoveragePat::Constructor(Constructor::Tuple(patterns.len()), elements)
            }
            PatternKind::Record(name, fields) => {
                let (id, arguments) = if let Some(name) = name {
                    let id = self.names.record(self.module, &name.text, name.span)?;
                    let arguments = (0..self.types.records[id].parameters.len())
                        .map(|_| self.inference.fresh())
                        .collect();
                    self.same(&matched.ty, &Type::Record(id, arguments), pattern.span)?;
                    matched.ty = self.inference.resolve(&matched.ty);
                    let Type::Record(id, arguments) = &matched.ty else {
                        unreachable!("the pattern type was unified with a record")
                    };
                    (*id, arguments.clone())
                } else if let Type::Record(id, arguments) = self.inference.resolve(&matched.ty) {
                    (id, arguments)
                } else {
                    return Err(Diagnostic::of(
                        DiagnosticKind::RecordPatternUnknownType,
                        "an unqualified record pattern needs a known record type",
                        pattern.span,
                    ));
                };
                self.record_storage(id, pattern.span)?;
                if let Some(name) = name {
                    self.note_name(name, NameTarget::Record(id));
                }
                let mut seen = BTreeSet::new();
                let field_count = self.types.records[id].fields.len();
                let mut coverage = vec![CoveragePat::Wildcard; field_count];
                for (name, pattern) in fields {
                    if !seen.insert(name.text.clone()) {
                        return Err(duplicate(name));
                    }
                    let index = self.types.records[id]
                        .fields
                        .iter()
                        .position(|(field, _)| field == &name.text)
                        .ok_or_else(|| {
                            Diagnostic::of(
                                DiagnosticKind::RecordHasNoField,
                                format!("record has no field '{}'", name.text),
                                name.span,
                            )
                        })?;
                    self.note_name(name, NameTarget::Field(id, index));
                    let ty = self
                        .inference
                        .resolve(&self.types.record_field(id, &arguments, index));
                    let projection = value(
                        TypedExprKind::Field(Box::new(matched.clone()), index),
                        ty,
                        pattern.span,
                    );
                    let (next, field) = self.pattern_alternatives(pattern, projection)?;
                    alternatives = combine(alternatives, next, pattern.span)?;
                    coverage[index] = field;
                }
                CoveragePat::Constructor(
                    Constructor::Record {
                        record_id: id,
                        field_count,
                    },
                    coverage,
                )
            }
            PatternKind::Array(patterns) | PatternKind::List(patterns) => {
                if matches!(self.inference.resolve(&matched.ty), Type::FixedArray(..)) {
                    return Err(Diagnostic::of(
                        DiagnosticKind::ArrayPatternOnFixedArray,
                        "fixed-length arrays cannot be destructured by patterns; index the elements instead",
                        pattern.span,
                    ));
                }
                let element = self.inference.fresh();
                let ty = if matches!(pattern.kind, PatternKind::List(_)) {
                    Type::List(Box::new(element.clone()))
                } else {
                    Type::Array(Box::new(element.clone()))
                };
                self.same(&matched.ty, &ty, pattern.span)?;
                matched.ty = self.inference.resolve(&matched.ty);
                alternatives[0]
                    .steps
                    .push(PatternStep::Test(Self::length_test(
                        &matched,
                        patterns.len(),
                        BinaryOp::Equal,
                        pattern.span,
                    )));
                let mut elements = Vec::new();
                for (index, pattern) in patterns.iter().enumerate() {
                    let index = value(TypedExprKind::Int(index as u128), Type::I64, pattern.span);
                    let projection = value(
                        TypedExprKind::Index(Box::new(matched.clone()), Box::new(index)),
                        self.inference.resolve(&element),
                        pattern.span,
                    );
                    let (next, coverage) = self.pattern_alternatives(pattern, projection)?;
                    alternatives = combine(alternatives, next, pattern.span)?;
                    elements.push(coverage);
                }
                if matches!(pattern.kind, PatternKind::List(_)) {
                    CoveragePat::list(elements)
                } else {
                    CoveragePat::Constructor(Constructor::ArrayLen(patterns.len()), elements)
                }
            }
            PatternKind::Cons(head, tail) => {
                let element = self.inference.fresh();
                self.same(
                    &matched.ty,
                    &Type::List(Box::new(element.clone())),
                    pattern.span,
                )?;
                matched.ty = self.inference.resolve(&matched.ty);
                alternatives[0]
                    .steps
                    .push(PatternStep::Test(Self::length_test(
                        &matched,
                        1,
                        BinaryOp::GreaterEqual,
                        pattern.span,
                    )));
                let index = value(TypedExprKind::Int(0), Type::I64, head.span);
                let projection = value(
                    TypedExprKind::Index(Box::new(matched.clone()), Box::new(index)),
                    self.inference.resolve(&element),
                    head.span,
                );
                let (next, head_coverage) = self.pattern_alternatives(head, projection)?;
                alternatives = combine(alternatives, next, head.span)?;
                let projection = value(
                    TypedExprKind::ListTail(Box::new(matched.clone()), 1),
                    matched.ty.clone(),
                    tail.span,
                );
                let (next, tail_coverage) = self.pattern_alternatives(tail, projection)?;
                alternatives = combine(alternatives, next, tail.span)?;
                CoveragePat::Constructor(Constructor::ListCons, vec![head_coverage, tail_coverage])
            }
        };
        Ok((alternatives, coverage))
    }

    /// Resolves a pattern name to a union case, which takes precedence over
    /// active recognizers and bindings. An unqualified case of another module
    /// that meets a recognizer of this module is ambiguous.
    fn pattern_case(&self, name: &Ident) -> Result<Option<(usize, usize, bool)>, Diagnostic> {
        let Some(case) = self.names.case_path(self.module, &name.text, name.span)? else {
            return Ok(None);
        };
        if !name.text.contains(['.', ':'])
            && case.info.module != self.module
            && self
                .names
                .has_active_pattern(self.module, &format!("{}.{}", self.module, name.text))
        {
            return Err(Diagnostic::of(
                DiagnosticKind::AmbiguousPatternName,
                format!(
                    "'{}' names both the union case '{}' and an active pattern of module '{}'; qualify the case as '{}'",
                    name.text, case.info.name, self.module, case.info.name
                ),
                name.span,
            ));
        }
        Ok(Some((case.info.id, case.case, case.has_payload)))
    }

    /// A case pattern tests the tag of the matched union, then matches the
    /// payload projection of that case against the payload pattern.
    fn case_pattern(
        &mut self,
        name: &Ident,
        (union_id, case_id, has_payload): (usize, usize, bool),
        arguments: &[Pattern],
        matched: TypedExpr,
    ) -> Result<(Vec<Alternative>, CoveragePat), Diagnostic> {
        self.note_case(name, union_id, case_id);
        let payload = match (has_payload, arguments) {
            (false, []) => None,
            (true, [payload]) => Some(payload),
            (true, []) => {
                return Err(Diagnostic::of(
                    DiagnosticKind::CasePatternMissingPayload,
                    format!(
                        "union case '{}' carries a payload; match it with a payload pattern such as '{} _'",
                        name.text, name.text
                    ),
                    name.span,
                ));
            }
            (false, _) => {
                return Err(Diagnostic::of(
                    DiagnosticKind::CasePatternUnexpectedPayload,
                    format!(
                        "union case '{}' has no payload; remove the patterns after it",
                        name.text
                    ),
                    name.span,
                ));
            }
            (true, _) => {
                return Err(Diagnostic::of(
                    DiagnosticKind::CasePatternPayloadShape,
                    format!(
                        "union case '{}' carries one payload; match several values with one tuple pattern such as '{} (a, b)'",
                        name.text, name.text
                    ),
                    name.span,
                ));
            }
        };
        self.union_case_pattern(name.span, union_id, case_id, payload, matched)
    }

    /// The tag test of case `case_id` of union `union_id` on the matched value (dereferenced),
    /// and the payload projection matched against `payload`: the shared part of a case pattern
    /// and a type test `:? T` (A24). `span` locates the test.
    fn union_case_pattern(
        &mut self,
        span: Span,
        union_id: usize,
        case_id: usize,
        payload: Option<&Pattern>,
        matched: TypedExpr,
    ) -> Result<(Vec<Alternative>, CoveragePat), Diagnostic> {
        let mut matched = Self::autoderef(matched);
        let arity = self.types.unions[union_id].parameters.len();
        let args: Box<[Type]> = (0..arity).map(|_| self.inference.fresh()).collect();
        self.same(&matched.ty, &Type::Union(union_id, args), span)?;
        matched.ty = self.inference.resolve(&matched.ty);
        let tag = Type::Integer(32, true);
        let test = value(
            TypedExprKind::Binary(
                BinaryOp::Equal,
                Box::new(value(
                    TypedExprKind::UnionTag(Box::new(matched.clone())),
                    tag.clone(),
                    span,
                )),
                Box::new(value(TypedExprKind::Int(case_id as u128), tag, span)),
            ),
            Type::Bool,
            span,
        );
        let alternatives = vec![Alternative {
            steps: vec![PatternStep::Test(test)],
            ..Alternative::default()
        }];
        let case = Constructor::Union { union_id, case_id };
        let Some(pattern) = payload else {
            return Ok((alternatives, CoveragePat::Constructor(case, Vec::new())));
        };
        let Type::Union(_, args) = &matched.ty else {
            unreachable!("the pattern type was unified with a union")
        };
        let ty = self
            .types
            .union_payload(union_id, args, case_id)
            .expect("the case carries a payload");
        let projection = value(
            TypedExprKind::UnionPayload {
                value: Box::new(matched.clone()),
                case_id,
            },
            self.inference.resolve(&ty),
            pattern.span,
        );
        let (next, coverage) = self.pattern_alternatives(pattern, projection)?;
        Ok((
            combine(alternatives, next, pattern.span)?,
            CoveragePat::Constructor(case, vec![coverage]),
        ))
    }

    /// A type test `:? T [as x]` (A24): the case of member `T` of the matched anonymous union
    /// (dereferenced), whose payload `as x` binds as a case payload does. The matched type must
    /// be known to be an anonymous union when the pattern is checked, and `T`, resolved as a type
    /// annotation of the body, one of its members, or an anonymous union of its members (A25,
    /// `narrowing_pattern`).
    #[inline(never)]
    fn type_test_pattern(
        &mut self,
        pattern: &Pattern,
        tested: &TypeExpr,
        binder: Option<&Ident>,
        matched: TypedExpr,
    ) -> Result<(Vec<Alternative>, CoveragePat), Diagnostic> {
        // The `:?` that starts the pattern.
        let marker = Span {
            end: pattern.span.start + 2,
            ..pattern.span
        };
        let mut target = self.inference.resolve(&Self::autoderef(matched.clone()).ty);
        // A number literal takes a number type, never an anonymous union: it is named by the type
        // it takes when nothing else decides it.
        if let Some(default) = self.inference.numeric_default(&target) {
            target = default;
        }
        let (union_id, members) = match &target {
            Type::Union(id, members) if self.names.is_anonymous_union(*id) => (*id, members),
            Type::Error => {
                return Ok((
                    vec![Alternative {
                        steps: vec![PatternStep::Test(TypedExpr::error(pattern.span))],
                        ..Alternative::default()
                    }],
                    CoveragePat::Opaque,
                ));
            }
            Type::Infer(_) => {
                return Err(Diagnostic::of(
                    DiagnosticKind::TypeTestUnknownScrutinee,
                    "the type of the matched value must be known to use ':?'",
                    marker,
                )
                .with_help("annotate the parameter or binding with its anonymous union type"));
            }
            other => {
                let error = Diagnostic::of(
                    DiagnosticKind::TypeTestNotAnonymousUnion,
                    format!(
                        "':?' tests a member of an anonymous union, but the matched value has type {}",
                        other.display(&self.types)
                    ),
                    marker,
                );
                // The help fits a named union only.
                return Err(if matches!(other, Type::Union(..)) {
                    error.with_help("match the cases of a named union by name")
                } else {
                    error
                });
            }
        };
        let ty = self.annotation(tested)?;
        let ty = self.inference.resolve(&ty);
        if matches!(&ty, Type::Union(id, _) if self.names.is_anonymous_union(*id)) {
            return self.narrowing_pattern(pattern, tested.span, binder, matched, &target, &ty);
        }
        let Some(case) = self.type_test_member(&ty, members, tested.span)? else {
            return Err(self.not_tested_member(&ty, &target, members, tested.span));
        };
        // The payload: the binder, or a wildcard (a type test covers the whole member).
        let payload = Pattern {
            kind: match binder {
                Some(name) if name.text != "_" => PatternKind::Binding(name.clone()),
                _ => PatternKind::Wildcard,
            },
            span: binder.map_or(pattern.span, |name| name.span),
            depth: pattern.depth,
        };
        self.union_case_pattern(pattern.span, union_id, case, Some(&payload), matched)
    }

    /// The member of `members` that the type `ty` after `:?` names: the one equal to it, or the
    /// only one that still holds an inference variable and may be `ty`, which becomes `ty`.
    fn type_test_member(
        &mut self,
        ty: &Type,
        members: &[Type],
        span: Span,
    ) -> Result<Option<usize>, Diagnostic> {
        if let Some(case) = members.iter().position(|member| member == ty) {
            return Ok(Some(case));
        }
        let fitting: Vec<usize> = members
            .iter()
            .enumerate()
            .filter(|(_, member)| self.may_member(ty, member))
            .map(|(case, _)| case)
            .collect();
        match fitting.as_slice() {
            [case] => {
                self.same(&members[*case], ty, span)?;
                Ok(Some(*case))
            }
            _ => Ok(None),
        }
    }

    #[cold]
    #[inline(never)]
    fn not_tested_member(
        &self,
        ty: &Type,
        target: &Type,
        members: &[Type],
        span: Span,
    ) -> Diagnostic {
        Diagnostic::of(
            DiagnosticKind::TypeTestNotMember,
            format!(
                "{} is not a member of {}",
                ty.display(&self.types),
                target.display(&self.types)
            ),
            span,
        )
        .with_help(anonymous_union::members_help(members, &self.types))
    }

    /// A type test `:? W [as x]` whose type `narrowed` is an anonymous union (A25): the matched
    /// value (dereferenced), of the anonymous union `target`, holds one of the members of `W`,
    /// each of which is a member of `target`. The alternatives test the tag for each member of
    /// `W` in turn, and the coverage is that of `:? W0 | :? W1 | ...`.
    ///
    /// `as x` binds the value as a value of `W`:
    ///
    /// - When `W` has every member of `target`, it is the matched value itself, bound as `x as`
    ///   binds it, with nothing to test.
    /// - Otherwise the value is made anew, with the tag of `W` (`retag`). In an arm without a
    ///   guard the alternative binds the matched value to a hidden name and records a
    ///   `Narrowing`, from which `match_value` binds `x` around the body: an owned payload moves
    ///   into the new value, and a payload in borrowed storage is copied. When every member of
    ///   `W` is Copy, the hidden binding views the matched value also where a binding would
    ///   move it, so that the narrowing leaves the value whole, as the arms `:? W0 as x`,
    ///   `:? W1 as x` do.
    /// - A guard reads `x` before the arm is chosen, when nothing may move out of the matched
    ///   value yet, so there a step of the pattern builds the new value from a copy of the
    ///   payload, as a recognizer's result is bound (`active_pattern`).
    ///
    /// A copy needs a Copy payload: `as x` in borrowed storage or under a guard is an error when
    /// a member of `W` is not Copy (`type_test_narrow_borrowed`).
    #[inline(never)]
    fn narrowing_pattern(
        &mut self,
        pattern: &Pattern,
        tested: Span,
        binder: Option<&Ident>,
        matched: TypedExpr,
        target: &Type,
        narrowed: &Type,
    ) -> Result<(Vec<Alternative>, CoveragePat), Diagnostic> {
        let (Type::Union(union_id, members), Type::Union(narrowed_id, wanted)) = (target, narrowed)
        else {
            unreachable!("both types are anonymous unions")
        };
        // (the case of `target`, the case of `W`) for each member of `W`.
        let mut cases: Vec<(usize, usize)> = Vec::with_capacity(wanted.len());
        for (into, member) in wanted.iter().enumerate() {
            let from = self
                .type_test_member(member, members, tested)?
                .filter(|from| cases.iter().all(|(other, _)| other != from));
            let Some(from) = from else {
                return Err(self.not_tested_member(member, target, members, tested));
            };
            cases.push((from, into));
        }
        let binder = binder.filter(|name| name.text != "_");
        let mut place = Self::autoderef(matched);
        place.ty = self.inference.resolve(&place.ty);
        if cases.len() == members.len() {
            let mut alternative = Alternative::default();
            if let Some(name) = binder {
                alternative.bindings.push((name.clone(), place));
            }
            return Ok((vec![alternative], CoveragePat::Wildcard));
        }
        let guarded = self.guarded_arm;
        let wanted: Vec<Type> = wanted
            .iter()
            .map(|member| self.inference.resolve(member))
            .collect();
        let moved = wanted.iter().find(|member| !member.is_copy(&self.types));
        let borrowed = self.borrowed_place(&place);
        if binder.is_some()
            && (guarded || borrowed)
            && let Some(member) = moved
        {
            let (narrowed, member) = (narrowed.display(&self.types), member.display(&self.types));
            // The `:?` that starts the pattern.
            let marker = Span {
                end: pattern.span.start + 2,
                ..pattern.span
            };
            // A guard is no obstacle that a borrowed value does not have as well.
            let (message, help) = if borrowed {
                (
                    format!(
                        "cannot narrow a borrowed value into {narrowed} because {member} is not Copy"
                    ),
                    "match each member with its own ':?' arm, or match an owned value",
                )
            } else {
                (
                    format!(
                        "cannot narrow a value into {narrowed} in an arm with a guard because {member} is not Copy"
                    ),
                    "match each member with its own ':?' arm, or test the condition in the arm's body",
                )
            };
            return Err(
                Diagnostic::of(DiagnosticKind::TypeTestNarrowBorrowed, message, marker)
                    .with_help(help),
            );
        }
        // Under a guard, the local that a step of each alternative fills with the new value.
        let built = match binder {
            Some(name) if guarded => Some(self.bind(
                &Ident {
                    text: format!("$narrowed{}", self.next_local),
                    span: name.span,
                    provenance: Provenance::Generated,
                },
                narrowed.clone(),
                false,
            )),
            _ => None,
        };
        let tag = Type::Integer(32, true);
        let mut alternatives = Vec::with_capacity(cases.len());
        let mut coverage: Option<CoveragePat> = None;
        for &(from, into) in &cases {
            let test = value(
                TypedExprKind::Binary(
                    BinaryOp::Equal,
                    Box::new(value(
                        TypedExprKind::UnionTag(Box::new(place.clone())),
                        tag.clone(),
                        pattern.span,
                    )),
                    Box::new(value(
                        TypedExprKind::Int(from as u128),
                        tag.clone(),
                        pattern.span,
                    )),
                ),
                Type::Bool,
                pattern.span,
            );
            let mut alternative = Alternative {
                steps: vec![PatternStep::Test(test)],
                ..Alternative::default()
            };
            match (binder, &built) {
                (Some(name), Some(built)) => {
                    let payload = value(
                        TypedExprKind::UnionPayload {
                            value: Box::new(place.clone()),
                            case_id: from,
                        },
                        wanted[into].clone(),
                        pattern.span,
                    );
                    let made = value(
                        TypedExprKind::Construct {
                            union_id: *narrowed_id,
                            case_id: into,
                            payload: Some(Box::new(payload)),
                        },
                        narrowed.clone(),
                        pattern.span,
                    );
                    alternative
                        .steps
                        .push(PatternStep::Bind(built.clone(), made));
                    alternative
                        .bindings
                        .push((name.clone(), local_value(built)));
                }
                (Some(name), None) => {
                    let hidden = narrowed_name(&name.text);
                    alternative.bindings.push((
                        Ident {
                            text: hidden.clone(),
                            span: name.span,
                            provenance: Provenance::Generated,
                        },
                        place.clone(),
                    ));
                    alternative.narrowed.push(Narrowing {
                        name: name.clone(),
                        hidden,
                        to: narrowed.clone(),
                        cases: cases.clone(),
                        view: moved.is_none() && !place.ty.is_copy(&self.types),
                        span: pattern.span,
                    });
                }
                (None, _) => {}
            }
            alternatives.push(alternative);
            let case = CoveragePat::Constructor(
                Constructor::Union {
                    union_id: *union_id,
                    case_id: from,
                },
                vec![CoveragePat::Wildcard],
            );
            coverage = Some(match coverage {
                Some(others) => CoveragePat::Or(Box::new(others), Box::new(case)),
                None => case,
            });
        }
        Ok((
            alternatives,
            coverage.expect("an anonymous union has members"),
        ))
    }

    fn active_recognizer(&self, name: &Ident) -> Result<Option<(usize, ActiveCase)>, Diagnostic> {
        self.names
            .active_pattern(self.module, &name.text, name.span)
    }

    fn active_argument(pattern: &Pattern) -> Result<Expr, Diagnostic> {
        let kind = match &pattern.kind {
            PatternKind::Binding(name) => {
                let mut segments = name.text.split('.');
                let root = segments.next().expect("a name has a first segment");
                let mut kind = ExprKind::Name(Ident {
                    text: root.into(),
                    span: name.span,
                    provenance: name.provenance,
                });
                for (depth, field) in segments.enumerate() {
                    let value = Expr {
                        kind,
                        span: name.span,
                        depth: depth + 1,
                    };
                    kind = ExprKind::Field(
                        Box::new(value),
                        Ident {
                            text: field.into(),
                            span: name.span,
                            provenance: name.provenance,
                        },
                    );
                }
                kind
            }
            PatternKind::Literal(expression) | PatternKind::Argument(expression) => {
                return Ok((**expression).clone());
            }
            PatternKind::Tuple(elements) => ExprKind::Tuple(
                elements
                    .iter()
                    .map(Self::active_argument)
                    .collect::<Result<_, _>>()?,
            ),
            _ => {
                return Err(Diagnostic::of(
                    DiagnosticKind::ActiveArgumentNotValue,
                    "a recognizer argument must be a value expression, not a binding pattern",
                    pattern.span,
                ));
            }
        };
        Ok(Expr {
            kind,
            span: pattern.span,
            depth: pattern.depth,
        })
    }

    fn active_pattern(
        &mut self,
        name: &Ident,
        arguments: &[Pattern],
        matched: TypedExpr,
    ) -> Result<(Vec<Alternative>, CoveragePat), Diagnostic> {
        let (id, active_case) = self.active_recognizer(name)?.ok_or_else(|| {
            let unknown = Diagnostic::of(
                DiagnosticKind::UnknownCaseOrActivePattern,
                format!("unknown union case or active pattern '{}'", name.text),
                name.span,
            );
            find::with_suggestions(unknown, &name.text, name.span, &self.pattern_pool())
        })?;
        let (kind, ty) = self.function(id);
        if ty.contains_error() {
            return Ok((
                vec![Alternative {
                    steps: vec![PatternStep::Test(TypedExpr::error(name.span))],
                    ..Alternative::default()
                }],
                CoveragePat::Opaque,
            ));
        }
        let Type::Function(parameters, result) = &ty else {
            unreachable!("recognizer signature checked")
        };
        let extras = parameters.len() - 1;
        let union_case = match active_case {
            ActiveCase::MaybePartial => {
                let Type::Union(union_id, _) = result.as_ref() else {
                    unreachable!("Maybe result checked")
                };
                Some((
                    *union_id,
                    self.types.unions[*union_id]
                        .cases
                        .iter()
                        .position(|(case, _)| case == "Some")
                        .unwrap(),
                ))
            }
            ActiveCase::TotalCase { index, .. } => {
                let Type::Union(union_id, _) = result.as_ref() else {
                    unreachable!("union result checked")
                };
                Some((*union_id, index))
            }
            _ => None,
        };
        let payload_type = match active_case {
            ActiveCase::BoolPartial => None,
            ActiveCase::TotalSingle => Some(result.as_ref().clone()),
            ActiveCase::MaybePartial | ActiveCase::TotalCase { .. } => {
                let Type::Union(union_id, args) = result.as_ref() else {
                    unreachable!()
                };
                self.types
                    .union_payload(*union_id, args, union_case.unwrap().1)
            }
        };
        let payload = if arguments.len() == extras + 1 && payload_type.is_some() {
            arguments.last()
        } else if arguments.len() == extras
            && payload_type.as_ref().is_none_or(|ty| *ty == Type::Unit)
        {
            None
        } else {
            return Err(Diagnostic::of(
                DiagnosticKind::ActivePatternArgumentCount,
                format!(
                    "recognizer '{}' expects {extras} arguments{}",
                    name.text,
                    match &payload_type {
                        Some(Type::Unit) => " and an optional unit payload pattern",
                        Some(_) => " followed by a payload pattern",
                        None => " and no payload pattern",
                    }
                ),
                name.span,
            ));
        };
        let mut values = Vec::new();
        for (argument, ty) in arguments[..extras].iter().zip(parameters.iter()) {
            values.push(self.argument(&Self::active_argument(argument)?, ty)?);
        }
        let input = self.inference.resolve(parameters.last().unwrap());
        let input = match input {
            Type::Reference(_, false) if matches!(matched.ty, Type::Reference(_, false)) => {
                self.same(&matched.ty, &input, name.span)?;
                matched
            }
            Type::Reference(inner, false) => {
                let matched = Self::autoderef(matched);
                self.same(&matched.ty, &inner, name.span)?;
                value(
                    TypedExprKind::Borrow(Box::new(matched), false),
                    Type::Reference(inner, false),
                    name.span,
                )
            }
            Type::Reference(_, true) => {
                return Err(Diagnostic::of(
                    DiagnosticKind::ActivePatternMutatesInput,
                    "active recognizers cannot mutate the matched input",
                    name.span,
                ));
            }
            input => {
                let matched = if polymorph::is_unknown(&input) {
                    matched
                } else {
                    Self::autoderef(matched)
                };
                self.same(&matched.ty, &input, name.span)?;
                matched
            }
        };
        values.push(input);
        let result_type = self.inference.resolve(result);
        let callee = value(kind, self.inference.resolve(&ty), name.span);
        let call = value(
            TypedExprKind::Call(Box::new(callee), values),
            result_type.clone(),
            name.span,
        );
        if active_case == ActiveCase::BoolPartial {
            return Ok((
                vec![Alternative {
                    steps: vec![PatternStep::Test(call)],
                    ..Alternative::default()
                }],
                CoveragePat::Opaque,
            ));
        }
        let local = self.bind(
            &Ident {
                text: format!("$recognizer{}", self.next_local),
                span: name.span,
                provenance: Provenance::Generated,
            },
            result_type,
            false,
        );
        let (mut alternatives, payload) = if let Some((union_id, case_id)) = union_case {
            let wildcard = Pattern {
                kind: PatternKind::Wildcard,
                span: name.span,
                depth: 1,
            };
            let has_payload = payload_type.is_some();
            let patterns = if has_payload {
                std::slice::from_ref(payload.unwrap_or(&wildcard))
            } else {
                &[]
            };
            let (alternatives, _) = self.case_pattern(
                name,
                (union_id, case_id, has_payload),
                patterns,
                local_value(&local),
            )?;
            (alternatives, None)
        } else if let Some(payload) = payload {
            let (alternatives, coverage) =
                self.pattern_alternatives(payload, local_value(&local))?;
            (alternatives, Some(Box::new(coverage)))
        } else {
            (vec![Alternative::default()], None)
        };
        for alternative in &mut alternatives {
            alternative
                .steps
                .insert(0, PatternStep::Bind(local.clone(), call.clone()));
        }
        let coverage = match active_case {
            ActiveCase::TotalSingle => CoveragePat::Total(payload),
            ActiveCase::TotalCase { index, count } => CoveragePat::ActiveCase {
                function: id,
                index,
                count,
            },
            _ => CoveragePat::Opaque,
        };
        Ok((alternatives, coverage))
    }

    fn length_test(matched: &TypedExpr, count: usize, operator: BinaryOp, span: Span) -> TypedExpr {
        let length = value(
            TypedExprKind::Length(Box::new(matched.clone())),
            Type::I64,
            span,
        );
        let count = value(TypedExprKind::Int(count as u128), Type::I64, span);
        value(
            TypedExprKind::Binary(operator, Box::new(length), Box::new(count)),
            Type::Bool,
            span,
        )
    }
}
