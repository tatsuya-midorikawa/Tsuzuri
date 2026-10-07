pub mod abi;
pub mod bindings;
pub mod cache;
pub mod check;
pub mod copies;
pub mod diagnostic;
pub mod docgen;
pub mod driver;
pub mod formatter;
pub mod gpu;
pub mod lexer;
pub mod llvm;
pub mod lsp;
pub mod numeric;
pub mod ownership;
pub mod package;
pub mod parser;
mod ranges;
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
    /// The namespace that `path` is relative to: the root package's default
    /// namespace for its files, and empty for dependency and std files, whose
    /// paths already start with their namespace.
    pub namespace: &'a str,
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
            namespace: "",
        })
        .chain(std_sources.iter().map(|(path, text)| SourceInput {
            path,
            text,
            origin: ModuleOrigin::Std,
            namespace: "",
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
                None
            } else {
                Some(name.to_owned())
            };
            parser::parse_with_source_all(input.text, id).and_then(|mut program| {
                program.source_kind = extension.and_then(syntax::SourceKind::from_extension);
                let identity = match name {
                    Some(name) => (name, stdlib::NAMESPACE.to_owned(), false),
                    None => {
                        let declared = program.namespace.as_ref().map(|namespace| &namespace.path);
                        let (key, namespace) =
                            module_identity(input.path, declared, input.namespace).map_err(
                                |mut error| {
                                    error.span.source = Some(id);
                                    vec![error]
                                },
                            )?;
                        let entry = module_name_from_relative(std::path::Path::new(input.path))
                            .is_ok_and(|relative| relative == "Main");
                        (key, namespace, entry)
                    }
                };
                Ok((identity, program))
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
        .map(|(((name, namespace, entry), program), input)| ModuleInput {
            name,
            program,
            origin: input.origin,
            namespace,
            entry: *entry,
        })
        .collect();
    check::check_modules_indexed_all(&modules, semantic)
}

/// Checks `inputs` and fills `index`, keeping only references spelled in the sources.
pub(crate) fn analyze_inputs_semantic(
    inputs: &[SourceInput<'_>],
    index: &mut check::semantic::SemanticIndex,
) -> Result<check::CheckedModule, DiagnosticSet> {
    let checked = analyze_inputs_indexed_all(inputs, Some(&mut *index));
    let texts: Vec<_> = inputs.iter().map(|input| input.text).collect();
    index.retain_spelled(&texts);
    checked
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
            namespace: "",
        })
        .chain(stdlib::SOURCES.iter().map(|(path, text)| SourceInput {
            path,
            text,
            origin: ModuleOrigin::Std,
            namespace: "",
        }))
        .collect();
    let mut index = check::semantic::SemanticIndex::default();
    let module = analyze_inputs_semantic(&inputs, &mut index)?;
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

/// The key and namespace of the user module at `path`. A declared namespace
/// (`A::B`) and the file stem form the module's full name; without a
/// declaration, the namespace is `root_namespace` followed by the directories
/// of `path`. Both stay dotted inside the compiler, which names a module by
/// its key: the full name without a leading `root_namespace`, so files
/// without a declaration keep their path names.
pub(crate) fn module_identity(
    path: &str,
    declared: Option<&syntax::Ident>,
    root_namespace: &str,
) -> Result<(String, String), Diagnostic> {
    let relative = module_name_from_relative(std::path::Path::new(path))?;
    let (directories, stem) = relative.rsplit_once('.').unwrap_or(("", &relative));
    let Some(declared) = declared else {
        let namespace = match (root_namespace.is_empty(), directories.is_empty()) {
            (true, _) => directories.to_owned(),
            (false, true) => root_namespace.to_owned(),
            (false, false) => format!("{root_namespace}.{directories}"),
        };
        return Ok((relative.clone(), namespace));
    };
    let declared = declared.text.replace("::", ".");
    let inner = (!root_namespace.is_empty())
        .then(|| declared.strip_prefix(root_namespace))
        .flatten();
    let key = match inner {
        Some("") => stem.to_owned(),
        Some(rest) if rest.starts_with('.') => format!("{}.{stem}", &rest[1..]),
        _ => format!("{declared}.{stem}"),
    };
    Ok((key, declared))
}

/// The `namespace` and `using` paths at the top of `text`, read from its
/// tokens so that later syntax errors do not hide them.
pub(crate) fn declared_header(text: &str) -> (Option<syntax::Ident>, Vec<syntax::Ident>) {
    use syntax::TokenKind;
    let (tokens, _) = lexer::lex_all(text);
    let broken = |from: usize, to: usize| text[from..to].contains(['\n', '\r']);
    let mut rest = tokens.as_slice();
    let mut header = |keyword: &str| {
        let [first, second, tail @ ..] = rest else {
            return None;
        };
        let (TokenKind::Ident(word), TokenKind::Ident(name)) = (&first.kind, &second.kind) else {
            return None;
        };
        if word != keyword || broken(first.span.end, second.span.start) {
            return None;
        }
        let mut path = syntax::Ident {
            text: name.clone(),
            span: second.span,
            provenance: syntax::Provenance::User,
        };
        rest = tail;
        while let [separator, segment, tail @ ..] = rest
            && separator.kind == TokenKind::PathSep
            && let TokenKind::Ident(segment_text) = &segment.kind
        {
            path.text.push_str("::");
            path.text.push_str(segment_text);
            path.span = path.span.through(segment.span);
            rest = tail;
        }
        Some(path)
    };
    let namespace = header("namespace");
    let mut usings = Vec::new();
    while let Some(using) = header("using") {
        usings.push(using);
    }
    (namespace, usings)
}
