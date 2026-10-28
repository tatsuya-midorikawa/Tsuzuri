//! Writing the knowledge graph of an analysis (G27 §4–§6, D-60): `plan` decides before the
//! analysis whether anything is written (a fresh graph is left alone, and the analysis then runs
//! without a capture); `finish` builds the files from the analysis and publishes them. A failed
//! analysis writes a minimal graph: the marker, `manifest.json` with `complete: false`, and
//! `diag.jsonl`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::KgModel;
use super::input::{digest, hex, source_hash, stamp};
use super::manifest::{Freshness, Manifest, ModuleEntry, SCHEMA, compiler, inspect};
use super::settings::KgSettings;
use super::store::KgData;
use super::write::{Files, MANIFEST, not_ours, publish};
use crate::check::CheckedModule;
use crate::check::kg_capture::Capture;
use crate::diagnostic::{Diagnostic, DiagnosticKind, DiagnosticSet, Span, json_string};
use crate::driver::Project;

/// Where a graph is written and what it is written for.
#[derive(Clone, Debug)]
pub struct Target {
    pub dir: PathBuf,
    pub hash: String,
    pub stamp: String,
}

#[derive(Clone, Debug)]
pub enum Plan {
    /// The command does not write a graph.
    Off,
    /// The graph is up to date (complete or for sources with errors): nothing is written.
    Fresh,
    Build(Target),
}

/// The analysis that `finish` writes the graph of.
pub(crate) enum Analysis<'a> {
    Checked {
        module: &'a CheckedModule,
        capture: Box<Capture>,
        warnings: &'a DiagnosticSet,
    },
    Failed {
        errors: &'a DiagnosticSet,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Written { files: usize },
    Incomplete { files: usize },
}

/// The one-line notice of a graph that could not be written: `kg not updated: ...`.
pub fn notice(error: &Diagnostic) -> String {
    format!("kg not updated: {}", error.message)
}

fn ancestor_of(dir: &Path, path: &Path) -> bool {
    match (std::fs::canonicalize(dir), std::fs::canonicalize(path)) {
        (Ok(dir), Ok(path)) => path.starts_with(dir),
        _ => false,
    }
}

/// Refuses a directory that is not a graph the compiler wrote, or that holds the project's sources.
pub fn protect(dir: &Path, project: &Project) -> Result<(), Diagnostic> {
    if std::fs::symlink_metadata(dir).is_ok() && !super::write::has_marker(dir) {
        return Err(not_ours(dir));
    }
    if project
        .sources
        .iter()
        .chain(&project.manifests)
        .filter(|source| source.origin == crate::check::ModuleOrigin::User)
        .any(|source| ancestor_of(dir, &source.path))
    {
        return Err(not_ours(dir));
    }
    Ok(())
}

/// The stamp of the running compiler, or the error of a compiler that cannot be identified.
pub fn current_stamp() -> Result<String, Diagnostic> {
    stamp().map(|stamp| hex(&stamp)).ok_or_else(|| {
        Diagnostic::of(
            DiagnosticKind::IoFailure,
            "cannot identify the running compiler",
            Span::default(),
        )
    })
}

/// Decides, before the analysis, whether the command writes the graph of `project`. `force`
/// rewrites a fresh graph.
pub fn plan(settings: &KgSettings, project: &Project, force: bool) -> Result<Plan, Diagnostic> {
    if !settings.wants(project) {
        return Ok(Plan::Off);
    }
    target_for(&settings.directory(project), project, force)
}

pub fn target_for(dir: &Path, project: &Project, force: bool) -> Result<Plan, Diagnostic> {
    protect(dir, project)?;
    let hash = hex(&source_hash(project));
    let stamp = current_stamp()?;
    if !force
        && matches!(
            inspect(dir, &hash, &stamp),
            Freshness::Fresh | Freshness::Incomplete(_)
        )
    {
        return Ok(Plan::Fresh);
    }
    Ok(Plan::Build(Target {
        dir: dir.to_owned(),
        hash,
        stamp,
    }))
}

/// The relative path of a source with `/` separators.
pub fn relative(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// `diag.jsonl`: each diagnostic as `check --json` writes it, with paths relative to the root, and
/// the note of the omitted ones.
pub fn diagnostics_jsonl(set: &DiagnosticSet, project: &Project) -> Vec<u8> {
    let files: Vec<(String, &str)> = project
        .sources
        .iter()
        .chain(&project.manifests)
        .map(|file| (relative(&file.relative_path), file.text.as_str()))
        .collect();
    let lookup = |index: usize| files.get(index).map(|(path, text)| (path.as_str(), *text));
    let mut out = String::new();
    for diagnostic in &set.diagnostics {
        let source = project.source_for(diagnostic);
        out.push_str(&diagnostic.json_with(
            diagnostic.severity.as_str(),
            &relative(&source.relative_path),
            &source.text,
            &lookup,
        ));
        out.push('\n');
    }
    if let Some(note) = set.omission_note() {
        out.push_str(&format!(
            "{{\"severity\":\"note\",\"message\":{}}}\n",
            json_string(&note)
        ));
    }
    out.into_bytes()
}

/// The name, version and edition of the project for the manifest.
pub fn project_identity(project: &Project) -> (String, Option<String>, String) {
    let edition = project.sources.get(project.root).map_or_else(
        || crate::syntax::Edition::LATEST.to_string(),
        |source| source.edition.to_string(),
    );
    // Every package's manifest is a `Tsuzuri.toml`: the root package's is the project's.
    let manifest = project.root_package.as_ref().and_then(|root| {
        project
            .manifests
            .iter()
            .find(|manifest| {
                manifest.relative_path == Path::new("Tsuzuri.toml")
                    && manifest.package.as_ref() == Some(root)
            })
            .and_then(|manifest| crate::package::parse_manifest(&manifest.text, 0).ok())
    });
    let name = match &manifest {
        Some(manifest) => manifest.name.clone(),
        None => project
            .project_directory()
            .and_then(|root| std::fs::canonicalize(root).ok())
            .and_then(|root| {
                root.file_name()
                    .map(|name| name.to_string_lossy().into_owned())
            })
            .unwrap_or_else(|| "project".to_owned()),
    };
    (name, manifest.map(|manifest| manifest.version), edition)
}

fn count_lines(bytes: Option<&[u8]>) -> u64 {
    bytes.map_or(0, |bytes| {
        bytes.iter().filter(|byte| **byte == b'\n').count() as u64
    })
}

fn count_matching(bytes: Option<&[u8]>, needle: &str) -> u64 {
    bytes.map_or(0, |bytes| {
        String::from_utf8_lossy(bytes)
            .lines()
            .filter(|line| line.contains(needle))
            .count() as u64
    })
}

/// The severities of `diag.jsonl`: (errors, warnings).
fn severities(set: &DiagnosticSet) -> (u64, u64) {
    let errors = set
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.severity.as_str() == "error")
        .count() as u64;
    let warnings = set
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.severity.as_str() == "warning")
        .count() as u64;
    (errors, warnings)
}

/// The files and the counts of the graph of a checked analysis, without the manifest: the model's
/// files, `diag.jsonl`, the brief cards and the views.
pub fn checked_files(
    project: &Project,
    model: &KgModel,
    warnings: &DiagnosticSet,
) -> Result<(Files, Vec<(String, u64)>), Diagnostic> {
    let rendering = |error: std::io::Error| {
        Diagnostic::of(
            DiagnosticKind::IoFailure,
            format!("cannot render the KG: {error}"),
            Span::default(),
        )
    };
    let write_time = crate::timings::Phase::start("kg-write");
    let mut files = super::emit::emit_rows(model).map_err(rendering)?;
    files.put("diag.jsonl", diagnostics_jsonl(warnings, project));
    drop(write_time);
    let facts_time = crate::timings::Phase::start("kg-facts");
    files
        .0
        .extend(super::emit::emit_facts(model).map_err(rendering)?.0);
    drop(facts_time);
    let (errors, warning_count) = severities(warnings);
    let nodes = files.get("nodes.jsonl");
    let public_nodes = nodes.map_or(0, |bytes| {
        String::from_utf8_lossy(bytes)
            .lines()
            .filter(|line| {
                line.contains("\"public\":true") && !line.contains("\"kind\":\"module\"")
            })
            .count() as u64
    });
    let mut counts = vec![
        ("modules".to_owned(), model.modules().len() as u64),
        ("nodes".to_owned(), count_lines(nodes)),
        ("public_nodes".to_owned(), public_nodes),
        ("edges".to_owned(), count_lines(files.get("edges.jsonl"))),
        (
            "tests".to_owned(),
            count_matching(files.get("tests.jsonl"), "\"kind\":\"test\""),
        ),
        (
            "benches".to_owned(),
            count_matching(files.get("tests.jsonl"), "\"kind\":\"bench\""),
        ),
        ("errors".to_owned(), errors),
        ("warnings".to_owned(), warning_count),
    ];
    let brief_time = crate::timings::Phase::start("kg-brief");
    let data = interim_data(project, model, &files, warning_count)?;
    let modules: Vec<(String, String)> = model
        .modules()
        .iter()
        .map(|module| (module.id.clone(), module.key.clone()))
        .collect();
    let (l2_cards, l2_omitted) = super::brief::add_cards(&data, &modules, &mut files);
    drop(brief_time);
    // The diagrams that need no ID (G30); one that cannot be drawn is left out.
    let views_time = crate::timings::Phase::start("kg-views");
    if let Ok(input) = crate::diagram::Input::from_kg(&data) {
        for (path, text) in crate::diagram::views(&input) {
            files.put(path, text.into_bytes());
        }
    }
    drop(views_time);
    counts.push(("l2_cards".to_owned(), l2_cards));
    counts.push(("l2_omitted".to_owned(), l2_omitted));
    Ok((files, counts))
}

/// The graph of `files` as a reader will see it, before the manifest exists: what the cards and
/// the views read.
fn interim_data(
    project: &Project,
    model: &KgModel,
    files: &Files,
    warnings: u64,
) -> Result<KgData, Diagnostic> {
    let mut manifest = manifest_for(
        project,
        "",
        Some(model),
        &Files::default(),
        vec![("warnings".to_owned(), warnings)],
        true,
    );
    manifest.files = files
        .0
        .keys()
        .map(|path| (path.clone(), String::new()))
        .collect();
    KgData::from_parts(&manifest, files).map_err(|reason| {
        Diagnostic::of(
            DiagnosticKind::IoFailure,
            format!("cannot read the KG that was built: {reason}"),
            Span::default(),
        )
    })
}

/// The manifest of `files` for `project`.
pub fn manifest_for(
    project: &Project,
    target_hash: &str,
    model: Option<&KgModel>,
    files: &Files,
    counts: Vec<(String, u64)>,
    complete: bool,
) -> Manifest {
    let (name, version, edition) = project_identity(project);
    let inputs = project.inputs();
    let mut modules: Vec<ModuleEntry> = model
        .map(|model| {
            model
                .modules()
                .iter()
                .map(|module| ModuleEntry {
                    name: module.id.clone(),
                    key: module.key.clone(),
                    origin: module.origin.clone(),
                    path: inputs
                        .get(module.source)
                        .map_or_else(String::new, |input| input.path.replace('\\', "/")),
                    source: inputs
                        .get(module.source)
                        .map_or_else(String::new, |input| digest(input.text.as_bytes())),
                })
                .collect()
        })
        .unwrap_or_default();
    modules.sort_by(|a, b| a.name.cmp(&b.name));
    Manifest {
        schema: SCHEMA.to_owned(),
        compiler: compiler(),
        edition,
        project: name,
        version,
        complete,
        source_hash: target_hash.to_owned(),
        counts,
        modules,
        files: files
            .0
            .iter()
            .map(|(path, bytes)| (path.clone(), digest(bytes)))
            .collect::<BTreeMap<_, _>>(),
    }
}

/// The files of the graph of `analysis`, the manifest included, and whether it is complete.
pub(crate) fn files_of(
    target_hash: &str,
    project: &Project,
    analysis: Analysis<'_>,
) -> Result<(Files, bool, Option<KgModel>), Diagnostic> {
    match analysis {
        Analysis::Checked {
            module,
            capture,
            warnings,
        } => {
            let model_time = crate::timings::Phase::start("kg-model");
            let model = KgModel::build(&project.inputs(), module, *capture);
            drop(model_time);
            let (mut files, counts) = checked_files(project, &model, warnings)?;
            let manifest = manifest_for(project, target_hash, Some(&model), &files, counts, true);
            files.put(MANIFEST, manifest.render().into_bytes());
            Ok((files, true, Some(model)))
        }
        Analysis::Failed { errors } => {
            let mut files = Files::default();
            files.put("diag.jsonl", diagnostics_jsonl(errors, project));
            let (error_count, warning_count) = severities(errors);
            let counts = vec![
                ("errors".to_owned(), error_count),
                ("warnings".to_owned(), warning_count),
            ];
            let manifest = manifest_for(project, target_hash, None, &files, counts, false);
            files.put(MANIFEST, manifest.render().into_bytes());
            Ok((files, false, None))
        }
    }
}

/// Builds the files of the graph of `analysis` and publishes them at `target`.
pub(crate) fn finish(
    target: &Target,
    project: &Project,
    analysis: Analysis<'_>,
) -> Result<Outcome, Diagnostic> {
    let (files, complete, _) = files_of(&target.hash, project, analysis)?;
    let _publish = crate::timings::Phase::start("kg-publish");
    let written = publish(&target.dir, &files, &target.stamp)?;
    Ok(if complete {
        Outcome::Written { files: written }
    } else {
        Outcome::Incomplete { files: written }
    })
}

/// A graph to write after the analysis: where, and the declaration view that the analysis fills.
pub struct Pending {
    target: Target,
    capture: Capture,
}

/// Runs the analysis of a command (`check`, `build`, `run`, `test`) once: with a capture when
/// `plan` writes the graph, and as `analyze_cached` otherwise.
pub fn analyze(
    project: &Project,
    cache: bool,
    kind: crate::driver::AnalysisKind,
    plan: Plan,
) -> (Result<CheckedModule, DiagnosticSet>, Option<Pending>) {
    match plan {
        Plan::Build(target) => {
            let mut capture = Capture::default();
            let result = project.analyze_cached_captured(cache, kind, &mut capture);
            (result, Some(Pending { target, capture }))
        }
        Plan::Off | Plan::Fresh => (project.analyze_cached(cache, kind), None),
    }
}

/// Writes the graph of the analysis that `pending` captured: `Ok` with the module's warnings
/// (without `W1006`, which only a flag adds), or `Err` with its errors. Returns the notice of a
/// graph that could not be written.
pub fn complete(
    pending: Pending,
    project: &Project,
    result: Result<&CheckedModule, &DiagnosticSet>,
) -> Option<String> {
    let outcome = match result {
        Ok(module) => {
            let warnings = DiagnosticSet::from_diagnostics(module.warnings.iter().cloned(), 0);
            finish(
                &pending.target,
                project,
                Analysis::Checked {
                    module,
                    capture: Box::new(pending.capture),
                    warnings: &warnings,
                },
            )
        }
        Err(errors) => finish(&pending.target, project, Analysis::Failed { errors }),
    };
    outcome.err().map(|error| notice(&error))
}
