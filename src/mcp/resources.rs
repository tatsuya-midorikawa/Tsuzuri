//! The resources (G29 "リソース", D-60 names): the guide, the project's brief cards and the
//! explanations of diagnostics, as Markdown read through the same functions as the tools.

use serde_json::{Value, json};

use super::backend::{Cancel, Capabilities, ProjectBackend, ToolError};
use super::tools::Version;

/// A resource that a URI names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Resource {
    GuideCard,
    GuideFull,
    BriefProject,
    Brief(String),
    Explain(String),
}

/// `%XX` decoded; any other `%` is an error.
fn decode(text: &str) -> Result<String, String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == b'%' {
            let hex = bytes
                .get(at + 1..at + 3)
                .and_then(|pair| std::str::from_utf8(pair).ok())
                .and_then(|pair| u8::from_str_radix(pair, 16).ok())
                .filter(|_| bytes[at + 1].is_ascii_hexdigit() && bytes[at + 2].is_ascii_hexdigit());
            let Some(byte) = hex else {
                return Err("invalid percent-encoding in uri".into());
            };
            out.push(byte);
            at += 3;
        } else {
            out.push(bytes[at]);
            at += 1;
        }
    }
    String::from_utf8(out).map_err(|_| "invalid percent-encoding in uri".into())
}

/// `text` percent-encoded: every byte but the unreserved `A-Za-z0-9-._~` as `%XX`.
pub fn encode(text: &str) -> String {
    let mut out = String::new();
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// The resource of `uri`: `Ok(None)` for a URI of no known form, `Err` for a bad encoding.
pub fn parse(uri: &str) -> Result<Option<Resource>, String> {
    Ok(match uri {
        "tz://guide/L0" => Some(Resource::GuideCard),
        "tz://guide/L1" => Some(Resource::GuideFull),
        "tz://kg/brief/L0" => Some(Resource::BriefProject),
        _ => {
            if let Some(id) = uri.strip_prefix("tz://kg/brief/") {
                let id = decode(id)?;
                (!id.is_empty()).then_some(Resource::Brief(id))
            } else if let Some(query) = uri.strip_prefix("tz://explain/") {
                let query = decode(query)?;
                (!query.is_empty()).then_some(Resource::Explain(query))
            } else {
                None
            }
        }
    })
}

fn entry(
    version: Version,
    name: &str,
    title: &str,
    description: &str,
    key: &str,
    uri: &str,
) -> Value {
    let mut value = json!({"name": name, "description": description, "mimeType": "text/markdown"});
    value[key] = json!(uri);
    if version.titles() {
        value["title"] = json!(title);
    }
    value
}

/// The resources of `resources/list` that `caps` offers.
pub fn list(version: Version, caps: Capabilities) -> Vec<Value> {
    let mut list = Vec::new();
    if caps.guide {
        list.push(entry(
            version,
            "guide-l0",
            "Tsuzuri language card",
            "Short guide to the language that matches this compiler.",
            "uri",
            "tz://guide/L0",
        ));
        list.push(entry(
            version,
            "guide-l1",
            "Tsuzuri language guide",
            "Full guide to the language that matches this compiler.",
            "uri",
            "tz://guide/L1",
        ));
    }
    if caps.kg {
        list.push(entry(
            version,
            "brief-l0",
            "Project brief",
            "Summary card of the whole project.",
            "uri",
            "tz://kg/brief/L0",
        ));
    }
    list
}

/// The templates of `resources/templates/list` that `caps` offers.
pub fn templates(version: Version, caps: Capabilities) -> Vec<Value> {
    let mut list = Vec::new();
    if caps.kg {
        list.push(entry(
            version,
            "brief",
            "Brief card of a module, function or type",
            "{id} is a knowledge-graph id, percent-encoded.",
            "uriTemplate",
            "tz://kg/brief/{id}",
        ));
    }
    if caps.explain {
        list.push(entry(
            version,
            "explain",
            "Explanation of a diagnostic",
            "{kind} is a diagnostic code such as E1012 or a kind such as use_after_move.",
            "uriTemplate",
            "tz://explain/{kind}",
        ));
    }
    list
}

/// The text of `uri`, through the functions of the tools.
pub fn read(
    backend: &ProjectBackend,
    uri: &str,
    cancel: &Cancel,
) -> Result<Option<String>, ToolError> {
    use super::backend::{Backend, ToolRequest};
    let caps = backend.capabilities();
    let Ok(Some(resource)) = parse(uri) else {
        return Ok(None);
    };
    let (name, arguments) = match &resource {
        Resource::GuideCard if caps.guide => ("tz_guide", json!({})),
        Resource::GuideFull if caps.guide => ("tz_guide", json!({"topic": "language"})),
        Resource::BriefProject if caps.kg => ("tz_brief", json!({})),
        Resource::Brief(id) if caps.kg => ("tz_brief", json!({"id": id})),
        Resource::Explain(query) if caps.explain => ("tz_explain", json!({"query": query})),
        _ => return Ok(None),
    };
    let request = ToolRequest {
        name,
        arguments: &arguments,
        cancel,
    };
    match backend.call(&request) {
        Ok(output) => Ok(Some(output.text)),
        Err(_) => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uri_decoding_table() {
        assert_eq!(parse("tz://guide/L0"), Ok(Some(Resource::GuideCard)));
        assert_eq!(
            parse("tz://kg/brief/Point%3A%3APoint.hypotenuse"),
            Ok(Some(Resource::Brief("Point::Point.hypotenuse".into())))
        );
        assert_eq!(
            parse("tz://kg/brief/%E6%97%A5"),
            Ok(Some(Resource::Brief("日".into())))
        );
        assert_eq!(
            parse("tz://explain/use_after_move"),
            Ok(Some(Resource::Explain("use_after_move".into())))
        );
        assert!(parse("tz://kg/brief/%").is_err());
        assert!(parse("tz://kg/brief/%G0").is_err());
        assert_eq!(parse("tz://kg/brief/"), Ok(None));
        assert_eq!(parse("tz://nothing"), Ok(None));
        assert_eq!(encode("Point::Point.x"), "Point%3A%3APoint.x");
        assert_eq!(decode(&encode("日 x")).unwrap(), "日 x");
    }
}
