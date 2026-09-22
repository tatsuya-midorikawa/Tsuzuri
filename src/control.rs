use super::*;

const MAX_PATTERN_ALTERNATIVES: usize = 1024;

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
                    let element = match &source.ty {
                        Type::Array(element) | Type::List(element) => (**element).clone(),
                        Type::String => Type::Integer(8, false),
                        _ => {
                            return Err(Diagnostic::new(
                                "E1005",
                                "for...in expects an array, list, UTF-8 string, or integer range",
                                source.span,
                            ));
                        }
                    };
                    (Some(source), None, element)
                };
                self.scopes.push(BTreeMap::new());
                let simple_binding = matches!(&pattern.kind, PatternKind::Binding(name) if self.active_recognizer(name).is_none());
                let name = match &pattern.kind {
                    PatternKind::Binding(name) if simple_binding => name.clone(),
                    _ => Ident {
                        text: format!("$iteration{}", pattern.span.start),
                        span: pattern.span,
                    },
                };
                let local = self.bind(&name, element, false);
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
            ExprKind::Match { value, arms } => {
                let value = self.expression(value, None)?;
                return self.match_value(value, arms, expected, expression.span);
            }
            _ => unreachable!("control expression kinds checked by expression"),
        };
        if let Some(expected) = expected {
            self.same(&Type::Unit, expected, expression.span)?;
        }
        Ok(value(kind, Type::Unit, expression.span))
    }

    fn match_value(
        &mut self,
        matched: TypedExpr,
        arms: &[MatchArm],
        expected: Option<&Type>,
        span: Span,
    ) -> Result<TypedExpr, Diagnostic> {
        self.scopes.push(BTreeMap::new());
        let subject = self.bind(
            &Ident {
                text: format!("$match{}", span.start),
                span,
            },
            matched.ty.clone(),
            false,
        );
        let mut checked = Vec::new();
        let mut result = expected.cloned();
        for arm in arms {
            self.scopes.push(BTreeMap::new());
            let alternatives = self.pattern_alternatives(&arm.pattern, local_value(&subject))?;
            let mut locals: BTreeMap<String, Local> = BTreeMap::new();
            let mut plans = Vec::new();
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
            });
        }
        self.scopes.pop();
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

    fn pattern_alternatives(
        &mut self,
        pattern: &Pattern,
        mut matched: TypedExpr,
    ) -> Result<Vec<Alternative>, Diagnostic> {
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
        match &pattern.kind {
            PatternKind::Wildcard => {}
            PatternKind::Binding(name) => {
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
            }
            PatternKind::Apply(name, arguments) => {
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
                alternatives[0].steps.push(PatternStep::Test(value(
                    TypedExprKind::Binary(BinaryOp::Equal, Box::new(matched), Box::new(literal)),
                    Type::Bool,
                    pattern.span,
                )));
            }
            PatternKind::Annotated(inner, annotation) => {
                let ty = self.annotation(annotation)?;
                self.same(&matched.ty, &ty, pattern.span)?;
                matched.ty = self.inference.resolve(&ty);
                return self.pattern_alternatives(inner, matched);
            }
            PatternKind::As(inner, name) => {
                let mut alternatives = self.pattern_alternatives(inner, matched.clone())?;
                for alternative in &mut alternatives {
                    alternative.bindings.push((name.clone(), matched.clone()));
                }
                return Ok(alternatives);
            }
            PatternKind::Or(left, right) => {
                let mut alternatives = self.pattern_alternatives(left, matched.clone())?;
                alternatives.extend(self.pattern_alternatives(right, matched)?);
                if alternatives.len() > MAX_PATTERN_ALTERNATIVES {
                    return Err(Diagnostic::new(
                        "E1017",
                        "too many OR pattern alternatives",
                        pattern.span,
                    ));
                }
                return Ok(alternatives);
            }
            PatternKind::And(left, right) => {
                let left = self.pattern_alternatives(left, matched.clone())?;
                let right = self.pattern_alternatives(right, matched)?;
                return combine(left, right, pattern.span);
            }
            PatternKind::Tuple(patterns) => {
                let types: Vec<_> = patterns.iter().map(|_| self.inference.fresh()).collect();
                self.same(&matched.ty, &Type::Tuple(types.clone()), pattern.span)?;
                matched.ty = self.inference.resolve(&matched.ty);
                for (index, (pattern, ty)) in patterns.iter().zip(types).enumerate() {
                    let projection = value(
                        TypedExprKind::Field(Box::new(matched.clone()), index),
                        self.inference.resolve(&ty),
                        pattern.span,
                    );
                    let next = self.pattern_alternatives(pattern, projection)?;
                    alternatives = combine(alternatives, next, pattern.span)?;
                }
            }
            PatternKind::Record(name, fields) => {
                let id = if let Some(name) = name {
                    let id = self.names.record(self.module, &name.text, name.span)?;
                    self.same(&matched.ty, &Type::Record(id), pattern.span)?;
                    matched.ty = Type::Record(id);
                    id
                } else if let Type::Record(id) = matched.ty {
                    id
                } else {
                    return Err(Diagnostic::new(
                        "E1020",
                        "an unqualified record pattern needs a known record type",
                        pattern.span,
                    ));
                };
                let mut seen = BTreeSet::new();
                for (name, pattern) in fields {
                    if !seen.insert(name.text.clone()) {
                        return Err(duplicate(name));
                    }
                    let index = self.records[id]
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
                    let ty = self.records[id].fields[index].1.clone();
                    let projection = value(
                        TypedExprKind::Field(Box::new(matched.clone()), index),
                        ty,
                        pattern.span,
                    );
                    let next = self.pattern_alternatives(pattern, projection)?;
                    alternatives = combine(alternatives, next, pattern.span)?;
                }
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
                for (index, pattern) in patterns.iter().enumerate() {
                    let index = value(TypedExprKind::Int(index as u128), Type::I64, pattern.span);
                    let projection = value(
                        TypedExprKind::Index(Box::new(matched.clone()), Box::new(index)),
                        self.inference.resolve(&element),
                        pattern.span,
                    );
                    let next = self.pattern_alternatives(pattern, projection)?;
                    alternatives = combine(alternatives, next, pattern.span)?;
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
                let next = self.pattern_alternatives(head, projection)?;
                alternatives = combine(alternatives, next, head.span)?;
                let projection = value(
                    TypedExprKind::ListTail(Box::new(matched.clone()), 1),
                    matched.ty.clone(),
                    tail.span,
                );
                let next = self.pattern_alternatives(tail, projection)?;
                alternatives = combine(alternatives, next, tail.span)?;
            }
        }
        Ok(alternatives)
    }

    fn active_recognizer(&self, name: &Ident) -> Option<(usize, bool)> {
        self.names
            .active_patterns
            .get(&format!("{}.{}", self.module, name.text))
            .or_else(|| self.names.active_patterns.get(&name.text))
            .copied()
    }

    fn active_argument(pattern: &Pattern) -> Result<Expr, Diagnostic> {
        let kind = match &pattern.kind {
            PatternKind::Binding(name) => {
                if let Some((root, field)) = name.text.split_once('.') {
                    let root = Expr {
                        kind: ExprKind::Name(Ident {
                            text: root.into(),
                            span: name.span,
                        }),
                        span: name.span,
                        depth: 1,
                    };
                    ExprKind::Field(
                        Box::new(root),
                        Ident {
                            text: field.into(),
                            span: name.span,
                        },
                    )
                } else {
                    ExprKind::Name(name.clone())
                }
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
    ) -> Result<Vec<Alternative>, Diagnostic> {
        let (id, partial) = self.active_recognizer(name).ok_or_else(|| {
            Diagnostic::new(
                "E1020",
                format!("unknown active pattern '{}'", name.text),
                name.span,
            )
        })?;
        let (kind, ty) = self.function(id);
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
            return Ok(vec![Alternative {
                steps: vec![PatternStep::Test(call)],
                bindings: Vec::new(),
            }]);
        }
        let local = self.bind(
            &Ident {
                text: format!("$recognizer{}", self.next_local),
                span: name.span,
            },
            result_type,
            false,
        );
        let mut alternatives = if let Some(payload) = payload {
            self.pattern_alternatives(payload, local_value(&local))?
        } else {
            vec![Alternative {
                steps: Vec::new(),
                bindings: Vec::new(),
            }]
        };
        for alternative in &mut alternatives {
            alternative
                .steps
                .insert(0, PatternStep::Bind(local.clone(), call.clone()));
        }
        Ok(alternatives)
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
