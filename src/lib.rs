pub mod abi;
pub mod api_diff;
pub mod bindgen;
pub mod bindings;
pub mod cache;
pub mod check;
pub mod copies;
pub mod coverage;
pub mod diagnostic;
pub mod docgen;
pub mod driver;
pub mod fetch;
pub mod formatter;
pub(crate) mod frontend_cache;
pub mod gpu;
pub mod gpu_devices;
pub mod lexer;
pub mod llvm;
pub mod lsp;
pub mod numeric;
pub mod ownership;
pub mod package;
mod parallel;
pub mod parser;
mod ranges;
pub mod repl;
mod runtime_cache;
pub mod simd;
pub mod stdlib;
pub mod syntax;
pub(crate) mod syntax_codec;
pub mod target;
pub mod timings;
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
    /// The edition that `text` is read with (G19): the edition of its package's manifest, and
    /// `Edition::LATEST` for a source that no manifest governs.
    pub edition: syntax::Edition,
    /// The dependency package that owns the source. `None` for the root package, which no
    /// package depends on, for sources without a manifest, and for std. W1005 reports a use of
    /// a deprecated declaration only when it is in another package, and only a declaration of
    /// a dependency can be used by another package.
    pub package: Option<&'a package::PackageId>,
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
    let std_sources = stdlib::sources_for(sources.iter().map(|(_, text)| *text));
    analyze_modules_with_std_all(sources, &std_sources)
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

/// The entry points without a manifest (`analyze*`) read every source, user and std, with the
/// latest edition, and no source belongs to a package.
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
            edition: syntax::Edition::LATEST,
            package: None,
        })
        .chain(std_sources.iter().map(|(path, text)| SourceInput {
            path,
            text,
            origin: ModuleOrigin::Std,
            namespace: "",
            edition: syntax::Edition::LATEST,
            package: None,
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
    analyze_inputs_with(inputs, semantic, None, parallel::Parallelism::library())
}

/// Reports an invalid `TSUZURI_THREADS`, the bound on the threads that the compiler itself uses
/// to analyze a program. The commands of the command line call it before they analyze anything;
/// the analyses that a library user starts use one thread unless the variable asks for more.
pub fn validate_thread_setting() -> Result<(), Diagnostic> {
    parallel::Parallelism::validate_environment()
}

/// Parses `inputs` in order, through a project's frontend cache when `cache` is given, and
/// checks them. The cache changes neither the programs nor the diagnostics: parsing stops
/// and reports in the same order, and the manifest is recorded only after every parse succeeds.
/// `parallelism` bounds the threads of the stages that run in parallel; it changes neither the
/// result nor the order and content of the diagnostics.
pub(crate) fn analyze_inputs_with(
    inputs: &[SourceInput<'_>],
    semantic: Option<&mut check::semantic::SemanticIndex>,
    mut cache: Option<&mut frontend_cache::FrontendCache>,
    parallelism: parallel::Parallelism,
) -> Result<check::CheckedModule, DiagnosticSet> {
    let mut programs = Vec::new();
    let mut diagnostics = Diagnostics::new(0);
    let parse_time = timings::Phase::start("parse");
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
            let parsed = match cache.as_mut() {
                Some(cache) => cache
                    .parse(input.text, id, input.edition)
                    .map(|parsed| (parsed.program, Some((parsed.key, parsed.interface)))),
                None => parser::parse_with_source_all(input.text, id, input.edition)
                    .map(|program| (program, None)),
            };
            parsed.and_then(|(mut program, hashes)| {
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
                Ok((identity, program, hashes))
            })
        })();
        match parsed {
            Ok(program) => programs.push(program),
            Err(errors) => diagnostics.extend(errors),
        }
    }
    drop(parse_time);
    if !diagnostics.is_empty() {
        return Err(diagnostics.finish());
    }
    if let Some(cache) = cache {
        let _cache_time = timings::Phase::start("cache");
        let records: Vec<_> = programs
            .iter()
            .zip(inputs)
            .filter_map(|(((name, namespace, entry), program, hashes), input)| {
                let (source, interface) = (*hashes)?;
                Some(frontend_cache::ModuleRecord {
                    name,
                    path: input.path,
                    origin: input.origin,
                    kind: program.source_kind,
                    namespace,
                    entry: *entry,
                    source,
                    interface,
                })
            })
            .collect();
        cache.finish(&records);
    }
    let modules: Vec<_> = programs
        .iter()
        .zip(inputs)
        .map(
            |(((name, namespace, entry), program, _), input)| ModuleInput {
                name,
                program,
                origin: input.origin,
                namespace,
                entry: *entry,
                edition: input.edition,
                package: input.package,
            },
        )
        .collect();
    check::check_modules_indexed_all(&modules, semantic, parallelism)
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
            edition: syntax::Edition::LATEST,
            package: None,
        })
        .chain(stdlib::SOURCES.iter().map(|(path, text)| SourceInput {
            path,
            text,
            origin: ModuleOrigin::Std,
            namespace: "",
            edition: syntax::Edition::LATEST,
            package: None,
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
pub(crate) fn declared_header(
    text: &str,
    edition: syntax::Edition,
) -> (Option<syntax::Ident>, Vec<syntax::Ident>) {
    use syntax::TokenKind;
    let (tokens, _) = lexer::lex_all(text, edition);
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

#[cfg(test)]
mod tests {
    use super::parallel::{PARALLEL_STAGES, Parallelism};
    use super::*;

    /// Analyzes the user sources with the std modules that they reach, with at most `threads` threads.
    fn analyze_on(
        threads: usize,
        sources: &[(String, String)],
    ) -> Result<check::CheckedModule, DiagnosticSet> {
        let std_sources = stdlib::sources_for(sources.iter().map(|(_, text)| text.as_str()));
        let inputs: Vec<_> = sources
            .iter()
            .map(|(path, text)| (path.as_str(), text.as_str(), ModuleOrigin::User))
            .chain(
                std_sources
                    .iter()
                    .map(|(path, text)| (*path, *text, ModuleOrigin::Std)),
            )
            .map(|(path, text, origin)| SourceInput {
                path,
                text,
                origin,
                namespace: "",
                edition: syntax::Edition::LATEST,
                package: None,
            })
            .collect();
        analyze_inputs_with(&inputs, None, None, Parallelism::fixed(threads))
    }

    /// The stages that started workers while `run` ran on this thread.
    fn parallel_stages<T>(run: impl FnOnce() -> T) -> (T, usize) {
        let before = PARALLEL_STAGES.get();
        let result = run();
        (result, PARALLEL_STAGES.get() - before)
    }

    /// A module whose functions each use a value after it moved: one E1012 per function.
    fn moved_value_module(index: usize, functions: usize) -> (String, String) {
        let mut text = format!("record Owned{index} {{ text: string, count: i64 }}\n");
        for function in 0..functions {
            text += &format!(
                "\ndef broken{function} :: i64 -> i64 = \\x ->\n    let value = Owned{index} {{ text: \"text\", count: x + {function} }}\n    let other = {{ value with count = x + 1 }}\n    value.text.length\n"
            );
        }
        (format!("Module{index}.tz"), text)
    }

    /// A module of the shapes that the synthetic build benchmark uses (benchmarks/build/generate.mjs).
    fn clean_module(index: usize) -> (String, String) {
        let mut text = String::from(
            "private record Pair { left: i64u, right: i64u }\n\nprivate union Shape =\n    | Dot of i64u\n    | Span of i64u * i64u\n    | Empty\n\nprivate def twice :: Add<'a> -> 'a = \\value -> value + value\n\nprivate def rec count :: i64u -> i64u -> i64u = \\n acc -> if n == 0 then acc else count (n - 1) (acc + n)\n",
        );
        for function in 0..20 {
            let c = index * 20 + function + 3;
            let body = match function % 10 {
                0 => format!(
                    "\n    let mut total = x\n    let mut index = 0i64u\n    while index < 8 do\n        total = total * 6364136223846793005 + index + {c}\n        index = index + 1\n    total"
                ),
                1 => format!(
                    "\n    let mut total = x\n    for i in 0 .. 7 do\n        total = total * 31 + (i as i64u) + {c}\n    total"
                ),
                2 => format!(
                    "\n    match x % 4 with\n    | 0 -> x + {c}\n    | 1 -> x * 3\n    | 2 -> x + {}\n    | _ -> x / 2",
                    c + 4
                ),
                3 => format!(
                    "\n    let pair = Pair {{ left: x, right: x + {c} }}\n    pair.left * pair.right"
                ),
                4 => format!(
                    "\n    let shape = if x % 3 == 0 then Dot x else (if x % 3 == 1 then Span (x, x + {c}) else Empty)\n    match shape with\n    | Dot value -> value\n    | Span (left, right) -> left * right\n    | Empty -> x + {}",
                    c + 2
                ),
                5 => format!(" twice x + twice {c}i64u"),
                6 => format!(
                    "\n    let offset = x % {c}\n    let add = \\value -> value + offset\n    add (add x)"
                ),
                7 => format!(
                    "\n    let text = \"m{index}-\" + to_string x\n    x + (String.length text as i64u)"
                ),
                8 => format!(
                    "\n    let values = new [i64u](8, \\i -> (i as i64u) * x + {c})\n    let mut total = 0i64u\n    for value in values do\n        total = total + value\n    total"
                ),
                _ => String::from(" count (x % 16) x"),
            };
            text += &format!("private def f{function} :: i64u -> i64u = \\x ->{body}\n\n");
        }
        text += "def run :: i64u -> i64u = \\seed ->\n    let mut state = seed\n";
        for function in 0..20 {
            text += &format!("    state = f{function} state\n");
        }
        text += "    state\n";
        (format!("Module{index}.tz"), text)
    }

    #[test]
    fn ownership_errors_replay_the_diagnostic_cap_on_every_thread_count() {
        // 1,200 functions with one error each: the cap of 1,000 unique diagnostics is reached in
        // the first stage that reports ownership errors, and the first 50 sort by source.
        let sources: Vec<_> = (0..1200)
            .map(|index| moved_value_module(index, 1))
            .collect();
        let (sequential, stages) = parallel_stages(|| analyze_on(1, &sources).unwrap_err());
        assert_eq!(
            stages, 0,
            "one thread runs every stage on the calling thread"
        );
        assert_eq!(sequential.diagnostics.len(), 50);
        assert_eq!(sequential.omitted, 950);
        assert!(sequential.omitted_is_lower_bound);
        for (index, diagnostic) in sequential.diagnostics.iter().enumerate() {
            assert_eq!(diagnostic.code, "E1012", "{diagnostic:?}");
            assert_eq!(diagnostic.span.source, Some(index));
        }
        for threads in [2, 8] {
            let (parallel, stages) = parallel_stages(|| analyze_on(threads, &sources).unwrap_err());
            assert!(
                stages > 0,
                "{threads} threads checked ownership in parallel"
            );
            assert_eq!(parallel, sequential, "{threads} threads");
        }
    }

    #[test]
    fn ownership_errors_below_the_cap_keep_every_diagnostic_in_order() {
        let sources: Vec<_> = (0..80).map(|index| moved_value_module(index, 3)).collect();
        let sequential = analyze_on(1, &sources).unwrap_err();
        assert_eq!(sequential.diagnostics.len(), 50);
        assert_eq!(sequential.omitted, 190);
        assert!(!sequential.omitted_is_lower_bound);
        let mut previous = (0, 0);
        for (index, diagnostic) in sequential.diagnostics.iter().enumerate() {
            let at = (diagnostic.span.source.unwrap(), diagnostic.span.start);
            assert_eq!(at.0, index / 3, "three errors per module, in source order");
            assert!(index == 0 || previous < at, "{previous:?} {at:?}");
            previous = at;
        }
        for threads in [2, 3, 8] {
            assert_eq!(
                analyze_on(threads, &sources).unwrap_err(),
                sequential,
                "{threads}"
            );
        }
    }

    #[test]
    fn a_clean_project_has_the_same_typed_module_on_every_thread_count() {
        let sources: Vec<_> = (0..30).map(clean_module).collect();
        let (sequential, _) = parallel_stages(|| format!("{:?}", analyze_on(1, &sources)));
        assert!(
            sequential.starts_with("Ok(CheckedModule"),
            "{}",
            &sequential[..300]
        );
        for threads in [2, 3, 8] {
            let (parallel, stages) =
                parallel_stages(|| format!("{:?}", analyze_on(threads, &sources)));
            assert!(stages > 0, "{threads} threads");
            assert!(parallel == sequential, "{threads} threads");
        }
    }

    #[test]
    fn a_small_program_runs_every_stage_on_the_calling_thread() {
        let sources = [("Main.tz".to_owned(), "let x = 1\nx + 1\n".to_owned())];
        let (_, stages) = parallel_stages(|| analyze_on(8, &sources).unwrap());
        assert_eq!(stages, 0);
    }
}
