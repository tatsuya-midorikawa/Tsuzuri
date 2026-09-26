use crate::diagnostic::{Diagnostic, MAX_UNIQUE_DIAGNOSTICS, Span};
use crate::syntax::{MAX_SOURCE_BYTES, StringLiteral, Token, TokenKind};

pub fn lex(source: &str) -> Result<Vec<Token>, Diagnostic> {
    let (tokens, diagnostics) = tokenize(source, false);
    diagnostics.into_iter().next().map_or(Ok(tokens), Err)
}

pub fn lex_all(source: &str) -> (Vec<Token>, Vec<Diagnostic>) {
    tokenize(source, true)
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
        position: if source.starts_with('\u{feff}') { 3 } else { 0 },
    }
    .tokens(recovering)
}

struct Lexer<'a> {
    source: &'a str,
    position: usize,
}

impl Lexer<'_> {
    fn tokens(mut self, recovering: bool) -> (Vec<Token>, Vec<Diagnostic>) {
        let mut tokens = Vec::new();
        let mut diagnostics = Vec::new();
        while self.position < self.source.len() {
            let start = self.position;
            match self.token() {
                Ok(Some(token)) => tokens.push(token),
                Ok(None) => {}
                Err(error) => {
                    diagnostics.push(error);
                    if !recovering || diagnostics.len() == MAX_UNIQUE_DIAGNOSTICS {
                        self.position = self.source.len();
                        break;
                    }
                    self.recover(start);
                }
            }
        }
        tokens.push(Token {
            kind: TokenKind::End,
            span: Span::new(self.position, self.position),
        });
        (tokens, diagnostics)
    }

    fn recover(&mut self, start: usize) {
        let first = self.source[start..].chars().next().unwrap();
        self.position = self.position.max(start + first.len_utf8());
        if first == '"' || self.source[start..].starts_with("u8\"") {
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
            self.position += 1;
            return Ok(None);
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
        let kind = if byte == b'\'' {
            self.position += 1;
            if !self
                .source
                .as_bytes()
                .get(self.position)
                .is_some_and(|b| b.is_ascii_alphabetic())
            {
                return Err(Diagnostic::new(
                    "E0001",
                    "a type variable starts with an apostrophe and an ASCII letter",
                    Span::new(start, self.position),
                ));
            }
            self.identifier();
            TokenKind::TypeVariable(self.source[start + 1..self.position].to_owned())
        } else if self.rest().starts_with("u8\"") {
            self.position += 2;
            self.string(true)?
        } else if byte.is_ascii_alphabetic() || byte == b'_' {
            self.identifier()
        } else if byte.is_ascii_digit() {
            self.number()?
        } else if byte == b'"' {
            self.string(false)?
        } else {
            self.symbol()?
        };
        Ok(Some(Token {
            kind,
            span: Span::new(start, self.position),
        }))
    }

    fn rest(&self) -> &str {
        &self.source[self.position..]
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
            "fx" => TokenKind::Fx,
            "def" => TokenKind::Def,
            "rec" => TokenKind::Rec,
            "and" => TokenKind::And,
            "export" => TokenKind::Export,
            "private" => TokenKind::Private,
            "record" => TokenKind::Record,
            "union" => TokenKind::Union,
            "class" => TokenKind::Class,
            "instance" => TokenKind::Instance,
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
        if !suffix.is_empty() && !crate::numeric::is_numeric_name(suffix) {
            return Err(Diagnostic::new(
                "E0001",
                "invalid numeric literal or type suffix",
                Span::new(start, self.position),
            ));
        }
        float |= suffix.starts_with(['f', 'd']);
        let text = self.source[start..self.position].replace('_', "");
        Ok(if float {
            TokenKind::Float(text)
        } else {
            TokenKind::Integer(text)
        })
    }

    fn string(&mut self, utf8: bool) -> Result<TokenKind, Diagnostic> {
        let start = self.position;
        self.position += 1;
        let mut text = if utf8 {
            StringLiteral::Utf8(String::new())
        } else {
            StringLiteral::Utf16(Vec::new())
        };
        while self.position < self.source.len() {
            let ch = self.rest().chars().next().unwrap();
            self.position += ch.len_utf8();
            let codepoint = match ch {
                '"' => return Ok(TokenKind::String(text)),
                '\\' => {
                    let Some(escape) = self.rest().chars().next() else {
                        break;
                    };
                    self.position += escape.len_utf8();
                    match escape {
                        '"' => u32::from('"'),
                        '\\' => u32::from('\\'),
                        'n' => u32::from('\n'),
                        'r' => u32::from('\r'),
                        't' => u32::from('\t'),
                        '0' => 0,
                        'u' if !utf8 || self.rest().starts_with('{') => {
                            self.unicode_escape(utf8)?
                        }
                        _ => {
                            return Err(Diagnostic::new(
                                "E0001",
                                "invalid string escape",
                                Span::new(self.position - escape.len_utf8() - 1, self.position),
                            ));
                        }
                    }
                }
                '\n' | '\r' => {
                    return Err(Diagnostic::new(
                        "E0001",
                        "use '\\n' for a newline inside a string",
                        Span::new(start, self.position),
                    ));
                }
                _ => u32::from(ch),
            };
            match &mut text {
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
        Err(Diagnostic::new(
            "E0001",
            "unterminated string literal",
            Span::new(start, self.position),
        ))
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
            ("|]", RightList),
            (">>>", ShiftRightUnsigned),
            ("->", Arrow),
            ("=>", FatArrow),
            ("::", DoubleColon),
            ("..", DotDot),
            ("==", EqualEqual),
            ("!=", BangEqual),
            ("<=", LessEqual),
            (">=", GreaterEqual),
            ("&&", AndAnd),
            ("||", OrOr),
            ("<<", ShiftLeft),
            (">>", ShiftRight),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lexes_comments_numbers_and_longest_operators() {
        let tokens = lex("\u{feff}/* 外 /* nested */ */ 0xff 0b10 1_024 1.5e-2 >>> >> |>").unwrap();
        assert_eq!(tokens[0].kind, TokenKind::Integer("0xff".into()));
        assert_eq!(tokens[1].kind, TokenKind::Integer("0b10".into()));
        assert_eq!(tokens[2].kind, TokenKind::Integer("1024".into()));
        assert_eq!(tokens[3].kind, TokenKind::Float("1.5e-2".into()));
        assert_eq!(tokens[4].kind, TokenKind::ShiftRightUnsigned);
        assert_eq!(tokens[5].kind, TokenKind::ShiftRight);
        assert_eq!(tokens[6].kind, TokenKind::PipeForward);
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
