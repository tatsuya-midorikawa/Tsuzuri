//! The fast debug backend (PB05): the `-O0` LLVM IR text that `src/llvm*.rs` emits is translated to an
//! object file with Cranelift instead of being compiled by Clang. Every LLVM instruction is mapped to
//! Cranelift IR one by one, so ownership, drops, traps and evaluation order keep the single lowering of
//! `src/llvm*.rs`. A construct that the translator does not know is E2002, never a different meaning.

mod error;
mod func;
mod lex;
mod module;
mod parse;
mod types;

use crate::diagnostic::{Diagnostic, Span};

use self::error::Failure;
use self::module::SymKind;

/// What reading a module found; for tests of the reader.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ReadStats {
    pub functions: usize,
    pub reachable_functions: usize,
    pub globals: usize,
    pub instructions: usize,
}

/// Reads every function of the module without lowering it. It is the translator's front end alone:
/// the tests use it to see that every IR the compiler and the runtime produce is understood.
pub fn read_module(ir: &str) -> Result<ReadStats, Diagnostic> {
    let module = module::scan(ir).map_err(Failure::into_diagnostic)?;
    let reachable = module.reachable();
    let mut cx = func::Cx {
        module: &module,
        types: types::Types::overlay(&module.types, ir),
    };
    let mut stats = ReadStats::default();
    for (id, sym) in module.syms.iter().enumerate() {
        match &sym.kind {
            SymKind::Function {
                header,
                body_end: Some(_),
                ..
            } => {
                stats.functions += 1;
                stats.reachable_functions += usize::from(reachable[id]);
                let func = parse::parse_function(&mut cx, *header)
                    .map_err(|failure| failure.in_function(sym.name).into_diagnostic())?;
                stats.instructions += func.insts.len();
            }
            SymKind::Global { init: Some(_), .. } => stats.globals += 1,
            _ => {}
        }
    }
    Ok(stats)
}

/// Translates the textual LLVM IR of a `-O0` build into the bytes of a relocatable object for the host.
pub fn compile(_ir: &str) -> Result<Vec<u8>, Diagnostic> {
    Err(Diagnostic::new(
        "E2002",
        "the fast backend cannot translate anything yet",
        Span::default(),
    ))
}
