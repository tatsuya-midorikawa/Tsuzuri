//! The fast debug backend (PB05): the `-O0` LLVM IR text that `src/llvm*.rs` emits is translated to an
//! object file with Cranelift instead of being compiled by Clang. Every LLVM instruction is mapped to
//! Cranelift IR one by one, so ownership, drops, traps and evaluation order keep the single lowering of
//! `src/llvm*.rs`. A construct that the translator does not know is E2002, never a different meaning.

use crate::diagnostic::{Diagnostic, Span};

/// Translates the textual LLVM IR of a `-O0` build into the bytes of a relocatable object for the host.
pub fn compile(_ir: &str) -> Result<Vec<u8>, Diagnostic> {
    Err(Diagnostic::new(
        "E2002",
        "the fast backend cannot translate anything yet",
        Span::default(),
    ))
}
