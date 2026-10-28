//! The tool definitions (`tools.json`, G29 appendix B with D-60's `tz_path`), how `tools/list`
//! shows them for each protocol version, and how a tool's output becomes a `tools/call` result.

use serde_json::{Value, json};

use super::backend::{Capabilities, ToolOutput};
use super::{MAX_STRUCTURED_BYTES, MAX_TEXT_CHARS};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Version {
    V2024_11_05,
    V2025_03_26,
    V2025_06_18,
}

impl Version {
    pub fn parse(text: &str) -> Option<Version> {
        match text {
            "2024-11-05" => Some(Self::V2024_11_05),
            "2025-03-26" => Some(Self::V2025_03_26),
            "2025-06-18" => Some(Self::V2025_06_18),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::V2024_11_05 => "2024-11-05",
            Self::V2025_03_26 => "2025-03-26",
            Self::V2025_06_18 => "2025-06-18",
        }
    }

    /// `title`, `outputSchema` and `structuredContent`.
    pub fn titles(self) -> bool {
        self >= Self::V2025_06_18
    }

    pub fn annotations(self) -> bool {
        self >= Self::V2025_03_26
    }
}

/// The tools of `tools.json`, in its order.
pub struct Registry {
    tools: Vec<(String, Value)>,
}

impl Registry {
    pub fn load() -> Registry {
        let tools: Vec<Value> =
            serde_json::from_str(include_str!("tools.json")).expect("tools.json is JSON");
        Registry {
            tools: tools
                .into_iter()
                .map(|tool| {
                    let name = tool["name"].as_str().expect("a tool has a name").to_owned();
                    (name, tool)
                })
                .collect(),
        }
    }

    /// The definitions that `caps` offers, as `version` shows them.
    pub fn list(&self, version: Version, caps: Capabilities) -> Vec<Value> {
        self.tools
            .iter()
            .filter(|(name, _)| caps.allows(name))
            .map(|(_, tool)| {
                let mut tool = tool.clone();
                let object = tool.as_object_mut().expect("a tool is an object");
                if !version.titles() {
                    object.remove("title");
                    object.remove("outputSchema");
                }
                if !version.annotations() {
                    object.remove("annotations");
                }
                tool
            })
            .collect()
    }

    /// The definition of `name` when `caps` offers it.
    pub fn get(&self, name: &str, caps: Capabilities) -> Option<&Value> {
        self.tools
            .iter()
            .find(|(tool, _)| tool == name && caps.allows(name))
            .map(|(_, tool)| tool)
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.tools.iter().map(|(name, _)| name.as_str())
    }
}

/// `text` cut to `MAX_TEXT_CHARS` characters with a mark of what was cut, and whether it was cut.
pub fn cap_text(text: String) -> (String, bool) {
    let count = text.chars().count();
    if count <= MAX_TEXT_CHARS {
        return (text, false);
    }
    let mut out: String = text.chars().take(MAX_TEXT_CHARS).collect();
    out.push_str(&format!(
        "\n\u{2026} truncated: {} more characters",
        count - MAX_TEXT_CHARS
    ));
    (out, true)
}

/// A result with `isError: true` and `message` as its text, cut as any result's text is (an
/// argument's name or value can be most of a message).
pub fn error_result(message: &str) -> Value {
    let (text, _) = cap_text(message.to_owned());
    json!({"content": [{"type": "text", "text": text}], "isError": true})
}

/// The `tools/call` result of a tool's output (G29 "結果の組み立て").
pub fn shape(version: Version, output: ToolOutput) -> Value {
    let (text, cut) = cap_text(output.text);
    let mut structured = match output.structured {
        Value::Object(object) => object,
        _ => serde_json::Map::new(),
    };
    let truncated = structured
        .get("truncated")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    structured.insert("truncated".into(), Value::Bool(truncated || cut));
    let structured = Value::Object(structured);
    let size = serde_json::to_string(&structured).map_or(0, |text| text.len());
    if size > MAX_STRUCTURED_BYTES {
        return error_result(&format!(
            "result too large ({size} bytes); narrow the request"
        ));
    }
    if version.titles() {
        json!({"content": [{"type": "text", "text": text}], "structuredContent": structured})
    } else {
        json!({"content": [{"type": "text", "text": text}]})
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::schema;

    #[test]
    fn tools_json_is_well_formed() {
        let registry = Registry::load();
        let mut names: Vec<&str> = registry.names().collect();
        let count = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), count, "unique names");
        for (name, tool) in &registry.tools {
            assert!(name.starts_with("tz_") && name.len() <= 128, "{name}");
            for key in ["inputSchema", "outputSchema"] {
                let schema = &tool[key];
                assert_eq!(schema["type"], "object", "{name} {key}");
                assert!(
                    schema::keywords_outside_the_subset(schema).is_empty(),
                    "{name} {key}: {:?}",
                    schema::keywords_outside_the_subset(schema)
                );
                for required in schema["required"].as_array().into_iter().flatten() {
                    let required = required.as_str().unwrap();
                    assert!(
                        schema["properties"].get(required).is_some(),
                        "{name} {key} {required}"
                    );
                }
            }
            assert_eq!(tool["inputSchema"]["additionalProperties"], false, "{name}");
            assert!(
                tool["outputSchema"]["required"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("truncated")),
                "{name}"
            );
            assert!(Capabilities::all().allows(name), "{name}");
        }
    }

    #[test]
    fn tools_list_stays_small() {
        let tools = Registry::load().list(Version::V2025_06_18, Capabilities::all());
        let line =
            serde_json::to_string(&json!({"jsonrpc": "2.0", "id": 3, "result": {"tools": tools}}))
                .unwrap();
        assert!(line.len() < 16_384, "{}", line.len());
    }

    #[test]
    fn list_downgrades_by_version() {
        let registry = Registry::load();
        let caps = Capabilities::all();
        let newest = registry.list(Version::V2025_06_18, caps);
        assert!(newest.iter().all(|tool| tool.get("outputSchema").is_some()));
        let middle = registry.list(Version::V2025_03_26, caps);
        assert!(middle.iter().all(|tool| tool.get("title").is_none()
            && tool.get("outputSchema").is_none()
            && tool.get("annotations").is_some()));
        let oldest = registry.list(Version::V2024_11_05, caps);
        assert!(oldest.iter().all(|tool| tool.get("annotations").is_none()));
        let only_check = registry.list(
            Version::V2025_06_18,
            Capabilities {
                check: true,
                ..Capabilities::none()
            },
        );
        assert_eq!(only_check.len(), 1);
        assert_eq!(only_check[0]["name"], "tz_check");
    }

    #[test]
    fn error_results_are_capped() {
        let result = error_result(&"x".repeat(MAX_TEXT_CHARS + 10));
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(text.ends_with("\n\u{2026} truncated: 10 more characters"));
        assert_eq!(result["isError"], true);
        assert_eq!(error_result("short")["content"][0]["text"], "short");
    }

    #[test]
    fn cap_text_cuts_at_a_character_boundary() {
        let (text, cut) = cap_text("日".repeat(MAX_TEXT_CHARS + 3));
        assert!(cut);
        assert_eq!(
            text,
            format!(
                "{}\n\u{2026} truncated: 3 more characters",
                "日".repeat(MAX_TEXT_CHARS)
            )
        );
        assert_eq!(cap_text("a".into()), ("a".into(), false));
    }

    #[test]
    fn shape_adds_truncated_and_enforces_the_structured_limit() {
        let output = ToolOutput {
            text: "t".into(),
            structured: json!({"a": 1}),
        };
        assert_eq!(
            shape(Version::V2025_06_18, output.clone()),
            json!({"content": [{"type": "text", "text": "t"}], "structuredContent": {"a": 1, "truncated": false}})
        );
        assert_eq!(
            shape(Version::V2025_03_26, output),
            json!({"content": [{"type": "text", "text": "t"}]})
        );
        let large = ToolOutput {
            text: "t".into(),
            structured: json!({"blob": "x".repeat(MAX_STRUCTURED_BYTES)}),
        };
        let result = shape(Version::V2025_06_18, large);
        assert_eq!(result["isError"], true);
        assert!(
            result["content"][0]["text"]
                .as_str()
                .unwrap()
                .starts_with("result too large (")
        );
    }
}
