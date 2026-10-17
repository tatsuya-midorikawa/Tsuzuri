//! Scanning of the textual IR: the bytes between two tokens, names, numbers and quoted strings.

use super::error::{Failure, Res, bail};

pub(crate) struct Lexer<'a> {
    pub(crate) text: &'a str,
    src: &'a [u8],
    pub(crate) pos: usize,
}

fn is_name_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'$' | b'-')
}

impl<'a> Lexer<'a> {
    pub(crate) fn new(text: &'a str, pos: usize) -> Self {
        Self {
            text,
            src: text.as_bytes(),
            pos,
        }
    }

    #[inline]
    pub(crate) fn peek(&self) -> u8 {
        self.src.get(self.pos).copied().unwrap_or(0)
    }

    #[inline]
    pub(crate) fn peek_at(&self, offset: usize) -> u8 {
        self.src.get(self.pos + offset).copied().unwrap_or(0)
    }

    #[inline]
    pub(crate) fn bump(&mut self) {
        self.pos += 1;
    }

    pub(crate) fn eof(&self) -> bool {
        self.pos >= self.src.len()
    }

    /// Spaces and tabs: the end of a line stays.
    #[inline]
    pub(crate) fn skip_ws(&mut self) {
        while matches!(self.peek(), b' ' | b'\t' | b'\r') {
            self.pos += 1;
        }
    }

    /// Spaces, line ends and `;` comments.
    pub(crate) fn skip_ws_nl(&mut self) {
        loop {
            match self.peek() {
                b' ' | b'\t' | b'\r' | b'\n' => self.pos += 1,
                b';' => self.skip_to_eol(),
                _ => break,
            }
        }
    }

    /// Stops at the line end without consuming it.
    pub(crate) fn skip_to_eol(&mut self) {
        match self.text[self.pos.min(self.src.len())..].find('\n') {
            Some(offset) => self.pos += offset,
            None => self.pos = self.src.len(),
        }
    }

    /// Moves after the next line end.
    pub(crate) fn skip_line(&mut self) {
        self.skip_to_eol();
        if !self.eof() {
            self.pos += 1;
        }
    }

    /// Whether only a comment or nothing is left on the line.
    pub(crate) fn at_line_end(&mut self) -> bool {
        self.skip_ws();
        matches!(self.peek(), b'\n' | b';') || self.eof()
    }

    pub(crate) fn eat(&mut self, byte: u8) -> bool {
        self.skip_ws();
        if self.peek() == byte {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    /// `eat`, looking over line ends too.
    pub(crate) fn eat_nl(&mut self, byte: u8) -> bool {
        self.skip_ws_nl();
        if self.peek() == byte {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    pub(crate) fn expect(&mut self, byte: u8) -> Res<()> {
        if self.eat(byte) {
            Ok(())
        } else {
            bail!("'{}' expected near {}", byte as char, self.context())
        }
    }

    pub(crate) fn expect_nl(&mut self, byte: u8) -> Res<()> {
        if self.eat_nl(byte) {
            Ok(())
        } else {
            bail!("'{}' expected near {}", byte as char, self.context())
        }
    }

    /// A keyword or type name: letters, digits, `_` and `.`.
    pub(crate) fn word(&mut self) -> &'a str {
        self.skip_ws();
        let start = self.pos;
        while matches!(self.peek(), b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_' | b'.') {
            self.pos += 1;
        }
        &self.text[start..self.pos]
    }

    pub(crate) fn peek_word(&mut self) -> &'a str {
        let saved = self.pos;
        let word = self.word();
        self.pos = saved;
        word
    }

    pub(crate) fn eat_word(&mut self, word: &str) -> bool {
        let saved = self.pos;
        if self.word() == word {
            true
        } else {
            self.pos = saved;
            false
        }
    }

    pub(crate) fn expect_word(&mut self, word: &str) -> Res<()> {
        if self.eat_word(word) {
            Ok(())
        } else {
            bail!("'{word}' expected near {}", self.context())
        }
    }

    /// The name after a `%` or `@`: quoted or made of name characters. Quotes are not part of it,
    /// and escapes stay as they are spelled.
    pub(crate) fn name(&mut self) -> Res<&'a str> {
        if self.peek() == b'"' {
            return Ok(self.quoted()?);
        }
        let start = self.pos;
        while is_name_byte(self.peek()) {
            self.pos += 1;
        }
        if self.pos == start {
            bail!("a name near {}", self.context());
        }
        Ok(&self.text[start..self.pos])
    }

    /// The text between two quotes, with its escapes undecoded.
    pub(crate) fn quoted(&mut self) -> Res<&'a str> {
        if self.peek() != b'"' {
            bail!("a quoted string near {}", self.context());
        }
        let start = self.pos + 1;
        match self.text[start..].find('"') {
            Some(length) => {
                self.pos = start + length + 1;
                Ok(&self.text[start..start + length])
            }
            None => bail!("an unterminated string near {}", self.context()),
        }
    }

    /// A decimal integer with an optional sign, wrapping at 128 bits.
    pub(crate) fn int(&mut self) -> Res<i128> {
        self.skip_ws();
        let negative = match self.peek() {
            b'-' => {
                self.pos += 1;
                true
            }
            b'+' => {
                self.pos += 1;
                false
            }
            _ => false,
        };
        let start = self.pos;
        let mut value = 0u128;
        while self.peek().is_ascii_digit() {
            value = value
                .wrapping_mul(10)
                .wrapping_add(u128::from(self.peek() - b'0'));
            self.pos += 1;
        }
        if self.pos == start {
            bail!("an integer near {}", self.context());
        }
        let value = value as i128;
        Ok(if negative {
            value.wrapping_neg()
        } else {
            value
        })
    }

    pub(crate) fn uint(&mut self) -> Res<u64> {
        self.skip_ws();
        let start = self.pos;
        let mut value = 0u64;
        while self.peek().is_ascii_digit() {
            value = value
                .checked_mul(10)
                .and_then(|value| value.checked_add(u64::from(self.peek() - b'0')))
                .ok_or_else(|| Failure::new("an integer that large"))?;
            self.pos += 1;
        }
        if self.pos == start {
            bail!("an integer near {}", self.context());
        }
        Ok(value)
    }

    /// Skips a balanced `( ... )` group, strings included.
    pub(crate) fn skip_parens(&mut self) -> Res<()> {
        self.expect(b'(')?;
        let mut depth = 1;
        while depth > 0 {
            match self.peek() {
                0 if self.eof() => bail!("an unclosed '(' near {}", self.context()),
                b'(' => depth += 1,
                b')' => depth -= 1,
                b'"' => {
                    self.quoted()?;
                    continue;
                }
                _ => {}
            }
            self.bump();
        }
        Ok(())
    }

    /// The line around the position, for a message.
    pub(crate) fn context(&self) -> String {
        let mut at = self.pos.min(self.src.len());
        while !self.text.is_char_boundary(at) {
            at -= 1;
        }
        let start = self.text[..at].rfind('\n').map_or(0, |index| index + 1);
        let end = self.text[at..]
            .find('\n')
            .map_or(self.src.len(), |index| at + index);
        let line = self.text[start..end].trim();
        let mut shown: String = line.chars().take(100).collect();
        if line.chars().count() > 100 {
            shown.push_str("...");
        }
        format!("\"{shown}\"")
    }
}
