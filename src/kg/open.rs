//! Opening a project's knowledge graph for a query (G27 Phase 2, D-60): with `Refresh::Auto` a
//! graph that is missing or stale is built first (one analysis, its diagnostics reported as
//! `check` reports them); with `Refresh::Never` the graph on disk is read as it is, and one that is
//! missing, stale, broken or incomplete is an `E2009`.

use std::path::{Path, PathBuf};

use super::build::{self, Analysis, Plan};
use super::input::{hex, source_hash};
use super::manifest::{self, Freshness};
use super::settings::KgSettings;
use super::store::KgData;
use crate::check::kg_capture::Capture;
use crate::diagnostic::{Diagnostic, DiagnosticSet};
use crate::driver::{AnalysisKind, Project};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refresh {
    Auto,
    Never,
}

/// The project at `path` (a directory or a file, as `check` takes it; `Main.tz` is not needed).
pub fn load(path: &Path) -> Result<Project, Diagnostic> {
    Project::load_for_tests(path).map_err(|error| error.diagnostic)
}

/// Where the graph of `project` is: `kg_dir`, `TSUZURI_KG_DIR`, or `<root>/target/tz/kg`.
pub fn directory(project: &Project, kg_dir: Option<&Path>) -> PathBuf {
    let dir = kg_dir.map(Path::to_path_buf).or_else(|| {
        std::env::var_os("TSUZURI_KG_DIR")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
    });
    KgSettings::explicit(dir).directory(project)
}

/// The graph directory of a second project of one command (the base of `kg impact` and
/// `test --affected`): its own `<root>/target/tz/kg`. `--kg-dir` and `TSUZURI_KG_DIR` name the
/// graph of the command's own project, and two graphs in one directory would replace each other.
pub fn own_directory(project: &Project) -> PathBuf {
    KgSettings::explicit(None).directory(project)
}

/// Whether two graph directories are one, so that opening the second may replace the first.
pub fn same_directory(a: &Path, b: &Path) -> bool {
    let real = |path: &Path| std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    real(a) == real(b)
}

/// Reads the graph of `project` at `dir`. `report` receives the diagnostics of an analysis that
/// `Refresh::Auto` runs.
pub fn open_project(
    project: &Project,
    dir: &Path,
    refresh: Refresh,
    report: &mut dyn FnMut(&DiagnosticSet, &Project),
) -> Result<KgData, Diagnostic> {
    match refresh {
        Refresh::Never => {
            let hash = hex(&source_hash(project));
            let stamp = build::current_stamp()?;
            match manifest::inspect(dir, &hash, &stamp) {
                Freshness::Fresh => read(dir),
                Freshness::NotOurs if !super::write::has_marker(dir) && dir.is_dir() => {
                    Err(manifest::missing(dir, true))
                }
                other => Err(manifest::diagnostic(dir, &other).expect("not fresh")),
            }
        }
        Refresh::Auto => {
            if let Plan::Build(target) = build::target_for(dir, project, false)? {
                let mut capture = Capture::default();
                let result =
                    project.analyze_cached_captured(true, AnalysisKind::Tests, &mut capture);
                match &result {
                    Ok(module) => {
                        let warnings =
                            DiagnosticSet::from_diagnostics(module.warnings.iter().cloned(), 0);
                        report(&warnings, project);
                        build::finish(
                            &target,
                            project,
                            Analysis::Checked {
                                module,
                                capture: Box::new(capture),
                                warnings: &warnings,
                            },
                        )?;
                    }
                    Err(errors) => {
                        report(errors, project);
                        build::finish(&target, project, Analysis::Failed { errors })?;
                    }
                }
            }
            read(dir)
        }
    }
}

/// Reads the graph at `dir` without looking at the sources: `E2009` when it is missing,
/// broken or incomplete.
pub fn read(dir: &Path) -> Result<KgData, Diagnostic> {
    let data = KgData::open(dir).map_err(|freshness| match freshness {
        Freshness::NotOurs if dir.is_dir() => manifest::missing(dir, true),
        other => manifest::diagnostic(dir, &other).unwrap_or_else(|| manifest::missing(dir, false)),
    })?;
    if !data.meta.complete {
        let errors = super::store::read_manifest(dir)
            .and_then(|manifest| manifest.count("errors"))
            .unwrap_or(0);
        return Err(manifest::incomplete(dir, errors));
    }
    Ok(data)
}
