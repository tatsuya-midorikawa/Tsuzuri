//! XML property lists (M15): the `Info.plist` of an application bundle and the `info_plist` file of
//! `[macos]`. [`write`] gives the bytes that `plutil -convert xml1` gives for the same value, so the
//! output is the same on every host and a golden file checks it. [`read`] takes the subset of XML that
//! such files use and refuses everything else with a reason; it never guesses.

use std::cmp::Ordering;
use std::collections::BTreeMap;

/// The largest file that [`read`] takes.
pub const MAX_BYTES: usize = 1 << 20;
/// The deepest nesting of values that [`read`] takes; the top-level value is at depth 1.
pub const MAX_DEPTH: usize = 128;

/// A dictionary key. Keys order by their UTF-16 code units, as Core Foundation sorts them when it writes
/// a dictionary: `B` < `_` < `a`, and a character outside the BMP comes before `U+E000`..`U+FFFF`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Key(String);

impl Key {
    pub fn new(text: impl Into<String>) -> Self {
        Self(text.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Ord for Key {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.encode_utf16().cmp(other.0.encode_utf16())
    }
}

impl PartialOrd for Key {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

pub type Dictionary = BTreeMap<Key, Value>;

/// A property-list value. `Integer`, `Real` and `Date` hold the text of the element, whose form was
/// checked and which is written back as it is (`<real>2.0</real>` stays `2.0`; `plutil` would write `2`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    String(String),
    Integer(String),
    Real(String),
    Bool(bool),
    Date(String),
    Data(Vec<u8>),
    Array(Vec<Value>),
    Dict(Dictionary),
}

impl Value {
    pub fn string(text: impl Into<String>) -> Self {
        Self::String(text.into())
    }
}

const HEADER: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\">\n";

/// The XML document of `value`, byte for byte what `plutil -convert xml1` writes: tabs, LF, the keys of
/// each dictionary in UTF-16 order, and only `&`, `<` and `>` escaped.
pub fn write(value: &Value) -> Vec<u8> {
    let mut out = String::from(HEADER);
    write_value(&mut out, value, 0);
    out.push_str("</plist>\n");
    out.into_bytes()
}

fn indent(out: &mut String, depth: usize) {
    for _ in 0..depth {
        out.push('\t');
    }
}

fn escape(out: &mut String, text: &str) {
    for character in text.chars() {
        match character {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            other => out.push(other),
        }
    }
}

fn leaf(out: &mut String, name: &str, text: &str) {
    out.push('<');
    out.push_str(name);
    out.push('>');
    escape(out, text);
    out.push_str("</");
    out.push_str(name);
    out.push_str(">\n");
}

fn write_value(out: &mut String, value: &Value, depth: usize) {
    indent(out, depth);
    match value {
        Value::String(text) => leaf(out, "string", text),
        Value::Integer(text) => leaf(out, "integer", text),
        Value::Real(text) => leaf(out, "real", text),
        Value::Date(text) => leaf(out, "date", text),
        Value::Bool(true) => out.push_str("<true/>\n"),
        Value::Bool(false) => out.push_str("<false/>\n"),
        Value::Data(bytes) => {
            out.push_str("<data>\n");
            indent(out, depth);
            write_base64(out, bytes, depth);
            out.push_str("</data>\n");
        }
        Value::Array(items) if items.is_empty() => out.push_str("<array/>\n"),
        Value::Array(items) => {
            out.push_str("<array>\n");
            for item in items {
                write_value(out, item, depth + 1);
            }
            indent(out, depth);
            out.push_str("</array>\n");
        }
        Value::Dict(entries) if entries.is_empty() => out.push_str("<dict/>\n"),
        Value::Dict(entries) => {
            out.push_str("<dict>\n");
            for (key, item) in entries {
                indent(out, depth + 1);
                leaf(out, "key", key.as_str());
                write_value(out, item, depth + 1);
            }
            indent(out, depth);
            out.push_str("</dict>\n");
        }
    }
}

const BASE64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Base64 in Core Foundation's layout: a line ends once it holds `76 - 8 * depth` characters (the check
/// runs after each input byte, so a deep value gets very short lines), and each line starts with the
/// indentation. The text ends with a line end and the indentation of the closing tag.
fn write_base64(out: &mut String, bytes: &[u8], depth: usize) {
    let limit = 76isize - 8 * depth as isize;
    let mut held = 0isize;
    for (index, byte) in bytes.iter().enumerate() {
        match index % 3 {
            0 => {
                out.push(BASE64[usize::from(byte >> 2)] as char);
                held += 1;
            }
            1 => {
                let previous = bytes[index - 1];
                out.push(BASE64[usize::from(((previous << 4) & 0x30) | (byte >> 4))] as char);
                held += 1;
            }
            _ => {
                let previous = bytes[index - 1];
                out.push(BASE64[usize::from(((previous << 2) & 0x3c) | (byte >> 6))] as char);
                out.push(BASE64[usize::from(byte & 0x3f)] as char);
                held += 2;
            }
        }
        if held >= limit {
            out.push('\n');
            indent(out, depth);
            held = 0;
        }
    }
    match bytes.len() % 3 {
        1 => {
            out.push(BASE64[usize::from((bytes[bytes.len() - 1] << 4) & 0x30)] as char);
            out.push_str("==");
            held += 3;
        }
        2 => {
            out.push(BASE64[usize::from((bytes[bytes.len() - 1] << 2) & 0x3c)] as char);
            out.push('=');
            held += 2;
        }
        _ => {}
    }
    if held > 0 {
        out.push('\n');
        indent(out, depth);
    }
}

/// Reads an XML property list. The subset: the elements `plist`, `dict`, `key`, `array`, `string`,
/// `integer`, `real`, `true`, `false`, `date` and `data`; the five predefined entities and numeric
/// character references; comments between elements, the XML declaration and Apple's DOCTYPE, which are
/// skipped. A binary property list, CDATA, another processing instruction or DOCTYPE, an unknown
/// element, a repeated key, more than [`MAX_DEPTH`] levels and more than [`MAX_BYTES`] bytes are errors.
pub fn read(bytes: &[u8]) -> Result<Value, String> {
    if bytes.starts_with(b"bplist") {
        return Err(
            "it is a binary property list; convert it with 'plutil -convert xml1 <file>'".into(),
        );
    }
    if bytes.len() > MAX_BYTES {
        return Err(format!(
            "it is larger than {MAX_BYTES} bytes (1 MiB), the limit of a property list here"
        ));
    }
    let text = std::str::from_utf8(bytes)
        .map_err(|_| "it is not UTF-8; save it as an XML property list in UTF-8".to_owned())?;
    let mut reader = Reader {
        rest: text.strip_prefix('\u{feff}').unwrap_or(text),
    };
    reader.document()
}

/// Reads an XML property list whose top-level value is a dictionary.
pub fn read_dictionary(bytes: &[u8]) -> Result<Dictionary, String> {
    match read(bytes)? {
        Value::Dict(entries) => Ok(entries),
        _ => Err("its top-level value must be a <dict>".into()),
    }
}

struct Reader<'a> {
    rest: &'a str,
}

/// An opening tag: its name and whether it closed itself (`<true/>`).
struct Tag<'a> {
    name: &'a str,
    empty: bool,
}

const APPLE_DOCTYPES: [&str; 2] = [
    "-//Apple//DTD PLIST 1.0//EN",
    "-//Apple Computer//DTD PLIST 1.0//EN",
];

impl<'a> Reader<'a> {
    fn take(&mut self, token: &str) -> bool {
        match self.rest.strip_prefix(token) {
            Some(rest) => {
                self.rest = rest;
                true
            }
            None => false,
        }
    }

    fn space(&mut self) {
        self.rest = self.rest.trim_start_matches([' ', '\t', '\n', '\r']);
    }

    /// Skips white space and comments: what may stand between elements.
    fn misc(&mut self) -> Result<(), String> {
        loop {
            self.space();
            if !self.take("<!--") {
                return Ok(());
            }
            let Some(end) = self.rest.find("-->") else {
                return Err("a comment is not closed with '-->'".into());
            };
            self.rest = &self.rest[end + 3..];
        }
    }

    fn document(&mut self) -> Result<Value, String> {
        if self.rest.starts_with("<?xml") {
            let Some(end) = self.rest.find("?>") else {
                return Err("the XML declaration is not closed with '?>'".into());
            };
            let declaration = &self.rest[..end];
            if let Some(at) = declaration.find("encoding") {
                let value = declaration[at + "encoding".len()..]
                    .trim_start_matches([' ', '\t', '\n', '\r', '='])
                    .trim_start_matches(['"', '\'']);
                if !value.to_ascii_lowercase().starts_with("utf-8") {
                    return Err("its XML declaration names an encoding other than UTF-8".into());
                }
            }
            self.rest = &self.rest[end + 2..];
        }
        self.misc()?;
        if self.take("<!DOCTYPE") {
            let Some(end) = self.rest.find('>') else {
                return Err("the DOCTYPE is not closed with '>'".into());
            };
            let doctype = &self.rest[..end];
            if doctype.contains('[') || !APPLE_DOCTYPES.iter().any(|id| doctype.contains(id)) {
                return Err(
                    "only Apple's property-list DOCTYPE (\"-//Apple//DTD PLIST 1.0//EN\") is read; remove the other DOCTYPE"
                        .into(),
                );
            }
            self.rest = &self.rest[end + 1..];
            self.misc()?;
        }
        let tag = self.open()?;
        if tag.name != "plist" {
            return Err(format!(
                "expected the <plist> element, found <{}>",
                tag.name
            ));
        }
        if tag.empty {
            return Err("the <plist> element holds no value".into());
        }
        self.misc()?;
        let value = self.value(1)?;
        self.misc()?;
        self.close("plist")?;
        self.misc()?;
        if !self.rest.is_empty() {
            return Err("there is text after </plist>".into());
        }
        Ok(value)
    }

    /// An opening tag. Only `<plist>` takes an attribute, `version="1.0"`.
    fn open(&mut self) -> Result<Tag<'a>, String> {
        if self.rest.starts_with("<![CDATA[") {
            return Err(
                "CDATA sections are not read; write the text with &amp;, &lt; and &gt;".into(),
            );
        }
        if self.rest.starts_with("<?") {
            return Err(
                "processing instructions other than the XML declaration are not read".into(),
            );
        }
        if self.rest.starts_with("<!") {
            return Err("only comments and Apple's DOCTYPE may start with '<!'".into());
        }
        if self.rest.starts_with("</") || !self.take("<") {
            return Err(match self.rest.chars().next() {
                Some(_) => "expected an element".to_owned(),
                None => "it ends before its elements are closed".to_owned(),
            });
        }
        let end = self
            .rest
            .find(|character: char| !character.is_ascii_alphanumeric())
            .unwrap_or(self.rest.len());
        let name = &self.rest[..end];
        if name.is_empty() {
            return Err("expected an element name after '<'".into());
        }
        self.rest = &self.rest[end..];
        self.space();
        if name == "plist" && self.take("version") {
            self.space();
            let quoted = self.take("=") && {
                self.space();
                self.take("\"1.0\"") || self.take("'1.0'")
            };
            if !quoted {
                return Err("the version of <plist> must be \"1.0\"".into());
            }
            self.space();
        }
        let empty = self.take("/");
        if !self.take(">") {
            return Err(format!("<{name}> takes no attributes here; expected '>'"));
        }
        Ok(Tag { name, empty })
    }

    fn close(&mut self, name: &str) -> Result<(), String> {
        let closed = self.take("</") && self.take(name) && {
            self.space();
            self.take(">")
        };
        if closed {
            Ok(())
        } else {
            Err(format!("expected </{name}>"))
        }
    }

    /// The text of a leaf element up to its closing tag, with its references replaced.
    fn text(&mut self, name: &str) -> Result<String, String> {
        let mut text = String::new();
        loop {
            let Some(at) = self.rest.find(['<', '&']) else {
                return Err(format!("<{name}> is not closed"));
            };
            let (plain, rest) = self.rest.split_at(at);
            if let Some(bad) = plain.chars().find(|&character| !xml_character(character)) {
                return Err(format!(
                    "the control character U+{:04X} cannot be in XML text",
                    bad as u32
                ));
            }
            text.push_str(plain);
            self.rest = rest;
            if self.rest.starts_with('<') {
                if self.rest.starts_with("<![CDATA[") {
                    return Err(
                        "CDATA sections are not read; write the text with &amp;, &lt; and &gt;"
                            .into(),
                    );
                }
                if !self.rest.starts_with("</") {
                    return Err(format!(
                        "<{name}> holds text only; a comment or an element inside it is not read"
                    ));
                }
                self.close(name)?;
                return Ok(text);
            }
            let Some(end) = self.rest.find(';') else {
                return Err("a reference that starts with '&' is not closed with ';'".into());
            };
            let reference = &self.rest[1..end];
            text.push(match reference {
                "amp" => '&',
                "lt" => '<',
                "gt" => '>',
                "quot" => '"',
                "apos" => '\'',
                _ => {
                    let scalar = match reference.strip_prefix("#x") {
                        Some(hex) if !hex.is_empty() => u32::from_str_radix(hex, 16).ok(),
                        Some(_) => None,
                        None => reference
                            .strip_prefix('#')
                            .filter(|digits| {
                                !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit())
                            })
                            .and_then(|digits| digits.parse().ok()),
                    };
                    scalar
                        .and_then(char::from_u32)
                        .filter(|&character| xml_character(character))
                        .ok_or_else(|| {
                            format!(
                                "'&{reference};' is not a reference that is read; use &amp;, &lt;, &gt;, &quot;, &apos; or a numeric character reference"
                            )
                        })?
                }
            });
            self.rest = &self.rest[end + 1..];
        }
    }

    fn leaf(&mut self, tag: &Tag<'_>) -> Result<String, String> {
        if tag.empty {
            Ok(String::new())
        } else {
            self.text(tag.name)
        }
    }

    fn value(&mut self, depth: usize) -> Result<Value, String> {
        let tag = self.open()?;
        match tag.name {
            "string" => Ok(Value::String(self.leaf(&tag)?)),
            "true" | "false" => {
                if !tag.empty {
                    self.misc()?;
                    self.close(tag.name)?;
                }
                Ok(Value::Bool(tag.name == "true"))
            }
            "integer" => {
                let text = self.leaf(&tag)?;
                let text = text.trim_matches([' ', '\t', '\n', '\r']);
                if !valid_integer(text) {
                    return Err(format!(
                        "<integer>{text}</integer> must be decimal digits with an optional '-', within 64 bits"
                    ));
                }
                Ok(Value::Integer(text.to_owned()))
            }
            "real" => {
                let text = self.leaf(&tag)?;
                let text = text.trim_matches([' ', '\t', '\n', '\r']);
                if !valid_real(text) {
                    return Err(format!(
                        "<real>{text}</real> must be a decimal number such as 1.5 or -2e3"
                    ));
                }
                Ok(Value::Real(text.to_owned()))
            }
            "date" => {
                let text = self.leaf(&tag)?;
                let text = text.trim_matches([' ', '\t', '\n', '\r']);
                if !valid_date(text) {
                    return Err(format!(
                        "<date>{text}</date> must have the form 2026-11-28T09:30:00Z"
                    ));
                }
                Ok(Value::Date(text.to_owned()))
            }
            "data" => {
                let text = self.leaf(&tag)?;
                decode_base64(&text)
                    .map(Value::Data)
                    .ok_or_else(|| "<data> must hold base64 text".to_owned())
            }
            "array" | "dict" if depth > MAX_DEPTH => {
                Err(format!("it nests values deeper than {MAX_DEPTH} levels"))
            }
            "array" => {
                let mut items = Vec::new();
                if !tag.empty {
                    loop {
                        self.misc()?;
                        if self.rest.starts_with("</") {
                            break;
                        }
                        items.push(self.value(depth + 1)?);
                    }
                    self.close("array")?;
                }
                Ok(Value::Array(items))
            }
            "dict" => {
                let mut entries = Dictionary::new();
                if !tag.empty {
                    loop {
                        self.misc()?;
                        if self.rest.starts_with("</") {
                            break;
                        }
                        let key = self.open()?;
                        if key.name != "key" {
                            return Err(format!(
                                "expected a <key> in <dict>, found <{}>",
                                key.name
                            ));
                        }
                        let name = self.leaf(&key)?;
                        self.misc()?;
                        if self.rest.starts_with("</") {
                            return Err(format!("the key '{name}' has no value"));
                        }
                        let value = self.value(depth + 1)?;
                        if entries.contains_key(&Key::new(name.as_str())) {
                            return Err(format!("the key '{name}' occurs twice in one <dict>"));
                        }
                        entries.insert(Key::new(name), value);
                    }
                    self.close("dict")?;
                }
                Ok(Value::Dict(entries))
            }
            "key" => Err("a <key> may only stand in a <dict>, before its value".into()),
            other => Err(format!(
                "<{other}> is not a property-list element; expected dict, array, string, integer, real, true, false, date or data"
            )),
        }
    }
}

/// The characters of XML 1.0 text.
fn xml_character(character: char) -> bool {
    matches!(character, '\t' | '\n' | '\r' | '\u{20}'..='\u{d7ff}' | '\u{e000}'..='\u{fffd}' | '\u{10000}'..)
}

fn valid_integer(text: &str) -> bool {
    let digits = text.strip_prefix('-').unwrap_or(text);
    !digits.is_empty()
        && digits.bytes().all(|byte| byte.is_ascii_digit())
        && if text.starts_with('-') {
            text.parse::<i64>().is_ok()
        } else {
            text.parse::<u64>().is_ok()
        }
}

fn valid_real(text: &str) -> bool {
    let rest = text.strip_prefix(['-', '+']).unwrap_or(text);
    let (mantissa, exponent) = match rest.find(['e', 'E']) {
        Some(at) => (&rest[..at], Some(&rest[at + 1..])),
        None => (rest, None),
    };
    let (whole, fraction) = match mantissa.find('.') {
        Some(at) => (&mantissa[..at], &mantissa[at + 1..]),
        None => (mantissa, ""),
    };
    let digits = |part: &str| part.bytes().all(|byte| byte.is_ascii_digit());
    digits(whole)
        && digits(fraction)
        && !(whole.is_empty() && fraction.is_empty())
        && exponent.is_none_or(|exponent| {
            let exponent = exponent.strip_prefix(['-', '+']).unwrap_or(exponent);
            !exponent.is_empty() && digits(exponent)
        })
}

/// `YYYY-MM-DDTHH:MM:SSZ` with a day that the month has.
fn valid_date(text: &str) -> bool {
    let bytes = text.as_bytes();
    if bytes.len() != 20 {
        return false;
    }
    let number = |range: std::ops::Range<usize>| -> Option<u32> {
        let part = &text[range];
        part.bytes()
            .all(|byte| byte.is_ascii_digit())
            .then(|| part.parse().ok())
            .flatten()
    };
    let (Some(year), Some(month), Some(day), Some(hour), Some(minute), Some(second)) = (
        number(0..4),
        number(5..7),
        number(8..10),
        number(11..13),
        number(14..16),
        number(17..19),
    ) else {
        return false;
    };
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return false,
    };
    bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes[10] == b'T'
        && bytes[13] == b':'
        && bytes[16] == b':'
        && bytes[19] == b'Z'
        && (1..=days).contains(&day)
        && hour < 24
        && minute < 60
        && second < 60
}

/// Standard base64 with padding; white space between the characters is skipped.
fn decode_base64(text: &str) -> Option<Vec<u8>> {
    let mut symbols = Vec::with_capacity(text.len());
    for byte in text.bytes() {
        if !matches!(byte, b' ' | b'\t' | b'\n' | b'\r') {
            symbols.push(byte);
        }
    }
    if symbols.len() % 4 != 0 {
        return None;
    }
    let sextet = |byte: u8| -> Option<u32> {
        Some(u32::from(match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        }))
    };
    let mut bytes = Vec::with_capacity(symbols.len() / 4 * 3);
    let groups = symbols.len() / 4;
    for (index, group) in symbols.chunks_exact(4).enumerate() {
        let padding = group.iter().rev().take_while(|&&byte| byte == b'=').count();
        if padding > 2 || (padding > 0 && index + 1 != groups) {
            return None;
        }
        let mut word = 0u32;
        for &byte in &group[..4 - padding] {
            word = (word << 6) | sextet(byte)?;
        }
        word <<= 6 * padding as u32;
        bytes.push((word >> 16) as u8);
        if padding < 2 {
            bytes.push((word >> 8) as u8);
        }
        if padding < 1 {
            bytes.push(word as u8);
        }
    }
    Some(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_order_by_utf16_code_units() {
        let mut keys: Vec<Key> = [
            "b",
            "a",
            "_x",
            "Z",
            "B",
            "1",
            "\u{ff61}",
            "\u{1f600}",
            "\u{e9}",
        ]
        .into_iter()
        .map(Key::new)
        .collect();
        keys.sort();
        let order: Vec<&str> = keys.iter().map(Key::as_str).collect();
        // U+1F600 is the surrogates D83D DE00, which come before U+FF61.
        assert_eq!(
            order,
            [
                "1",
                "B",
                "Z",
                "_x",
                "a",
                "b",
                "\u{e9}",
                "\u{1f600}",
                "\u{ff61}"
            ]
        );
    }

    #[test]
    fn base64_round_trips_every_tail_length() {
        for length in 0..=9usize {
            let bytes: Vec<u8> = (0..length).map(|index| (index * 37 + 250) as u8).collect();
            let mut text = String::new();
            write_base64(&mut text, &bytes, 0);
            assert_eq!(decode_base64(&text), Some(bytes), "{text:?}");
        }
        assert_eq!(decode_base64("AQ=="), Some(vec![1]));
        assert_eq!(decode_base64("AQ=A"), None);
        assert_eq!(decode_base64("AQ"), None);
        assert_eq!(decode_base64("A==="), None);
        assert_eq!(decode_base64("AQ==AQ=="), None);
        assert_eq!(decode_base64("A-=="), None);
    }

    #[test]
    fn checks_the_forms_of_numbers_and_dates() {
        for good in ["0", "-5", "18446744073709551615", "-9223372036854775808"] {
            assert!(valid_integer(good), "{good}");
        }
        for bad in [
            "",
            "-",
            "+5",
            "0x10",
            "1.0",
            "18446744073709551616",
            "-9223372036854775809",
        ] {
            assert!(!valid_integer(bad), "{bad}");
        }
        for good in ["2.0", "-1", "+.5", "1.", "1e9", "1.5E-3"] {
            assert!(valid_real(good), "{good}");
        }
        for bad in ["", ".", "e5", "1e", "nan", "1.2.3", "1 2"] {
            assert!(!valid_real(bad), "{bad}");
        }
        assert!(valid_date("2024-02-29T23:59:59Z"));
        for bad in [
            "2023-02-29T00:00:00Z",
            "2026-13-01T00:00:00Z",
            "2026-11-28T24:00:00Z",
            "2026-11-28 09:30:00Z",
            "2026-11-28T09:30:00",
        ] {
            assert!(!valid_date(bad), "{bad}");
        }
    }
}
