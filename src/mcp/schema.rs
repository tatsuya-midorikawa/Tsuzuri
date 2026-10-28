//! The subset of JSON Schema that the tool definitions use (G29 "検証器"): the first violation of
//! the arguments, as `{pointer}: {reason}`.

use serde_json::{Map, Value};

pub const SUPPORTED_KEYWORDS: [&str; 16] = [
    "type",
    "properties",
    "required",
    "additionalProperties",
    "items",
    "enum",
    "const",
    "minimum",
    "maximum",
    "minLength",
    "maxLength",
    "minItems",
    "maxItems",
    "description",
    "default",
    "title",
];

fn type_name(name: &str) -> &'static str {
    match name {
        "string" => "a string",
        "integer" => "an integer",
        "number" => "a number",
        "boolean" => "a boolean",
        "object" => "an object",
        "array" => "an array",
        _ => "null",
    }
}

fn has_type(value: &Value, name: &str) -> bool {
    match name {
        "string" => value.is_string(),
        "integer" => value.as_number().is_some_and(|n| n.is_i64() || n.is_u64()),
        "number" => value.is_number(),
        "boolean" => value.is_boolean(),
        "object" => value.is_object(),
        "array" => value.is_array(),
        "null" => value.is_null(),
        _ => false,
    }
}

fn shown(value: &Value) -> String {
    match value {
        Value::String(text) => format!("\"{text}\""),
        other => other.to_string(),
    }
}

fn number(value: &Value) -> String {
    match value.as_number() {
        Some(number) if number.is_i64() || number.is_u64() => number.to_string(),
        Some(number) => number.as_f64().map_or_else(String::new, |f| f.to_string()),
        None => value.to_string(),
    }
}

/// The first violation of `value` against `schema`, or `Ok`. The root's pointer is empty.
pub fn validate(schema: &Value, value: &Value) -> Result<(), String> {
    check(schema, value, "")
}

fn check(schema: &Value, value: &Value, pointer: &str) -> Result<(), String> {
    let fail = |reason: String| Err(format!("{pointer}: {reason}"));
    let types: Vec<&str> = match schema.get("type") {
        Some(Value::String(name)) => vec![name.as_str()],
        Some(Value::Array(names)) => names.iter().filter_map(Value::as_str).collect(),
        _ => Vec::new(),
    };
    if !types.is_empty() && !types.iter().any(|name| has_type(value, name)) {
        let names: Vec<&str> = types.iter().map(|name| type_name(name)).collect();
        return fail(format!("must be {}", names.join(" or ")));
    }
    if let Some(Value::Array(options)) = schema.get("enum")
        && !options.contains(value)
    {
        let options: Vec<String> = options.iter().map(shown).collect();
        return fail(format!("must be one of {}", options.join(", ")));
    }
    if let Some(constant) = schema.get("const")
        && constant != value
    {
        return fail(format!("must be {}", shown(constant)));
    }
    if let Some(actual) = value.as_f64() {
        if let Some(minimum) = schema.get("minimum")
            && minimum.as_f64().is_some_and(|minimum| actual < minimum)
        {
            return fail(format!("must be at least {}", number(minimum)));
        }
        if let Some(maximum) = schema.get("maximum")
            && maximum.as_f64().is_some_and(|maximum| actual > maximum)
        {
            return fail(format!("must be at most {}", number(maximum)));
        }
    }
    if let Some(text) = value.as_str() {
        let length = text.chars().count() as u64;
        if let Some(minimum) = schema.get("minLength").and_then(Value::as_u64)
            && length < minimum
        {
            return fail(format!("must have at least {minimum} characters"));
        }
        if let Some(maximum) = schema.get("maxLength").and_then(Value::as_u64)
            && length > maximum
        {
            return fail(format!("must have at most {maximum} characters"));
        }
    }
    if let Some(items) = value.as_array() {
        let length = items.len() as u64;
        if let Some(minimum) = schema.get("minItems").and_then(Value::as_u64)
            && length < minimum
        {
            return fail(format!("must have at least {minimum} items"));
        }
        if let Some(maximum) = schema.get("maxItems").and_then(Value::as_u64)
            && length > maximum
        {
            return fail(format!("must have at most {maximum} items"));
        }
        if let Some(item_schema) = schema.get("items") {
            for (index, item) in items.iter().enumerate() {
                check(item_schema, item, &format!("{pointer}/{index}"))?;
            }
        }
    }
    if let Some(object) = value.as_object() {
        check_object(schema, object, pointer)?;
    }
    Ok(())
}

fn check_object(schema: &Value, object: &Map<String, Value>, pointer: &str) -> Result<(), String> {
    if let Some(Value::Array(required)) = schema.get("required") {
        for key in required.iter().filter_map(Value::as_str) {
            if !object.contains_key(key) {
                return Err(format!("{pointer}/{key}: is required"));
            }
        }
    }
    let properties = schema.get("properties").and_then(Value::as_object);
    let closed = schema.get("additionalProperties") == Some(&Value::Bool(false));
    // The keys of a `Map` are in byte order.
    for (key, child) in object {
        match properties.and_then(|properties| properties.get(key)) {
            Some(child_schema) => check(child_schema, child, &format!("{pointer}/{key}"))?,
            None if closed => return Err(format!("{pointer}/{key}: unknown property")),
            None => {}
        }
    }
    Ok(())
}

/// The keywords of `schema` (through `properties` and `items`) outside `SUPPORTED_KEYWORDS`.
pub fn keywords_outside_the_subset(schema: &Value) -> Vec<String> {
    let mut found = Vec::new();
    collect(schema, &mut found);
    found.sort();
    found.dedup();
    found
}

fn collect(schema: &Value, found: &mut Vec<String>) {
    let Some(object) = schema.as_object() else {
        return;
    };
    for (key, value) in object {
        if !SUPPORTED_KEYWORDS.contains(&key.as_str()) {
            found.push(key.clone());
        }
        match key.as_str() {
            "properties" => {
                for child in value.as_object().into_iter().flat_map(|map| map.values()) {
                    collect(child, found);
                }
            }
            "items" => collect(value, found),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn schema() -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {"type": "string", "maxLength": 4},
                "budget": {"type": "integer", "minimum": 200, "maximum": 1000},
                "kind": {"type": "string", "enum": ["a", "b"]},
                "list": {"type": "array", "items": {"type": "string", "minLength": 1}, "maxItems": 2},
                "either": {"type": ["object", "null"]}
            },
            "required": ["kind"],
            "additionalProperties": false
        })
    }

    #[test]
    fn validates_types_and_reports_the_first_error() {
        let schema = schema();
        let error = |value: Value| validate(&schema, &value).unwrap_err();
        assert_eq!(
            error(json!({"kind":"a","path":5})),
            "/path: must be a string"
        );
        assert_eq!(
            error(json!({"kind":"a","budget":10})),
            "/budget: must be at least 200"
        );
        assert_eq!(
            error(json!({"kind":"a","budget":1001})),
            "/budget: must be at most 1000"
        );
        assert_eq!(
            error(json!({"kind":"x"})),
            "/kind: must be one of \"a\", \"b\""
        );
        assert_eq!(
            error(json!({"kind":"a","path":"abcde"})),
            "/path: must have at most 4 characters"
        );
        assert_eq!(
            error(json!({"kind":"a","list":["x",""]})),
            "/list/1: must have at least 1 characters"
        );
        assert_eq!(
            error(json!({"kind":"a","list":["x","y","z"]})),
            "/list: must have at most 2 items"
        );
        assert_eq!(
            error(json!({"kind":"a","either":1})),
            "/either: must be an object or null"
        );
        assert_eq!(error(json!([])), ": must be an object");
        assert!(validate(&schema, &json!({"kind":"b","either":null,"budget":200})).is_ok());
    }

    #[test]
    fn integer_rejects_floats() {
        let schema = json!({"type": "integer"});
        assert_eq!(
            validate(&schema, &json!(1.0)).unwrap_err(),
            ": must be an integer"
        );
        assert!(validate(&schema, &json!(u64::MAX)).is_ok());
        assert!(validate(&schema, &json!(i64::MIN)).is_ok());
    }

    #[test]
    fn required_is_checked_before_unknown_keys() {
        let schema = schema();
        assert_eq!(
            validate(&schema, &json!({"zzz": 1})).unwrap_err(),
            "/kind: is required"
        );
        assert_eq!(
            validate(&schema, &json!({"kind": "a", "aaa": 1, "path": 5})).unwrap_err(),
            "/aaa: unknown property"
        );
    }

    #[test]
    fn unknown_keywords_are_reported() {
        let schema =
            json!({"type":"object","properties":{"a":{"type":"string","pattern":"x"}},"oneOf":[]});
        assert_eq!(keywords_outside_the_subset(&schema), ["oneOf", "pattern"]);
        assert!(keywords_outside_the_subset(&self::tests::schema()).is_empty());
    }
}
