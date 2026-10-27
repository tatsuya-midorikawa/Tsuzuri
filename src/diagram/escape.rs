//! The escapes of the labels (G30 "共通の規則" 3 and 4): cut first, then escape, so that an entity
//! code is never cut in two.

/// The ellipsis that ends a cut text.
pub const ELLIPSIS: char = '\u{2026}';

/// `text` on one line, cut to `limit` characters with `…` as the last one.
pub fn cut(text: &str, limit: usize) -> String {
    let line: String = text
        .chars()
        .map(|character| match character {
            '\t' | '\r' | '\n' => ' ',
            other => other,
        })
        .collect();
    if line.chars().count() <= limit {
        return line;
    }
    let mut out: String = line.chars().take(limit.saturating_sub(1)).collect();
    out.push(ELLIPSIS);
    out
}

/// A Mermaid label: cut, then the six characters that break the syntax as entity codes.
pub fn esc(text: &str, limit: usize) -> String {
    let mut out = String::new();
    for character in cut(text, limit).chars() {
        match character {
            '#' => out.push_str("#35;"),
            '"' => out.push_str("#quot;"),
            '<' => out.push_str("#lt;"),
            '>' => out.push_str("#gt;"),
            '&' => out.push_str("#amp;"),
            '`' => out.push_str("#96;"),
            other => out.push(other),
        }
    }
    out
}

/// The label of a state diagram transition: also `:` as `#58;`, which Mermaid reads as the start
/// of another description in the unquoted text after ` : ` (`Own::Main.peek` does not parse).
pub fn transition(text: &str, limit: usize) -> String {
    esc(text, limit).replace(':', "#58;")
}

/// The text of a class diagram member: a bare parenthesis makes Mermaid read a field as a method.
pub fn member(text: &str, limit: usize) -> String {
    esc(text, limit).replace('(', "#40;").replace(')', "#41;")
}

/// A DOT label: cut, then `\` and `"` with a backslash.
pub fn dot_esc(text: &str, limit: usize) -> String {
    cut(text, limit).replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn esc_replaces_the_six_characters() {
        assert_eq!(esc("a#b\"c<d>e&f", 60), "a#35;b#quot;c#lt;d#gt;e#amp;f");
        assert_eq!(esc("`x`", 60), "#96;x#96;");
        assert_eq!(esc("a\tb\nc\rd", 60), "a b c d");
    }

    #[test]
    fn esc_cuts_before_escaping() {
        let text = "#".repeat(61);
        assert_eq!(esc(&text, 60), format!("{}{ELLIPSIS}", "#35;".repeat(59)));
        assert_eq!(esc(&"#".repeat(60), 60), "#35;".repeat(60));
    }

    #[test]
    fn transition_labels_escape_colons() {
        assert_eq!(
            transition("L10 borrow \u{2192} Own::Main.peek", 60),
            "L10 borrow \u{2192} Own#58;#58;Main.peek"
        );
    }

    #[test]
    fn member_escapes_parentheses() {
        assert_eq!(member("f(x) <y>", 60), "f#40;x#41; #lt;y#gt;");
    }

    #[test]
    fn dot_quotes_and_backslashes() {
        assert_eq!(dot_esc("a\"b\\c<d>", 60), "a\\\"b\\\\c<d>");
    }
}
