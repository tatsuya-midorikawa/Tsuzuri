use crate::diagnostic::{Diagnostic, MAX_UNIQUE_DIAGNOSTICS, Span};
use crate::syntax::{
    FormatAlign, FormatKind, FormatSpec, InterpolationPiece, MAX_FORMAT_FIELD, MAX_SOURCE_BYTES,
    StringLiteral, Token, TokenKind,
};

pub fn lex(source: &str) -> Result<Vec<Token>, Diagnostic> {
    let (tokens, diagnostics) = tokenize(source, false);
    diagnostics.into_iter().next().map_or(Ok(tokens), Err)
}

pub fn lex_all(source: &str) -> (Vec<Token>, Vec<Diagnostic>) {
    tokenize(source, true)
}

/// The length of the interpreter line that starts with `#!` at byte 0, as in
/// `#!/usr/bin/env tsuzuri script`, or 0. The lexer skips it like a line comment
/// and keeps the line break, so every later position stays the same. A `#` can
/// never begin a program, so no other program changes meaning.
pub fn shebang_length(source: &str) -> usize {
    if !source.starts_with("#!") {
        return 0;
    }
    let end = source.find('\n').unwrap_or(source.len());
    if source[..end].ends_with('\r') {
        end - 1
    } else {
        end
    }
}

/// Where the first token can start: after a byte order mark or a `#!` line.
fn content_start(source: &str) -> usize {
    if source.starts_with('\u{feff}') {
        3
    } else {
        shebang_length(source)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TriviaKind {
    Whitespace,
    Newline,
    LineComment,
    BlockComment,
}

#[derive(Clone, Debug)]
pub struct Trivia {
    pub kind: TriviaKind,
    pub text: String,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct TokenWithTrivia {
    pub token: Token,
    pub leading: Vec<Trivia>,
}

pub fn lex_with_trivia(source: &str) -> Result<Vec<TokenWithTrivia>, Diagnostic> {
    let tokens = lex(source)?;
    let mut scanner = Lexer {
        source,
        position: content_start(source),
        holes: Vec::new(),
    };
    let mut result = Vec::with_capacity(tokens.len());
    for token in tokens {
        let mut leading = Vec::new();
        while scanner.position < token.span.start {
            let start = scanner.position;
            let kind = if scanner.rest().starts_with("/*") {
                scanner.comment()?;
                TriviaKind::BlockComment
            } else if scanner.rest().starts_with("//") {
                scanner.position = source[start..]
                    .find('\n')
                    .map_or(source.len(), |offset| start + offset);
                if scanner.position < source.len()
                    && source.as_bytes()[scanner.position - 1] == b'\r'
                {
                    scanner.position -= 1;
                }
                TriviaKind::LineComment
            } else if scanner.rest().starts_with("\r\n") {
                scanner.position += 2;
                TriviaKind::Newline
            } else if scanner.rest().starts_with(['\r', '\n']) {
                scanner.position += 1;
                TriviaKind::Newline
            } else {
                while scanner.position < token.span.start
                    && source.as_bytes()[scanner.position].is_ascii_whitespace()
                    && !matches!(source.as_bytes()[scanner.position], b'\r' | b'\n')
                {
                    scanner.position += 1;
                }
                TriviaKind::Whitespace
            };
            leading.push(Trivia {
                kind,
                text: source[start..scanner.position].to_owned(),
                span: Span::new(start, scanner.position),
            });
        }
        scanner.position = token.span.end;
        result.push(TokenWithTrivia { token, leading });
    }
    Ok(result)
}

fn tokenize(source: &str, recovering: bool) -> (Vec<Token>, Vec<Diagnostic>) {
    if source.len() > MAX_SOURCE_BYTES {
        return (
            Vec::new(),
            vec![Diagnostic::new(
                "E0003",
                format!("source exceeds the {MAX_SOURCE_BYTES}-byte limit; split the program"),
                Span::default(),
            )],
        );
    }
    Lexer {
        source,
        position: content_start(source),
        holes: Vec::new(),
    }
    .tokens(recovering)
}

/// Turns the `::` of namespace paths such as `Sample::Shape.area` into
/// `PathSep`: a `::` without spaces between two identifiers, in a chain that
/// ends with a capitalized module name or follows `namespace` or `using` at
/// the start of a line. The `::` after a declaration's name still annotates
/// its type, and `head::tail` stays a list pattern.
fn mark_paths(tokens: &mut [Token], source: &str) {
    let segment = |token: &Token| matches!(&token.kind, TokenKind::Ident(name) if name != "_");
    let joins = |tokens: &[Token], at: usize| {
        tokens[at].kind == TokenKind::DoubleColon
            && segment(&tokens[at - 1])
            && tokens.get(at + 1).is_some_and(segment)
            && tokens[at - 1].span.end == tokens[at].span.start
            && tokens[at].span.end == tokens[at + 1].span.start
    };
    let mut at = 1;
    while at + 1 < tokens.len() {
        let declared = at >= 2
            && matches!(
                tokens[at - 2].kind,
                TokenKind::Def | TokenKind::Rec | TokenKind::And
            );
        if declared || !joins(tokens, at) {
            at += 1;
            continue;
        }
        let mut last = at;
        while last + 2 < tokens.len() && joins(tokens, last + 2) {
            last += 2;
        }
        let header = at >= 2
            && matches!(&tokens[at - 2].kind, TokenKind::Ident(word) if word == "namespace" || word == "using")
            && (at == 2
                || source
                    .get(tokens[at - 3].span.end..tokens[at - 2].span.start)
                    .is_some_and(|gap| gap.contains(['\n', '\r'])));
        let module = matches!(&tokens[last + 1].kind, TokenKind::Ident(name) if name.starts_with(|first: char| first.is_ascii_uppercase()));
        if header || module {
            for separator in (at..=last).step_by(2) {
                tokens[separator].kind = TokenKind::PathSep;
            }
        }
        at = last + 2;
    }
}

struct Lexer<'a> {
    source: &'a str,
    position: usize,
    /// Interpolation holes that are open, innermost last.
    holes: Vec<OpenHole>,
}

/// An open `{` of an interpolated string whose `}` has not been read.
struct OpenHole {
    utf8: bool,
    /// Open brackets inside the hole; the hole closes at a `}` or `:` with none.
    depth: u32,
    /// Where the hole's `{` is.
    start: usize,
}

/// What ended a text segment of an interpolated string.
enum TextEnd {
    Quote,
    Hole,
}

impl Lexer<'_> {
    fn tokens(mut self, recovering: bool) -> (Vec<Token>, Vec<Diagnostic>) {
        let mut tokens = Vec::new();
        let mut diagnostics = Vec::new();
        while self.position < self.source.len() {
            let start = self.position;
            let in_hole = !self.holes.is_empty();
            match self.token() {
                Ok(Some(token)) => tokens.push(token),
                Ok(None) => {}
                Err(error) => {
                    self.holes.clear();
                    diagnostics.push(error);
                    if !recovering || diagnostics.len() == MAX_UNIQUE_DIAGNOSTICS {
                        self.position = self.source.len();
                        break;
                    }
                    self.recover(start, in_hole);
                }
            }
        }
        if let Some(hole) = self.holes.last() {
            diagnostics.push(Diagnostic::new(
                "E0001",
                "unterminated interpolation hole; close it with '}'",
                Span::new(hole.start, self.position),
            ));
        }
        tokens.push(Token {
            kind: TokenKind::End,
            span: Span::new(self.position, self.position),
        });
        mark_paths(&mut tokens, self.source);
        (tokens, diagnostics)
    }

    fn recover(&mut self, start: usize, in_hole: bool) {
        let first = self.source[start..].chars().next().unwrap();
        self.position = self.position.max(start + first.len_utf8());
        if first == '"'
            || in_hole
            || ["u8\"", "$\"", "u8$\""]
                .iter()
                .any(|prefix| self.source[start..].starts_with(prefix))
        {
            if !matches!(
                self.source.as_bytes().get(self.position - 1),
                Some(b'\n' | b'\r')
            ) {
                while self.position < self.source.len() && !self.rest().starts_with(['\n', '\r']) {
                    self.position += self.rest().chars().next().unwrap().len_utf8();
                }
            }
        } else if first.is_ascii_digit() {
            while self
                .source
                .as_bytes()
                .get(self.position)
                .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_')
            {
                self.position += 1;
            }
        }
    }

    fn token(&mut self) -> Result<Option<Token>, Diagnostic> {
        let start = self.position;
        let byte = self.source.as_bytes()[start];
        if byte.is_ascii_whitespace() {
            if let Some(hole) = self.holes.last()
                && matches!(byte, b'\n' | b'\r')
            {
                return Err(Diagnostic::new(
                    "E0001",
                    "an interpolation hole must stay on one line; bind the value with 'let' first",
                    Span::new(hole.start, start + 1),
                ));
            }
            self.position += 1;
            return Ok(None);
        }
        if !self.holes.is_empty()
            && (self.rest().starts_with("//") || self.rest().starts_with("/*"))
        {
            return Err(Diagnostic::new(
                "E0001",
                "comments are not allowed inside an interpolation hole",
                Span::new(start, start + 2),
            ));
        }
        if self.rest().starts_with("///") {
            self.position += 3;
            if self.rest().starts_with(' ') {
                self.position += 1;
            }
            let text_start = self.position;
            while self.position < self.source.len()
                && !matches!(self.source.as_bytes()[self.position], b'\r' | b'\n')
            {
                self.position += 1;
            }
            return Ok(Some(Token {
                kind: TokenKind::DocComment(self.source[text_start..self.position].into()),
                span: Span::new(start, self.position),
            }));
        }
        if self.rest().starts_with("//") {
            while self.position < self.source.len()
                && self.source.as_bytes()[self.position] != b'\n'
            {
                self.position += 1;
            }
            return Ok(None);
        }
        if self.rest().starts_with("/*") {
            self.comment()?;
            return Ok(None);
        }
        if let Some(hole) = self.holes.last()
            && hole.depth == 0
            && (byte == b'}'
                || (byte == b':' && self.source.as_bytes().get(start + 1) != Some(&b':')))
        {
            let kind = self.hole_end(start)?;
            let kind = if matches!(kind, TokenKind::InterpolationEnd(_)) {
                self.no_byte_suffix(kind, start)?
            } else {
                kind
            };
            return Ok(Some(Token {
                kind,
                span: Span::new(start, self.position),
            }));
        }
        let kind = if byte == b'\'' {
            let kind = self.char_or_type_variable()?;
            self.byte_suffix(kind, start)?
        } else if self.rest().starts_with("u8'") {
            self.position += 2;
            let kind = self.character(true)?;
            self.byte_suffix(kind, start)?
        } else if self.rest().starts_with("u8\"") {
            self.position += 2;
            let kind = self.string(true)?;
            self.byte_suffix(kind, start)?
        } else if self.rest().starts_with("u8$\"") {
            self.position += 3;
            let kind = self.interpolated(true, start)?;
            self.no_byte_suffix(kind, start)?
        } else if self.rest().starts_with("$\"") {
            self.position += 1;
            let kind = self.interpolated(false, start)?;
            self.no_byte_suffix(kind, start)?
        } else if byte.is_ascii_alphabetic() || byte == b'_' {
            self.identifier()
        } else if byte.is_ascii_digit() {
            self.number()?
        } else if byte == b'"' {
            let kind = self.string(false)?;
            self.byte_suffix(kind, start)?
        } else {
            self.symbol()?
        };
        if let Some(hole) = self.holes.last_mut() {
            match kind {
                TokenKind::LeftParen
                | TokenKind::LeftBracket
                | TokenKind::LeftBrace
                | TokenKind::LeftList => hole.depth += 1,
                TokenKind::RightParen
                | TokenKind::RightBracket
                | TokenKind::RightBrace
                | TokenKind::RightList => hole.depth = hole.depth.saturating_sub(1),
                _ => {}
            }
        }
        Ok(Some(Token {
            kind,
            span: Span::new(start, self.position),
        }))
    }

    fn rest(&self) -> &str {
        &self.source[self.position..]
    }

    /// Whether a `B` suffix directly follows the literal that ends at the position.
    fn at_byte_suffix(&self) -> bool {
        let bytes = self.source.as_bytes();
        bytes.get(self.position) == Some(&b'B')
            && !bytes
                .get(self.position + 1)
                .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
    }

    /// Reads the `B` of `'a'B` (a byte), `"ascii"B` (bytes), and `u8"text"B` (scalars).
    fn byte_suffix(&mut self, kind: TokenKind, start: usize) -> Result<TokenKind, Diagnostic> {
        if !self.at_byte_suffix() {
            return Ok(kind);
        }
        self.position += 1;
        let error = |message: &str| {
            Diagnostic::new("E0001", message.to_owned(), Span::new(start, self.position))
        };
        let ascii = "a 'B' literal holds only ASCII characters (U+0000 to U+007F)";
        match kind {
            TokenKind::Char(unit) if unit < 0x80 => Ok(TokenKind::Integer(format!("{unit}i8u"))),
            TokenKind::String(StringLiteral::Utf16(units)) => {
                if units.iter().all(|unit| *unit < 0x80) {
                    Ok(TokenKind::ByteString(
                        units.iter().map(|unit| *unit as u8).collect(),
                    ))
                } else {
                    Err(error(ascii))
                }
            }
            TokenKind::String(StringLiteral::Utf8(text)) => Ok(TokenKind::ScalarString(
                text.chars().map(u32::from).collect(),
            )),
            TokenKind::Char(_) => Err(error(ascii)),
            _ => Err(error(
                "only 'c'B, \"text\"B, and u8\"text\"B take the 'B' suffix",
            )),
        }
    }

    fn no_byte_suffix(&mut self, kind: TokenKind, start: usize) -> Result<TokenKind, Diagnostic> {
        if self.at_byte_suffix() {
            self.position += 1;
            return Err(Diagnostic::new(
                "E0001",
                "an interpolated string does not take the 'B' suffix",
                Span::new(start, self.position),
            ));
        }
        Ok(kind)
    }

    fn char_or_type_variable(&mut self) -> Result<TokenKind, Diagnostic> {
        let start = self.position;
        if self.source.as_bytes().get(start + 1) == Some(&b'_')
            && self.source.as_bytes().get(start + 2) != Some(&b'\'')
        {
            self.position += 1;
            return Err(Diagnostic::new(
                "E0001",
                "a type variable starts with an apostrophe and an ASCII letter",
                Span::new(start, self.position),
            ));
        }
        if self
            .source
            .as_bytes()
            .get(start + 1)
            .is_some_and(u8::is_ascii_alphabetic)
        {
            self.position += 1;
            self.identifier();
            if !self.rest().starts_with('\'') {
                return Ok(TokenKind::TypeVariable(
                    self.source[start + 1..self.position].to_owned(),
                ));
            }
            if self.position - start != 2 {
                self.position += 1;
                return Err(Diagnostic::new(
                    "E0001",
                    "a char literal contains exactly one UTF-16 code unit",
                    Span::new(start, self.position),
                ));
            }
            self.position = start;
        }
        self.character(false)
    }

    fn character(&mut self, utf8: bool) -> Result<TokenKind, Diagnostic> {
        let start = self.position;
        self.position += 1;
        let error = |end| {
            Diagnostic::new(
                "E0001",
                if utf8 {
                    "a utf8char literal contains exactly one Unicode scalar"
                } else {
                    "a char literal contains exactly one UTF-16 code unit"
                },
                Span::new(start, end),
            )
        };
        let Some(character) = self.rest().chars().next() else {
            return Err(error(self.position));
        };
        self.position += character.len_utf8();
        let value = match character {
            '\'' | '\n' | '\r' => return Err(error(self.position)),
            '\\' => {
                let Some(escape) = self.rest().chars().next() else {
                    return Err(error(self.position));
                };
                self.position += escape.len_utf8();
                match escape {
                    '\'' => u32::from('\''),
                    '\\' => u32::from('\\'),
                    'n' => 10,
                    'r' => 13,
                    't' => 9,
                    '0' => 0,
                    'u' if !utf8 || self.rest().starts_with('{') => {
                        let before = self.position;
                        let value = self.unicode_escape(utf8)?;
                        if self.position - before > 8 {
                            return Err(error(self.position));
                        }
                        value
                    }
                    _ => return Err(error(self.position)),
                }
            }
            _ => u32::from(character),
        };
        if !self.rest().starts_with('\'')
            || (!utf8 && value > 0xffff)
            || (utf8 && char::from_u32(value).is_none())
        {
            return Err(error(self.position));
        }
        self.position += 1;
        Ok(if utf8 {
            TokenKind::Utf8Char(value)
        } else {
            TokenKind::Char(value as u16)
        })
    }

    fn comment(&mut self) -> Result<(), Diagnostic> {
        let start = self.position;
        self.position += 2;
        let mut depth = 1;
        while self.position < self.source.len() {
            if self.rest().starts_with("/*") {
                depth += 1;
                self.position += 2;
            } else if self.rest().starts_with("*/") {
                depth -= 1;
                self.position += 2;
                if depth == 0 {
                    return Ok(());
                }
            } else {
                self.position += self.rest().chars().next().unwrap().len_utf8();
            }
        }
        Err(Diagnostic::new(
            "E0001",
            "unterminated block comment",
            Span::new(start, self.position),
        ))
    }

    fn identifier(&mut self) -> TokenKind {
        let start = self.position;
        while self.position < self.source.len() {
            let byte = self.source.as_bytes()[self.position];
            if !byte.is_ascii_alphanumeric() && byte != b'_' {
                break;
            }
            self.position += 1;
        }
        match &self.source[start..self.position] {
            "fn" => TokenKind::Fn,
            "def" => TokenKind::Def,
            "rec" => TokenKind::Rec,
            "and" => TokenKind::And,
            "export" => TokenKind::Export,
            "extern" => TokenKind::Extern,
            "private" => TokenKind::Private,
            "record" => TokenKind::Record,
            "union" => TokenKind::Union,
            "type" => TokenKind::Type,
            "const" => TokenKind::Const,
            "test" => TokenKind::Test,
            "bench" => TokenKind::Bench,
            "class" => TokenKind::Class,
            "instance" => TokenKind::Instance,
            "deriving" => TokenKind::Deriving,
            "dyn" => TokenKind::Dyn,
            "let" => TokenKind::Let,
            "task" => TokenKind::Task,
            "do" => TokenKind::Do,
            "return" => TokenKind::Return,
            "yield" => TokenKind::Yield,
            "for" => TokenKind::For,
            "in" => TokenKind::In,
            "to" => TokenKind::To,
            "downto" => TokenKind::Downto,
            "while" => TokenKind::While,
            "break" => TokenKind::Break,
            "continue" => TokenKind::Continue,
            "mut" => TokenKind::Mut,
            "ref" => TokenKind::Ref,
            "deref" => TokenKind::Deref,
            "new" => TokenKind::New,
            "as" => TokenKind::As,
            "if" => TokenKind::If,
            "then" => TokenKind::Then,
            "elif" => TokenKind::Elif,
            "else" => TokenKind::Else,
            "match" => TokenKind::Match,
            "with" => TokenKind::With,
            "when" => TokenKind::When,
            "true" => TokenKind::True,
            "false" => TokenKind::False,
            name => TokenKind::Ident(name.to_owned()),
        }
    }

    fn digits(&mut self, radix: u32) -> Result<(), Diagnostic> {
        let start = self.position;
        let mut previous_digit = false;
        while self.position < self.source.len() {
            let byte = self.source.as_bytes()[self.position];
            if (byte as char).is_digit(radix) {
                previous_digit = true;
                self.position += 1;
            } else if byte == b'_' {
                let next_digit = self
                    .source
                    .as_bytes()
                    .get(self.position + 1)
                    .is_some_and(|next| (*next as char).is_digit(radix));
                if !previous_digit || !next_digit {
                    return Err(Diagnostic::new(
                        "E0001",
                        "numeric separators must occur between digits",
                        Span::new(self.position, self.position + 1),
                    ));
                }
                previous_digit = false;
                self.position += 1;
            } else {
                break;
            }
        }
        if self.position == start {
            return Err(Diagnostic::new(
                "E0001",
                "expected a digit",
                Span::new(start, start),
            ));
        }
        Ok(())
    }

    fn number(&mut self) -> Result<TokenKind, Diagnostic> {
        let start = self.position;
        let radix = if self.rest().starts_with("0x") {
            16
        } else if self.rest().starts_with("0b") {
            2
        } else {
            10
        };
        let mut float = false;
        if radix != 10 {
            self.position += 2;
            self.digits(radix)?;
        } else {
            self.digits(10)?;
            if self.source.as_bytes().get(self.position) == Some(&b'.')
                && self
                    .source
                    .as_bytes()
                    .get(self.position + 1)
                    .is_some_and(u8::is_ascii_digit)
            {
                float = true;
                self.position += 1;
                self.digits(10)?;
            }
            if matches!(self.source.as_bytes().get(self.position), Some(b'e' | b'E')) {
                float = true;
                self.position += 1;
                if matches!(self.source.as_bytes().get(self.position), Some(b'+' | b'-')) {
                    self.position += 1;
                }
                self.digits(10)?;
            }
        }
        let suffix_start = self.position;
        while self
            .source
            .as_bytes()
            .get(self.position)
            .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
        {
            self.position += 1;
        }
        let suffix = &self.source[suffix_start..self.position];
        let digits = self.source[start..suffix_start].replace('_', "");
        if suffix == "I" {
            if float {
                return Err(Diagnostic::new(
                    "E0001",
                    "the 'I' suffix makes a bigint, which has no fraction or exponent",
                    Span::new(start, self.position),
                ));
            }
            return Ok(TokenKind::BigInteger(digits));
        }
        let name = if suffix.is_empty() {
            ""
        } else {
            crate::numeric::suffix_type(suffix).ok_or_else(|| {
                Diagnostic::new(
                    "E0001",
                    "invalid numeric literal or type suffix",
                    Span::new(start, self.position),
                )
            })?
        };
        let float_suffix = name.starts_with(['f', 'd']);
        if radix != 10 && float_suffix {
            return Err(Diagnostic::new(
                "E0001",
                "a hexadecimal or binary literal takes an integer suffix, not a floating-point one",
                Span::new(start, self.position),
            ));
        }
        float |= float_suffix;
        // Short suffixes become the type name, so `86uy` and `86i8u` are the same token.
        let text = format!("{digits}{name}");
        Ok(if float {
            TokenKind::Float(text)
        } else {
            TokenKind::Integer(text)
        })
    }

    fn string(&mut self, utf8: bool) -> Result<TokenKind, Diagnostic> {
        let start = self.position;
        self.position += 1;
        let mut text = Self::empty_literal(utf8);
        while self.position < self.source.len() {
            let ch = self.rest().chars().next().unwrap();
            self.position += ch.len_utf8();
            if ch == '"' {
                return Ok(TokenKind::String(text));
            }
            let Some(codepoint) = self.string_char(ch, utf8, start)? else {
                break;
            };
            Self::push_codepoint(&mut text, codepoint);
        }
        Err(Diagnostic::new(
            "E0001",
            "unterminated string literal",
            Span::new(start, self.position),
        ))
    }

    fn empty_literal(utf8: bool) -> StringLiteral {
        if utf8 {
            StringLiteral::Utf8(String::new())
        } else {
            StringLiteral::Utf16(Vec::new())
        }
    }

    fn push_codepoint(text: &mut StringLiteral, codepoint: u32) {
        match text {
            StringLiteral::Utf16(units) if codepoint <= 0xffff => {
                units.push(codepoint as u16);
            }
            StringLiteral::Utf16(units) => {
                let value = codepoint - 0x10000;
                units.push(0xd800 | (value >> 10) as u16);
                units.push(0xdc00 | (value & 0x3ff) as u16);
            }
            StringLiteral::Utf8(text) => {
                text.push(char::from_u32(codepoint).expect("validated Unicode scalar"));
            }
        }
    }

    /// Reads the character `ch` (already consumed) or the escape that starts
    /// with it; `None` when a backslash ends the source.
    fn string_char(
        &mut self,
        ch: char,
        utf8: bool,
        start: usize,
    ) -> Result<Option<u32>, Diagnostic> {
        match ch {
            '\\' => {
                let Some(escape) = self.rest().chars().next() else {
                    return Ok(None);
                };
                self.position += escape.len_utf8();
                Ok(Some(match escape {
                    '"' => u32::from('"'),
                    '\\' => u32::from('\\'),
                    'n' => u32::from('\n'),
                    'r' => u32::from('\r'),
                    't' => u32::from('\t'),
                    '0' => 0,
                    'u' if !utf8 || self.rest().starts_with('{') => self.unicode_escape(utf8)?,
                    _ => {
                        return Err(Diagnostic::new(
                            "E0001",
                            "invalid string escape",
                            Span::new(self.position - escape.len_utf8() - 1, self.position),
                        ));
                    }
                }))
            }
            '\n' | '\r' => Err(Diagnostic::new(
                "E0001",
                "use '\\n' for a newline inside a string",
                Span::new(start, self.position),
            )),
            _ => Ok(Some(u32::from(ch))),
        }
    }

    /// Reads the rest of `$"..."` after the opening quote's `$` (the position is
    /// at the quote). A literal without holes is an ordinary string token.
    fn interpolated(&mut self, utf8: bool, start: usize) -> Result<TokenKind, Diagnostic> {
        self.position += 1;
        let (text, end) = self.interpolated_text(utf8, start)?;
        Ok(match end {
            TextEnd::Quote => TokenKind::String(text),
            TextEnd::Hole => {
                self.holes.push(OpenHole {
                    utf8,
                    depth: 0,
                    start: self.position - 1,
                });
                TokenKind::InterpolationStart(Box::new(InterpolationPiece { text, spec: None }))
            }
        })
    }

    /// Reads literal text up to the closing quote or the `{` that opens a hole.
    /// `{{` and `}}` are single braces.
    fn interpolated_text(
        &mut self,
        utf8: bool,
        start: usize,
    ) -> Result<(StringLiteral, TextEnd), Diagnostic> {
        let mut text = Self::empty_literal(utf8);
        while self.position < self.source.len() {
            let ch = self.rest().chars().next().unwrap();
            self.position += ch.len_utf8();
            let codepoint = match ch {
                '"' => return Ok((text, TextEnd::Quote)),
                '{' if self.rest().starts_with('{') => {
                    self.position += 1;
                    u32::from('{')
                }
                '{' => return Ok((text, TextEnd::Hole)),
                '}' if self.rest().starts_with('}') => {
                    self.position += 1;
                    u32::from('}')
                }
                '}' => {
                    return Err(Diagnostic::new(
                        "E0001",
                        "a single '}' in an interpolated string must be written '}}'",
                        Span::new(self.position - 1, self.position),
                    ));
                }
                _ => match self.string_char(ch, utf8, start)? {
                    Some(codepoint) => codepoint,
                    None => break,
                },
            };
            Self::push_codepoint(&mut text, codepoint);
        }
        Err(Diagnostic::new(
            "E0001",
            "unterminated string literal",
            Span::new(start, self.position),
        ))
    }

    /// Reads the `}` or `:spec}` that closes the innermost hole (the position is
    /// at it) and the literal text after it.
    fn hole_end(&mut self, start: usize) -> Result<TokenKind, Diagnostic> {
        let hole = self.holes.last().expect("a hole is open");
        let (utf8, opened) = (hole.utf8, hole.start);
        let spec = if self.rest().starts_with(':') {
            self.format_spec(opened)?
        } else {
            self.position += 1;
            None
        };
        let (text, end) = self.interpolated_text(utf8, start)?;
        let piece = Box::new(InterpolationPiece { text, spec });
        Ok(match end {
            TextEnd::Quote => {
                self.holes.pop();
                TokenKind::InterpolationEnd(piece)
            }
            TextEnd::Hole => {
                *self.holes.last_mut().expect("a hole is open") = OpenHole {
                    utf8,
                    depth: 0,
                    start: self.position - 1,
                };
                TokenKind::InterpolationMiddle(piece)
            }
        })
    }

    /// Reads `:spec}` (the position is at the colon) and leaves the position
    /// after the `}`.
    fn format_spec(&mut self, opened: usize) -> Result<Option<FormatSpec>, Diagnostic> {
        let text_start = self.position + 1;
        let tail = &self.source[text_start..];
        let Some(length) = tail
            .find(['}', '\n', '\r', '"'])
            .filter(|index| tail.as_bytes()[*index] == b'}')
        else {
            return Err(Diagnostic::new(
                "E0001",
                "unterminated interpolation hole; close it with '}'",
                Span::new(opened, self.position + 1),
            ));
        };
        let span = Span::new(text_start, text_start + length);
        let spec = parse_format_spec(&tail[..length], span)?;
        self.position = text_start + length + 1;
        Ok(spec)
    }

    fn unicode_escape(&mut self, utf8: bool) -> Result<u32, Diagnostic> {
        let braced = self.rest().starts_with('{');
        if braced {
            self.position += 1;
        }
        let digits = self.position;
        while self
            .source
            .as_bytes()
            .get(self.position)
            .is_some_and(u8::is_ascii_hexdigit)
            && (braced || self.position - digits < 4)
        {
            self.position += 1;
        }
        let text = &self.source[digits..self.position];
        let value = u32::from_str_radix(text, 16)
            .ok()
            .filter(|value| *value <= 0x10ffff && (!utf8 || char::from_u32(*value).is_some()));
        let valid_length = if braced {
            !text.is_empty() && (!utf8 || text.len() <= 6)
        } else {
            text.len() == 4
        };
        if !valid_length || (braced && !self.rest().starts_with('}')) || value.is_none() {
            return Err(Diagnostic::new(
                "E0001",
                if utf8 {
                    "invalid Unicode scalar escape"
                } else {
                    "invalid Unicode escape"
                },
                Span::new(digits, self.position),
            ));
        }
        if braced {
            self.position += 1;
        }
        Ok(value.unwrap())
    }

    fn symbol(&mut self) -> Result<TokenKind, Diagnostic> {
        use TokenKind::*;
        // Keep a generic close followed by a constraint arrow separate from '>='.
        if self.rest().starts_with(">=>") {
            self.position += 1;
            return Ok(Greater);
        }
        for (text, kind) in [
            ("[|", LeftList),
            ("<<<", TripleLess),
            (">>>", TripleGreater),
            ("&&&", TripleAmpersand),
            ("|||", TriplePipe),
            ("^^^", TripleCaret),
            ("~~~", TripleTilde),
            ("|]", RightList),
            ("->", Arrow),
            ("=>", FatArrow),
            ("::", DoubleColon),
            ("..", DotDot),
            ("**", DoubleStar),
            ("==", EqualEqual),
            ("!=", BangEqual),
            ("<=", LessEqual),
            (">=", GreaterEqual),
            ("&&", AndAnd),
            ("||", OrOr),
            ("<<", DoubleLess),
            (">>", DoubleGreater),
            ("|>", PipeForward),
        ] {
            if self.rest().starts_with(text) {
                self.position += text.len();
                return Ok(kind);
            }
        }
        let character = self.rest().chars().next().unwrap();
        let kind = match character {
            '(' => LeftParen,
            ')' => RightParen,
            '{' => LeftBrace,
            '}' => RightBrace,
            '[' => LeftBracket,
            ']' => RightBracket,
            ':' => Colon,
            ';' => Semicolon,
            ',' => Comma,
            '.' => Dot,
            '@' => At,
            '#' => Hash,
            '=' => Equal,
            '+' => Plus,
            '-' => Minus,
            '*' => Star,
            '/' => Slash,
            '\\' => Backslash,
            '%' => Percent,
            '!' => Bang,
            '~' => Tilde,
            '<' => Less,
            '>' => Greater,
            '&' => Ampersand,
            '|' => Pipe,
            '^' => Caret,
            _ => {
                return Err(Diagnostic::new(
                    "E0001",
                    format!("unexpected character {character:?}; identifiers use ASCII"),
                    Span::new(self.position, self.position + character.len_utf8()),
                ));
            }
        };
        self.position += character.len_utf8();
        Ok(kind)
    }
}

/// Parses the text after the `:` of a hole; an empty spec means no spec.
fn parse_format_spec(text: &str, span: Span) -> Result<Option<FormatSpec>, Diagnostic> {
    if text.is_empty() {
        return Ok(None);
    }
    let invalid = |message: String| Diagnostic::new("E0001", message, span);
    let malformed = || {
        invalid(format!(
            "invalid format spec '{text}'; expected [[fill]align][+][width][.precision][type]"
        ))
    };
    let out_of_range = || {
        invalid(format!(
            "format width and precision are at most {MAX_FORMAT_FIELD} and have no leading zeros; write '0>' to pad with zeros"
        ))
    };
    let chars: Vec<char> = text.chars().collect();
    let align_of = |ch: char| match ch {
        '<' => Some(FormatAlign::Left),
        '>' => Some(FormatAlign::Right),
        '^' => Some(FormatAlign::Center),
        _ => None,
    };
    let (mut fill, mut align, mut index) = (' ', None, 0);
    if let Some(found) = chars.get(1).copied().and_then(align_of) {
        if matches!(chars[0], '{' | '}' | '"' | '\\' | '\r' | '\n') {
            return Err(malformed());
        }
        (fill, align, index) = (chars[0], Some(found), 2);
    } else if let Some(found) = align_of(chars[0]) {
        (align, index) = (Some(found), 1);
    }
    let plus = chars.get(index) == Some(&'+');
    index += usize::from(plus);
    let number = |index: &mut usize, zero_allowed: bool| -> Result<Option<u16>, Diagnostic> {
        let begin = *index;
        while chars.get(*index).is_some_and(char::is_ascii_digit) {
            *index += 1;
        }
        if begin == *index {
            return Ok(None);
        }
        let digits: String = chars[begin..*index].iter().collect();
        let leading_zero = digits.starts_with('0') && (!zero_allowed || digits.len() > 1);
        if leading_zero || digits.len() > 4 {
            return Err(out_of_range());
        }
        let value: u16 = digits.parse().map_err(|_| out_of_range())?;
        if value > MAX_FORMAT_FIELD {
            return Err(out_of_range());
        }
        Ok(Some(value))
    };
    let width = number(&mut index, false)?.unwrap_or(0);
    let mut precision = None;
    if chars.get(index) == Some(&'.') {
        index += 1;
        precision = Some(number(&mut index, true)?.ok_or_else(malformed)?);
    }
    let mut kind = None;
    let mut type_char = ' ';
    if let Some(&ch) = chars.get(index) {
        kind = Some(match ch {
            'x' => FormatKind::LowerHex,
            'X' => FormatKind::UpperHex,
            'o' => FormatKind::Octal,
            'b' => FormatKind::Binary,
            'e' => FormatKind::Exponent,
            'f' => FormatKind::Fixed,
            _ => return Err(malformed()),
        });
        type_char = ch;
        index += 1;
    }
    if index != chars.len() {
        return Err(malformed());
    }
    match (kind, precision) {
        (
            Some(
                FormatKind::LowerHex
                | FormatKind::UpperHex
                | FormatKind::Octal
                | FormatKind::Binary,
            ),
            Some(precision),
        ) => {
            return Err(invalid(format!(
                "precision does not apply to integer format '{type_char}'; remove '.{precision}'"
            )));
        }
        (Some(FormatKind::Exponent | FormatKind::Fixed), None) => {
            return Err(invalid(format!(
                "format type '{type_char}' needs a precision such as '.2{type_char}'"
            )));
        }
        _ => {}
    }
    Ok(Some(FormatSpec {
        fill,
        align,
        plus,
        width,
        precision,
        kind,
        span,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skips_a_shebang_line_only_at_byte_zero() {
        let source = "#!/usr/bin/env tsuzuri script\nlet x = 1\nx";
        assert_eq!(shebang_length(source), 29);
        let tokens = lex(source).unwrap();
        assert_eq!(tokens[0].kind, TokenKind::Let);
        // Positions stay those of the whole file, so diagnostics keep their lines and columns.
        assert_eq!(tokens[0].span, Span::new(30, 33));
        assert_eq!(
            crate::diagnostic::location(source, tokens[1].span.start),
            (2, 5)
        );
        // The line break stays a separate trivia, with a CRLF line's `\r`.
        assert_eq!(shebang_length("#!x\r\nlet y = 2"), 3);
        assert_eq!(lex("#!x\r\nlet y = 2").unwrap()[0].span.start, 5);
        assert_eq!(shebang_length("#!only"), 6);
        assert_eq!(lex("#!only").unwrap()[0].kind, TokenKind::End);
        let trivia = lex_with_trivia("#!x\n// note\nlet y = 2").unwrap();
        let kinds: Vec<_> = trivia[0].leading.iter().map(|trivia| trivia.kind).collect();
        assert_eq!(
            kinds,
            [
                TriviaKind::Newline,
                TriviaKind::LineComment,
                TriviaKind::Newline
            ]
        );
        // Anywhere else, `#!` lexes as before and no program can start with it.
        for other in [" #!x\n1", "\n#!x\n1", "\u{feff}#!x\n1", "1\n#!x"] {
            assert_eq!(shebang_length(other), 0, "{other:?}");
            assert!(crate::parser::parse(other).is_err(), "{other:?}");
        }
        assert!(crate::parser::parse(source).is_ok());
    }

    #[test]
    fn marks_namespace_paths_apart_from_annotations_and_cons() {
        use TokenKind::{DoubleColon, PathSep};
        for (source, expected) in [
            ("Sample::Features::Shape.area", vec![PathSep, PathSep]),
            ("a::b::Shape", vec![PathSep, PathSep]),
            ("namespace sample::tools", vec![PathSep]),
            ("using Sample::Features", vec![PathSep]),
            ("x\nusing a::b", vec![PathSep]),
            ("x\rusing a::b", vec![PathSep]),
            ("f using a::b", vec![DoubleColon]),
            ("def area::Sample::Shape", vec![DoubleColon, PathSep]),
            ("rec go::i64", vec![DoubleColon]),
            ("x::xs", vec![DoubleColon]),
            ("Sample :: Shape", vec![DoubleColon]),
            ("Sample::\nShape", vec![DoubleColon]),
            ("_::Shape", vec![DoubleColon]),
        ] {
            let separators: Vec<_> = lex(source)
                .unwrap()
                .into_iter()
                .map(|token| token.kind)
                .filter(|kind| matches!(kind, PathSep | DoubleColon))
                .collect();
            assert_eq!(separators, expected, "{source}");
        }
    }

    #[test]
    fn lexes_backslash_lambdas_without_changing_literal_escapes() {
        let tokens = lex(r#"\value -> "\\" '\\'"#).unwrap();
        assert_eq!(tokens[0].kind, TokenKind::Backslash);
        assert_eq!(tokens[0].span, Span::new(0, 1));
        assert_eq!(tokens[1].kind, TokenKind::Ident("value".into()));
        assert_eq!(tokens[2].kind, TokenKind::Arrow);
        assert_eq!(
            tokens[3].kind,
            TokenKind::String(StringLiteral::Utf16(vec![u16::from(b'\\')]))
        );
        assert_eq!(tokens[4].kind, TokenKind::Char(u16::from(b'\\')));
        assert_eq!(lex("fx").unwrap()[0].kind, TokenKind::Ident("fx".into()));
    }

    #[test]
    fn preserves_documentation_comments_and_line_endings() {
        let source = "/// hello\r\n///  indented\n//// slash\r\n// ignored\n/** ignored */\ndef";
        let tokens = lex(source).unwrap();
        assert_eq!(tokens[0].kind, TokenKind::DocComment("hello".into()));
        assert_eq!(tokens[1].kind, TokenKind::DocComment(" indented".into()));
        assert_eq!(tokens[2].kind, TokenKind::DocComment("/ slash".into()));
        assert_eq!(tokens[3].kind, TokenKind::Def);
        let preserved = lex_with_trivia(source).unwrap();
        assert_eq!(preserved[0].token.kind, tokens[0].kind);
        assert_eq!(preserved[1].leading[0].text, "\r\n");
    }

    #[test]
    fn lexes_comments_numbers_and_longest_operators() {
        let tokens = lex(
            "\u{feff}/* 外 /* nested */ */ 0xff 0b10 1_024 1.5e-2 >>> >> |> <<< << ** &&& ||| ^^^ ~~~",
        )
        .unwrap();
        assert_eq!(tokens[0].kind, TokenKind::Integer("0xff".into()));
        assert_eq!(tokens[1].kind, TokenKind::Integer("0b10".into()));
        assert_eq!(tokens[2].kind, TokenKind::Integer("1024".into()));
        assert_eq!(tokens[3].kind, TokenKind::Float("1.5e-2".into()));
        assert_eq!(tokens[4].kind, TokenKind::TripleGreater);
        assert_eq!(tokens[5].kind, TokenKind::DoubleGreater);
        assert_eq!(tokens[6].kind, TokenKind::PipeForward);
        assert_eq!(tokens[7].kind, TokenKind::TripleLess);
        assert_eq!(tokens[8].kind, TokenKind::DoubleLess);
        assert_eq!(tokens[9].kind, TokenKind::DoubleStar);
        assert_eq!(tokens[10].kind, TokenKind::TripleAmpersand);
        assert_eq!(tokens[11].kind, TokenKind::TriplePipe);
        assert_eq!(tokens[12].kind, TokenKind::TripleCaret);
        assert_eq!(tokens[13].kind, TokenKind::TripleTilde);
    }

    #[test]
    fn lexes_short_suffixes_and_byte_literals() {
        let tokens = lex("86y 86uy 86s 86us 86u 86l 86ul 86L 86UL 99I 4.14hf 4.14f 4.14F 0.5hm 0.5m 0.5M 'a'B \"ab\"B u8\"é\"B").unwrap();
        let names: Vec<_> = tokens
            .iter()
            .take(9)
            .map(|token| token.kind.clone())
            .collect();
        assert_eq!(
            names,
            [
                "86i8", "86i8u", "86i16", "86i16u", "86i32u", "86i64", "86i64u", "86i128",
                "86i128u",
            ]
            .map(|text| TokenKind::Integer(text.into()))
        );
        assert_eq!(tokens[9].kind, TokenKind::BigInteger("99".into()));
        let floats: Vec<_> = tokens[10..16]
            .iter()
            .map(|token| token.kind.clone())
            .collect();
        assert_eq!(
            floats,
            [
                "4.14f16", "4.14f32", "4.14f128", "0.5d32", "0.5d64", "0.5d128"
            ]
            .map(|text| TokenKind::Float(text.into()))
        );
        assert_eq!(tokens[16].kind, TokenKind::Integer("97i8u".into()));
        assert_eq!(tokens[17].kind, TokenKind::ByteString(Box::from(*b"ab")));
        assert_eq!(tokens[18].kind, TokenKind::ScalarString(Box::from([0xE9])));
        for source in [
            "0x10hf", "0b1hm", "1.5I", "'é'B", "\"é\"B", "$\"x\"B", "'ab'B",
        ] {
            assert!(lex(source).is_err(), "{source}");
        }
    }

    #[test]
    fn rejects_malformed_tokens() {
        for source in [
            "/*", "1__2", "1_", "0x", "0b2", "1e+", "1abc", "日本", "\"x",
        ] {
            assert!(lex(source).is_err(), "{source}");
        }
    }

    #[test]
    fn preserves_utf16_units_and_utf8_scalars_in_separate_literals() {
        let tokens = lex(r#""😀\uD83D\uDE00\uD800\u{dc00}\0" u8"😀\u{1f600}\0""#).unwrap();
        assert_eq!(
            tokens[0].kind,
            TokenKind::String(StringLiteral::Utf16(vec![
                0xd83d, 0xde00, 0xd83d, 0xde00, 0xd800, 0xdc00, 0,
            ]))
        );
        assert_eq!(
            tokens[1].kind,
            TokenKind::String(StringLiteral::Utf8("😀😀\0".into()))
        );
        assert_eq!(
            lex(r#""\u{00000001f600}""#).unwrap()[0].kind,
            TokenKind::String(StringLiteral::Utf16(vec![0xd83d, 0xde00]))
        );
    }

    #[test]
    fn rejects_invalid_unicode_without_replacing_surrogates() {
        for source in [
            r#""\u""#,
            r#""\u123""#,
            r#""\u12xz""#,
            r#""\u{}""#,
            r#""\u{110000}""#,
            r#""\u{ffffffffffffffff}""#,
            r#""\u{12""#,
            r#"u8"\u{d800}""#,
            r#"u8"\u{dfff}""#,
            r#"u8"\uD800""#,
            r#"u8"\u{0000000}""#,
        ] {
            assert_eq!(lex(source).unwrap_err().code, "E0001", "{source}");
        }
        let (tokens, errors) = lex_all("u8\"\\u{d800}\" junk\n42");
        assert_eq!(errors.len(), 1);
        assert_eq!(tokens[0].kind, TokenKind::Integer("42".into()));
    }

    #[test]
    fn accepts_the_source_limit_and_rejects_one_extra_byte() {
        let mut source = " ".repeat(MAX_SOURCE_BYTES);
        assert!(lex(&source).is_ok());
        source.push(' ');
        assert_eq!(lex(&source).unwrap_err().code, "E0003");
    }
}
