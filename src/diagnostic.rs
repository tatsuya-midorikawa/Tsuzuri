use std::{collections::BTreeMap, fmt::Write};

pub use crate::diagnostic_kinds::DiagnosticKind;

pub const MAX_REPORTED_ERRORS: usize = 50;
pub const MAX_UNIQUE_DIAGNOSTICS: usize = 1000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Error,
    Warning,
}

impl Severity {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiagnosticSet {
    pub diagnostics: Vec<Diagnostic>,
    pub omitted: usize,
    pub omitted_is_lower_bound: bool,
    /// How many diagnostics of each severity and code were collected, the omitted ones included
    /// (up to the collection limit; `omitted_is_lower_bound` says whether it was reached).
    pub counts: BTreeMap<(Severity, &'static str), usize>,
}

impl DiagnosticSet {
    pub fn from_diagnostics(
        diagnostics: impl IntoIterator<Item = Diagnostic>,
        root: usize,
    ) -> Self {
        let mut collected = Diagnostics::new(root);
        collected.extend(diagnostics);
        collected.finish()
    }

    pub fn first(self) -> Option<Diagnostic> {
        self.diagnostics.into_iter().next()
    }

    pub fn is_empty(&self) -> bool {
        self.diagnostics.is_empty()
    }

    pub fn omission_note(&self) -> Option<String> {
        (self.omitted != 0).then(|| {
            format!(
                "{}{} more {} not shown",
                if self.omitted_is_lower_bound {
                    "at least "
                } else {
                    ""
                },
                self.omitted,
                if self
                    .diagnostics
                    .iter()
                    .all(|diagnostic| diagnostic.severity == Severity::Warning)
                {
                    "warnings"
                } else if self
                    .diagnostics
                    .iter()
                    .any(|diagnostic| diagnostic.severity == Severity::Warning)
                {
                    "diagnostics"
                } else {
                    "errors"
                },
            )
        })
    }
}

impl From<Diagnostic> for DiagnosticSet {
    fn from(diagnostic: Diagnostic) -> Self {
        Self::from_diagnostics([diagnostic], 0)
    }
}

type DiagnosticKey = (usize, usize, usize, Severity, &'static str, String);

/// Collect before applying the display cap; duplicate reports never use budget.
pub(crate) struct Diagnostics {
    entries: BTreeMap<DiagnosticKey, Diagnostic>,
    root: usize,
}

impl Diagnostics {
    pub fn new(root: usize) -> Self {
        Self {
            entries: BTreeMap::new(),
            root,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn is_full(&self) -> bool {
        self.entries.len() >= MAX_UNIQUE_DIAGNOSTICS
    }

    pub fn push(&mut self, diagnostic: Diagnostic) {
        if !self.is_full() {
            let span = diagnostic.span;
            self.entries
                .entry((
                    span.source.unwrap_or(self.root),
                    span.start,
                    span.end,
                    diagnostic.severity,
                    diagnostic.code,
                    diagnostic.message.clone(),
                ))
                .or_insert(diagnostic);
        }
    }

    pub fn extend(&mut self, diagnostics: impl IntoIterator<Item = Diagnostic>) {
        for diagnostic in diagnostics {
            self.push(diagnostic);
            if self.is_full() {
                break;
            }
        }
    }

    pub fn check(&self) -> Result<(), Diagnostic> {
        self.entries
            .values()
            .find(|diagnostic| diagnostic.severity == Severity::Error)
            .map_or(Ok(()), |error| Err(error.clone()))
    }

    pub fn finish(self) -> DiagnosticSet {
        let omitted_is_lower_bound = self.is_full();
        let mut counts = BTreeMap::new();
        for diagnostic in self.entries.values() {
            *counts
                .entry((diagnostic.severity, diagnostic.code))
                .or_insert(0) += 1;
        }
        DiagnosticSet {
            counts,
            omitted: self.entries.len().saturating_sub(MAX_REPORTED_ERRORS),
            omitted_is_lower_bound,
            diagnostics: self
                .entries
                .into_values()
                .take(MAX_REPORTED_ERRORS)
                .collect(),
        }
    }

    pub fn into_vec(self) -> Vec<Diagnostic> {
        self.entries.into_values().collect()
    }
}

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

/// How many related locations, fixes, edits of a fix, candidates, and scope entries a
/// diagnostic keeps; the builders drop the rest.
pub const MAX_RELATED: usize = 16;
pub const MAX_FIXES: usize = 8;
pub const MAX_EDITS: usize = 64;
pub const MAX_CANDIDATES: usize = 10;
pub const MAX_SCOPE: usize = 32;

/// Why a related location is shown. The set is closed; `as_str` is the JSON `role`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RelatedRole {
    MovedHere,
    BorrowedHere,
    BorrowUsedHere,
    DeclaredHere,
    ExpectedBecause,
    FirstDefinedHere,
    ViaCall,
    RequiredBy,
    Note,
}

impl RelatedRole {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::MovedHere => "moved_here",
            Self::BorrowedHere => "borrowed_here",
            Self::BorrowUsedHere => "borrow_used_here",
            Self::DeclaredHere => "declared_here",
            Self::ExpectedBecause => "expected_because",
            Self::FirstDefinedHere => "first_defined_here",
            Self::ViaCall => "via_call",
            Self::RequiredBy => "required_by",
            Self::Note => "note",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Related {
    pub role: RelatedRole,
    pub message: String,
    pub span: Span,
}

/// How far a fix can be trusted: `Certain` keeps the meaning, `Likely` is almost always right,
/// `Maybe` is a suggestion that a person or an agent completes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Applicability {
    Certain,
    Likely,
    Maybe,
}

impl Applicability {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Certain => "certain",
            Self::Likely => "likely",
            Self::Maybe => "maybe",
        }
    }
}

/// Replaces the bytes `span.start..span.end` of a source with `text`; an empty span inserts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Edit {
    pub span: Span,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fix {
    pub title: String,
    pub applicability: Applicability,
    pub edits: Vec<Edit>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Candidate {
    pub name: String,
    pub signature: String,
    pub how: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScopeEntry {
    pub name: String,
    pub ty: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Detail {
    pub expected: Option<String>,
    pub found: Option<String>,
    pub candidates: Vec<Candidate>,
    pub scope: Vec<ScopeEntry>,
}

/// Finds the path and text of the source that a `Span::source` index names.
pub type SourceLookup<'a> = dyn Fn(usize) -> Option<(&'a str, &'a str)> + 'a;

/// A diagnostic. The data is boxed so that the many `Result<_, Diagnostic>` temporaries on the
/// checker's recursive paths stay one pointer wide; the fields are read and written through
/// `Deref`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic(Box<DiagnosticData>);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiagnosticData {
    pub severity: Severity,
    pub code: &'static str,
    pub message: String,
    pub span: Span,
    pub kind: Option<&'static str>,
    pub related: Vec<Related>,
    pub help: Option<String>,
    pub fixes: Vec<Fix>,
    pub detail: Detail,
}

impl Diagnostic {
    /// The data of the diagnostic, to move fields out of it.
    pub fn into_data(self) -> DiagnosticData {
        *self.0
    }
}

impl std::ops::Deref for Diagnostic {
    type Target = DiagnosticData;
    fn deref(&self) -> &DiagnosticData {
        &self.0
    }
}

impl std::ops::DerefMut for Diagnostic {
    fn deref_mut(&mut self) -> &mut DiagnosticData {
        &mut self.0
    }
}

impl Diagnostic {
    fn build(
        severity: Severity,
        code: &'static str,
        kind: Option<&'static str>,
        message: String,
        span: Span,
    ) -> Self {
        Self(Box::new(DiagnosticData {
            severity,
            code,
            message,
            span,
            kind,
            related: Vec::new(),
            help: None,
            fixes: Vec::new(),
            detail: Detail::default(),
        }))
    }

    /// A diagnostic of `kind`: the code, the severity, and the kind come from the registry.
    pub fn of(kind: DiagnosticKind, message: impl Into<String>, span: Span) -> Self {
        Self::build(
            kind.severity(),
            kind.code(),
            Some(kind.id()),
            message.into(),
            span,
        )
    }

    #[deprecated(note = "use Diagnostic::of")]
    pub fn new(code: &'static str, message: impl Into<String>, span: Span) -> Self {
        Self::build(Severity::Error, code, None, message.into(), span)
    }

    #[deprecated(note = "use Diagnostic::of")]
    pub fn warning(code: &'static str, message: impl Into<String>, span: Span) -> Self {
        Self::build(Severity::Warning, code, None, message.into(), span)
    }

    /// The same diagnostic reported as `kind`: the severity, the code, and the kind change; the
    /// message, the span, and the other fields stay.
    pub fn recode(mut self, kind: DiagnosticKind) -> Self {
        self.severity = kind.severity();
        self.code = kind.code();
        self.kind = Some(kind.id());
        self
    }

    /// The registered kind of the diagnostic.
    pub fn kind_of(&self) -> Option<DiagnosticKind> {
        self.kind.and_then(DiagnosticKind::from_id)
    }

    pub fn with_kind(mut self, kind: &'static str) -> Self {
        self.kind = Some(kind);
        self
    }

    pub fn related(mut self, role: RelatedRole, message: impl Into<String>, span: Span) -> Self {
        self.add_related(role, message, span);
        self
    }

    pub fn add_related(&mut self, role: RelatedRole, message: impl Into<String>, span: Span) {
        if self.related.len() < MAX_RELATED {
            self.related.push(Related {
                role,
                message: message.into(),
                span,
            });
        }
    }

    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }

    /// Adds a fix with 1 to `MAX_EDITS` edits; a fix with no edits or more is dropped whole,
    /// since applying part of it would change the meaning.
    pub fn fix(
        mut self,
        title: impl Into<String>,
        applicability: Applicability,
        edits: Vec<Edit>,
    ) -> Self {
        if self.fixes.len() < MAX_FIXES && !edits.is_empty() && edits.len() <= MAX_EDITS {
            self.fixes.push(Fix {
                title: title.into(),
                applicability,
                edits,
            });
        }
        self
    }

    pub fn expected_found(mut self, expected: impl Into<String>, found: impl Into<String>) -> Self {
        self.detail.expected = Some(expected.into());
        self.detail.found = Some(found.into());
        self
    }

    pub fn with_candidates(mut self, mut candidates: Vec<Candidate>) -> Self {
        candidates.truncate(MAX_CANDIDATES);
        self.detail.candidates = candidates;
        self
    }

    pub fn with_scope(mut self, mut scope: Vec<ScopeEntry>) -> Self {
        scope.truncate(MAX_SCOPE);
        self.detail.scope = scope;
        self
    }

    pub fn render(&self, path: &str, source: &str) -> String {
        self.render_with_severity(self.severity.as_str(), path, source)
    }

    /// Renders the diagnostic as `path:line:column: severity[code]: message`.
    pub fn render_with_severity(&self, severity: &str, path: &str, source: &str) -> String {
        self.render_with(severity, path, source, &|_| None)
    }

    /// Calls `use_source` with the path and text of the source that `span` is in: the primary
    /// source for a span without a source or in the primary source, `lookup` for any other.
    /// `None` when `lookup` does not know the source.
    fn with_source<T>(
        &self,
        span: Span,
        path: &str,
        source: &str,
        lookup: &SourceLookup<'_>,
        use_source: impl FnOnce(&str, &str) -> T,
    ) -> Option<T> {
        match span.source {
            None => Some(use_source(path, source)),
            Some(_) if span.source == self.span.source => Some(use_source(path, source)),
            Some(index) => lookup(index).map(|(path, source)| use_source(path, source)),
        }
    }

    /// `render_with_severity` plus the related locations, the help, and the fixes. A related
    /// location in a source that `lookup` does not know, and a fix with an edit in one, are left out.
    pub fn render_with(
        &self,
        severity: &str,
        path: &str,
        source: &str,
        lookup: &SourceLookup<'_>,
    ) -> String {
        let (line, column) = location(source, self.span.start);
        let mut output = format!(
            "{path}:{line}:{column}: {severity}[{}]: {}",
            self.code, self.message
        );
        write_quote(&mut output, source, line, column);
        for related in &self.related {
            let note = self.with_source(related.span, path, source, lookup, |path, source| {
                let (line, column) = location(source, related.span.start);
                let mut note = format!(
                    "\n  note: {}\n    --> {path}:{line}:{column}",
                    related.message
                );
                write_quote(&mut note, source, line, column);
                note
            });
            output.extend(note);
        }
        if let Some(help) = &self.help {
            let _ = write!(output, "\n  help: {help}");
        }
        for fix in &self.fixes {
            if self.fix_resolves(fix, path, source, lookup) {
                let _ = write!(
                    output,
                    "\n  fix ({}): {}",
                    fix.applicability.as_str(),
                    fix.title
                );
            }
        }
        output
    }

    fn fix_resolves(&self, fix: &Fix, path: &str, source: &str, lookup: &SourceLookup<'_>) -> bool {
        fix.edits.iter().all(|edit| {
            self.with_source(edit.span, path, source, lookup, |_, _| ())
                .is_some()
        })
    }

    pub fn json(&self, path: &str, source: &str) -> String {
        self.json_with_severity(self.severity.as_str(), path, source)
    }

    /// Renders the diagnostic as one JSON object with the given severity.
    pub fn json_with_severity(&self, severity: &str, path: &str, source: &str) -> String {
        self.json_with(severity, path, source, &|_| None)
    }

    /// `json_with_severity` plus the keys that the diagnostic has values for, in the order
    /// `kind`, `related`, `help`, `expected`, `found`, `candidates`, `scope`, `fixes`.
    pub fn json_with(
        &self,
        severity: &str,
        path: &str,
        source: &str,
        lookup: &SourceLookup<'_>,
    ) -> String {
        let mut json = format!(
            "{{\"severity\":{},\"code\":{},\"message\":{},\"path\":{},\"span\":{}",
            json_string(severity),
            json_string(self.code),
            json_string(&self.message),
            json_string(path),
            span_json(source, self.span),
        );
        if let Some(kind) = self.kind {
            let _ = write!(json, ",\"kind\":{}", json_string(kind));
        }
        let related: Vec<String> = self
            .related
            .iter()
            .filter_map(|related| {
                self.with_source(related.span, path, source, lookup, |path, source| {
                    format!(
                        "{{\"role\":{},\"message\":{},\"path\":{},\"span\":{}}}",
                        json_string(related.role.as_str()),
                        json_string(&related.message),
                        json_string(path),
                        span_json(source, related.span),
                    )
                })
            })
            .collect();
        if !related.is_empty() {
            let _ = write!(json, ",\"related\":[{}]", related.join(","));
        }
        if let Some(help) = &self.help {
            let _ = write!(json, ",\"help\":{}", json_string(help));
        }
        if let Some(expected) = &self.detail.expected {
            let _ = write!(json, ",\"expected\":{}", json_string(expected));
        }
        if let Some(found) = &self.detail.found {
            let _ = write!(json, ",\"found\":{}", json_string(found));
        }
        if !self.detail.candidates.is_empty() {
            let candidates: Vec<String> = self
                .detail
                .candidates
                .iter()
                .map(|candidate| {
                    format!(
                        "{{\"name\":{},\"signature\":{},\"how\":{}}}",
                        json_string(&candidate.name),
                        json_string(&candidate.signature),
                        json_string(&candidate.how),
                    )
                })
                .collect();
            let _ = write!(json, ",\"candidates\":[{}]", candidates.join(","));
        }
        if !self.detail.scope.is_empty() {
            let scope: Vec<String> = self
                .detail
                .scope
                .iter()
                .map(|entry| {
                    format!(
                        "{{\"name\":{},\"type\":{}}}",
                        json_string(&entry.name),
                        json_string(&entry.ty),
                    )
                })
                .collect();
            let _ = write!(json, ",\"scope\":[{}]", scope.join(","));
        }
        let fixes: Vec<String> = self
            .fixes
            .iter()
            .filter(|fix| self.fix_resolves(fix, path, source, lookup))
            .map(|fix| {
                let edits: Vec<String> = fix
                    .edits
                    .iter()
                    .filter_map(|edit| {
                        self.with_source(edit.span, path, source, lookup, |path, source| {
                            format!(
                                "{{\"path\":{},\"span\":{},\"text\":{}}}",
                                json_string(path),
                                span_json(source, edit.span),
                                json_string(&edit.text),
                            )
                        })
                    })
                    .collect();
                format!(
                    "{{\"title\":{},\"applicability\":{},\"edits\":[{}]}}",
                    json_string(&fix.title),
                    json_string(fix.applicability.as_str()),
                    edits.join(","),
                )
            })
            .collect();
        if !fixes.is_empty() {
            let _ = write!(json, ",\"fixes\":[{}]", fixes.join(","));
        }
        json.push('}');
        json
    }
}

/// The text with `edits` applied. The edits are byte ranges of `text`, in any order; two
/// insertions at one offset keep the order they were given in. `None` when an edit is out of
/// range, starts or ends inside a character, or overlaps another.
pub fn apply_edits(text: &str, edits: &[&Edit]) -> Option<String> {
    let mut sorted: Vec<&Edit> = edits.to_vec();
    sorted.sort_by_key(|edit| (edit.span.start, edit.span.end));
    let mut output = String::with_capacity(text.len());
    let mut cursor = 0;
    for edit in sorted {
        let (start, end) = (edit.span.start, edit.span.end);
        if start < cursor
            || end < start
            || end > text.len()
            || !text.is_char_boundary(start)
            || !text.is_char_boundary(end)
        {
            return None;
        }
        output.push_str(&text[cursor..start]);
        output.push_str(&edit.text);
        cursor = end;
    }
    output.push_str(&text[cursor..]);
    Some(output)
}

/// Appends the line of `column` and a caret under it, unless the line is empty.
fn write_quote(output: &mut String, source: &str, line: usize, column: usize) {
    let text = source.lines().nth(line - 1).unwrap_or("");
    if !text.is_empty() {
        let start = column.saturating_sub(41);
        let snippet: String = text.chars().skip(start).take(120).collect();
        let indent = " ".repeat(column - 1 - start);
        let _ = write!(output, "\n  {line} | {snippet}\n    | {indent}^");
    }
}

/// The span object that the JSON diagnostics use: byte offsets, then lines and columns.
fn span_json(source: &str, span: Span) -> String {
    let (line, column) = location(source, span.start);
    let (end_line, end_column) = location(source, span.end);
    format!(
        "{{\"start\":{},\"end\":{},\"line\":{line},\"column\":{column},\
         \"end_line\":{end_line},\"end_column\":{end_column}}}",
        span.start, span.end,
    )
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
        let diagnostic = Diagnostic::of(
            DiagnosticKind::UnexpectedCharacter,
            "bad \"value\"",
            Span::new(0, 1),
        );
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
        let warning = Diagnostic::of(
            DiagnosticKind::UnreachableMatchArm,
            "unreachable",
            Span::new(4, 5),
        );
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

    #[test]
    fn diagnostic_is_one_pointer_wide() {
        let word = std::mem::size_of::<usize>();
        assert_eq!(std::mem::size_of::<Diagnostic>(), word);
        assert_eq!(std::mem::size_of::<Result<(), Diagnostic>>(), word);
        assert_eq!(std::mem::size_of::<Option<Diagnostic>>(), word);
    }

    #[test]
    fn without_extras_the_output_is_unchanged() {
        let source = "a\nbc d";
        let mut warning = Diagnostic::of(
            DiagnosticKind::UnreachableMatchArm,
            "unreachable",
            Span::new(4, 5),
        );
        warning.kind = None;
        assert_eq!(
            warning.render("M.tz", source),
            "M.tz:2:3: warning[W1003]: unreachable\n  2 | bc d\n    |   ^"
        );
        assert_eq!(
            warning.json("M.tz", source),
            "{\"severity\":\"warning\",\"code\":\"W1003\",\"message\":\"unreachable\",\"path\":\"M.tz\",\"span\":{\"start\":4,\"end\":5,\"line\":2,\"column\":3,\"end_line\":2,\"end_column\":4}}"
        );
        let mut error = Diagnostic::of(DiagnosticKind::UnexpectedCharacter, "bad", Span::new(0, 1));
        error.kind = None;
        assert_eq!(
            error.render_with("error", "A.tz", "x", &|_| None),
            "A.tz:1:1: error[E0001]: bad\n  1 | x\n    | ^"
        );
    }

    fn edits(count: usize) -> Vec<Edit> {
        (0..count)
            .map(|_| Edit {
                span: Span::new(0, 0),
                text: String::new(),
            })
            .collect()
    }

    #[test]
    fn builders_keep_the_limits() {
        let span = Span::new(0, 1);
        let mut diagnostic = Diagnostic::of(DiagnosticKind::UnexpectedCharacter, "m", span);
        for index in 0..20 {
            diagnostic = diagnostic.related(RelatedRole::Note, format!("n{index}"), span);
        }
        assert_eq!(diagnostic.related.len(), MAX_RELATED);
        assert_eq!(diagnostic.related[MAX_RELATED - 1].message, "n15");
        let mut diagnostic = Diagnostic::of(DiagnosticKind::UnexpectedCharacter, "m", span);
        for _ in 0..10 {
            diagnostic = diagnostic.fix("t", Applicability::Maybe, edits(1));
        }
        assert_eq!(diagnostic.fixes.len(), MAX_FIXES);
        let diagnostic = Diagnostic::of(DiagnosticKind::UnexpectedCharacter, "m", span)
            .fix("none", Applicability::Maybe, edits(0))
            .fix("too many", Applicability::Maybe, edits(MAX_EDITS + 1))
            .fix("limit", Applicability::Maybe, edits(MAX_EDITS));
        assert_eq!(diagnostic.fixes.len(), 1);
        assert_eq!(diagnostic.fixes[0].title, "limit");
        let candidates = (0..12)
            .map(|index| Candidate {
                name: index.to_string(),
                signature: String::new(),
                how: String::new(),
            })
            .collect();
        let diagnostic = Diagnostic::of(DiagnosticKind::UnexpectedCharacter, "m", span)
            .with_candidates(candidates);
        assert_eq!(diagnostic.detail.candidates.len(), MAX_CANDIDATES);
        let scope = (0..40)
            .map(|index| ScopeEntry {
                name: index.to_string(),
                ty: String::new(),
            })
            .collect();
        let diagnostic =
            Diagnostic::of(DiagnosticKind::UnexpectedCharacter, "m", span).with_scope(scope);
        assert_eq!(diagnostic.detail.scope.len(), MAX_SCOPE);
    }

    #[test]
    fn role_and_applicability_names_are_stable() {
        let roles = [
            (RelatedRole::MovedHere, "moved_here"),
            (RelatedRole::BorrowedHere, "borrowed_here"),
            (RelatedRole::BorrowUsedHere, "borrow_used_here"),
            (RelatedRole::DeclaredHere, "declared_here"),
            (RelatedRole::ExpectedBecause, "expected_because"),
            (RelatedRole::FirstDefinedHere, "first_defined_here"),
            (RelatedRole::ViaCall, "via_call"),
            (RelatedRole::RequiredBy, "required_by"),
            (RelatedRole::Note, "note"),
        ];
        for (role, name) in roles {
            assert_eq!(role.as_str(), name);
        }
        assert_eq!(Applicability::Certain.as_str(), "certain");
        assert_eq!(Applicability::Likely.as_str(), "likely");
        assert_eq!(Applicability::Maybe.as_str(), "maybe");
    }

    #[test]
    fn json_key_order_and_escaping() {
        let source = "let x = 1\n";
        let diagnostic = Diagnostic::of(
            DiagnosticKind::UnexpectedCharacter,
            "a\n\"\\\u{1}",
            Span::new(4, 5),
        )
        .with_kind("k")
        .related(RelatedRole::Note, "r", Span::new(0, 3))
        .with_help("h")
        .expected_found("E", "F")
        .with_candidates(vec![Candidate {
            name: "n".into(),
            signature: "s".into(),
            how: "w".into(),
        }])
        .with_scope(vec![ScopeEntry {
            name: "x".into(),
            ty: "i64".into(),
        }])
        .fix(
            "t",
            Applicability::Certain,
            vec![Edit {
                span: Span::new(9, 9),
                text: "\n".into(),
            }],
        );
        assert_eq!(
            diagnostic.json("M.tz", source),
            r#"{"severity":"error","code":"E0001","message":"a\n\"\\\u0001","path":"M.tz","span":{"start":4,"end":5,"line":1,"column":5,"end_line":1,"end_column":6},"kind":"k","related":[{"role":"note","message":"r","path":"M.tz","span":{"start":0,"end":3,"line":1,"column":1,"end_line":1,"end_column":4}}],"help":"h","expected":"E","found":"F","candidates":[{"name":"n","signature":"s","how":"w"}],"scope":[{"name":"x","type":"i64"}],"fixes":[{"title":"t","applicability":"certain","edits":[{"path":"M.tz","span":{"start":9,"end":9,"line":1,"column":10,"end_line":1,"end_column":10},"text":"\n"}]}]}"#
        );
    }

    #[test]
    fn related_in_another_source_uses_the_lookup() {
        let main = "def first :: i64 = 2\n";
        let other = "\n\ndef first :: i64 = 1\n";
        let diagnostic = Diagnostic::of(
            DiagnosticKind::DuplicateName,
            "dup",
            Span::new(4, 9).in_source(0),
        )
        .related(
            RelatedRole::FirstDefinedHere,
            "first defined here",
            Span::new(6, 11).in_source(1),
        )
        .related(RelatedRole::Note, "same source", Span::new(0, 3));
        let lookup = |index: usize| (index == 1).then_some(("Other.tz", other));
        assert_eq!(
            diagnostic.render_with("error", "Main.tz", main, &lookup),
            "Main.tz:1:5: error[E1001]: dup\n  1 | def first :: i64 = 2\n    |     ^\n  note: first defined here\n    --> Other.tz:3:5\n  3 | def first :: i64 = 1\n    |     ^\n  note: same source\n    --> Main.tz:1:1\n  1 | def first :: i64 = 2\n    | ^"
        );
        let json = diagnostic.json_with("error", "Main.tz", main, &lookup);
        assert!(json.contains(r#""related":[{"role":"first_defined_here","message":"first defined here","path":"Other.tz","span":{"start":6,"end":11,"line":3,"column":5,"end_line":3,"end_column":10}},{"role":"note","message":"same source","path":"Main.tz","span":{"start":0,"end":3,"line":1,"column":1,"end_line":1,"end_column":4}}]"#), "{json}");
    }

    #[test]
    fn unresolved_related_and_fixes_are_left_out() {
        let main = "def first :: i64 = 2\n";
        let elsewhere = Span::new(0, 1).in_source(1);
        let edit = |span| Edit {
            span,
            text: "x".into(),
        };
        let diagnostic = Diagnostic::of(
            DiagnosticKind::DuplicateName,
            "dup",
            Span::new(4, 9).in_source(0),
        )
        .related(RelatedRole::Note, "unknown source", elsewhere)
        .fix("whole", Applicability::Maybe, vec![edit(Span::new(0, 0))])
        .fix(
            "half",
            Applicability::Maybe,
            vec![edit(Span::new(0, 0)), edit(elsewhere)],
        );
        let text = diagnostic.render_with("error", "Main.tz", main, &|_| None);
        assert!(
            !text.contains("note:")
                && text.contains("fix (maybe): whole")
                && !text.contains("half")
        );
        let json = diagnostic.json_with("error", "Main.tz", main, &|_| None);
        assert!(
            !json.contains("\"related\"")
                && json.contains("\"title\":\"whole\"")
                && !json.contains("half")
        );
        // `render` and `json` pass no lookup, so a location in the main source is the only kind they show.
        assert!(
            diagnostic
                .json("Main.tz", main)
                .contains("\"title\":\"whole\"")
        );
    }

    #[test]
    fn apply_edits_orders_and_rejects_overlaps() {
        let edit = |start, end, text: &str| Edit {
            span: Span::new(start, end),
            text: text.into(),
        };
        let text = "abcdef";
        let (a, b, c) = (edit(4, 5, "X"), edit(0, 1, "Y"), edit(2, 2, "Z"));
        assert_eq!(apply_edits(text, &[&a, &b, &c]).as_deref(), Some("YbZcdXf"));
        let (p, q) = (edit(3, 3, "1"), edit(3, 3, "2"));
        assert_eq!(apply_edits(text, &[&p, &q]).as_deref(), Some("abc12def"));
        assert_eq!(apply_edits(text, &[&q, &p]).as_deref(), Some("abc21def"));
        assert_eq!(apply_edits(text, &[&edit(1, 3, ""), &edit(2, 4, "")]), None);
        assert_eq!(apply_edits(text, &[&edit(5, 7, "")]), None);
        assert_eq!(apply_edits("日本", &[&edit(1, 1, "x")]), None);
        assert_eq!(apply_edits(text, &[]).as_deref(), Some(text));
        assert_eq!(
            apply_edits(text, &[&edit(1, 3, "-"), &edit(3, 4, "+")]).as_deref(),
            Some("a-+ef")
        );
    }
}
