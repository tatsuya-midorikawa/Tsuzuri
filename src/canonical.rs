//! A22: canonical forms. The parser records each noncanonical spelling of a source
//! (`Program.noncanonical`); `warnings` reports those of the root package as `W1007` with their
//! rewrites (`--warn canonical`, and `tsuzuri fix --edition`), and `rejections` turns them into
//! `E0004` in an edition that rejects them (2027 on). The two codes have separate kinds (D8).

use crate::check::CheckedModule;
use crate::diagnostic::{Applicability, Diagnostic, DiagnosticKind, Edit};
use crate::syntax::{FixLevel, Noncanonical, NoncanonicalKind};

/// The most findings that one source records; later ones are not recorded.
pub const MAX_FINDINGS: usize = 65_536;

/// The kinds of `W1007` and `E0004`, with their codes.
pub const KINDS: [(&str, &str); 12] = [
    ("bare_range_in_for", "W1007"),
    ("short_literal_suffix", "W1007"),
    ("symbolic_borrow", "W1007"),
    ("symbolic_deref", "W1007"),
    ("symbolic_reference_type", "W1007"),
    ("legacy_lambda", "W1007"),
    ("bare_range_in_for_rejected", "E0004"),
    ("short_literal_suffix_rejected", "E0004"),
    ("symbolic_borrow_rejected", "E0004"),
    ("symbolic_deref_rejected", "E0004"),
    ("symbolic_reference_type_rejected", "E0004"),
    ("legacy_lambda_rejected", "E0004"),
];

fn diagnostic_kind(kind: NoncanonicalKind, rejected: bool) -> DiagnosticKind {
    use NoncanonicalKind as K;
    match (kind, rejected) {
        (K::BareRange, false) => DiagnosticKind::BareRangeInFor,
        (K::ShortSuffix, false) => DiagnosticKind::ShortLiteralSuffix,
        (K::SymbolicBorrow, false) => DiagnosticKind::SymbolicBorrow,
        (K::SymbolicDeref, false) => DiagnosticKind::SymbolicDeref,
        (K::SymbolicReferenceType, false) => DiagnosticKind::SymbolicReferenceType,
        (K::LegacyLambda, false) => DiagnosticKind::LegacyLambda,
        (K::BareRange, true) => DiagnosticKind::BareRangeInForRejected,
        (K::ShortSuffix, true) => DiagnosticKind::ShortLiteralSuffixRejected,
        (K::SymbolicBorrow, true) => DiagnosticKind::SymbolicBorrowRejected,
        (K::SymbolicDeref, true) => DiagnosticKind::SymbolicDerefRejected,
        (K::SymbolicReferenceType, true) => DiagnosticKind::SymbolicReferenceTypeRejected,
        (K::LegacyLambda, true) => DiagnosticKind::LegacyLambdaRejected,
    }
}

fn message(finding: &Noncanonical, rejected: bool) -> String {
    let (spelling, canonical) = (&finding.spelling, &finding.canonical);
    if rejected {
        format!("'{spelling}' is not allowed in edition 2027; write '{canonical}'")
    } else {
        format!("'{spelling}' is not canonical; write '{canonical}'")
    }
}

fn help(finding: &Noncanonical) -> String {
    match finding.kind {
        NoncanonicalKind::BareRange => {
            "write '..=' to include the end or '..<' to exclude it".into()
        }
        NoncanonicalKind::ShortSuffix => {
            "write the type name after the number, as in '86i8u' or '1f32'".into()
        }
        NoncanonicalKind::SymbolicBorrow if finding.level == FixLevel::Unavailable => {
            "a reference to a reference cannot be written with 'ref'; 'ref r' reborrows r, so pass 'r' itself or restructure the code".into()
        }
        NoncanonicalKind::SymbolicBorrow => "write 'ref x' to borrow x".into(),
        NoncanonicalKind::SymbolicDeref => "write 'deref r' to read through the reference r".into(),
        NoncanonicalKind::SymbolicReferenceType => {
            "write 'ref T' for a shared reference and 'ref mut T' for an exclusive one".into()
        }
        NoncanonicalKind::LegacyLambda if finding.level == FixLevel::Unavailable => {
            format!(
                "write the lambda as '{} ...' with all of its parameters",
                finding.canonical
            )
        }
        NoncanonicalKind::LegacyLambda => {
            "start the lambda with a backslash, as in '\\x y -> body'".into()
        }
    }
}

fn title(finding: &Noncanonical) -> String {
    match finding.kind {
        NoncanonicalKind::ShortSuffix => {
            let suffix = crate::lexer::numeric_suffix(&finding.spelling);
            let name = crate::numeric::short_suffix_type(suffix).unwrap_or_default();
            format!("Replace the suffix '{suffix}' with '{name}'")
        }
        _ => format!(
            "Replace '{}' with '{}'",
            finding.spelling, finding.canonical
        ),
    }
}

/// `W1007` for each noncanonical spelling of the root package's sources (`--warn canonical`),
/// with its rewrite, by source and position.
pub fn warnings(module: &CheckedModule) -> Vec<Diagnostic> {
    module
        .noncanonical
        .iter()
        .map(|finding| {
            let diagnostic = Diagnostic::of(
                diagnostic_kind(finding.kind, false),
                message(finding, false),
                finding.span,
            )
            .with_help(help(finding));
            let applicability = match finding.level {
                FixLevel::Certain => Applicability::Certain,
                FixLevel::Likely => Applicability::Likely,
                FixLevel::Unavailable => return diagnostic,
            };
            let edits = finding
                .edits
                .iter()
                .map(|edit| Edit {
                    span: edit.span,
                    text: edit.text.clone(),
                })
                .collect();
            diagnostic.fix(title(finding), applicability, edits)
        })
        .collect()
}

/// `E0004` for each noncanonical spelling of a source that an edition from 2027 on reads, in
/// parse order. They carry no rewrite: the migration rewrites with `W1007` first (D9).
pub(crate) fn rejections(findings: &[Noncanonical]) -> Vec<Diagnostic> {
    findings
        .iter()
        .map(|finding| {
            Diagnostic::of(
                diagnostic_kind(finding.kind, true),
                message(finding, true),
                finding.span,
            )
            .with_help(help(finding))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::Span;

    #[test]
    fn kinds_are_unique_and_short() {
        let mut seen = std::collections::BTreeSet::new();
        for (kind, code) in KINDS {
            assert!(seen.insert(kind), "{kind} twice");
            assert!(
                kind.len() <= 48
                    && kind
                        .bytes()
                        .all(|byte| byte.is_ascii_lowercase() || byte == b'_')
            );
            assert!(matches!(code, "W1007" | "E0004"));
        }
        for kind in [
            NoncanonicalKind::BareRange,
            NoncanonicalKind::ShortSuffix,
            NoncanonicalKind::SymbolicBorrow,
            NoncanonicalKind::SymbolicDeref,
            NoncanonicalKind::SymbolicReferenceType,
            NoncanonicalKind::LegacyLambda,
        ] {
            for rejected in [false, true] {
                let registered = diagnostic_kind(kind, rejected);
                assert!(
                    KINDS.contains(&(registered.id(), registered.code())),
                    "{}",
                    registered.id()
                );
            }
        }
    }

    #[test]
    fn messages_follow_the_templates() {
        let finding = Noncanonical {
            kind: NoncanonicalKind::ShortSuffix,
            span: Span::new(0, 4),
            spelling: "86uy".into(),
            canonical: "86i8u".into(),
            level: FixLevel::Certain,
            edits: Vec::new(),
        };
        assert_eq!(
            message(&finding, false),
            "'86uy' is not canonical; write '86i8u'"
        );
        assert_eq!(
            message(&finding, true),
            "'86uy' is not allowed in edition 2027; write '86i8u'"
        );
        assert_eq!(title(&finding), "Replace the suffix 'uy' with 'i8u'");
        let range = Noncanonical {
            kind: NoncanonicalKind::BareRange,
            spelling: "..".into(),
            canonical: "..=".into(),
            ..finding
        };
        assert_eq!(title(&range), "Replace '..' with '..='");
        assert_eq!(
            help(&range),
            "write '..=' to include the end or '..<' to exclude it"
        );
    }
}
