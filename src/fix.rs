//! `tsuzuri fix` (G24): applies the fixes of diagnostics. A round collects one fix per
//! diagnostic, at the requested applicability or above, that only edits the project's own
//! sources, drops those that overlap one already taken, applies them in memory, and analyzes the
//! project again; a round that makes the diagnostics worse aborts without a change. Rounds repeat
//! until no fix applies or `MAX_ROUNDS`, and the changed files are then written together.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fmt::Write;
use std::path::PathBuf;

use crate::check::{CheckedModule, ModuleOrigin};
use crate::diagnostic::{
    Applicability, Diagnostic, DiagnosticKind, DiagnosticSet, Fix, Span, json_string, location,
};
use crate::driver::{
    AnalysisKind, FileUpdate, Project, ReplaceError, ReplaceFailure, SourceError, replace_files,
};
use crate::syntax::{Edition, MAX_SOURCE_BYTES};

/// The most rounds of one run; the fixes that remain are reported as `round_limit`.
pub const MAX_ROUNDS: usize = 5;
/// The most fixes that one round applies.
pub const MAX_CANDIDATES: usize = 20_000;
/// The longest text of one edit, in bytes.
pub const MAX_EDIT_TEXT: usize = 4096;

/// The lowest applicability that a run applies.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Threshold {
    Certain,
    Likely,
    Maybe,
}

impl Threshold {
    pub fn admits(self, applicability: Applicability) -> bool {
        match self {
            Self::Certain => applicability == Applicability::Certain,
            Self::Likely => applicability != Applicability::Maybe,
            Self::Maybe => true,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Certain => "certain",
            Self::Likely => "likely",
            Self::Maybe => "maybe",
        }
    }
}

/// The sources that a run may change: all of the project's own, or one, by its index.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    Project,
    File(usize),
}

pub struct Options {
    pub threshold: Threshold,
    pub scope: Scope,
    /// Only the diagnostics of these codes; the others are neither applied nor reported.
    pub only_codes: Option<&'static [&'static str]>,
}

/// The diagnostics of one analysis: the errors, or the warnings of a successful one.
#[derive(Clone, Debug)]
pub struct Analysis {
    pub errors: DiagnosticSet,
    pub warnings: Vec<Diagnostic>,
}

impl Analysis {
    fn error_total(&self) -> usize {
        self.errors.diagnostics.len() + self.errors.omitted
    }
}

/// Why a fix was not applied.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Skip {
    /// Below the requested applicability.
    Applicability,
    /// It edits a dependency, the standard library, or no source of the project.
    OutsideProject,
    /// It edits another file than the one that was given.
    OutsideInput,
    /// An edit is out of range, splits a character, is too long, overlaps another edit of the
    /// fix, or the fix has no edit.
    InvalidEdit,
    /// It is left for another run after `MAX_ROUNDS`.
    RoundLimit,
}

impl Skip {
    pub fn name(self) -> &'static str {
        match self {
            Self::Applicability => "applicability",
            Self::OutsideProject => "outside_project",
            Self::OutsideInput => "outside_input",
            Self::InvalidEdit => "invalid_edit",
            Self::RoundLimit => "round_limit",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Applied { round: usize },
    Skipped(Skip),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FixRecord {
    pub status: Status,
    pub code: &'static str,
    pub kind: Option<&'static str>,
    pub applicability: Applicability,
    pub title: String,
    pub edits: Vec<RecordedEdit>,
}

/// An edit at the text of the round that collected it. The line and the column start at 1, the
/// column counts Unicode scalar values; an index outside the project is `<invalid>` at line 0.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecordedEdit {
    pub path: String,
    pub start: usize,
    pub end: usize,
    pub line: usize,
    pub column: usize,
    pub end_line: usize,
    pub end_column: usize,
    pub text: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stop {
    FixedPoint,
    RoundLimit,
}

/// A source that the run changed, with the number of applied fixes that edit it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileChange {
    pub source: usize,
    pub original: String,
    pub fixed: String,
    pub fixes: usize,
}

#[derive(Clone, Debug)]
pub struct Report {
    pub rounds: usize,
    pub stop: Stop,
    /// The applied fixes in the order of application, then the skipped ones.
    pub records: Vec<FixRecord>,
    pub files: Vec<FileChange>,
    /// The analysis of the fixed text.
    pub last: Analysis,
}

/// The warnings that bring a source to the forms that every edition accepts (A22's `W1007`),
/// each with a `certain` fix. `tsuzuri fix --edition` applies them before it migrates. No such
/// warning exists yet, so this is empty.
pub fn canonical_warnings(_project: &Project, _module: &CheckedModule) -> Vec<Diagnostic> {
    Vec::new()
}

/// Analyzes the project like `check`, without the frontend cache; with `canonical`, the
/// canonical-form warnings are added to the warnings of a successful analysis.
pub fn analyze_project(project: &Project, canonical: bool) -> Analysis {
    match project.analyze_cached(false, AnalysisKind::Program) {
        Ok(module) => {
            let mut warnings = module.warnings.clone();
            if canonical {
                warnings.extend(canonical_warnings(project, &module));
            }
            Analysis {
                errors: DiagnosticSet::from_diagnostics([], project.root),
                warnings,
            }
        }
        Err(errors) => Analysis {
            errors,
            warnings: Vec::new(),
        },
    }
}

/// An edit resolved to a source index.
#[derive(Clone, Debug, PartialEq, Eq)]
struct ResolvedEdit {
    source: usize,
    start: usize,
    end: usize,
    text: String,
}

struct Candidate {
    record: FixRecord,
    edits: Vec<ResolvedEdit>,
}

fn command_line_error(kind: DiagnosticKind, message: impl Into<String>) -> SourceError {
    SourceError {
        path: PathBuf::from("<command line>"),
        diagnostic: Diagnostic::of(kind, message, Span::default()),
    }
}

/// The path of a source, relative to the root and with `/` separators.
fn display_path(project: &Project, source: usize) -> String {
    project.sources.get(source).map_or_else(
        || "<invalid>".to_owned(),
        |file| {
            file.relative_path
                .components()
                .map(|part| part.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/")
        },
    )
}

fn record(project: &Project, diagnostic: &Diagnostic, fix: &Fix, status: Status) -> FixRecord {
    let edits = fix
        .edits
        .iter()
        .map(|edit| {
            let source = edit.span.source.unwrap_or(project.root);
            let (path, (line, column), (end_line, end_column)) = match project.sources.get(source) {
                Some(file) => (
                    display_path(project, source),
                    location(&file.text, edit.span.start),
                    location(&file.text, edit.span.end),
                ),
                None => ("<invalid>".to_owned(), (0, 0), (0, 0)),
            };
            RecordedEdit {
                path,
                start: edit.span.start,
                end: edit.span.end,
                line,
                column,
                end_line,
                end_column,
                text: edit.text.clone(),
            }
        })
        .collect();
    FixRecord {
        status,
        code: diagnostic.code,
        kind: diagnostic.kind,
        applicability: fix.applicability,
        title: fix.title.clone(),
        edits,
    }
}

/// The edits of `fix` when it may be applied: the checks run in order, each over all edits, and
/// the first that fails names the reason.
fn validate(project: &Project, fix: &Fix, scope: Scope) -> Result<Vec<ResolvedEdit>, Skip> {
    let edits: Vec<ResolvedEdit> = fix
        .edits
        .iter()
        .map(|edit| ResolvedEdit {
            source: edit.span.source.unwrap_or(project.root),
            start: edit.span.start,
            end: edit.span.end,
            text: edit.text.clone(),
        })
        .collect();
    let file = |edit: &ResolvedEdit| project.sources.get(edit.source);
    if edits.iter().any(|edit| {
        file(edit).is_none_or(|file| {
            file.origin != ModuleOrigin::User || project.dependency_package(file).is_some()
        })
    }) {
        return Err(Skip::OutsideProject);
    }
    if let Scope::File(input) = scope
        && edits.iter().any(|edit| edit.source != input)
    {
        return Err(Skip::OutsideInput);
    }
    if edits.iter().any(|edit| {
        let text = &project.sources[edit.source].text;
        edit.start > edit.end
            || edit.end > text.len()
            || !text.is_char_boundary(edit.start)
            || !text.is_char_boundary(edit.end)
    }) {
        return Err(Skip::InvalidEdit);
    }
    if edits.iter().any(|edit| edit.text.len() > MAX_EDIT_TEXT)
        || grown_sizes(project, &edits, &BTreeMap::new()).is_none()
    {
        return Err(Skip::InvalidEdit);
    }
    let mut sorted: Vec<&ResolvedEdit> = edits.iter().collect();
    sorted.sort_by_key(|edit| (edit.source, edit.start, edit.end));
    if sorted
        .windows(2)
        .any(|pair| pair[0].source == pair[1].source && pair[0].end > pair[1].start)
        || edits.is_empty()
    {
        return Err(Skip::InvalidEdit);
    }
    Ok(edits)
}

/// The size of each source that `edits` change, from `sizes` (the sizes after the fixes taken
/// before) or the source's text; `None` when one would exceed `MAX_SOURCE_BYTES` or the edits
/// remove more than the source holds (overlapping deletions).
fn grown_sizes(
    project: &Project,
    edits: &[ResolvedEdit],
    sizes: &BTreeMap<usize, usize>,
) -> Option<BTreeMap<usize, usize>> {
    let mut grown: BTreeMap<usize, usize> = BTreeMap::new();
    for edit in edits {
        let size = grown.entry(edit.source).or_insert_with(|| {
            sizes
                .get(&edit.source)
                .copied()
                .unwrap_or(project.sources[edit.source].text.len())
        });
        *size = size
            .checked_add(edit.text.len())?
            .checked_sub(edit.end - edit.start)?;
    }
    grown
        .values()
        .all(|size| *size <= MAX_SOURCE_BYTES)
        .then_some(grown)
}

/// Whether the closed interval `[start, end]` meets one already taken in `accepted`. The taken
/// intervals are disjoint, so the last one that starts at or before `end` decides.
fn conflicts(accepted: &BTreeMap<usize, BTreeMap<usize, usize>>, edit: &ResolvedEdit) -> bool {
    accepted.get(&edit.source).is_some_and(|taken| {
        taken
            .range(..=edit.end)
            .next_back()
            .is_some_and(|(_, end)| *end >= edit.start)
    })
}

/// The fixes of one round, and the ones that cannot be applied. Of overlapping candidates, the
/// first in sorted order is taken; the others wait for a later round and are not recorded.
fn collect(
    project: &Project,
    analysis: &Analysis,
    options: &Options,
) -> Result<(Vec<Candidate>, Vec<FixRecord>), SourceError> {
    let mut candidates = Vec::new();
    let mut skipped = Vec::new();
    for diagnostic in analysis.errors.diagnostics.iter().chain(&analysis.warnings) {
        if diagnostic.fixes.is_empty()
            || options
                .only_codes
                .is_some_and(|codes| !codes.contains(&diagnostic.code))
        {
            continue;
        }
        let Some(fix) = diagnostic
            .fixes
            .iter()
            .find(|fix| options.threshold.admits(fix.applicability))
        else {
            skipped.push(record(
                project,
                diagnostic,
                &diagnostic.fixes[0],
                Status::Skipped(Skip::Applicability),
            ));
            continue;
        };
        match validate(project, fix, options.scope) {
            Ok(edits) => candidates.push(Candidate {
                record: record(project, diagnostic, fix, Status::Applied { round: 0 }),
                edits,
            }),
            Err(skip) => skipped.push(record(project, diagnostic, fix, Status::Skipped(skip))),
        }
    }
    let key = |candidate: &Candidate| {
        let first = &candidate.edits[0];
        (
            (first.source, first.start, first.end),
            candidate.record.code,
            candidate.record.title.clone(),
            candidate
                .edits
                .iter()
                .map(|edit| (edit.source, edit.start, edit.end, edit.text.clone()))
                .collect::<Vec<_>>(),
        )
    };
    candidates.sort_by_cached_key(key);
    candidates.dedup_by(|later, earlier| later.edits == earlier.edits);
    let mut accepted: BTreeMap<usize, BTreeMap<usize, usize>> = BTreeMap::new();
    // The size of each source after the fixes taken so far: together they must stay within the
    // limit, though each fits alone.
    let mut sizes: BTreeMap<usize, usize> = BTreeMap::new();
    let mut chosen = Vec::new();
    for candidate in candidates {
        if candidate
            .edits
            .iter()
            .any(|edit| conflicts(&accepted, edit))
        {
            continue;
        }
        let Some(grown) = grown_sizes(project, &candidate.edits, &sizes) else {
            skipped.push(FixRecord {
                status: Status::Skipped(Skip::InvalidEdit),
                ..candidate.record
            });
            continue;
        };
        sizes.extend(grown);
        for edit in &candidate.edits {
            // Edits of one fix may share a start (an insert and a replacement): keep the furthest end.
            let end = accepted
                .entry(edit.source)
                .or_default()
                .entry(edit.start)
                .or_insert(edit.end);
            *end = (*end).max(edit.end);
        }
        chosen.push(candidate);
    }
    if chosen.len() > MAX_CANDIDATES {
        return Err(command_line_error(
            DiagnosticKind::FixCandidateLimit,
            format!(
                "too many fixes (more than {MAX_CANDIDATES}) in one round; run tsuzuri fix on one file at a time"
            ),
        ));
    }
    Ok((chosen, skipped))
}

/// Applies the edits of `chosen` to the project's texts, each file in one pass, saving the text
/// of a file before its first change.
fn apply(project: &mut Project, chosen: &[Candidate], saved: &mut BTreeMap<usize, String>) {
    let mut by_source: BTreeMap<usize, Vec<&ResolvedEdit>> = BTreeMap::new();
    for edit in chosen.iter().flat_map(|candidate| &candidate.edits) {
        by_source.entry(edit.source).or_default().push(edit);
    }
    for (source, mut edits) in by_source {
        edits.sort_by_key(|edit| (edit.start, edit.end));
        let text = &project.sources[source].text;
        saved.entry(source).or_insert_with(|| text.clone());
        project.sources[source].text = splice(text, &edits);
    }
}

/// `text` with the sorted, disjoint `edits` applied.
fn splice(text: &str, edits: &[&ResolvedEdit]) -> String {
    let mut result = String::with_capacity(text.len());
    let mut at = 0;
    for edit in edits {
        result.push_str(&text[at..edit.start]);
        result.push_str(&edit.text);
        at = edit.end;
    }
    result.push_str(&text[at..]);
    result
}

fn restore(project: &mut Project, saved: &BTreeMap<usize, String>) {
    for (source, text) in saved {
        project.sources[*source].text = text.clone();
    }
}

type Counts = BTreeMap<(String, &'static str), usize>;

fn counts(project: &Project, diagnostics: &[Diagnostic]) -> Counts {
    let mut counts = Counts::new();
    for diagnostic in diagnostics {
        let source = diagnostic.span.source.unwrap_or(project.root);
        *counts
            .entry((display_path(project, source), diagnostic.code))
            .or_default() += 1;
    }
    counts
}

/// The first diagnostic of `after` whose (path, code) is more frequent than in `before`.
fn first_increase<'a>(
    project: &Project,
    before: &[Diagnostic],
    after: &'a [Diagnostic],
) -> Option<&'a Diagnostic> {
    let (before, increased) = (counts(project, before), counts(project, after));
    after.iter().find(|diagnostic| {
        let source = diagnostic.span.source.unwrap_or(project.root);
        let key = (display_path(project, source), diagnostic.code);
        increased[&key] > before.get(&key).copied().unwrap_or(0)
    })
}

/// The diagnostic that shows a round made the analysis worse: more errors (V1), more errors of
/// a file and a code when no error was omitted (V2), or more warnings of a file and a code when
/// there was no error (V3).
fn worsened<'a>(
    project: &Project,
    before: &Analysis,
    after: &'a Analysis,
) -> Option<&'a Diagnostic> {
    if before.errors.omitted == 0
        && let Some(diagnostic) = first_increase(
            project,
            &before.errors.diagnostics,
            &after.errors.diagnostics,
        )
    {
        return Some(diagnostic);
    }
    if after.error_total() > before.error_total()
        || (before.error_total() == 0 && after.error_total() > 0)
    {
        return after.errors.diagnostics.first().or(after.warnings.first());
    }
    if before.error_total() == 0 {
        return first_increase(project, &before.warnings, &after.warnings);
    }
    None
}

/// Applies fixes round by round until none applies or `MAX_ROUNDS`; `analyze` analyzes the
/// project's current texts. A round that makes the diagnostics worse restores every text and
/// fails with `E2008`. The project's texts are the fixed ones afterwards; no file is written.
pub fn run(
    project: &mut Project,
    options: &Options,
    analyze: &mut dyn FnMut(&Project) -> Analysis,
) -> Result<Report, SourceError> {
    let mut saved = BTreeMap::new();
    let mut applied: Vec<(FixRecord, Vec<ResolvedEdit>)> = Vec::new();
    let mut current = analyze(project);
    let mut rounds = 0;
    let (stop, skipped) = loop {
        let (chosen, mut skipped) = collect(project, &current, options)?;
        if chosen.is_empty() {
            break (Stop::FixedPoint, skipped);
        }
        if rounds == MAX_ROUNDS {
            skipped.extend(chosen.into_iter().map(|candidate| FixRecord {
                status: Status::Skipped(Skip::RoundLimit),
                ..candidate.record
            }));
            break (Stop::RoundLimit, skipped);
        }
        apply(project, &chosen, &mut saved);
        let next = analyze(project);
        if let Some(diagnostic) = worsened(project, &current, &next) {
            let source = diagnostic.span.source.unwrap_or(project.root);
            let text = project.sources.get(source).map_or("", |file| &file.text);
            let message = format!(
                "the fixes would introduce {} in '{}' at line {}: {}; no file was changed, review the fixes with --dry-run --json",
                diagnostic.code,
                display_path(project, source),
                location(text, diagnostic.span.start).0,
                diagnostic.message
            );
            restore(project, &saved);
            return Err(command_line_error(
                DiagnosticKind::FixVerificationFailed,
                message,
            ));
        }
        rounds += 1;
        applied.extend(chosen.into_iter().map(|candidate| {
            (
                FixRecord {
                    status: Status::Applied { round: rounds },
                    ..candidate.record
                },
                candidate.edits,
            )
        }));
        current = next;
    };
    let files = saved
        .into_iter()
        .filter(|(source, original)| project.sources[*source].text != *original)
        .map(|(source, original)| FileChange {
            source,
            fixed: project.sources[source].text.clone(),
            original,
            fixes: applied
                .iter()
                .filter(|(_, edits)| edits.iter().any(|edit| edit.source == source))
                .count(),
        })
        .collect();
    let records = applied
        .into_iter()
        .map(|(record, _)| record)
        .chain(skipped)
        .collect();
    Ok(Report {
        rounds,
        stop,
        records,
        files,
        last: current,
    })
}

/// The arguments of `tsuzuri fix`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Arguments {
    pub input: PathBuf,
    pub threshold: Threshold,
    pub dry_run: bool,
    pub json: bool,
    /// `--edition`: migrate to this edition instead of applying the diagnostics' fixes.
    pub edition: Option<Edition>,
    /// `--from`: the edition that an input without a manifest was written in.
    pub from: Option<Edition>,
}

pub const USAGE: &str = "tsuzuri fix [--applicability certain|likely|maybe] [--dry-run] [--json] PATH | tsuzuri fix --edition YEAR [--from YEAR] [--dry-run] [--json] PATH";

fn usage_error(kind: DiagnosticKind, message: impl Into<String>) -> Diagnostic {
    Diagnostic::of(kind, message, Span::default())
}

/// Parses the arguments after `fix`; every error is an `E2000`.
pub fn parse_arguments(arguments: &[OsString]) -> Result<Arguments, Diagnostic> {
    let bad = |message: String| usage_error(DiagnosticKind::FixBadArguments, message);
    let mut paths = Vec::new();
    let mut threshold = None;
    let (mut dry_run, mut json, mut edition, mut from) = (false, false, None, None);
    let mut seen = std::collections::BTreeSet::new();
    let mut words = arguments.iter();
    let mut paths_only = false;
    while let Some(argument) = words.next() {
        let word = argument.to_string_lossy();
        if paths_only || !word.starts_with('-') {
            paths.push(PathBuf::from(argument));
            continue;
        }
        if word == "--" {
            paths_only = true;
            continue;
        }
        if !seen.insert(word.to_string()) {
            return Err(bad(format!("{word} specified more than once")));
        }
        match word.as_ref() {
            "--dry-run" => dry_run = true,
            "--json" => json = true,
            "--applicability" => {
                threshold = Some(match words.next().and_then(|value| value.to_str()) {
                    Some("certain") => Threshold::Certain,
                    Some("likely") => Threshold::Likely,
                    Some("maybe") => Threshold::Maybe,
                    _ => {
                        return Err(bad(
                            "--applicability needs one of certain, likely, or maybe".to_owned(),
                        ));
                    }
                });
            }
            "--edition" | "--from" => {
                let value = words
                    .next()
                    .and_then(|value| value.to_str())
                    .ok_or_else(|| bad(format!("{word} needs a year such as 2027")))?;
                let parsed = Edition::parse(value).ok_or_else(|| {
                    usage_error(
                        DiagnosticKind::FixEditionUnsupported,
                        format!(
                            "unsupported edition '{value}'; supported editions: {}",
                            Edition::supported_names()
                        ),
                    )
                })?;
                if word == "--edition" {
                    edition = Some(parsed);
                } else {
                    from = Some(parsed);
                }
            }
            _ => {
                return Err(bad(format!(
                    "unknown option '{word}' for fix; supported: --applicability, --dry-run, --edition, --from, --json"
                )));
            }
        }
    }
    let [input] = <[PathBuf; 1]>::try_from(paths).map_err(|_| {
        bad(format!(
            "fix takes one .tz, .tt, or .tc file or project directory; usage: {USAGE}"
        ))
    })?;
    if edition.is_some() && threshold.is_some() {
        return Err(usage_error(
            DiagnosticKind::FixEditionWithApplicability,
            "--edition applies only certain fixes; remove --applicability",
        ));
    }
    if from.is_some() && edition.is_none() {
        return Err(bad(
            "--from names the edition that the input was written in; use it with --edition"
                .to_owned(),
        ));
    }
    let threshold = threshold.unwrap_or(Threshold::Certain);
    if threshold == Threshold::Maybe && !dry_run {
        return Err(usage_error(
            DiagnosticKind::FixMaybeNeedsDryRun,
            "--applicability maybe only shows suggestions; add --dry-run to review them, because maybe fixes are never applied",
        ));
    }
    Ok(Arguments {
        input,
        threshold,
        dry_run,
        json,
        edition,
        from,
    })
}

fn plural(count: usize, one: &str, many: &str) -> String {
    format!("{count} {}", if count == 1 { one } else { many })
}

/// The lines of `tsuzuri fix` on standard output.
pub(crate) fn render_text(
    report: &Report,
    project: &Project,
    dry_run: bool,
    threshold: Threshold,
) -> String {
    let mut text = String::new();
    for change in &report.files {
        let path = display_path(project, change.source);
        if dry_run {
            match crate::diff::unified(&path, &change.original, &change.fixed, 3) {
                Some(diff) => text.push_str(&diff),
                None => {
                    let _ = writeln!(
                        text,
                        "{path}: diff omitted (more than {} changed lines)",
                        crate::diff::MAX_DIFF_CHANGES
                    );
                }
            }
        } else {
            let _ = writeln!(
                text,
                "fixed {path}: {}",
                plural(change.fixes, "fix", "fixes")
            );
        }
    }
    let applied = report
        .records
        .iter()
        .filter(|record| matches!(record.status, Status::Applied { .. }))
        .count();
    let skipped = report.records.len() - applied;
    if applied == 0 {
        text.push_str("no fixes to apply\n");
    } else {
        let _ = writeln!(
            text,
            "{} {} in {}",
            if dry_run { "would apply" } else { "applied" },
            plural(applied, "fix", "fixes"),
            plural(report.files.len(), "file", "files")
        );
    }
    if skipped > 0 {
        let _ = writeln!(
            text,
            "skipped {} (use --json for the list)",
            plural(skipped, "fix", "fixes")
        );
    }
    let likely = report
        .records
        .iter()
        .filter(|record| {
            record.status == Status::Skipped(Skip::Applicability)
                && record.applicability == Applicability::Likely
        })
        .count();
    if threshold == Threshold::Certain && likely > 0 {
        let _ = writeln!(
            text,
            "note: rerun with --applicability likely to apply {} more {}",
            likely,
            if likely == 1 { "fix" } else { "fixes" }
        );
    }
    if report.stop == Stop::RoundLimit {
        let remaining = report
            .records
            .iter()
            .filter(|record| record.status == Status::Skipped(Skip::RoundLimit))
            .count();
        let _ = writeln!(
            text,
            "note: stopped after {MAX_ROUNDS} rounds; run tsuzuri fix again to apply the remaining {}",
            plural(remaining, "fix", "fixes")
        );
    }
    text
}

fn record_json(record: &FixRecord) -> String {
    let mut json = String::from("{\"event\":\"fix\",");
    match record.status {
        Status::Applied { round } => {
            let _ = write!(json, "\"status\":\"applied\",\"round\":{round},");
        }
        Status::Skipped(skip) => {
            let _ = write!(
                json,
                "\"status\":\"skipped\",\"reason\":{},",
                json_string(skip.name())
            );
        }
    }
    let _ = write!(json, "\"code\":{},", json_string(record.code));
    if let Some(kind) = record.kind {
        let _ = write!(json, "\"kind\":{},", json_string(kind));
    }
    let edits: Vec<String> = record
        .edits
        .iter()
        .map(|edit| {
            format!(
                "{{\"path\":{},\"span\":{{\"start\":{},\"end\":{},\"line\":{},\"column\":{},\"end_line\":{},\"end_column\":{}}},\"text\":{}}}",
                json_string(&edit.path),
                edit.start,
                edit.end,
                edit.line,
                edit.column,
                edit.end_line,
                edit.end_column,
                json_string(&edit.text)
            )
        })
        .collect();
    let _ = write!(
        json,
        "\"applicability\":{},\"title\":{},\"edits\":[{}]}}",
        json_string(record.applicability.as_str()),
        json_string(&record.title),
        edits.join(",")
    );
    json
}

/// The objects of `tsuzuri fix --json`, one per line: the fixes, the files, and a summary.
pub(crate) fn render_json(
    report: &Report,
    project: &Project,
    dry_run: bool,
    threshold: Threshold,
) -> String {
    let mut json = String::new();
    for record in &report.records {
        json.push_str(&record_json(record));
        json.push('\n');
    }
    for change in &report.files {
        let path = display_path(project, change.source);
        let _ = write!(
            json,
            "{{\"event\":\"file\",\"path\":{},\"fixes\":{}",
            json_string(&path),
            change.fixes
        );
        if dry_run {
            let diff = crate::diff::unified(&path, &change.original, &change.fixed, 3);
            let _ = write!(
                json,
                ",\"diff\":{}",
                diff.map_or_else(|| "null".to_owned(), |diff| json_string(&diff))
            );
        }
        json.push_str("}\n");
    }
    let applied = report
        .records
        .iter()
        .filter(|record| matches!(record.status, Status::Applied { .. }))
        .count();
    let _ = writeln!(
        json,
        "{{\"event\":\"summary\",\"dry_run\":{dry_run},\"applicability\":{},\"rounds\":{},\"stop\":{},\"applied\":{applied},\"skipped\":{},\"files\":{},\"remaining_errors\":{},\"remaining_warnings\":{}}}",
        json_string(threshold.name()),
        report.rounds,
        json_string(match report.stop {
            Stop::FixedPoint => "fixed_point",
            Stop::RoundLimit => "round_limit",
        }),
        report.records.len() - applied,
        report.files.len(),
        report.last.error_total(),
        report.last.warnings.len()
    );
    json
}

/// What `tsuzuri fix` prints and how it ends.
pub struct Execution {
    pub stdout: String,
    pub exit: u8,
    /// The diagnostics of the fixed project, which the command prints like `check` (none for
    /// `--dry-run`).
    pub remaining: Option<Analysis>,
}

/// The `E2008` of a failed replacement.
pub fn replace_error(error: ReplaceError) -> SourceError {
    let undone = if error.unrestored.is_empty() {
        String::new()
    } else {
        let paths: Vec<String> = error
            .unrestored
            .iter()
            .map(|path| path.display().to_string())
            .collect();
        format!("; could not restore {}", paths.join(", "))
    };
    match error.failure {
        ReplaceFailure::Changed { path } => SourceError {
            diagnostic: Diagnostic::of(
                DiagnosticKind::FixSourceChanged,
                format!(
                    "'{}' changed while fixing; nothing was left replaced, rerun tsuzuri fix{undone}",
                    path.display()
                ),
                Span::default(),
            ),
            path,
        },
        ReplaceFailure::Failed { path, detail } => SourceError {
            diagnostic: Diagnostic::of(
                DiagnosticKind::FixWriteFailed,
                format!(
                    "cannot replace '{}': {detail}; {} restored{undone}",
                    path.display(),
                    plural(
                        error.restored,
                        "earlier replacement was",
                        "earlier replacements were"
                    ),
                ),
                Span::default(),
            ),
            path,
        },
    }
}

/// Applies the fixes of `project` per `arguments` (without `--edition`), with `analyze` as the
/// analysis, and writes the changed files unless `--dry-run`.
pub fn execute_with(
    project: &mut Project,
    arguments: &Arguments,
    analyze: &mut dyn FnMut(&Project, bool) -> Analysis,
) -> Result<Execution, SourceError> {
    let scope = if arguments.input.is_dir() {
        Scope::Project
    } else {
        Scope::File(project.root)
    };
    let options = Options {
        threshold: arguments.threshold,
        scope,
        only_codes: None,
    };
    let report = run(project, &options, &mut |project| analyze(project, false))?;
    if !arguments.dry_run {
        let updates: Vec<FileUpdate> = report
            .files
            .iter()
            .map(|change| FileUpdate {
                path: project.sources[change.source].path.clone(),
                original: change.original.clone(),
                replacement: change.fixed.clone(),
            })
            .collect();
        replace_files(&updates).map_err(replace_error)?;
    }
    let render = if arguments.json {
        render_json
    } else {
        render_text
    };
    Ok(Execution {
        stdout: render(&report, project, arguments.dry_run, arguments.threshold),
        exit: 0,
        remaining: (!arguments.dry_run).then_some(report.last),
    })
}

/// `execute_with` with `analyze_project`.
pub fn execute(project: &mut Project, arguments: &Arguments) -> Result<Execution, SourceError> {
    execute_with(project, arguments, &mut analyze_project)
}

/// The edition move's first step: the canonical-form fixes (`W1007`, A22), applied in memory to
/// the sources in `scope`. The project's texts are the fixed ones afterwards.
pub fn canonical_round(
    project: &mut Project,
    scope: Scope,
    analyze: &mut dyn FnMut(&Project, bool) -> Analysis,
) -> Result<Report, SourceError> {
    run(
        project,
        &Options {
            threshold: Threshold::Certain,
            scope,
            only_codes: Some(&["W1007"]),
        },
        &mut |project| analyze(project, true),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::Edit;
    use crate::driver::SourceFile;

    fn project(text: &str) -> Project {
        Project::single_main(PathBuf::from("Main.tz"), text.to_owned())
    }

    /// A project with a second user source `B.tz` at index 1.
    fn two_files(a: &str, b: &str) -> Project {
        let mut project = project(a);
        let main = &project.sources[0];
        let second = SourceFile {
            path: PathBuf::from("B.tz"),
            relative_path: PathBuf::from("B.tz"),
            name: "B".to_owned(),
            text: b.to_owned(),
            origin: main.origin,
            package: main.package.clone(),
            namespace: main.namespace.clone(),
            edition: main.edition,
        };
        project.sources.insert(1, second);
        project
    }

    fn edit(source: usize, start: usize, end: usize, text: &str) -> Edit {
        Edit {
            span: Span::new(start, end).in_source(source),
            text: text.to_owned(),
        }
    }

    fn fixed(
        kind: DiagnosticKind,
        source: usize,
        start: usize,
        end: usize,
        text: &str,
        applicability: Applicability,
    ) -> Diagnostic {
        let mut diagnostic = Diagnostic::of(kind, "m", Span::new(start, end).in_source(source));
        diagnostic.fixes.push(Fix {
            title: format!("replace with '{text}'"),
            applicability,
            edits: vec![edit(source, start, end, text)],
        });
        diagnostic
    }

    fn analysis(errors: Vec<Diagnostic>, omitted: usize, warnings: Vec<Diagnostic>) -> Analysis {
        let mut errors = DiagnosticSet::from_diagnostics(errors, 0);
        errors.omitted = omitted;
        Analysis { errors, warnings }
    }

    /// A `W1001` warning for each `x` of every user source, fixed by replacing it with `y`.
    fn x_warnings(project: &Project) -> Analysis {
        let mut warnings = Vec::new();
        for (source, file) in project.sources.iter().enumerate() {
            if file.origin == ModuleOrigin::User {
                for (at, _) in file.text.match_indices('x') {
                    warnings.push(fixed(
                        DiagnosticKind::UnusedLocal,
                        source,
                        at,
                        at + 1,
                        "y",
                        Applicability::Certain,
                    ));
                }
            }
        }
        analysis(Vec::new(), 0, warnings)
    }

    fn options(scope: Scope) -> Options {
        Options {
            threshold: Threshold::Certain,
            scope,
            only_codes: None,
        }
    }

    fn applied(report: &Report) -> usize {
        report
            .records
            .iter()
            .filter(|record| matches!(record.status, Status::Applied { .. }))
            .count()
    }

    #[test]
    fn threshold_admits_by_applicability() {
        use Applicability::{Certain, Likely, Maybe};
        let admitted = |threshold: Threshold| {
            [Certain, Likely, Maybe]
                .into_iter()
                .filter(|applicability| threshold.admits(*applicability))
                .collect::<Vec<_>>()
        };
        assert_eq!(admitted(Threshold::Certain), [Certain]);
        assert_eq!(admitted(Threshold::Likely), [Certain, Likely]);
        assert_eq!(admitted(Threshold::Maybe), [Certain, Likely, Maybe]);
    }

    #[test]
    fn rejects_edits_outside_the_writable_files() {
        let project = two_files("a é b", "bbbb");
        let std = project
            .sources
            .iter()
            .position(|file| file.origin == ModuleOrigin::Std)
            .unwrap();
        let fix = |edits: Vec<Edit>| Fix {
            title: "t".to_owned(),
            applicability: Applicability::Certain,
            edits,
        };
        let check = |edits: Vec<Edit>, scope: Scope| validate(&project, &fix(edits), scope);
        let all = Scope::Project;
        assert_eq!(
            check(vec![edit(9999, 0, 0, "")], all),
            Err(Skip::OutsideProject)
        );
        assert_eq!(
            check(vec![edit(std, 0, 0, "")], all),
            Err(Skip::OutsideProject)
        );
        assert_eq!(
            check(vec![edit(1, 0, 1, "c")], Scope::File(0)),
            Err(Skip::OutsideInput)
        );
        assert_eq!(check(vec![edit(0, 2, 1, "")], all), Err(Skip::InvalidEdit));
        assert_eq!(check(vec![edit(0, 0, 99, "")], all), Err(Skip::InvalidEdit));
        assert_eq!(check(vec![edit(0, 3, 4, "")], all), Err(Skip::InvalidEdit));
        assert_eq!(
            check(vec![edit(0, 0, 0, &"z".repeat(MAX_EDIT_TEXT + 1))], all),
            Err(Skip::InvalidEdit)
        );
        assert_eq!(
            check(vec![edit(0, 0, 3, ""), edit(0, 2, 4, "")], all),
            Err(Skip::InvalidEdit)
        );
        assert_eq!(check(Vec::new(), all), Err(Skip::InvalidEdit));
        // Touching edits and inserts at one position do not overlap.
        assert!(check(vec![edit(0, 0, 1, ""), edit(0, 1, 2, "")], all).is_ok());
        assert!(check(vec![edit(0, 1, 1, "p"), edit(0, 1, 1, "q")], all).is_ok());
    }

    #[test]
    fn conflict_is_closed_interval_intersection() {
        let meets = |taken: (usize, usize), edit: (usize, usize)| {
            let accepted = BTreeMap::from([(0, BTreeMap::from([taken]))]);
            conflicts(
                &accepted,
                &ResolvedEdit {
                    source: 0,
                    start: edit.0,
                    end: edit.1,
                    text: String::new(),
                },
            )
        };
        assert!(meets((5, 10), (10, 12)));
        assert!(!meets((5, 10), (11, 12)));
        assert!(meets((5, 5), (5, 5)));
        assert!(meets((5, 10), (7, 7)));
        assert!(!meets((5, 10), (3, 4)));
        assert!(meets((5, 10), (3, 5)));
        assert!(!meets((0, 0), (1, 1)));
    }

    #[test]
    fn resolve_keeps_the_first_in_sorted_order_and_defers_the_rest() {
        let project = project(&"a".repeat(30));
        let mut analysis = analysis(Vec::new(), 0, Vec::new());
        for (start, end) in [(8, 12), (5, 10), (20, 21)] {
            analysis.warnings.push(fixed(
                DiagnosticKind::UnusedLocal,
                0,
                start,
                end,
                "z",
                Applicability::Certain,
            ));
        }
        let (chosen, skipped) = collect(&project, &analysis, &options(Scope::Project)).unwrap();
        let spans: Vec<_> = chosen
            .iter()
            .map(|candidate| (candidate.edits[0].start, candidate.edits[0].end))
            .collect();
        assert_eq!(spans, [(5, 10), (20, 21)]);
        assert!(skipped.is_empty());
    }

    #[test]
    fn a_fix_with_an_insert_at_its_start_keeps_its_whole_span() {
        // One fix deletes 0..6 and inserts at 0; another edits 3..4 and must wait.
        let project = project("abcdefgh");
        let mut first = Diagnostic::of(
            DiagnosticKind::UnusedLocal,
            "m",
            Span::new(0, 6).in_source(0),
        );
        first.fixes.push(Fix {
            title: "first".to_owned(),
            applicability: Applicability::Certain,
            edits: vec![edit(0, 0, 6, ""), edit(0, 0, 0, "X")],
        });
        let analysis = analysis(
            Vec::new(),
            0,
            vec![
                first,
                fixed(
                    DiagnosticKind::UnusedLocal,
                    0,
                    3,
                    4,
                    "Y",
                    Applicability::Certain,
                ),
            ],
        );
        let (chosen, _) = collect(&project, &analysis, &options(Scope::Project)).unwrap();
        assert_eq!(chosen.len(), 1);
        assert_eq!(chosen[0].record.title, "first");
    }

    #[test]
    fn overlapping_deletions_are_an_invalid_edit() {
        let project = project("abcdef");
        let fix = Fix {
            title: "t".to_owned(),
            applicability: Applicability::Certain,
            edits: vec![edit(0, 0, 6, ""), edit(0, 0, 6, "")],
        };
        assert_eq!(
            validate(&project, &fix, Scope::Project),
            Err(Skip::InvalidEdit)
        );
    }

    #[test]
    fn fixes_that_fit_alone_but_not_together_are_split() {
        let project = project(&"a".repeat(MAX_SOURCE_BYTES - 1));
        let analysis = analysis(
            Vec::new(),
            0,
            vec![
                fixed(
                    DiagnosticKind::UnusedLocal,
                    0,
                    0,
                    0,
                    "b",
                    Applicability::Certain,
                ),
                fixed(
                    DiagnosticKind::UnusedLocal,
                    0,
                    9,
                    9,
                    "c",
                    Applicability::Certain,
                ),
            ],
        );
        let (chosen, skipped) = collect(&project, &analysis, &options(Scope::Project)).unwrap();
        assert_eq!(chosen.len(), 1);
        assert_eq!(chosen[0].edits[0].text, "b");
        assert_eq!(skipped.len(), 1);
        assert_eq!(skipped[0].status, Status::Skipped(Skip::InvalidEdit));
    }

    #[test]
    fn identical_fixes_collapse() {
        let mut project = project("x");
        let mut twice = |project: &Project| {
            let mut analysis = x_warnings(project);
            analysis.warnings.extend(analysis.warnings.clone());
            analysis
        };
        let (chosen, _) = collect(&project, &twice(&project), &options(Scope::Project)).unwrap();
        assert_eq!(chosen.len(), 1);
        let report = run(&mut project, &options(Scope::Project), &mut twice).unwrap();
        assert_eq!(applied(&report), 1);
        assert_eq!(project.sources[0].text, "y");
    }

    #[test]
    fn apply_edits_builds_the_text_in_one_pass() {
        let edits =
            [(0, 3, "X"), (4, 7, "Y"), (11, 11, "!")].map(|(start, end, text)| ResolvedEdit {
                source: 0,
                start,
                end,
                text: text.to_owned(),
            });
        assert_eq!(
            splice("abc def ghi", &edits.iter().collect::<Vec<_>>()),
            "X Y ghi!"
        );
    }

    #[test]
    fn candidate_limit_is_e1017() {
        let mut project = project(&"x ".repeat(MAX_CANDIDATES + 1));
        let error = run(&mut project, &options(Scope::Project), &mut x_warnings).unwrap_err();
        assert_eq!(error.diagnostic.code, "E1017");
        assert_eq!(error.diagnostic.kind, Some("fix_candidate_limit"));
    }

    #[test]
    fn rounds_stop_at_the_fixed_point() {
        let mut project = project("x x");
        let report = run(&mut project, &options(Scope::Project), &mut x_warnings).unwrap();
        assert_eq!((report.rounds, report.stop), (1, Stop::FixedPoint));
        assert_eq!(report.files.len(), 1);
        assert_eq!(report.files[0].fixed, "y y");
        assert_eq!(report.files[0].fixes, 2);
        assert_eq!(applied(&report), 2);
        assert!(report.last.warnings.is_empty());
    }

    #[test]
    fn touching_fixes_apply_in_two_rounds() {
        let mut project = project("xx");
        let report = run(&mut project, &options(Scope::Project), &mut x_warnings).unwrap();
        assert_eq!(report.rounds, 2);
        assert_eq!(report.files[0].fixed, "yy");
        let rounds: Vec<_> = report.records.iter().map(|record| record.status).collect();
        assert_eq!(
            rounds,
            [Status::Applied { round: 1 }, Status::Applied { round: 2 }]
        );
    }

    #[test]
    fn a_round_that_adds_an_error_aborts_without_changes() {
        let mut project = project("x");
        let mut analyze = |project: &Project| {
            if project.sources[0].text.contains('y') {
                analysis(
                    vec![Diagnostic::of(
                        DiagnosticKind::TypeMismatch,
                        "expected i64, found bool",
                        Span::new(0, 1).in_source(0),
                    )],
                    0,
                    Vec::new(),
                )
            } else {
                x_warnings(project)
            }
        };
        let error = run(&mut project, &options(Scope::Project), &mut analyze).unwrap_err();
        assert_eq!(error.diagnostic.code, "E2008");
        assert_eq!(error.diagnostic.kind, Some("fix_verification_failed"));
        assert_eq!(
            error.diagnostic.message,
            "the fixes would introduce E1003 in 'Main.tz' at line 1: expected i64, found bool; no file was changed, review the fixes with --dry-run --json"
        );
        assert_eq!(project.sources[0].text, "x");
    }

    /// `e` is an error fixed to `w`, `w` a warning fixed to `v`, and `x` a warning fixed to `q`,
    /// which is a warning of another code without a fix.
    fn staged(project: &Project) -> Analysis {
        let text = project.sources[0].text.as_str();
        let at = |kind, to: &str| fixed(kind, 0, 0, 1, to, Applicability::Certain);
        match text {
            "e" => analysis(vec![at(DiagnosticKind::UnknownValue, "w")], 0, Vec::new()),
            "w" => analysis(
                Vec::new(),
                0,
                vec![at(DiagnosticKind::UnusedPrivateFunction, "v")],
            ),
            "x" => analysis(Vec::new(), 0, vec![at(DiagnosticKind::UnusedLocal, "q")]),
            "q" => analysis(
                Vec::new(),
                0,
                vec![Diagnostic::of(
                    DiagnosticKind::UnusedPrivateFunction,
                    "m",
                    Span::new(0, 1).in_source(0),
                )],
            ),
            _ => analysis(Vec::new(), 0, Vec::new()),
        }
    }

    #[test]
    fn a_round_that_adds_a_warning_aborts_only_when_errors_were_absent() {
        let mut clean = project("x");
        let error = run(&mut clean, &options(Scope::Project), &mut staged).unwrap_err();
        assert_eq!(error.diagnostic.kind, Some("fix_verification_failed"));
        assert_eq!(clean.sources[0].text, "x");
        let mut broken = project("e");
        assert!(run(&mut broken, &options(Scope::Project), &mut staged).is_ok());
    }

    #[test]
    fn error_fixes_may_reveal_hidden_warnings() {
        let mut project = project("e");
        let report = run(&mut project, &options(Scope::Project), &mut staged).unwrap();
        assert_eq!(report.rounds, 2);
        assert_eq!(project.sources[0].text, "v");
        assert_eq!(applied(&report), 2);
    }

    #[test]
    fn round_limit_stops_at_five_and_reports_the_rest() {
        let mut project = project("x");
        let mut grow = |project: &Project| {
            let length = project.sources[0].text.len();
            analysis(
                Vec::new(),
                0,
                vec![fixed(
                    DiagnosticKind::UnusedLocal,
                    0,
                    length,
                    length,
                    "x",
                    Applicability::Certain,
                )],
            )
        };
        let report = run(&mut project, &options(Scope::Project), &mut grow).unwrap();
        assert_eq!((report.rounds, report.stop), (MAX_ROUNDS, Stop::RoundLimit));
        assert_eq!(project.sources[0].text, "x".repeat(MAX_ROUNDS + 1));
        assert_eq!(
            report.records.last().unwrap().status,
            Status::Skipped(Skip::RoundLimit)
        );
        let text = render_text(&report, &project, false, Threshold::Certain);
        assert!(text.ends_with(
            "note: stopped after 5 rounds; run tsuzuri fix again to apply the remaining 1 fix\n"
        ));
    }

    #[test]
    fn omitted_errors_skip_the_pair_check() {
        let mut project = project("e");
        let mut analyze = |project: &Project| match project.sources[0].text.as_str() {
            "e" => analysis(
                vec![fixed(
                    DiagnosticKind::UnknownValue,
                    0,
                    0,
                    1,
                    "f",
                    Applicability::Certain,
                )],
                3,
                Vec::new(),
            ),
            _ => analysis(
                vec![Diagnostic::of(
                    DiagnosticKind::TypeMismatch,
                    "m",
                    Span::new(0, 1).in_source(0),
                )],
                2,
                Vec::new(),
            ),
        };
        assert!(run(&mut project, &options(Scope::Project), &mut analyze).is_ok());
    }

    #[test]
    fn file_scope_skips_other_files() {
        let mut project = two_files("x", "x");
        let report = run(&mut project, &options(Scope::File(0)), &mut x_warnings).unwrap();
        assert_eq!(project.sources[0].text, "y");
        assert_eq!(project.sources[1].text, "x");
        let statuses: Vec<_> = report.records.iter().map(|record| record.status).collect();
        assert_eq!(
            statuses,
            [
                Status::Applied { round: 1 },
                Status::Skipped(Skip::OutsideInput)
            ]
        );
    }

    #[test]
    fn the_canonical_round_applies_only_canonical_fixes() {
        let mut project = project("x z");
        let mut asked = Vec::new();
        let mut analyze = |project: &Project, canonical: bool| {
            asked.push(canonical);
            let text = &project.sources[0].text;
            let mut warnings = Vec::new();
            for (at, character) in text.char_indices() {
                let (kind, to) = match character {
                    'x' => (DiagnosticKind::UnusedLocal, "y"),
                    'z' => (DiagnosticKind::UnusedLocal, "w"),
                    _ => continue,
                };
                let mut warning = fixed(kind, 0, at, at + 1, to, Applicability::Certain);
                if character == 'x' {
                    // A22's canonical-form warning.
                    warning.code = "W1007";
                }
                warnings.push(warning);
            }
            analysis(Vec::new(), 0, warnings)
        };
        let report = canonical_round(&mut project, Scope::Project, &mut analyze).unwrap();
        assert_eq!(project.sources[0].text, "y z");
        assert_eq!(applied(&report), 1);
        assert!(asked.iter().all(|canonical| *canonical));
    }

    #[test]
    fn there_are_no_canonical_warnings_yet() {
        let project = project("let unused = 1\n42\n");
        assert_eq!(
            analyze_project(&project, true).warnings,
            analyze_project(&project, false).warnings
        );
    }

    #[test]
    fn parse_arguments_table() {
        let parse =
            |words: &[&str]| parse_arguments(&words.iter().map(OsString::from).collect::<Vec<_>>());
        let ok = parse(&["a"]).unwrap();
        assert_eq!(
            (ok.input, ok.threshold, ok.dry_run, ok.json, ok.edition),
            (PathBuf::from("a"), Threshold::Certain, false, false, None)
        );
        let ok = parse(&["a", "--dry-run", "--applicability", "likely", "--json"]).unwrap();
        assert_eq!(
            (ok.threshold, ok.dry_run, ok.json),
            (Threshold::Likely, true, true)
        );
        assert_eq!(
            parse(&["a", "--applicability", "maybe", "--dry-run"])
                .unwrap()
                .threshold,
            Threshold::Maybe
        );
        assert_eq!(
            parse(&["--", "--json"]).unwrap().input,
            PathBuf::from("--json")
        );
        let ok = parse(&["a", "--edition", "2027", "--from", "2026"]).unwrap();
        assert_eq!(
            (ok.edition, ok.from),
            (Edition::parse("2027"), Edition::parse("2026"))
        );
        let failure = |words: &[&str]| {
            let error = parse(words).unwrap_err();
            assert_eq!(error.code, "E2000", "{words:?}");
            (error.kind.unwrap(), error.message.clone())
        };
        for words in [
            &[][..],
            &["a", "b"],
            &["a", "--x"],
            &["a", "--applicability"],
            &["a", "--applicability", "sure"],
            &["a", "--edition"],
            &["a", "--from", "2026"],
        ] {
            assert_eq!(failure(words).0, "fix_bad_arguments", "{words:?}");
        }
        assert_eq!(
            failure(&["a", "--dry-run", "--dry-run"]),
            (
                "fix_bad_arguments",
                "--dry-run specified more than once".to_owned()
            )
        );
        assert_eq!(
            failure(&["a", "--applicability", "maybe"]).0,
            "fix_maybe_needs_dry_run"
        );
        let (kind, message) = failure(&["a", "--edition", "2030"]);
        assert_eq!(kind, "fix_edition_unsupported");
        assert!(message.contains("unsupported edition '2030'"), "{message}");
        assert_eq!(
            failure(&["a", "--edition", "2026", "--applicability", "likely"]).0,
            "fix_edition_with_applicability"
        );
    }

    #[test]
    fn render_text_and_json_are_stable() {
        let project = project("");
        let recorded = |start, end, line, column, text: &str| RecordedEdit {
            path: "Main.tz".to_owned(),
            start,
            end,
            line,
            column,
            end_line: line,
            end_column: column + (end - start),
            text: text.to_owned(),
        };
        let record = |status, code, applicability, title: &str, edit| FixRecord {
            status,
            code,
            kind: None,
            applicability,
            title: title.to_owned(),
            edits: vec![edit],
        };
        let report = Report {
            rounds: 1,
            stop: Stop::FixedPoint,
            records: vec![
                record(
                    Status::Applied { round: 1 },
                    "W1001",
                    Applicability::Certain,
                    "Prefix 'b' with '_'",
                    recorded(35, 36, 1, 36, "_b"),
                ),
                record(
                    Status::Applied { round: 1 },
                    "W1001",
                    Applicability::Certain,
                    "Prefix 'unused' with '_'",
                    recorded(47, 53, 3, 5, "_unused"),
                ),
                record(
                    Status::Skipped(Skip::Applicability),
                    "W1005",
                    Applicability::Likely,
                    "Replace 'pair' with 'pair2'",
                    recorded(72, 76, 5, 1, "pair2"),
                ),
            ],
            files: vec![FileChange {
                source: 0,
                original: String::new(),
                fixed: String::new(),
                fixes: 2,
            }],
            last: analysis(Vec::new(), 0, Vec::new()),
        };
        assert_eq!(
            render_text(&report, &project, false, Threshold::Certain),
            "fixed Main.tz: 2 fixes\napplied 2 fixes in 1 file\nskipped 1 fix (use --json for the list)\nnote: rerun with --applicability likely to apply 1 more fix\n"
        );
        assert_eq!(
            render_json(&report, &project, false, Threshold::Certain),
            concat!(
                r#"{"event":"fix","status":"applied","round":1,"code":"W1001","applicability":"certain","title":"Prefix 'b' with '_'","edits":[{"path":"Main.tz","span":{"start":35,"end":36,"line":1,"column":36,"end_line":1,"end_column":37},"text":"_b"}]}"#,
                "\n",
                r#"{"event":"fix","status":"applied","round":1,"code":"W1001","applicability":"certain","title":"Prefix 'unused' with '_'","edits":[{"path":"Main.tz","span":{"start":47,"end":53,"line":3,"column":5,"end_line":3,"end_column":11},"text":"_unused"}]}"#,
                "\n",
                r#"{"event":"fix","status":"skipped","reason":"applicability","code":"W1005","applicability":"likely","title":"Replace 'pair' with 'pair2'","edits":[{"path":"Main.tz","span":{"start":72,"end":76,"line":5,"column":1,"end_line":5,"end_column":5},"text":"pair2"}]}"#,
                "\n",
                r#"{"event":"file","path":"Main.tz","fixes":2}"#,
                "\n",
                r#"{"event":"summary","dry_run":false,"applicability":"certain","rounds":1,"stop":"fixed_point","applied":2,"skipped":1,"files":1,"remaining_errors":0,"remaining_warnings":0}"#,
                "\n"
            )
        );
    }
}
