pub mod abi;
pub mod api_diff;
#[cfg(feature = "fast-backend")]
pub mod backend;
pub mod bindgen;
pub mod bindings;
mod bundle_identity;
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
mod interval;
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
pub mod resident;
mod runtime_cache;
pub mod server;
pub mod simd;
pub mod stdlib;
pub mod syntax;
pub(crate) mod syntax_codec;
pub mod target;
pub mod timings;
pub mod trap;
pub mod watch;

use check::{ModuleInput, ModuleOrigin};
use diagnostic::{Diagnostic, DiagnosticSet, Diagnostics, Span};
use std::ops::ControlFlow;

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

/// A module of the program: its name, namespace and whether it is the entry, the program that
/// was parsed, and the hashes that the frontend cache records.
type ParsedInput = (
    (String, String, bool),
    syntax::Program,
    Option<([u8; 32], [u8; 32])>,
);

/// The work of one source: its module identity and its program, looked up in `cache` when there
/// is one. Nothing shared changes, so the sources can be parsed in any order and on any thread.
/// The lookup is for `FrontendCache::commit`; there is none when the source is not looked up.
fn parse_input(
    id: usize,
    input: &SourceInput<'_>,
    cache: Option<&frontend_cache::FrontendCache>,
) -> (
    Result<ParsedInput, Vec<Diagnostic>>,
    Option<frontend_cache::Lookup>,
) {
    let (name, extension) = match input.origin {
        ModuleOrigin::User => input
            .path
            .rsplit_once('.')
            .filter(|(_, extension)| syntax::SourceKind::from_extension(extension).is_some())
            .map_or((input.path, None), |(name, extension)| {
                (name, Some(extension))
            }),
        ModuleOrigin::Std => {
            let Some(name) = stdlib::module_name(input.path) else {
                let error = Diagnostic::new(
                    "E1011",
                    format!(
                        "invalid standard library path '{}'; std sources are flat files such as 'std/Math.tz'",
                        input.path
                    ),
                    Span::default().in_source(id),
                );
                return (Err(vec![error]), None);
            };
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
    let (parsed, lookup) = match cache {
        Some(cache) => {
            let (parsed, lookup) = cache.lookup(input.text, id, input.edition);
            let parsed =
                parsed.map(|parsed| (parsed.program, Some((parsed.key, parsed.interface))));
            (parsed, Some(lookup))
        }
        None => {
            let parsed = parser::parse_with_source_all(input.text, id, input.edition)
                .map(|program| (program, None));
            (parsed, None)
        }
    };
    let parsed = parsed.and_then(|(mut program, hashes)| {
        program.source_kind = extension.and_then(syntax::SourceKind::from_extension);
        let identity = match name {
            Some(name) => (name, stdlib::NAMESPACE.to_owned(), false),
            None => {
                let declared = program.namespace.as_ref().map(|namespace| &namespace.path);
                let (key, namespace) = module_identity(input.path, declared, input.namespace)
                    .map_err(|mut error| {
                        error.span.source = Some(id);
                        vec![error]
                    })?;
                let entry = module_name_from_relative(std::path::Path::new(input.path))
                    .is_ok_and(|relative| relative == "Main");
                (key, namespace, entry)
            }
        };
        Ok((identity, program, hashes))
    });
    (parsed, lookup)
}

/// The text that one worker takes before the parse stage uses another thread, and the least
/// number of sources a worker takes.
const PARSE_BYTES_PER_WORKER: usize = 32 << 10;
const PARSE_SOURCES_PER_WORKER: usize = 2;

/// How many workers parse `inputs`, and the order in which they take the sources: the largest
/// first, so that the last source taken is a small one. Sources are claimed one at a time.
fn parse_plan(
    inputs: &[SourceInput<'_>],
    parallelism: parallel::Parallelism,
) -> (usize, Option<Vec<usize>>) {
    let bytes: usize = inputs.iter().map(|input| input.text.len()).sum();
    let workers = parallelism
        .workers(bytes, PARSE_BYTES_PER_WORKER)
        .min(parallelism.workers(inputs.len(), PARSE_SOURCES_PER_WORKER));
    if workers < 2 {
        return (workers, None);
    }
    let mut order: Vec<usize> = (0..inputs.len()).collect();
    order.sort_by_key(|&id| std::cmp::Reverse(inputs[id].text.len()));
    (workers, Some(order))
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
    let mut lookups = Vec::new();
    let parse_time = timings::Phase::start("parse");
    // The sources are parsed in parallel and read in order: the first source after `diagnostics`
    // is full is not read (a stage on one thread does not even parse it), and the cache records
    // the sources that were read, in their order, after the stage.
    let shared = cache.as_deref();
    let (workers, claim) = parse_plan(inputs, parallelism);
    parallel::ordered_for_each(
        inputs.len(),
        workers,
        1,
        claim.as_deref(),
        |id| parse_input(id, &inputs[id], shared),
        |_, outcome| {
            if diagnostics.is_full() {
                return ControlFlow::Break(());
            }
            let (parsed, lookup) = outcome.get();
            lookups.extend(lookup);
            match parsed {
                Ok(program) => programs.push(program),
                Err(errors) => diagnostics.extend(errors),
            }
            ControlFlow::Continue(())
        },
    );
    if let Some(cache) = cache.as_deref_mut() {
        for lookup in lookups {
            cache.commit(lookup);
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
        analyze_indexed_on(threads, sources, None)
    }

    fn analyze_indexed_on(
        threads: usize,
        sources: &[(String, String)],
        index: Option<&mut check::semantic::SemanticIndex>,
    ) -> Result<check::CheckedModule, DiagnosticSet> {
        with_inputs(sources, |inputs| {
            analyze_inputs_with(inputs, index, None, Parallelism::fixed(threads))
        })
    }

    /// Runs `run` on the inputs of the user `sources` and of the std modules that they reach.
    fn with_inputs<T>(
        sources: &[(String, String)],
        run: impl FnOnce(&[SourceInput<'_>]) -> T,
    ) -> T {
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
        run(&inputs)
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

    /// A module whose functions each add a number and a string: one error in each body.
    fn type_error_module(index: usize, functions: usize) -> (String, String) {
        let mut text = String::new();
        for function in 0..functions {
            text += &format!("def bad{function} :: i64 -> i64 = \\x -> x + \"{index}\"\n\n");
        }
        (format!("Module{index}.tz"), text)
    }

    /// A module whose function has a type error and, after it, a value that is used after it moved:
    /// the recovery pass reports the second error because the body is only partly typed.
    fn recovered_module(index: usize) -> (String, String) {
        (
            format!("Module{index}.tz"),
            format!(
                "record Owned{index} {{ text: string, count: i64 }}\n\ndef mixed :: i64 -> i64 = \\x ->\n    let value = Owned{index} {{ text: \"text\", count: x }}\n    let other = {{ value with count = 1 }}\n    let bad = x + \"{index}\"\n    value.text.length\n"
            ),
        )
    }

    /// A module with a syntax error, padded so that a thousand of them are enough text for the
    /// parse stage to use workers.
    fn syntax_error_module(index: usize) -> (String, String) {
        (
            format!("Module{index}.tz"),
            format!("// {}\ndef broken{index} :: = \n", "padding ".repeat(30)),
        )
    }

    /// A valid module of the size of `syntax_error_module`.
    fn padded_module(index: usize) -> (String, String) {
        (
            format!("Module{index}.tz"),
            format!(
                "// {}\ndef value{index} :: i64 = {index}\n",
                "padding ".repeat(30)
            ),
        )
    }

    /// A module whose path is not a module path (a dot in a directory): E1011 after it parsed.
    fn misplaced_module(index: usize) -> (String, String) {
        let (_, text) = padded_module(index);
        (format!("bad.directory/Module{index}.tz"), text)
    }

    /// A module whose only function does not cover the case `Blue`: E1021 when the function is finished.
    fn incomplete_match_module(index: usize) -> (String, String) {
        (
            format!("Module{index}.tz"),
            format!(
                "union Color{index} = Red | Green | Blue\n\ndef pick :: Color{index} -> i64 = \\color ->\n    match color with\n    | Red -> 1\n    | Green -> 2\n"
            ),
        )
    }

    /// A module whose functions each leave a local unused (W1001) and call a neighbour.
    fn unused_local_module(index: usize) -> (String, String) {
        let mut text = String::new();
        for function in 0..24 {
            text += &format!(
                "def f{function} :: i64 -> i64 = \\x ->\n    let unused{function} = x + {index}\n    x * {function}\n\n"
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
    fn syntax_errors_replay_the_diagnostic_cap_on_every_thread_count() {
        // 1,100 broken modules: the 1,000th unique diagnostic fills the set, so the parse of the
        // modules after it never matters, and the first 50 are those of the first modules.
        let sources: Vec<_> = (0..1100).map(syntax_error_module).collect();
        let (sequential, stages) = parallel_stages(|| analyze_on(1, &sources).unwrap_err());
        assert_eq!(stages, 0);
        assert_eq!(sequential.diagnostics.len(), 50);
        assert_eq!(sequential.omitted, 950);
        assert!(sequential.omitted_is_lower_bound);
        for (index, diagnostic) in sequential.diagnostics.iter().enumerate() {
            assert_eq!(diagnostic.span.source, Some(index), "{diagnostic:?}");
        }
        for threads in [2, 3, 8] {
            let (parallel, stages) = parallel_stages(|| analyze_on(threads, &sources).unwrap_err());
            assert_eq!(
                stages, 1,
                "{threads} threads parse in parallel and stop there"
            );
            assert_eq!(parallel, sequential, "{threads} threads");
        }
    }

    #[test]
    fn syntax_and_path_errors_below_the_cap_keep_the_order_of_the_modules() {
        // Every third module is broken, every third has a path that is not a module path.
        let sources: Vec<_> = (0..600)
            .map(|index| match index % 3 {
                0 => syntax_error_module(index),
                1 => misplaced_module(index),
                _ => padded_module(index),
            })
            .collect();
        let (sequential, _) = parallel_stages(|| analyze_on(1, &sources).unwrap_err());
        assert_eq!(sequential.diagnostics.len(), 50);
        assert!(!sequential.omitted_is_lower_bound);
        assert_eq!(sequential.omitted, 400 - 50);
        let codes: std::collections::BTreeSet<_> = sequential
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code)
            .collect();
        assert!(codes.contains("E1011") && codes.len() == 2, "{codes:?}");
        for (index, diagnostic) in sequential.diagnostics.iter().enumerate() {
            // Modules 0, 1, 3, 4, 6, 7, ... report.
            assert_eq!(diagnostic.span.source, Some(3 * (index / 2) + index % 2));
        }
        for threads in [2, 8] {
            let (parallel, stages) = parallel_stages(|| analyze_on(threads, &sources).unwrap_err());
            assert_eq!(stages, 1, "{threads} threads");
            assert_eq!(parallel, sequential, "{threads} threads");
        }
    }

    #[test]
    fn the_frontend_cache_holds_the_same_bytes_and_counts_on_every_thread_count() {
        let sources: Vec<_> = (0..30)
            .map(clean_module)
            .chain((30..180).map(padded_module))
            .collect();
        let expected = format!("{:?}", analyze_on(1, &sources));
        assert!(expected.starts_with("Ok(CheckedModule"));
        let temporary = driver::TemporaryDirectory::new(&std::env::temp_dir()).unwrap();
        let root = temporary.path.join("cache");
        let project = frontend_cache::ProjectKey::new(&temporary.path, "program").unwrap();
        let mut outcomes = Vec::new();
        for threads in [1, 2, 8] {
            let _ = std::fs::remove_dir_all(root.join("frontend"));
            let mut runs = Vec::new();
            // The first run is cold (every source is a miss), the second warm (every one a hit).
            for run in 0..2 {
                let mut cache = frontend_cache::FrontendCache::open(&root, project).unwrap();
                let (result, stages) = parallel_stages(|| {
                    with_inputs(&sources, |inputs| {
                        analyze_inputs_with(
                            inputs,
                            None,
                            Some(&mut cache),
                            Parallelism::fixed(threads),
                        )
                    })
                });
                // Parse, bodies, finish and the two ownership stages, or none on one thread.
                assert!(
                    if threads == 1 {
                        stages == 0
                    } else {
                        stages >= 4
                    },
                    "{threads} threads, run {run}: {stages} stages"
                );
                assert_eq!(
                    format!("{result:?}"),
                    expected,
                    "{threads} threads, run {run}"
                );
                let mut files: Vec<_> = std::fs::read_dir(root.join("frontend"))
                    .unwrap()
                    .map(|entry| entry.unwrap().path())
                    .map(|path| {
                        (
                            path.file_name().unwrap().to_owned(),
                            std::fs::read(path).unwrap(),
                        )
                    })
                    .collect();
                files.sort();
                assert_eq!(files.len(), 2, "a pack and a manifest");
                runs.push((format!("{:?}", cache.stats), files));
            }
            outcomes.push(runs);
        }
        let inputs = with_inputs(&sources, |inputs| inputs.len());
        assert_eq!(
            outcomes[0][0].0,
            format!("FrontendStats {{ hits: 0, misses: {inputs}, rejected: 0 }}")
        );
        assert_eq!(
            outcomes[0][1].0,
            format!("FrontendStats {{ hits: {inputs}, misses: 0, rejected: 0 }}")
        );
        for threads in 1..outcomes.len() {
            assert!(outcomes[threads] == outcomes[0], "threads index {threads}");
        }
    }

    #[test]
    fn parse_plan_follows_the_amount_of_text() {
        // The plan of eight threads, and of one, for sources of the given lengths.
        let plan = |lengths: &[usize]| {
            let texts: Vec<String> = lengths.iter().map(|&length| "x".repeat(length)).collect();
            let inputs: Vec<_> = texts
                .iter()
                .map(|text| SourceInput {
                    path: "Module.tz",
                    text,
                    origin: ModuleOrigin::User,
                    namespace: "",
                    edition: syntax::Edition::LATEST,
                    package: None,
                })
                .collect();
            (
                parse_plan(&inputs, Parallelism::fixed(8)),
                parse_plan(&inputs, Parallelism::SEQUENTIAL),
            )
        };
        // One large source cannot be shared; a few small ones are not worth a thread.
        let (eight, _) = plan(&[4 << 20]);
        assert_eq!(eight, (0, None));
        let (eight, _) = plan(&[100, 100, 100]);
        assert_eq!(eight, (0, None));
        // 100 KiB in 100 sources: one worker per 32 KiB, in the order of the sources.
        let (eight, one) = plan(&[1024; 100]);
        assert_eq!(eight, (3, Some((0..100).collect())));
        assert_eq!(one, (1, None));
        // The largest source is taken first, equal ones in the order of the sources; a worker
        // takes at least two sources.
        let kib = [10, 40, 10, 60, 40, 10, 40, 10, 60, 40].map(|kib| kib << 10);
        let (eight, _) = plan(&kib);
        assert_eq!(eight, (5, Some(vec![3, 8, 1, 4, 6, 9, 0, 2, 5, 7])));
        // Never more workers than threads.
        let (eight, _) = plan(&[1 << 20; 64]);
        assert_eq!(eight.0, 8);
    }

    #[test]
    fn body_errors_replay_the_diagnostic_cap_on_every_thread_count() {
        // 1,600 errors in 400 modules: the cap of 1,000 is reached while the bodies are checked, in
        // the middle of the stage, and a later body never adds to the diagnostics.
        let sources: Vec<_> = (0..400).map(|index| type_error_module(index, 4)).collect();
        let sequential = analyze_on(1, &sources).unwrap_err();
        assert_eq!(sequential.diagnostics.len(), 50);
        assert_eq!(sequential.omitted, 950);
        assert!(sequential.omitted_is_lower_bound);
        let code = sequential.diagnostics[0].code;
        assert!(
            sequential
                .diagnostics
                .iter()
                .all(|diagnostic| diagnostic.code == code)
        );
        for threads in [2, 3, 8] {
            let (parallel, stages) = parallel_stages(|| analyze_on(threads, &sources).unwrap_err());
            assert!(stages > 0, "{threads} threads");
            assert_eq!(parallel, sequential, "{threads} threads");
        }
    }

    #[test]
    fn body_errors_below_the_cap_and_the_recovery_pass_do_not_depend_on_the_thread_count() {
        let mut sources: Vec<_> = (0..120).map(|index| type_error_module(index, 3)).collect();
        sources.extend((120..400).map(recovered_module));
        let sequential = analyze_on(1, &sources).unwrap_err();
        assert!(!sequential.omitted_is_lower_bound, "{}", sequential.omitted);
        assert_eq!(sequential.diagnostics.len(), 50);
        assert!(sequential.omitted > 0);
        for threads in [2, 3, 8] {
            let (parallel, stages) = parallel_stages(|| analyze_on(threads, &sources).unwrap_err());
            assert!(stages > 0, "{threads} threads");
            assert_eq!(parallel, sequential, "{threads} threads");
        }
        // Only the recovered modules: every error of the recovery pass is reported, in module order.
        let recovered: Vec<_> = (0..300).map(recovered_module).collect();
        let sequential = analyze_on(1, &recovered).unwrap_err();
        for threads in [2, 8] {
            assert_eq!(analyze_on(threads, &recovered).unwrap_err(), sequential);
        }
    }

    #[test]
    fn finish_errors_replay_the_diagnostic_cap_on_every_thread_count() {
        // The match of each module fails when its function is finished, after every body checked:
        // 1,100 errors reach the cap of 1,000 in function order.
        let sources: Vec<_> = (0..1100).map(incomplete_match_module).collect();
        let sequential = analyze_on(1, &sources).unwrap_err();
        assert_eq!(sequential.diagnostics.len(), 50);
        assert_eq!(sequential.omitted, 950);
        assert!(sequential.omitted_is_lower_bound);
        for (index, diagnostic) in sequential.diagnostics.iter().enumerate() {
            assert_eq!(diagnostic.code, "E1021", "{diagnostic:?}");
            assert_eq!(diagnostic.span.source, Some(index));
        }
        for threads in [2, 8] {
            let (parallel, stages) = parallel_stages(|| analyze_on(threads, &sources).unwrap_err());
            assert!(stages > 0, "{threads} threads");
            assert_eq!(parallel, sequential, "{threads} threads");
        }
    }

    #[test]
    fn warnings_and_the_semantic_index_do_not_depend_on_the_thread_count() {
        let sources: Vec<_> = (0..30).map(unused_local_module).collect();
        let mut sequential_index = check::semantic::SemanticIndex::default();
        let sequential = analyze_indexed_on(1, &sources, Some(&mut sequential_index)).unwrap();
        assert!(
            sequential.warnings.len() >= 30 * 24,
            "{}",
            sequential.warnings.len()
        );
        assert!(
            sequential
                .warnings
                .iter()
                .all(|warning| warning.code == "W1001")
        );
        let sequential = format!("{sequential:?}");
        let sequential_index = format!("{sequential_index:?}");
        for threads in [2, 3, 8] {
            let mut index = check::semantic::SemanticIndex::default();
            let (parallel, stages) = parallel_stages(|| {
                analyze_indexed_on(threads, &sources, Some(&mut index)).unwrap()
            });
            assert!(stages >= 4, "{threads} threads: {stages} parallel stages");
            assert!(format!("{parallel:?}") == sequential, "{threads} threads");
            assert!(
                format!("{index:?}") == sequential_index,
                "{threads} threads"
            );
        }
    }

    /// Analyzes `sources` as the user modules and compares the whole result at 1, 2 and 8 threads.
    fn assert_same_on_every_thread_count(sources: &[(String, String)]) -> String {
        let sequential = format!("{:?}", analyze_on(1, sources));
        for threads in [2, 8] {
            let (parallel, stages) =
                parallel_stages(|| format!("{:?}", analyze_on(threads, sources)));
            assert!(stages > 0, "{threads} threads");
            assert!(parallel == sequential, "{threads} threads");
        }
        sequential
    }

    /// The modules that make the stages of the check run on workers.
    fn large_project() -> Vec<(String, String)> {
        (0..30).map(clean_module).collect()
    }

    #[test]
    fn limit_and_cycle_errors_do_not_depend_on_the_thread_count() {
        for (main, code) in [
            // F is specialized without end (E1017).
            (
                "def rec f :: 'a -> unit\nfn rec f x = f [x]\nf 1\n",
                "E1017",
            ),
            ("record A { b: B }\nrecord B { a: A }\n", "E1010"),
        ] {
            let mut sources = large_project();
            sources.push(("Main.tz".to_owned(), main.to_owned()));
            let dump = assert_same_on_every_thread_count(&sources);
            assert!(dump.contains(&format!("\"{code}\"")), "{code}: {dump}");
        }
    }

    #[test]
    fn the_deepest_accepted_nesting_is_checked_the_same_on_workers() {
        // The deepest nesting that the parser accepts inside `\x -> let _ = ...`, in a function
        // that a worker of each stage checks among many others. Lists and `if` chains use more
        // stack per level, which a debug build of the tests (4 MiB threads) cannot take that deep.
        let depth = syntax::MAX_NESTING - 3;
        let shallow = 40;
        let shapes = [
            format!("{}1{}", "(".repeat(depth), ")".repeat(depth)),
            format!("{}1", "1 + ".repeat(depth)),
            format!("{}0{}", "[".repeat(shallow), "]".repeat(shallow)),
            format!("{}1", "if true then 0 else ".repeat(shallow)),
        ];
        let mut sources = large_project();
        for (index, shape) in shapes.iter().enumerate() {
            sources.push((
                format!("Deep{index}.tz"),
                format!("def deep :: i64 -> i64 = \\x ->\n    let _value = {shape}\n    x\n"),
            ));
        }
        let dump = assert_same_on_every_thread_count(&sources);
        assert!(
            dump.starts_with("Ok(CheckedModule"),
            "{}",
            &dump[..300.min(dump.len())]
        );
        // One more level is a syntax error on every thread count.
        let mut too_deep = large_project();
        too_deep.push((
            "Deep.tz".to_owned(),
            format!(
                "def deep :: i64 -> i64 = \\x ->\n    let _value = {}1{}\n    x\n",
                "(".repeat(depth + 1),
                ")".repeat(depth + 1)
            ),
        ));
        let dump = assert_same_on_every_thread_count(&too_deep);
        assert!(dump.contains("syntax nesting exceeds 128"), "{dump}");
    }

    #[test]
    fn examples_and_fixtures_analyze_identically_on_every_thread_count() {
        // Every stage of every project runs on workers (`eager`), however small the project is.
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut projects = Vec::new();
        for directory in ["examples", "tests/fixtures"] {
            for entry in std::fs::read_dir(root.join(directory)).unwrap() {
                let entry = entry.unwrap();
                if entry.file_type().unwrap().is_dir() {
                    projects.push(entry.path());
                }
            }
        }
        projects.sort();
        let (mut analyzed, mut accepted) = (0, 0);
        for path in &projects {
            let Ok(project) = driver::Project::load(path) else {
                continue;
            };
            let inputs = project.inputs();
            let analyze = |parallelism| {
                parallel_stages(|| {
                    format!(
                        "{:?}",
                        analyze_inputs_with(&inputs, None, None, parallelism)
                    )
                })
            };
            let (sequential, stages) = analyze(Parallelism::SEQUENTIAL);
            assert_eq!(stages, 0);
            let (parallel, stages) = analyze(Parallelism::eager(8));
            assert!(parallel == sequential, "{}", path.display());
            assert!(stages >= 4, "{}: {stages} parallel stages", path.display());
            analyzed += 1;
            accepted += usize::from(sequential.starts_with("Ok("));
        }
        assert!(analyzed >= 90 && accepted >= 90, "{analyzed} {accepted}");
    }

    #[test]
    fn a_program_with_little_to_do_stays_on_the_calling_thread() {
        // One tiny module and no standard library: no stage has enough work for a second thread.
        let input = SourceInput {
            path: "Main.tz",
            text: "let x = 1\nx + 1\n",
            origin: ModuleOrigin::User,
            namespace: "",
            edition: syntax::Edition::LATEST,
            package: None,
        };
        let (result, stages) =
            parallel_stages(|| analyze_inputs_with(&[input], None, None, Parallelism::fixed(8)));
        assert!(result.is_ok());
        assert_eq!(stages, 0);
    }

    #[test]
    fn hello_with_the_standard_library_uses_workers_for_its_declarations() {
        // The standard library's text and declarations are what make even `1 + 1` worth parallel
        // stages: its parse and the check of its functions. One thread never starts a worker.
        let sources = [("Main.tz".to_owned(), "let x = 1\nx + 1\n".to_owned())];
        let (sequential, stages) = parallel_stages(|| format!("{:?}", analyze_on(1, &sources)));
        assert_eq!(stages, 0);
        let (parallel, stages) = parallel_stages(|| format!("{:?}", analyze_on(8, &sources)));
        assert!(stages >= 2, "{stages} parallel stages");
        assert!(parallel == sequential);
    }
}
