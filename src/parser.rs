use crate::diagnostic::{Diagnostic, MAX_UNIQUE_DIAGNOSTICS, Span};
use crate::lexer::{lex, lex_all};
use crate::syntax::*;
use std::collections::{BTreeMap, BTreeSet};

#[path = "parse_control.rs"]
mod control;

fn duplicate_active_case(case: &Ident) -> Diagnostic {
    Diagnostic::new(
        "E1020",
        format!("duplicate active pattern case '{}'", case.text),
        case.span,
    )
}

fn is_top_level_declaration_start(kind: &TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::DocComment(_)
            | TokenKind::Def
            | TokenKind::Fn
            | TokenKind::Record
            | TokenKind::Union
            | TokenKind::Type
            | TokenKind::Const
            | TokenKind::Test
            | TokenKind::Class
            | TokenKind::Instance
            | TokenKind::Export
            | TokenKind::Extern
            | TokenKind::Private
            | TokenKind::Let
    )
}

pub fn parse(source: &str) -> Result<Program, Diagnostic> {
    parse_source(source, None)
}

pub fn parse_with_source(source: &str, source_id: usize) -> Result<Program, Diagnostic> {
    parse_source(source, Some(source_id))
}

pub fn parse_with_source_all(source: &str, source_id: usize) -> Result<Program, Vec<Diagnostic>> {
    let (mut tokens, mut diagnostics) = lex_all(source);
    for error in &mut diagnostics {
        error.span.source = Some(source_id);
    }
    if !diagnostics.is_empty() {
        return Err(diagnostics);
    }
    for token in &mut tokens {
        token.span.source = Some(source_id);
    }
    Parser::new(source, tokens).program_all(true)
}

fn parse_source(source: &str, source_id: Option<usize>) -> Result<Program, Diagnostic> {
    let mut tokens = lex(source).map_err(|mut error| {
        error.span.source = source_id;
        error
    })?;
    for token in &mut tokens {
        token.span.source = source_id;
    }
    Parser::new(source, tokens).program()
}

struct Parser<'a> {
    source: &'a str,
    tokens: Vec<Token>,
    position: usize,
    previous_end: usize,
    nesting: usize,
    in_task: bool,
    stop_at_arm: bool,
    stop_at_arrow: bool,
    active_patterns: BTreeMap<String, ActivePattern>,
    pattern_type_arrow: bool,
    type_offside: Option<usize>,
    slice_context: bool,
    stop_at_slice_dotdot: bool,
    region_suffix: bool,
}

impl<'a> Parser<'a> {
    fn new(source: &'a str, tokens: Vec<Token>) -> Self {
        Self {
            source,
            tokens,
            position: 0,
            previous_end: 0,
            nesting: 0,
            in_task: false,
            stop_at_arm: false,
            stop_at_arrow: false,
            active_patterns: BTreeMap::new(),
            pattern_type_arrow: false,
            type_offside: None,
            slice_context: false,
            stop_at_slice_dotdot: false,
            region_suffix: true,
        }
    }
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
        self.previous_end = token.span.end;
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
                provenance: Provenance::User,
            };
            self.take();
            Ok(result)
        } else {
            Err(self.error("expected an identifier"))
        }
    }

    fn program(self) -> Result<Program, Diagnostic> {
        self.program_all(false).map_err(|errors| {
            errors
                .into_iter()
                .next()
                .expect("a failed parse has a diagnostic")
        })
    }

    fn take_doc(&mut self) -> Option<Documentation> {
        let mut documentation: Option<Documentation> = None;
        while let TokenKind::DocComment(_) = &self.current().kind {
            let token = self.take();
            let TokenKind::DocComment(text) = token.kind else {
                unreachable!()
            };
            if let Some(doc) = &mut documentation {
                doc.text.push('\n');
                doc.text.push_str(&text);
                doc.span = doc.span.through(token.span);
            } else {
                documentation = Some(Documentation {
                    text,
                    span: token.span,
                });
            }
        }
        documentation
    }

    fn doc_target(&self) -> bool {
        match &self.current().kind {
            TokenKind::Def
            | TokenKind::Extern
            | TokenKind::Record
            | TokenKind::Union
            | TokenKind::Type
            | TokenKind::Const
            | TokenKind::Class => true,
            TokenKind::Export => self
                .tokens
                .get(self.position + 1)
                .is_some_and(|token| token.kind == TokenKind::Def),
            _ => false,
        }
    }

    fn program_all(mut self, recovering: bool) -> Result<Program, Vec<Diagnostic>> {
        let mut program = Program {
            source_kind: None,
            type_aliases: Vec::new(),
            constants: Vec::new(),
            externs: Vec::new(),
            records: Vec::new(),
            unions: Vec::new(),
            functions: Vec::new(),
            classes: Vec::new(),
            instances: Vec::new(),
            active_patterns: Vec::new(),
            tests: Vec::new(),
            entry: None,
        };
        let mut signatures = BTreeMap::new();
        let mut definitions = Vec::new();
        let mut defined_names = BTreeSet::new();
        let mut signature_group = None;
        let mut definition_group = None;
        let mut diagnostics = Vec::new();
        while !self.at(&TokenKind::End) && diagnostics.len() < MAX_UNIQUE_DIAGNOSTICS {
            let start = self.position;
            let parsed = (|| -> Result<(), Diagnostic> {
                let doc = self.take_doc();
                let column = self.column(self.current().span);
                let visibility = self.visibility()?;
                if let Some(documentation) = &doc {
                    if !self.doc_target() {
                        return Err(Diagnostic::new(
                            "E0002",
                            "doc comments attach to API declarations such as 'def', not to implementations or expressions",
                            documentation.span,
                        ));
                    }
                }
                if self.eat(&TokenKind::Extern) {
                if self.eat(&TokenKind::Export) { return Err(Diagnostic::new("E1008", "extern declarations cannot be exported", self.current().span)); }
                self.expect(&TokenKind::Def, "'def' after extern")?;
                let name = self.ident()?;
                if defined_names.contains(&name.text) || signatures.contains_key(&name.text) { return Err(Diagnostic::new("E1001", "duplicate extern or function declaration", name.span)); }
                self.expect(&TokenKind::DoubleColon, "'::' after the extern name")?;
                let mut signature = self.signature(name.clone(), false, column)?;
                signature.doc = doc;
                signature.visibility = visibility;
                defined_names.insert(name.text);
                program.externs.push(signature);
                self.eat(&TokenKind::Semicolon);
            } else if self.eat(&TokenKind::Type) {
                let mut declaration = self.type_alias(visibility, column)?;
                declaration.doc = doc;
                program.type_aliases.push(declaration);
            } else if self.eat(&TokenKind::Const) {
                let mut declaration = self.const_declaration(visibility)?;
                declaration.doc = doc;
                program.constants.push(declaration);
            } else if self.at(&TokenKind::Test) {
                program.tests.push(self.test_declaration()?);
            } else if self.eat(&TokenKind::Union) {
                let mut declaration = self.union_declaration(visibility, column)?;
                declaration.doc = doc;
                program.unions.push(declaration);
            } else if self.eat(&TokenKind::Record) {
                let name = self.ident()?;
                let parameters = self.type_parameters()?;
                let regions = if self.region_list_ahead() { self.region_list()? } else { Vec::new() };
                self.expect(&TokenKind::LeftBrace, "'{' after the record name")?;
                let fields = self.parameters(TokenKind::RightBrace)?;
                let derives = self.derives()?;
                program.records.push(RecordDecl {
                    doc,
                    visibility,
                    name,
                    parameters,
                    regions,
                    fields,
                    derives,
                });
            } else if self.eat(&TokenKind::Class) {
                let superclasses = self.constraints()?;
                let name = self.ident()?;
                let mut parameters = self.angle_list(Self::class_parameter)?;
                if parameters.len() != 1 {
                    return Err(self.error("a type class requires exactly one type parameter, as in class C<'a>"));
                }
                let (variable, kind) = parameters.remove(0);
                self.expect(&TokenKind::LeftBrace, "'{' after the class parameter")?;
                let mut methods = Vec::new();
                let mut defaults = Vec::new();
                while !self.eat(&TokenKind::RightBrace) {
                    let method_doc = self.take_doc();
                    let column = self.column(self.current().span);
                    if self.eat(&TokenKind::Def) {
                        let method = self.ident()?;
                        self.expect(&TokenKind::DoubleColon, "'::' before the method type")?;
                        let mut signature = self.signature(method, false, column)?;
                        signature.doc = method_doc;
                        methods.push(signature);
                    } else {
                        if let Some(documentation) = method_doc {
                            return Err(Diagnostic::new("E0002", "doc comments attach to a class method's 'def', not its implementation", documentation.span));
                        }
                        defaults.push(self.instance_method()?);
                    }
                    self.eat(&TokenKind::Semicolon);
                }
                program.classes.push(ClassDecl {
                    doc,
                    name,
                    variable,
                    kind,
                    superclasses,
                    methods,
                    defaults,
                });
            } else if self.eat(&TokenKind::Instance) {
                let constraints = self.constraints()?;
                let class = self.qualified_ident()?;
                let ty = self.single_type_argument()?;
                self.expect(&TokenKind::LeftBrace, "'{' after the instance type")?;
                let mut methods = Vec::new();
                while !self.eat(&TokenKind::RightBrace) {
                    methods.push(self.instance_method()?);
                    self.eat(&TokenKind::Semicolon);
                }
                program.instances.push(InstanceDecl { class, ty, constraints, methods });
            } else if self.at(&TokenKind::Let)
                && self.tokens.get(self.position + if self.tokens.get(self.position + 1).is_some_and(|token| token.kind == TokenKind::Rec) { 2 } else { 1 }).is_some_and(|token|
                    matches!(&token.kind, TokenKind::Ident(name) if signatures.contains_key(name) && !defined_names.contains(name)))
            {
                self.take();
                let recursive = self.eat(&TokenKind::Rec);
                let name = self.ident()?;
                if defined_names.contains(&name.text) {
                    return Err(Diagnostic::new("E1001", "duplicate function definition", name.span));
                }
                self.expect(&TokenKind::Equal, "'=' before the anonymous function")?;
                let body = self.body_expression()?;
                if !matches!(body.kind, ExprKind::Lambda(..)) {
                    return Err(Diagnostic::new("E0002", "a declared top-level 'let' function needs a lambda such as 'x -> y -> x + y'", body.span));
                }
                let recursion = recursive.then(|| name.text.clone());
                definition_group = recursion.clone();
                defined_names.insert(name.text.clone());
                definitions.push(Definition { name, recursion, parameters: Vec::new(), body });
                self.eat(&TokenKind::Semicolon);
            } else if self.at(&TokenKind::Fn) || self.at(&TokenKind::Def) || self.at(&TokenKind::Export) || self.at(&TokenKind::And) {
                let export = self.current().span;
                let exported = self.eat(&TokenKind::Export);
                if exported && self.at(&TokenKind::Extern) { return Err(Diagnostic::new("E1008", "extern declarations cannot be exported", export)); }
                if exported && self.at(&TokenKind::Private) {
                    return Err(Self::private_export(export.through(self.current().span)));
                }
                let declaration = self.eat(&TokenKind::Def);
                let continuation = self.eat(&TokenKind::And);
                if !declaration && !continuation {
                    self.expect(&TokenKind::Fn, "'def', 'fn', 'record', or 'union'")?;
                }
                let recursive = !continuation && self.eat(&TokenKind::Rec);
                let name = self.function_name()?;
                let group = if declaration { &mut signature_group } else { &mut definition_group };
                let recursion = if continuation {
                    Some(group.clone().ok_or_else(|| Diagnostic::new(
                        "E1019", "'and' requires a preceding recursive declaration or definition", name.span,
                    ))?)
                } else {
                    recursive.then(|| name.text.clone())
                };
                *group = recursion.clone();
                if declaration {
                    let regions = if self.at(&TokenKind::LeftBrace) { self.region_list()? } else { Vec::new() };
                    self.expect(&TokenKind::DoubleColon, "'::' after the declaration name")?;
                    let mut signature = self.signature(name.clone(), exported, column)?;
                    signature.doc = doc;
                    signature.regions = regions;
                    signature.recursion = recursion;
                    signature.visibility = visibility;
                    if signatures.contains_key(&name.text) || program.externs.iter().any(|external| external.name.text == name.text) {
                        return Err(Diagnostic::new(
                            "E1001",
                            "duplicate function signature",
                            name.span,
                        ));
                    }
                    if self.at(&TokenKind::DoubleColon) {
                        return Err(self.error("use 'def name :: ...' for a signature; 'fn' introduces its implementation"));
                    }
                    signatures.insert(name.text.clone(), signature);
                    self.eat(&TokenKind::Semicolon);
                    return Ok(());
                }
                if defined_names.contains(&name.text) {
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
                    let mut definition = self.definition(name)?;
                    definition.recursion = recursion;
                    defined_names.insert(definition.name.text.clone());
                    definitions.push(definition);
                    self.eat(&TokenKind::Semicolon);
                    return Ok(());
                }
                self.expect(&TokenKind::LeftParen, "'(' after the function name")?;
                let parameters = self.parameters(TokenKind::RightParen)?;
                self.expect(&TokenKind::Arrow, "'->' and an explicit return type")?;
                let outer = std::mem::replace(&mut self.region_suffix, false);
                let result = self.type_expr()?;
                self.region_suffix = outer;
                let body = self.block()?;
                defined_names.insert(name.text.clone());
                program.functions.push(FunctionDecl {
                    doc: None,
                    name,
                    regions: Vec::new(),
                    recursion,
                    visibility: Visibility::Public,
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
                        "module and namespace declarations are not supported; each .tz, .tt, or .tc file is one module named after its filename",
                    ));
                }
                program.entry = Some(self.entry()?);
            }
                Ok(())
            })();
            if let Err(error) = parsed {
                let resume = error.span.start.min(self.current().span.start);
                diagnostics.push(error);
                if !recovering {
                    return Err(diagnostics);
                }
                self.recover_top_level_boundary(start, resume);
                signature_group = None;
                definition_group = None;
            } else if program.entry.is_some() {
                break;
            }
        }
        if !diagnostics.is_empty() {
            return Err(diagnostics);
        }
        for definition in definitions {
            if diagnostics.len() == MAX_UNIQUE_DIAGNOSTICS {
                break;
            }
            if definition.name.text == "main"
                && definition.parameters.is_empty()
                && definition.recursion.is_none()
                && !signatures.contains_key("main")
            {
                if program.entry.is_some() {
                    diagnostics.push(Diagnostic::new(
                        "E2004",
                        "use either top-level entry-point code or 'fn main', not both",
                        definition.name.span,
                    ));
                } else {
                    program.entry = Some(definition.body);
                }
                continue;
            }
            let defined = signatures
                .remove(&definition.name.text)
                .ok_or_else(|| {
                    Diagnostic::new(
                        "E0002",
                        "function definition needs a 'def name :: ...' signature",
                        definition.name.span,
                    )
                })
                .and_then(|signature| Self::define(signature, definition));
            match defined {
                Ok(function) => program.functions.push(function),
                Err(error) => {
                    diagnostics.push(error);
                    if !recovering {
                        return Err(diagnostics);
                    }
                }
            }
        }
        for signature in signatures.values() {
            if diagnostics.len() == MAX_UNIQUE_DIAGNOSTICS {
                break;
            }
            diagnostics.push(Diagnostic::new(
                "E0002",
                "function signature has no definition",
                signature.name.span,
            ));
            if !recovering {
                break;
            }
        }
        if !diagnostics.is_empty() {
            return Err(diagnostics);
        }
        program.active_patterns = self.active_patterns.into_values().collect();
        Ok(program)
    }

    fn recover_top_level_boundary(&mut self, start: usize, resume: usize) {
        let mut delimiters = Vec::new();
        self.position = self.tokens.len() - 1;
        for index in start..self.tokens.len() {
            let token = &self.tokens[index];
            if index > start
                && token.span.start >= resume
                && delimiters.is_empty()
                && self.column(token.span) == 0
                && is_top_level_declaration_start(&token.kind)
            {
                self.position = index;
                break;
            }
            match token.kind {
                TokenKind::LeftParen => delimiters.push(TokenKind::RightParen),
                TokenKind::LeftBracket => delimiters.push(TokenKind::RightBracket),
                TokenKind::LeftList => delimiters.push(TokenKind::RightList),
                TokenKind::LeftBrace => delimiters.push(TokenKind::RightBrace),
                TokenKind::RightParen
                | TokenKind::RightBracket
                | TokenKind::RightList
                | TokenKind::RightBrace
                    if delimiters.last() == Some(&token.kind) =>
                {
                    delimiters.pop();
                }
                _ => {}
            }
        }
        self.previous_end = self.tokens[self.position.saturating_sub(1)].span.end;
        self.nesting = 0;
        self.in_task = false;
        self.stop_at_arm = false;
        self.stop_at_arrow = false;
        self.pattern_type_arrow = false;
        self.type_offside = None;
        self.slice_context = false;
        self.stop_at_slice_dotdot = false;
        self.region_suffix = true;
    }

    fn visibility(&mut self) -> Result<Visibility, Diagnostic> {
        let private = self.current().span;
        if !self.eat(&TokenKind::Private) {
            return Ok(Visibility::Public);
        }
        let next = self.current();
        let message = match next.kind {
            TokenKind::Record
            | TokenKind::Union
            | TokenKind::Type
            | TokenKind::Const
            | TokenKind::Def
            | TokenKind::Extern => {
                return Ok(Visibility::Private);
            }
            TokenKind::Export => return Err(Self::private_export(private.through(next.span))),
            TokenKind::Fn | TokenKind::And | TokenKind::Let => {
                "put 'private' on the 'def' signature; its 'fn', 'and', or 'let' implementation inherits that visibility"
            }
            TokenKind::Class => "type classes are always public; remove 'private'",
            TokenKind::Instance => {
                "instances take part in global coherence and are always public; remove 'private'"
            }
            _ => "'private' must be followed by 'def', 'record', 'union', 'type', or 'const'",
        };
        Err(Diagnostic::new("E1022", message, private))
    }

    fn const_declaration(&mut self, visibility: Visibility) -> Result<ConstDecl, Diagnostic> {
        let name = self.ident()?;
        self.expect(&TokenKind::Colon, "':' and an explicit constant type")?;
        let ty = self.type_expr()?;
        self.expect(&TokenKind::Equal, "'=' before the constant value")?;
        let value = self.body_expression()?;
        self.eat(&TokenKind::Semicolon);
        Ok(ConstDecl {
            doc: None,
            visibility,
            name,
            ty,
            value,
        })
    }

    fn type_alias(
        &mut self,
        visibility: Visibility,
        column: usize,
    ) -> Result<TypeAliasDecl, Diagnostic> {
        let name = self.ident()?;
        let parameters = self.type_parameters()?;
        self.expect(&TokenKind::Equal, "'=' after the type alias name")?;
        let outer = self.type_offside.replace(column);
        let target = self.type_expr()?;
        self.type_offside = outer;
        self.eat(&TokenKind::Semicolon);
        Ok(TypeAliasDecl {
            doc: None,
            visibility,
            name,
            parameters,
            target,
        })
    }

    /// Parses the cases after `union Name<'a, ...> =`. `of` is contextual, and a
    /// payload type ends before a new line that is not indented past the
    /// declaration, so the entry-point code may follow the last case.
    fn union_declaration(
        &mut self,
        visibility: Visibility,
        column: usize,
    ) -> Result<UnionDecl, Diagnostic> {
        let name = self.ident()?;
        let parameters = self.type_parameters()?;
        self.expect(&TokenKind::Equal, "'=' after the union name")?;
        self.eat(&TokenKind::Pipe);
        let outer = self.type_offside.replace(column);
        let mut cases = Vec::new();
        loop {
            if !matches!(self.current().kind, TokenKind::Ident(_)) {
                return Err(self.error("expected a union case name"));
            }
            let name = self.ident()?;
            let payload = if matches!(&self.current().kind, TokenKind::Ident(word) if word == "of")
                && !self.offside()
            {
                self.take();
                Some(self.type_expr()?)
            } else {
                None
            };
            cases.push(UnionCaseDecl { name, payload });
            if !self.eat(&TokenKind::Pipe) {
                break;
            }
        }
        self.type_offside = outer;
        let derives = self.derives()?;
        self.eat(&TokenKind::Semicolon);
        Ok(UnionDecl {
            doc: None,
            visibility,
            name,
            parameters,
            cases,
            derives,
        })
    }

    fn derives(&mut self) -> Result<Vec<(DeriveClass, Span)>, Diagnostic> {
        let mut derives = Vec::new();
        if !self.eat(&TokenKind::Deriving) {
            return Ok(derives);
        }
        self.expect(&TokenKind::LeftParen, "'(' after 'deriving'")?;
        loop {
            let name = self.ident()?;
            let class = match name.text.as_str() {
                "Eq" => DeriveClass::Eq,
                "Ord" => DeriveClass::Ord,
                "Display" => DeriveClass::Display,
                "Hash" => DeriveClass::Hash,
                "Default" => DeriveClass::Default,
                _ => {
                    return Err(Diagnostic::new(
                        "E1025",
                        "only Eq, Ord, Display, Hash and Default can be derived",
                        name.span,
                    ));
                }
            };
            if derives.iter().any(|(previous, _)| *previous == class) {
                return Err(Diagnostic::new(
                    "E1001",
                    "duplicate derived class",
                    name.span,
                ));
            }
            derives.push((class, name.span));
            if !self.eat(&TokenKind::Comma) || self.at(&TokenKind::RightParen) {
                break;
            }
        }
        self.expect(&TokenKind::RightParen, "')' after derived classes")?;
        Ok(derives)
    }

    /// Whether the current token starts a new line at or left of the
    /// enclosing declaration, which ends a union payload type.
    fn offside(&self) -> bool {
        self.type_offside.is_some_and(|column| {
            self.newline_before_current() && self.column(self.current().span) <= column
        })
    }

    fn private_export(span: Span) -> Diagnostic {
        Diagnostic::new(
            "E1022",
            "'private export' is not allowed; remove 'private' or remove 'export'",
            span,
        )
    }

    fn region_list_ahead(&self) -> bool {
        if !self.at(&TokenKind::LeftBrace) {
            return false;
        }
        let mut named = false;
        for token in &self.tokens[self.position + 1..] {
            match token.kind {
                TokenKind::Ident(_) => named = true,
                TokenKind::Comma => {}
                TokenKind::RightBrace => return named,
                _ => return false,
            }
        }
        false
    }

    fn region_list(&mut self) -> Result<Vec<Ident>, Diagnostic> {
        self.expect(&TokenKind::LeftBrace, "'{' before region names")?;
        let mut regions = Vec::new();
        let mut used = BTreeSet::new();
        while !self.eat(&TokenKind::RightBrace) {
            let region = self.ident()?;
            if !region.text.as_bytes()[0].is_ascii_lowercase()
                || !region
                    .text
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
                || !used.insert(region.text.clone())
            {
                return Err(Diagnostic::new(
                    "E1013",
                    "region names must be distinct lowercase identifiers",
                    region.span,
                ));
            }
            if regions.len() >= MAX_NESTING {
                return Err(Diagnostic::new(
                    "E1017",
                    "too many region parameters",
                    region.span,
                ));
            }
            regions.push(region);
            self.eat(&TokenKind::Comma);
        }
        if regions.is_empty() {
            return Err(self.error("a region list must not be empty"));
        }
        Ok(regions)
    }

    fn member_ident(&mut self) -> Result<Ident, Diagnostic> {
        if self.at(&TokenKind::Union) {
            let token = self.take();
            Ok(Ident {
                text: "union".into(),
                span: token.span,
                provenance: Provenance::User,
            })
        } else {
            self.ident()
        }
    }

    fn qualified_ident(&mut self) -> Result<Ident, Diagnostic> {
        self.qualified_path(18)
    }

    /// Reads up to `segments` dot-separated identifiers, as in
    /// `Module.Union.Case` patterns.
    pub(super) fn qualified_path(&mut self, segments: usize) -> Result<Ident, Diagnostic> {
        let mut name = self.ident()?;
        for _ in 1..segments {
            if !self.eat(&TokenKind::Dot) {
                break;
            }
            let member = self.ident()?;
            name.text.push('.');
            name.text.push_str(&member.text);
            name.span = name.span.through(member.span);
        }
        if self.at(&TokenKind::Dot) {
            return Err(Diagnostic::new(
                "E1017",
                "qualified names exceed the 18-segment compiler limit",
                name.span,
            ));
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
            provenance: Provenance::User,
        })
    }

    fn class_parameter(&mut self) -> Result<(Ident, Kind), Diagnostic> {
        let variable = self.type_variable()?;
        let kind = if self.eat(&TokenKind::Colon) {
            self.kind_expression()?
        } else {
            Kind::Type
        };
        Ok((variable, kind))
    }

    fn kind_expression(&mut self) -> Result<Kind, Diagnostic> {
        self.enter()?;
        let left = if self.eat(&TokenKind::Star) {
            Kind::Type
        } else if self.eat(&TokenKind::LeftParen) {
            let kind = self.kind_expression()?;
            self.expect(&TokenKind::RightParen, "')' after kind")?;
            kind
        } else {
            return Err(self.error("expected '*' or a parenthesized kind"));
        };
        let result = if self.eat(&TokenKind::Arrow) {
            Kind::Arrow(Box::new(left), Box::new(self.kind_expression()?))
        } else {
            left
        };
        self.nesting -= 1;
        Ok(result)
    }

    fn variable_type(&mut self) -> Result<TypeExprKind, Diagnostic> {
        let mut variable = self.type_variable()?;
        if self.at(&TokenKind::Less) && self.current().span.start == self.previous_end {
            variable.text.insert(0, '\'');
            Ok(TypeExprKind::Apply(
                Box::new(variable),
                self.angle_list(Self::type_expr)?.into_boxed_slice(),
            ))
        } else {
            Ok(TypeExprKind::Variable(variable.text))
        }
    }

    fn type_parameters(&mut self) -> Result<Vec<Ident>, Diagnostic> {
        if self.at(&TokenKind::Less) {
            self.angle_list(Self::type_variable)
        } else if self.at(&TokenKind::TypeVariable(String::new())) {
            Err(self.error("enclose type parameters in '<...>', as in Name<'a, 'b>"))
        } else {
            Ok(Vec::new())
        }
    }

    fn angle_list<T>(
        &mut self,
        element: fn(&mut Self) -> Result<T, Diagnostic>,
    ) -> Result<Vec<T>, Diagnostic> {
        if self.at(&TokenKind::Less) && self.current().span.start != self.previous_end {
            return Err(self.error("'<' must immediately follow the name"));
        }
        self.expect(&TokenKind::Less, "'<' before type parameters or arguments")?;
        let mut elements = Vec::new();
        loop {
            if elements.len() >= MAX_NESTING {
                return Err(self.error("too many type parameters or arguments"));
            }
            elements.push(element(self)?);
            if !self.eat(&TokenKind::Comma) || self.at_type_close() {
                break;
            }
        }
        self.type_close()?;
        Ok(elements)
    }

    fn single_type_argument(&mut self) -> Result<TypeExpr, Diagnostic> {
        let mut arguments = self.angle_list(Self::type_expr)?;
        if arguments.len() != 1 {
            return Err(self.error("expected exactly one type argument"));
        }
        Ok(arguments.remove(0))
    }

    fn at_type_close(&self) -> bool {
        matches!(
            self.current().kind,
            TokenKind::Greater
                | TokenKind::GreaterEqual
                | TokenKind::ShiftRight
                | TokenKind::ShiftRightUnsigned
        )
    }

    fn type_close(&mut self) -> Result<(), Diagnostic> {
        let remainder = match self.current().kind {
            TokenKind::Greater => {
                self.take();
                return Ok(());
            }
            TokenKind::GreaterEqual => TokenKind::Equal,
            TokenKind::ShiftRight => TokenKind::Greater,
            TokenKind::ShiftRightUnsigned => TokenKind::ShiftRight,
            _ => return Err(self.error("expected '>' after type parameters or arguments")),
        };
        // Consume one '>' without inserting tokens or changing expression operators.
        let token = &mut self.tokens[self.position];
        token.kind = remainder;
        token.span.start += 1;
        self.previous_end = token.span.start;
        Ok(())
    }

    fn instance_method(&mut self) -> Result<Definition, Diagnostic> {
        let anonymous = self.eat(&TokenKind::Let);
        if !anonymous {
            self.expect(&TokenKind::Fn, "a 'fn' or 'let' method definition, or '}'")?;
        }
        let recursive = self.eat(&TokenKind::Rec);
        let name = self.ident()?;
        let mut definition = self.definition(name.clone())?;
        definition.recursion = recursive.then_some(name.text);
        if anonymous
            && (!definition.parameters.is_empty()
                || !matches!(definition.body.kind, ExprKind::Lambda(..)))
        {
            return Err(Diagnostic::new(
                "E0002",
                "a 'let' method implementation needs a lambda",
                definition.name.span,
            ));
        }
        Ok(definition)
    }

    fn constraints(&mut self) -> Result<Vec<ConstraintExpr>, Diagnostic> {
        let mut constraints = Vec::new();
        // A constraint prefix always ends in => before the next declaration/body.
        let has_constraints = self.constraint_prefix();
        if has_constraints {
            let grouped = self.eat(&TokenKind::LeftParen);
            loop {
                let class = self.qualified_ident()?;
                let ty = self.single_type_argument()?;
                constraints.push(ConstraintExpr {
                    name: ConstraintName::Class(class),
                    ty,
                });
                if !self.eat(&TokenKind::Comma) {
                    break;
                }
            }
            if grouped {
                self.expect(&TokenKind::RightParen, "')' after constraints")?;
            }
            self.expect(&TokenKind::FatArrow, "'=>' after constraints")?;
        }
        Ok(constraints)
    }

    fn signature(
        &mut self,
        name: Ident,
        exported: bool,
        column: usize,
    ) -> Result<SignatureDecl, Diagnostic> {
        let mut constraints = self.constraints()?;
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
        let constraint_column = self.column(self.current().span);
        while self.at(&TokenKind::At) {
            if !self.newline_before_current()
                || constraint_column <= column
                || self.column(self.current().span) != constraint_column
            {
                return Err(
                    self.error("constraint lines must be indented after the 'def' signature")
                );
            }
            self.take();
            if self.newline_before_current() {
                return Err(self.error("expected a type variable after '@' on the same line"));
            }
            let variable = self.type_variable()?;
            let ty = TypeExpr {
                kind: TypeExprKind::Variable(variable.text),
                span: variable.span,
            };
            if self.newline_before_current() {
                return Err(self.error("expected ':' on the constraint line"));
            }
            self.expect(&TokenKind::Colon, "':' before the constraint list")?;
            loop {
                if self.newline_before_current() {
                    return Err(
                        self.error("expected a type class or '#function' on the constraint line")
                    );
                }
                let name = if self.eat(&TokenKind::Hash) {
                    if self.newline_before_current() {
                        return Err(
                            self.error("expected a function name after '#' on the same line")
                        );
                    }
                    ConstraintName::Function(self.ident()?)
                } else {
                    ConstraintName::Class(self.qualified_ident()?)
                };
                constraints.push(ConstraintExpr {
                    name,
                    ty: ty.clone(),
                });
                if self.newline_before_current() || !self.eat(&TokenKind::Comma) {
                    break;
                }
            }
        }
        Ok(SignatureDecl {
            doc: None,
            name,
            regions: Vec::new(),
            recursion: None,
            visibility: Visibility::Public,
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
                TokenKind::DocComment(_)
                | TokenKind::Def
                | TokenKind::Private
                | TokenKind::Let
                | TokenKind::Fn
                | TokenKind::And
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
        while !self.at(&TokenKind::Equal) && !self.at(&TokenKind::Pipe) {
            let mutable = self.eat(&TokenKind::Mut);
            parameters.push((self.ident()?, mutable));
        }
        let body = if self.at(&TokenKind::Pipe) {
            self.guarded_definition(&parameters)?
        } else {
            self.take();
            self.body_expression()?
        };
        Ok(Definition {
            name,
            recursion: None,
            parameters,
            body,
        })
    }

    pub(crate) fn define(
        signature: SignatureDecl,
        mut definition: Definition,
    ) -> Result<FunctionDecl, Diagnostic> {
        if signature.recursion != definition.recursion {
            return Err(Diagnostic::new(
                "E1019",
                "'def' and its implementation must have matching 'rec'/'and' groups",
                definition.name.span,
            ));
        }
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
            doc: signature.doc,
            name: definition.name,
            regions: signature.regions,
            recursion: definition.recursion,
            visibility: signature.visibility,
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
        let first = self.type_product()?;
        if !self.eat(&TokenKind::Arrow) {
            return Ok(first);
        }
        let mut parameters = vec![first];
        let result = loop {
            let next = self.type_product()?;
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

    fn type_primary(&mut self) -> Result<TypeExpr, Diagnostic> {
        self.enter()?;
        let start = self.current().span;
        let kind = if self.eat(&TokenKind::Ampersand) || self.eat(&TokenKind::Ref) {
            self.reference_type(start)?
        } else if self.eat(&TokenKind::AndAnd) {
            // As in Rust, `&&T` is `& &T` and `&&mut T` is `& &mut T`.
            let mutable = self.eat(&TokenKind::Mut);
            let value = self.type_primary()?;
            let span = Span {
                start: start.start + 1,
                ..start.through(value.span)
            };
            TypeExprKind::Reference(
                Box::new(TypeExpr {
                    kind: TypeExprKind::Reference(Box::new(value), mutable),
                    span,
                }),
                false,
            )
        } else if self.at(&TokenKind::TypeVariable(String::new())) {
            self.variable_type()?
        } else if self.eat(&TokenKind::LeftParen) {
            let ty = self.type_expr()?;
            self.expect(&TokenKind::RightParen, "')' after the type")?;
            ty.kind
        } else if self.at(&TokenKind::Fn)
            && self
                .tokens
                .get(self.position + 1)
                .is_some_and(|token| token.kind == TokenKind::LeftParen)
        {
            self.take();
            self.take();
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
            if name.text == "Task" && self.at(&TokenKind::Less) {
                TypeExprKind::Task(Box::new(self.single_type_argument()?))
            } else if self.at(&TokenKind::Less) && self.current().span.start == self.previous_end {
                TypeExprKind::Apply(
                    Box::new(name),
                    self.angle_list(Self::type_expr)?.into_boxed_slice(),
                )
            } else {
                TypeExprKind::Named(name.text)
            }
        };
        self.nesting -= 1;
        self.finish_type_primary(kind, start)
    }

    fn reference_type(&mut self, start: Span) -> Result<TypeExprKind, Diagnostic> {
        let mutable = self.eat(&TokenKind::Mut);
        let regions = if self.at(&TokenKind::LeftBrace) {
            self.region_list()?
        } else {
            Vec::new()
        };
        let inner = self.type_primary()?;
        let span = start.through(inner.span);
        let reference = TypeExprKind::Reference(Box::new(inner), mutable);
        if regions.is_empty() {
            return Ok(reference);
        }
        if regions.len() != 1 {
            return Err(Diagnostic::new(
                "E1013",
                "a reference has exactly one region",
                span,
            ));
        }
        Ok(TypeExprKind::Regions(
            Box::new(TypeExpr {
                kind: reference,
                span,
            }),
            regions.into_boxed_slice(),
        ))
    }

    fn finish_type_primary(
        &mut self,
        kind: TypeExprKind,
        start: Span,
    ) -> Result<TypeExpr, Diagnostic> {
        let mut ty = TypeExpr {
            kind,
            span: Span {
                end: self.previous_end,
                ..start
            },
        };
        if self.region_suffix && self.region_list_ahead() && !self.newline_before_current() {
            let regions = self.region_list()?;
            let span = Span {
                end: self.previous_end,
                ..ty.span
            };
            ty = TypeExpr {
                kind: TypeExprKind::Regions(Box::new(ty), regions.into_boxed_slice()),
                span,
            };
        }
        Ok(ty)
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
        let outer_arm = std::mem::replace(&mut self.stop_at_arm, false);
        let outer_arrow = std::mem::replace(&mut self.stop_at_arrow, false);
        let result = if self.implicit_computation_ahead(None) {
            let mut body = self.implicit_computation(None, false)?;
            let end = self.expect(&TokenKind::RightBrace, "'}' after the computation")?;
            body.span = start.through(end.span);
            Ok(body)
        } else if matches!(
            self.current().kind,
            TokenKind::Let | TokenKind::Return | TokenKind::Do | TokenKind::RightBrace
        ) {
            self.block_after_open(start, None)
        } else {
            let first = self.expression(0, true)?;
            if self.eat(&TokenKind::With) {
                self.record_update(start, first)
            } else {
                self.block_after_open(start, Some(first))
            }
        };
        self.stop_at_arm = outer_arm;
        self.stop_at_arrow = outer_arrow;
        self.nesting -= 1;
        result
    }

    fn block_after_open(
        &mut self,
        start: Span,
        mut first: Option<Expr>,
    ) -> Result<Expr, Diagnostic> {
        let mut bindings = Vec::new();
        let mut depth = 0;
        let result = loop {
            if first.is_none() && self.eat(&TokenKind::Let) {
                let binding = self.binding(self.in_task)?;
                depth = depth.max(binding.value.depth);
                bindings.push(binding);
            } else if first.is_none() && self.at(&TokenKind::Return) {
                break self.task_return()?;
            } else if first.is_none() && self.at(&TokenKind::Do) {
                let binding = self.task_do()?;
                depth = depth.max(binding.value.depth);
                bindings.push(binding);
            } else if first.is_none() && self.at(&TokenKind::RightBrace) {
                break self.make(ExprKind::Unit, self.current().span, 1)?;
            } else {
                let value = match first.take() {
                    Some(value) => value,
                    None => self.expression(0, true)?,
                };
                if !self.eat(&TokenKind::Semicolon)
                    && !(self.newline_before_current() && !self.at(&TokenKind::RightBrace))
                {
                    break value;
                }
                depth = depth.max(value.depth);
                bindings.push(Binding {
                    name: Ident {
                        text: "_".into(),
                        span: value.span,
                        provenance: Provenance::Generated,
                    },
                    mutable: false,
                    annotation: None,
                    value,
                });
            }
        };
        depth = depth.max(result.depth) + 1;
        let end = self.expect(&TokenKind::RightBrace, "'}' after the result expression")?;
        self.make(
            ExprKind::Block {
                bindings,
                result: Box::new(result),
            },
            start.through(end.span),
            depth,
        )
    }

    fn record_update(&mut self, start: Span, base: Expr) -> Result<Expr, Diagnostic> {
        let mut fields = Vec::new();
        let mut depth = base.depth;
        loop {
            let name = self.ident()?;
            if !self.eat(&TokenKind::Equal) {
                self.expect(&TokenKind::Colon, "'=' or ':' after the update field name")?;
            }
            let value = self.expression(0, true)?;
            depth = depth.max(value.depth);
            fields.push((name, value));
            if !(self.eat(&TokenKind::Semicolon) || self.eat(&TokenKind::Comma))
                || self.at(&TokenKind::RightBrace)
            {
                break;
            }
        }
        let end = self.expect(&TokenKind::RightBrace, "'}' after the record update")?;
        self.make(
            ExprKind::RecordUpdate {
                base: Box::new(base),
                fields,
            },
            start.through(end.span),
            depth + 1,
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
                provenance: Provenance::Generated,
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
        let mut binding = self.binding_value(top_level)?;
        if run {
            let span = binding.value.span;
            let depth = binding.value.depth + 1;
            binding.value = self.make(ExprKind::TaskRun(Box::new(binding.value)), span, depth)?;
        }
        self.binding_end(top_level)?;
        Ok(binding)
    }

    fn binding_value(&mut self, stop_at_newline: bool) -> Result<Binding, Diagnostic> {
        let mutable = self.eat(&TokenKind::Mut);
        let name = self.ident()?;
        let annotation = if self.eat(&TokenKind::Colon) {
            Some(self.type_expr()?)
        } else {
            None
        };
        self.expect(&TokenKind::Equal, "'=' in the binding")?;
        let value = self.expression_inner(0, true, stop_at_newline)?;
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
                && (self.at(&TokenKind::End) || newline || self.at(&TokenKind::RightBrace)))
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
        self.source[self.previous_end..self.current().span.start].contains(['\n', '\r'])
    }

    fn entry(&mut self) -> Result<Expr, Diagnostic> {
        let result = self.layout_block(0)?;
        self.expect(
            &TokenKind::End,
            "the end of the file; declarations must precede the entry-point code",
        )?;
        Ok(result)
    }

    fn record(&mut self, name: Ident) -> Result<Expr, Diagnostic> {
        let start = name.span;
        self.expect(&TokenKind::LeftBrace, "'{' after the record name")?;
        let outer_arm = std::mem::replace(&mut self.stop_at_arm, false);
        let outer_arrow = std::mem::replace(&mut self.stop_at_arrow, false);
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
        self.stop_at_arm = outer_arm;
        self.stop_at_arrow = outer_arrow;
        let depth = fields
            .iter()
            .map(|(_, value)| value.depth)
            .max()
            .unwrap_or(0)
            + 1;
        self.make(
            ExprKind::Record {
                name: Box::new(name),
                fields,
            },
            start.through(end.span),
            depth,
        )
    }

    fn computation(&mut self, builder: Ident) -> Result<Expr, Diagnostic> {
        let outer = self.in_task;
        self.in_task = false;
        let body = self.computation_block()?;
        self.in_task = outer;
        let span = builder.span.through(body.span);
        let depth = body.depth + 1;
        self.make(ExprKind::Computation(builder, Box::new(body)), span, depth)
    }

    fn implicit_computation_ahead(&self, indent: Option<usize>) -> bool {
        if self.in_task {
            return false;
        }
        let mut depth = 0usize;
        let mut statement = true;
        let mut previous_end = self.previous_end;
        for (offset, token) in self.tokens[self.position..].iter().enumerate() {
            let newline = self.source[previous_end..token.span.start].contains(['\n', '\r']);
            if depth == 0 {
                statement |=
                    newline && indent.is_none_or(|indent| self.column(token.span) == indent);
                if matches!(
                    token.kind,
                    TokenKind::End
                        | TokenKind::RightBrace
                        | TokenKind::RightParen
                        | TokenKind::RightBracket
                        | TokenKind::RightList
                ) || (statement
                    && matches!(
                        token.kind,
                        TokenKind::Def
                            | TokenKind::Fn
                            | TokenKind::Export
                            | TokenKind::Private
                            | TokenKind::Const
                            | TokenKind::Test
                    ))
                    || indent.is_some_and(|indent| newline && self.column(token.span) < indent)
                {
                    break;
                }
                if statement
                    && (matches!(
                        token.kind,
                        TokenKind::Do | TokenKind::Return | TokenKind::Yield
                    ) || (matches!(token.kind, TokenKind::Let | TokenKind::Match)
                        && self
                            .tokens
                            .get(self.position + offset + 1)
                            .is_some_and(|next| next.kind == TokenKind::Bang)))
                {
                    return true;
                }
                statement = token.kind == TokenKind::Semicolon;
            }
            match token.kind {
                TokenKind::LeftBrace
                | TokenKind::LeftParen
                | TokenKind::LeftBracket
                | TokenKind::LeftList => depth += 1,
                TokenKind::RightBrace
                | TokenKind::RightParen
                | TokenKind::RightBracket
                | TokenKind::RightList => depth -= 1,
                _ => {}
            }
            previous_end = token.span.end;
        }
        false
    }

    fn implicit_computation(
        &mut self,
        indent: Option<usize>,
        single: bool,
    ) -> Result<Expr, Diagnostic> {
        self.enter()?;
        let body = self.computation_sequence(indent, single)?;
        self.nesting -= 1;
        let span = body.span;
        let depth = body.depth + 1;
        self.make(
            ExprKind::Computation(
                Ident {
                    text: "$implicit".into(),
                    span,
                    provenance: Provenance::Generated,
                },
                Box::new(body),
            ),
            span,
            depth,
        )
    }

    fn computation_block(&mut self) -> Result<ComputationBlock, Diagnostic> {
        self.enter()?;
        let start = self.expect(&TokenKind::LeftBrace, "'{'")?.span;
        let outer_arm = std::mem::replace(&mut self.stop_at_arm, false);
        let outer_arrow = std::mem::replace(&mut self.stop_at_arrow, false);
        let mut body = self.computation_sequence(None, false)?;
        let end = self.expect(&TokenKind::RightBrace, "'}' after the computation")?;
        body.span = start.through(end.span);
        self.stop_at_arm = outer_arm;
        self.stop_at_arrow = outer_arrow;
        self.nesting -= 1;
        Ok(body)
    }

    fn computation_body(&mut self) -> Result<ComputationBlock, Diagnostic> {
        if self.at(&TokenKind::LeftBrace) {
            return self.computation_block();
        }
        self.enter()?;
        let single = !self.newline_before_current();
        let body = self.computation_sequence(Some(self.column(self.current().span)), single)?;
        self.nesting -= 1;
        if body.statements.is_empty() {
            return Err(self.error("expected a computation body"));
        }
        Ok(body)
    }

    fn computation_end(&self, indent: Option<usize>) -> bool {
        self.at(&TokenKind::RightBrace)
            || self.at(&TokenKind::End)
            || indent.is_some_and(|indent| {
                self.column(self.current().span) < indent
                    || matches!(
                        self.current().kind,
                        TokenKind::Else
                            | TokenKind::Elif
                            | TokenKind::Pipe
                            | TokenKind::Def
                            | TokenKind::Fn
                            | TokenKind::Export
                            | TokenKind::Private
                            | TokenKind::Const
                            | TokenKind::Test
                            | TokenKind::RightParen
                            | TokenKind::RightBracket
                            | TokenKind::RightList
                    )
            })
    }

    fn computation_sequence(
        &mut self,
        indent: Option<usize>,
        single: bool,
    ) -> Result<ComputationBlock, Diagnostic> {
        let start = self.current().span;
        let mut statements = Vec::new();
        let mut depth = 0;
        while !self.computation_end(indent) {
            let statement = self.computation_statement()?;
            if !single && !self.computation_end(indent) {
                self.binding_end(true)?;
            }
            if matches!(
                statement.kind,
                ComputationStatementKind::Operation("Return" | "ReturnFrom", _)
            ) && !single
                && !self.computation_end(indent)
            {
                return Err(self.error("'return' and 'return!' must end a computation block"));
            }
            depth = depth.max(statement.depth());
            statements.push(statement);
            if single {
                break;
            }
        }
        let depth = depth + 1;
        let span = start.through(statements.last().map_or(start, |statement| statement.span));
        self.make(ExprKind::Unit, span, depth)?;
        Ok(ComputationBlock {
            statements,
            span,
            depth,
        })
    }

    fn computation_statement(&mut self) -> Result<ComputationStatement, Diagnostic> {
        let start = self.current().span;
        if let TokenKind::Ident(name) = &self.current().kind {
            let following = &self.tokens[self.position + 1..];
            if name == "use"
                && (following
                    .first()
                    .is_some_and(|token| token.kind == TokenKind::Bang)
                    || (following
                        .first()
                        .is_some_and(|token| matches!(token.kind, TokenKind::Ident(_)))
                        && following.get(1).is_some_and(|token| {
                            matches!(token.kind, TokenKind::Equal | TokenKind::Colon)
                        })))
            {
                return Err(Diagnostic::new(
                    "E1018",
                    "use bindings are not supported; bind the owned value with let and rely on lexical drop",
                    start,
                ));
            }
            if name == "try" && self.try_clause_ahead() {
                return Err(Diagnostic::new(
                    "E1018",
                    "try expressions are not supported; represent recoverable failure with Option or Result",
                    start,
                ));
            }
        }
        if self.at(&TokenKind::If) {
            return self.computation_if();
        }
        if self.at(&TokenKind::Match)
            && self
                .tokens
                .get(self.position + 1)
                .is_some_and(|token| token.kind == TokenKind::Bang)
        {
            return self.computation_match();
        }
        let kind = if self.eat(&TokenKind::Let) {
            self.computation_binding()?
        } else if self.eat(&TokenKind::Do) {
            let bind = self.eat(&TokenKind::Bang);
            let value = self.expression_inner(0, true, true)?;
            if bind {
                ComputationStatementKind::Do(value)
            } else {
                ComputationStatementKind::Expression(value)
            }
        } else if self.at(&TokenKind::Return) || self.at(&TokenKind::Yield) {
            let returns = self.take().kind == TokenKind::Return;
            let from = self.eat(&TokenKind::Bang);
            let operation = match (returns, from) {
                (true, false) => "Return",
                (true, true) => "ReturnFrom",
                (false, false) => "Yield",
                (false, true) => "YieldFrom",
            };
            ComputationStatementKind::Operation(operation, self.expression_inner(0, true, true)?)
        } else if self.eat(&TokenKind::For) {
            let pattern = Box::new(self.pattern(0)?);
            self.expect(&TokenKind::In, "'in' after the loop binding")?;
            let source = self.expression(0, false)?;
            let body = if self.eat(&TokenKind::Do) {
                self.computation_body()?
            } else {
                self.computation_block()?
            };
            ComputationStatementKind::For(pattern, source, body)
        } else if self.eat(&TokenKind::While) {
            let condition = self.expression(0, false)?;
            let body = if self.eat(&TokenKind::Do) {
                self.computation_body()?
            } else {
                self.computation_block()?
            };
            ComputationStatementKind::While(condition, body)
        } else {
            ComputationStatementKind::Expression(self.expression_inner(0, true, true)?)
        };
        Ok(ComputationStatement {
            kind,
            span: start.through(self.tokens[self.position - 1].span),
        })
    }

    fn computation_binding(&mut self) -> Result<ComputationStatementKind, Diagnostic> {
        let bind = self.eat(&TokenKind::Bang);
        let binding = self.binding_value(true)?;
        if !bind || !self.at(&TokenKind::And) {
            return Ok(ComputationStatementKind::Let(binding, bind));
        }
        let mut bindings = vec![binding];
        let mut names = BTreeSet::from([bindings[0].name.text.clone()]);
        while self.eat(&TokenKind::And) {
            self.expect(&TokenKind::Bang, "'!' after and in a binding group")?;
            let binding = self.binding_value(true)?;
            if binding.name.text != "_" && !names.insert(binding.name.text.clone()) {
                return Err(Diagnostic::new(
                    "E1001",
                    "duplicate and! binding name",
                    binding.name.span,
                ));
            }
            if bindings.len() >= MAX_NESTING {
                return Err(Diagnostic::new(
                    "E1017",
                    "and! group exceeds the compiler binding limit",
                    binding.name.span,
                ));
            }
            bindings.push(binding);
        }
        Ok(ComputationStatementKind::LetAnd(bindings))
    }

    fn try_clause_ahead(&self) -> bool {
        let mut depth = 0usize;
        for token in &self.tokens[self.position + 1..] {
            match &token.kind {
                TokenKind::LeftParen
                | TokenKind::LeftBracket
                | TokenKind::LeftList
                | TokenKind::LeftBrace => depth += 1,
                TokenKind::RightParen
                | TokenKind::RightBracket
                | TokenKind::RightList
                | TokenKind::RightBrace => {
                    if depth == 0 {
                        break;
                    }
                    depth -= 1;
                }
                TokenKind::Semicolon | TokenKind::End if depth == 0 => break,
                TokenKind::With if depth == 0 => return true,
                TokenKind::Ident(name) if depth == 0 && name == "finally" => return true,
                _ => {}
            }
        }
        false
    }

    fn computation_match(&mut self) -> Result<ComputationStatement, Diagnostic> {
        self.enter()?;
        let start = self.take().span;
        self.expect(&TokenKind::Bang, "'!' after match")?;
        let value = self.expression(0, false)?;
        self.expect(&TokenKind::With, "'with' after the match! computation")?;
        let outer_arm = self.stop_at_arm;
        let outer_arrow = self.stop_at_arrow;
        let column = self.column(self.current().span);
        let mut arms = Vec::new();
        while self.at(&TokenKind::Pipe)
            && (!self.newline_before_current() || self.column(self.current().span) == column)
        {
            let arm_start = self.take().span;
            self.stop_at_arm = true;
            self.stop_at_arrow = true;
            let pattern = self.pattern(0)?;
            let guard = if self.eat(&TokenKind::When) {
                Some(self.expression_inner(0, true, true)?)
            } else {
                None
            };
            self.expect(&TokenKind::Arrow, "'->' after the match! pattern")?;
            self.stop_at_arrow = false;
            let body = self.computation_body()?;
            let span = arm_start.through(body.span);
            arms.push(ComputationMatchArm {
                pattern,
                guard,
                body,
                span,
            });
        }
        self.stop_at_arm = outer_arm;
        self.stop_at_arrow = outer_arrow;
        self.nesting -= 1;
        if arms.is_empty() {
            return Err(self.error("match! requires at least one '| pattern -> computation' arm"));
        }
        let span = start.through(arms.last().unwrap().span);
        Ok(ComputationStatement {
            kind: ComputationStatementKind::Match(Box::new(value), arms),
            span,
        })
    }

    fn computation_if(&mut self) -> Result<ComputationStatement, Diagnostic> {
        self.enter()?;
        let start = self.take().span;
        let condition = self.expression(0, false)?;
        let keyword = self.eat(&TokenKind::Then);
        let yes = if keyword {
            self.computation_body()?
        } else {
            self.computation_block()?
        };
        let has_else = (self.at(&TokenKind::Else) || self.at(&TokenKind::Elif))
            && (!keyword
                || !self.newline_before_current()
                || self.column(self.current().span) >= self.column(start));
        let no = if has_else {
            self.eat(&TokenKind::Else);
            Some(if self.at(&TokenKind::If) || self.at(&TokenKind::Elif) {
                let statement = self.computation_if()?;
                ComputationBlock {
                    span: statement.span,
                    depth: statement.depth() + 1,
                    statements: vec![statement],
                }
            } else {
                if keyword {
                    self.computation_body()?
                } else {
                    self.computation_block()?
                }
            })
        } else {
            None
        };
        self.nesting -= 1;
        Ok(ComputationStatement {
            span: start.through(self.tokens[self.position - 1].span),
            kind: ComputationStatementKind::If(condition, yes, no),
        })
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
            if self.stop_at_arm && self.at(&TokenKind::Pipe) {
                break;
            }
            if minimum == 0 && !self.stop_at_slice_dotdot && self.eat(&TokenKind::DotDot) {
                left = self.range_expression(left, allow_record, stop_at_newline)?;
                continue;
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
                let ty = self.type_primary()?;
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
                    let outer = self.stop_at_arm;
                    let outer_arrow = self.stop_at_arrow;
                    self.stop_at_arm = false;
                    self.stop_at_arrow = false;
                    let mut grouped = self.expressions(TokenKind::RightParen)?;
                    self.stop_at_arm = outer;
                    self.stop_at_arrow = outer_arrow;
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
                        let span = start.through(self.tokens[self.position - 1].span);
                        let depth = grouped.iter().map(|value| value.depth).max().unwrap_or(0) + 1;
                        arguments.push(self.make(ExprKind::Tuple(grouped), span, depth)?);
                    }
                }
            } else {
                arguments.push(self.term(allow_record, stop_at_newline)?);
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
            && match self.current().kind {
                TokenKind::Ident(_)
                | TokenKind::TypeVariable(_)
                | TokenKind::Integer(_)
                | TokenKind::Float(_)
                | TokenKind::String(_)
                | TokenKind::Char(_)
                | TokenKind::Utf8Char(_)
                | TokenKind::True
                | TokenKind::False
                | TokenKind::New
                | TokenKind::Task
                | TokenKind::Fx
                | TokenKind::Ref
                | TokenKind::Deref
                | TokenKind::LeftParen
                | TokenKind::LeftBracket
                | TokenKind::LeftList => true,
                // Never an argument; parsing it as one reports the 'ref mut' hint.
                TokenKind::Mut => true,
                // `f &x` and `f *r` pass prefix arguments; `f & x` and `a * b` stay binary.
                TokenKind::Ampersand | TokenKind::Star => {
                    self.tokens.get(self.position + 1).is_some_and(|next| {
                        next.kind != TokenKind::End && next.span.start == self.current().span.end
                    })
                }
                _ => false,
            }
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
                let field = self.member_ident()?;
                let span = left.span.through(field.span);
                let depth = left.depth + 1;
                self.make(ExprKind::Field(Box::new(left), field), span, depth)
            }
            TokenKind::LeftBracket => self.index_or_slice(left),
            _ => unreachable!("postfix operators are checked by expression"),
        }
    }

    fn index_or_slice(&mut self, left: Expr) -> Result<Expr, Diagnostic> {
        let outer_arm = std::mem::replace(&mut self.stop_at_arm, false);
        let outer_arrow = std::mem::replace(&mut self.stop_at_arrow, false);
        let borrowed = std::mem::replace(&mut self.slice_context, false);
        let outer_range = std::mem::replace(&mut self.stop_at_slice_dotdot, true);
        let first = if self.at(&TokenKind::DotDot) {
            None
        } else {
            Some(self.expression(0, true)?)
        };
        let slice = self.eat(&TokenKind::DotDot);
        let last = if slice && !self.at(&TokenKind::RightBracket) {
            Some(self.expression(0, true)?)
        } else {
            None
        };
        let end = self.expect(&TokenKind::RightBracket, "']'")?;
        self.stop_at_arm = outer_arm;
        self.stop_at_arrow = outer_arrow;
        self.slice_context = borrowed;
        self.stop_at_slice_dotdot = outer_range;
        let span = left.span.through(end.span);
        let depth = first
            .iter()
            .chain(&last)
            .map(|value| value.depth)
            .max()
            .unwrap_or(0)
            .max(left.depth)
            + 1;
        let kind = if slice {
            if !borrowed {
                return Err(Diagnostic::new(
                    "E0002",
                    "array slices must be shared borrows; write 'ref xs[start..end]' or '&xs[start..end]'",
                    span,
                ));
            }
            if first.is_none() && last.is_none() {
                return Err(Diagnostic::new(
                    "E0002",
                    "a slice needs at least one bound; borrow the whole array with 'ref xs'",
                    span,
                ));
            }
            ExprKind::Slice {
                value: Box::new(left),
                start: first.map(Box::new),
                end: last.map(Box::new),
            }
        } else {
            ExprKind::Index(
                Box::new(left),
                Box::new(first.expect("an index has one expression")),
            )
        };
        self.make(kind, span, depth)
    }

    fn expressions(&mut self, end: TokenKind) -> Result<Vec<Expr>, Diagnostic> {
        let outer_arm = std::mem::replace(&mut self.stop_at_arm, false);
        let outer_arrow = std::mem::replace(&mut self.stop_at_arrow, false);
        let outer_slice = std::mem::replace(&mut self.slice_context, false);
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
        self.stop_at_arm = outer_arm;
        self.stop_at_arrow = outer_arrow;
        self.slice_context = outer_slice;
        Ok(values)
    }

    fn new_collection(&mut self) -> Result<Expr, Diagnostic> {
        let start = self.take().span;
        let list = self.at(&TokenKind::LeftList);
        if !list && !self.at(&TokenKind::LeftBracket) {
            return Err(self.error(
                "expected an array or list after 'new': 'new [T](length, initializer)', 'new [values]', or 'new [|values|]'",
            ));
        }
        if !self.sized_collection() {
            let literal = self.collection_literal()?;
            if let ExprKind::Array(values) | ExprKind::List(values) = &literal.kind {
                if let [
                    Expr {
                        kind: ExprKind::Name(name),
                        ..
                    },
                ] = values.as_slice()
                {
                    if crate::numeric::primitive(&name.text).is_some() {
                        return Err(Diagnostic::new(
                            "E0002",
                            format!(
                                "expected '(' after the collection type; use 'new [{}](length, initializer)'",
                                name.text
                            ),
                            literal.span,
                        ));
                    }
                }
            }
            let span = start.through(literal.span);
            let depth = literal.depth + 1;
            return self.make(ExprKind::NewLiteral(Box::new(literal)), span, depth);
        }
        let ty = self.type_primary()?;
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

    /// `new [T](...)` puts '(' directly after the closing delimiter; `new [values]` does not.
    fn sized_collection(&self) -> bool {
        let mut depth = 0usize;
        for (offset, token) in self.tokens[self.position..].iter().enumerate() {
            match token.kind {
                TokenKind::LeftBracket
                | TokenKind::LeftList
                | TokenKind::LeftParen
                | TokenKind::LeftBrace => depth += 1,
                TokenKind::RightBracket
                | TokenKind::RightList
                | TokenKind::RightParen
                | TokenKind::RightBrace => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        return self
                            .tokens
                            .get(self.position + offset + 1)
                            .is_some_and(|next| next.kind == TokenKind::LeftParen);
                    }
                }
                TokenKind::End => return false,
                _ => {}
            }
        }
        false
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
            TokenKind::Fx => return self.fx(),
            TokenKind::New => return self.new_collection(),
            TokenKind::While => return self.while_expression(),
            TokenKind::For => return self.for_expression(),
            TokenKind::Match => return self.match_expression(),
            TokenKind::Integer(_)
            | TokenKind::Float(_)
            | TokenKind::String(_)
            | TokenKind::Char(_)
            | TokenKind::Utf8Char(_) => {
                return self.literal();
            }
            TokenKind::Ampersand
            | TokenKind::Star
            | TokenKind::Minus
            | TokenKind::Bang
            | TokenKind::Tilde => return self.prefix(allow_record, stop_at_newline, false),
            TokenKind::Ref | TokenKind::Deref => {
                return self.keyword_prefix(allow_record, stop_at_newline);
            }
            TokenKind::Mut => {
                return Err(self.error(
                    "'mut' is not an expression; write 'ref mut x' to borrow exclusively, or 'let mut x = ...' for a mutable binding",
                ));
            }
            TokenKind::True | TokenKind::False => {
                ExprKind::Bool(self.take().kind == TokenKind::True)
            }
            TokenKind::Break => {
                self.take();
                ExprKind::Break
            }
            TokenKind::Continue => {
                self.take();
                ExprKind::Continue
            }
            TokenKind::TypeVariable(_) => return self.type_function_expression(),
            TokenKind::Ident(_) => {
                return self.identifier_expression(allow_record, stop_at_newline);
            }
            TokenKind::LeftParen => return self.grouped_expression(),
            TokenKind::LeftBracket | TokenKind::LeftList => return self.collection_literal(),
            TokenKind::LeftBrace => return self.block(),
            TokenKind::If => return self.conditional(),
            _ => return Err(self.error("expected an expression")),
        };
        let end = self.tokens[self.position - 1].span;
        self.make(kind, start.through(end), 1)
    }

    fn test_declaration(&mut self) -> Result<TestDecl, Diagnostic> {
        let start = self.take().span;
        let token = self.take();
        let name = match token.kind {
            TokenKind::String(StringLiteral::Utf16(units)) => {
                String::from_utf16(&units).map_err(|_| {
                    Diagnostic::new(
                        "E0002",
                        "test names must contain valid Unicode scalars",
                        token.span,
                    )
                })?
            }
            TokenKind::String(StringLiteral::Utf8(name)) => name,
            _ => {
                return Err(Diagnostic::new(
                    "E0002",
                    "expected a string literal test name",
                    token.span,
                ));
            }
        };
        self.expect(&TokenKind::Equal, "'=' after the test name")?;
        let body = self.body_expression()?;
        let span = start.through(body.span);
        self.eat(&TokenKind::Semicolon);
        Ok(TestDecl {
            name,
            name_span: token.span,
            body,
            span,
        })
    }

    fn identifier_expression(
        &mut self,
        allow_record: bool,
        stop_at_newline: bool,
    ) -> Result<Expr, Diagnostic> {
        let mut name = self.ident()?;
        if !self.stop_at_arrow && self.eat(&TokenKind::Arrow) {
            return self.lambda(name, stop_at_newline);
        }
        if allow_record {
            let mut end = self.position;
            while self
                .tokens
                .get(end)
                .is_some_and(|token| token.kind == TokenKind::Dot)
                && self
                    .tokens
                    .get(end + 1)
                    .is_some_and(|token| matches!(token.kind, TokenKind::Ident(_)))
            {
                end += 2;
            }
            if self.tokens.get(end).is_some_and(|token| {
                token.kind == TokenKind::LeftBrace
                    && (!stop_at_newline
                        || !self.source[name.span.end..token.span.start].contains(['\n', '\r']))
            }) {
                if end - self.position > 34 {
                    return Err(Diagnostic::new(
                        "E1017",
                        "qualified names exceed the 18-segment compiler limit",
                        name.span,
                    ));
                }
                while self.position < end {
                    self.take();
                    let member = self.ident()?;
                    name.text.push('.');
                    name.text.push_str(&member.text);
                    name.span = name.span.through(member.span);
                }
            }
        }
        if allow_record
            && self.at(&TokenKind::LeftBrace)
            && (!stop_at_newline || !self.newline_before_current())
        {
            let record = self.tokens.get(self.position + 1).is_some_and(|token| {
                token.kind == TokenKind::RightBrace
                    || (matches!(token.kind, TokenKind::Ident(_))
                        && self
                            .tokens
                            .get(self.position + 2)
                            .is_some_and(|next| next.kind == TokenKind::Colon))
            });
            return if record {
                self.record(name)
            } else {
                self.computation(name)
            };
        }
        let span = name.span;
        self.make(ExprKind::Name(name), span, 1)
    }

    fn type_function_expression(&mut self) -> Result<Expr, Diagnostic> {
        let variable = self.type_variable()?;
        self.expect(
            &TokenKind::Dot,
            "'.' before a type-constrained function name",
        )?;
        let function = self.ident()?;
        let span = variable.span.through(function.span);
        self.make(
            ExprKind::TypeFunction(Box::new(variable), Box::new(function)),
            span,
            1,
        )
    }

    fn lambda(&mut self, name: Ident, stop_at_newline: bool) -> Result<Expr, Diagnostic> {
        let outer = self.in_task;
        self.in_task = false;
        let body = if self.implicit_computation_ahead(Some(self.column(self.current().span))) {
            self.body_expression()?
        } else {
            self.expression_inner(0, true, stop_at_newline)?
        };
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
            TokenKind::Char(value) => ExprKind::Char(*value),
            TokenKind::Utf8Char(value) => ExprKind::Utf8Char(*value),
            _ => unreachable!("literal token checked"),
        };
        self.make(kind, token.span, 1)
    }

    /// Symbol prefixes. At the head of an expression the operand may be an application
    /// (`*f x` is `*(f x)`); a prefix argument (`f *r y`) takes only a `term`.
    fn prefix(
        &mut self,
        allow_record: bool,
        stop_at_newline: bool,
        term: bool,
    ) -> Result<Expr, Diagnostic> {
        let token = self.take();
        let mutable = token.kind == TokenKind::Ampersand && self.eat(&TokenKind::Mut);
        let shared = token.kind == TokenKind::Ampersand && !mutable;
        let outer_slice = std::mem::replace(&mut self.slice_context, shared);
        let value = if term {
            self.term(allow_record, stop_at_newline)?
        } else {
            self.expression_inner(13, allow_record, stop_at_newline)?
        };
        self.slice_context = outer_slice;
        if shared && matches!(value.kind, ExprKind::Slice { .. }) {
            return Ok(Expr {
                span: token.span.through(value.span),
                ..value
            });
        }
        let span = token.span.through(value.span);
        let depth = value.depth + 1;
        let value = Box::new(value);
        let kind = match token.kind {
            TokenKind::Ampersand => ExprKind::Borrow(value, mutable, Notation::Symbol),
            TokenKind::Star => ExprKind::Dereference(value, Notation::Symbol),
            TokenKind::Minus => ExprKind::Unary(UnaryOp::Negate, value),
            TokenKind::Bang => ExprKind::Unary(UnaryOp::Not, value),
            TokenKind::Tilde => ExprKind::Unary(UnaryOp::BitNot, value),
            _ => unreachable!("prefix token checked"),
        };
        self.make(kind, span, depth)
    }

    /// `ref x`, `ref mut x`, and `deref r` take one `term`, in head and argument positions alike.
    fn keyword_prefix(
        &mut self,
        allow_record: bool,
        stop_at_newline: bool,
    ) -> Result<Expr, Diagnostic> {
        let token = self.take();
        let mutable = token.kind == TokenKind::Ref && self.eat(&TokenKind::Mut);
        let shared = token.kind == TokenKind::Ref && !mutable;
        let outer_slice = std::mem::replace(&mut self.slice_context, shared);
        let value = self.term(allow_record, stop_at_newline)?;
        self.slice_context = outer_slice;
        if self.space_argument() {
            let keyword = if token.kind == TokenKind::Ref {
                "ref"
            } else {
                "deref"
            };
            let text = &self.source[token.span.start..value.span.end];
            let form = if text.len() <= 60 && !text.contains(['\n', '\r']) {
                text.to_owned()
            } else {
                format!("{keyword} ...")
            };
            return Err(self.error(format!(
                "'{keyword}' takes exactly one operand; write '({form})' when more arguments follow, or parenthesize an application operand"
            )));
        }
        let span = token.span.through(value.span);
        if shared && matches!(value.kind, ExprKind::Slice { .. }) {
            return Ok(Expr { span, ..value });
        }
        let depth = value.depth + 1;
        let value = Box::new(value);
        let kind = if token.kind == TokenKind::Ref {
            ExprKind::Borrow(value, mutable, Notation::Keyword)
        } else {
            ExprKind::Dereference(value, Notation::Keyword)
        };
        self.make(kind, span, depth)
    }

    /// The operand of a prefix argument or keyword form: nested prefix forms, or one primary
    /// expression with its `.field`, `[index]`, and adjacent `(...)` suffixes.
    fn term(&mut self, allow_record: bool, stop_at_newline: bool) -> Result<Expr, Diagnostic> {
        let keyword = matches!(self.current().kind, TokenKind::Ref | TokenKind::Deref);
        if !keyword
            && !matches!(
                self.current().kind,
                TokenKind::Ampersand
                    | TokenKind::Star
                    | TokenKind::Minus
                    | TokenKind::Bang
                    | TokenKind::Tilde
            )
        {
            // Ordinary arguments keep their previous parse and nesting accounting.
            return self.expression_inner(15, allow_record, stop_at_newline);
        }
        self.enter()?;
        let value = if keyword {
            self.keyword_prefix(allow_record, stop_at_newline)?
        } else {
            self.prefix(allow_record, stop_at_newline, true)?
        };
        self.nesting -= 1;
        Ok(value)
    }

    fn conditional(&mut self) -> Result<Expr, Diagnostic> {
        let start = self.take().span;
        let condition = Box::new(self.expression(0, false)?);
        let keyword = self.eat(&TokenKind::Then);
        let then_branch = Box::new(self.conditional_body(keyword)?);
        let else_branch = Box::new(self.conditional_else(keyword, start, then_branch.span)?);
        let span = start.through(else_branch.span);
        let depth = condition
            .depth
            .max(then_branch.depth)
            .max(else_branch.depth)
            + 1;
        self.make(
            ExprKind::If {
                condition,
                then_branch,
                else_branch,
            },
            span,
            depth,
        )
    }

    fn conditional_body(&mut self, keyword: bool) -> Result<Expr, Diagnostic> {
        if keyword {
            self.body_expression()
        } else {
            self.block()
        }
    }

    fn conditional_else(
        &mut self,
        keyword: bool,
        start: Span,
        then_span: Span,
    ) -> Result<Expr, Diagnostic> {
        let has_else = (self.at(&TokenKind::Else) || self.at(&TokenKind::Elif))
            && (!keyword
                || !self.newline_before_current()
                || self.column(self.current().span) >= self.column(start));
        if !has_else {
            return self.make(ExprKind::Unit, then_span, 1);
        }
        if self.at(&TokenKind::Else) {
            self.take();
        }
        if self.at(&TokenKind::Elif) || (self.at(&TokenKind::If) && !self.newline_before_current())
        {
            self.enter()?;
            let branch = self.conditional()?;
            self.nesting -= 1;
            Ok(branch)
        } else {
            self.conditional_body(keyword)
        }
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
    fn keeps_following_statements_in_a_multiline_else_block() {
        let program = parse("def choose :: unit\nfn choose =\n    if true then ()\n    else\n        if false then ()\n        ()").unwrap();
        let ExprKind::Block { result, .. } = &program.functions[0].body.kind else {
            panic!("function layout block")
        };
        let ExprKind::If { else_branch, .. } = &result.kind else {
            panic!("outer conditional")
        };
        let ExprKind::Block { bindings, result } = &else_branch.kind else {
            panic!("multiline else is a block")
        };
        assert_eq!(bindings.len(), 1);
        assert!(matches!(result.kind, ExprKind::Unit));
    }

    #[test]
    fn parses_only_borrowed_partial_array_slices() {
        for prefix in ["&", "ref "] {
            for range in ["1..3", "1..", "..3", "1 + 2..3 + 4"] {
                let program = parse(&format!("{prefix}values[{range}]")).unwrap();
                let debug = format!("{program:?}");
                assert!(debug.contains("Slice"));
                assert!(!debug.contains("Borrow("));
            }
        }
        for source in [
            "values[0..1]",
            "values[..1]",
            "values[1..]",
            "&values[..]",
            "ref mut values[0..1]",
            "&mut values[0..1]",
            "ref identity(values[0..1])",
        ] {
            assert_eq!(parse(source).unwrap_err().code, "E0002", "{source}");
        }
        parse("values[1 + 2]").unwrap();
    }

    #[test]
    fn distinguishes_record_updates_from_blocks_and_matches() {
        for source in [
            "{ value with x = 1; y: 2, }",
            "{ (match flag with | true -> left | false -> right) with x = 1 }",
            "def update :: R -> R\nfn update value = { value with x = 1 }",
        ] {
            let program = parse(source).unwrap();
            assert!(format!("{program:?}").contains("RecordUpdate"), "{source}");
        }
        for source in [
            "{ match value with | { x = field } -> field }",
            "{ (); let value = 1; value }",
            "{ let value = 1; value }",
        ] {
            let program = parse(source).unwrap();
            assert!(!format!("{program:?}").contains("RecordUpdate"), "{source}");
        }
        assert!(parse("{ value with }").is_err());
        assert!(parse("{ value with x }").is_err());
    }

    #[test]
    fn parses_transparent_type_alias_declarations() {
        let program =
            parse("type Meters = f64\nprivate type Pair2<'a> = Pair<'a, 'a>\n42").unwrap();
        assert_eq!(program.type_aliases.len(), 2);
        assert_eq!(program.type_aliases[0].name.text, "Meters");
        assert_eq!(program.type_aliases[1].visibility, Visibility::Private);
        assert_eq!(program.type_aliases[1].parameters[0].text, "a");
        assert!(matches!(
            program.type_aliases[1].target.kind,
            TypeExprKind::Apply(..)
        ));
        assert!(program.entry.is_some());
        assert!(parse("let type = 1\ntype").is_err());
        assert!(parse("type Empty<> = i64").is_err());
        assert!(parse("type Bad =").is_err());
    }

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
    fn accepts_optional_else_but_requires_parameter_types() {
        assert!(parse("fn f() -> i64 { if true { 1 } }").is_ok());
        assert!(parse("fn f(x) -> i64 { x }").is_err());
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
            ("{ value with x = ", " }"),
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
