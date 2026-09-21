use crate::diagnostic::{Diagnostic, Span};
use crate::lexer::lex;
use crate::syntax::*;
use std::collections::{BTreeMap, BTreeSet};

pub fn parse(source: &str) -> Result<Program, Diagnostic> {
    parse_source(source, None)
}

pub fn parse_with_source(source: &str, source_id: usize) -> Result<Program, Diagnostic> {
    parse_source(source, Some(source_id))
}

fn parse_source(source: &str, source_id: Option<usize>) -> Result<Program, Diagnostic> {
    let mut tokens = lex(source).map_err(|mut error| {
        error.span.source = source_id;
        error
    })?;
    for token in &mut tokens {
        token.span.source = source_id;
    }
    Parser {
        source,
        tokens,
        position: 0,
        nesting: 0,
        in_task: false,
    }
    .program()
}

struct Parser<'a> {
    source: &'a str,
    tokens: Vec<Token>,
    position: usize,
    nesting: usize,
    in_task: bool,
}

impl Parser<'_> {
    fn current(&self) -> &Token {
        &self.tokens[self.position]
    }

    fn at(&self, kind: &TokenKind) -> bool {
        std::mem::discriminant(&self.current().kind) == std::mem::discriminant(kind)
    }

    fn take(&mut self) -> Token {
        let token = self.current().clone();
        if !self.at(&TokenKind::End) {
            self.position += 1;
        }
        token
    }

    fn eat(&mut self, kind: &TokenKind) -> bool {
        if self.at(kind) {
            self.take();
            true
        } else {
            false
        }
    }

    fn expect(&mut self, kind: &TokenKind, description: &str) -> Result<Token, Diagnostic> {
        if self.at(kind) {
            Ok(self.take())
        } else {
            Err(self.error(format!("expected {description}")))
        }
    }

    fn error(&self, message: impl Into<String>) -> Diagnostic {
        Diagnostic::new("E0002", message, self.current().span)
    }

    fn enter(&mut self) -> Result<(), Diagnostic> {
        self.nesting += 1;
        if self.nesting > MAX_NESTING {
            Err(self.error(format!("syntax nesting exceeds {MAX_NESTING}")))
        } else {
            Ok(())
        }
    }

    fn ident(&mut self) -> Result<Ident, Diagnostic> {
        if let TokenKind::Ident(text) = &self.current().kind {
            let result = Ident {
                text: text.clone(),
                span: self.current().span,
            };
            self.take();
            Ok(result)
        } else {
            Err(self.error("expected an identifier"))
        }
    }

    fn program(mut self) -> Result<Program, Diagnostic> {
        let mut program = Program {
            records: Vec::new(),
            functions: Vec::new(),
            classes: Vec::new(),
            instances: Vec::new(),
            entry: None,
        };
        let mut signatures = BTreeMap::new();
        let mut definitions = Vec::new();
        let mut defined_names = BTreeSet::new();
        while !self.at(&TokenKind::End) {
            if self.eat(&TokenKind::Record) {
                let name = self.ident()?;
                self.expect(&TokenKind::LeftBrace, "'{' after the record name")?;
                let fields = self.parameters(TokenKind::RightBrace)?;
                program.records.push(RecordDecl { name, fields });
            } else if self.eat(&TokenKind::Class) {
                let name = self.ident()?;
                let variable = self.type_variable()?;
                self.expect(&TokenKind::LeftBrace, "'{' after the class parameter")?;
                let mut methods = Vec::new();
                while !self.eat(&TokenKind::RightBrace) {
                    self.expect(&TokenKind::Def, "a 'def' method signature or '}'")?;
                    let method = self.ident()?;
                    self.expect(&TokenKind::DoubleColon, "'::' before the method type")?;
                    methods.push(self.signature(method, false)?);
                    self.eat(&TokenKind::Semicolon);
                }
                program.classes.push(ClassDecl {
                    name,
                    variable,
                    methods,
                });
            } else if self.eat(&TokenKind::Instance) {
                let class = self.qualified_ident()?;
                let ty = self.type_atom()?;
                self.expect(&TokenKind::LeftBrace, "'{' after the instance type")?;
                let mut methods = Vec::new();
                while !self.eat(&TokenKind::RightBrace) {
                    let anonymous = self.eat(&TokenKind::Let);
                    if !anonymous {
                        self.expect(&TokenKind::Fn, "a 'fn' or 'let' method definition, or '}'")?;
                    }
                    let name = self.ident()?;
                    let definition = self.definition(name)?;
                    if anonymous && (!definition.parameters.is_empty() || !matches!(definition.body.kind, ExprKind::Lambda(..))) {
                        return Err(Diagnostic::new("E0002", "a 'let' method implementation needs a lambda", definition.name.span));
                    }
                    methods.push(definition);
                    self.eat(&TokenKind::Semicolon);
                }
                program.instances.push(InstanceDecl { class, ty, methods });
            } else if self.at(&TokenKind::Let)
                && self.tokens.get(self.position + 1).is_some_and(|token|
                    matches!(&token.kind, TokenKind::Ident(name) if signatures.contains_key(name) && !defined_names.contains(name)))
            {
                self.take();
                let name = self.ident()?;
                if !defined_names.insert(name.text.clone()) {
                    return Err(Diagnostic::new("E1001", "duplicate function definition", name.span));
                }
                self.expect(&TokenKind::Equal, "'=' before the anonymous function")?;
                let body = self.expression_inner(0, true, true)?;
                if !matches!(body.kind, ExprKind::Lambda(..)) {
                    return Err(Diagnostic::new("E0002", "a declared top-level 'let' function needs a lambda such as 'x -> y -> x + y'", body.span));
                }
                definitions.push(Definition { name, parameters: Vec::new(), body });
                self.eat(&TokenKind::Semicolon);
            } else if self.at(&TokenKind::Fn) || self.at(&TokenKind::Def) || self.at(&TokenKind::Export) {
                let exported = self.eat(&TokenKind::Export);
                let declaration = self.eat(&TokenKind::Def);
                if !declaration {
                    self.expect(&TokenKind::Fn, "'def', 'fn', or 'record'")?;
                }
                let name = self.ident()?;
                if declaration {
                    self.expect(&TokenKind::DoubleColon, "'::' after the declaration name")?;
                    let signature = self.signature(name.clone(), exported)?;
                    if signatures.insert(name.text.clone(), signature).is_some() {
                        return Err(Diagnostic::new(
                            "E1001",
                            "duplicate function signature",
                            name.span,
                        ));
                    }
                    if self.at(&TokenKind::DoubleColon) {
                        return Err(self.error("use 'def name :: ...' for a signature; 'fn' introduces its implementation"));
                    }
                    self.eat(&TokenKind::Semicolon);
                    continue;
                }
                if !defined_names.insert(name.text.clone()) {
                    return Err(Diagnostic::new(
                        "E1001",
                        "duplicate function definition",
                        name.span,
                    ));
                }
                if !self.at(&TokenKind::LeftParen) {
                    if exported {
                        return Err(self.error("put 'export' on the function signature"));
                    }
                    definitions.push(self.definition(name)?);
                    self.eat(&TokenKind::Semicolon);
                    continue;
                }
                self.expect(&TokenKind::LeftParen, "'(' after the function name")?;
                let parameters = self.parameters(TokenKind::RightParen)?;
                self.expect(&TokenKind::Arrow, "'->' and an explicit return type")?;
                let result = self.type_expr()?;
                let body = self.block()?;
                program.functions.push(FunctionDecl {
                    name,
                    exported,
                    parameters,
                    result,
                    constraints: Vec::new(),
                    body,
                });
            } else {
                if matches!(&self.current().kind, TokenKind::Ident(name) if name == "module" || name == "namespace")
                    && matches!(
                        self.tokens.get(self.position + 1).map(|token| &token.kind),
                        Some(TokenKind::Ident(_))
                    )
                {
                    return Err(self.error(
                        "module and namespace declarations are not supported; each .tzr file is one module named after its filename",
                    ));
                }
                program.entry = Some(self.entry()?);
                break;
            }
        }
        for definition in definitions {
            let signature = signatures.remove(&definition.name.text).ok_or_else(|| {
                Diagnostic::new(
                    "E0002",
                    "function definition needs a 'def name :: ...' signature",
                    definition.name.span,
                )
            })?;
            program.functions.push(Self::define(signature, definition)?);
        }
        if let Some(signature) = signatures.values().next() {
            return Err(Diagnostic::new(
                "E0002",
                "function signature has no definition",
                signature.name.span,
            ));
        }
        Ok(program)
    }

    fn qualified_ident(&mut self) -> Result<Ident, Diagnostic> {
        let mut name = self.ident()?;
        if self.eat(&TokenKind::Dot) {
            let member = self.ident()?;
            name.text.push('.');
            name.text.push_str(&member.text);
            name.span = name.span.through(member.span);
        }
        Ok(name)
    }

    fn type_variable(&mut self) -> Result<Ident, Diagnostic> {
        let token = self.expect(
            &TokenKind::TypeVariable(String::new()),
            "a type variable such as 'a",
        )?;
        let TokenKind::TypeVariable(text) = token.kind else {
            unreachable!()
        };
        Ok(Ident {
            text,
            span: token.span,
        })
    }

    fn signature(&mut self, name: Ident, exported: bool) -> Result<SignatureDecl, Diagnostic> {
        let mut constraints = Vec::new();
        // A constraint prefix always ends in => before the next declaration/body.
        let has_constraints = self.constraint_prefix();
        if has_constraints {
            let grouped = self.eat(&TokenKind::LeftParen);
            loop {
                let class = self.qualified_ident()?;
                let ty = self.type_atom()?;
                constraints.push(ConstraintExpr { class, ty });
                if !self.eat(&TokenKind::Comma) {
                    break;
                }
            }
            if grouped {
                self.expect(&TokenKind::RightParen, "')' after constraints")?;
            }
            self.expect(&TokenKind::FatArrow, "'=>' after constraints")?;
        }
        let ty = self.type_expr()?;
        let (mut parameters, mut result) = match ty.kind {
            TypeExprKind::Function(parameters, result) => (parameters, *result),
            _ => (Vec::new(), ty),
        };
        while !parameters.is_empty()
            && matches!(&result.kind, TypeExprKind::Function(parameters, _) if !parameters.is_empty())
        {
            let TypeExprKind::Function(more, tail) = result.kind else {
                unreachable!()
            };
            parameters.extend(more);
            result = *tail;
        }
        Ok(SignatureDecl {
            name,
            exported,
            parameters,
            result,
            constraints,
        })
    }

    fn constraint_prefix(&self) -> bool {
        for (offset, token) in self.tokens[self.position..].iter().enumerate() {
            match token.kind {
                TokenKind::FatArrow => return true,
                TokenKind::Fn
                    if self
                        .tokens
                        .get(self.position + offset + 1)
                        .is_some_and(|next| next.kind == TokenKind::LeftParen) => {}
                TokenKind::Def
                | TokenKind::Let
                | TokenKind::Fn
                | TokenKind::Equal
                | TokenKind::LeftBrace
                | TokenKind::RightBrace
                | TokenKind::End => return false,
                _ => {}
            }
        }
        false
    }

    fn definition(&mut self, name: Ident) -> Result<Definition, Diagnostic> {
        let mut parameters = Vec::new();
        while !self.at(&TokenKind::Equal) {
            let mutable = self.eat(&TokenKind::Mut);
            parameters.push((self.ident()?, mutable));
        }
        self.take();
        let body = self.expression_inner(0, true, true)?;
        Ok(Definition {
            name,
            parameters,
            body,
        })
    }

    pub(crate) fn define(
        signature: SignatureDecl,
        mut definition: Definition,
    ) -> Result<FunctionDecl, Diagnostic> {
        while let ExprKind::Lambda(parameters, body) = definition.body.kind {
            definition.parameters.extend(parameters);
            definition.body = *body;
        }
        if signature.parameters.len() < definition.parameters.len()
            || (definition.parameters.is_empty() && !signature.parameters.is_empty())
        {
            return Err(Diagnostic::new(
                "E1006",
                format!(
                    "signature expects {} parameters, definition has {}",
                    signature.parameters.len(),
                    definition.parameters.len()
                ),
                definition.name.span,
            ));
        }
        let mut types = signature.parameters;
        let remaining = types.split_off(definition.parameters.len());
        let result = if remaining.is_empty() {
            signature.result
        } else {
            let span = remaining[0].span.through(signature.result.span);
            TypeExpr {
                kind: TypeExprKind::Function(remaining, Box::new(signature.result)),
                span,
            }
        };
        Ok(FunctionDecl {
            name: definition.name,
            exported: signature.exported,
            parameters: definition
                .parameters
                .into_iter()
                .zip(types)
                .map(|((name, mutable), ty)| Parameter { name, mutable, ty })
                .collect(),
            result,
            constraints: signature.constraints,
            body: definition.body,
        })
    }

    fn parameters(&mut self, end: TokenKind) -> Result<Vec<Parameter>, Diagnostic> {
        let mut parameters = Vec::new();
        if !self.at(&end) {
            loop {
                let mutable = self.eat(&TokenKind::Mut);
                let name = self.ident()?;
                self.expect(&TokenKind::Colon, "':' and a type")?;
                let ty = self.type_expr()?;
                parameters.push(Parameter { name, ty, mutable });
                if !self.eat(&TokenKind::Comma) || self.at(&end) {
                    break;
                }
            }
        }
        self.expect(&end, "the closing delimiter")?;
        Ok(parameters)
    }

    fn type_expr(&mut self) -> Result<TypeExpr, Diagnostic> {
        let first = self.type_atom()?;
        if !self.eat(&TokenKind::Arrow) {
            return Ok(first);
        }
        let mut parameters = vec![first];
        let result = loop {
            let next = self.type_atom()?;
            if !self.eat(&TokenKind::Arrow) {
                break next;
            }
            if parameters.len() >= MAX_NESTING {
                return Err(self.error("too many function parameters"));
            }
            parameters.push(next);
        };
        let span = parameters[0].span.through(result.span);
        Ok(TypeExpr {
            kind: TypeExprKind::Function(parameters, Box::new(result)),
            span,
        })
    }

    fn type_atom(&mut self) -> Result<TypeExpr, Diagnostic> {
        self.enter()?;
        let start = self.current().span;
        let kind = if self.eat(&TokenKind::Ampersand) {
            let mutable = self.eat(&TokenKind::Mut);
            TypeExprKind::Reference(Box::new(self.type_atom()?), mutable)
        } else if self.at(&TokenKind::TypeVariable(String::new())) {
            TypeExprKind::Variable(self.type_variable()?.text)
        } else if self.eat(&TokenKind::LeftParen) {
            let ty = self.type_expr()?;
            self.expect(&TokenKind::RightParen, "')' after the type")?;
            ty.kind
        } else if self.eat(&TokenKind::Fn) {
            self.expect(&TokenKind::LeftParen, "'(' in a function type")?;
            let mut parameters = Vec::new();
            if !self.at(&TokenKind::RightParen) {
                loop {
                    parameters.push(self.type_expr()?);
                    if !self.eat(&TokenKind::Comma) || self.at(&TokenKind::RightParen) {
                        break;
                    }
                }
            }
            self.expect(&TokenKind::RightParen, "')'")?;
            self.expect(&TokenKind::Arrow, "'->' in a function type")?;
            TypeExprKind::Function(parameters, Box::new(self.type_expr()?))
        } else if self.at(&TokenKind::LeftBracket) || self.at(&TokenKind::LeftList) {
            let list = self.take().kind == TokenKind::LeftList;
            let element = self.type_expr()?;
            if self.at(&TokenKind::Semicolon) {
                return Err(self.error("array types use '[T]', without a length; use 'new [T](length, initializer)' to create an array"));
            }
            if list {
                self.expect(&TokenKind::RightList, "'|]'")?;
                TypeExprKind::List(Box::new(element))
            } else {
                self.expect(&TokenKind::RightBracket, "']'")?;
                TypeExprKind::Array(Box::new(element))
            }
        } else {
            let name = self.qualified_ident()?;
            if name.text == "Task" {
                TypeExprKind::Task(Box::new(self.type_atom()?))
            } else if self.at(&TokenKind::TypeVariable(String::new())) {
                let variable = self.type_variable()?;
                TypeExprKind::Constrained(
                    Box::new(name),
                    Box::new(TypeExpr {
                        kind: TypeExprKind::Variable(variable.text),
                        span: variable.span,
                    }),
                )
            } else {
                TypeExprKind::Named(name.text)
            }
        };
        let end = self.tokens[self.position - 1].span;
        self.nesting -= 1;
        Ok(TypeExpr {
            kind,
            span: start.through(end),
        })
    }

    fn make(&self, kind: ExprKind, span: Span, depth: usize) -> Result<Expr, Diagnostic> {
        if depth > MAX_NESTING {
            Err(Diagnostic::new(
                "E0002",
                format!("expression depth exceeds {MAX_NESTING}; introduce let bindings"),
                span,
            ))
        } else {
            Ok(Expr { kind, span, depth })
        }
    }

    fn block(&mut self) -> Result<Expr, Diagnostic> {
        self.enter()?;
        let start = self.expect(&TokenKind::LeftBrace, "'{'")?.span;
        let mut bindings = Vec::new();
        let mut depth = 0;
        let result = loop {
            if self.eat(&TokenKind::Let) {
                let binding = self.binding(self.in_task)?;
                depth = depth.max(binding.value.depth);
                bindings.push(binding);
            } else if self.at(&TokenKind::Return) {
                break self.task_return()?;
            } else if self.at(&TokenKind::Do) {
                let binding = self.task_do()?;
                depth = depth.max(binding.value.depth);
                bindings.push(binding);
            } else if self.at(&TokenKind::RightBrace) {
                break self.make(ExprKind::Unit, self.current().span, 1)?;
            } else {
                let value = self.expression(0, true)?;
                if !self.eat(&TokenKind::Semicolon) {
                    break value;
                }
                depth = depth.max(value.depth);
                bindings.push(Binding {
                    name: Ident {
                        text: "_".into(),
                        span: value.span,
                    },
                    mutable: false,
                    annotation: None,
                    value,
                });
            }
        };
        depth = depth.max(result.depth) + 1;
        let end = self.expect(&TokenKind::RightBrace, "'}' after the result expression")?;
        self.nesting -= 1;
        self.make(
            ExprKind::Block {
                bindings,
                result: Box::new(result),
            },
            start.through(end.span),
            depth,
        )
    }

    fn task(&mut self) -> Result<Expr, Diagnostic> {
        let start = self.take().span;
        let outer = self.in_task;
        self.in_task = true;
        let body = self.block()?;
        self.in_task = outer;
        let span = start.through(body.span);
        let depth = body.depth + 1;
        self.make(ExprKind::Task(Box::new(body)), span, depth)
    }

    fn task_return(&mut self) -> Result<Expr, Diagnostic> {
        if !self.in_task {
            return Err(self.error("'return' is only allowed at the end of a task block"));
        }
        let start = self.take().span;
        let run = self.eat(&TokenKind::Bang);
        let value = self.expression_inner(0, true, true)?;
        let value = if run {
            let span = start.through(value.span);
            let depth = value.depth + 1;
            self.make(ExprKind::TaskRun(Box::new(value)), span, depth)?
        } else {
            value
        };
        self.eat(&TokenKind::Semicolon);
        Ok(value)
    }

    fn task_do(&mut self) -> Result<Binding, Diagnostic> {
        if !self.in_task {
            return Err(self.error("'do!' is only allowed inside a task block"));
        }
        let start = self.take().span;
        self.expect(&TokenKind::Bang, "'!' after 'do'")?;
        let value = self.expression_inner(0, true, true)?;
        let span = start.through(value.span);
        let depth = value.depth + 1;
        let value = self.make(ExprKind::TaskRun(Box::new(value)), span, depth)?;
        self.binding_end(true)?;
        Ok(Binding {
            name: Ident {
                text: "_".into(),
                span,
            },
            mutable: false,
            annotation: Some(TypeExpr {
                kind: TypeExprKind::Named("unit".into()),
                span,
            }),
            value,
        })
    }

    fn binding(&mut self, top_level: bool) -> Result<Binding, Diagnostic> {
        let run = self.eat(&TokenKind::Bang);
        if run && !self.in_task {
            return Err(self.error("'let!' is only allowed inside a task block"));
        }
        let mutable = self.eat(&TokenKind::Mut);
        let name = self.ident()?;
        let annotation = if self.eat(&TokenKind::Colon) {
            Some(self.type_expr()?)
        } else {
            None
        };
        self.expect(&TokenKind::Equal, "'=' in the binding")?;
        let value = self.expression_inner(0, true, top_level)?;
        let value = if run {
            let span = value.span;
            let depth = value.depth + 1;
            self.make(ExprKind::TaskRun(Box::new(value)), span, depth)?
        } else {
            value
        };
        self.binding_end(top_level)?;
        Ok(Binding {
            name,
            mutable,
            annotation,
            value,
        })
    }

    fn binding_end(&mut self, newline_allowed: bool) -> Result<(), Diagnostic> {
        let newline = self.newline_before_current();
        if !self.eat(&TokenKind::Semicolon)
            && !(newline_allowed
                && (self.at(&TokenKind::End)
                    || newline
                    || (self.in_task && self.at(&TokenKind::RightBrace))))
        {
            return Err(self.error(if newline_allowed {
                "expected ';' or a newline after the binding"
            } else {
                "expected ';' after the let binding"
            }));
        }
        Ok(())
    }

    fn newline_before_current(&self) -> bool {
        self.source[self.tokens[self.position - 1].span.end..self.current().span.start]
            .contains(['\n', '\r'])
    }

    fn entry(&mut self) -> Result<Expr, Diagnostic> {
        self.enter()?;
        let start = self.current().span;
        let mut bindings = Vec::new();
        let mut depth = 0;
        while self.eat(&TokenKind::Let) {
            let binding = self.binding(true)?;
            depth = depth.max(binding.value.depth);
            bindings.push(binding);
        }
        let result = if self.at(&TokenKind::End) {
            self.make(ExprKind::Unit, self.current().span, 1)?
        } else {
            self.expression(0, true)?
        };
        depth = depth.max(result.depth) + 1;
        let end = self.expect(
            &TokenKind::End,
            "the end of the file; declarations must precede the entry-point code",
        )?;
        self.nesting -= 1;
        self.make(
            ExprKind::Block {
                bindings,
                result: Box::new(result),
            },
            start.through(end.span),
            depth,
        )
    }

    fn record(&mut self, name: Ident) -> Result<Expr, Diagnostic> {
        let start = name.span;
        self.expect(&TokenKind::LeftBrace, "'{' after the record name")?;
        let mut fields = Vec::new();
        if !self.at(&TokenKind::RightBrace) {
            loop {
                let field = self.ident()?;
                self.expect(&TokenKind::Colon, "':' before a field value")?;
                let value = self.expression(0, true)?;
                fields.push((field, value));
                if !self.eat(&TokenKind::Comma) || self.at(&TokenKind::RightBrace) {
                    break;
                }
            }
        }
        let end = self.expect(&TokenKind::RightBrace, "'}'")?;
        let depth = fields
            .iter()
            .map(|(_, value)| value.depth)
            .max()
            .unwrap_or(0)
            + 1;
        self.make(
            ExprKind::Record { name, fields },
            start.through(end.span),
            depth,
        )
    }

    fn expression(&mut self, minimum: u8, allow_record: bool) -> Result<Expr, Diagnostic> {
        self.expression_inner(minimum, allow_record, false)
    }

    fn expression_inner(
        &mut self,
        minimum: u8,
        allow_record: bool,
        stop_at_newline: bool,
    ) -> Result<Expr, Diagnostic> {
        self.enter()?;
        let mut left = self.primary(allow_record, stop_at_newline)?;
        loop {
            if stop_at_newline && self.newline_before_current() {
                break;
            }
            if minimum <= 14 && self.space_argument() {
                left = self.application(left, allow_record, stop_at_newline)?;
                continue;
            }
            if self.at(&TokenKind::Dot)
                || (matches!(
                    self.current().kind,
                    TokenKind::LeftParen | TokenKind::LeftBracket
                ) && self.tokens[self.position - 1].span.end == self.current().span.start)
            {
                left = self.postfix(left)?;
                continue;
            }
            if minimum <= 12 && self.eat(&TokenKind::As) {
                let ty = self.type_expr()?;
                let span = left.span.through(ty.span);
                let depth = left.depth + 1;
                left = self.make(ExprKind::Cast(Box::new(left), ty), span, depth)?;
                continue;
            }
            if minimum == 0 && self.eat(&TokenKind::Equal) {
                let right = self.expression_inner(0, allow_record, stop_at_newline)?;
                let span = left.span.through(right.span);
                let depth = left.depth.max(right.depth) + 1;
                left = self.make(
                    ExprKind::Assign(Box::new(left), Box::new(right)),
                    span,
                    depth,
                )?;
                continue;
            }
            let Some((operator, precedence)) = binary(&self.current().kind) else {
                break;
            };
            if precedence < minimum {
                break;
            }
            self.take();
            let right = self.expression_inner(precedence + 1, allow_record, stop_at_newline)?;
            let span = left.span.through(right.span);
            let depth = left.depth.max(right.depth) + 1;
            left = self.make(
                ExprKind::Binary(operator, Box::new(left), Box::new(right)),
                span,
                depth,
            )?;
        }
        self.nesting -= 1;
        Ok(left)
    }

    fn application(
        &mut self,
        left: Expr,
        allow_record: bool,
        stop_at_newline: bool,
    ) -> Result<Expr, Diagnostic> {
        let mut arguments = Vec::new();
        while self.space_argument() {
            if self.at(&TokenKind::LeftParen) {
                let start = self.take().span;
                if self.eat(&TokenKind::RightParen) {
                    arguments.push(self.make(
                        ExprKind::Unit,
                        start.through(self.tokens[self.position - 1].span),
                        1,
                    )?);
                } else {
                    let mut grouped = self.expressions(TokenKind::RightParen)?;
                    if grouped.len() == 1 {
                        let mut argument = grouped.pop().unwrap();
                        argument.span = start.through(self.tokens[self.position - 1].span);
                        while !(stop_at_newline && self.newline_before_current())
                            && (self.at(&TokenKind::Dot)
                                || (matches!(
                                    self.current().kind,
                                    TokenKind::LeftParen | TokenKind::LeftBracket
                                ) && self.tokens[self.position - 1].span.end
                                    == self.current().span.start))
                        {
                            argument = self.postfix(argument)?;
                        }
                        arguments.push(argument);
                    } else {
                        arguments.extend(grouped);
                    }
                }
            } else {
                arguments.push(self.expression_inner(15, allow_record, stop_at_newline)?);
            }
        }
        let span = left.span.through(self.tokens[self.position - 1].span);
        let depth = arguments
            .iter()
            .map(|argument| argument.depth)
            .max()
            .unwrap_or(0)
            .max(left.depth)
            + 1;
        self.make(ExprKind::Call(Box::new(left), arguments), span, depth)
    }

    fn space_argument(&self) -> bool {
        !self.newline_before_current()
            && self.tokens[self.position - 1].span.end < self.current().span.start
            && matches!(
                self.current().kind,
                TokenKind::Ident(_)
                    | TokenKind::Integer(_)
                    | TokenKind::Float(_)
                    | TokenKind::String(_)
                    | TokenKind::True
                    | TokenKind::False
                    | TokenKind::New
                    | TokenKind::Task
                    | TokenKind::LeftParen
                    | TokenKind::LeftBracket
                    | TokenKind::LeftList
            )
    }

    fn postfix(&mut self, left: Expr) -> Result<Expr, Diagnostic> {
        match self.take().kind {
            TokenKind::LeftParen => {
                let arguments = self.expressions(TokenKind::RightParen)?;
                let end = self.tokens[self.position - 1].span;
                let depth = arguments
                    .iter()
                    .map(|argument| argument.depth)
                    .max()
                    .unwrap_or(0)
                    .max(left.depth)
                    + 1;
                let span = left.span.through(end);
                self.make(ExprKind::Call(Box::new(left), arguments), span, depth)
            }
            TokenKind::Dot => {
                let field = self.ident()?;
                let span = left.span.through(field.span);
                let depth = left.depth + 1;
                self.make(ExprKind::Field(Box::new(left), field), span, depth)
            }
            TokenKind::LeftBracket => {
                let index = self.expression(0, true)?;
                let end = self.expect(&TokenKind::RightBracket, "']'")?;
                let span = left.span.through(end.span);
                let depth = left.depth.max(index.depth) + 1;
                self.make(
                    ExprKind::Index(Box::new(left), Box::new(index)),
                    span,
                    depth,
                )
            }
            _ => unreachable!("postfix operators are checked by expression"),
        }
    }

    fn expressions(&mut self, end: TokenKind) -> Result<Vec<Expr>, Diagnostic> {
        let mut values = Vec::new();
        if !self.at(&end) {
            loop {
                values.push(self.expression(0, true)?);
                if !self.eat(&TokenKind::Comma) || self.at(&end) {
                    break;
                }
            }
        }
        self.expect(&end, "the closing delimiter")?;
        Ok(values)
    }

    fn new_collection(&mut self) -> Result<Expr, Diagnostic> {
        let start = self.take().span;
        let list = self.at(&TokenKind::LeftList);
        if !list && !self.at(&TokenKind::LeftBracket) {
            return Err(self.error("expected an array type '[T]' or list type '[|T|]' after 'new'"));
        }
        let ty = self.type_atom()?;
        self.expect(&TokenKind::LeftParen, "'(' before the collection length")?;
        let length = self.expression(0, true)?;
        self.expect(&TokenKind::Comma, "',' before the collection initializer")?;
        let initializer = self.expression(0, true)?;
        self.eat(&TokenKind::Comma);
        let end = self.expect(
            &TokenKind::RightParen,
            "')' after the collection initializer",
        )?;
        let depth = length.depth.max(initializer.depth) + 1;
        let kind = if list {
            ExprKind::NewList(Box::new(ty), Box::new(length), Box::new(initializer))
        } else {
            ExprKind::NewArray(Box::new(ty), Box::new(length), Box::new(initializer))
        };
        self.make(kind, start.through(end.span), depth)
    }

    fn collection_literal(&mut self) -> Result<Expr, Diagnostic> {
        let start = self.take();
        let list = start.kind == TokenKind::LeftList;
        let values = self.expressions(if list {
            TokenKind::RightList
        } else {
            TokenKind::RightBracket
        })?;
        let end = self.tokens[self.position - 1].span;
        let depth = values.iter().map(|value| value.depth).max().unwrap_or(0) + 1;
        let kind = if list {
            ExprKind::List(values)
        } else {
            ExprKind::Array(values)
        };
        self.make(kind, start.span.through(end), depth)
    }

    fn primary(&mut self, allow_record: bool, stop_at_newline: bool) -> Result<Expr, Diagnostic> {
        let start = self.current().span;
        let kind = match &self.current().kind {
            TokenKind::Task => return self.task(),
            TokenKind::New => return self.new_collection(),
            TokenKind::Integer(_) | TokenKind::Float(_) | TokenKind::String(_) => {
                return self.literal();
            }
            TokenKind::Ampersand
            | TokenKind::Star
            | TokenKind::Minus
            | TokenKind::Bang
            | TokenKind::Tilde => return self.prefix(allow_record, stop_at_newline),
            TokenKind::True | TokenKind::False => {
                ExprKind::Bool(self.take().kind == TokenKind::True)
            }
            TokenKind::Ident(_) => {
                let mut name = self.ident()?;
                if self.eat(&TokenKind::Arrow) {
                    return self.lambda(name, stop_at_newline);
                }
                if allow_record
                    && self.at(&TokenKind::Dot)
                    && (!stop_at_newline || !self.newline_before_current())
                    && self.tokens.get(self.position + 2).is_some_and(|token| {
                        token.kind == TokenKind::LeftBrace
                            && (!stop_at_newline
                                || !self.source
                                    [self.tokens[self.position + 1].span.end..token.span.start]
                                    .contains(['\n', '\r']))
                    })
                {
                    self.take();
                    let record = self.ident()?;
                    name.text.push('.');
                    name.text.push_str(&record.text);
                    name.span = name.span.through(record.span);
                }
                if allow_record
                    && self.at(&TokenKind::LeftBrace)
                    && (!stop_at_newline || !self.newline_before_current())
                {
                    return self.record(name);
                }
                ExprKind::Name(name)
            }
            TokenKind::LeftParen => {
                self.take();
                if self.eat(&TokenKind::RightParen) {
                    ExprKind::Unit
                } else {
                    let mut value = self.expression(0, true)?;
                    let end = self.expect(&TokenKind::RightParen, "')'")?;
                    value.span = start.through(end.span);
                    return Ok(value);
                }
            }
            TokenKind::LeftBracket | TokenKind::LeftList => return self.collection_literal(),
            TokenKind::LeftBrace => return self.block(),
            TokenKind::If => return self.conditional(),
            _ => return Err(self.error("expected an expression")),
        };
        let end = self.tokens[self.position - 1].span;
        self.make(kind, start.through(end), 1)
    }

    fn lambda(&mut self, name: Ident, stop_at_newline: bool) -> Result<Expr, Diagnostic> {
        let outer = self.in_task;
        self.in_task = false;
        let body = self.expression_inner(0, true, stop_at_newline)?;
        self.in_task = outer;
        let span = name.span.through(body.span);
        let depth = body.depth + 1;
        let (mut parameters, body) = match body {
            Expr {
                kind: ExprKind::Lambda(parameters, body),
                ..
            } => (parameters, body),
            body => (Vec::new(), Box::new(body)),
        };
        parameters.insert(0, (name, false));
        self.make(ExprKind::Lambda(parameters, body), span, depth)
    }

    fn literal(&mut self) -> Result<Expr, Diagnostic> {
        let token = self.take();
        let kind = match &token.kind {
            TokenKind::Integer(text) => {
                let suffix = crate::numeric::literal_parts(text).1.map(str::to_owned);
                ExprKind::Integer(integer(&token)?, suffix)
            }
            TokenKind::Float(text) => {
                let (value, suffix) = crate::numeric::literal_parts(text);
                ExprKind::Float(value.to_owned(), suffix.map(str::to_owned))
            }
            TokenKind::String(text) => ExprKind::String(text.clone()),
            _ => unreachable!("literal token checked"),
        };
        self.make(kind, token.span, 1)
    }

    fn prefix(&mut self, allow_record: bool, stop_at_newline: bool) -> Result<Expr, Diagnostic> {
        let token = self.take();
        let mutable = token.kind == TokenKind::Ampersand && self.eat(&TokenKind::Mut);
        let value = self.expression_inner(13, allow_record, stop_at_newline)?;
        let span = token.span.through(value.span);
        let depth = value.depth + 1;
        let value = Box::new(value);
        let kind = match token.kind {
            TokenKind::Ampersand => ExprKind::Borrow(value, mutable),
            TokenKind::Star => ExprKind::Dereference(value),
            TokenKind::Minus => ExprKind::Unary(UnaryOp::Negate, value),
            TokenKind::Bang => ExprKind::Unary(UnaryOp::Not, value),
            TokenKind::Tilde => ExprKind::Unary(UnaryOp::BitNot, value),
            _ => unreachable!("prefix token checked"),
        };
        self.make(kind, span, depth)
    }

    fn conditional(&mut self) -> Result<Expr, Diagnostic> {
        let start = self.take().span;
        let condition = self.expression(0, false)?;
        let then_branch = self.block()?;
        self.expect(
            &TokenKind::Else,
            "'else'; every if expression must return a value",
        )?;
        let else_branch = if self.at(&TokenKind::If) {
            self.enter()?;
            let branch = self.conditional()?;
            self.nesting -= 1;
            branch
        } else {
            self.block()?
        };
        let span = start.through(else_branch.span);
        let depth = condition
            .depth
            .max(then_branch.depth)
            .max(else_branch.depth)
            + 1;
        self.make(
            ExprKind::If {
                condition: Box::new(condition),
                then_branch: Box::new(then_branch),
                else_branch: Box::new(else_branch),
            },
            span,
            depth,
        )
    }
}

fn integer(token: &Token) -> Result<u128, Diagnostic> {
    let TokenKind::Integer(text) = &token.kind else {
        unreachable!("integer() is only called for integer tokens")
    };
    let (text, _) = crate::numeric::literal_parts(text);
    let result = if let Some(digits) = text.strip_prefix("0x") {
        u128::from_str_radix(digits, 16)
    } else if let Some(digits) = text.strip_prefix("0b") {
        u128::from_str_radix(digits, 2)
    } else {
        text.parse()
    };
    result.map_err(|_| Diagnostic::new("E0002", "integer literal is too large", token.span))
}

fn binary(token: &TokenKind) -> Option<(BinaryOp, u8)> {
    use BinaryOp as B;
    use TokenKind as T;
    Some(match token {
        T::PipeForward => (B::Pipe, 1),
        T::OrOr => (B::Or, 2),
        T::AndAnd => (B::And, 3),
        T::Pipe => (B::BitOr, 4),
        T::Caret => (B::BitXor, 5),
        T::Ampersand => (B::BitAnd, 6),
        T::EqualEqual => (B::Equal, 7),
        T::BangEqual => (B::NotEqual, 7),
        T::Less => (B::Less, 8),
        T::LessEqual => (B::LessEqual, 8),
        T::Greater => (B::Greater, 8),
        T::GreaterEqual => (B::GreaterEqual, 8),
        T::ShiftLeft => (B::ShiftLeft, 9),
        T::ShiftRight => (B::ShiftRight, 9),
        T::ShiftRightUnsigned => (B::ShiftRightUnsigned, 9),
        T::Plus => (B::Add, 10),
        T::Minus => (B::Subtract, 10),
        T::Star => (B::Multiply, 11),
        T::Slash => (B::Divide, 11),
        T::Percent => (B::Remainder, 11),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_application_syntax() {
        let source = "
            record Pair { x: i64, y: f64, }
            fn choose(flag: bool, f: fn(i64) -> i64) -> i64 {
                let xs: [i64] = [10, 20];
                if flag { xs[0] |> f } else { Pair { x: 3, y: 2.0 }.x }
            }";
        let program = parse(source).unwrap();
        assert_eq!(program.records.len(), 1);
        assert_eq!(program.functions[0].parameters.len(), 2);
    }

    #[test]
    fn rejects_missing_else_and_parameter_types() {
        for source in ["fn f() -> i64 { if true { 1 } }", "fn f(x) -> i64 { x }"] {
            assert!(parse(source).is_err(), "{source}");
        }
    }

    #[test]
    fn bounds_recursive_and_flat_expression_depth() {
        let nested = format!(
            "fn f() -> i64 {{ {}1{} }}",
            "(".repeat(200),
            ")".repeat(200)
        );
        assert!(parse(&nested).is_err());
        let flat = format!("fn f() -> i64 {{ {}1 }}", "1 + ".repeat(200));
        assert!(parse(&flat).is_err());
        let chain = format!(
            "fn f() -> i64 {{ {} {{ 1 }} }}",
            "if true { 0 } else ".repeat(200)
        );
        assert!(parse(&chain).is_err());
        for (prefix, suffix) in [
            ("Module.f(", ")"),
            ("values[", "]"),
            ("R { x: ", "}"),
            ("Module.R { x: ", "}"),
            ("[", "]"),
            ("[|", "|]"),
            ("new [i64](1, i -> ", ")"),
            ("new [|i64|](1, i -> ", ")"),
            ("- ", ""),
        ] {
            let source = format!(
                "fn f() -> i64 {{ {}0{} }}",
                prefix.repeat(200),
                suffix.repeat(200)
            );
            assert!(parse(&source).is_err(), "{prefix}");
        }
    }

    #[test]
    fn honors_exact_expression_and_parenthesis_depth_limits() {
        for extra in [0, 1] {
            let depth = MAX_NESTING - 2 + extra;
            let flat = format!("fn f() -> i64 {{ {}1 }}", "1 + ".repeat(depth));
            let nested = format!(
                "fn f() -> i64 {{ {}1{} }}",
                "(".repeat(depth),
                ")".repeat(depth)
            );
            assert_eq!(parse(&flat).is_ok(), extra == 0, "flat depth {depth}");
            assert_eq!(parse(&nested).is_ok(), extra == 0, "nested depth {depth}");
        }
    }
}
