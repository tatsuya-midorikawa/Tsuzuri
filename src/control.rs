use super::exhaustiveness::{Constructor, CoverageArm, CoveragePat, MatchCoverage};
use super::*;

pub(super) const MAX_PATTERN_ALTERNATIVES: usize = 1024;

#[derive(Clone)]
struct Alternative {
    steps: Vec<PatternStep>,
    bindings: Vec<(Ident, TypedExpr)>,
}

fn value(kind: TypedExprKind, ty: Type, span: Span) -> TypedExpr {
    TypedExpr { kind, ty, span }
}

fn local_value(local: &Local) -> TypedExpr {
    value(TypedExprKind::Local(local.id), local.ty.clone(), local.span)
}

/// Matches the place classification of ownership and LLVM lowering, which
/// view such a match subject in place instead of binding a temporary.
fn is_place(expression: &TypedExpr) -> bool {
    match &expression.kind {
        TypedExprKind::Local(_) | TypedExprKind::Dereference(_) => true,
        TypedExprKind::Field(value, _)
        | TypedExprKind::ListTail(value, _)
        | TypedExprKind::UnionPayload { value, .. } => is_place(value),
        TypedExprKind::Index(value, _) => {
            matches!(value.ty, Type::Array(_) | Type::List(_)) && is_place(value)
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
        return Err(Diagnostic::new(
            "E1017",
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
            ExprKind::While { condition, body } => TypedExprKind::While {
                condition: Box::new(self.expression(condition, Some(&Type::Bool))?),
                body: Box::new(self.expression(body, Some(&Type::Unit))?),
            },
            ExprKind::For {
                pattern,
                source,
                body,
            } => {
                let (source, range, element) = if let ExprKind::Range {
                    start,
                    step,
                    finish,
                    counted,
                    descending,
                } = &source.kind
                {
                    let ty = if *counted {
                        Type::Integer(32, true)
                    } else {
                        self.inference.fresh()
                    };
                    let start = self.expression(start, Some(&ty))?;
                    let step = if let Some(step) = step {
                        self.expression(step, Some(&ty))?
                    } else {
                        let (kind, ty) =
                            self.integer(1, None, Some(&ty), *descending, source.span)?;
                        value(kind, ty, source.span)
                    };
                    let finish = self.expression(finish, Some(&ty))?;
                    self.require("Integer", ty.clone(), source.span)?;
                    (
                        None,
                        Some((start, step, finish)),
                        self.inference.resolve(&ty),
                    )
                } else {
                    let source = Self::autoderef(self.expression(source, None)?);
                    if source.ty.contains_error() {
                        return Ok(TypedExpr::error(expression.span));
                    }
                    let element = match &source.ty {
                        Type::Array(element) | Type::List(element) => (**element).clone(),
                        Type::String => Type::Integer(16, false),
                        Type::Utf8String => Type::Integer(8, false),
                        _ => {
                            return Err(Diagnostic::new(
                                "E1005",
                                "for...in expects an array, list, string, utf8string, or integer range",
                                source.span,
                            ));
                        }
                    };
                    (Some(source), None, element)
                };
                self.scopes.push(BTreeMap::new());
                let simple_binding = matches!(&pattern.kind, PatternKind::Binding(name)
                    if self.active_recognizer(name).is_none()
                        && matches!(self.names.case_path(self.module, &name.text, name.span), Ok(None)));
                let name = match &pattern.kind {
                    PatternKind::Binding(name) if simple_binding => name.clone(),
                    _ => Ident {
                        text: format!("$iteration{}", pattern.span.start),
                        span: pattern.span,
                    },
                };
                let local = self.bind(&name, element, false);
                if source.is_some() {
                    self.borrowed.insert(local.id);
                }
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
                        false,
                    )?
                };
                self.scopes.pop();
                if let Some((start, step, finish)) = range {
                    TypedExprKind::ForRange {
                        local,
                        start: Box::new(start),
                        step: Box::new(step),
                        finish: Box::new(finish),
                        body: Box::new(body),
                    }
                } else {
                    let source = source.unwrap();
                    let owner = self.bind(
                        &Ident {
                            text: "$enumerable".into(),
                            span: source.span,
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
                    origin.checks_coverage(),
                );
            }
            _ => unreachable!("control expression kinds checked by expression"),
        };
        if let Some(expected) = expected {
            self.same(&Type::Unit, expected, expression.span)?;
        }
        Ok(value(kind, Type::Unit, expression.span))
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
                matches!(value.ty, Type::Array(_) | Type::List(_)) && is_place(value)
            }
            TypedExprKind::Field(value, _) | TypedExprKind::UnionPayload { value, .. } => {
                self.borrowed_place(value)
            }
            _ => false,
        }
    }

    /// Checks the arms of a match. With `checked`, the arms' coverage is
    /// recorded so that `check_coverage` can reject a non-exhaustive match and
    /// warn about unreachable arms; destructuring keeps its runtime trap.
    fn match_value(
        &mut self,
        matched: TypedExpr,
        arms: &[MatchArm],
        expected: Option<&Type>,
        span: Span,
        checked: bool,
    ) -> Result<TypedExpr, Diagnostic> {
        if matched.ty.contains_error() {
            return Ok(TypedExpr::error(span));
        }
        let slot = checked.then(|| {
            self.coverage.push(MatchCoverage {
                span,
                arms: Vec::new(),
            });
            self.coverage.len() - 1
        });
        let mut coverage = Vec::new();
        self.scopes.push(BTreeMap::new());
        let subject = self.bind(
            &Ident {
                text: format!("$match{}", span.start),
                span,
            },
            matched.ty.clone(),
            false,
        );
        if self.borrowed_place(&matched) {
            self.borrowed.insert(subject.id);
        }
        let mut checked = Vec::new();
        let mut result = expected.cloned();
        for arm in arms {
            self.scopes.push(BTreeMap::new());
            let (alternatives, pattern) =
                self.pattern_alternatives(&arm.pattern, local_value(&subject))?;
            coverage.push(CoverageArm {
                pattern,
                guarded: arm.guard.is_some(),
                span: arm.pattern.span,
            });
            let mut locals: BTreeMap<String, Local> = BTreeMap::new();
            let mut plans = Vec::new();
            let mut borrowed = BTreeSet::new();
            for (index, alternative) in alternatives.into_iter().enumerate() {
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
                        let local = locals
                            .get(&name.text)
                            .ok_or_else(|| {
                                Diagnostic::new(
                                    "E1020",
                                    "both sides of an OR pattern must bind the same names",
                                    name.span,
                                )
                            })?
                            .clone();
                        self.same(&projection.ty, &local.ty, name.span)?;
                        local
                    };
                    if self.borrowed_place(&projection) {
                        borrowed.insert(local.id);
                    }
                    bindings.push((local, projection));
                }
                if seen.len() != locals.len() {
                    return Err(Diagnostic::new(
                        "E1020",
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
            let guard = arm
                .guard
                .as_ref()
                .map(|guard| self.expression(guard, Some(&Type::Bool)))
                .transpose()?;
            let body = self.expression(&arm.body, result.as_ref())?;
            result = Some(body.ty.clone());
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
        let mut alternatives = vec![Alternative {
            steps: Vec::new(),
            bindings: Vec::new(),
        }];
        let coverage = match &pattern.kind {
            PatternKind::Wildcard => CoveragePat::Wildcard,
            PatternKind::Binding(name) => {
                if let Some(case) = self.pattern_case(name)? {
                    return self.case_pattern(name, case, &[], matched);
                }
                if self.active_recognizer(name).is_some() {
                    return self.active_pattern(name, &[], matched);
                }
                if name.text.contains('.') {
                    return Err(Diagnostic::new(
                        "E1020",
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
                return Err(Diagnostic::new(
                    "E1020",
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
                    return Err(Diagnostic::new(
                        "E1017",
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
                    return Err(Diagnostic::new(
                        "E1020",
                        "an unqualified record pattern needs a known record type",
                        pattern.span,
                    ));
                };
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
                            Diagnostic::new(
                                "E1007",
                                format!("record has no field '{}'", name.text),
                                name.span,
                            )
                        })?;
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
        if !name.text.contains('.')
            && case.info.module != self.module
            && self.active_recognizer(name).is_some()
        {
            return Err(Diagnostic::new(
                "E1004",
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
        let payload = match (has_payload, arguments) {
            (false, []) => None,
            (true, [payload]) => Some(payload),
            (true, []) => {
                return Err(Diagnostic::new(
                    "E1020",
                    format!(
                        "union case '{}' carries a payload; match it with a payload pattern such as '{} _'",
                        name.text, name.text
                    ),
                    name.span,
                ));
            }
            (false, _) => {
                return Err(Diagnostic::new(
                    "E1020",
                    format!(
                        "union case '{}' has no payload; remove the patterns after it",
                        name.text
                    ),
                    name.span,
                ));
            }
            (true, _) => {
                return Err(Diagnostic::new(
                    "E1020",
                    format!(
                        "union case '{}' carries one payload; match several values with one tuple pattern such as '{} (a, b)'",
                        name.text, name.text
                    ),
                    name.span,
                ));
            }
        };
        let mut matched = Self::autoderef(matched);
        let arity = self.types.unions[union_id].parameters.len();
        let args: Box<[Type]> = (0..arity).map(|_| self.inference.fresh()).collect();
        self.same(&matched.ty, &Type::Union(union_id, args), name.span)?;
        matched.ty = self.inference.resolve(&matched.ty);
        let tag = Type::Integer(32, true);
        let test = value(
            TypedExprKind::Binary(
                BinaryOp::Equal,
                Box::new(value(
                    TypedExprKind::UnionTag(Box::new(matched.clone())),
                    tag.clone(),
                    name.span,
                )),
                Box::new(value(TypedExprKind::Int(case_id as u128), tag, name.span)),
            ),
            Type::Bool,
            name.span,
        );
        let alternatives = vec![Alternative {
            steps: vec![PatternStep::Test(test)],
            bindings: Vec::new(),
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

    fn active_recognizer(&self, name: &Ident) -> Option<String> {
        let local = format!("{}.{}", self.module, name.text);
        if self.names.has_active_pattern(self.module, &local) {
            return Some(local);
        }
        self.names
            .has_active_pattern(self.module, &name.text)
            .then(|| name.text.clone())
    }

    fn active_argument(pattern: &Pattern) -> Result<Expr, Diagnostic> {
        let kind = match &pattern.kind {
            PatternKind::Binding(name) => {
                let mut segments = name.text.split('.');
                let root = segments.next().expect("a name has a first segment");
                let mut kind = ExprKind::Name(Ident {
                    text: root.into(),
                    span: name.span,
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
                return Err(Diagnostic::new(
                    "E1020",
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
        let recognizer = self.active_recognizer(name).ok_or_else(|| {
            Diagnostic::new(
                "E1020",
                format!("unknown union case or active pattern '{}'", name.text),
                name.span,
            )
        })?;
        let (id, partial) = self
            .names
            .active_pattern(self.module, &recognizer, name.span)?
            .expect("active recognizer exists");
        let (kind, ty) = self.function(id);
        if ty.contains_error() {
            return Ok((
                vec![Alternative {
                    steps: vec![PatternStep::Test(TypedExpr::error(name.span))],
                    bindings: Vec::new(),
                }],
                CoveragePat::Opaque,
            ));
        }
        let Type::Function(parameters, result) = &ty else {
            unreachable!("recognizer signature checked")
        };
        let extras = parameters.len() - 1;
        let payload = if partial {
            if arguments.len() != extras {
                return Err(Diagnostic::new(
                    "E1006",
                    format!(
                        "partial recognizer '{}' expects {extras} arguments and has no payload",
                        name.text
                    ),
                    name.span,
                ));
            }
            None
        } else if arguments.len() == extras + 1 {
            arguments.last()
        } else if arguments.len() == extras && **result == Type::Unit {
            None
        } else {
            return Err(Diagnostic::new(
                "E1006",
                format!(
                    "total recognizer '{}' expects {extras} arguments followed by a payload pattern",
                    name.text
                ),
                name.span,
            ));
        };
        let mut values = Vec::new();
        for (argument, ty) in arguments[..extras].iter().zip(parameters) {
            values.push(self.expression(&Self::active_argument(argument)?, Some(ty))?);
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
                return Err(Diagnostic::new(
                    "E1014",
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
        if partial {
            return Ok((
                vec![Alternative {
                    steps: vec![PatternStep::Test(call)],
                    bindings: Vec::new(),
                }],
                CoveragePat::Opaque,
            ));
        }
        let local = self.bind(
            &Ident {
                text: format!("$recognizer{}", self.next_local),
                span: name.span,
            },
            result_type,
            false,
        );
        let (mut alternatives, payload) = if let Some(payload) = payload {
            let (alternatives, coverage) =
                self.pattern_alternatives(payload, local_value(&local))?;
            (alternatives, Some(Box::new(coverage)))
        } else {
            (
                vec![Alternative {
                    steps: Vec::new(),
                    bindings: Vec::new(),
                }],
                None,
            )
        };
        for alternative in &mut alternatives {
            alternative
                .steps
                .insert(0, PatternStep::Bind(local.clone(), call.clone()));
        }
        Ok((alternatives, CoveragePat::Total(payload)))
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
