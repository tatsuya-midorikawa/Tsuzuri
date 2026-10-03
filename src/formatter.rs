use std::collections::{BTreeMap, BTreeSet};

use crate::diagnostic::{Diagnostic, Span};
use crate::syntax::*;

#[derive(Debug)]
pub struct FormatResult {
    pub changed: bool,
    pub formatted: String,
}

pub fn format_source(
    _path: &str,
    source: &str,
    kind: SourceKind,
) -> Result<FormatResult, Diagnostic> {
    let mut before = crate::parser::parse_with_source(source, 0)?;
    before.source_kind = Some(kind);
    let (before, mut hints) = canonicalize(before);
    let formatted = print_tokens(source, &mut hints)?;
    let mut after = crate::parser::parse_with_source(&formatted, 0).map_err(|error| {
        Diagnostic::new(
            "E2000",
            format!(
                "formatter produced invalid syntax: {}; please report a bug",
                error.message
            ),
            Span::default(),
        )
    })?;
    after.source_kind = Some(kind);
    if before != ast_fingerprint(after) {
        return Err(Diagnostic::new(
            "E2000",
            "formatter would change program semantics; please report a bug",
            Span::default(),
        ));
    }
    Ok(FormatResult {
        changed: formatted != source,
        formatted,
    })
}

fn print_tokens(source: &str, hints: &mut Hints) -> Result<String, Diagnostic> {
    use crate::lexer::TriviaKind;
    let tokens = crate::lexer::lex_with_trivia(source)?;
    let generic = generic_tokens(&tokens, hints);
    let mut layout = Layout::new(&tokens);
    let crlf = source
        .as_bytes()
        .windows(2)
        .filter(|pair| *pair == b"\r\n")
        .count();
    let lf = source.bytes().filter(|byte| *byte == b'\n').count() - crlf;
    let newline = if crlf > lf { "\r\n" } else { "\n" };
    let mut output = if source.starts_with('\u{feff}') {
        "\u{feff}".to_owned()
    } else {
        String::new()
    };
    let mut pending = String::new();
    for (index, retained) in tokens.iter().enumerate() {
        let previous = index.checked_sub(1).map(|previous| &tokens[previous].token);
        let indent = layout.before(
            source,
            previous,
            &retained.token,
            tokens.get(index + 1).map(|next| &next.token),
            hints,
        );
        let mut comments = false;
        for trivia in &retained.leading {
            match trivia.kind {
                TriviaKind::Whitespace => pending.push_str(&trivia.text),
                TriviaKind::Newline => {
                    pending.truncate(pending.trim_end_matches([' ', '\t', '\x0b', '\x0c']).len());
                    pending.push_str(newline);
                }
                TriviaKind::LineComment | TriviaKind::BlockComment => {
                    comments = true;
                    if let Some(original) = line_indent(source, trivia.span.start) {
                        set_indent(&mut pending, layout.comment_indent(original));
                    }
                    output.push_str(&pending);
                    pending.clear();
                    output.push_str(&trivia.text);
                }
            }
        }
        if retained.token.kind != TokenKind::End {
            if let Some(indent) = indent {
                set_indent(&mut pending, indent);
            }
            if index > 0 && !comments && !pending.contains(['\r', '\n']) && indent.is_none() {
                pending = spacing(
                    &tokens[index - 1].token,
                    &retained.token,
                    generic[index - 1],
                    generic[index],
                    hints,
                )
                .to_owned();
            }
            output.push_str(&pending);
            pending.clear();
            layout.token(
                source,
                &retained.token,
                column(&output, output.len()),
                output
                    .rsplit(['\r', '\n'])
                    .next()
                    .unwrap_or("")
                    .bytes()
                    .take_while(|byte| *byte == b' ')
                    .count(),
                hints,
            );
            output.push_str(&source[retained.token.span.start..retained.token.span.end]);
        }
    }
    output.push_str(newline);
    Ok(output)
}

pub fn ast_fingerprint(program: Program) -> String {
    canonicalize(program).0
}

fn canonicalize(mut program: Program) -> (String, Hints) {
    let mut canonical = Canonical::default();
    for external in &mut program.externs {
        Canonical::doc(&mut external.doc);
        canonical.ident(&mut external.name);
        for parameter in &mut external.parameters {
            canonical.ty(parameter);
        }
        canonical.ty(&mut external.result);
        canonical.constraints(&mut external.constraints);
        if let Some(link) = &mut external.link {
            link.span = Span::default();
            if let Some((_, span)) = &mut link.module {
                *span = Span::default();
            }
        }
    }
    for handle in &mut program.extern_types {
        Canonical::doc(&mut handle.doc);
        canonical.ident(&mut handle.name);
    }
    for constant in &mut program.constants {
        Canonical::doc(&mut constant.doc);
        canonical.ident(&mut constant.name);
        canonical.ty(&mut constant.ty);
        canonical.expression(&mut constant.value);
    }
    for alias in &mut program.type_aliases {
        Canonical::doc(&mut alias.doc);
        canonical.hints.type_headers.insert(alias.name.span.end);
        canonical.ident(&mut alias.name);
        for parameter in &mut alias.parameters {
            canonical.ident(parameter);
        }
        canonical.ty(&mut alias.target);
    }
    for record in &mut program.records {
        Canonical::doc(&mut record.doc);
        for region in &mut record.regions {
            canonical.ident(region);
        }
        for (_, span) in &mut record.derives {
            *span = Span::default();
        }
        canonical.hints.type_headers.insert(record.name.span.end);
        canonical.ident(&mut record.name);
        for parameter in &mut record.parameters {
            canonical.ident(parameter);
        }
        for field in &mut record.fields {
            canonical.parameter(field);
        }
    }
    for union in &mut program.unions {
        Canonical::doc(&mut union.doc);
        for (_, span) in &mut union.derives {
            *span = Span::default();
        }
        canonical.hints.type_headers.insert(union.name.span.end);
        canonical.ident(&mut union.name);
        for parameter in &mut union.parameters {
            canonical.ident(parameter);
        }
        for case in &mut union.cases {
            canonical.ident(&mut case.name);
            if let Some(payload) = &mut case.payload {
                canonical.ty(payload);
            }
        }
    }
    for function in &mut program.functions {
        Canonical::doc(&mut function.doc);
        for region in &mut function.regions {
            canonical.ident(region);
        }
        canonical.ident(&mut function.name);
        for parameter in &mut function.parameters {
            canonical.parameter(parameter);
        }
        canonical.ty(&mut function.result);
        canonical.constraints(&mut function.constraints);
        canonical.expression(&mut function.body);
    }
    for class in &mut program.classes {
        Canonical::doc(&mut class.doc);
        canonical.hints.type_headers.insert(class.name.span.end);
        canonical.ident(&mut class.name);
        canonical.ident(&mut class.variable);
        canonical.constraints(&mut class.superclasses);
        for method in &mut class.methods {
            Canonical::doc(&mut method.doc);
            canonical.ident(&mut method.name);
            for parameter in &mut method.parameters {
                canonical.ty(parameter);
            }
            canonical.ty(&mut method.result);
            canonical.constraints(&mut method.constraints);
        }
        for method in &mut class.defaults {
            canonical.ident(&mut method.name);
            for (parameter, _) in &mut method.parameters {
                canonical.ident(parameter);
            }
            canonical.expression(&mut method.body);
        }
    }
    for instance in &mut program.instances {
        canonical.hints.type_headers.insert(instance.class.span.end);
        canonical.ident(&mut instance.class);
        canonical.ty(&mut instance.ty);
        canonical.constraints(&mut instance.constraints);
        for method in &mut instance.methods {
            canonical.ident(&mut method.name);
            for (parameter, _) in &mut method.parameters {
                canonical.ident(parameter);
            }
            canonical.expression(&mut method.body);
        }
    }
    for pattern in &mut program.active_patterns {
        for case in &mut pattern.cases {
            canonical.ident(case);
        }
        if let Some(name) = canonical.generated.get(&pattern.function) {
            pattern.function = name.clone();
        }
    }
    for test in &mut program.tests {
        test.span = Span::default();
        test.name_span = Span::default();
        canonical.expression(&mut test.body);
    }
    if let Some(entry) = &mut program.entry {
        canonical.expression(entry);
    }
    (format!("{program:?}"), canonical.hints)
}

#[derive(Default)]
struct Hints {
    prefixes: BTreeSet<usize>,
    type_headers: BTreeSet<usize>,
    types: Vec<Span>,
    ends: BTreeMap<usize, usize>,
    heads: BTreeMap<usize, usize>,
    else_owners: BTreeMap<usize, (usize, usize)>,
    arm_owners: BTreeMap<usize, usize>,
    body_owners: BTreeMap<usize, usize>,
}

fn column(source: &str, offset: usize) -> usize {
    offset
        - source[..offset]
            .rfind(['\r', '\n'])
            .map_or(0, |index| index + 1)
}

fn line_indent(source: &str, offset: usize) -> Option<usize> {
    let start = offset - column(source, offset);
    let prefix = source[start..offset]
        .strip_prefix('\u{feff}')
        .unwrap_or(&source[start..offset]);
    prefix
        .bytes()
        .all(|byte| matches!(byte, b' ' | b'\t' | b'\x0b' | b'\x0c'))
        .then_some(prefix.len())
}

fn set_indent(pending: &mut String, indent: usize) {
    pending.truncate(pending.rfind(['\r', '\n']).map_or(0, |index| index + 1));
    pending.extend(std::iter::repeat_n(' ', indent));
}

struct Indent {
    original: usize,
    formatted: usize,
    end: usize,
}

struct Delimiter {
    indent: usize,
    levels: usize,
    end: usize,
}

struct Layout {
    levels: Vec<Indent>,
    delimiters: Vec<Delimiter>,
    closing: BTreeMap<usize, usize>,
    anchors: BTreeMap<usize, (usize, usize)>,
    previous_indent: usize,
    type_index: usize,
    type_end: usize,
}

impl Layout {
    fn new(tokens: &[crate::lexer::TokenWithTrivia]) -> Self {
        let mut opening = Vec::new();
        let mut closing = BTreeMap::new();
        for retained in tokens {
            match retained.token.kind {
                TokenKind::LeftParen
                | TokenKind::LeftBracket
                | TokenKind::LeftList
                | TokenKind::LeftBrace => opening.push(retained.token.span.start),
                TokenKind::RightParen
                | TokenKind::RightBracket
                | TokenKind::RightList
                | TokenKind::RightBrace => {
                    if let Some(start) = opening.pop() {
                        closing.insert(start, retained.token.span.end);
                    }
                }
                _ => {}
            }
        }
        Self {
            levels: vec![Indent {
                original: 0,
                formatted: 0,
                end: usize::MAX,
            }],
            delimiters: Vec::new(),
            closing,
            anchors: BTreeMap::new(),
            previous_indent: 0,
            type_index: 0,
            type_end: 0,
        }
    }

    fn before(
        &mut self,
        source: &str,
        previous: Option<&Token>,
        token: &Token,
        next: Option<&Token>,
        hints: &Hints,
    ) -> Option<usize> {
        let original = line_indent(source, token.span.start)?;
        while self.levels.len() > 1 && self.levels.last().unwrap().end <= token.span.start {
            self.levels.pop();
        }
        if matches!(
            token.kind,
            TokenKind::RightParen
                | TokenKind::RightBracket
                | TokenKind::RightList
                | TokenKind::RightBrace
        ) {
            if let Some(delimiter) = self.delimiters.last() {
                self.levels.truncate(delimiter.levels);
                return Some(delimiter.indent);
            }
        }
        while let Some(span) = hints
            .types
            .get(self.type_index)
            .filter(|span| span.start <= token.span.start)
        {
            self.type_end = self.type_end.max(span.end);
            self.type_index += 1;
        }
        let in_type = token.span.start < self.type_end;
        if self.delimiters.is_empty()
            && !in_type
            && matches!(
                token.kind,
                TokenKind::Def
                    | TokenKind::Fn
                    | TokenKind::Record
                    | TokenKind::Union
                    | TokenKind::Type
                    | TokenKind::Const
                    | TokenKind::Test
                    | TokenKind::Extern
                    | TokenKind::Class
                    | TokenKind::Instance
                    | TokenKind::Private
                    | TokenKind::Export
                    | TokenKind::And
            )
        {
            self.levels.truncate(1);
            return Some(0);
        }
        let owner = if matches!(token.kind, TokenKind::Else | TokenKind::Elif) {
            hints
                .else_owners
                .range(..=token.span.start)
                .next_back()
                .and_then(|(_, (end, owner))| (token.span.start <= *end).then_some(*owner))
        } else if token.kind == TokenKind::Pipe {
            next.and_then(|next| hints.arm_owners.get(&next.span.start).copied())
                .filter(|owner| {
                    self.anchors.get(owner).is_some_and(|(_, original_column)| {
                        *original_column == column(source, token.span.start)
                    })
                })
        } else {
            None
        };
        let forced = owner
            .and_then(|owner| {
                self.anchors
                    .get(&owner)
                    .map(|(indent, _)| (*indent, hints.ends[&owner]))
            })
            .or_else(|| {
                hints
                    .body_owners
                    .get(&token.span.start)
                    .and_then(|owner| self.anchors.get(owner))
                    .map(|(indent, _)| (*indent + 4, hints.ends[&token.span.start]))
            })
            .or_else(|| {
                previous
                    .filter(|previous| {
                        matches!(
                            previous.kind,
                            TokenKind::Equal
                                | TokenKind::Then
                                | TokenKind::Do
                                | TokenKind::Arrow
                                | TokenKind::Else
                                | TokenKind::LeftBrace
                                | TokenKind::LeftParen
                                | TokenKind::LeftBracket
                                | TokenKind::LeftList
                        )
                    })
                    .map(|_| {
                        (
                            self.previous_indent + 4,
                            hints
                                .ends
                                .get(&token.span.start)
                                .copied()
                                .or_else(|| self.delimiters.last().map(|delimiter| delimiter.end))
                                .unwrap_or(usize::MAX),
                        )
                    })
            });
        while self.levels.len() > 1 && self.levels.last().unwrap().original > original {
            self.levels.pop();
        }
        if let Some((formatted, end)) = forced {
            self.levels.push(Indent {
                original,
                formatted,
                end,
            });
            return Some(formatted);
        }
        let previous = self.levels.last().unwrap();
        if original > previous.original {
            self.levels.push(Indent {
                original,
                formatted: previous.formatted + 4,
                end: previous.end,
            });
        }
        Some(self.levels.last().unwrap().formatted)
    }

    fn comment_indent(&self, original: usize) -> usize {
        let base = self
            .levels
            .iter()
            .rev()
            .find(|level| level.original <= original)
            .unwrap();
        base.formatted + if original > base.original { 4 } else { 0 }
    }

    fn token(
        &mut self,
        source: &str,
        token: &Token,
        formatted_column: usize,
        indent: usize,
        hints: &Hints,
    ) {
        if matches!(
            token.kind,
            TokenKind::If | TokenKind::Elif | TokenKind::Match
        ) {
            if let Some((start, end)) = hints.heads.range(..=token.span.start).next_back() {
                if token.span.start < *end {
                    self.anchors
                        .insert(*start, (formatted_column, column(source, token.span.start)));
                }
            }
        }
        match token.kind {
            TokenKind::LeftParen
            | TokenKind::LeftBracket
            | TokenKind::LeftList
            | TokenKind::LeftBrace => self.delimiters.push(Delimiter {
                indent,
                levels: self.levels.len(),
                end: self.closing[&token.span.start],
            }),
            TokenKind::RightParen
            | TokenKind::RightBracket
            | TokenKind::RightList
            | TokenKind::RightBrace => {
                if let Some(delimiter) = self.delimiters.pop() {
                    self.levels.truncate(delimiter.levels);
                }
            }
            _ => {}
        }
        self.previous_indent = indent;
    }
}

fn generic_tokens(tokens: &[crate::lexer::TokenWithTrivia], hints: &mut Hints) -> Vec<bool> {
    hints.types.sort_by_key(|span| span.start);
    let mut types = hints.types.iter().peekable();
    let mut type_end = 0;
    let mut depth = 0usize;
    tokens
        .iter()
        .enumerate()
        .map(|(index, retained)| {
            let token = &retained.token;
            while types
                .peek()
                .is_some_and(|span| span.start <= token.span.start)
            {
                type_end = type_end.max(types.next().unwrap().end);
            }
            if token.kind == TokenKind::Less
                && (token.span.start < type_end
                    || index > 0
                        && hints
                            .type_headers
                            .contains(&tokens[index - 1].token.span.end))
            {
                depth += 1;
                true
            } else if depth > 0 {
                let closing = match token.kind {
                    TokenKind::Greater | TokenKind::GreaterEqual => 1,
                    TokenKind::ShiftRight => 2,
                    TokenKind::ShiftRightUnsigned => 3,
                    _ => 0,
                };
                depth = depth.saturating_sub(closing);
                closing != 0
            } else {
                false
            }
        })
        .collect()
}

fn spacing(
    previous: &Token,
    current: &Token,
    previous_generic: bool,
    current_generic: bool,
    hints: &Hints,
) -> &'static str {
    use TokenKind::*;
    let separated = if previous.span.end < current.span.start {
        " "
    } else {
        ""
    };
    if matches!(current.kind, Dot) || matches!(previous.kind, Dot) {
        return separated;
    }
    // A hole's expression sits directly between the braces of `$"a{x}b"`.
    if matches!(current.kind, InterpolationMiddle(_) | InterpolationEnd(_))
        || matches!(
            previous.kind,
            InterpolationStart(_) | InterpolationMiddle(_)
        )
    {
        return "";
    }
    if matches!(
        current.kind,
        Comma | Semicolon | Colon | RightParen | RightBracket | RightList
    ) || matches!(
        previous.kind,
        LeftParen | LeftBracket | LeftList | Backslash
    ) {
        return "";
    }
    if current.kind == RightBrace {
        return if previous.kind == LeftBrace { "" } else { " " };
    }
    if matches!(current.kind, LeftParen | LeftBracket | LeftList)
        && matches!(
            previous.kind,
            Ident(_)
                | Integer(_)
                | Float(_)
                | String(_)
                | InterpolationEnd(_)
                | Char(_)
                | Utf8Char(_)
                | True
                | False
                | RightParen
                | RightBracket
                | RightList
                | RightBrace
        )
    {
        return separated;
    }
    if current.kind == Bang
        && (matches!(previous.kind, Let | Do | Return | Yield)
            || matches!(&previous.kind, Ident(name) if name == "use"))
    {
        return "";
    }
    if hints.prefixes.contains(&previous.span.start)
        && matches!(previous.kind, Minus | Ampersand | Star | Bang | Tilde)
    {
        return if previous.kind == Minus {
            separated
        } else if current.kind == Mut {
            " "
        } else {
            ""
        };
    }
    if current_generic || previous_generic && previous.kind == Less {
        return "";
    }
    if matches!(previous.kind, At | Hash) {
        return separated;
    }
    " "
}

#[derive(Default)]
struct Canonical {
    generated: BTreeMap<String, String>,
    hints: Hints,
}

impl Canonical {
    fn doc(doc: &mut Option<Documentation>) {
        if let Some(doc) = doc {
            doc.span = Span::default();
        }
    }

    fn ident(&mut self, name: &mut Ident) {
        name.span = Span::default();
        if name.provenance == Provenance::Generated {
            let next = self.generated.len();
            name.text = self
                .generated
                .entry(name.text.clone())
                .or_insert_with(|| format!("$generated{next}"))
                .clone();
        }
    }

    fn parameter(&mut self, parameter: &mut Parameter) {
        self.ident(&mut parameter.name);
        self.ty(&mut parameter.ty);
    }

    fn constraints(&mut self, constraints: &mut [ConstraintExpr]) {
        for constraint in constraints {
            match &mut constraint.name {
                ConstraintName::Class(name) | ConstraintName::Function(name) => {
                    self.hints.type_headers.insert(name.span.end);
                    self.ident(name);
                }
            }
            self.ty(&mut constraint.ty);
        }
    }

    fn ty(&mut self, ty: &mut TypeExpr) {
        self.hints.types.push(ty.span);
        if matches!(ty.kind, TypeExprKind::Reference(..)) {
            self.hints.prefixes.insert(ty.span.start);
        }
        ty.span = Span::default();
        match &mut ty.kind {
            TypeExprKind::Regions(inner, regions) => {
                self.ty(inner);
                for region in regions {
                    self.ident(region);
                }
            }
            TypeExprKind::Apply(name, arguments) => {
                self.ident(name);
                for argument in arguments {
                    self.ty(argument);
                }
            }
            TypeExprKind::Array(inner)
            | TypeExprKind::List(inner)
            | TypeExprKind::Task(inner)
            | TypeExprKind::Reference(inner, _) => self.ty(inner),
            TypeExprKind::Tuple(elements) => {
                for element in elements {
                    self.ty(element);
                }
            }
            TypeExprKind::Function(parameters, result) => {
                for parameter in parameters {
                    self.ty(parameter);
                }
                self.ty(result);
            }
            TypeExprKind::Named(_) | TypeExprKind::Variable(_) => {}
        }
    }

    fn expression(&mut self, expression: &mut Expr) {
        self.hints
            .ends
            .entry(expression.span.start)
            .and_modify(|end| *end = (*end).max(expression.span.end))
            .or_insert(expression.span.end);
        if let ExprKind::If {
            condition,
            then_branch,
            else_branch,
        } = &expression.kind
        {
            self.hints
                .heads
                .insert(expression.span.start, condition.span.start);
            self.hints.else_owners.insert(
                then_branch.span.end,
                (else_branch.span.start, expression.span.start),
            );
            self.hints
                .body_owners
                .insert(then_branch.span.start, expression.span.start);
            self.hints
                .body_owners
                .insert(else_branch.span.start, expression.span.start);
        }
        if let ExprKind::Match { value, arms, .. } = &expression.kind {
            self.hints
                .heads
                .insert(expression.span.start, value.span.start);
            for arm in arms {
                self.hints
                    .arm_owners
                    .insert(arm.pattern.span.start, expression.span.start);
            }
        }
        if matches!(
            expression.kind,
            ExprKind::Unary(..) | ExprKind::Borrow(..) | ExprKind::Dereference(..)
        ) {
            self.hints.prefixes.insert(expression.span.start);
        }
        expression.span = Span::default();
        expression.depth = 0;
        use ExprKind::*;
        match &mut expression.kind {
            Name(name) | QualifiedFunction(name) => self.ident(name),
            TypeFunction(receiver, member) => {
                self.ident(receiver);
                self.ident(member);
            }
            Block { bindings, result } => {
                for binding in bindings {
                    self.binding(binding);
                }
                self.expression(result);
            }
            NewArray(ty, length, initialize) | NewList(ty, length, initialize) => {
                self.ty(ty);
                self.expression(length);
                self.expression(initialize);
            }
            Cast(value, ty) => {
                self.ty(ty);
                self.expression(value);
            }
            For {
                pattern,
                source,
                body,
            } => {
                self.pattern(pattern);
                self.expression(source);
                self.expression(body);
            }
            Match { value, arms, .. } => {
                self.expression(value);
                for arm in arms {
                    arm.span = Span::default();
                    self.pattern(&mut arm.pattern);
                    if let Some(guard) = &mut arm.guard {
                        self.expression(guard);
                    }
                    self.expression(&mut arm.body);
                }
            }
            Record { name, fields } => {
                self.ident(name);
                for (name, value) in fields {
                    self.ident(name);
                    self.expression(value);
                }
            }
            RecordUpdate { base, fields } => {
                self.expression(base);
                for (name, value) in fields {
                    self.ident(name);
                    self.expression(value);
                }
            }
            Computation(name, body) => {
                self.ident(name);
                self.computation(body);
            }
            Lambda(parameters, body) => {
                for (name, _) in parameters {
                    self.ident(name);
                }
                self.expression(body);
            }
            Field(value, name) => {
                self.expression(value);
                self.ident(name);
            }
            Unary(_, value)
            | Task(value)
            | TaskRun(value)
            | ComputationBoundary(value)
            | NewLiteral(value)
            | Borrow(value, ..)
            | Dereference(value, _) => self.expression(value),
            Binary(_, left, right)
            | Index(left, right)
            | Assign(left, right)
            | While {
                condition: left,
                body: right,
            } => {
                self.expression(left);
                self.expression(right);
            }
            Call(callee, arguments) => {
                self.expression(callee);
                for argument in arguments {
                    self.expression(argument);
                }
            }
            If {
                condition,
                then_branch,
                else_branch,
            } => {
                self.expression(condition);
                self.expression(then_branch);
                self.expression(else_branch);
            }
            Range {
                start,
                step,
                finish,
                ..
            } => {
                self.expression(start);
                if let Some(step) = step {
                    self.expression(step);
                }
                self.expression(finish);
            }
            Slice { value, start, end } => {
                self.expression(value);
                for bound in start.iter_mut().chain(end.iter_mut()) {
                    self.expression(bound);
                }
            }
            Array(values) | List(values) | Tuple(values) => {
                for value in values {
                    self.expression(value);
                }
            }
            Interpolated(interpolation) => {
                for hole in &mut interpolation.holes {
                    if let Some(spec) = &mut hole.spec {
                        spec.span = Span::default();
                    }
                    self.expression(&mut hole.value);
                }
            }
            Integer(..) | Float(..) | String(_) | Char(_) | Utf8Char(_) | Bool(_) | Unit
            | Break | Continue => {}
        }
    }

    fn pattern(&mut self, pattern: &mut Pattern) {
        pattern.span = Span::default();
        pattern.depth = 0;
        use PatternKind::*;
        match &mut pattern.kind {
            Binding(name) => self.ident(name),
            Annotated(pattern, ty) => {
                self.ty(ty);
                self.pattern(pattern);
            }
            Record(name, fields) => {
                if let Some(name) = name {
                    self.ident(name);
                }
                for (name, field) in fields {
                    self.ident(name);
                    self.pattern(field);
                }
            }
            Apply(name, patterns) => {
                self.ident(name);
                for pattern in patterns {
                    self.pattern(pattern);
                }
            }
            Tuple(patterns) | Array(patterns) | List(patterns) => {
                for pattern in patterns {
                    self.pattern(pattern);
                }
            }
            Cons(left, right) | Or(left, right) | And(left, right) => {
                self.pattern(left);
                self.pattern(right);
            }
            As(pattern, name) => {
                self.pattern(pattern);
                self.ident(name);
            }
            Argument(value) | Literal(value) => self.expression(value),
            Wildcard => {}
        }
    }

    fn binding(&mut self, binding: &mut Binding) {
        self.ident(&mut binding.name);
        if let Some(ty) = &mut binding.annotation {
            self.ty(ty);
        }
        self.expression(&mut binding.value);
    }

    fn computation(&mut self, block: &mut ComputationBlock) {
        if let Some(first) = block.statements.first() {
            self.hints.ends.insert(first.span.start, block.span.end);
        }
        for statement in &block.statements {
            if let ComputationStatementKind::If(condition, yes, Some(no)) = &statement.kind {
                self.hints
                    .heads
                    .insert(statement.span.start, condition.span.start);
                self.hints
                    .ends
                    .insert(statement.span.start, statement.span.end);
                self.hints
                    .else_owners
                    .insert(yes.span.end, (no.span.start, statement.span.start));
            }
        }
        block.span = Span::default();
        block.depth = 0;
        use ComputationStatementKind::*;
        for statement in &mut block.statements {
            statement.span = Span::default();
            match &mut statement.kind {
                Match(value, arms) => {
                    self.expression(value);
                    for arm in arms {
                        self.pattern(&mut arm.pattern);
                        if let Some(guard) = &mut arm.guard {
                            self.expression(guard);
                        }
                        self.computation(&mut arm.body);
                        arm.span = Span::default();
                    }
                }
                Let(binding, _) => self.binding(binding),
                LetAnd(bindings) => {
                    for binding in bindings {
                        self.binding(binding);
                    }
                }
                Do(value) | Operation(_, value) | Expression(value) => self.expression(value),
                If(condition, yes, no) => {
                    self.expression(condition);
                    self.computation(yes);
                    if let Some(no) = no {
                        self.computation(no);
                    }
                }
                For(pattern, source, body) => {
                    self.pattern(pattern);
                    self.expression(source);
                    self.computation(body);
                }
                While(condition, body) => {
                    self.expression(condition);
                    self.computation(body);
                }
            }
        }
    }
}
