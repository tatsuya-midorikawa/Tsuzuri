use super::*;

const OPERATIONS: &[&str] = &[
    "Bind",
    "Return",
    "ReturnFrom",
    "Yield",
    "YieldFrom",
    "Zero",
    "Combine",
    "Delay",
    "Run",
    "For",
    "While",
];

pub(super) fn collect(
    modules: &[(&str, &Program)],
) -> Result<BTreeMap<String, BTreeSet<String>>, Diagnostic> {
    let mut builders = BTreeMap::new();
    for (source, (name, program)) in modules.iter().enumerate() {
        let Some(kind) = program.source_kind else {
            continue;
        };
        if kind != SourceKind::TypeClass {
            if let Some(class) = program.classes.first() {
                return Err(Diagnostic::new(
                    "E1018",
                    "type class declarations belong in a .tt file; a .tt file may declare multiple classes",
                    class.name.span,
                ));
            }
        } else {
            let invalid = program
                .records
                .first()
                .map(|record| record.name.span)
                .or_else(|| program.functions.first().map(|function| function.name.span))
                .or_else(|| {
                    program
                        .instances
                        .first()
                        .map(|instance| instance.class.span)
                })
                .or_else(|| program.entry.as_ref().map(|entry| entry.span));
            if let Some(span) = invalid {
                return Err(Diagnostic::new(
                    "E1018",
                    "a .tt file contains only type class declarations; put records, functions, and instances in .tz or .tc files",
                    span,
                ));
            }
        }
        if kind == SourceKind::Computation {
            if let Some(entry) = &program.entry {
                return Err(Diagnostic::new(
                    "E2004",
                    "a .tc file implements one builder named after the file; top-level execution belongs in Main.tz",
                    entry.span,
                ));
            }
            let methods: BTreeSet<_> = program
                .functions
                .iter()
                .map(|function| function.name.text.clone())
                .collect();
            if !OPERATIONS
                .iter()
                .any(|operation| methods.contains(*operation))
            {
                return Err(Diagnostic::new(
                    "E1018",
                    "a .tc builder needs at least one computation operation, such as Bind, Return, Yield, or Zero",
                    program
                        .functions
                        .first()
                        .map_or(Span::default().in_source(source), |function| {
                            function.name.span
                        }),
                ));
            }
            builders.insert((*name).to_owned(), methods);
        }
    }
    Ok(builders)
}

pub(super) fn expand(expression: &mut Expr, names: &Names) -> Result<(), Diagnostic> {
    let span = expression.span;
    let mut pattern_depth = 0;
    let children: Vec<&mut Expr> = match &mut expression.kind {
        ExprKind::Computation(builder, body) => {
            expand_block(body, names)?;
            *expression = lower(builder, body, names)?;
            return Ok(());
        }
        ExprKind::Record { name, fields }
            if fields.is_empty() && names.builders.contains_key(&name.text) =>
        {
            let body = ComputationBlock {
                statements: Vec::new(),
                span,
                depth: 1,
            };
            *expression = lower(name, &body, names)?;
            return Ok(());
        }
        ExprKind::Unary(_, value)
        | ExprKind::Lambda(_, value)
        | ExprKind::Task(value)
        | ExprKind::TaskRun(value)
        | ExprKind::Field(value, _)
        | ExprKind::Borrow(value, _)
        | ExprKind::Dereference(value)
        | ExprKind::Cast(value, _) => vec![value],
        ExprKind::Binary(_, left, right)
        | ExprKind::Index(left, right)
        | ExprKind::Assign(left, right)
        | ExprKind::NewArray(_, left, right)
        | ExprKind::NewList(_, left, right) => vec![left, right],
        ExprKind::Call(callee, arguments) => {
            std::iter::once(callee.as_mut()).chain(arguments).collect()
        }
        ExprKind::If {
            condition,
            then_branch,
            else_branch,
        } => vec![condition, then_branch, else_branch],
        ExprKind::While { condition, body } => vec![condition, body],
        ExprKind::For {
            pattern,
            source,
            body,
        } => {
            expand_pattern(pattern, names)?;
            pattern_depth = pattern.depth;
            vec![source, body]
        }
        ExprKind::Range {
            start,
            step,
            finish,
            ..
        } => {
            let mut values = vec![start.as_mut()];
            values.extend(step.iter_mut().map(Box::as_mut));
            values.push(finish);
            values
        }
        ExprKind::Match { value, arms } => {
            let mut values = vec![value.as_mut()];
            for arm in arms {
                expand_pattern(&mut arm.pattern, names)?;
                pattern_depth = pattern_depth.max(arm.pattern.depth);
                values.extend(arm.guard.iter_mut());
                values.push(&mut arm.body);
            }
            values
        }
        ExprKind::Block { bindings, result } => bindings
            .iter_mut()
            .map(|binding| &mut binding.value)
            .chain(std::iter::once(result.as_mut()))
            .collect(),
        ExprKind::Record { fields, .. } => fields.iter_mut().map(|(_, value)| value).collect(),
        ExprKind::Array(values) | ExprKind::List(values) | ExprKind::Tuple(values) => {
            values.iter_mut().collect()
        }
        ExprKind::Integer(..)
        | ExprKind::Float(..)
        | ExprKind::String(_)
        | ExprKind::Bool(_)
        | ExprKind::Unit
        | ExprKind::Name(_)
        | ExprKind::QualifiedFunction(_) => Vec::new(),
    };
    let mut depth = 0;
    for child in children {
        expand(child, names)?;
        depth = depth.max(child.depth);
    }
    expression.depth = depth.max(pattern_depth) + 1;
    bounded_depth(expression.depth, span)
}

fn expand_pattern(pattern: &mut Pattern, names: &Names) -> Result<(), Diagnostic> {
    let children: Vec<&mut Pattern> = match &mut pattern.kind {
        PatternKind::Literal(value) | PatternKind::Argument(value) => {
            expand(value, names)?;
            pattern.depth = value.depth;
            return bounded_depth(pattern.depth, pattern.span);
        }
        PatternKind::Tuple(values)
        | PatternKind::Array(values)
        | PatternKind::List(values)
        | PatternKind::Apply(_, values) => values.iter_mut().collect(),
        PatternKind::Record(_, fields) => fields.iter_mut().map(|(_, pattern)| pattern).collect(),
        PatternKind::Cons(a, b) | PatternKind::Or(a, b) | PatternKind::And(a, b) => vec![a, b],
        PatternKind::As(pattern, _) | PatternKind::Annotated(pattern, _) => vec![pattern],
        PatternKind::Wildcard | PatternKind::Binding(_) => Vec::new(),
    };
    let mut depth = 0;
    for child in children {
        expand_pattern(child, names)?;
        depth = depth.max(child.depth);
    }
    pattern.depth = depth + 1;
    bounded_depth(pattern.depth, pattern.span)
}

fn expand_block(body: &mut ComputationBlock, names: &Names) -> Result<(), Diagnostic> {
    let mut depth = 0;
    for statement in &mut body.statements {
        let value = match &mut statement.kind {
            ComputationStatementKind::Let(binding, _) => &mut binding.value,
            ComputationStatementKind::Do(value)
            | ComputationStatementKind::Operation(_, value)
            | ComputationStatementKind::Expression(value) => value,
            ComputationStatementKind::If(condition, yes, no) => {
                expand_block(yes, names)?;
                if let Some(no) = no {
                    expand_block(no, names)?;
                }
                condition
            }
            ComputationStatementKind::For(pattern, source, body) => {
                expand_pattern(pattern, names)?;
                expand_block(body, names)?;
                source
            }
            ComputationStatementKind::While(source, body) => {
                expand_block(body, names)?;
                source
            }
        };
        expand(value, names)?;
        depth = depth.max(statement.depth());
    }
    body.depth = depth + 1;
    bounded_depth(body.depth, body.span)
}

fn lower(builder: &Ident, body: &ComputationBlock, names: &Names) -> Result<Expr, Diagnostic> {
    let methods = names.builders.get(&builder.text).ok_or_else(|| {
        Diagnostic::new(
            "E1018",
            format!(
                "unknown computation builder '{}'; define its operations in {}.tc",
                builder.text, builder.text
            ),
            builder.span,
        )
    })?;
    let lowering = Lowering { builder, methods };
    let span = builder.span.through(body.span);
    let body = lowering.block(body)?;
    let body = lowering.delay(body, span)?;
    if methods.contains("Run") {
        lowering.call("Run", vec![body], span)
    } else {
        Ok(body)
    }
}

struct Lowering<'a> {
    builder: &'a Ident,
    methods: &'a BTreeSet<String>,
}

impl Lowering<'_> {
    fn block(&self, body: &ComputationBlock) -> Result<Expr, Diagnostic> {
        let mut result = None;
        let mut bindings = Vec::new();
        for statement in body.statements.iter().rev() {
            let span = statement.span;
            let value = match &statement.kind {
                ComputationStatementKind::Let(binding, false) => {
                    bindings.push(binding.clone());
                    continue;
                }
                ComputationStatementKind::Expression(value) => {
                    bindings.push(Binding {
                        name: ident("_", span),
                        mutable: false,
                        annotation: Some(annotation("unit", span)),
                        value: value.clone(),
                    });
                    continue;
                }
                ComputationStatementKind::Let(binding, true) => {
                    let tail = self.finish(result, bindings, body.span)?;
                    let continuation = self.continuation(binding, tail, span)?;
                    result =
                        Some(self.call("Bind", vec![binding.value.clone(), continuation], span)?);
                    bindings = Vec::new();
                    continue;
                }
                ComputationStatementKind::Do(value) => {
                    let tail = self.finish(result, bindings, body.span)?;
                    let binding = Binding {
                        name: ident("_", span),
                        mutable: false,
                        annotation: Some(annotation("unit", span)),
                        value: value.clone(),
                    };
                    let continuation = self.continuation(&binding, tail, span)?;
                    result = Some(self.call("Bind", vec![value.clone(), continuation], span)?);
                    bindings = Vec::new();
                    continue;
                }
                ComputationStatementKind::Operation(operation, value) => {
                    self.call(operation, vec![value.clone()], span)?
                }
                ComputationStatementKind::If(condition, yes, no) => {
                    let yes = self.block(yes)?;
                    let no = match no {
                        Some(no) => self.block(no)?,
                        None => self.call("Zero", Vec::new(), span)?,
                    };
                    let depth = condition.depth.max(yes.depth).max(no.depth) + 1;
                    make(
                        ExprKind::If {
                            condition: Box::new(condition.clone()),
                            then_branch: Box::new(yes),
                            else_branch: Box::new(no),
                        },
                        span,
                        depth,
                    )?
                }
                ComputationStatementKind::For(pattern, source, body) => {
                    let mut body = self.block(body)?;
                    let simple = matches!(&pattern.kind, PatternKind::Binding(name) if !name.text.as_bytes()[0].is_ascii_uppercase() && !name.text.contains('.'));
                    let name = if simple {
                        let PatternKind::Binding(name) = &pattern.kind else {
                            unreachable!()
                        };
                        name.clone()
                    } else {
                        let name = ident("$computation.element", pattern.span);
                        let matched = make(ExprKind::Name(name.clone()), pattern.span, 1)?;
                        let depth = body.depth.max(pattern.depth) + 1;
                        body = make(
                            ExprKind::Match {
                                value: Box::new(matched),
                                arms: vec![MatchArm {
                                    pattern: (**pattern).clone(),
                                    guard: None,
                                    body,
                                    span,
                                }],
                            },
                            span,
                            depth,
                        )?;
                        name
                    };
                    let depth = body.depth + 1;
                    let body = make(
                        ExprKind::Lambda(vec![(name, false)], Box::new(body)),
                        span,
                        depth,
                    )?;
                    self.call("For", vec![source.clone(), body], span)?
                }
                ComputationStatementKind::While(condition, body) => {
                    let name = ident("$computation.condition", condition.span);
                    let guard = block(
                        vec![Binding {
                            name: name.clone(),
                            mutable: false,
                            annotation: Some(annotation("bool", condition.span)),
                            value: condition.clone(),
                        }],
                        make(ExprKind::Name(name), condition.span, 1)?,
                        condition.span,
                    )?;
                    let guard = self.thunk(guard, condition.span)?;
                    let body = self.thunk(self.block(body)?, body.span)?;
                    let body = self.call("Delay", vec![body], span)?;
                    self.call("While", vec![guard, body], span)?
                }
            };
            result = Some(if result.is_some() || !bindings.is_empty() {
                let tail = self.finish(result, bindings, body.span)?;
                let tail = self.delay(tail, span)?;
                self.call("Combine", vec![value, tail], span)?
            } else {
                value
            });
            bindings = Vec::new();
        }
        self.finish(result, bindings, body.span)
    }

    fn finish(
        &self,
        result: Option<Expr>,
        mut bindings: Vec<Binding>,
        span: Span,
    ) -> Result<Expr, Diagnostic> {
        let result = match result {
            Some(result) => result,
            None => self.call("Zero", Vec::new(), span)?,
        };
        bindings.reverse();
        block(bindings, result, span)
    }

    fn continuation(&self, binding: &Binding, body: Expr, span: Span) -> Result<Expr, Diagnostic> {
        if binding.annotation.is_none() {
            let depth = body.depth + 1;
            return make(
                ExprKind::Lambda(
                    vec![(binding.name.clone(), binding.mutable)],
                    Box::new(body),
                ),
                span,
                depth,
            );
        }
        // Generated names and qualified operations cannot be captured by user bindings.
        let parameter = ident("$computation.value", binding.name.span);
        let value = make(ExprKind::Name(parameter.clone()), binding.name.span, 1)?;
        let body = block(
            vec![Binding {
                name: binding.name.clone(),
                mutable: binding.mutable,
                annotation: binding.annotation.clone(),
                value,
            }],
            body,
            span,
        )?;
        let depth = body.depth + 1;
        make(
            ExprKind::Lambda(vec![(parameter, false)], Box::new(body)),
            span,
            depth,
        )
    }

    fn thunk(&self, body: Expr, span: Span) -> Result<Expr, Diagnostic> {
        self.continuation(
            &Binding {
                name: ident("_", span),
                mutable: false,
                annotation: Some(annotation("unit", span)),
                value: make(ExprKind::Unit, span, 1)?,
            },
            body,
            span,
        )
    }

    fn delay(&self, body: Expr, span: Span) -> Result<Expr, Diagnostic> {
        if self.methods.contains("Delay") {
            self.call("Delay", vec![self.thunk(body, span)?], span)
        } else {
            Ok(body)
        }
    }

    fn call(&self, operation: &str, arguments: Vec<Expr>, span: Span) -> Result<Expr, Diagnostic> {
        if !self.methods.contains(operation) {
            return Err(Diagnostic::new(
                "E1018",
                format!(
                    "computation builder '{}' does not define '{operation}'",
                    self.builder.text
                ),
                span,
            ));
        }
        let callee = make(
            ExprKind::QualifiedFunction(ident(&format!("{}.{operation}", self.builder.text), span)),
            span,
            1,
        )?;
        let depth = arguments.iter().map(|value| value.depth).max().unwrap_or(1) + 1;
        make(ExprKind::Call(Box::new(callee), arguments), span, depth)
    }
}

fn ident(name: &str, span: Span) -> Ident {
    Ident {
        text: name.to_owned(),
        span,
    }
}

fn annotation(name: &str, span: Span) -> TypeExpr {
    TypeExpr {
        kind: TypeExprKind::Named(name.to_owned()),
        span,
    }
}

fn block(bindings: Vec<Binding>, result: Expr, span: Span) -> Result<Expr, Diagnostic> {
    if bindings.is_empty() {
        return Ok(result);
    }
    let depth = bindings
        .iter()
        .map(|binding| binding.value.depth)
        .max()
        .unwrap_or(0)
        .max(result.depth)
        + 1;
    make(
        ExprKind::Block {
            bindings,
            result: Box::new(result),
        },
        span,
        depth,
    )
}

fn make(kind: ExprKind, span: Span, depth: usize) -> Result<Expr, Diagnostic> {
    bounded_depth(depth, span)?;
    Ok(Expr { kind, span, depth })
}

fn bounded_depth(depth: usize, span: Span) -> Result<(), Diagnostic> {
    if depth > MAX_NESTING {
        return Err(Diagnostic::new(
            "E0002",
            format!("computation expansion exceeds depth {MAX_NESTING}; split the computation"),
            span,
        ));
    }
    Ok(())
}
