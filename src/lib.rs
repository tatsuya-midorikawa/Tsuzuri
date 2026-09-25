pub mod check;
pub mod diagnostic;
pub mod driver;
pub mod lexer;
pub mod llvm;
pub mod numeric;
mod ownership;
pub mod parser;
pub mod stdlib;
pub mod syntax;

use check::{ModuleInput, ModuleOrigin};
use diagnostic::{Diagnostic, Span};

/// One program source: a user file named `Name` or `Name.ext`, or a standard
/// library file at a virtual path such as `std/Math.tz`.
#[derive(Clone, Copy, Debug)]
pub struct SourceInput<'a> {
    pub path: &'a str,
    pub text: &'a str,
    pub origin: ModuleOrigin,
}

pub fn analyze(source: &str) -> Result<check::CheckedModule, Diagnostic> {
    analyze_modules(&[("Main", source)])
}

/// Checks user sources together with the embedded standard library.
pub fn analyze_modules(sources: &[(&str, &str)]) -> Result<check::CheckedModule, Diagnostic> {
    analyze_modules_with_std(sources, stdlib::SOURCES)
}

/// Checks user sources with `std_sources`, given as `("std/Name.tz", text)`,
/// in place of the embedded standard library; an empty slice is an empty
/// custom library.
pub fn analyze_modules_with_std(
    sources: &[(&str, &str)],
    std_sources: &[(&str, &str)],
) -> Result<check::CheckedModule, Diagnostic> {
    let inputs: Vec<_> = sources
        .iter()
        .map(|(path, text)| SourceInput {
            path,
            text,
            origin: ModuleOrigin::User,
        })
        .chain(std_sources.iter().map(|(path, text)| SourceInput {
            path,
            text,
            origin: ModuleOrigin::Std,
        }))
        .collect();
    analyze_inputs(&inputs)
}

/// The single parse entry point. Sources are parsed in the given order, so a
/// diagnostic's `Span::source` indexes `inputs`.
pub(crate) fn analyze_inputs(
    inputs: &[SourceInput<'_>],
) -> Result<check::CheckedModule, Diagnostic> {
    let programs = inputs
        .iter()
        .enumerate()
        .map(|(id, input)| {
            let (name, extension) = match input.origin {
                ModuleOrigin::User => input
                    .path
                    .rsplit_once('.')
                    .filter(|(_, extension)| syntax::SourceKind::from_extension(extension).is_some())
                    .map_or((input.path, None), |(name, extension)| (name, Some(extension))),
                ModuleOrigin::Std => {
                    let name = stdlib::module_name(input.path).ok_or_else(|| {
                        Diagnostic::new(
                            "E1011",
                            format!(
                                "invalid standard library path '{}'; std sources are flat files such as 'std/Math.tz'",
                                input.path
                            ),
                            Span::default().in_source(id),
                        )
                    })?;
                    (name, input.path.rsplit_once('.').map(|(_, extension)| extension))
                }
            };
            parser::parse_with_source(input.text, id).map(|mut program| {
                program.source_kind = extension.and_then(syntax::SourceKind::from_extension);
                (name, program)
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let modules: Vec<_> = programs
        .iter()
        .zip(inputs)
        .map(|((name, program), input)| ModuleInput {
            name,
            program,
            origin: input.origin,
        })
        .collect();
    check::check_modules(&modules)
}
