use std::fmt::Write;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    pub source: Option<usize>,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Self {
        Self {
            start,
            end,
            source: None,
        }
    }

    pub fn in_source(self, source: usize) -> Self {
        Self {
            source: Some(source),
            ..self
        }
    }

    pub fn through(self, other: Self) -> Self {
        Self {
            end: other.end,
            ..self
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub code: &'static str,
    pub message: String,
    pub span: Span,
}

impl Diagnostic {
    pub fn new(code: &'static str, message: impl Into<String>, span: Span) -> Self {
        Self {
            code,
            message: message.into(),
            span,
        }
    }

    pub fn render(&self, path: &str, source: &str) -> String {
        self.render_with_severity("error", path, source)
    }

    /// Renders the diagnostic as `path:line:column: severity[code]: message`.
    pub fn render_with_severity(&self, severity: &str, path: &str, source: &str) -> String {
        let (line, column) = location(source, self.span.start);
        let text = source.lines().nth(line - 1).unwrap_or("");
        let mut output = format!(
            "{path}:{line}:{column}: {severity}[{}]: {}",
            self.code, self.message
        );
        if !text.is_empty() {
            let start = column.saturating_sub(41);
            let snippet: String = text.chars().skip(start).take(120).collect();
            let indent = " ".repeat(column - 1 - start);
            let _ = write!(output, "\n  {line} | {snippet}\n    | {indent}^");
        }
        output
    }

    pub fn json(&self, path: &str, source: &str) -> String {
        self.json_with_severity("error", path, source)
    }

    /// Renders the diagnostic as one JSON object with the given severity.
    pub fn json_with_severity(&self, severity: &str, path: &str, source: &str) -> String {
        let (line, column) = location(source, self.span.start);
        let (end_line, end_column) = location(source, self.span.end);
        format!(
            "{{\"severity\":{},\"code\":{},\"message\":{},\"path\":{},\
             \"span\":{{\"start\":{},\"end\":{},\"line\":{line},\"column\":{column},\
             \"end_line\":{end_line},\"end_column\":{end_column}}}}}",
            json_string(severity),
            json_string(self.code),
            json_string(&self.message),
            json_string(path),
            self.span.start,
            self.span.end,
        )
    }
}

pub fn location(source: &str, offset: usize) -> (usize, usize) {
    let mut offset = offset.min(source.len());
    while !source.is_char_boundary(offset) {
        offset -= 1;
    }
    let prefix = &source[..offset];
    let line = prefix.bytes().filter(|byte| *byte == b'\n').count() + 1;
    let column = prefix.rsplit('\n').next().unwrap_or("").chars().count() + 1;
    (line, column)
}

pub fn json_string(value: &str) -> String {
    let mut result = String::from("\"");
    for character in value.chars() {
        match character {
            '"' => result.push_str("\\\""),
            '\\' => result.push_str("\\\\"),
            '\n' => result.push_str("\\n"),
            '\r' => result.push_str("\\r"),
            '\t' => result.push_str("\\t"),
            c if c <= '\u{1f}' => {
                let _ = write!(result, "\\u{:04x}", c as u32);
            }
            c => result.push(c),
        }
    }
    result.push('"');
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_character_columns_and_escapes_json() {
        assert_eq!(location("// 日本語\nlet x", 18), (2, 6));
        assert_eq!(json_string("a\n\"\\\u{1}"), "\"a\\n\\\"\\\\\\u0001\"");
        let diagnostic = Diagnostic::new("E0001", "bad \"value\"", Span::new(0, 1));
        assert!(diagnostic.json("A.tz", "x").contains("\"column\":1"));
        assert!(diagnostic.render("A.tz", "x").contains("A.tz:1:1"));
        assert!(
            diagnostic
                .json("A.tz", "x")
                .starts_with("{\"severity\":\"error\",\"code\":\"E0001\"")
        );
    }

    #[test]
    fn renders_warnings_with_their_severity() {
        let warning = Diagnostic::new("W1003", "unreachable", Span::new(4, 5));
        let source = "a\nbc d";
        assert_eq!(
            warning.render_with_severity("warning", "M.tz", source),
            "M.tz:2:3: warning[W1003]: unreachable\n  2 | bc d\n    |   ^"
        );
        let json = warning.json_with_severity("warning", "M.tz", source);
        assert!(json.starts_with("{\"severity\":\"warning\",\"code\":\"W1003\""));
        assert!(json.contains("\"path\":\"M.tz\""));
        assert!(json.contains("\"line\":2,\"column\":3"));
    }
}
