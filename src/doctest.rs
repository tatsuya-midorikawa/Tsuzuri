//! Doc-tests (G34): the ```` ```tsuzuri ```` examples of `///` comments. `tsuzuri test --doc`
//! collects the examples of the documented declarations of a package (or of `std`), runs each as
//! the `Main.tz` of the package, and checks that it type-checks, prints the expected output,
//! passes its tests, or is rejected with the expected code. `tsuzuri doc` and the editor's hover
//! show the examples without their setup lines and with their expected output (`render_doc`).

use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::check::ModuleOrigin;
use crate::diagnostic::{Diagnostic, DiagnosticKind, DiagnosticSet, Span};
use crate::driver::{self, BuildOptions, Project};
use crate::syntax::{Documentation, Edition, Program, Provenance, TokenKind, Visibility};

/// The most examples that one package may have.
pub const MAX_DOC_EXAMPLES: usize = 10_000;
/// How long the program of one `run=` example may run.
pub const EXAMPLE_TIMEOUT: Duration = Duration::from_secs(30);
/// The notation errors that `tsuzuri test --doc` reports at most.
const MAX_NOTATION_ERRORS: usize = 50;

/// What an example promises.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Expectation {
    /// It type-checks.
    Check,
    /// It runs and prints this (decoded) output.
    Run(String),
    /// Its `test` declarations pass.
    Test,
    /// The checker rejects it, and its first error has this code.
    Fails(String),
    /// Nothing is checked.
    Ignore,
}

impl Expectation {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Check => "check",
            Self::Run(_) => "run",
            Self::Test => "test",
            Self::Fails(_) => "fails",
            Self::Ignore => "ignore",
        }
    }
}

/// The kind of declaration that a documentation belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OwnerKind {
    Function,
    ExternFunction,
    ExternType,
    Record,
    Union,
    Alias,
    Constant,
    Class,
    Method,
}

/// One example of a documentation comment.
#[derive(Clone, Debug)]
pub struct Example {
    /// The 0-based number across the package (`collect`); 0 from `examples_in_source`.
    pub index: usize,
    /// The index of the source in `Project::sources`.
    pub source: usize,
    /// The module as source code names it (`Geometry::Point`).
    pub module: String,
    /// The declaration (`sum`, or `Eq.eq` for a class method).
    pub owner: String,
    pub owner_kind: OwnerKind,
    /// The 1-based number of the example in its documentation.
    pub number: usize,
    pub expectation: Expectation,
    /// The program: the lines of the example, hidden lines without their marker, each ending
    /// with a newline.
    pub program: String,
    /// The 1-based line of the opening fence in the file.
    pub line: usize,
    /// The doc comment token of the opening fence, in its source.
    pub span: Span,
}

impl Example {
    /// `sum#1`
    pub fn name(&self) -> String {
        format!("{}#{}", self.owner, self.number)
    }

    /// `Array.sum#1`, which `--filter` matches.
    pub fn label(&self) -> String {
        format!("{}.{}", self.module, self.name())
    }
}

/// How `run` checks the examples.
#[derive(Clone, Debug, Default)]
pub struct Options {
    pub build: BuildOptions,
    /// The optimization level of the `test` examples.
    pub level: u8,
    pub filter: Option<String>,
    pub indices: Vec<usize>,
    pub warnings: Warnings,
}

/// The warning settings of the command line. Without `deny`, an example's warnings are ignored;
/// with it, an example that has a warning fails with its first one (the `--warn` warnings count).
#[derive(Clone, Copy, Debug, Default)]
pub struct Warnings {
    pub deny: bool,
    pub implicit_copy: bool,
    pub canonical: bool,
    pub untrusted_extern: bool,
}

impl Options {
    pub fn includes(&self, example: &Example) -> bool {
        (self.indices.is_empty() || self.indices.contains(&example.index))
            && self
                .filter
                .as_deref()
                .is_none_or(|filter| example.label().contains(filter))
    }
}

/// The result of one example: `failure` is `None` when it holds.
#[derive(Clone, Debug)]
pub struct ExampleResult {
    pub example: Example,
    pub failure: Option<String>,
    /// The standard error of a failed `test` example.
    pub output: String,
    pub duration_ms: u128,
}

#[derive(Clone, Debug, Default)]
pub struct Report {
    pub results: Vec<ExampleResult>,
    /// The examples that were not run: `ignore`, left out by `--filter` or `--index`, or in `Main.tz`.
    pub ignored: usize,
    pub duration_ms: u128,
    /// The build notices and the note of the examples in `Main.tz`, each once.
    pub messages: Vec<String>,
}

/// A fenced block of `tsuzuri` code: the indices of its opening and closing lines in the body of
/// the documentation, the number of backticks, and the information string.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Fence {
    open: usize,
    close: usize,
    ticks: usize,
    info: String,
}

#[derive(Debug, Default, PartialEq, Eq)]
struct Scan {
    fences: Vec<Fence>,
    /// `(line, message)` of the notation errors that the scan finds.
    problems: Vec<(usize, String)>,
}

const INDENTED: &str =
    "a tsuzuri code fence must start at column 1 of the comment; remove the indentation";
const UNCLOSED: &str =
    "the tsuzuri code fence is not closed; end it with a line of three backticks";
const EMPTY: &str = "a tsuzuri example needs at least one line of code";

fn leading_ticks(line: &str) -> usize {
    line.bytes().take_while(|byte| *byte == b'`').count()
}

/// The `tsuzuri` fences of `lines`, and the indented or unclosed ones. A fence of another
/// language hides its lines from the search.
fn scan(lines: &[&str]) -> Scan {
    let mut found = Scan::default();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let ticks = leading_ticks(line);
        if ticks >= 3 && !line[ticks..].contains('`') {
            let info = line[ticks..].trim().to_owned();
            let language = info.split_whitespace().next().unwrap_or("");
            let close = (i + 1..lines.len()).find(|&j| {
                let text = lines[j].trim_end();
                text.len() >= ticks && text.bytes().all(|byte| byte == b'`')
            });
            match (language == "tsuzuri", close) {
                (true, Some(j)) => {
                    found.fences.push(Fence {
                        open: i,
                        close: j,
                        ticks,
                        info,
                    });
                    i = j + 1;
                }
                (true, None) => {
                    found.problems.push((i, UNCLOSED.to_owned()));
                    break;
                }
                (false, Some(j)) => i = j + 1,
                (false, None) => break,
            }
            continue;
        }
        if line.starts_with([' ', '\t']) {
            let rest = line.trim_start();
            if let Some(after) = rest.strip_prefix("```tsuzuri")
                && (after.is_empty() || after.starts_with([' ', '\t']))
            {
                found.problems.push((i, INDENTED.to_owned()));
            }
        }
        i += 1;
    }
    found
}

/// The expectation of the options after `tsuzuri` in an information string.
fn parse_options(info: &str) -> Result<Expectation, String> {
    let mut chosen: Option<(String, Expectation)> = None;
    for word in info.split([' ', '\t']).filter(|word| !word.is_empty()) {
        let expectation = if let Some(output) = word.strip_prefix("run=") {
            Expectation::Run(decode_output(output)?)
        } else if let Some(code) = word.strip_prefix("fails=") {
            let valid = code.len() == 5
                && code.starts_with('E')
                && code[1..].bytes().all(|byte| byte.is_ascii_digit());
            if !valid {
                return Err(format!(
                    "fails= needs an error code such as E1012; found '{code}'"
                ));
            }
            Expectation::Fails(code.to_owned())
        } else if word == "test" {
            Expectation::Test
        } else if word == "ignore" {
            Expectation::Ignore
        } else {
            return Err(format!(
                "unknown option '{word}' in a tsuzuri code fence; use run=OUTPUT, test, ignore, or fails=CODE"
            ));
        };
        let name = word.split_once('=').map_or(word, |(name, _)| name);
        let spelled = if word.contains('=') {
            format!("{name}=")
        } else {
            name.to_owned()
        };
        if let Some((previous, _)) = &chosen {
            if *previous == spelled {
                return Err(format!("option '{spelled}' is given twice; write it once"));
            }
            return Err(format!(
                "a tsuzuri code fence takes one of run=, test, ignore, and fails=; found '{previous}' and '{spelled}'"
            ));
        }
        chosen = Some((spelled, expectation));
    }
    Ok(chosen.map_or(Expectation::Check, |(_, expectation)| expectation))
}

/// The output that a `run=` token encodes: `%` and two hexadecimal digits are one byte, every
/// other character is itself, and the bytes are UTF-8 (JavaScript's `decodeURIComponent`).
pub fn decode_output(token: &str) -> Result<String, String> {
    let bytes = token.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let digits = bytes.get(i + 1..i + 3).and_then(|pair| {
                std::str::from_utf8(pair)
                    .ok()
                    .and_then(|text| u8::from_str_radix(text, 16).ok())
                    .filter(|_| pair.iter().all(u8::is_ascii_hexdigit))
            });
            let Some(byte) = digits else {
                return Err(
                    "'%' in run= must be followed by two hexadecimal digits; write a literal percent sign as %25"
                        .to_owned(),
                );
            };
            decoded.push(byte);
            i += 3;
        } else {
            decoded.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(decoded).map_err(|_| {
        "run= is not valid UTF-8 after decoding; encode the output as UTF-8".to_owned()
    })
}

/// Whether a line of an example is a hidden setup line: `#` alone, or `# ` and code.
fn is_hidden(line: &str) -> bool {
    line == "#" || line.starts_with("# ")
}

/// The program of an example's lines: a hidden line loses `#` and the one space after it.
fn program_of(body: &[&str]) -> String {
    let mut program = String::new();
    for line in body {
        let line = if is_hidden(line) {
            line.strip_prefix("# ").unwrap_or(&line[1..])
        } else {
            line
        };
        program.push_str(line);
        program.push('\n');
    }
    program
}

/// A documented declaration whose examples are collected.
struct Item<'a> {
    owner: String,
    kind: OwnerKind,
    doc: &'a Documentation,
}

/// The declarations that `tsuzuri doc` shows, with their documentation, in source order.
fn documented_items<'a>(program: &'a Program) -> Vec<Item<'a>> {
    let mut items = Vec::new();
    let mut push = |owner: &str, kind: OwnerKind, doc: Option<&'a Documentation>| {
        if let Some(doc) = doc {
            items.push((owner.to_owned(), kind, doc));
        }
    };
    let public = |visibility: Visibility| visibility != Visibility::Private;
    for declaration in &program.functions {
        if public(declaration.visibility) {
            push(
                &declaration.name.text,
                OwnerKind::Function,
                declaration.doc.as_ref(),
            );
        }
    }
    for declaration in &program.externs {
        if public(declaration.visibility) {
            push(
                &declaration.name.text,
                OwnerKind::ExternFunction,
                declaration.doc.as_ref(),
            );
        }
    }
    for declaration in &program.extern_types {
        if public(declaration.visibility) {
            push(
                &declaration.name.text,
                OwnerKind::ExternType,
                declaration.doc.as_ref(),
            );
        }
    }
    for declaration in &program.records {
        if public(declaration.visibility) {
            push(
                &declaration.name.text,
                OwnerKind::Record,
                declaration.doc.as_ref(),
            );
        }
    }
    for declaration in &program.unions {
        if public(declaration.visibility) && declaration.name.provenance != Provenance::Generated {
            push(
                &declaration.name.text,
                OwnerKind::Union,
                declaration.doc.as_ref(),
            );
        }
    }
    for declaration in &program.type_aliases {
        if public(declaration.visibility) {
            push(
                &declaration.name.text,
                OwnerKind::Alias,
                declaration.doc.as_ref(),
            );
        }
    }
    for declaration in &program.constants {
        if public(declaration.visibility) {
            push(
                &declaration.name.text,
                OwnerKind::Constant,
                declaration.doc.as_ref(),
            );
        }
    }
    for class in &program.classes {
        push(&class.name.text, OwnerKind::Class, class.doc.as_ref());
        for method in &class.methods {
            push(
                &format!("{}.{}", class.name.text, method.name.text),
                OwnerKind::Method,
                method.doc.as_ref(),
            );
        }
    }
    let mut items: Vec<Item<'a>> = items
        .into_iter()
        .map(|(owner, kind, doc)| Item { owner, kind, doc })
        .collect();
    items.sort_by_key(|item| item.doc.span.start);
    items
}

fn notation_error(message: impl Into<String>, span: Span) -> Diagnostic {
    Diagnostic::of(DiagnosticKind::DocExampleInvalid, message, span)
}

/// Whether `text` may hold a `tsuzuri` fence: a quick test before `scan`, which also accepts
/// blanks between the backticks and the language (`/// ``` tsuzuri run=1`).
fn may_have_examples(text: &str) -> bool {
    text.contains("```") && text.contains("tsuzuri")
}

/// The examples of one source, or its notation errors (`E2011`). `index` is 0.
pub fn examples_in_source(
    text: &str,
    edition: Edition,
    source: usize,
    module: &str,
) -> Result<Vec<Example>, Vec<Diagnostic>> {
    if !may_have_examples(text) {
        return Ok(Vec::new());
    }
    let program =
        crate::parser::parse_with_source(text, source, edition).map_err(|error| vec![error])?;
    let (tokens, _) = crate::lexer::lex_all(text, edition);
    let comments: Vec<(&str, Span)> = tokens
        .iter()
        .filter_map(|token| match &token.kind {
            TokenKind::DocComment(text) => Some((text.as_str(), token.span)),
            _ => None,
        })
        .collect();
    let mut examples = Vec::new();
    let mut errors = Vec::new();
    let mut next = 0;
    for item in documented_items(&program) {
        let range = item.doc.span;
        while next < comments.len() && comments[next].1.start < range.start {
            next += 1;
        }
        let mut lines = Vec::new();
        let mut spans = Vec::new();
        let mut cursor = next;
        while cursor < comments.len() && comments[cursor].1.end <= range.end {
            lines.push(comments[cursor].0);
            spans.push(comments[cursor].1.in_source(source));
            cursor += 1;
        }
        let found = scan(&lines);
        for (line, message) in found.problems {
            errors.push(notation_error(message, spans[line]));
        }
        for (number, fence) in found.fences.iter().enumerate() {
            let options = fence.info["tsuzuri".len()..].trim();
            let expectation = match parse_options(options) {
                Ok(expectation) => expectation,
                Err(message) => {
                    errors.push(notation_error(message, spans[fence.open]));
                    continue;
                }
            };
            let body = &lines[fence.open + 1..fence.close];
            if body.iter().all(|line| line.trim().is_empty()) {
                errors.push(notation_error(EMPTY, spans[fence.open]));
                continue;
            }
            examples.push(Example {
                index: 0,
                source,
                module: module.to_owned(),
                owner: item.owner.clone(),
                owner_kind: item.kind,
                number: number + 1,
                expectation,
                program: program_of(body),
                line: crate::diagnostic::location(text, spans[fence.open].start).0,
                span: spans[fence.open],
            });
        }
    }
    if errors.is_empty() {
        Ok(examples)
    } else {
        Err(errors)
    }
}

/// The module name as source code writes it.
fn module_display(name: &str) -> String {
    name.replace('.', "::")
}

/// The sources whose examples `tsuzuri test --doc` checks: all of a std project, else the root
/// package's own.
fn checked_sources(project: &Project) -> Vec<usize> {
    let std_mode = !project
        .sources
        .iter()
        .any(|source| source.origin == ModuleOrigin::User);
    (0..project.sources.len())
        .filter(|&index| {
            let source = &project.sources[index];
            if std_mode {
                source.origin == ModuleOrigin::Std
            } else {
                source.origin == ModuleOrigin::User && project.dependency_package(source).is_none()
            }
        })
        .collect()
}

/// `E1017` when a package has more examples than `MAX_DOC_EXAMPLES`.
fn check_limit(count: usize) -> Result<(), Diagnostic> {
    if count > MAX_DOC_EXAMPLES {
        return Err(Diagnostic::of(
            DiagnosticKind::DocExampleLimit,
            format!("the package has more than {MAX_DOC_EXAMPLES} doc examples; split the package"),
            Span::default(),
        ));
    }
    Ok(())
}

/// Every example of the project, numbered, or the notation errors of all its sources.
pub fn collect(project: &Project) -> Result<Vec<Example>, DiagnosticSet> {
    let mut examples = Vec::new();
    let mut errors = Vec::new();
    for index in checked_sources(project) {
        let source = &project.sources[index];
        match examples_in_source(
            &source.text,
            source.edition,
            index,
            &module_display(&source.name),
        ) {
            Ok(found) => examples.extend(found),
            Err(found) => errors.extend(found),
        }
    }
    if !errors.is_empty() {
        errors.truncate(MAX_NOTATION_ERRORS);
        return Err(DiagnosticSet::from_diagnostics(errors, 0));
    }
    if let Err(error) = check_limit(examples.len()) {
        return Err(DiagnosticSet::from_diagnostics([error], 0));
    }
    for (index, example) in examples.iter_mut().enumerate() {
        example.index = index;
    }
    Ok(examples)
}

/// `\r\n` as `\n`, without trailing whitespace.
fn normalize(text: &str) -> String {
    text.replace("\r\n", "\n").trim_end().to_owned()
}

/// A text for a failure message: `{:?}`, cut after 200 characters.
fn shown(text: &str) -> String {
    let mut text: String = text.to_owned();
    if text.chars().count() > 200 {
        text = text.chars().take(200).collect::<String>();
        return format!("{text:?}...");
    }
    format!("{text:?}")
}

/// ` (example line L, column C)` for a diagnostic in the example's `Main`.
fn location_suffix(diagnostic: &Diagnostic, program: &str, root: usize) -> String {
    let span = diagnostic.span;
    if span.source.unwrap_or(root) != root || span.start == span.end {
        return String::new();
    }
    let (line, column) = crate::diagnostic::location(program, span.start);
    format!(" (example line {line}, column {column})")
}

fn describe(diagnostic: &Diagnostic, program: &str, root: usize) -> String {
    format!(
        "{}: {}{}",
        diagnostic.code,
        diagnostic.message,
        location_suffix(diagnostic, program, root)
    )
}

/// Whether the example belongs to the `Main.tz` of the root package, which it would replace.
fn in_main(project: &Project, example: &Example) -> bool {
    let source = &project.sources[example.source];
    source.origin == ModuleOrigin::User
        && source.relative_path == Path::new("Main.tz")
        && project.dependency_package(source).is_none()
}

/// Runs the examples that `options` selects, in parallel, and reports them in order.
pub fn run(project: &Project, input: &Path, examples: &[Example], options: &Options) -> Report {
    let started = Instant::now();
    let mut report = Report::default();
    let mut main_examples = 0;
    let mut runnable = Vec::new();
    for example in examples {
        if !options.includes(example) || example.expectation == Expectation::Ignore {
            report.ignored += 1;
        } else if in_main(project, example) {
            report.ignored += 1;
            main_examples += 1;
        } else {
            runnable.push(example);
        }
    }
    let std_mode = !project
        .sources
        .iter()
        .any(|source| source.origin == ModuleOrigin::User);
    // A project without a manifest reads its sources in the edition that `--edition` chose (A22);
    // the example's project is loaded again, so it gets the same edition.
    let edition = project
        .root_package
        .is_none()
        .then(|| {
            project
                .sources
                .iter()
                .find(|source| source.origin == ModuleOrigin::User)
                .map(|source| source.edition)
        })
        .flatten();
    let workers = crate::parallel::Parallelism::command_line().workers(runnable.len(), 1);
    let mut notices = std::collections::BTreeSet::new();
    crate::parallel::ordered_for_each(
        runnable.len(),
        workers,
        1,
        None,
        |item| run_one(input, runnable[item], options, std_mode, edition),
        |_, item| {
            let (result, messages) = item.get();
            notices.extend(messages);
            report.results.push(result);
            ControlFlow::Continue(())
        },
    );
    if main_examples > 0 {
        let plural = if main_examples == 1 { "" } else { "s" };
        report.messages.push(format!(
            "{main_examples} doc example{plural} in Main.tz not run; move the documented declaration to another module"
        ));
    }
    report.messages.extend(notices);
    report.duration_ms = started.elapsed().as_millis();
    report
}

/// The project that runs one example: the package with `Main.tz` replaced, or `Main.tz` alone
/// with the std modules that it names.
fn example_project(
    input: &Path,
    program: &str,
    std_mode: bool,
    edition: Option<Edition>,
) -> Result<Project, Diagnostic> {
    if std_mode {
        Ok(Project::single_main(
            PathBuf::from("<doctest>").join("Main.tz"),
            program.to_owned(),
        ))
    } else {
        let project = Project::for_doc_example(input, program).map_err(|error| error.diagnostic)?;
        match edition {
            Some(edition) => project
                .with_edition(edition)
                .map_err(|error| error.diagnostic),
            None => Ok(project),
        }
    }
}

/// The warnings of an analyzed example project, as `check` reports them under `settings`.
fn example_warnings(
    built: &Project,
    module: &crate::check::CheckedModule,
    settings: Warnings,
) -> Result<Vec<Diagnostic>, Diagnostic> {
    let mut warnings = module.warnings.clone();
    if settings.implicit_copy {
        warnings.extend(crate::copies::warnings(module));
    }
    if settings.canonical {
        warnings.extend(crate::canonical::warnings(module));
    }
    if settings.untrusted_extern {
        warnings.extend(built.untrusted_extern_warnings(module)?);
    }
    Ok(warnings)
}

fn run_one(
    input: &Path,
    example: &Example,
    options: &Options,
    std_mode: bool,
    edition: Option<Edition>,
) -> (ExampleResult, Vec<String>) {
    let started = Instant::now();
    let mut messages = Vec::new();
    let mut output = String::new();
    let failure = (|| -> Option<String> {
        let built = match example_project(input, &example.program, std_mode, edition) {
            Ok(built) => built,
            Err(error) => return Some(format!("{}: {}", error.code, error.message)),
        };
        let root = built.root;
        let program = &example.program;
        // E15: the package's `[capabilities]` policy binds its examples as it binds `check`.
        let analyzed = built.analyze_all().and_then(|module| {
            let denied = built
                .check_capabilities(&module)
                .unwrap_or_else(|error| vec![error]);
            if denied.is_empty() {
                Ok(module)
            } else {
                Err(DiagnosticSet::from_diagnostics(denied, 0))
            }
        });
        if let Expectation::Fails(code) = &example.expectation {
            return match analyzed {
                Ok(_) => Some(format!("expected {code}, but the example was accepted")),
                Err(errors) => {
                    let first = errors.diagnostics.first()?;
                    (first.code != code).then(|| {
                        format!("expected {code}, found {}", describe(first, program, root))
                    })
                }
            };
        }
        let module = match analyzed {
            Ok(module) => module,
            Err(errors) => {
                return errors
                    .diagnostics
                    .first()
                    .map(|first| describe(first, program, root));
            }
        };
        if options.warnings.deny {
            match example_warnings(&built, &module, options.warnings) {
                Ok(warnings) => {
                    // The package's own warnings were reported (and denied) before any example ran.
                    if let Some(first) = warnings
                        .iter()
                        .find(|warning| warning.span.source.unwrap_or(root) == root)
                    {
                        return Some(describe(first, program, root));
                    }
                }
                Err(error) => return Some(describe(&error, program, root)),
            }
        }
        match &example.expectation {
            Expectation::Check | Expectation::Fails(_) | Expectation::Ignore => None,
            Expectation::Run(expected) => {
                match driver::run_captured_quiet(
                    &module,
                    &built,
                    options.build,
                    &built.native,
                    Some(EXAMPLE_TIMEOUT),
                ) {
                    Ok((notices, stdout)) => {
                        messages.extend(notices);
                        let actual = normalize(&String::from_utf8_lossy(&stdout));
                        let expected = normalize(expected);
                        (actual != expected).then(|| {
                            format!(
                                "expected output {}, found {}",
                                shown(&expected),
                                shown(&actual)
                            )
                        })
                    }
                    Err(error) => Some(describe(&error, program, root)),
                }
            }
            Expectation::Test => {
                let indices: Vec<usize> = module
                    .tests
                    .iter()
                    .filter(|case| case.span.source.unwrap_or(root) == root)
                    .map(|case| case.index)
                    .collect();
                if indices.is_empty() {
                    return Some("the example declares no test".to_owned());
                }
                let test_options = driver::TestOptions {
                    optimization: options.level,
                    indices,
                    backend: options.build.backend,
                    allocator: options.build.allocator,
                    ..Default::default()
                };
                match driver::run_tests_linked(&module, &test_options, &built.native) {
                    Ok(test_report) => {
                        messages.extend(test_report.messages);
                        let failed = test_report
                            .results
                            .into_iter()
                            .find(|result| result.failure.is_some())?;
                        output = failed.output;
                        Some(format!(
                            "test {:?} failed: {}",
                            failed.case.name,
                            failed.failure.unwrap_or_default()
                        ))
                    }
                    Err(error) => Some(describe(&error, program, root)),
                }
            }
        }
    })();
    (
        ExampleResult {
            example: example.clone(),
            failure,
            output,
            duration_ms: started.elapsed().as_millis(),
        },
        messages,
    )
}

/// The documentation as `tsuzuri doc` and the hover show it: an example with options or hidden
/// lines loses its options and hidden lines, and shows its expected output or error code after
/// it. Other text, and an example without either, stays byte for byte.
pub fn render_doc(text: &str) -> String {
    if !may_have_examples(text) {
        return text.to_owned();
    }
    let lines: Vec<&str> = text.split('\n').collect();
    let found = scan(&lines);
    let mut out: Vec<String> = Vec::new();
    let mut next = 0;
    for fence in &found.fences {
        out.extend(
            lines[next..fence.open]
                .iter()
                .map(|line| (*line).to_owned()),
        );
        next = fence.close + 1;
        let original = &lines[fence.open..=fence.close];
        let options = fence.info["tsuzuri".len()..].trim();
        let body = &lines[fence.open + 1..fence.close];
        let hidden = body.iter().any(|line| is_hidden(line));
        let expectation = match parse_options(options) {
            Ok(_) if options.is_empty() && !hidden => {
                out.extend(original.iter().map(|line| (*line).to_owned()));
                continue;
            }
            Ok(expectation) => expectation,
            Err(_) => {
                out.extend(original.iter().map(|line| (*line).to_owned()));
                continue;
            }
        };
        let ticks = "`".repeat(fence.ticks);
        out.push(format!("{ticks}tsuzuri"));
        out.extend(
            body.iter()
                .filter(|line| !is_hidden(line))
                .map(|line| (*line).to_owned()),
        );
        out.push(lines[fence.close].to_owned());
        match expectation {
            Expectation::Run(output) if !output.trim_end().is_empty() => {
                let output = output.trim_end();
                let mut fence_ticks = 3;
                while output.contains(&"`".repeat(fence_ticks)) {
                    fence_ticks += 1;
                }
                let outer = "`".repeat(fence_ticks);
                out.push(String::new());
                out.push("Output:".to_owned());
                out.push(String::new());
                out.push(format!("{outer}text"));
                out.extend(output.split('\n').map(str::to_owned));
                out.push(outer);
            }
            Expectation::Fails(code) => {
                out.push(String::new());
                out.push(format!("Compile error: `{code}`."));
            }
            _ => {}
        }
    }
    out.extend(lines[next..].iter().map(|line| (*line).to_owned()));
    out.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_percent_escapes_like_decode_uri_component() {
        // Each expectation is what `decodeURIComponent` returns.
        for (token, decoded) in [
            ("", ""),
            ("%20", " "),
            ("%E3%81%82", "あ"),
            ("%2B", "+"),
            ("a+b", "a+b"),
            ("%25", "%"),
            ("[1,%202,%203]", "[1, 2, 3]"),
            ("a%0Ab", "a\nb"),
        ] {
            assert_eq!(decode_output(token).as_deref(), Ok(decoded), "{token}");
        }
    }

    #[test]
    fn rejects_malformed_percent_escapes() {
        for token in ["%", "%2", "%zz", "%FF", "%+1"] {
            assert!(decode_output(token).is_err(), "{token}");
        }
    }

    #[test]
    fn parses_each_option_and_rejects_the_rest() {
        assert_eq!(parse_options(""), Ok(Expectation::Check));
        assert_eq!(parse_options("run=3"), Ok(Expectation::Run("3".into())));
        assert_eq!(parse_options("run="), Ok(Expectation::Run(String::new())));
        assert_eq!(parse_options("test"), Ok(Expectation::Test));
        assert_eq!(parse_options("ignore"), Ok(Expectation::Ignore));
        assert_eq!(
            parse_options("fails=E1012"),
            Ok(Expectation::Fails("E1012".into()))
        );
        for bad in [
            "fails=W1001",
            "fails=e1012",
            "foo",
            "test ignore",
            "test test",
            "run=%zz",
            "run=1 test",
        ] {
            assert!(parse_options(bad).is_err(), "{bad}");
        }
        assert_eq!(
            parse_options("test test").unwrap_err(),
            "option 'test' is given twice; write it once"
        );
        assert_eq!(
            parse_options("run=1 test").unwrap_err(),
            "a tsuzuri code fence takes one of run=, test, ignore, and fails=; found 'run=' and 'test'"
        );
    }

    #[test]
    fn finds_fences_only_at_column_zero_with_the_tsuzuri_language() {
        let lines = [
            "Text.",
            "```text",
            "```tsuzuri",
            "```",
            "```tsuzuri run=1",
            "1",
            "```",
            "```",
        ];
        let found = scan(&lines);
        assert_eq!(found.problems, []);
        assert_eq!(
            found.fences,
            [Fence {
                open: 4,
                close: 6,
                ticks: 3,
                info: "tsuzuri run=1".into()
            }]
        );
    }

    #[test]
    fn longer_fences_close_only_on_an_equal_or_longer_run() {
        let lines = ["````tsuzuri", "```", "x", "`````", "after"];
        let found = scan(&lines);
        assert_eq!(found.fences.len(), 1);
        assert_eq!((found.fences[0].open, found.fences[0].close), (0, 3));
        assert_eq!(found.fences[0].ticks, 4);
    }

    #[test]
    fn unclosed_and_indented_fences_are_problems() {
        let found = scan(&[" ```tsuzuri", "x", " ```", "```tsuzuri", "x"]);
        assert_eq!(
            found.problems,
            [(0, INDENTED.to_owned()), (3, UNCLOSED.to_owned())]
        );
        // An unclosed fence of another language hides the rest and is no problem.
        assert_eq!(scan(&["```text", "```tsuzuri"]), Scan::default());
    }

    #[test]
    fn hidden_lines_lose_their_marker_and_keep_indentation() {
        assert_eq!(
            program_of(&["# let base = 10", "#     x", "#", "#x", "#!y", "base"]),
            "let base = 10\n    x\n\n#x\n#!y\nbase\n"
        );
        assert!(is_hidden("# a") && is_hidden("#") && !is_hidden("#a") && !is_hidden(" # a"));
    }

    const LIBRARY: &str = "/// Adds one.\n///\n/// ```tsuzuri run=3\n/// Lib.add_one 2\n/// ```\n\n// A plain comment.\n/// ```tsuzuri\n/// let _x = Lib.add_one 1\n/// ```\ndef add_one :: i64 -> i64 = \\n -> n + 1\n\n/// Private.\n///\n/// ```tsuzuri\n/// 1\n/// ```\nprivate def hidden :: i64 -> i64 = \\n -> n\n\n/// Equality.\nclass Same<'a> {\n    /// Compares.\n    ///\n    /// ```tsuzuri ignore\n    /// x\n    /// ```\n    def same :: 'a -> 'a -> bool\n}\n";

    #[test]
    fn examples_map_to_file_lines_across_blank_and_comment_lines() {
        let examples = examples_in_source(LIBRARY, Edition::LATEST, 2, "Lib").unwrap();
        let lines: Vec<(String, usize)> = examples.iter().map(|e| (e.name(), e.line)).collect();
        assert_eq!(
            lines,
            [
                ("add_one#1".to_owned(), 3),
                ("add_one#2".to_owned(), 8),
                ("Same.same#1".to_owned(), 24)
            ]
        );
        assert_eq!(examples[0].program, "Lib.add_one 2\n");
        assert_eq!(examples[0].span.source, Some(2));
        assert_eq!(examples[0].expectation, Expectation::Run("3".into()));
    }

    #[test]
    fn collect_skips_private_generated_and_instance_docs() {
        let examples = examples_in_source(LIBRARY, Edition::LATEST, 0, "Lib").unwrap();
        assert!(examples.iter().all(|example| example.owner != "hidden"));
        // An instance's methods take no doc comments (the parser rejects them), and a private
        // record's documentation is not shown.
        let instance = "/// Shown.\n///\n/// ```tsuzuri\n/// 1\n/// ```\nrecord Box { v: i64 }\n\n/// Hidden.\n///\n/// ```tsuzuri\n/// 2\n/// ```\nprivate record Inner { v: i64 }\n\ninstance Display<Box> {\n    fn display value = \"box\"\n}\n";
        let examples = examples_in_source(instance, Edition::LATEST, 0, "Lib").unwrap();
        assert_eq!(examples.len(), 1);
        assert_eq!(examples[0].owner_kind, OwnerKind::Record);
    }

    #[test]
    fn class_method_examples_are_named_class_dot_method() {
        let examples = examples_in_source(LIBRARY, Edition::LATEST, 0, "Geometry::Lib").unwrap();
        let method = examples.last().unwrap();
        assert_eq!(method.owner_kind, OwnerKind::Method);
        assert_eq!(method.label(), "Geometry::Lib.Same.same#1");
        assert_eq!(method.expectation.kind(), "ignore");
    }

    #[test]
    fn examples_are_numbered_per_doc_and_indexed_across_the_project() {
        let examples = examples_in_source(LIBRARY, Edition::LATEST, 0, "Lib").unwrap();
        let numbers: Vec<usize> = examples.iter().map(|example| example.number).collect();
        assert_eq!(numbers, [1, 2, 1]);
        assert!(examples.iter().all(|example| example.index == 0));
    }

    #[test]
    fn the_limit_is_ten_thousand_examples() {
        assert!(check_limit(MAX_DOC_EXAMPLES).is_ok());
        let error = check_limit(MAX_DOC_EXAMPLES + 1).unwrap_err();
        assert_eq!(error.code, "E1017");
        assert_eq!(
            error.message,
            "the package has more than 10000 doc examples; split the package"
        );
    }

    #[test]
    fn normalizes_line_endings_and_trailing_whitespace_only() {
        assert_eq!(normalize("  a\r\nb \n\t\n"), "  a\nb");
        assert_eq!(normalize(""), "");
    }

    #[test]
    fn shown_truncates_at_200_characters() {
        assert_eq!(shown("a\"b"), "\"a\\\"b\"");
        let long = "x".repeat(250);
        assert_eq!(shown(&long), format!("{:?}...", "x".repeat(200)));
    }

    #[test]
    fn location_suffix_uses_example_lines() {
        let error = Diagnostic::of(DiagnosticKind::TypeMismatch, "m", Span::new(6, 7));
        assert_eq!(
            location_suffix(&error, "let a\nlet b\n", 0),
            " (example line 2, column 1)"
        );
        let elsewhere = Diagnostic::of(
            DiagnosticKind::TypeMismatch,
            "m",
            Span::new(6, 7).in_source(3),
        );
        assert_eq!(location_suffix(&elsewhere, "let a\nlet b\n", 0), "");
        let empty = Diagnostic::of(DiagnosticKind::TypeMismatch, "m", Span::default());
        assert_eq!(location_suffix(&empty, "x", 0), "");
    }

    #[test]
    fn render_doc_is_the_identity_without_options_or_hidden_lines() {
        for text in [
            "First.\n```tsuzuri\nfirst()\n```",
            "No fence at all.",
            "```text\n# not hidden\n```",
        ] {
            assert_eq!(render_doc(text), text);
        }
    }

    #[test]
    fn render_doc_hides_lines_and_shows_outputs() {
        let text =
            "Adds one.\n\n```tsuzuri run=11\n# let base = 10\nCounter.add_one base\n```\n\nAfter.";
        assert_eq!(
            render_doc(text),
            "Adds one.\n\n```tsuzuri\nCounter.add_one base\n```\n\nOutput:\n\n```text\n11\n```\n\nAfter."
        );
        // An empty output adds nothing.
        assert_eq!(
            render_doc("```tsuzuri run=\n()\n```"),
            "```tsuzuri\n()\n```"
        );
    }

    #[test]
    fn render_doc_shows_the_error_code_of_a_failing_example() {
        assert_eq!(
            render_doc("```tsuzuri fails=E1003\nCounter.add_one \"two\"\n```"),
            "```tsuzuri\nCounter.add_one \"two\"\n```\n\nCompile error: `E1003`."
        );
    }

    #[test]
    fn render_doc_keeps_malformed_fences() {
        for text in ["```tsuzuri run=%zz\n1\n```", "```tsuzuri sloppy\n1\n```"] {
            assert_eq!(render_doc(text), text);
        }
    }

    #[test]
    fn render_doc_uses_a_longer_fence_for_outputs_with_backticks() {
        assert_eq!(
            render_doc("```tsuzuri run=%60%60%60\nx\n```"),
            "```tsuzuri\nx\n```\n\nOutput:\n\n````text\n```\n````"
        );
    }
}
