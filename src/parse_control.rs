use super::*;

impl Parser<'_> {
    pub(super) fn function_name(&mut self) -> Result<Ident, Diagnostic> {
        if !self.eat(&TokenKind::LeftParen) {
            return self.member_ident();
        }
        self.expect(&TokenKind::Pipe, "'|' before an active pattern name")?;
        let mut cases = Vec::new();
        let partial = loop {
            let case = self.ident()?;
            if !case.text.as_bytes()[0].is_ascii_uppercase() {
                return Err(Diagnostic::new(
                    "E1020",
                    "an active pattern case starts with an uppercase letter",
                    case.span,
                ));
            }
            if cases
                .iter()
                .any(|previous: &Ident| previous.text == case.text)
            {
                return Err(duplicate_active_case(&case));
            }
            if cases.len() == MAX_NESTING {
                return Err(self.error("too many active pattern cases"));
            }
            cases.push(case);
            self.expect(&TokenKind::Pipe, "'|' after the active pattern case")?;
            if matches!(&self.current().kind, TokenKind::Ident(name) if name == "_") {
                if cases.len() != 1 {
                    return Err(Diagnostic::new(
                        "E1020",
                        "partial active patterns have exactly one case",
                        self.current().span,
                    ));
                }
                self.take();
                self.expect(&TokenKind::Pipe, "'|' after '_'")?;
                break true;
            }
            if self.at(&TokenKind::RightParen) {
                break false;
            }
        };
        self.expect(&TokenKind::RightParen, "')' after active pattern cases")?;
        let function = format!(
            "$active.{}.{}",
            cases
                .iter()
                .map(|case| case.text.as_str())
                .collect::<Vec<_>>()
                .join("."),
            if partial { "partial" } else { "total" }
        );
        let span = cases[0].span;
        self.active_patterns
            .entry(function.clone())
            .or_insert_with(|| ActivePattern {
                cases,
                function: function.clone(),
                partial,
            });
        Ok(Ident {
            text: function,
            span,
            provenance: Provenance::Generated,
        })
    }

    pub(super) fn implicit_active_union(
        &self,
        mut function: FunctionDecl,
        unions: &mut Vec<UnionDecl>,
    ) -> Result<FunctionDecl, Diagnostic> {
        let Some(active) = self.active_patterns.get(&function.name.text) else {
            return Ok(function);
        };
        if active.cases.len() < 2 {
            return Ok(function);
        }
        if !matches!(&function.result.kind, TypeExprKind::Variable(name) if name == "T") {
            return Err(Diagnostic::new(
                "E1020",
                "multi-case active patterns use the implicit result type 'T, not an explicit union",
                function.result.span,
            ));
        }
        let union_name = Ident {
            text: format!("Active{}$result", active.function.replace('.', "$")),
            span: function.name.span,
            provenance: Provenance::Generated,
        };
        let mut payload_cases = BTreeSet::new();
        function.body.visit(&mut |expression| {
            let callee = match &expression.kind {
                ExprKind::Call(callee, arguments) if !arguments.is_empty() => Some(callee),
                ExprKind::Binary(BinaryOp::Pipe, _, callee) => Some(callee),
                _ => None,
            };
            if let Some(Expr {
                kind: ExprKind::Name(name),
                ..
            }) = callee.map(Box::as_ref)
            {
                if active.cases.iter().any(|case| case.text == name.text) {
                    payload_cases.insert(name.text.clone());
                }
            }
        });
        let parameters: Vec<_> = active
            .cases
            .iter()
            .enumerate()
            .filter(|(_, case)| payload_cases.contains(&case.text))
            .map(|(index, case)| Ident {
                text: format!("$active_payload{index}"),
                span: case.span,
                provenance: Provenance::Generated,
            })
            .collect();
        let arguments: Box<[_]> = parameters
            .iter()
            .map(|parameter| TypeExpr {
                kind: TypeExprKind::Variable(parameter.text.clone()),
                span: parameter.span,
            })
            .collect();
        function.result.kind = if arguments.is_empty() {
            TypeExprKind::Named(union_name.text.clone())
        } else {
            TypeExprKind::Apply(Box::new(union_name.clone()), arguments)
        };
        let mut cases = Vec::new();
        let mut payloads = parameters.iter();
        for case in &active.cases {
            let payload = payload_cases.contains(&case.text).then(|| {
                let parameter = payloads.next().unwrap();
                TypeExpr {
                    kind: TypeExprKind::Variable(parameter.text.clone()),
                    span: case.span,
                }
            });
            let name = Ident {
                text: format!("{}${}", union_name.text, case.text),
                span: case.span,
                provenance: Provenance::Generated,
            };
            cases.push(UnionCaseDecl { name, payload });
        }
        unions.push(UnionDecl {
            doc: None,
            visibility: function.visibility,
            name: union_name,
            parameters,
            cases,
            derives: Vec::new(),
        });
        Ok(function)
    }

    pub(super) fn range_expression(
        &mut self,
        first: Expr,
        allow_record: bool,
        stop_at_newline: bool,
    ) -> Result<Expr, Diagnostic> {
        let next = self.expression_inner(1, allow_record, stop_at_newline)?;
        let (step, finish) = if self.eat(&TokenKind::DotDot) {
            (
                Some(Box::new(next)),
                self.expression_inner(1, allow_record, stop_at_newline)?,
            )
        } else {
            (None, next)
        };
        let span = first.span.through(finish.span);
        let depth = first
            .depth
            .max(finish.depth)
            .max(step.as_ref().map_or(0, |step| step.depth))
            + 1;
        self.make(
            ExprKind::Range {
                start: Box::new(first),
                step,
                finish: Box::new(finish),
                counted: false,
                descending: false,
            },
            span,
            depth,
        )
    }

    /// `$"a{x}b"`: one expression per hole between the interpolation tokens.
    /// Each hole's expression counts toward the nesting limit as in a group.
    pub(super) fn interpolation(&mut self) -> Result<Expr, Diagnostic> {
        let outer_arm = std::mem::replace(&mut self.stop_at_arm, false);
        let outer_arrow = std::mem::replace(&mut self.stop_at_arrow, false);
        let outer_slice = std::mem::replace(&mut self.slice_context, false);
        let result = self.interpolation_holes();
        self.stop_at_arm = outer_arm;
        self.stop_at_arrow = outer_arrow;
        self.slice_context = outer_slice;
        result
    }

    fn interpolation_holes(&mut self) -> Result<Expr, Diagnostic> {
        let first = self.take();
        let TokenKind::InterpolationStart(piece) = first.kind else {
            unreachable!("an interpolation starts at its first token")
        };
        let mut texts = vec![piece.text];
        let mut holes: Vec<InterpolationHole> = Vec::new();
        let mut depth = 0;
        let last = loop {
            let mut brace = self.tokens[self.position - 1].span;
            brace.start = brace.end - 1;
            if holes.len() == MAX_INTERPOLATION_HOLES {
                return Err(Diagnostic::new(
                    "E0002",
                    format!(
                        "an interpolated string has at most {MAX_INTERPOLATION_HOLES} holes; split it into several strings"
                    ),
                    brace,
                ));
            }
            if matches!(
                self.current().kind,
                TokenKind::InterpolationMiddle(_) | TokenKind::InterpolationEnd(_)
            ) {
                let mut span = brace;
                span.end = self.current().span.start + 1;
                return Err(Diagnostic::new(
                    "E0002",
                    "an interpolation hole needs an expression; write '{{' for a literal brace",
                    span,
                ));
            }
            let value = self.expression(0, true)?;
            depth = depth.max(value.depth);
            if !matches!(
                self.current().kind,
                TokenKind::InterpolationMiddle(_) | TokenKind::InterpolationEnd(_)
            ) {
                return Err(self.error("expected '}' to close the interpolation hole"));
            }
            let token = self.take();
            let (mut piece, last) = match token.kind {
                TokenKind::InterpolationMiddle(piece) => (piece, false),
                TokenKind::InterpolationEnd(piece) => (piece, true),
                _ => unreachable!("the token kind was checked above"),
            };
            if let Some(spec) = &mut piece.spec {
                spec.span.source = token.span.source;
            }
            holes.push(InterpolationHole {
                value,
                spec: piece.spec,
            });
            texts.push(piece.text);
            if last {
                break token.span;
            }
        };
        let span = first.span.through(last);
        self.make(
            ExprKind::Interpolated(Box::new(Interpolation { texts, holes })),
            span,
            depth + 1,
        )
    }

    pub(super) fn grouped_expression(&mut self) -> Result<Expr, Diagnostic> {
        let start = self.take().span;
        let outer = self.stop_at_arm;
        let outer_arrow = self.stop_at_arrow;
        self.stop_at_arm = false;
        self.stop_at_arrow = false;
        let mut values = self.expressions(TokenKind::RightParen)?;
        self.stop_at_arm = outer;
        self.stop_at_arrow = outer_arrow;
        let span = start.through(self.tokens[self.position - 1].span);
        match values.len() {
            0 => self.make(ExprKind::Unit, span, 1),
            1 => {
                let mut value = values.pop().unwrap();
                value.span = span;
                Ok(value)
            }
            _ => {
                let depth = values.iter().map(|value| value.depth).max().unwrap() + 1;
                self.make(ExprKind::Tuple(values), span, depth)
            }
        }
    }

    pub(super) fn column(&self, span: Span) -> usize {
        let prefix = &self.source[..span.start];
        span.start - prefix.rfind(['\n', '\r']).map_or(0, |index| index + 1)
    }

    pub(super) fn body_expression(&mut self) -> Result<Expr, Diagnostic> {
        if self.at(&TokenKind::LeftBrace) {
            self.block()
        } else if self.newline_before_current() {
            let start = self.position;
            let body = self.layout_block(self.column(self.current().span))?;
            if self.position == start {
                return Err(self.error("expected a body expression"));
            }
            Ok(body)
        } else if self.implicit_computation_ahead(Some(self.column(self.current().span))) {
            self.implicit_computation(Some(self.column(self.current().span)), true)
        } else {
            self.expression_inner(0, true, true)
        }
    }

    pub(super) fn layout_block(&mut self, indent: usize) -> Result<Expr, Diagnostic> {
        if self.implicit_computation_ahead(Some(indent)) {
            return self.implicit_computation(Some(indent), false);
        }
        self.enter()?;
        let start = self.current().span;
        let mut bindings = Vec::new();
        let mut result: Option<Expr> = None;
        loop {
            if matches!(
                self.current().kind,
                TokenKind::End
                    | TokenKind::RightBrace
                    | TokenKind::RightParen
                    | TokenKind::RightBracket
                    | TokenKind::RightList
                    | TokenKind::Comma
                    | TokenKind::Else
                    | TokenKind::Elif
                    | TokenKind::Pipe
                    | TokenKind::Def
                    | TokenKind::Fn
                    | TokenKind::And
                    | TokenKind::Export
                    | TokenKind::Private
                    | TokenKind::Record
                    | TokenKind::Union
                    | TokenKind::Test
                    | TokenKind::Class
                    | TokenKind::Instance
                    | TokenKind::With
            ) || self.column(self.current().span) < indent
                || (self.in_handler
                    && matches!(&self.current().kind, TokenKind::Ident(name) if name == "finally"))
            {
                break;
            }
            if let Some(value) = result.take() {
                bindings.push(Binding {
                    name: Ident {
                        text: "_".into(),
                        span: value.span,
                        provenance: Provenance::Generated,
                    },
                    mutable: false,
                    using: false,
                    annotation: Some(TypeExpr {
                        kind: TypeExprKind::Named("unit".into()),
                        span: value.span,
                    }),
                    value,
                });
            }
            let checked = self.checked_attribute();
            if self.eat(&TokenKind::Let) {
                let mut binding = self.binding(true, false)?;
                if checked {
                    binding.value = self.checked(binding.value)?;
                }
                bindings.push(binding);
            } else if self.use_binding_ahead() {
                self.take();
                let mut binding = self.binding(true, true)?;
                if checked {
                    binding.value = self.checked(binding.value)?;
                }
                bindings.push(binding);
            } else {
                let mut value = self.expression_inner(0, true, true)?;
                if checked {
                    value = self.checked(value)?;
                }
                if self.eat(&TokenKind::Semicolon) {
                    bindings.push(Binding {
                        name: Ident {
                            text: "_".into(),
                            span: value.span,
                            provenance: Provenance::Generated,
                        },
                        mutable: false,
                        using: false,
                        annotation: None,
                        value,
                    });
                } else {
                    result = Some(value);
                    if !self.newline_before_current() {
                        break;
                    }
                }
            }
        }
        let result = result.unwrap_or(self.make(ExprKind::Unit, self.current().span, 1)?);
        let depth = bindings
            .iter()
            .map(|binding| binding.value.depth)
            .max()
            .unwrap_or(0)
            .max(result.depth)
            + 1;
        let span = start.through(result.span);
        self.nesting -= 1;
        self.make(
            ExprKind::Block {
                bindings,
                result: Box::new(result),
            },
            span,
            depth,
        )
    }

    pub(super) fn type_product(&mut self) -> Result<TypeExpr, Diagnostic> {
        let first = self.type_primary()?;
        if !self.eat(&TokenKind::Star) {
            return Ok(first);
        }
        let mut elements = vec![first];
        loop {
            elements.push(self.type_primary()?);
            if elements.len() > MAX_NESTING {
                return Err(self.error("too many tuple elements"));
            }
            if !self.eat(&TokenKind::Star) {
                break;
            }
        }
        let span = elements[0].span.through(elements.last().unwrap().span);
        Ok(TypeExpr {
            kind: TypeExprKind::Tuple(elements),
            span,
        })
    }

    pub(super) fn while_expression(&mut self) -> Result<Expr, Diagnostic> {
        self.enter()?;
        let start = self.take().span;
        let condition = self.expression(0, false)?;
        self.expect(&TokenKind::Do, "'do' after the while condition")?;
        let body = self.body_expression()?;
        let depth = condition.depth.max(body.depth) + 1;
        let span = start.through(body.span);
        self.nesting -= 1;
        self.make(
            ExprKind::While {
                condition: Box::new(condition),
                body: Box::new(body),
            },
            span,
            depth,
        )
    }

    pub(super) fn for_expression(&mut self) -> Result<Expr, Diagnostic> {
        self.enter()?;
        let start = self.take().span;
        let pattern = Box::new(self.pattern(0)?);
        let source = self.for_source(&pattern)?;
        self.expect(&TokenKind::Do, "'do' after the enumerable expression")?;
        let body = self.body_expression()?;
        let span = start.through(body.span);
        let depth = source.depth.max(body.depth).max(pattern.depth) + 1;
        self.nesting -= 1;
        self.make(
            ExprKind::For {
                pattern,
                source: Box::new(source),
                body: Box::new(body),
            },
            span,
            depth,
        )
    }

    fn for_source(&mut self, pattern: &Pattern) -> Result<Expr, Diagnostic> {
        if self.eat(&TokenKind::Equal) {
            if !matches!(pattern.kind, PatternKind::Binding(_)) {
                return Err(Diagnostic::new(
                    "E0002",
                    "a for...to loop requires an identifier",
                    pattern.span,
                ));
            }
            let first = self.expression(0, false)?;
            let descending = self.eat(&TokenKind::Downto);
            if !descending {
                self.expect(&TokenKind::To, "'to' or 'downto' after the initial value")?;
            }
            let finish = self.expression(0, false)?;
            let span = first.span.through(finish.span);
            let depth = first.depth.max(finish.depth) + 1;
            self.make(
                ExprKind::Range {
                    start: Box::new(first),
                    step: None,
                    finish: Box::new(finish),
                    counted: true,
                    descending,
                },
                span,
                depth,
            )
        } else {
            self.expect(&TokenKind::In, "'in' or '=' after the loop pattern")?;
            self.expression(0, false)
        }
    }

    pub(super) fn explicit_lambda(&mut self) -> Result<Expr, Diagnostic> {
        self.enter()?;
        let start = self.take().span;
        let mut parameters = Vec::new();
        let mut patterns = Vec::new();
        let mut guard = None;
        while !self.eat(&TokenKind::Arrow) {
            if !parameters.is_empty() && self.eat(&TokenKind::When) {
                // `\x when condition -> body` traps when the condition is false.
                let outer_arrow = std::mem::replace(&mut self.stop_at_arrow, true);
                let condition = self.expression_inner(0, true, true);
                self.stop_at_arrow = outer_arrow;
                guard = Some(condition?);
                self.expect(&TokenKind::Arrow, "'->' after the lambda guard")?;
                break;
            }
            let mutable = self.eat(&TokenKind::Mut);
            let pattern = self.pattern(4)?;
            let name = match &pattern.kind {
                PatternKind::Binding(name)
                    if !name.text.as_bytes()[0].is_ascii_uppercase()
                        && !name.text.contains('.') =>
                {
                    name.clone()
                }
                _ => {
                    let name = Ident {
                        text: format!("$lambda{}", pattern.span.start),
                        span: pattern.span,
                        provenance: Provenance::Generated,
                    };
                    patterns.push((name.clone(), pattern));
                    name
                }
            };
            parameters.push((name, mutable));
            if parameters.len() > MAX_NESTING {
                return Err(self.error("too many lambda parameters"));
            }
        }
        if parameters.is_empty() {
            return Err(
                self.error("a lambda requires at least one parameter; use '\\() -> ...' for unit")
            );
        }
        let outer = self.in_task;
        self.in_task = false;
        let mut body = if guard.is_none() && self.at(&TokenKind::Pipe) {
            self.guarded_definition(&parameters)?
        } else {
            self.body_expression()?
        };
        self.in_task = outer;
        if let Some(guard) = guard {
            // The guard sees the destructured names, so it wraps the body inside the patterns.
            let span = guard.span.through(body.span);
            let depth = body.depth.max(guard.depth) + 1;
            let value = self.make(ExprKind::Unit, guard.span, 1)?;
            let pattern = self.make_pattern(PatternKind::Wildcard, guard.span, 1)?;
            body = self.make(
                ExprKind::Match {
                    value: Box::new(value),
                    arms: vec![MatchArm {
                        pattern,
                        guard: Some(guard),
                        body,
                        span,
                    }],
                    origin: MatchOrigin::LambdaDestructuring,
                },
                span,
                depth,
            )?;
        }
        for (name, pattern) in patterns.into_iter().rev() {
            let value = self.make(ExprKind::Name(name.clone()), name.span, 1)?;
            let span = pattern.span.through(body.span);
            let depth = body.depth.max(pattern.depth) + 1;
            body = self.make(
                ExprKind::Match {
                    value: Box::new(value),
                    arms: vec![MatchArm {
                        pattern,
                        guard: None,
                        body,
                        span,
                    }],
                    origin: MatchOrigin::LambdaDestructuring,
                },
                span,
                depth,
            )?;
        }
        let span = start.through(body.span);
        let depth = body.depth + 1;
        self.nesting -= 1;
        self.make(ExprKind::Lambda(parameters, Box::new(body)), span, depth)
    }

    pub(super) fn match_expression(&mut self) -> Result<Expr, Diagnostic> {
        self.enter()?;
        let start = self.take().span;
        let value = self.expression(0, true)?;
        self.expect(&TokenKind::With, "'with' after the matched expression")?;
        let arms = self.match_arms(None)?;
        self.nesting -= 1;
        self.make_match(value, arms, start, MatchOrigin::Explicit)
    }

    /// `@checked expression`: the attribute covers the whole expression after it.
    #[inline(never)]
    pub(super) fn checked_expression(
        &mut self,
        allow_record: bool,
        stop_at_newline: bool,
    ) -> Result<Expr, Diagnostic> {
        let start = self.current().span;
        if !self.checked_attribute() {
            return Err(self.error(
                "expected an expression; '@checked' is the only attribute of an expression",
            ));
        }
        if stop_at_newline && self.newline_before_current() {
            return Err(self.error(
                "'@checked' on its own line applies to the next statement of a body or block",
            ));
        }
        self.enter()?;
        let value = self.expression_inner(0, allow_record, stop_at_newline)?;
        self.nesting -= 1;
        let span = start.through(value.span);
        let depth = value.depth + 1;
        self.make(ExprKind::Checked(Box::new(value)), span, depth)
    }

    /// `try body with | pattern [is Exception] [when guard] -> handler ... [finally cleanup]`.
    /// The body, handlers, and cleanup run IO from `let!`/`do!` directly.
    #[inline(never)]
    pub(super) fn try_expression(&mut self) -> Result<Expr, Diagnostic> {
        self.enter()?;
        let start = self.take().span;
        let indent = self.line_indent(start);
        let outer_arm = std::mem::replace(&mut self.stop_at_arm, false);
        let outer_arrow = std::mem::replace(&mut self.stop_at_arrow, false);
        let outer_handler = std::mem::replace(&mut self.in_handler, false);
        let body = self.body_expression();
        self.stop_at_arm = outer_arm;
        self.stop_at_arrow = outer_arrow;
        let body = Self::direct(body?);
        self.expect(
            &TokenKind::With,
            "'with' and '| pattern -> handler' arms after the try body",
        )?;
        self.in_handler = true;
        self.exception_arms = true;
        let arms = self.match_arms(None);
        self.in_handler = outer_handler;
        let mut arms = arms?;
        for arm in &mut arms {
            let body = std::mem::replace(
                &mut arm.body,
                Expr {
                    kind: ExprKind::Unit,
                    span: arm.span,
                    depth: 1,
                },
            );
            arm.body = Self::direct(body);
        }
        let finally = if matches!(&self.current().kind, TokenKind::Ident(name) if name == "finally")
            && (!self.newline_before_current() || self.column(self.current().span) >= indent)
        {
            self.take();
            Some(Self::direct(self.body_expression()?))
        } else {
            None
        };
        let end = finally
            .as_ref()
            .map_or_else(|| arms.last().unwrap().body.span, |cleanup| cleanup.span);
        let depth = arms
            .iter()
            .map(|arm| {
                arm.pattern
                    .depth
                    .max(arm.body.depth)
                    .max(arm.guard.as_ref().map_or(0, |guard| guard.depth))
            })
            .chain(finally.iter().map(|cleanup| cleanup.depth))
            .max()
            .unwrap_or(0)
            .max(body.depth)
            + 1;
        self.nesting -= 1;
        self.make(
            ExprKind::Try(Box::new(TryExpr {
                body,
                arms,
                finally,
            })),
            start.through(end),
            depth,
        )
    }

    /// Marks an implicit computation body to run its IO binds directly.
    fn direct(mut body: Expr) -> Expr {
        if let ExprKind::Computation(builder, _) = &mut body.kind
            && builder.text == "$implicit"
        {
            builder.text = "$implicit.root.$direct".into();
        }
        body
    }

    /// The column of the first token on the line that holds `span`.
    pub(super) fn line_indent(&self, span: Span) -> usize {
        let line = self.source[..span.start]
            .rfind(['\n', '\r'])
            .map_or(0, |index| index + 1);
        self.source[line..span.start]
            .chars()
            .take_while(|character| *character == ' ' || *character == '\t')
            .count()
    }

    /// `pattern is Kind` in a handler arm matches an exception of that kind:
    /// `{ kind = Kind } & pattern`.
    #[inline(never)]
    fn exception_pattern(&mut self, pattern: Pattern) -> Result<Pattern, Diagnostic> {
        let start = self.take().span;
        let kind = self.qualified_ident()?;
        let span = pattern.span.through(kind.span);
        let field = Ident {
            text: "kind".into(),
            span: start,
            provenance: Provenance::Generated,
        };
        let case = Pattern {
            kind: PatternKind::Binding(kind),
            span,
            depth: 1,
        };
        let test = Pattern {
            kind: PatternKind::Record(None, vec![(field, case)]),
            span,
            depth: 2,
        };
        let depth = pattern.depth.max(test.depth) + 1;
        Ok(Pattern {
            kind: PatternKind::And(Box::new(test), Box::new(pattern)),
            span,
            depth,
        })
    }

    pub(super) fn guarded_definition(
        &mut self,
        parameters: &[(Ident, bool)],
    ) -> Result<Expr, Diagnostic> {
        self.enter()?;
        let start = self.current().span;
        let values = parameters
            .iter()
            .map(|(name, _)| self.make(ExprKind::Name(name.clone()), name.span, 1))
            .collect::<Result<Vec<_>, _>>()?;
        let mut value = match values.len() {
            0 => self.make(ExprKind::Unit, start, 1)?,
            1 => values.into_iter().next().unwrap(),
            _ => self.make(ExprKind::Tuple(values), start, 2)?,
        };
        let arms = self.match_arms(Some(parameters))?;
        if arms
            .iter()
            .all(|arm| matches!(arm.pattern.kind, PatternKind::Wildcard))
        {
            value = self.make(ExprKind::Unit, start, 1)?;
        }
        let result = self.make_match(value, arms, start, MatchOrigin::FunctionGuard)?;
        if !matches!(&self.current().kind, TokenKind::Ident(name) if name == "where")
            || !self.newline_before_current()
            || self.column(self.current().span) != self.column(start)
        {
            self.nesting -= 1;
            return Ok(result);
        }
        self.take();
        let indent = self.column(self.current().span);
        if !self.newline_before_current() || indent <= self.column(start) {
            return Err(self.error("expected indented bindings after 'where'"));
        }
        let mut bindings = Vec::new();
        loop {
            bindings.push(self.binding_value(true)?);
            if self.at(&TokenKind::End)
                || !self.newline_before_current()
                || self.column(self.current().span) < indent
            {
                break;
            }
            if self.column(self.current().span) != indent {
                return Err(self.error("where bindings must have the same indentation"));
            }
        }
        let span = start.through(bindings.last().unwrap().value.span);
        let depth = bindings
            .iter()
            .map(|binding| binding.value.depth)
            .max()
            .unwrap()
            .max(result.depth)
            + 1;
        self.nesting -= 1;
        self.make(
            ExprKind::Block {
                bindings,
                result: Box::new(result),
            },
            span,
            depth,
        )
    }

    fn make_match(
        &self,
        value: Expr,
        arms: Vec<MatchArm>,
        start: Span,
        origin: MatchOrigin,
    ) -> Result<Expr, Diagnostic> {
        let span = start.through(arms.last().unwrap().body.span);
        let depth = arms
            .iter()
            .map(|arm| {
                arm.pattern
                    .depth
                    .max(arm.body.depth)
                    .max(arm.guard.as_ref().map_or(0, |guard| guard.depth))
            })
            .max()
            .unwrap()
            .max(value.depth)
            + 1;
        self.make(
            ExprKind::Match {
                value: Box::new(value),
                arms,
                origin,
            },
            span,
            depth,
        )
    }

    fn match_arms(
        &mut self,
        function_parameters: Option<&[(Ident, bool)]>,
    ) -> Result<Vec<MatchArm>, Diagnostic> {
        let outer_arm = self.stop_at_arm;
        let outer_arrow = self.stop_at_arrow;
        // Only the arms of this `try` take `is`; matches inside the handlers do not.
        let exception_arms = std::mem::take(&mut self.exception_arms);
        let column = self.column(self.current().span);
        let mut arms = Vec::new();
        while self.at(&TokenKind::Pipe)
            && (!self.newline_before_current() || self.column(self.current().span) == column)
        {
            let start = self.take().span;
            self.stop_at_arm = true;
            self.stop_at_arrow = true;
            let predicate = function_parameters.is_some() && self.guard_predicate();
            let (pattern, mut guard) = if predicate {
                let condition = self.expression_inner(0, true, true)?;
                (
                    Pattern {
                        kind: PatternKind::Wildcard,
                        span: start,
                        depth: 1,
                    },
                    Some(condition),
                )
            } else {
                let pattern = self.pattern(0)?;
                if exception_arms
                    && matches!(&self.current().kind, TokenKind::Ident(name) if name == "is")
                {
                    (self.exception_pattern(pattern)?, None)
                } else {
                    (pattern, None)
                }
            };
            if self.eat(&TokenKind::When) {
                let condition = self.expression_inner(0, true, true)?;
                guard = Some(if let Some(first) = guard {
                    let span = first.span.through(condition.span);
                    let depth = first.depth.max(condition.depth) + 1;
                    self.make(
                        ExprKind::Binary(BinaryOp::And, Box::new(first), Box::new(condition)),
                        span,
                        depth,
                    )?
                } else {
                    condition
                });
            }
            self.expect(
                &TokenKind::Arrow,
                "'->' after the pattern and optional guard",
            )?;
            self.stop_at_arrow = false;
            let body = self.body_expression()?;
            let span = start.through(body.span);
            arms.push(MatchArm {
                pattern,
                guard,
                body,
                span,
            });
        }
        self.stop_at_arm = outer_arm;
        self.stop_at_arrow = outer_arrow;
        if arms.is_empty() {
            return Err(self.error("expected at least one '| pattern -> expression' arm"));
        }
        Ok(arms)
    }

    fn guard_predicate(&self) -> bool {
        if let TokenKind::Ident(name) = &self.current().kind {
            if name.as_bytes()[0].is_ascii_lowercase()
                && self.tokens.get(self.position + 1).is_some_and(|token| {
                    matches!(
                        token.kind,
                        TokenKind::Ident(_)
                            | TokenKind::LeftParen
                            | TokenKind::Integer(_)
                            | TokenKind::Float(_)
                            | TokenKind::String(_)
                            | TokenKind::InterpolationStart(_)
                            | TokenKind::Char(_)
                            | TokenKind::Utf8Char(_)
                            | TokenKind::True
                            | TokenKind::False
                            | TokenKind::Ref
                            | TokenKind::Deref
                    )
                })
            {
                return true;
            }
        }
        for token in &self.tokens[self.position..] {
            match token.kind {
                TokenKind::Arrow | TokenKind::When | TokenKind::End => return false,
                TokenKind::EqualEqual
                | TokenKind::BangEqual
                | TokenKind::Less
                | TokenKind::LessEqual
                | TokenKind::Greater
                | TokenKind::GreaterEqual
                | TokenKind::AndAnd
                | TokenKind::OrOr
                | TokenKind::Bang
                | TokenKind::Ref
                | TokenKind::Deref => return true,
                _ => {}
            }
        }
        false
    }

    fn make_pattern(
        &self,
        kind: PatternKind,
        span: Span,
        depth: usize,
    ) -> Result<Pattern, Diagnostic> {
        if depth > MAX_NESTING {
            return Err(Diagnostic::new(
                "E0002",
                "pattern nesting exceeds the compiler limit",
                span,
            ));
        }
        Ok(Pattern { kind, span, depth })
    }

    pub(super) fn pattern(&mut self, minimum: u8) -> Result<Pattern, Diagnostic> {
        self.enter()?;
        let mut left = self.pattern_atom()?;
        loop {
            let precedence = match self.current().kind {
                TokenKind::As | TokenKind::Colon => 0,
                TokenKind::Pipe => 1,
                TokenKind::Ampersand => 2,
                TokenKind::DoubleColon => 3,
                _ => break,
            };
            if precedence < minimum {
                break;
            }
            let token = self.take();
            left = self.pattern_suffix(left, token.kind, precedence)?;
        }
        self.nesting -= 1;
        Ok(left)
    }

    fn pattern_suffix(
        &mut self,
        left: Pattern,
        token: TokenKind,
        precedence: u8,
    ) -> Result<Pattern, Diagnostic> {
        if token == TokenKind::As {
            let name = self.ident()?;
            let span = left.span.through(name.span);
            let depth = left.depth + 1;
            self.make_pattern(PatternKind::As(Box::new(left), name), span, depth)
        } else if token == TokenKind::Colon {
            let ty = if self.pattern_type_arrow {
                self.type_expr()?
            } else {
                self.type_product()?
            };
            let span = left.span.through(ty.span);
            let depth = left.depth + 1;
            self.make_pattern(PatternKind::Annotated(Box::new(left), ty), span, depth)
        } else {
            let right = self.pattern(if precedence == 3 { 3 } else { precedence + 1 })?;
            let span = left.span.through(right.span);
            let depth = left.depth.max(right.depth) + 1;
            let kind = match token {
                TokenKind::Pipe => PatternKind::Or(Box::new(left), Box::new(right)),
                TokenKind::Ampersand => PatternKind::And(Box::new(left), Box::new(right)),
                _ => PatternKind::Cons(Box::new(left), Box::new(right)),
            };
            self.make_pattern(kind, span, depth)
        }
    }

    fn pattern_atom(&mut self) -> Result<Pattern, Diagnostic> {
        let start = self.current().span;
        let (kind, depth) = match &self.current().kind {
            TokenKind::Ident(_) => return self.named_pattern(),
            TokenKind::LeftBrace => return self.record_pattern(None),
            TokenKind::LeftParen => return self.grouped_pattern(),
            TokenKind::LeftBracket | TokenKind::LeftList => return self.collection_pattern(),
            TokenKind::Minus => {
                self.take();
                if !matches!(
                    self.current().kind,
                    TokenKind::Integer(_) | TokenKind::Float(_)
                ) {
                    return Err(self.error("a negative pattern requires a numeric literal"));
                }
                let value = self.literal()?;
                let span = start.through(value.span);
                let value =
                    self.make(ExprKind::Unary(UnaryOp::Negate, Box::new(value)), span, 2)?;
                (PatternKind::Literal(Box::new(value)), 1)
            }
            TokenKind::True | TokenKind::False => {
                let value = self.take().kind == TokenKind::True;
                (
                    PatternKind::Literal(Box::new(self.make(ExprKind::Bool(value), start, 1)?)),
                    1,
                )
            }
            TokenKind::Integer(_)
            | TokenKind::Float(_)
            | TokenKind::String(_)
            | TokenKind::Char(_)
            | TokenKind::Utf8Char(_) => (PatternKind::Literal(Box::new(self.literal()?)), 1),
            _ => return Err(self.error("expected a pattern")),
        };
        let span = start.through(self.tokens[self.position - 1].span);
        self.make_pattern(kind, span, depth)
    }

    fn named_pattern(&mut self) -> Result<Pattern, Diagnostic> {
        let name = self.qualified_path(18)?;
        if name.text == "null" {
            return Err(Diagnostic::new(
                "E1020",
                "Tsuzuri has no null values; null patterns are not supported",
                name.span,
            ));
        }
        if self.at(&TokenKind::LeftBrace) {
            return self.record_pattern(Some(name));
        }
        if name.text.rsplit('.').next().unwrap().as_bytes()[0].is_ascii_uppercase() {
            let mut arguments = Vec::new();
            while !self.newline_before_current()
                && matches!(
                    self.current().kind,
                    TokenKind::Ident(_)
                        | TokenKind::LeftParen
                        | TokenKind::LeftBracket
                        | TokenKind::LeftList
                        | TokenKind::Integer(_)
                        | TokenKind::Float(_)
                        | TokenKind::String(_)
                        | TokenKind::Char(_)
                        | TokenKind::Utf8Char(_)
                        | TokenKind::True
                        | TokenKind::False
                        | TokenKind::Minus
                )
            {
                arguments.push(self.pattern(4)?);
            }
            if !arguments.is_empty() {
                let span = name.span.through(arguments.last().unwrap().span);
                let depth = arguments
                    .iter()
                    .map(|argument| argument.depth)
                    .max()
                    .unwrap()
                    + 1;
                return self.make_pattern(PatternKind::Apply(name, arguments), span, depth);
            }
        }
        let span = name.span;
        let kind = if name.text == "_" || name.text == "otherwise" {
            PatternKind::Wildcard
        } else {
            PatternKind::Binding(name)
        };
        self.make_pattern(kind, span, 1)
    }

    fn grouped_pattern(&mut self) -> Result<Pattern, Diagnostic> {
        let start = self.take().span;
        if self.pattern_argument_expression() {
            return self.pattern_expression_argument(start);
        }
        if self.eat(&TokenKind::RightParen) {
            let span = start.through(self.tokens[self.position - 1].span);
            let value = self.make(ExprKind::Unit, span, 1)?;
            return self.make_pattern(PatternKind::Literal(Box::new(value)), span, 1);
        }
        let outer_type_arrow = std::mem::replace(&mut self.pattern_type_arrow, true);
        let mut first = self.pattern(0)?;
        if !self.eat(&TokenKind::Comma) {
            let end = self.expect(&TokenKind::RightParen, "')' after the pattern")?;
            first.span = start.through(end.span);
            self.pattern_type_arrow = outer_type_arrow;
            return Ok(first);
        }
        let mut values = vec![first];
        loop {
            values.push(self.pattern(0)?);
            if !self.eat(&TokenKind::Comma) {
                break;
            }
        }
        let end = self.expect(&TokenKind::RightParen, "')' after the tuple pattern")?;
        self.pattern_type_arrow = outer_type_arrow;
        let depth = values.iter().map(|value| value.depth).max().unwrap() + 1;
        self.make_pattern(PatternKind::Tuple(values), start.through(end.span), depth)
    }

    fn pattern_expression_argument(&mut self, start: Span) -> Result<Pattern, Diagnostic> {
        let outer = self.stop_at_arm;
        let outer_arrow = self.stop_at_arrow;
        self.stop_at_arm = false;
        self.stop_at_arrow = false;
        let expression = self.expression(0, true)?;
        self.stop_at_arm = outer;
        self.stop_at_arrow = outer_arrow;
        let end = self.expect(
            &TokenKind::RightParen,
            "')' after the active pattern argument",
        )?;
        let depth = expression.depth + 1;
        self.make_pattern(
            PatternKind::Argument(Box::new(expression)),
            start.through(end.span),
            depth,
        )
    }

    fn pattern_argument_expression(&self) -> bool {
        if matches!(
            self.current().kind,
            TokenKind::Ampersand
                | TokenKind::If
                | TokenKind::Backslash
                | TokenKind::Task
                | TokenKind::New
        ) {
            return true;
        }
        let mut depth = 0usize;
        for (index, token) in self.tokens[self.position..].iter().enumerate() {
            match token.kind {
                TokenKind::RightParen if depth == 0 => break,
                TokenKind::LeftParen => depth += 1,
                TokenKind::RightParen => depth -= 1,
                TokenKind::Plus
                | TokenKind::Star
                | TokenKind::Slash
                | TokenKind::Percent
                | TokenKind::EqualEqual
                | TokenKind::BangEqual
                | TokenKind::Less
                | TokenKind::Greater
                | TokenKind::LessEqual
                | TokenKind::GreaterEqual
                | TokenKind::AndAnd
                | TokenKind::OrOr
                | TokenKind::Bang
                | TokenKind::Ref
                | TokenKind::Deref => return true,
                TokenKind::Minus
                    if index != 0
                        || !matches!(
                            self.tokens.get(self.position + 1).map(|token| &token.kind),
                            Some(TokenKind::Integer(_) | TokenKind::Float(_))
                        ) =>
                {
                    return true;
                }
                TokenKind::Comma
                | TokenKind::Colon
                | TokenKind::DoubleColon
                | TokenKind::Arrow
                | TokenKind::End
                    if depth == 0 =>
                {
                    break;
                }
                _ => {}
            }
            if matches!(&token.kind, TokenKind::Ident(name) if name.as_bytes()[0].is_ascii_lowercase())
                && self
                    .tokens
                    .get(self.position + index + 1)
                    .is_some_and(|next| {
                        matches!(next.kind, TokenKind::Ident(_) | TokenKind::LeftParen)
                    })
            {
                return true;
            }
        }
        false
    }

    fn collection_pattern(&mut self) -> Result<Pattern, Diagnostic> {
        let start = self.take();
        let list = start.kind == TokenKind::LeftList;
        let end = if list {
            TokenKind::RightList
        } else {
            TokenKind::RightBracket
        };
        let mut values = Vec::new();
        while !self.at(&end) {
            values.push(self.pattern(0)?);
            if !self.eat(&TokenKind::Comma) && !self.eat(&TokenKind::Semicolon) {
                break;
            }
        }
        let end = self.expect(&end, "the end of the collection pattern")?;
        let depth = values.iter().map(|value| value.depth).max().unwrap_or(0) + 1;
        let kind = if list {
            PatternKind::List(values)
        } else {
            PatternKind::Array(values)
        };
        self.make_pattern(kind, start.span.through(end.span), depth)
    }

    fn record_pattern(&mut self, name: Option<Ident>) -> Result<Pattern, Diagnostic> {
        let start = name.as_ref().map_or(self.current().span, |name| name.span);
        self.expect(&TokenKind::LeftBrace, "'{' in the record pattern")?;
        let mut fields = Vec::new();
        while !self.at(&TokenKind::RightBrace) {
            let field = self.ident()?;
            if !self.eat(&TokenKind::Colon) {
                self.expect(&TokenKind::Equal, "':' or '=' before a field pattern")?;
            }
            fields.push((field, self.pattern(0)?));
            if !self.eat(&TokenKind::Comma) && !self.eat(&TokenKind::Semicolon) {
                break;
            }
        }
        let end = self.expect(&TokenKind::RightBrace, "'}' after the record pattern")?;
        let depth = fields
            .iter()
            .map(|(_, pattern)| pattern.depth)
            .max()
            .unwrap_or(0)
            + 1;
        self.make_pattern(
            PatternKind::Record(name, fields),
            start.through(end.span),
            depth,
        )
    }
}
