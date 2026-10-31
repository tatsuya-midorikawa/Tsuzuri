use crate::diagnostic::{Diagnostic, DiagnosticKind, Span, json_string, location};

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
    /// A checked operation overflowed outside every `try` that could catch it.
    Overflow,
    /// A21: a `@requires` line of the called function was false on entry.
    ContractRequires,
    /// A21: an `@ensures` line was false on exit.
    ContractEnsures,
}

impl TrapKind {
    pub const ALL: [Self; 18] = [
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
        Self::Overflow,
        Self::ContractRequires,
        Self::ContractEnsures,
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
            Self::Overflow => {
                "unhandled OverflowException: arithmetic operation resulted in an overflow"
            }
            Self::ContractRequires => "contract requires failed",
            Self::ContractEnsures => "contract ensures failed",
        }
    }

    /// The kind of the `E2005` that reports this trap, for the kinds that have one (A21).
    pub fn diagnostic_kind(self) -> Option<crate::diagnostic::DiagnosticKind> {
        match self {
            Self::ContractRequires => Some(crate::diagnostic::DiagnosticKind::ContractRequires),
            Self::ContractEnsures => Some(crate::diagnostic::DiagnosticKind::ContractEnsures),
            _ => None,
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

    /// The text of the site without the `trap: ` of its message: `<reason> at <path>:<line>:<column>`, or the
    /// reason alone when `sources` cannot place it (a runner without a source map). `Failure.message` (B10).
    pub fn text(&self, sources: &[TrapSource<'_>]) -> String {
        match self.message(sources) {
            Ok(message) => message
                .strip_prefix("trap: ")
                .map_or_else(|| message.clone(), str::to_owned),
            Err(_) => self.kind.description().to_owned(),
        }
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
                Diagnostic::of(
                    DiagnosticKind::InvalidTrapSourceMap,
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

/// The most frames that a crash report holds (`src/runtime/backtrace.c`, B10).
pub const MAX_TRAP_FRAMES: usize = 32;

/// One line of the report file of `src/runtime/backtrace.c`:
/// `tz-trap/1 <site> <slide> <truncated> <address>...`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RawTrapReport {
    pub site: u32,
    /// Where the executable was loaded minus where it was linked.
    pub slide: u64,
    pub truncated: bool,
    /// Return addresses, the innermost first, as the process saw them.
    pub addresses: Vec<u64>,
}

/// A frame of the chain of calls that led to a trap.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrapFrame {
    pub function: Option<String>,
    pub path: Option<String>,
    pub line: Option<u32>,
    pub column: Option<u32>,
    /// The return address minus the slide: the address in the linked executable.
    pub address: u64,
}

/// The `trap` object of an `E2005` (B10): what trapped and the frames that led there.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrapReport {
    /// `TrapKind::description()`, or `"stack overflow"`.
    pub kind: &'static str,
    /// The site of the `.trap.json` side table; 0 for a stack overflow.
    pub site: u32,
    pub frames: Vec<TrapFrame>,
    /// Whether a symbolizer named the frames.
    pub resolved: bool,
    pub truncated: bool,
}

impl TrapReport {
    /// `{"kind":...,"site":...,"frames":[...]}`, with `"truncated":true` last when the chain was cut.
    pub fn json(&self) -> String {
        use std::fmt::Write;
        let mut output = format!(
            "{{\"kind\":{},\"site\":{},\"frames\":[",
            json_string(self.kind),
            self.site
        );
        for (index, frame) in self.frames.iter().enumerate() {
            if index > 0 {
                output.push(',');
            }
            output.push('{');
            if let Some(function) = &frame.function {
                let _ = write!(output, "\"function\":{},", json_string(function));
            }
            if let Some(path) = &frame.path {
                let _ = write!(output, "\"path\":{},", json_string(path));
            }
            if let Some(line) = frame.line {
                let _ = write!(output, "\"line\":{line},");
            }
            if let Some(column) = frame.column {
                let _ = write!(output, "\"column\":{column},");
            }
            let _ = write!(output, "\"address\":\"0x{:x}\"}}", frame.address);
        }
        output.push(']');
        if self.truncated {
            output.push_str(",\"truncated\":true");
        }
        output.push('}');
        output
    }

    /// The lines that follow the quote of a rendered `E2005`, without line breaks: one `  at ...` for each
    /// frame, or one note when no symbolizer named them.
    pub fn lines(&self) -> Vec<String> {
        if self.frames.is_empty() {
            return Vec::new();
        }
        if !self.resolved {
            return vec![
                "  note: frames are not resolved; install llvm-symbolizer or set TSUZURI_LLVM_SYMBOLIZER"
                    .to_owned(),
            ];
        }
        let mut lines: Vec<String> = self
            .frames
            .iter()
            .map(|frame| {
                let function = frame.function.as_deref().unwrap_or("?");
                match (&frame.path, frame.line, frame.column) {
                    (Some(path), Some(line), Some(column)) => {
                        format!("  at {function} ({path}:{line}:{column})")
                    }
                    (Some(path), Some(line), None) => format!("  at {function} ({path}:{line})"),
                    _ => format!("  at {function}"),
                }
            })
            .collect();
        if self.truncated {
            lines.push("  ... (more frames omitted)".to_owned());
        }
        lines
    }
}

fn hexadecimal(field: &str) -> Option<u64> {
    let digits = field.strip_prefix("0x")?;
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    u64::from_str_radix(digits, 16).ok()
}

/// Reads one line of the report file; `None` for any other version or shape.
pub fn parse_report_line(line: &str) -> Option<RawTrapReport> {
    let mut fields = line.trim_end_matches(['\r', '\n']).split(' ');
    if fields.next()? != "tz-trap/1" {
        return None;
    }
    let site = fields.next()?;
    if site.is_empty() || !site.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let site = site.parse().ok()?;
    let slide = hexadecimal(fields.next()?)?;
    let truncated = match fields.next()? {
        "0" => false,
        "1" => true,
        _ => return None,
    };
    let addresses = fields.map(hexadecimal).collect::<Option<Vec<_>>>()?;
    (addresses.len() <= MAX_TRAP_FRAMES).then_some(RawTrapReport {
        site,
        slide,
        truncated,
        addresses,
    })
}

/// Strips the suffixes that cloning and specialization add (`.$mono.2`, `.cold.1`, `.cpu.avx2`), as many as there are.
fn without_clone_suffixes(mut name: &str) -> &str {
    loop {
        let Some((rest, last)) = name.rsplit_once('.') else {
            return name;
        };
        let digits = !last.is_empty() && last.bytes().all(|byte| byte.is_ascii_digit());
        let level = !last.is_empty()
            && last
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_');
        let stripped = digits
            .then(|| {
                rest.strip_suffix(".$mono")
                    .or_else(|| rest.strip_suffix(".cold"))
            })
            .flatten()
            .or_else(|| level.then(|| rest.strip_suffix(".cpu")).flatten());
        match stripped {
            Some(stripped) => name = stripped,
            None => return name,
        }
    }
}

/// The name of a frame for its linkage name (the table of B10 1.3), or `None` for a frame that is not shown:
/// `tz.fn.Main.twice.$mono.2` is `Main.twice`, `tz_entry.cold.1` (an export's entry) is `entry`, and the joints of
/// calls (`tz.apply.`, `tz.env.`, `tz.closure.`), `tsuzuri_main`, `main` and the C runtime are not shown.
pub fn display_name(linkage: &str) -> Option<String> {
    let rest = if let Some(rest) = linkage.strip_prefix("tz.fn.") {
        rest
    } else if let Some(rest) = linkage.strip_prefix("tz_").filter(|rest| !rest.is_empty()) {
        rest
    } else {
        let number = linkage.strip_prefix("tz.specialized.")?;
        return (!number.is_empty() && number.bytes().all(|byte| byte.is_ascii_digit()))
            .then(|| linkage.to_owned());
    };
    let rest = without_clone_suffixes(rest);
    (!rest.is_empty()).then(|| rest.to_owned())
}

/// The frames that the JSON output of `llvm-symbolizer --output-style=JSON --inlining --functions=linkage` names for
/// the addresses of `raw`; `None` when it is not that output. `export` says whether a `tz_<name>` symbol is the entry
/// of an export of the program (the C runtime's own functions are `tz_` too).
pub fn frames_from_symbolizer(
    output: &str,
    raw: &RawTrapReport,
    export: &dyn Fn(&str) -> bool,
) -> Option<Vec<TrapFrame>> {
    let value: serde_json::Value = serde_json::from_str(output).ok()?;
    let entries = value.as_array()?;
    if entries.len() != raw.addresses.len() {
        return None;
    }
    let mut frames: Vec<TrapFrame> = Vec::new();
    let mut cold: Vec<bool> = Vec::new();
    for (entry, address) in entries.iter().zip(&raw.addresses) {
        let symbols = entry
            .get("Symbol")
            .and_then(serde_json::Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or_default();
        for symbol in symbols {
            let linkage = symbol
                .get("FunctionName")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("");
            let Some(name) = display_name(linkage) else {
                continue;
            };
            if linkage.starts_with("tz_") && !export(&name) {
                continue;
            }
            let number = |key: &str| {
                symbol
                    .get(key)
                    .and_then(serde_json::Value::as_u64)
                    .and_then(|value| u32::try_from(value).ok())
                    .filter(|value| *value != 0)
            };
            let path = symbol
                .get("FileName")
                .and_then(serde_json::Value::as_str)
                .filter(|path| !path.is_empty())
                .map(str::to_owned);
            let line = number("Line");
            let located = path.is_some() && line.is_some();
            let frame = TrapFrame {
                function: Some(name),
                path: if located { path } else { None },
                line: if located { line } else { None },
                column: if located { number("Column") } else { None },
                address: address.wrapping_sub(raw.slide),
            };
            let is_cold = linkage.contains(".cold.");
            // A cold part and the rest of the same function are one logical frame: the rest stays.
            if frames.last().and_then(|last| last.function.as_ref()) == frame.function.as_ref()
                && (is_cold || cold.last() == Some(&true))
            {
                if is_cold {
                    continue;
                }
                frames.pop();
                cold.pop();
            }
            frames.push(frame);
            cold.push(is_cold);
        }
    }
    Some(frames)
}

#[cfg(test)]
mod tests {
    use super::*;

    const LINE: &str = "tz-trap/1 2 0x2188000 0 0x1021884e4 0x10218856c 0x1021885c0 0x10218861c 0x102188774 0x10218cd24 0x180d23e80";

    #[test]
    fn parses_a_report_line() {
        let raw = parse_report_line(LINE).unwrap();
        assert_eq!(raw.site, 2);
        assert_eq!(raw.slide, 0x2188000);
        assert!(!raw.truncated);
        assert_eq!(raw.addresses.len(), 7);
        assert_eq!(raw.addresses[0], 0x1021884e4);
        assert_eq!(raw.addresses[6], 0x180d23e80);
        assert_eq!(parse_report_line(&format!("{LINE}\n")), Some(raw));
        let truncated = parse_report_line("tz-trap/1 7 0x0 1").unwrap();
        assert!(truncated.truncated && truncated.addresses.is_empty());
        for bad in [
            "",
            "tz-trap/2 2 0x0 0 0x1",
            "tz-trap/1 x 0x0 0 0x1",
            "tz-trap/1 2 2188000 0 0x1",
            "tz-trap/1 2 0x 0 0x1",
            "tz-trap/1 2 0x0 2 0x1",
            "tz-trap/1 2 0x0 0 0xg1",
            "tz-trap/1 2 0x0 0 0x1  0x2",
            "tz-trap/1 -2 0x0 0 0x1",
        ] {
            assert_eq!(parse_report_line(bad), None, "{bad}");
        }
        let full = format!("tz-trap/1 2 0x0 1{}", " 0x10".repeat(MAX_TRAP_FRAMES));
        assert_eq!(
            parse_report_line(&full).unwrap().addresses.len(),
            MAX_TRAP_FRAMES
        );
        let over = format!("tz-trap/1 2 0x0 1{}", " 0x10".repeat(MAX_TRAP_FRAMES + 1));
        assert_eq!(parse_report_line(&over), None);
    }

    #[test]
    fn display_name_follows_the_table() {
        let shown = |name: &str| display_name(name);
        assert_eq!(shown("tz.fn.Main.level3").as_deref(), Some("Main.level3"));
        assert_eq!(
            shown("tz.fn.Main.twice.$mono.2").as_deref(),
            Some("Main.twice")
        );
        assert_eq!(
            shown("tz.fn.Main.twice.$mono.2.cold.1").as_deref(),
            Some("Main.twice")
        );
        assert_eq!(
            shown("tz.fn.Main.sum.cpu.avx2").as_deref(),
            Some("Main.sum")
        );
        assert_eq!(
            shown("tz.fn.Main.sum.cpu.x86_64_v3").as_deref(),
            Some("Main.sum")
        );
        assert_eq!(shown("tz.fn.$lambda.3").as_deref(), Some("$lambda.3"));
        assert_eq!(
            shown("tz.specialized.12").as_deref(),
            Some("tz.specialized.12")
        );
        assert_eq!(shown("tz.specialized.x"), None);
        assert_eq!(shown("tz_entry").as_deref(), Some("entry"));
        assert_eq!(shown("tz_entry.cold.1").as_deref(), Some("entry"));
        assert_eq!(shown("tz_"), None);
        assert_eq!(shown("tz.fn."), None);
        for hidden in [
            "tz.apply.i64.i64",
            "tz.env.3",
            "tz.closure.4",
            "tsuzuri_main",
            "main",
            "start",
            "tz.trap.report",
            "tz.alloc",
            "tz.string.concat",
            "malloc",
            "",
        ] {
            assert_eq!(shown(hidden), None, "{hidden}");
        }
    }

    fn symbol(name: &str, file: &str, line: u32, column: u32) -> String {
        format!(
            r#"{{"Column":{column},"Discriminator":0,"FileName":"{file}","FunctionName":"{name}","Line":{line},"StartAddress":"0x0","StartFileName":"","StartLine":0}}"#
        )
    }

    fn entry(symbols: &[String]) -> String {
        format!(
            r#"{{"Address":"0x0","ModuleName":"exe","Symbol":[{}]}}"#,
            symbols.join(",")
        )
    }

    #[test]
    fn frames_from_symbolizer_reads_the_json_and_merges_cold_parts() {
        let raw = RawTrapReport {
            site: 2,
            slide: 0x2188000,
            truncated: false,
            addresses: vec![0x1021884e4, 0x10218856c],
        };
        let none = |_: &str| false;
        let output = format!(
            "[{},{}]",
            entry(&[symbol("tz.fn.Main.level3", "", 0, 0)]),
            entry(&[symbol("tz.fn.Main.level2", "./Main.tz", 5, 15)])
        );
        let frames = frames_from_symbolizer(&output, &raw, &none).unwrap();
        assert_eq!(
            frames,
            vec![
                TrapFrame {
                    function: Some("Main.level3".into()),
                    path: None,
                    line: None,
                    column: None,
                    address: 0x1000004e4,
                },
                TrapFrame {
                    function: Some("Main.level2".into()),
                    path: Some("./Main.tz".into()),
                    line: Some(5),
                    column: Some(15),
                    address: 0x10000056c,
                },
            ]
        );
        // The number of entries must be the number of addresses.
        let one = format!("[{}]", entry(&[symbol("tz.fn.Main.level3", "", 0, 0)]));
        assert_eq!(frames_from_symbolizer(&one, &raw, &none), None);
        assert_eq!(frames_from_symbolizer("not json", &raw, &none), None);
        assert_eq!(frames_from_symbolizer("{}", &raw, &none), None);
        // A function and its cold part are one frame, whichever comes first; a recursion stays.
        let merged = format!(
            "[{},{}]",
            entry(&[symbol("tz.fn.Main.foo.cold.1", "", 0, 0)]),
            entry(&[symbol("tz.fn.Main.foo", "", 0, 0)])
        );
        let frames = frames_from_symbolizer(&merged, &raw, &none).unwrap();
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].address, 0x10000056c);
        let merged = format!(
            "[{},{}]",
            entry(&[symbol("tz.fn.Main.foo", "", 0, 0)]),
            entry(&[symbol("tz.fn.Main.foo.cold.1", "", 0, 0)])
        );
        let frames = frames_from_symbolizer(&merged, &raw, &none).unwrap();
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].address, 0x1000004e4);
        let recursion = format!(
            "[{},{}]",
            entry(&[symbol("tz.fn.Main.foo", "", 0, 0)]),
            entry(&[symbol("tz.fn.Main.foo", "", 0, 0)])
        );
        assert_eq!(
            frames_from_symbolizer(&recursion, &raw, &none)
                .unwrap()
                .len(),
            2
        );
        // Inlined functions are frames of their own, the innermost first; hidden names are dropped, and a `tz_`
        // symbol counts only when it is an export of the program.
        let inlined = format!(
            "[{},{}]",
            entry(&[
                symbol("tz.fn.Main.inner", "", 0, 0),
                symbol("tz.fn.Main.outer", "", 0, 0)
            ]),
            entry(&[
                symbol("tz_task_run_item", "", 0, 0),
                symbol("tz_div", "", 0, 0),
                symbol("tsuzuri_main", "", 0, 0)
            ])
        );
        let export = |name: &str| name == "div";
        let names: Vec<_> = frames_from_symbolizer(&inlined, &raw, &export)
            .unwrap()
            .into_iter()
            .map(|frame| frame.function.unwrap())
            .collect();
        assert_eq!(names, ["Main.inner", "Main.outer", "div"]);
        // A stripped executable names nothing: no frame, but the symbolizer ran.
        let stripped = format!("[{},{}]", entry(&[]), entry(&[symbol("", "", 0, 0)]));
        assert_eq!(
            frames_from_symbolizer(&stripped, &raw, &none),
            Some(Vec::new())
        );
    }

    fn named(function: &str, address: u64) -> TrapFrame {
        TrapFrame {
            function: Some(function.into()),
            path: None,
            line: None,
            column: None,
            address,
        }
    }

    #[test]
    fn report_renders_json_and_lines() {
        let mut report = TrapReport {
            kind: "integer division by zero",
            site: 2,
            frames: vec![
                named("Main.level3", 0x1000004e4),
                named("Main.level2", 0x10000056c),
                named("Main.level1", 0x1000005c0),
                named("Main.main", 0x10000061c),
            ],
            resolved: true,
            truncated: false,
        };
        assert_eq!(
            report.json(),
            r#"{"kind":"integer division by zero","site":2,"frames":[{"function":"Main.level3","address":"0x1000004e4"},{"function":"Main.level2","address":"0x10000056c"},{"function":"Main.level1","address":"0x1000005c0"},{"function":"Main.main","address":"0x10000061c"}]}"#
        );
        assert_eq!(
            report.lines(),
            [
                "  at Main.level3",
                "  at Main.level2",
                "  at Main.level1",
                "  at Main.main"
            ]
        );
        report.frames[1].path = Some("./Main.tz".into());
        report.frames[1].line = Some(5);
        report.frames[1].column = Some(15);
        assert_eq!(report.lines()[1], "  at Main.level2 (./Main.tz:5:15)");
        assert!(report.json().contains(
            r#"{"function":"Main.level2","path":"./Main.tz","line":5,"column":15,"address":"0x10000056c"}"#
        ));
        report.truncated = true;
        assert!(report.json().ends_with(r#"],"truncated":true}"#));
        assert_eq!(
            report.lines().last().map(String::as_str),
            Some("  ... (more frames omitted)")
        );
        // Without a symbolizer: addresses only, and one note instead of the frames.
        let raw = TrapReport {
            kind: "index out of bounds",
            site: 4,
            frames: vec![TrapFrame {
                function: None,
                path: None,
                line: None,
                column: None,
                address: 0x10,
            }],
            resolved: false,
            truncated: false,
        };
        assert_eq!(
            raw.json(),
            r#"{"kind":"index out of bounds","site":4,"frames":[{"address":"0x10"}]}"#
        );
        assert_eq!(
            raw.lines(),
            [
                "  note: frames are not resolved; install llvm-symbolizer or set TSUZURI_LLVM_SYMBOLIZER"
            ]
        );
        let empty = TrapReport {
            kind: "stack overflow",
            site: 0,
            frames: Vec::new(),
            resolved: false,
            truncated: false,
        };
        assert_eq!(
            empty.json(),
            r#"{"kind":"stack overflow","site":0,"frames":[]}"#
        );
        assert!(empty.lines().is_empty());
    }
}
