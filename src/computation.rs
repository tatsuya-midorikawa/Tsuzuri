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
    "MergeSources",
    "BindReturn",
    "Bind2",
    "Using",
];

pub(super) fn collect_all(
    modules: &[ModuleInput<'_>],
    diagnostics: &mut Diagnostics,
) -> BTreeMap<String, BTreeSet<String>> {
    let mut builders = BTreeMap::new();
    for (source, &ModuleInput { name, program, .. }) in modules.iter().enumerate() {
        if diagnostics.is_full() {
            break;
        }
        let collected = (|| {
            let Some(kind) = program.source_kind else {
                return Ok(None);
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
                    .or_else(|| program.unions.first().map(|union| union.name.span))
                    .or_else(|| program.type_aliases.first().map(|alias| alias.name.span))
                    .or_else(|| program.constants.first().map(|constant| constant.name.span))
                    .or_else(|| program.externs.first().map(|external| external.name.span))
                    .or_else(|| program.functions.first().map(|function| function.name.span))
                    .or_else(|| program.tests.first().map(|test| test.name_span))
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
                        "a .tt file contains only type class declarations; put records, unions, type aliases, functions, and instances in .tz or .tc files",
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
                if let Some(function) = program.functions.iter().find(|function| {
                    function.visibility == Visibility::Private
                        && OPERATIONS.contains(&function.name.text.as_str())
                }) {
                    return Err(Diagnostic::new(
                        "E1022",
                        format!(
                            "builder operation '{}' cannot be private; make the operation public and keep helpers private",
                            function.name.text
                        ),
                        function.name.span,
                    ));
                }
                let methods: BTreeSet<_> = program
                    .functions
                    .iter()
                    .filter(|function| function.visibility == Visibility::Public)
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
                return Ok(Some(methods));
            }
            Ok(None)
        })();
        match collected {
            Ok(Some(methods)) => {
                builders.insert(name.to_owned(), methods);
            }
            Ok(None) => {}
            Err(error) => diagnostics.push(error),
        }
    }
    builders
}

pub(super) fn expand(expression: &mut Expr, names: &Names) -> Result<(), Diagnostic> {
    let span = expression.span;
    let mut pattern_depth = 0;
    let children: Vec<&mut Expr> = match &mut expression.kind {
        ExprKind::Computation(builder, body) => {
            expand_block(body, names)?;
            if builder.text.starts_with("$implicit") {
                expression.depth = body.depth + 1;
                return bounded_depth(expression.depth, span);
            }
            let lowered = lower(builder, body, names)?;
            expression.depth = lowered.depth;
            expression.kind = ExprKind::ComputationBoundary(Box::new(lowered));
            bounded_depth(expression.depth, span)?;
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
            let lowered = lower(name, &body, names)?;
            expression.depth = lowered.depth;
            expression.kind = ExprKind::ComputationBoundary(Box::new(lowered));
            bounded_depth(expression.depth, span)?;
            return Ok(());
        }
        ExprKind::ComputationBoundary(value) => {
            expand(value, names)?;
            expression.depth = value.depth;
            return Ok(());
        }
        ExprKind::Unary(_, value)
        | ExprKind::Lambda(_, value)
        | ExprKind::Task(value)
        | ExprKind::TaskRun(value)
        | ExprKind::Field(value, _)
        | ExprKind::Borrow(value, _, _)
        | ExprKind::Dereference(value, _)
        | ExprKind::NewLiteral(value)
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
        ExprKind::Match { value, arms, .. } => {
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
        ExprKind::Slice { value, start, end } => std::iter::once(value.as_mut())
            .chain(start.iter_mut().map(Box::as_mut))
            .chain(end.iter_mut().map(Box::as_mut))
            .collect(),
        ExprKind::RecordUpdate { base, fields } => std::iter::once(base.as_mut())
            .chain(fields.iter_mut().map(|(_, value)| value))
            .collect(),
        ExprKind::Array(values) | ExprKind::List(values) | ExprKind::Tuple(values) => {
            values.iter_mut().collect()
        }
        ExprKind::Integer(..)
        | ExprKind::Float(..)
        | ExprKind::String(_)
        | ExprKind::Char(_)
        | ExprKind::Utf8Char(_)
        | ExprKind::Bool(_)
        | ExprKind::Unit
        | ExprKind::Break
        | ExprKind::Continue
        | ExprKind::Name(_)
        | ExprKind::TypeFunction(..)
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
            ComputationStatementKind::LetAnd(bindings) => {
                for binding in bindings {
                    expand(&mut binding.value, names)?;
                }
                depth = depth.max(statement.depth());
                continue;
            }
            ComputationStatementKind::Match(value, arms) => {
                for arm in arms {
                    expand_pattern(&mut arm.pattern, names)?;
                    if let Some(guard) = &mut arm.guard {
                        expand(guard, names)?;
                    }
                    expand_block(&mut arm.body, names)?;
                }
                value
            }
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
    let lowering = Lowering {
        builder,
        methods,
        implicit: false,
    };
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
    implicit: bool,
}

impl Lowering<'_> {
    fn nested_block(&self, body: &ComputationBlock) -> Result<Expr, Diagnostic> {
        if self.implicit {
            implicit_body(&self.builder.text, body.clone())
        } else {
            self.block(body)
        }
    }

    fn implicit_tail(
        &self,
        first: &ComputationStatement,
        tail: ComputationBlock,
    ) -> Result<Expr, Diagnostic> {
        if let ComputationStatementKind::Match(_, arms) = &first.kind {
            let (_, matched) = self.match_body(arms, first.span)?;
            if tail.statements.is_empty() {
                return Ok(matched);
            }
            let rest = self.delay(implicit_body(&self.builder.text, tail)?, first.span)?;
            self.call("Combine", vec![matched, rest], first.span)
        } else {
            implicit_body(&self.builder.text, tail)
        }
    }

    fn block(&self, body: &ComputationBlock) -> Result<Expr, Diagnostic> {
        let mut result = None;
        let mut bindings = Vec::new();
        let fusion = body
            .statements
            .len()
            .checked_sub(2)
            .filter(|index| {
                matches!(
                    body.statements[index + 1].kind,
                    ComputationStatementKind::Operation("Return", _)
                )
            })
            .and_then(|index| match &body.statements[index].kind {
                ComputationStatementKind::Let(_, true) if self.methods.contains("BindReturn") => {
                    Some((index, "BindReturn"))
                }
                ComputationStatementKind::LetAnd(group)
                    if group.len() == 2 && self.methods.contains("Bind2") =>
                {
                    Some((index, "Bind2"))
                }
                _ => None,
            });
        for (index, statement) in body.statements.iter().enumerate().rev() {
            let span = statement.span;
            if fusion.is_some_and(|(fused, _)| index == fused + 1)
                && let ComputationStatementKind::Operation("Return", value) = &statement.kind
            {
                result = Some(value.clone());
                continue;
            }
            let value = match &statement.kind {
                ComputationStatementKind::LetAnd(group) => {
                    let tail = self.finish(result, bindings, body.span)?;
                    result = Some(self.and_bindings(
                        group,
                        tail,
                        fusion.is_some_and(|(fused, _)| fused == index),
                        span,
                    )?);
                    bindings = Vec::new();
                    continue;
                }
                ComputationStatementKind::Match(source, arms) => {
                    self.match_binding(source, arms, span)?
                }
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
                    result = Some(self.call(
                        if fusion.is_some_and(|(fused, _)| fused == index) {
                            "BindReturn"
                        } else {
                            "Bind"
                        },
                        vec![binding.value.clone(), continuation],
                        span,
                    )?);
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
                    let yes = self.nested_block(yes)?;
                    let no = match no {
                        Some(no) => self.nested_block(no)?,
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
                    let mut body = self.nested_block(body)?;
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
                                origin: MatchOrigin::ComputationDestructuring,
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
                    let body = self.thunk(self.nested_block(body)?, body.span)?;
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

    fn match_binding(
        &self,
        source: &Expr,
        arms: &[ComputationMatchArm],
        span: Span,
    ) -> Result<Expr, Diagnostic> {
        let (name, matched) = self.match_body(arms, span)?;
        let depth = matched.depth + 1;
        let continuation = make(
            ExprKind::Lambda(vec![(name, false)], Box::new(matched)),
            span,
            depth,
        )?;
        self.call("Bind", vec![source.clone(), continuation], span)
    }

    fn match_body(
        &self,
        arms: &[ComputationMatchArm],
        span: Span,
    ) -> Result<(Ident, Expr), Diagnostic> {
        let name = ident("$computation_match", span);
        let arms: Vec<_> = arms
            .iter()
            .map(|arm| {
                Ok(MatchArm {
                    pattern: arm.pattern.clone(),
                    guard: arm.guard.clone(),
                    body: self.nested_block(&arm.body)?,
                    span: arm.span,
                })
            })
            .collect::<Result<_, Diagnostic>>()?;
        let depth = arms
            .iter()
            .map(|arm| {
                arm.body
                    .depth
                    .max(arm.pattern.depth)
                    .max(arm.guard.as_ref().map_or(0, |guard| guard.depth))
            })
            .max()
            .unwrap_or(0)
            + 1;
        let matched = make(
            ExprKind::Match {
                value: Box::new(make(ExprKind::Name(name.clone()), span, 1)?),
                arms,
                origin: MatchOrigin::Explicit,
            },
            span,
            depth,
        )?;
        Ok((name, matched))
    }

    fn and_bindings(
        &self,
        group: &[Binding],
        tail: Expr,
        fused: bool,
        span: Span,
    ) -> Result<Expr, Diagnostic> {
        let mut staged = Vec::new();
        let mut sources = Vec::new();
        for (index, binding) in group.iter().enumerate() {
            let name = ident(
                &format!("$computation_source_{}_{}", span.start, index),
                binding.name.span,
            );
            staged.push(Binding {
                name: name.clone(),
                mutable: false,
                annotation: None,
                value: binding.value.clone(),
            });
            sources.push(make(ExprKind::Name(name), binding.value.span, 1)?);
        }
        let value = if fused {
            let mut parameters = Vec::new();
            let mut annotations = Vec::new();
            for (index, binding) in group.iter().enumerate() {
                if binding.annotation.is_some() {
                    let name = ident(&format!("$computation_callback_{index}"), binding.name.span);
                    parameters.push((name.clone(), false));
                    annotations.push(Binding {
                        value: make(ExprKind::Name(name), binding.name.span, 1)?,
                        ..binding.clone()
                    });
                } else {
                    parameters.push((binding.name.clone(), binding.mutable));
                }
            }
            let body = block(annotations, tail, span)?;
            let depth = body.depth + 1;
            let continuation = make(ExprKind::Lambda(parameters, Box::new(body)), span, depth)?;
            let mut arguments = sources;
            arguments.push(continuation);
            self.call("Bind2", arguments, span)?
        } else {
            let mut sources = sources.into_iter();
            let mut merged = sources.next().unwrap();
            for source in sources {
                merged = self.call("MergeSources", vec![merged, source], span)?;
            }
            let name = ident("$computation_pair", span);
            let mut pattern = None;
            let mut locals = Vec::new();
            for (index, binding) in group.iter().enumerate() {
                let name = ident(&format!("$computation_bound_{index}"), binding.name.span);
                locals.push(Binding {
                    value: make(ExprKind::Name(name.clone()), binding.name.span, 1)?,
                    ..binding.clone()
                });
                let next = Pattern {
                    kind: PatternKind::Binding(name),
                    span: binding.name.span,
                    depth: 1,
                };
                pattern = Some(if let Some(previous) = pattern {
                    let previous: Pattern = previous;
                    let depth = previous.depth + 1;
                    Pattern {
                        kind: PatternKind::Tuple(vec![previous, next]),
                        span,
                        depth,
                    }
                } else {
                    next
                });
            }
            let body = block(locals, tail, span)?;
            let pattern = pattern.unwrap();
            let depth = pattern.depth.max(body.depth) + 1;
            let matched = make(
                ExprKind::Match {
                    value: Box::new(make(ExprKind::Name(name.clone()), span, 1)?),
                    arms: vec![MatchArm {
                        pattern,
                        guard: None,
                        body,
                        span,
                    }],
                    origin: MatchOrigin::ComputationDestructuring,
                },
                span,
                depth,
            )?;
            let continuation = make(
                ExprKind::Lambda(vec![(name, false)], Box::new(matched)),
                span,
                depth + 1,
            )?;
            self.call("Bind", vec![merged, continuation], span)?
        };
        block(staged, value, span)
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
        provenance: Provenance::Generated,
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

fn implicit_body(builder: &str, body: ComputationBlock) -> Result<Expr, Diagnostic> {
    let span = body.span;
    let depth = body.depth + 1;
    make(
        ExprKind::Computation(
            ident(&format!("$implicit.body.{builder}"), span),
            Box::new(body),
        ),
        span,
        depth,
    )
}

type StagedComputation = (Vec<(Local, TypedExpr)>, Expr);

pub(super) fn check_implicit(
    checker: &mut Checker<'_>,
    builder: &Ident,
    body: &ComputationBlock,
    expected: Option<&Type>,
) -> Result<TypedExpr, Diagnostic> {
    bounded_depth(checker.computation_depth + body.depth + 4, body.span)?;
    checker.computation_depth += 4;
    let checked = if let Some(builder) = builder.text.strip_prefix("$implicit.body.") {
        if builder == "task" {
            checker.implicit_task_sequence(body, expected)
        } else {
            checker.implicit_sequence(builder, body, expected)
        }
    } else {
        let outer_loop = std::mem::take(&mut checker.normal_loop_depth);
        let outer = checker
            .scopes
            .iter()
            .flat_map(|scope| scope.values())
            .map(|local| (local.id, local.clone()))
            .collect();
        checker.scopes.push(BTreeMap::new());
        let result = checker.implicit_root(
            builder.text.strip_prefix("$implicit.root."),
            body,
            expected,
            &outer,
        );
        checker.scopes.pop();
        checker.normal_loop_depth = outer_loop;
        result
    };
    checker.computation_depth -= 4;
    checked
}

impl Checker<'_> {
    fn implicit_candidates(&self, ty: &Type, result: bool) -> Vec<String> {
        if matches!(self.inference.resolve(ty), Type::Task(_)) {
            return vec!["task".into()];
        }
        let mut candidates = Vec::new();
        let mut fallback = Vec::new();
        for (builder, methods) in &self.names.builders {
            let operation = if result {
                ["Run", "Delay", "Return", "Yield", "Zero"]
                    .into_iter()
                    .find(|method| methods.contains(*method))
            } else {
                methods.contains("Bind").then_some("Bind")
            };
            let Some(operation) = operation else { continue };
            let Some(function) = self.names.functions.get(&format!("{builder}.{operation}")) else {
                continue;
            };
            let scheme = &self.signatures[function.id];
            if scheme.is_poisoned() {
                continue;
            }
            let template = if result {
                &scheme.signature.result
            } else if let Some(parameter) = scheme.signature.parameters.first() {
                parameter
            } else {
                continue;
            };
            let mut inference = self.inference.clone();
            let substitutions = scheme
                .variables
                .iter()
                .map(|name| (name.clone(), inference.fresh()))
                .collect();
            let parameter = polymorph::substitute(template, &substitutions);
            if inference
                .unify(ty, &parameter, &self.types, Span::default())
                .is_ok()
            {
                if matches!(template, Type::Variable(_) | Type::Application(..)) {
                    fallback.push(builder.clone());
                } else {
                    candidates.push(builder.clone());
                }
            }
        }
        if candidates.is_empty() && !result {
            fallback
        } else {
            candidates
        }
    }

    fn implicit_source_builder(
        &self,
        ty: &Type,
        preferred: Option<&str>,
        span: Span,
    ) -> Result<String, Diagnostic> {
        let candidates = self.implicit_candidates(ty, false);
        if let Some(preferred) =
            preferred.filter(|name| candidates.iter().any(|candidate| candidate == name))
        {
            return Ok(preferred.to_owned());
        }
        if let [builder] = candidates.as_slice() {
            return Ok(builder.clone());
        }
        Err(Diagnostic::new(
            "E1018",
            if candidates.is_empty() {
                "no computation builder has a Bind operation for this value; use an explicit builder or a type annotation".into()
            } else {
                format!(
                    "ambiguous computation builders: {}; use an explicit Builder {{ ... }} or a result type annotation",
                    candidates.join(", ")
                )
            },
            span,
        ))
    }

    fn implicit_function(
        &mut self,
        builder: &str,
        operation: &str,
        span: Span,
    ) -> Result<TypedExpr, Diagnostic> {
        let expression = make(
            ExprKind::QualifiedFunction(ident(&format!("{builder}.{operation}"), span)),
            span,
            1,
        )?;
        self.expression(&expression, None)
    }

    fn implicit_apply(
        &mut self,
        callee: TypedExpr,
        arguments: Vec<TypedExpr>,
        span: Span,
    ) -> Result<TypedExpr, Diagnostic> {
        let (parameters, result) = self.call_signature(&callee.ty, arguments.len(), span)?;
        for (value, parameter) in arguments.iter().zip(parameters) {
            self.same(&value.ty, &parameter, value.span)?;
        }
        self.finish_expression(Self::call_kind(callee, arguments), result, None, span)
    }

    fn implicit_root(
        &mut self,
        forced: Option<&str>,
        body: &ComputationBlock,
        expected: Option<&Type>,
        outer: &BTreeMap<usize, Local>,
    ) -> Result<TypedExpr, Diagnostic> {
        let mut builder = forced.map(str::to_owned).or_else(|| {
            let candidates = self.implicit_candidates(expected?, true);
            (candidates.len() == 1).then(|| candidates[0].clone())
        });
        let mut prefix = Vec::new();
        let mut remaining = body.clone();
        if builder.is_none() {
            let mut count = 0;
            for statement in &body.statements {
                let binding = match &statement.kind {
                    ComputationStatementKind::Let(binding, false) => binding.clone(),
                    ComputationStatementKind::Expression(value) => Binding {
                        name: ident("_", statement.span),
                        mutable: false,
                        annotation: Some(annotation("unit", statement.span)),
                        value: value.clone(),
                    },
                    _ => break,
                };
                let expected = binding
                    .annotation
                    .as_ref()
                    .map(|ty| self.annotation(ty))
                    .transpose()?;
                let value = self.expression(&binding.value, expected.as_ref())?;
                let local = self.bind(&binding.name, value.ty.clone(), binding.mutable);
                prefix.push((local, value));
                count += 1;
            }
            remaining.statements.drain(..count);
            if let Some(statement) = remaining.statements.first_mut() {
                let source = match &mut statement.kind {
                    ComputationStatementKind::Let(binding, true) => Some(&mut binding.value),
                    ComputationStatementKind::LetAnd(group) => Some(&mut group[0].value),
                    ComputationStatementKind::Do(value) => Some(value),
                    ComputationStatementKind::Match(value, _) => Some(value.as_mut()),
                    ComputationStatementKind::Operation("ReturnFrom" | "YieldFrom", value) => {
                        Some(value)
                    }
                    _ => None,
                };
                if let Some(source) = source {
                    let value = self.expression(source, None)?;
                    builder = Some(self.implicit_source_builder(&value.ty, None, source.span)?);
                    let name = ident("$implicit.source", source.span);
                    let local = self.bind(&name, value.ty.clone(), false);
                    *source = make(ExprKind::Name(name), source.span, 1)?;
                    prefix.push((local, value));
                }
            }
        }
        let Some(builder) = builder else {
            let result = match remaining.statements.as_slice() {
                [] => self.expression(&make(ExprKind::Unit, body.span, 1)?, expected)?,
                [
                    ComputationStatement {
                        kind: ComputationStatementKind::Operation("Return", value),
                        ..
                    },
                ] => self.expression(value, expected)?,
                _ => {
                    return Err(Diagnostic::new(
                        "E1018",
                        "cannot infer the computation builder; add a result type annotation or use Builder { ... }",
                        body.span,
                    ));
                }
            };
            return Ok(typed_block(prefix, result, body.span));
        };
        if builder == "task" {
            let required = match expected.map(|ty| self.inference.resolve(ty)) {
                Some(Type::Task(result)) => *result,
                _ => self.inference.fresh(),
            };
            let result = self.implicit_task_sequence(&remaining, Some(&required))?;
            let result = typed_block(prefix, result, body.span);
            return self.checked_lambda(Vec::new(), result, outer, expected, body.span, true);
        }
        let names = self.names;
        let methods = &names.builders[&builder];
        let mut required = expected.cloned().unwrap_or_else(|| self.inference.fresh());
        let run = if methods.contains("Run") {
            let callee = self.implicit_function(&builder, "Run", body.span)?;
            let (parameters, result) = self.call_signature(&callee.ty, 1, body.span)?;
            self.same(&result, &required, body.span)?;
            required = parameters[0].clone();
            Some(callee)
        } else {
            None
        };
        let delay = if methods.contains("Delay") {
            let callee = self.implicit_function(&builder, "Delay", body.span)?;
            let (parameters, result) = self.call_signature(&callee.ty, 1, body.span)?;
            self.same(&result, &required, body.span)?;
            let (arguments, result) = self.call_signature(&parameters[0], 1, body.span)?;
            self.same(&arguments[0], &Type::Unit, body.span)?;
            required = result;
            Some(callee)
        } else {
            None
        };
        let result = self.implicit_sequence(&builder, &remaining, Some(&required))?;
        let mut result = typed_block(prefix, result, body.span);
        if let Some(delay) = delay {
            let parameter = self.bind(&ident("$implicit.unit", body.span), Type::Unit, false);
            let closure =
                self.checked_lambda(vec![parameter], result, outer, None, body.span, false)?;
            result = self.implicit_apply(delay, vec![closure], body.span)?;
        }
        if let Some(run) = run {
            result = self.implicit_apply(run, vec![result], body.span)?;
        }
        if let Some(expected) = expected {
            self.same(&result.ty, expected, body.span)?;
        }
        Ok(result)
    }

    fn implicit_sequence(
        &mut self,
        builder: &str,
        body: &ComputationBlock,
        expected: Option<&Type>,
    ) -> Result<TypedExpr, Diagnostic> {
        let names = self.names;
        let builder_name = ident(builder, body.span);
        let lowering = Lowering {
            builder: &builder_name,
            methods: &names.builders[builder],
            implicit: true,
        };
        match body.statements.first().map(|statement| &statement.kind) {
            Some(
                ComputationStatementKind::Let(_, true)
                | ComputationStatementKind::Do(_)
                | ComputationStatementKind::Match(..),
            ) => self.implicit_binding(&lowering, body, expected),
            Some(ComputationStatementKind::LetAnd(group)) => {
                self.implicit_group(&lowering, group, body, expected)
            }
            _ => self.implicit_nonbinding_sequence(builder, body, expected),
        }
    }

    fn implicit_nonbinding_sequence(
        &mut self,
        builder: &str,
        body: &ComputationBlock,
        expected: Option<&Type>,
    ) -> Result<TypedExpr, Diagnostic> {
        let names = self.names;
        let builder_name = ident(builder, body.span);
        let lowering = Lowering {
            builder: &builder_name,
            methods: &names.builders[builder],
            implicit: true,
        };
        let Some(first) = body.statements.first() else {
            return self.expression(&lowering.call("Zero", Vec::new(), body.span)?, expected);
        };
        let mut prefix = Vec::new();
        for statement in &body.statements {
            prefix.push(match &statement.kind {
                ComputationStatementKind::Let(binding, false) => binding.clone(),
                ComputationStatementKind::Expression(value) => Binding {
                    name: ident("_", statement.span),
                    mutable: false,
                    annotation: Some(annotation("unit", statement.span)),
                    value: value.clone(),
                },
                _ => break,
            });
        }
        if !prefix.is_empty() {
            let mut tail = body.clone();
            tail.statements.drain(..prefix.len());
            let result = implicit_body(builder, tail)?;
            return self.expression(&block(prefix, result, body.span)?, expected);
        }
        if let ComputationStatementKind::Operation("ReturnFrom", source) = &first.kind {
            let value = self.expression(source, None)?;
            let source_builder =
                self.implicit_source_builder(&value.ty, Some(builder), source.span)?;
            self.scopes.push(BTreeMap::new());
            let name = ident("$implicit.returned", source.span);
            let local = self.bind(&name, value.ty.clone(), false);
            let source = make(ExprKind::Name(name), source.span, 1)?;
            let expression = if source_builder == builder {
                lowering.call("ReturnFrom", vec![source], first.span)?
            } else if source_builder == "task" {
                lowering.call(
                    "Return",
                    vec![make(ExprKind::TaskRun(Box::new(source)), first.span, 2)?],
                    first.span,
                )?
            } else {
                let foreign = ComputationBlock {
                    statements: vec![ComputationStatement {
                        kind: ComputationStatementKind::Operation("ReturnFrom", source),
                        span: first.span,
                    }],
                    span: first.span,
                    depth: 2,
                };
                let result = lower(&ident(&source_builder, first.span), &foreign, names)?;
                lowering.call("Return", vec![result], first.span)?
            };
            let checked = self.expression(&expression, expected);
            self.scopes.pop();
            return Ok(typed_block(vec![(local, value)], checked?, first.span));
        }
        let mut single = body.clone();
        single.statements.truncate(1);
        let mut value = lowering.block(&single)?;
        if body.statements.len() > 1 {
            let mut tail = body.clone();
            tail.statements.remove(0);
            let rest = lowering.delay(implicit_body(builder, tail)?, body.span)?;
            value = lowering.call("Combine", vec![value, rest], body.span)?;
        }
        self.expression(&value, expected)
    }

    fn implicit_binding(
        &mut self,
        lowering: &Lowering<'_>,
        body: &ComputationBlock,
        expected: Option<&Type>,
    ) -> Result<TypedExpr, Diagnostic> {
        self.scopes.push(BTreeMap::new());
        let checked =
            self.lower_implicit_binding(lowering, body)
                .and_then(|(local, value, expression)| {
                    self.expression(&expression, expected)
                        .map(|result| typed_block(vec![(local, value)], result, body.span))
                });
        self.scopes.pop();
        checked
    }

    fn lower_implicit_binding(
        &mut self,
        lowering: &Lowering<'_>,
        body: &ComputationBlock,
    ) -> Result<(Local, TypedExpr, Expr), Diagnostic> {
        let first = &body.statements[0];
        let mut binding = match &first.kind {
            ComputationStatementKind::Let(binding, true) => binding.clone(),
            ComputationStatementKind::Do(value) => Binding {
                name: ident("_", first.span),
                mutable: false,
                annotation: Some(annotation("unit", first.span)),
                value: value.clone(),
            },
            ComputationStatementKind::Match(value, _) => Binding {
                name: ident("$computation_match", first.span),
                mutable: false,
                annotation: None,
                value: (**value).clone(),
            },
            _ => unreachable!(),
        };
        let value = self.expression(&binding.value, None)?;
        let source_builder = self.implicit_source_builder(
            &value.ty,
            Some(&lowering.builder.text),
            binding.value.span,
        )?;
        let source_name = ident("$implicit.bound", binding.value.span);
        let local = self.bind(&source_name, value.ty.clone(), false);
        binding.value = make(ExprKind::Name(source_name), binding.value.span, 1)?;
        let mut tail = body.clone();
        tail.statements.remove(0);
        let result = if source_builder == "task" {
            binding.value = make(ExprKind::TaskRun(Box::new(binding.value)), first.span, 2)?;
            block(
                vec![binding],
                lowering.implicit_tail(first, tail)?,
                body.span,
            )?
        } else if source_builder == lowering.builder.text {
            let fused = lowering.methods.contains("BindReturn")
                && matches!(first.kind, ComputationStatementKind::Let(_, true))
                && matches!(
                    tail.statements.as_slice(),
                    [ComputationStatement {
                        kind: ComputationStatementKind::Operation("Return", _),
                        ..
                    }]
                );
            let rest = if fused {
                let ComputationStatementKind::Operation(_, value) = &tail.statements[0].kind else {
                    unreachable!()
                };
                value.clone()
            } else {
                lowering.implicit_tail(first, tail)?
            };
            let continuation = lowering.continuation(&binding, rest, first.span)?;
            lowering.call(
                if fused { "BindReturn" } else { "Bind" },
                vec![binding.value.clone(), continuation],
                first.span,
            )?
        } else if lowering.methods.contains("Using") {
            let callback = ident("$implicit.next", first.span);
            let payload = ident("$implicit.payload", first.span);
            let returned = make(
                ExprKind::Call(
                    Box::new(make(ExprKind::Name(callback.clone()), first.span, 1)?),
                    vec![make(ExprKind::Name(payload.clone()), first.span, 1)?],
                ),
                first.span,
                2,
            )?;
            let foreign = ComputationBlock {
                statements: vec![
                    ComputationStatement {
                        kind: ComputationStatementKind::Let(
                            Binding {
                                name: payload,
                                mutable: false,
                                annotation: None,
                                value: binding.value.clone(),
                            },
                            true,
                        ),
                        span: first.span,
                    },
                    ComputationStatement {
                        kind: ComputationStatementKind::Operation("Return", returned),
                        span: first.span,
                    },
                ],
                span: first.span,
                depth: 3,
            };
            let handler = lower(&ident(&source_builder, first.span), &foreign, self.names)?;
            let depth = handler.depth + 1;
            let handler = make(
                ExprKind::Lambda(vec![(callback, false)], Box::new(handler)),
                first.span,
                depth,
            )?;
            let continuation = lowering.continuation(
                &binding,
                lowering.implicit_tail(first, tail)?,
                first.span,
            )?;
            lowering.call("Using", vec![handler, continuation], first.span)?
        } else {
            let mut foreign = body.clone();
            if let ComputationStatementKind::Match(source, _) = &mut foreign.statements[0].kind {
                **source = binding.value;
            } else {
                foreign.statements[0].kind = ComputationStatementKind::Let(binding, true);
            }
            let computation = make(
                ExprKind::Computation(
                    ident(&format!("$implicit.root.{source_builder}"), body.span),
                    Box::new(foreign),
                ),
                body.span,
                body.depth + 1,
            )?;
            lowering.call("Return", vec![computation], first.span)?
        };
        Ok((local, value, result))
    }

    fn implicit_group(
        &mut self,
        lowering: &Lowering<'_>,
        group: &[Binding],
        body: &ComputationBlock,
        expected: Option<&Type>,
    ) -> Result<TypedExpr, Diagnostic> {
        self.scopes.push(BTreeMap::new());
        let checked =
            self.lower_implicit_group(lowering, group, body)
                .and_then(|(staged, expression)| {
                    self.expression(&expression, expected)
                        .map(|result| typed_block(staged, result, body.span))
                });
        self.scopes.pop();
        checked
    }

    fn lower_implicit_group(
        &mut self,
        lowering: &Lowering<'_>,
        group: &[Binding],
        body: &ComputationBlock,
    ) -> Result<StagedComputation, Diagnostic> {
        let mut staged = Vec::new();
        let mut sources = Vec::new();
        let mut source_builder = None;
        for (index, binding) in group.iter().enumerate() {
            let value = self.expression(&binding.value, None)?;
            if source_builder.is_none() {
                source_builder = Some(self.implicit_source_builder(
                    &value.ty,
                    Some(&lowering.builder.text),
                    binding.value.span,
                )?);
            }
            let name = ident(&format!("$implicit.source.{index}"), binding.value.span);
            let local = self.bind(&name, value.ty.clone(), false);
            staged.push((local, value));
            sources.push(Binding {
                value: make(ExprKind::Name(name), binding.value.span, 1)?,
                ..binding.clone()
            });
        }
        let source_builder = source_builder.unwrap();
        let mut tail = body.clone();
        tail.statements.remove(0);
        let expression = if source_builder == lowering.builder.text {
            let fused = group.len() == 2
                && lowering.methods.contains("Bind2")
                && matches!(
                    tail.statements.as_slice(),
                    [ComputationStatement {
                        kind: ComputationStatementKind::Operation("Return", _),
                        ..
                    }]
                );
            let rest = if fused {
                let ComputationStatementKind::Operation(_, value) = &tail.statements[0].kind else {
                    unreachable!()
                };
                value.clone()
            } else {
                implicit_body(&source_builder, tail)?
            };
            lowering.and_bindings(&sources, rest, fused, body.span)?
        } else if source_builder != "task" && lowering.methods.contains("Using") {
            let callback = ident("$implicit.next", body.span);
            let tuple = ident("$implicit.tuple", body.span);
            let mut values = Vec::new();
            let mut patterns = Vec::new();
            let mut locals = Vec::new();
            let mut handler_sources = Vec::new();
            for (index, binding) in sources.iter().enumerate() {
                let name = ident(&format!("$implicit_item_{index}"), binding.name.span);
                let value = make(ExprKind::Name(name.clone()), binding.name.span, 1)?;
                values.push(value.clone());
                patterns.push(Pattern {
                    kind: PatternKind::Binding(name.clone()),
                    span: binding.name.span,
                    depth: 1,
                });
                locals.push(Binding {
                    value,
                    ..binding.clone()
                });
                handler_sources.push(Binding {
                    name,
                    mutable: false,
                    annotation: None,
                    value: binding.value.clone(),
                });
            }
            let returned = make(
                ExprKind::Call(
                    Box::new(make(ExprKind::Name(callback.clone()), body.span, 1)?),
                    vec![make(ExprKind::Tuple(values), body.span, 2)?],
                ),
                body.span,
                3,
            )?;
            let foreign = ComputationBlock {
                statements: vec![
                    ComputationStatement {
                        kind: ComputationStatementKind::LetAnd(handler_sources),
                        span: body.span,
                    },
                    ComputationStatement {
                        kind: ComputationStatementKind::Operation("Return", returned),
                        span: body.span,
                    },
                ],
                span: body.span,
                depth: 4,
            };
            let handler = lower(&ident(&source_builder, body.span), &foreign, self.names)?;
            let depth = handler.depth + 1;
            let handler = make(
                ExprKind::Lambda(vec![(callback, false)], Box::new(handler)),
                body.span,
                depth,
            )?;
            let rest = block(
                locals,
                implicit_body(&lowering.builder.text, tail)?,
                body.span,
            )?;
            let depth = rest.depth + 1;
            let matched = make(
                ExprKind::Match {
                    value: Box::new(make(ExprKind::Name(tuple.clone()), body.span, 1)?),
                    arms: vec![MatchArm {
                        pattern: Pattern {
                            kind: PatternKind::Tuple(patterns),
                            span: body.span,
                            depth: 2,
                        },
                        guard: None,
                        body: rest,
                        span: body.span,
                    }],
                    origin: MatchOrigin::ComputationDestructuring,
                },
                body.span,
                depth,
            )?;
            let continuation = make(
                ExprKind::Lambda(vec![(tuple, false)], Box::new(matched)),
                body.span,
                depth + 1,
            )?;
            lowering.call("Using", vec![handler, continuation], body.span)?
        } else {
            let mut foreign = body.clone();
            foreign.statements[0].kind = ComputationStatementKind::LetAnd(sources);
            let expression = make(
                ExprKind::Computation(
                    ident(&format!("$implicit.root.{source_builder}"), body.span),
                    Box::new(foreign),
                ),
                body.span,
                body.depth + 1,
            )?;
            lowering.call("Return", vec![expression], body.span)?
        };
        Ok((staged, expression))
    }

    fn implicit_task_sequence(
        &mut self,
        body: &ComputationBlock,
        expected: Option<&Type>,
    ) -> Result<TypedExpr, Diagnostic> {
        self.scopes.push(BTreeMap::new());
        let checked = self
            .lower_implicit_task_sequence(body)
            .and_then(|(staged, expression)| {
                self.expression(&expression, expected)
                    .map(|result| typed_block(staged, result, body.span))
            });
        self.scopes.pop();
        checked
    }

    fn lower_implicit_task_sequence(
        &mut self,
        body: &ComputationBlock,
    ) -> Result<StagedComputation, Diagnostic> {
        let Some(first) = body.statements.first() else {
            return Ok((Vec::new(), make(ExprKind::Unit, body.span, 1)?));
        };
        let mut tail = body.clone();
        tail.statements.remove(0);
        let continuation = implicit_body("task", tail)?;
        let result = match &first.kind {
            ComputationStatementKind::Let(binding, false) => {
                block(vec![binding.clone()], continuation, body.span)?
            }
            ComputationStatementKind::Let(_, true) | ComputationStatementKind::Do(_) => {
                let mut binding = match &first.kind {
                    ComputationStatementKind::Let(binding, _) => binding.clone(),
                    ComputationStatementKind::Do(value) => Binding {
                        name: ident("_", first.span),
                        mutable: false,
                        annotation: Some(annotation("unit", first.span)),
                        value: value.clone(),
                    },
                    _ => unreachable!(),
                };
                let value = self.expression(&binding.value, None)?;
                let source_builder =
                    self.implicit_source_builder(&value.ty, Some("task"), binding.value.span)?;
                let name = ident("$implicit.task", first.span);
                let local = self.bind(&name, value.ty.clone(), false);
                binding.value = make(ExprKind::Name(name), first.span, 1)?;
                let expression = if source_builder == "task" {
                    binding.value =
                        make(ExprKind::TaskRun(Box::new(binding.value)), first.span, 2)?;
                    block(vec![binding], continuation, body.span)?
                } else {
                    let mut foreign = body.clone();
                    foreign.statements[0].kind = ComputationStatementKind::Let(binding, true);
                    make(
                        ExprKind::Computation(
                            ident(&format!("$implicit.root.{source_builder}"), body.span),
                            Box::new(foreign),
                        ),
                        body.span,
                        body.depth + 1,
                    )?
                };
                return Ok((vec![(local, value)], expression));
            }
            ComputationStatementKind::Operation("Return", value) => value.clone(),
            ComputationStatementKind::Operation("ReturnFrom", value) => make(
                ExprKind::TaskRun(Box::new(value.clone())),
                first.span,
                value.depth + 1,
            )?,
            ComputationStatementKind::Expression(value) if body.statements.len() == 1 => {
                value.clone()
            }
            ComputationStatementKind::Expression(value) => block(
                vec![Binding {
                    name: ident("_", first.span),
                    mutable: false,
                    annotation: Some(annotation("unit", first.span)),
                    value: value.clone(),
                }],
                continuation,
                body.span,
            )?,
            ComputationStatementKind::For(pattern, source, nested) => {
                let value = make(
                    ExprKind::For {
                        pattern: pattern.clone(),
                        source: Box::new(source.clone()),
                        body: Box::new(implicit_body("task", nested.clone())?),
                    },
                    first.span,
                    body.depth + 1,
                )?;
                block(
                    vec![Binding {
                        name: ident("_", first.span),
                        mutable: false,
                        annotation: Some(annotation("unit", first.span)),
                        value,
                    }],
                    continuation,
                    body.span,
                )?
            }
            ComputationStatementKind::While(condition, nested) => {
                let value = make(
                    ExprKind::While {
                        condition: Box::new(condition.clone()),
                        body: Box::new(implicit_body("task", nested.clone())?),
                    },
                    first.span,
                    body.depth + 1,
                )?;
                block(
                    vec![Binding {
                        name: ident("_", first.span),
                        mutable: false,
                        annotation: Some(annotation("unit", first.span)),
                        value,
                    }],
                    continuation,
                    body.span,
                )?
            }
            ComputationStatementKind::If(condition, yes, no) => {
                let branch = make(
                    ExprKind::If {
                        condition: Box::new(condition.clone()),
                        then_branch: Box::new(implicit_body("task", yes.clone())?),
                        else_branch: Box::new(match no {
                            Some(no) => implicit_body("task", no.clone())?,
                            None => make(ExprKind::Unit, first.span, 1)?,
                        }),
                    },
                    first.span,
                    body.depth + 1,
                )?;
                if body.statements.len() == 1 {
                    branch
                } else {
                    block(
                        vec![Binding {
                            name: ident("_", first.span),
                            mutable: false,
                            annotation: Some(annotation("unit", first.span)),
                            value: branch,
                        }],
                        continuation,
                        body.span,
                    )?
                }
            }
            _ => {
                return Err(Diagnostic::new(
                    "E1018",
                    "this implicit task operation requires an explicit computation builder",
                    first.span,
                ));
            }
        };
        Ok((Vec::new(), result))
    }
}

fn typed_block(bindings: Vec<(Local, TypedExpr)>, result: TypedExpr, span: Span) -> TypedExpr {
    if bindings.is_empty() {
        return result;
    }
    TypedExpr {
        ty: result.ty.clone(),
        kind: TypedExprKind::Block {
            bindings,
            result: Box::new(result),
        },
        span,
    }
}
