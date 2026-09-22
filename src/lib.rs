pub mod check;
pub mod diagnostic;
pub mod driver;
pub mod lexer;
pub mod llvm;
pub mod numeric;
mod ownership;
pub mod parser;
pub mod syntax;

pub fn analyze(source: &str) -> Result<check::CheckedModule, diagnostic::Diagnostic> {
    analyze_modules(&[("Main", source)])
}

pub fn analyze_modules(
    sources: &[(&str, &str)],
) -> Result<check::CheckedModule, diagnostic::Diagnostic> {
    let programs = sources
        .iter()
        .enumerate()
        .map(|(id, (name, source))| {
            let (name, kind) = name
                .rsplit_once('.')
                .and_then(|(name, extension)| {
                    syntax::SourceKind::from_extension(extension).map(|kind| (name, Some(kind)))
                })
                .unwrap_or((*name, None));
            parser::parse_with_source(source, id).map(|mut program| {
                program.source_kind = kind;
                (name, program)
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let modules: Vec<_> = programs
        .iter()
        .map(|(name, program)| (*name, program))
        .collect();
    check::check_modules(&modules)
}
