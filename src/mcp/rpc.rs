//! JSON-RPC 2.0 over lines (G29 "トランスポートと JSON-RPC"): one message per line, read with a
//! limit, classified in the order of the ticket's table, and written as one line with the keys in
//! byte order.

use std::io::{self, BufRead, Read, Write};

use serde_json::{Map, Value, json};

use super::MAX_LINE_BYTES;

pub const PARSE_ERROR: i64 = -32700;
pub const INVALID_REQUEST: i64 = -32600;
pub const METHOD_NOT_FOUND: i64 = -32601;
pub const INVALID_PARAMS: i64 = -32602;
pub const INTERNAL_ERROR: i64 = -32603;
/// A resource that does not exist (2025-06-18).
pub const RESOURCE_NOT_FOUND: i64 = -32002;
/// The queue is full (implementation-defined).
pub const TOO_MANY_PENDING: i64 = -32000;

#[derive(Debug, PartialEq, Eq)]
pub enum Line {
    Eof,
    Bytes(Vec<u8>),
    TooLong,
}

/// The next non-blank line without its `\n` (and a `\r` before it). A line whose payload (without
/// the `\r\n` or `\n`) is over `MAX_LINE_BYTES` is read to its end and dropped.
pub fn read_line(reader: &mut impl BufRead) -> io::Result<Line> {
    // The payload and both bytes of a `\r\n`.
    let limit = MAX_LINE_BYTES + 2;
    loop {
        let mut buffer = Vec::new();
        let read = Read::take(&mut *reader, limit as u64).read_until(b'\n', &mut buffer)?;
        if read == 0 {
            return Ok(Line::Eof);
        }
        if buffer.last() == Some(&b'\n') {
            buffer.pop();
        } else if buffer.len() == limit {
            // No `\n` in the payload's room and both bytes of a terminator: too long.
            discard_line(reader)?;
            return Ok(Line::TooLong);
        }
        if buffer.last() == Some(&b'\r') {
            buffer.pop();
        }
        if buffer.len() > MAX_LINE_BYTES {
            return Ok(Line::TooLong);
        }
        if !buffer.is_empty() {
            return Ok(Line::Bytes(buffer));
        }
    }
}

/// Reads up to the next `\n` without keeping it.
fn discard_line(reader: &mut impl BufRead) -> io::Result<()> {
    loop {
        let available = reader.fill_buf()?;
        if available.is_empty() {
            return Ok(());
        }
        match available.iter().position(|byte| *byte == b'\n') {
            Some(at) => {
                reader.consume(at + 1);
                return Ok(());
            }
            None => {
                let length = available.len();
                reader.consume(length);
            }
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum Incoming {
    /// `params` is `Null` when absent, an object otherwise.
    Request {
        id: Value,
        method: String,
        params: Value,
    },
    Notification {
        method: String,
        params: Value,
    },
    /// A response of the client, or a notification whose `params` is no object.
    Ignore,
    Invalid {
        id: Value,
        code: i64,
        message: String,
    },
}

fn valid_id(id: &Value) -> bool {
    match id {
        Value::String(_) => true,
        Value::Number(number) => number.is_i64() || number.is_u64(),
        _ => false,
    }
}

fn invalid(id: Value, code: i64, message: impl Into<String>) -> Incoming {
    Incoming::Invalid {
        id,
        code,
        message: message.into(),
    }
}

/// Classifies one line, in the order of the ticket's table.
pub fn parse(line: &[u8]) -> Incoming {
    let Ok(text) = std::str::from_utf8(line) else {
        return invalid(
            Value::Null,
            PARSE_ERROR,
            "parse error: the message is not valid UTF-8",
        );
    };
    let value: Value = match serde_json::from_str(text) {
        Ok(value) => value,
        Err(error) => return invalid(Value::Null, PARSE_ERROR, format!("parse error: {error}")),
    };
    let object = match value {
        Value::Array(_) => {
            return invalid(
                Value::Null,
                INVALID_REQUEST,
                "invalid request: batch requests are not supported",
            );
        }
        Value::Object(object) => object,
        _ => {
            return invalid(
                Value::Null,
                INVALID_REQUEST,
                "invalid request: expected a JSON object",
            );
        }
    };
    let id = object.get("id").cloned();
    let usable_id = id.clone().filter(valid_id).unwrap_or(Value::Null);
    if object.get("jsonrpc") != Some(&Value::String("2.0".into())) {
        return invalid(
            usable_id,
            INVALID_REQUEST,
            "invalid request: jsonrpc must be \"2.0\"",
        );
    }
    if id.as_ref().is_some_and(|id| !valid_id(id)) {
        return invalid(
            Value::Null,
            INVALID_REQUEST,
            "invalid request: id must be a string or an integer",
        );
    }
    let answer = object.contains_key("result") || object.contains_key("error");
    let method = match object.get("method") {
        None if id.is_some() && answer => return Incoming::Ignore,
        Some(Value::String(method)) => method.clone(),
        _ => {
            return invalid(
                usable_id,
                INVALID_REQUEST,
                "invalid request: method must be a string",
            );
        }
    };
    let params = object.get("params").cloned().unwrap_or(Value::Null);
    if !matches!(params, Value::Null | Value::Object(_))
        || object.get("params") == Some(&Value::Null)
    {
        return match id {
            Some(id) => invalid(id, INVALID_PARAMS, "invalid params: expected an object"),
            None => Incoming::Ignore,
        };
    }
    match id {
        Some(id) => Incoming::Request { id, method, params },
        None => Incoming::Notification { method, params },
    }
}

pub fn result_message(id: &Value, result: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}

pub fn error_message(id: &Value, code: i64, message: &str, data: Option<Value>) -> Value {
    let mut error = Map::new();
    error.insert("code".into(), Value::from(code));
    error.insert("message".into(), Value::from(message));
    if let Some(data) = data {
        error.insert("data".into(), data);
    }
    json!({"jsonrpc": "2.0", "id": id, "error": Value::Object(error)})
}

/// One message on one line, flushed.
pub fn write_line(output: &mut impl Write, message: &Value) -> io::Result<()> {
    let mut text = serde_json::to_string(message).map_err(io::Error::other)?;
    text.push('\n');
    output.write_all(text.as_bytes())?;
    output.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn invalid_of(line: &str) -> (Value, i64, String) {
        match parse(line.as_bytes()) {
            Incoming::Invalid { id, code, message } => (id, code, message),
            other => panic!("{line}: {other:?}"),
        }
    }

    #[test]
    fn parses_requests_notifications_and_invalid_messages() {
        assert_eq!(
            parse(br#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#),
            Incoming::Request {
                id: json!(1),
                method: "ping".into(),
                params: Value::Null
            }
        );
        assert_eq!(
            parse(br#"{"jsonrpc":"2.0","method":"notifications/initialized","params":{}}"#),
            Incoming::Notification {
                method: "notifications/initialized".into(),
                params: json!({})
            }
        );
        assert_eq!(
            parse(br#"{"jsonrpc":"2.0","method":"x","params":[1]}"#),
            Incoming::Ignore
        );
        assert_eq!(
            parse(br#"{"jsonrpc":"2.0","id":3,"result":{}}"#),
            Incoming::Ignore
        );
        assert_eq!(invalid_of("\u{0}\u{1}").1, PARSE_ERROR);
        assert!(invalid_of("}").2.starts_with("parse error: "));
        assert_eq!(
            invalid_of("[]"),
            (
                Value::Null,
                INVALID_REQUEST,
                "invalid request: batch requests are not supported".into()
            )
        );
        for line in ["5", "\"x\"", "null"] {
            assert_eq!(
                invalid_of(line).2,
                "invalid request: expected a JSON object",
                "{line}"
            );
        }
        assert_eq!(
            invalid_of(r#"{"jsonrpc":"1.0","id":1,"method":"ping"}"#),
            (
                json!(1),
                INVALID_REQUEST,
                "invalid request: jsonrpc must be \"2.0\"".into()
            )
        );
        for id in ["1.5", "true", "null", "[]", "{}", "18446744073709551616"] {
            let line = format!(r#"{{"jsonrpc":"2.0","id":{id},"method":"ping"}}"#);
            assert_eq!(
                invalid_of(&line),
                (
                    Value::Null,
                    INVALID_REQUEST,
                    "invalid request: id must be a string or an integer".into()
                ),
                "{id}"
            );
        }
        assert_eq!(
            invalid_of(r#"{"jsonrpc":"2.0","id":1}"#).2,
            "invalid request: method must be a string"
        );
        assert_eq!(
            invalid_of(r#"{"jsonrpc":"2.0","id":1,"method":5}"#).2,
            "invalid request: method must be a string"
        );
        assert_eq!(
            invalid_of(r#"{"jsonrpc":"2.0","id":1,"method":"x","params":[]}"#),
            (
                json!(1),
                INVALID_PARAMS,
                "invalid params: expected an object".into()
            )
        );
        let invalid_utf8 = parse(&[0xFF, 0xFE]);
        assert_eq!(
            invalid_utf8,
            invalid(
                Value::Null,
                PARSE_ERROR,
                "parse error: the message is not valid UTF-8"
            )
        );
    }

    #[test]
    fn read_line_splits_strips_cr_and_skips_blank_lines() {
        let mut input = std::io::Cursor::new(b"a\r\n\n\r\nb\nc".to_vec());
        assert_eq!(read_line(&mut input).unwrap(), Line::Bytes(b"a".to_vec()));
        assert_eq!(read_line(&mut input).unwrap(), Line::Bytes(b"b".to_vec()));
        assert_eq!(read_line(&mut input).unwrap(), Line::Bytes(b"c".to_vec()));
        assert_eq!(read_line(&mut input).unwrap(), Line::Eof);
    }

    #[test]
    fn read_line_discards_an_oversized_line_and_resyncs() {
        let mut bytes = vec![b'x'; MAX_LINE_BYTES + 1];
        bytes.extend_from_slice(b"\nok\n");
        let mut exact = vec![b'y'; MAX_LINE_BYTES];
        exact.push(b'\n');
        bytes.extend_from_slice(&exact);
        let mut input = std::io::BufReader::with_capacity(1024, std::io::Cursor::new(bytes));
        assert_eq!(read_line(&mut input).unwrap(), Line::TooLong);
        assert_eq!(read_line(&mut input).unwrap(), Line::Bytes(b"ok".to_vec()));
        assert_eq!(
            read_line(&mut input).unwrap(),
            Line::Bytes(vec![b'y'; MAX_LINE_BYTES])
        );
        assert_eq!(read_line(&mut input).unwrap(), Line::Eof);
    }

    /// The limit is the payload's: `\r\n` and `\n` end lines of the same lengths alike.
    #[test]
    fn read_line_limits_the_payload_with_either_terminator() {
        for terminator in [&b"\n"[..], &b"\r\n"[..]] {
            let mut bytes = vec![b'y'; MAX_LINE_BYTES];
            bytes.extend_from_slice(terminator);
            bytes.extend(vec![b'z'; MAX_LINE_BYTES + 1]);
            bytes.extend_from_slice(terminator);
            bytes.extend_from_slice(b"ok");
            bytes.extend_from_slice(terminator);
            let mut input = std::io::BufReader::with_capacity(1024, std::io::Cursor::new(bytes));
            assert_eq!(
                read_line(&mut input).unwrap(),
                Line::Bytes(vec![b'y'; MAX_LINE_BYTES])
            );
            assert_eq!(read_line(&mut input).unwrap(), Line::TooLong);
            assert_eq!(read_line(&mut input).unwrap(), Line::Bytes(b"ok".to_vec()));
            assert_eq!(read_line(&mut input).unwrap(), Line::Eof);
        }
    }

    #[test]
    fn ids_round_trip_integers_and_strings() {
        for id in [json!(i64::MIN), json!(u64::MAX), json!("x"), json!(0)] {
            let line =
                serde_json::to_string(&json!({"jsonrpc":"2.0","id":id,"method":"ping"})).unwrap();
            let Incoming::Request { id: parsed, .. } = parse(line.as_bytes()) else {
                panic!("{line}");
            };
            assert_eq!(parsed, id);
            let mut out = Vec::new();
            write_line(&mut out, &result_message(&parsed, json!({}))).unwrap();
            let back: Value = serde_json::from_slice(&out).unwrap();
            assert_eq!(back["id"], id);
        }
    }
}
