pub mod abi;
pub mod check;
pub mod diagnostic;
pub mod docgen;
pub mod driver;
pub mod formatter;
pub mod lexer;
pub mod llvm;
pub mod lsp;
pub mod numeric;
pub mod ownership;
pub mod parser;
pub mod simd;
pub mod stdlib;
pub mod syntax;
pub mod trap;

use check::{ModuleInput, ModuleOrigin};
use diagnostic::{Diagnostic, DiagnosticSet, Diagnostics, Span};

/// One program source: a user file named `Name` or `Name.ext`, or a standard
/// library file at a virtual path such as `std/Math.tz`.
#[derive(Clone, Copy, Debug)]
pub struct SourceInput<'a> {
    pub path: &'a str,
    pub text: &'a str,
    pub origin: ModuleOrigin,
}

pub fn analyze(source: &str) -> Result<check::CheckedModule, Diagnostic> {
    analyze_all(source).map_err(first_error)
}

pub fn analyze_all(source: &str) -> Result<check::CheckedModule, DiagnosticSet> {
    analyze_modules_all(&[("Main", source)])
}

/// Checks user sources together with the embedded standard library.
pub fn analyze_modules(sources: &[(&str, &str)]) -> Result<check::CheckedModule, Diagnostic> {
    analyze_modules_all(sources).map_err(first_error)
}

pub fn analyze_modules_all(
    sources: &[(&str, &str)],
) -> Result<check::CheckedModule, DiagnosticSet> {
    analyze_modules_with_std_all(sources, stdlib::SOURCES)
}

/// Checks user sources with `std_sources`, given as `("std/Name.tz", text)`,
/// in place of the embedded standard library; an empty slice is an empty
/// custom library.
pub fn analyze_modules_with_std(
    sources: &[(&str, &str)],
    std_sources: &[(&str, &str)],
) -> Result<check::CheckedModule, Diagnostic> {
    analyze_modules_with_std_all(sources, std_sources).map_err(first_error)
}

pub fn analyze_modules_with_std_all(
    sources: &[(&str, &str)],
    std_sources: &[(&str, &str)],
) -> Result<check::CheckedModule, DiagnosticSet> {
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
    analyze_inputs_all(&inputs)
}

pub(crate) fn first_error(errors: DiagnosticSet) -> Diagnostic {
    errors.first().expect("a failed analysis has a diagnostic")
}

/// The single parse entry point. Sources are parsed in the given order, so a
/// diagnostic's `Span::source` indexes `inputs`.
pub(crate) fn analyze_inputs_all(
    inputs: &[SourceInput<'_>],
) -> Result<check::CheckedModule, DiagnosticSet> {
    analyze_inputs_indexed_all(inputs, None)
}

pub(crate) fn analyze_inputs_indexed_all(
    inputs: &[SourceInput<'_>],
    semantic: Option<&mut check::semantic::SemanticIndex>,
) -> Result<check::CheckedModule, DiagnosticSet> {
    let mut programs = Vec::new();
    let mut diagnostics = Diagnostics::new(0);
    for (id, input) in inputs.iter().enumerate() {
        if diagnostics.is_full() {
            break;
        }
        let parsed = (|| {
            let (name, extension) = match input.origin {
                ModuleOrigin::User => input
                    .path
                    .rsplit_once('.')
                    .filter(|(_, extension)| {
                        syntax::SourceKind::from_extension(extension).is_some()
                    })
                    .map_or((input.path, None), |(name, extension)| {
                        (name, Some(extension))
                    }),
                ModuleOrigin::Std => {
                    let name = stdlib::module_name(input.path).ok_or_else(|| {
                        vec![Diagnostic::new(
                            "E1011",
                            format!(
                                "invalid standard library path '{}'; std sources are flat files such as 'std/Math.tz'",
                                input.path
                            ),
                            Span::default().in_source(id),
                        )]
                    })?;
                    (
                        name,
                        input.path.rsplit_once('.').map(|(_, extension)| extension),
                    )
                }
            };
            let name = if input.origin == ModuleOrigin::User {
                module_name_from_relative(std::path::Path::new(input.path)).map_err(
                    |mut error| {
                        error.span.source = Some(id);
                        vec![error]
                    },
                )?
            } else {
                name.to_owned()
            };
            parser::parse_with_source_all(input.text, id).map(|mut program| {
                program.source_kind = extension.and_then(syntax::SourceKind::from_extension);
                (name, program)
            })
        })();
        match parsed {
            Ok(program) => programs.push(program),
            Err(errors) => diagnostics.extend(errors),
        }
    }
    if !diagnostics.is_empty() {
        return Err(diagnostics.finish());
    }
    let modules: Vec<_> = programs
        .iter()
        .zip(inputs)
        .map(|((name, program), input)| ModuleInput {
            name,
            program,
            origin: input.origin,
        })
        .collect();
    check::check_modules_indexed_all(&modules, semantic)
}

pub fn analyze_modules_with_semantics(
    sources: &[(&str, &str)],
) -> Result<(check::CheckedModule, check::semantic::SemanticIndex), DiagnosticSet> {
    let inputs: Vec<_> = sources
        .iter()
        .map(|(path, text)| SourceInput {
            path,
            text,
            origin: ModuleOrigin::User,
        })
        .chain(stdlib::SOURCES.iter().map(|(path, text)| SourceInput {
            path,
            text,
            origin: ModuleOrigin::Std,
        }))
        .collect();
    let mut index = check::semantic::SemanticIndex::default();
    let module = analyze_inputs_indexed_all(&inputs, Some(&mut index))?;
    Ok((module, index))
}

pub(crate) fn module_name_from_relative(path: &std::path::Path) -> Result<String, Diagnostic> {
    use std::path::Component;
    let invalid = || {
        Diagnostic::new(
            "E1011",
            "source paths must be relative ASCII module segments without traversal or dots",
            Span::default(),
        )
    };
    let mut segments = Vec::new();
    let components: Vec<_> = path.components().collect();
    for (index, component) in components.iter().enumerate() {
        let Component::Normal(value) = component else {
            return Err(invalid());
        };
        let value = value.to_str().ok_or_else(invalid)?;
        let value = if index + 1 == components.len() {
            value
                .rsplit_once('.')
                .filter(|(_, extension)| syntax::SourceKind::from_extension(extension).is_some())
                .map_or(value, |(stem, _)| stem)
        } else {
            value
        };
        if value.contains('.') {
            return Err(invalid());
        }
        segments.push(value);
    }
    if segments.is_empty() {
        return Err(invalid());
    }
    Ok(segments.join("."))
}
