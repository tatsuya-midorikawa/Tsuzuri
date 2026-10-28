//! The knowledge graph of the standard library (G27 Phase 4, D-60). A project's graph does not
//! hold std; std's graph is decided by the compiler alone, so one copy lives in the compiler's
//! cache under `kg-std/<id>/` (`<id>` = the first 16 digits of the stamp), built the first time a
//! view needs it, and the three newest generations stay. Without a cache, or for a reader that
//! must not write (MCP), it is built in memory.

use std::path::{Path, PathBuf};

use super::build::current_stamp;
use super::emit::emit;
use super::manifest::{Manifest, ModuleEntry, SCHEMA, compiler};
use super::store::{KgData, StdSource};
use super::write::{Files, MANIFEST, publish};
use crate::cache::Sha256;
use crate::check::ModuleOrigin;
use crate::diagnostic::{Diagnostic, DiagnosticKind, Span};
use crate::{SourceInput, stdlib, syntax};

/// How many generations of the std graph the cache keeps.
const GENERATIONS: usize = 3;

fn inputs() -> Vec<SourceInput<'static>> {
    stdlib::SOURCES
        .iter()
        .map(|(path, text)| SourceInput {
            path,
            text,
            origin: ModuleOrigin::Std,
            namespace: "",
            edition: syntax::Edition::LATEST,
            package: None,
        })
        .collect()
}

/// The hash of the std sources that the compiler embeds.
fn sources_hash() -> String {
    let mut hash = Sha256::new();
    hash.field("tz-kg-std", &super::input::KG_FORMAT.to_le_bytes());
    for (path, text) in stdlib::SOURCES {
        hash.field("path", path.as_bytes());
        hash.field("source", text.as_bytes());
    }
    super::input::hex(&hash.finalize())
}

fn failure(message: String) -> Diagnostic {
    Diagnostic::of(DiagnosticKind::IoFailure, message, Span::default())
}

/// The files of the std graph, its manifest included: the nodes, edges, tests, facts and layouts
/// of every std module, with origin `std` and IDs without a namespace (`Array.map`).
pub fn files() -> Result<Files, Diagnostic> {
    let inputs = inputs();
    let model = super::collect_inputs(&inputs, true).map_err(|errors| {
        failure(format!(
            "cannot analyze the standard library: {}",
            errors
                .first()
                .map_or_else(String::new, |error| error.message.clone())
        ))
    })?;
    let mut files =
        emit(&model).map_err(|error| failure(format!("cannot render the std KG: {error}")))?;
    files.put("diag.jsonl", Vec::new());
    let modules = model
        .modules()
        .iter()
        .map(|module| ModuleEntry {
            name: module.id.clone(),
            key: module.key.clone(),
            origin: module.origin.clone(),
            path: inputs
                .get(module.source)
                .map_or_else(String::new, |input| input.path.to_owned()),
            source: inputs.get(module.source).map_or_else(String::new, |input| {
                super::input::digest(input.text.as_bytes())
            }),
        })
        .collect();
    let count = |path: &str| {
        files.get(path).map_or(0, |bytes| {
            bytes.iter().filter(|byte| **byte == b'\n').count() as u64
        })
    };
    let manifest = Manifest {
        schema: SCHEMA.to_owned(),
        compiler: compiler(),
        edition: syntax::Edition::LATEST.to_string(),
        project: "std".to_owned(),
        version: None,
        complete: true,
        source_hash: sources_hash(),
        counts: vec![
            ("modules".to_owned(), model.modules().len() as u64),
            ("nodes".to_owned(), count("nodes.jsonl")),
            ("edges".to_owned(), count("edges.jsonl")),
            ("errors".to_owned(), 0),
            ("warnings".to_owned(), 0),
        ],
        modules,
        files: files
            .0
            .iter()
            .map(|(path, bytes)| (path.clone(), super::input::digest(bytes)))
            .collect(),
    };
    files.put(MANIFEST, manifest.render().into_bytes());
    Ok(files)
}

/// The std graph in memory, written nowhere.
pub fn memory() -> Option<KgData> {
    let files = files().ok()?;
    let manifest = Manifest::parse(std::str::from_utf8(files.get(MANIFEST)?).ok()?).ok()?;
    KgData::from_parts(&manifest, &files).ok()
}

/// `<cache>/kg-std/`, or `None` without a cache.
pub fn cache_root() -> Option<PathBuf> {
    crate::cache::default_root().map(|root| root.join("kg-std"))
}

/// The directory of this compiler's std graph in the cache.
pub fn cache_directory() -> Option<PathBuf> {
    let stamp = current_stamp().ok()?;
    Some(cache_root()?.join(&stamp[..16]))
}

/// Whether `dir` holds a complete std graph of this compiler.
fn complete_at(dir: &Path) -> bool {
    super::store::read_manifest(dir)
        .is_some_and(|manifest| manifest.complete && manifest.compiler == compiler())
        && super::write::has_marker(dir)
}

/// Writes this compiler's std graph to the cache unless a complete one is there (`force` writes it
/// anyway), and keeps the newest generations. Returns the directory and how many files were
/// written (0 when it was up to date).
pub fn build(force: bool) -> Result<(PathBuf, usize), Diagnostic> {
    let dir = cache_directory()
        .ok_or_else(|| failure("there is no compiler cache directory for the std KG".to_owned()))?;
    if !force && complete_at(&dir) {
        return Ok((dir, 0));
    }
    let stamp = current_stamp()?;
    let written = publish(&dir, &files()?, &stamp)?;
    prune();
    Ok((dir, written))
}

/// Removes all but the newest generations of std graphs from the cache.
fn prune() {
    let Some(root) = cache_root() else {
        return;
    };
    let Ok(entries) = std::fs::read_dir(&root) else {
        return;
    };
    let mut generations: Vec<(std::time::SystemTime, PathBuf)> = entries
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            if !super::write::has_marker(&path) {
                return None;
            }
            let modified = std::fs::metadata(path.join(super::write::MARKER))
                .and_then(|metadata| metadata.modified())
                .ok()?;
            Some((modified, path))
        })
        .collect();
    generations.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    for (_, path) in generations.into_iter().skip(GENERATIONS) {
        super::write::remove_tree(&path);
    }
}

/// The std graph for a view: from the cache (built there once), or in memory.
pub fn load(source: StdSource) -> Option<KgData> {
    match source {
        StdSource::None => None,
        StdSource::Memory => memory(),
        StdSource::Cache => match build(false) {
            Ok((dir, _)) => KgData::open(&dir).ok().or_else(memory),
            Err(_) => memory(),
        },
        StdSource::Cached => {
            let dir = cache_directory()?;
            complete_at(&dir).then(|| KgData::open(&dir).ok()).flatten()
        }
    }
}
