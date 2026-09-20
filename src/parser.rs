use crate::diagnostic::{Diagnostic, Span};
use crate::lexer::lex;
use crate::syntax::*;

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
    }
    .program()
}

struct Parser<'a> {
    source: &'a str,
    tokens: Vec<Token>,
    position: usize,
    nesting: usize,
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
            entry: None,
        };
        while !self.at(&TokenKind::End) {
            if self.eat(&TokenKind::Record) {
                let name = self.ident()?;
                self.expect(&TokenKind::LeftBrace, "'{' after the record name")?;
                let fields = self.parameters(TokenKind::RightBrace)?;
                program.records.push(RecordDecl { name, fields });
            } else if self.at(&TokenKind::Fn) || self.at(&TokenKind::Export) {
                let exported = self.eat(&TokenKind::Export);
                self.expect(&TokenKind::Fn, "'fn', 'export fn', or 'record'")?;
                let name = self.ident()?;
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
        Ok(program)
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
        self.enter()?;
        let start = self.current().span;
        let kind = if self.eat(&TokenKind::Ampersand) {
            let mutable = self.eat(&TokenKind::Mut);
            TypeExprKind::Reference(Box::new(self.type_expr()?), mutable)
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
        } else if self.eat(&TokenKind::LeftBracket) {
            let element = self.type_expr()?;
            self.expect(&TokenKind::Semicolon, "';' and a fixed array length")?;
            let token = self.expect(&TokenKind::Integer(String::new()), "an array length")?;
            if matches!(&token.kind, TokenKind::Integer(text) if crate::numeric::literal_parts(text).1.is_some())
            {
                return Err(Diagnostic::new(
                    "E0002",
                    "array lengths do not have type suffixes",
                    token.span,
                ));
            }
            let length = integer(&token)?;
            let length = usize::try_from(length)
                .map_err(|_| Diagnostic::new("E0002", "array length is too large", token.span))?;
            self.expect(&TokenKind::RightBracket, "']'")?;
            TypeExprKind::Array(Box::new(element), length)
        } else {
            let mut name = self.ident()?.text;
            if self.eat(&TokenKind::Dot) {
                name.push('.');
                name.push_str(&self.ident()?.text);
            }
            TypeExprKind::Named(name)
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
                let binding = self.binding(false)?;
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

    fn binding(&mut self, top_level: bool) -> Result<Binding, Diagnostic> {
        let mutable = self.eat(&TokenKind::Mut);
        let name = self.ident()?;
        let annotation = if self.eat(&TokenKind::Colon) {
            Some(self.type_expr()?)
        } else {
            None
        };
        self.expect(&TokenKind::Equal, "'=' in the binding")?;
        let value = self.expression_inner(0, true, top_level)?;
        let newline = self.newline_before_current();
        if !self.eat(&TokenKind::Semicolon) && !(top_level && (self.at(&TokenKind::End) || newline))
        {
            return Err(self.error(if top_level {
                "expected ';' or a newline after the top-level let binding"
            } else {
                "expected ';' after the let binding"
            }));
        }
        Ok(Binding {
            name,
            mutable,
            annotation,
            value,
        })
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
            if matches!(
                self.current().kind,
                TokenKind::LeftParen | TokenKind::Dot | TokenKind::LeftBracket
            ) {
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

    fn primary(&mut self, allow_record: bool, stop_at_newline: bool) -> Result<Expr, Diagnostic> {
        let start = self.current().span;
        let kind = match &self.current().kind {
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
            TokenKind::LeftBracket => {
                self.take();
                let values = self.expressions(TokenKind::RightBracket)?;
                let end = self.tokens[self.position - 1].span;
                let depth = values.iter().map(|value| value.depth).max().unwrap_or(0) + 1;
                return self.make(ExprKind::Array(values), start.through(end), depth);
            }
            TokenKind::LeftBrace => return self.block(),
            TokenKind::If => return self.conditional(),
            _ => return Err(self.error("expected an expression")),
        };
        let end = self.tokens[self.position - 1].span;
        self.make(kind, start.through(end), 1)
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
                let xs: [i64; 2] = [10, 20];
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
