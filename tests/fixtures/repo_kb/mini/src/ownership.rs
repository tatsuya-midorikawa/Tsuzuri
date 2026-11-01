use crate::diagnostic_kinds::DiagnosticKind;

fn read_place(moved: bool) -> Option<Diagnostic> {
    // DiagnosticKind::AssignToImmutablePlace is reported by write_place.
    moved.then(|| Diagnostic::of(DiagnosticKind::UseAfterMove, "use of moved value", 0))
}

fn write_place() -> Diagnostic {
    Diagnostic::of(DiagnosticKind::AssignToImmutablePlace, "cannot assign twice", 0)
}

fn listed() -> usize {
    DiagnosticKind::ALL.len()
}

#[cfg(test)]
mod tests {
    #[test]
    fn reports() {
        assert_eq!(super::read_place(true).unwrap().kind, DiagnosticKind::UseAfterMove);
    }
}
