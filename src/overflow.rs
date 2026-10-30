//! A23: the report of integer arithmetic that wraps (`--overflow-report`, and `tsuzuri test` by
//! default). Emission checks each `+ - * ** -x` that a range proof cannot show to stay within its
//! type, before the operation itself, and records the sites that wrap: native code writes a line
//! to the standard error the first time a site wraps, and WebAssembly keeps a byte per site that
//! the host reads through `tsuzuri_overflow_count` and `tsuzuri_overflow_hit`.
//!
//! This module carries the report from the driver to emission and back: the sources and whether
//! their arithmetic is reported, the sites that emission numbered, their messages, the WASM side
//! table and the `run --json` lines. It does not depend on LLVM.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::fmt::Write;
use std::rc::Rc;

use crate::diagnostic::{DiagnosticKind, Span, json_string, location};

/// The most sites that one program reports; more is `E1017`.
pub const MAX_SITES: usize = 65_536;
/// The most sites whose report a native program writes; the next site writes `STOPPED_LINE`.
pub const MAX_PRINTED: u32 = 100;
/// The start of each report line of a program's standard error.
pub const REPORT_PREFIX: &str = "warning: integer overflow wrapped at ";
/// The line that a native program writes, once, instead of the report of its 101st site.
pub const STOPPED_LINE: &str = "warning: integer overflow reports stop after 100 sites";

/// The operation of a site.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Operation {
    Add,
    Subtract,
    Multiply,
    Negate,
    /// `x ** y`: one site for every product of its square-and-multiply loop.
    Power,
}

impl Operation {
    /// `add`, `sub`, `mul`, `neg` or `pow`.
    pub fn name(self) -> &'static str {
        match self {
            Self::Add => "add",
            Self::Subtract => "sub",
            Self::Multiply => "mul",
            Self::Negate => "neg",
            Self::Power => "pow",
        }
    }
}

/// One arithmetic site of the report: an operation of one integer type at one span. Specialized
/// copies of a body share their sites. The derived order makes the table deterministic.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Site {
    /// The index of the source, as `Span::source` has it.
    pub source: usize,
    pub start: usize,
    pub end: usize,
    /// The offset that the message names: the operator (`+` of `a + b`, `**`, the `-` of `-x`),
    /// which tells apart sites that start together (`a + b` and `a + b + c`); the start of the
    /// operation when it has no operator token (`Add.add a b`).
    pub anchor: usize,
    pub operation: Operation,
    pub bits: u16,
    pub signed: bool,
}

impl Site {
    /// The name of the site's type: `i32`, `i8u`.
    pub fn type_name(&self) -> String {
        format!("i{}{}", self.bits, if self.signed { "" } else { "u" })
    }

    pub fn span(&self) -> Span {
        Span::new(self.start, self.end).in_source(self.source)
    }

    /// `integer overflow wrapped at ./Main.tz:12:5 (i32 add)`: the path as the build received it,
    /// and the line and column (in characters) of the operator.
    pub fn message(&self, source: &Source) -> String {
        self.message_at(source, location(&source.text, self.anchor))
    }

    /// `message` with the line and column from an index of the source's lines.
    pub fn message_indexed(&self, source: &Source, lines: &LineIndex) -> String {
        self.message_at(source, lines.location(&source.text, self.anchor))
    }

    fn message_at(&self, source: &Source, (line, column): (usize, usize)) -> String {
        format!(
            "integer overflow wrapped at {}:{line}:{column} ({} {})",
            source.path,
            self.type_name(),
            self.operation.name()
        )
    }
}

/// The starts of the lines of a text, for many lookups of a line and column.
#[derive(Clone, Debug)]
pub struct LineIndex {
    starts: Vec<usize>,
}

impl LineIndex {
    pub fn new(text: &str) -> LineIndex {
        let mut starts = vec![0];
        starts.extend(text.match_indices('\n').map(|(at, _)| at + 1));
        LineIndex { starts }
    }

    /// The line and column (in characters, both from 1) of `offset`, as `diagnostic::location`.
    pub fn location(&self, text: &str, offset: usize) -> (usize, usize) {
        let mut offset = offset.min(text.len());
        while !text.is_char_boundary(offset) {
            offset -= 1;
        }
        let line = self.starts.partition_point(|&start| start <= offset);
        let start = self.starts[line - 1];
        (line, text[start..offset].chars().count() + 1)
    }

    /// `diagnostic::span_json` with the lines and columns from this index.
    pub fn span_json(&self, text: &str, span: Span) -> String {
        let (line, column) = self.location(text, span.start);
        let (end_line, end_column) = self.location(text, span.end);
        format!(
            "{{\"start\":{},\"end\":{},\"line\":{line},\"column\":{column},\
             \"end_line\":{end_line},\"end_column\":{end_column}}}",
            span.start, span.end,
        )
    }
}

/// A source file of the program, by the index that `Span::source` uses.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Source {
    pub path: String,
    pub text: String,
    /// Whether the report covers the arithmetic of this file: the root package's own sources.
    pub reported: bool,
}

pub(crate) struct ScopeData {
    sources: Vec<Source>,
    sites: RefCell<Vec<Site>>,
}

impl ScopeData {
    /// Whether the arithmetic of the source `source` is reported.
    pub(crate) fn reports(&self, source: Option<usize>) -> bool {
        source
            .and_then(|source| self.sources.get(source))
            .is_some_and(|source| source.reported)
    }

    pub(crate) fn source(&self, index: usize) -> Option<&Source> {
        self.sources.get(index)
    }

    /// Records the sites of a finished emission, in number order.
    pub(crate) fn publish(&self, sites: Vec<Site>) {
        *self.sites.borrow_mut() = sites;
    }
}

thread_local! {
    static ACTIVE: RefCell<Option<Rc<ScopeData>>> = const { RefCell::new(None) };
}

/// While it is alive, native and WASM IR emission on this thread instruments the arithmetic that
/// the report covers. Dropping it restores the previous scope.
#[must_use = "the report applies only while the scope is alive"]
pub struct Scope {
    data: Rc<ScopeData>,
    previous: Option<Rc<ScopeData>>,
}

impl Scope {
    pub fn enter(sources: Vec<Source>) -> Self {
        let data = Rc::new(ScopeData {
            sources,
            sites: RefCell::new(Vec::new()),
        });
        let previous = ACTIVE.with(|active| active.replace(Some(data.clone())));
        Self { data, previous }
    }

    /// The sites of the most recent emission, in number order.
    pub fn sites(&self) -> Vec<Site> {
        self.data.sites.borrow().clone()
    }

    pub fn sources(&self) -> &[Source] {
        &self.data.sources
    }
}

impl Drop for Scope {
    fn drop(&mut self) {
        let previous = self.previous.take();
        ACTIVE.with(|active| *active.borrow_mut() = previous);
    }
}

/// The scope of this thread, if one is alive.
pub(crate) fn active() -> Option<Rc<ScopeData>> {
    ACTIVE.with(|active| active.borrow().clone())
}

/// The WASM side table: `{"version":1,"sites":[{"id":0,"operation":"add","type":"i32",
/// "path":"./Main.tz","span":{...},"message":"..."}]}` and a newline. A site's `id` is its index,
/// which `tsuzuri_overflow_hit` takes.
pub fn side_table(sites: &[Site], sources: &[Source]) -> String {
    let mut lines = BTreeMap::new();
    let mut output = String::from("{\"version\":1,\"sites\":[");
    for (id, site) in sites.iter().enumerate() {
        let source = &sources[site.source];
        let index = lines
            .entry(site.source)
            .or_insert_with(|| LineIndex::new(&source.text));
        if id > 0 {
            output.push(',');
        }
        let _ = write!(
            output,
            "{{\"id\":{id},\"operation\":{},\"type\":{},\"path\":{},\"span\":{},\"message\":{}}}",
            json_string(site.operation.name()),
            json_string(&site.type_name()),
            json_string(&source.path),
            index.span_json(&source.text, site.span()),
            json_string(&site.message_indexed(source, index)),
        );
    }
    output.push_str("]}\n");
    output
}

/// The report lines of a program's standard error, as messages: without the `warning: ` that
/// starts them. The line that stops the report is one of them.
pub fn report_lines(stderr: &str) -> Vec<&str> {
    stderr
        .lines()
        .filter_map(report)
        .map(|record| &record["warning: ".len()..])
        .collect()
}

/// The report that `line`, without its line break, ends with: from the last `REPORT_PREFIX`, or
/// the line that stops the report. A report starts a line of its own, but the program's own
/// output that did not end its line (`IO.write_error "prefix"`) comes before it on that line.
fn report(line: &str) -> Option<&str> {
    report_bytes(line.as_bytes()).map(|start| &line[start..])
}

/// Where the report that `line` ends with starts, when it is UTF-8 (the bytes before it need
/// not be).
pub fn report_bytes(line: &[u8]) -> Option<usize> {
    let start = if line.ends_with(STOPPED_LINE.as_bytes()) {
        line.len() - STOPPED_LINE.len()
    } else {
        let prefix = REPORT_PREFIX.as_bytes();
        line.windows(prefix.len())
            .rposition(|window| window == prefix)?
    };
    std::str::from_utf8(&line[start..]).ok().map(|_| start)
}

/// The standard error of a program that `run --json` ran, with each report turned into a `W2001`
/// warning on a line of its own: the message without `warning: `, and the path and span of the
/// site whose message it is. The other bytes are kept, UTF-8 or not; output of the program that
/// did not end its line before a report keeps its bytes and gets the line break.
pub fn json_lines(stderr: &[u8], sites: &[Site], sources: &[Source]) -> Vec<u8> {
    // Each site's message and the index of its source's lines, once the first report line is
    // met: a message names the operator, so it names one site.
    let mut by_message: Option<Messages> = None;
    let mut output = Vec::with_capacity(stderr.len());
    for line in stderr.split_inclusive(|byte| *byte == b'\n') {
        let mut content = line.strip_suffix(b"\n").unwrap_or(line);
        while let Some(rest) = content.strip_suffix(b"\r") {
            content = rest;
        }
        let Some(start) = report_bytes(content) else {
            output.extend_from_slice(line);
            continue;
        };
        if start > 0 {
            output.extend_from_slice(&content[..start]);
            output.push(b'\n');
        }
        let text = std::str::from_utf8(&content[start..]).expect("report_bytes checks UTF-8");
        let by_message = by_message.get_or_insert_with(|| messages(sites, sources));
        let message = &text["warning: ".len()..];
        let kind = if text == STOPPED_LINE {
            DiagnosticKind::ToolNotice
        } else {
            DiagnosticKind::IntegerOverflowWrapped
        };
        let mut json = format!(
            "{{\"severity\":\"warning\",\"code\":\"{}\",\"message\":{}",
            kind.code(),
            json_string(message)
        );
        if let Some(site) = by_message.sites.get(message)
            && let Some(source) = sources.get(site.source)
        {
            let index = &by_message.indexes[&site.source];
            let _ = write!(
                json,
                ",\"path\":{},\"span\":{}",
                json_string(&source.path),
                index.span_json(&source.text, site.span())
            );
        }
        let _ = writeln!(json, ",\"kind\":\"{}\"}}", kind.id());
        output.extend_from_slice(json.as_bytes());
    }
    output
}

/// Each site by its message, and the line index of each source that has a site.
struct Messages<'a> {
    sites: HashMap<String, &'a Site>,
    indexes: BTreeMap<usize, LineIndex>,
}

fn messages<'a>(sites: &'a [Site], sources: &[Source]) -> Messages<'a> {
    let mut messages = Messages {
        sites: HashMap::new(),
        indexes: BTreeMap::new(),
    };
    for site in sites {
        let Some(source) = sources.get(site.source) else {
            continue;
        };
        let index = messages
            .indexes
            .entry(site.source)
            .or_insert_with(|| LineIndex::new(&source.text));
        messages
            .sites
            .entry(site.message_indexed(source, index))
            .or_insert(site);
    }
    messages
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(text: &str) -> Source {
        Source {
            path: "./Main.tz".into(),
            text: text.into(),
            reported: true,
        }
    }

    #[test]
    fn message_uses_character_columns() {
        let text = "# 日本語のコメント\nlet 名前 = a + b\n";
        let start = text.find("a + b").unwrap();
        let site = Site {
            source: 0,
            start,
            end: start + 5,
            anchor: start + 2,
            operation: Operation::Add,
            bits: 32,
            signed: true,
        };
        assert_eq!(
            site.message(&source(text)),
            "integer overflow wrapped at ./Main.tz:2:12 (i32 add)"
        );
        let unsigned = Site {
            operation: Operation::Power,
            bits: 8,
            signed: false,
            ..site
        };
        assert_eq!(unsigned.type_name(), "i8u");
        assert!(unsigned.message(&source(text)).ends_with("(i8u pow)"));
    }

    #[test]
    fn side_table_has_the_fixed_key_order() {
        let text = "export def add32 :: i32 -> i32 -> i32 = \\a b -> a + b\n";
        let start = text.find("a + b").unwrap();
        let site = Site {
            source: 0,
            start,
            end: start + 5,
            anchor: start + 2,
            operation: Operation::Add,
            bits: 32,
            signed: true,
        };
        assert_eq!(
            side_table(&[site], &[source(text)]),
            "{\"version\":1,\"sites\":[{\"id\":0,\"operation\":\"add\",\"type\":\"i32\",\"path\":\"./Main.tz\",\"span\":{\"start\":48,\"end\":53,\"line\":1,\"column\":49,\"end_line\":1,\"end_column\":54},\"message\":\"integer overflow wrapped at ./Main.tz:1:51 (i32 add)\"}]}\n"
        );
        assert_eq!(side_table(&[], &[]), "{\"version\":1,\"sites\":[]}\n");
    }

    #[test]
    fn json_lines_matches_messages_and_leaves_other_lines() {
        let text = "export def add32 :: i32 -> i32 -> i32 = \\a b -> a + b\n";
        let start = text.find("a + b").unwrap();
        let site = Site {
            source: 0,
            start,
            end: start + 5,
            anchor: start + 2,
            operation: Operation::Add,
            bits: 32,
            signed: true,
        };
        let stderr = "debug line\nwarning: integer overflow wrapped at ./Main.tz:1:51 (i32 add)\nwarning: integer overflow wrapped at ./Other.tz:3:1 (i8 mul)\nwarning: integer overflow reports stop after 100 sites\ntail";
        let lines = json_lines(stderr.as_bytes(), &[site], &[source(text)]);
        assert_eq!(
            String::from_utf8(lines).unwrap(),
            "debug line\n\
             {\"severity\":\"warning\",\"code\":\"W2001\",\"message\":\"integer overflow wrapped at ./Main.tz:1:51 (i32 add)\",\"path\":\"./Main.tz\",\"span\":{\"start\":48,\"end\":53,\"line\":1,\"column\":49,\"end_line\":1,\"end_column\":54},\"kind\":\"integer_overflow_wrapped\"}\n\
             {\"severity\":\"warning\",\"code\":\"W2001\",\"message\":\"integer overflow wrapped at ./Other.tz:3:1 (i8 mul)\",\"kind\":\"integer_overflow_wrapped\"}\n\
             {\"severity\":\"warning\",\"code\":\"W2001\",\"message\":\"integer overflow reports stop after 100 sites\",\"kind\":\"tool_notice\"}\n\
             tail"
        );
        assert_eq!(
            report_lines(stderr),
            [
                "integer overflow wrapped at ./Main.tz:1:51 (i32 add)",
                "integer overflow wrapped at ./Other.tz:3:1 (i8 mul)",
                "integer overflow reports stop after 100 sites",
            ]
        );
        // The bytes of other lines stay as the program wrote them, UTF-8 or not.
        let raw =
            b"\xff\xfe raw\r\nwarning: integer overflow wrapped at ./Main.tz:1:51 (i32 add)\n\x80";
        let lines = json_lines(raw, &[site], &[source(text)]);
        assert!(lines.starts_with(b"\xff\xfe raw\r\n{\"severity\":\"warning\",\"code\":\"W2001\""));
        assert!(lines.ends_with(b"\"kind\":\"integer_overflow_wrapped\"}\n\x80"));
        assert_eq!(json_lines(b"\xff\n", &[site], &[]), b"\xff\n");
        // Review 3 of #67: output that did not end its line comes before a report on the same
        // line; it keeps its bytes on a line of its own, and the report is still a warning.
        let after = b"prefix \xffwarning: integer overflow wrapped at ./Main.tz:1:51 (i32 add)\r\nnext\nwarning: integer overflow wrapped at ./Main.tz:1:51 (i32 add) and more\n";
        let lines = json_lines(after, &[site], &[source(text)]);
        let warning = "{\"severity\":\"warning\",\"code\":\"W2001\",\"message\":\"integer overflow wrapped at ./Main.tz:1:51 (i32 add)\",\"path\":\"./Main.tz\"";
        assert!(
            lines.starts_with(&[b"prefix \xff\n".as_slice(), warning.as_bytes()].concat()),
            "{}",
            String::from_utf8_lossy(&lines)
        );
        assert!(
            String::from_utf8_lossy(&lines)
                .contains("\"kind\":\"integer_overflow_wrapped\"}\nnext\n{\"severity\""),
            "{}",
            String::from_utf8_lossy(&lines)
        );
        assert_eq!(
            report_lines(
                "x: warning: integer overflow reports stop after 100 sites\nwarning: integer overflow wrapped at nothing"
            ),
            [
                "integer overflow reports stop after 100 sites",
                "integer overflow wrapped at nothing"
            ]
        );
    }

    #[test]
    fn line_index_agrees_with_location() {
        let text = "a\n日本語 b\r\n\n  c";
        let lines = LineIndex::new(text);
        for offset in 0..=text.len() + 2 {
            assert_eq!(
                lines.location(text, offset),
                location(text, offset),
                "{offset}"
            );
            let span = Span::new(offset / 2, offset);
            assert_eq!(
                lines.span_json(text, span),
                crate::diagnostic::span_json(text, span),
                "{offset}"
            );
        }
    }

    #[test]
    fn scope_restores_the_previous_scope() {
        assert!(active().is_none());
        {
            let outer = Scope::enter(vec![source("a")]);
            assert!(active().unwrap().reports(Some(0)));
            {
                let _inner = Scope::enter(Vec::new());
                assert!(!active().unwrap().reports(Some(0)));
            }
            let data = active().unwrap();
            assert!(data.reports(Some(0)) && !data.reports(Some(1)) && !data.reports(None));
            let site = Site {
                source: 0,
                start: 0,
                end: 1,
                anchor: 0,
                operation: Operation::Negate,
                bits: 64,
                signed: true,
            };
            data.publish(vec![site]);
            assert_eq!(outer.sites(), [site]);
            assert_eq!(outer.sources().len(), 1);
        }
        assert!(active().is_none());
    }
}
