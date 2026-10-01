use crate::diagnostic::{Diagnostic, Span, json_string, location};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u32)]
pub enum TrapKind {
    Assert,
    IntegerDivisionByZero,
    IntegerDivisionOverflow,
    BoundsCheck,
    AllocationSize,
    AllocationFailure,
    RangeStepZero,
    MatchFailure,
    PatternMismatch,
    StringConcatOverflow,
    NumericRuntime,
    WasmRuntime,
    Encoding,
    DebugOutput,
    StackOverflow,
}

impl TrapKind {
    pub const ALL: [Self; 15] = [
        Self::Assert,
        Self::IntegerDivisionByZero,
        Self::IntegerDivisionOverflow,
        Self::BoundsCheck,
        Self::AllocationSize,
        Self::AllocationFailure,
        Self::RangeStepZero,
        Self::MatchFailure,
        Self::PatternMismatch,
        Self::StringConcatOverflow,
        Self::NumericRuntime,
        Self::WasmRuntime,
        Self::Encoding,
        Self::DebugOutput,
        Self::StackOverflow,
    ];

    pub fn description(self) -> &'static str {
        match self {
            Self::Assert => "assertion failed",
            Self::IntegerDivisionByZero => "integer division by zero",
            Self::IntegerDivisionOverflow => "integer division overflow",
            Self::BoundsCheck => "index out of bounds",
            Self::AllocationSize => "allocation size overflow",
            Self::AllocationFailure => "allocation failed",
            Self::RangeStepZero => "range step is zero",
            Self::MatchFailure => "non-exhaustive match",
            Self::PatternMismatch => "pattern mismatch",
            Self::StringConcatOverflow => "string length overflow",
            Self::NumericRuntime => "numeric runtime trap",
            Self::WasmRuntime => "wasm runtime trap",
            Self::Encoding => "invalid Unicode encoding",
            Self::DebugOutput => "debug output failed",
            Self::StackOverflow => "stack overflow",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct TrapSource<'a> {
    pub path: &'a str,
    pub text: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrapSite {
    pub id: u32,
    pub kind: TrapKind,
    pub span: Span,
}

impl TrapSite {
    pub fn message(&self, sources: &[TrapSource<'_>]) -> Result<String, Diagnostic> {
        let source = self.source(sources)?;
        let (line, column) = location(source.text, self.span.start);
        Ok(format!(
            "trap: {} at {}:{line}:{column}",
            self.kind.description(),
            source.path
        ))
    }

    fn source<'a>(&self, sources: &'a [TrapSource<'a>]) -> Result<&'a TrapSource<'a>, Diagnostic> {
        sources
            .get(self.span.source.unwrap_or(0))
            .filter(|source| {
                self.span.start <= self.span.end
                    && self.span.end <= source.text.len()
                    && source.text.is_char_boundary(self.span.start)
                    && source.text.is_char_boundary(self.span.end)
            })
            .ok_or_else(|| {
                Diagnostic::new(
                    "E2000",
                    "trap information requires a valid source map",
                    self.span,
                )
            })
    }
}

pub fn side_table(sites: &[TrapSite], sources: &[TrapSource<'_>]) -> Result<String, Diagnostic> {
    use std::fmt::Write;
    let mut output = String::from("{\"version\":1,\"sites\":[");
    for (index, site) in sites.iter().enumerate() {
        let source = site.source(sources)?;
        let (line, column) = location(source.text, site.span.start);
        let (end_line, end_column) = location(source.text, site.span.end);
        if index > 0 {
            output.push(',');
        }
        write!(output, "{{\"id\":{},\"kind\":{},\"path\":{},\"span\":{{\"start\":{},\"end\":{},\"line\":{line},\"column\":{column},\"end_line\":{end_line},\"end_column\":{end_column}}}}}", site.id, json_string(site.kind.description()), json_string(source.path), site.span.start, site.span.end).unwrap();
    }
    output.push_str("]}\n");
    Ok(output)
}
