use super::*;
use std::cell::Cell;

pub(super) fn instances(
    modules: &[ModuleInput<'_>],
    names: &Names,
    types: &TypeContext<'_>,
) -> Result<Vec<(String, InstanceDecl)>, Diagnostic> {
    let mut output = Vec::new();
    for input in modules {
        for record in &input.program.records {
            let ty = declared_type(input.name, &record.name, &record.parameters, names)?;
            let Type::Record(id, arguments) = &ty else {
                unreachable!()
            };
            for &(class, span) in &record.derives {
                let build = Build {
                    span,
                    nodes: Cell::new(0),
                    module: input.name,
                };
                let methods = build.record(record, class)?;
                output.push((
                    input.name.to_owned(),
                    build.instance(
                        class,
                        &ty,
                        types.record_fields(*id, arguments),
                        methods,
                        types,
                    ),
                ));
            }
        }
        for union in &input.program.unions {
            let ty = declared_type(input.name, &union.name, &union.parameters, names)?;
            let Type::Union(id, arguments) = &ty else {
                unreachable!()
            };
            for &(class, span) in &union.derives {
                let build = Build {
                    span,
                    nodes: Cell::new(0),
                    module: input.name,
                };
                let methods = build.union(union, class)?;
                let payloads = types.union_payloads(*id, arguments);
                let count = if class == DeriveClass::Default {
                    1
                } else {
                    payloads.len()
                };
                output.push((
                    input.name.to_owned(),
                    build.instance(
                        class,
                        &ty,
                        payloads.into_iter().take(count).flatten().collect(),
                        methods,
                        types,
                    ),
                ));
            }
        }
    }
    if output.len() > 1024 {
        return Err(Diagnostic::new(
            "E1017",
            "deriving exceeds 1024 instances",
            output[1024].1.class.span,
        ));
    }
    Ok(output)
}

fn declared_type(
    module: &str,
    name: &Ident,
    parameters: &[Ident],
    names: &Names,
) -> Result<Type, Diagnostic> {
    let head = Ident {
        text: format!("{module}.{}", name.text),
        ..name.clone()
    };
    let kind = if parameters.is_empty() {
        TypeExprKind::Named(head.text)
    } else {
        TypeExprKind::Apply(
            Box::new(head),
            parameters
                .iter()
                .map(|parameter| TypeExpr {
                    kind: TypeExprKind::Variable(parameter.text.clone()),
                    span: parameter.span,
                })
                .collect(),
        )
    };
    resolve_type(
        &TypeExpr {
            kind,
            span: name.span,
        },
        module,
        names,
    )
}

struct Build<'a> {
    module: &'a str,
    span: Span,
    nodes: Cell<usize>,
}

impl Build<'_> {
    fn ident(&self, name: impl Into<String>) -> Ident {
        Ident {
            text: name.into(),
            span: self.span,
            provenance: Provenance::Generated,
        }
    }

    fn make(&self, kind: ExprKind) -> Result<Expr, Diagnostic> {
        use ExprKind::*;
        let depth = 1 + match &kind {
            Name(_) | Integer(..) | Bool(_) | String(_) => 0,
            Field(value, _) | Borrow(value, ..) | Unary(_, value) => value.depth,
            Binary(_, left, right) => left.depth.max(right.depth),
            Call(callee, arguments) => arguments
                .iter()
                .map(|value| value.depth)
                .max()
                .unwrap_or(0)
                .max(callee.depth),
            If {
                condition,
                then_branch,
                else_branch,
            } => condition
                .depth
                .max(then_branch.depth)
                .max(else_branch.depth),
            Block { bindings, result } => bindings
                .iter()
                .map(|binding| binding.value.depth)
                .max()
                .unwrap_or(0)
                .max(result.depth),
            Match { value, arms, .. } => arms
                .iter()
                .map(|arm| arm.body.depth.max(arm.pattern.depth))
                .max()
                .unwrap_or(0)
                .max(value.depth),
            Record { fields, .. } => fields
                .iter()
                .map(|(_, value)| value.depth)
                .max()
                .unwrap_or(0),
            _ => unreachable!(),
        };
        self.nodes.set(self.nodes.get() + 1);
        if depth > MAX_NESTING || self.nodes.get() > 4096 {
            return Err(Diagnostic::new(
                "E1017",
                "deriving exceeds the expression depth or node limit",
                self.span,
            ));
        }
        Ok(Expr {
            kind,
            span: self.span,
            depth,
        })
    }

    fn name(&self, name: &str) -> Result<Expr, Diagnostic> {
        self.make(ExprKind::Name(self.ident(name)))
    }
    fn boolean(&self, value: bool) -> Result<Expr, Diagnostic> {
        self.make(ExprKind::Bool(value))
    }
    fn integer(&self, value: usize) -> Result<Expr, Diagnostic> {
        self.make(ExprKind::Integer(value as u128, Some("i64".into())))
    }
    fn word(&self, value: u64) -> Result<Expr, Diagnostic> {
        self.make(ExprKind::Integer(value.into(), Some("i64u".into())))
    }
    fn text(&self, value: &str) -> Result<Expr, Diagnostic> {
        self.make(ExprKind::String(StringLiteral::Utf16(
            value.encode_utf16().collect(),
        )))
    }
    fn quoted(&self, value: Expr) -> Result<Expr, Diagnostic> {
        self.make(ExprKind::Call(
            Box::new(self.name("$builtin.display_quoted")?),
            vec![value],
        ))
    }
    fn mix(&self, state: Expr, word: Expr) -> Result<Expr, Diagnostic> {
        self.make(ExprKind::Call(
            Box::new(self.name("$builtin.hash_mix")?),
            vec![state, word],
        ))
    }
    fn hash_start(&self, tag: u64, count: u64) -> Result<Expr, Diagnostic> {
        self.mix(
            self.mix(self.word(14695981039346656037)?, self.word(tag)?)?,
            self.word(count)?,
        )
    }
    fn binary(&self, operator: BinaryOp, left: Expr, right: Expr) -> Result<Expr, Diagnostic> {
        self.make(ExprKind::Binary(operator, Box::new(left), Box::new(right)))
    }
    fn conditional(
        &self,
        condition: Expr,
        then_branch: Expr,
        else_branch: Expr,
    ) -> Result<Expr, Diagnostic> {
        self.make(ExprKind::If {
            condition: Box::new(condition),
            then_branch: Box::new(then_branch),
            else_branch: Box::new(else_branch),
        })
    }
    fn call(&self, class: &str, method: &str, arguments: Vec<Expr>) -> Result<Expr, Diagnostic> {
        let callee = self.make(ExprKind::Field(
            Box::new(self.name(&format!("$class.{class}"))?),
            self.ident(method),
        ))?;
        self.make(ExprKind::Call(Box::new(callee), arguments))
    }
    fn field(&self, owner: &str, name: &str) -> Result<Expr, Diagnostic> {
        let field = self.make(ExprKind::Field(
            Box::new(self.name(owner)?),
            self.ident(name),
        ))?;
        self.make(ExprKind::Borrow(Box::new(field), false, Notation::Keyword))
    }
    fn definition(&self, name: &str, parameters: &[&str], body: Expr) -> Definition {
        Definition {
            name: self.ident(name),
            recursion: Some(name.into()),
            parameters: parameters
                .iter()
                .map(|name| (self.ident(*name), false))
                .collect(),
            body,
        }
    }
    fn instance(
        &self,
        class: DeriveClass,
        ty: &Type,
        components: Vec<Type>,
        methods: Vec<Definition>,
        types: &TypeContext<'_>,
    ) -> InstanceDecl {
        let constraints = components
            .into_iter()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .map(|ty| ConstraintExpr {
                name: ConstraintName::Class(self.ident(class.name())),
                ty: polymorph::type_expression(&ty, types, self.span),
            })
            .collect();
        InstanceDecl {
            class: self.ident(class.name()),
            ty: polymorph::type_expression(ty, types, self.span),
            constraints,
            methods,
        }
    }
    fn operators(
        &self,
        class: DeriveClass,
    ) -> Result<&'static [(&'static str, BinaryOp)], Diagnostic> {
        use BinaryOp::*;
        match class {
            DeriveClass::Eq => Ok(&[("eq", Equal), ("ne", NotEqual)]),
            DeriveClass::Ord => Ok(&[
                ("lt", Less),
                ("le", LessEqual),
                ("gt", Greater),
                ("ge", GreaterEqual),
            ]),
            _ => Err(Diagnostic::new(
                "E1025",
                "this deriving implementation is not available yet",
                self.span,
            )),
        }
    }
    fn negate_equal(&self) -> Result<Expr, Diagnostic> {
        self.make(ExprKind::Unary(
            UnaryOp::Not,
            Box::new(self.call("Eq", "eq", vec![self.name("$left")?, self.name("$right")?])?),
        ))
    }
    fn record(
        &self,
        record: &RecordDecl,
        class: DeriveClass,
    ) -> Result<Vec<Definition>, Diagnostic> {
        if class == DeriveClass::Display {
            let mut text = self.text(&format!("{} {{", record.name.text))?;
            let mut bindings = Vec::new();
            for (index, field) in record.fields.iter().enumerate() {
                let label = self.text(&format!(
                    "{}{}: ",
                    if index == 0 { " " } else { ", " },
                    field.name.text
                ))?;
                let value = self.binary(
                    BinaryOp::Add,
                    self.binary(BinaryOp::Add, text, label)?,
                    self.quoted(self.field("$value", &field.name.text)?)?,
                )?;
                let name = format!("$text{index}");
                bindings.push(Binding {
                    name: self.ident(&name),
                    mutable: false,
                    annotation: None,
                    value,
                });
                text = self.name(&name)?;
            }
            let result = self.binary(BinaryOp::Add, text, self.text(" }")?)?;
            let body = self.make(ExprKind::Block {
                bindings,
                result: Box::new(result),
            })?;
            return Ok(vec![self.definition("display", &["$value"], body)]);
        }
        if class == DeriveClass::Hash {
            let mut state = self.hash_start(0x52, record.fields.len() as u64)?;
            let mut bindings = Vec::new();
            for (index, field) in record.fields.iter().enumerate() {
                let value = self.mix(
                    self.mix(state, self.word(index as u64)?)?,
                    self.call(
                        "Hash",
                        "hash",
                        vec![self.field("$value", &field.name.text)?],
                    )?,
                )?;
                let name = format!("$hash{index}");
                bindings.push(Binding {
                    name: self.ident(&name),
                    mutable: false,
                    annotation: None,
                    value,
                });
                state = self.name(&name)?;
            }
            let body = self.make(ExprKind::Block {
                bindings,
                result: Box::new(state),
            })?;
            return Ok(vec![self.definition("hash", &["$value"], body)]);
        }
        if class == DeriveClass::Default {
            let fields = record
                .fields
                .iter()
                .map(|field| {
                    Ok((
                        self.ident(&field.name.text),
                        self.call("Default", "default", Vec::new())?,
                    ))
                })
                .collect::<Result<_, Diagnostic>>()?;
            let body = self.make(ExprKind::Record {
                name: Box::new(self.ident(format!("{}.{}", self.module, record.name.text))),
                fields,
            })?;
            return Ok(vec![self.definition("default", &[], body)]);
        }
        self.operators(class)?
            .iter()
            .map(|&(method, operator)| {
                let mut body = self.boolean(matches!(
                    operator,
                    BinaryOp::Equal | BinaryOp::LessEqual | BinaryOp::GreaterEqual
                ))?;
                if operator == BinaryOp::NotEqual {
                    body = self.negate_equal()?;
                } else {
                    for field in record.fields.iter().rev() {
                        let equal = self.call(
                            "Eq",
                            "eq",
                            vec![
                                self.field("$left", &field.name.text)?,
                                self.field("$right", &field.name.text)?,
                            ],
                        )?;
                        body = if class == DeriveClass::Eq {
                            self.binary(BinaryOp::And, equal, body)?
                        } else {
                            let compared = self.call(
                                "Ord",
                                method,
                                vec![
                                    self.field("$left", &field.name.text)?,
                                    self.field("$right", &field.name.text)?,
                                ],
                            )?;
                            self.conditional(equal, body, compared)?
                        };
                    }
                }
                Ok(self.definition(method, &["$left", "$right"], body))
            })
            .collect()
    }
    fn pattern(&self, union: &UnionDecl, index: usize, binding: Option<&str>) -> Pattern {
        let case = &union.cases[index];
        let arguments = case
            .payload
            .as_ref()
            .map(|_| Pattern {
                kind: binding.map_or(PatternKind::Wildcard, |name| {
                    PatternKind::Binding(self.ident(name))
                }),
                span: self.span,
                depth: 1,
            })
            .into_iter()
            .collect();
        Pattern {
            kind: PatternKind::Apply(
                self.ident(format!(
                    "{}.{}.{}",
                    self.module, union.name.text, case.name.text
                )),
                arguments,
            ),
            span: self.span,
            depth: 2,
        }
    }
    fn matched(&self, name: &str, arms: Vec<MatchArm>) -> Result<Expr, Diagnostic> {
        self.make(ExprKind::Match {
            value: Box::new(self.name(name)?),
            arms,
            origin: MatchOrigin::Explicit,
        })
    }
    fn arm(&self, pattern: Pattern, body: Expr) -> MatchArm {
        MatchArm {
            pattern,
            body,
            guard: None,
            span: self.span,
        }
    }
    fn tag(&self, union: &UnionDecl, name: &str) -> Result<Expr, Diagnostic> {
        self.matched(
            name,
            (0..union.cases.len())
                .map(|index| Ok(self.arm(self.pattern(union, index, None), self.integer(index)?)))
                .collect::<Result<_, Diagnostic>>()?,
        )
    }
    fn union(&self, union: &UnionDecl, class: DeriveClass) -> Result<Vec<Definition>, Diagnostic> {
        if class == DeriveClass::Display {
            let mut arms = Vec::new();
            for (index, case) in union.cases.iter().enumerate() {
                let body = if case.payload.is_some() {
                    self.binary(
                        BinaryOp::Add,
                        self.text(&format!("{} ", case.name.text))?,
                        self.quoted(self.name("$payload")?)?,
                    )?
                } else {
                    self.text(&case.name.text)?
                };
                arms.push(self.arm(self.pattern(union, index, Some("$payload")), body));
            }
            return Ok(vec![self.definition(
                "display",
                &["$value"],
                self.matched("$value", arms)?,
            )]);
        }
        if class == DeriveClass::Hash {
            let mut arms = Vec::new();
            for (index, case) in union.cases.iter().enumerate() {
                let payload = if case.payload.is_some() {
                    self.call("Hash", "hash", vec![self.name("$payload")?])?
                } else {
                    self.word(u64::MAX)?
                };
                let value = self.mix(self.hash_start(0x55, index as u64)?, payload)?;
                arms.push(self.arm(self.pattern(union, index, Some("$payload")), value));
            }
            return Ok(vec![self.definition(
                "hash",
                &["$value"],
                self.matched("$value", arms)?,
            )]);
        }
        if class == DeriveClass::Default {
            let case = &union.cases[0];
            let owner = self.make(ExprKind::Field(
                Box::new(self.name(self.module)?),
                self.ident(&union.name.text),
            ))?;
            let mut body = self.make(ExprKind::Field(
                Box::new(owner),
                self.ident(&case.name.text),
            ))?;
            if case.payload.is_some() {
                body = self.make(ExprKind::Call(
                    Box::new(body),
                    vec![self.call("Default", "default", Vec::new())?],
                ))?;
            }
            return Ok(vec![self.definition("default", &[], body)]);
        }
        self.operators(class)?
            .iter()
            .map(|&(method, operator)| {
                if operator == BinaryOp::NotEqual {
                    return Ok(self.definition(method, &["$left", "$right"], self.negate_equal()?));
                }
                let mut arms = Vec::new();
                for (index, case) in union.cases.iter().enumerate() {
                    let compared = if case.payload.is_some() {
                        self.call(
                            class.name(),
                            method,
                            vec![self.name("$payload_left")?, self.name("$payload_right")?],
                        )?
                    } else {
                        self.boolean(matches!(
                            operator,
                            BinaryOp::Equal | BinaryOp::LessEqual | BinaryOp::GreaterEqual
                        ))?
                    };
                    let mut right = vec![
                        self.arm(self.pattern(union, index, Some("$payload_right")), compared),
                    ];
                    if union.cases.len() > 1 {
                        right.push(self.arm(
                            Pattern {
                                kind: PatternKind::Wildcard,
                                span: self.span,
                                depth: 1,
                            },
                            self.boolean(false)?,
                        ));
                    }
                    arms.push(self.arm(
                        self.pattern(union, index, Some("$payload_left")),
                        self.matched("$right", right)?,
                    ));
                }
                let mut body = self.matched("$left", arms)?;
                if class == DeriveClass::Ord {
                    let bindings = ["$left", "$right"]
                        .into_iter()
                        .map(|name| {
                            Ok(Binding {
                                name: self.ident(format!("{name}_tag")),
                                mutable: false,
                                annotation: None,
                                value: self.tag(union, name)?,
                            })
                        })
                        .collect::<Result<_, Diagnostic>>()?;
                    body = self.conditional(
                        self.binary(
                            BinaryOp::Equal,
                            self.name("$left_tag")?,
                            self.name("$right_tag")?,
                        )?,
                        body,
                        self.binary(operator, self.name("$left_tag")?, self.name("$right_tag")?)?,
                    )?;
                    body = self.make(ExprKind::Block {
                        bindings,
                        result: Box::new(body),
                    })?;
                }
                Ok(self.definition(method, &["$left", "$right"], body))
            })
            .collect()
    }
}
