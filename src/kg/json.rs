//! The JSON Lines writer of the knowledge graph (G26): keys in the order the caller writes them,
//! no space after `:` and `,`, strings escaped like the diagnostics (`diagnostic::json_string`),
//! and numbers only as non-negative decimal integers.

use crate::diagnostic::json_string;

/// One JSON object, built key by key.
pub(crate) struct Object {
    text: String,
}

impl Object {
    pub(crate) fn new() -> Self {
        Self { text: "{".into() }
    }

    fn key(&mut self, key: &str) {
        if self.text.len() > 1 {
            self.text.push(',');
        }
        self.text.push_str(&json_string(key));
        self.text.push(':');
    }

    pub(crate) fn string(mut self, key: &str, value: &str) -> Self {
        self.key(key);
        self.text.push_str(&json_string(value));
        self
    }

    /// `value` as a string, or `null`.
    pub(crate) fn optional(self, key: &str, value: Option<&str>) -> Self {
        match value {
            Some(value) => self.string(key, value),
            None => self.null(key),
        }
    }

    /// A value that is already JSON.
    pub(crate) fn raw(mut self, key: &str, json: &str) -> Self {
        self.key(key);
        self.text.push_str(json);
        self
    }

    pub(crate) fn uint(mut self, key: &str, value: usize) -> Self {
        self.key(key);
        self.text.push_str(&value.to_string());
        self
    }

    pub(crate) fn boolean(mut self, key: &str, value: bool) -> Self {
        self.key(key);
        self.text.push_str(if value { "true" } else { "false" });
        self
    }

    pub(crate) fn null(mut self, key: &str) -> Self {
        self.key(key);
        self.text.push_str("null");
        self
    }

    /// An array of strings.
    pub(crate) fn strings<S: AsRef<str>>(self, key: &str, values: &[S]) -> Self {
        let json = array(values.iter().map(|value| json_string(value.as_ref())));
        self.raw(key, &json)
    }

    pub(crate) fn finish(mut self) -> String {
        self.text.push('}');
        self.text
    }
}

/// A JSON array of values that are already JSON.
pub(crate) fn array(values: impl IntoIterator<Item = String>) -> String {
    let mut text = String::from("[");
    for (index, value) in values.into_iter().enumerate() {
        if index > 0 {
            text.push(',');
        }
        text.push_str(&value);
    }
    text.push(']');
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_like_diagnostic_json_string() {
        let text = "q\"b\\n\nr\rt\tc\u{1}é→";
        let line = Object::new().string("k", text).finish();
        assert_eq!(line, format!("{{\"k\":{}}}", json_string(text)));
        assert_eq!(line, "{\"k\":\"q\\\"b\\\\n\\nr\\rt\\tc\\u0001é→\"}");
    }

    #[test]
    fn writes_keys_in_the_given_order() {
        let line = Object::new()
            .string("z", "a")
            .uint("a", 7)
            .boolean("m", true)
            .null("b")
            .optional("c", Some("x"))
            .optional("d", None)
            .strings("e", &["p", "q"])
            .raw("f", "[]")
            .finish();
        assert_eq!(
            line,
            r#"{"z":"a","a":7,"m":true,"b":null,"c":"x","d":null,"e":["p","q"],"f":[]}"#
        );
        assert_eq!(Object::new().finish(), "{}");
        assert_eq!(array(Vec::new()), "[]");
    }
}
