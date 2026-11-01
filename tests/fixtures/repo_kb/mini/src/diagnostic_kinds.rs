//! The kinds of the fixture, in the shape of the compiler's registry.

macro_rules! diagnostic_kinds {
    ($($tokens:tt)*) => {};
}

diagnostic_kinds! {
    since "0.1.0" {
        // E1012
        (UseAfterMove, "use_after_move", "E1012"),
        // (CommentedOut, "commented_out", "E1012"),
        (AssignToImmutablePlace, "assign_to_immutable_place", "E1012"),
        // E1014
        (BorrowConflict, "borrow_conflict", "E1014"),
    }
}

impl DiagnosticKind {
    fn is_warning(self) -> bool {
        self.code().starts_with('W')
    }
}

#[cfg(test)]
mod tests {
    const SAMPLE: [(DiagnosticKind, &str, &str); 1] = [(TestOnly, "test_only", "E1015")];
}
