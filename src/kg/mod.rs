//! The knowledge graph of a program (G26, D-60): the facts that the compiler knows, as a typed
//! property graph. Nodes are declarations (`nodes.jsonl`), edges are typed relations between them
//! (`edges.jsonl`), and the property tables `tests.jsonl`, `facts/<Module>.jsonl` and
//! `layout/<Module>.jsonl` hold more facts of some nodes. Everything comes from one analysis, and
//! the same sources and compiler give the same bytes.
//!
//! G27 writes a project's graph to `target/tz/kg/` when `check`, `build`, `run` and `test` analyze
//! it (`build`, `write`, `manifest`), with brief cards (`brief`), and reads it back for queries
//! (`store`, `open`); the standard library's graph lives in the compiler's cache (`std_kg`).

pub mod algo;
pub mod brief;
pub mod build;
mod effects;
pub mod emit;
mod facts;
pub mod find;
pub mod git;
pub(crate) mod ids;
pub mod impact;
pub mod input;
pub(crate) mod json;
mod layout;
pub mod manifest;
pub mod model;
pub mod open;
pub mod query;
pub mod settings;
mod sites;
pub mod std_kg;
pub mod store;
pub mod suggest;
pub mod write;

pub use model::{EdgeRow, FactsRow, KgModel, LayoutRow, Location, ModuleInfo, NodeRow, TestRow};

use crate::check::ModuleOrigin;
use crate::check::kg_capture::Capture;
use crate::diagnostic::DiagnosticSet;
use crate::parallel::Parallelism;
use crate::{SourceInput, stdlib, syntax};

/// The schema of each row of `facts/<Module>.jsonl`.
pub const SCHEMA_FACTS: &str = "tz-facts/1";

/// Analyzes the project once and builds its knowledge graph; `Err` with the diagnostics when the
/// project does not type-check (no partial graph).
pub fn collect(project: &crate::driver::Project) -> Result<KgModel, DiagnosticSet> {
    collect_inputs(&project.inputs(), false)
}

/// `collect` for sources given as `(path, text)`: user modules without a namespace, read with the
/// edition of sources that no manifest governs, with the standard library.
pub fn collect_sources(sources: &[(&str, &str)]) -> Result<KgModel, DiagnosticSet> {
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
    collect_inputs(&inputs, false)
}

pub(crate) fn collect_inputs(
    inputs: &[SourceInput<'_>],
    include_std: bool,
) -> Result<KgModel, DiagnosticSet> {
    let mut capture = Capture {
        include_std,
        ..Capture::default()
    };
    let module = crate::analyze_inputs_captured(inputs, &mut capture, Parallelism::library())?;
    Ok(KgModel::build(inputs, &module, capture))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn sources_of(directory: &Path) -> Vec<(String, String)> {
        let mut found = Vec::new();
        let mut pending = vec![directory.to_owned()];
        while let Some(at) = pending.pop() {
            for entry in std::fs::read_dir(&at).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    pending.push(path);
                } else if path
                    .extension()
                    .is_some_and(|extension| matches!(extension.to_str(), Some("tz" | "tt" | "tc")))
                {
                    let relative = path
                        .strip_prefix(directory)
                        .unwrap()
                        .to_str()
                        .unwrap()
                        .replace('\\', "/");
                    found.push((relative, std::fs::read_to_string(&path).unwrap()));
                }
            }
        }
        found.sort();
        found
    }

    /// The capture only reads the checker's state: the IR and the warnings of an analysis with a
    /// capture are those of the same analysis without one.
    #[test]
    fn analysis_with_capture_keeps_the_ir_identical() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        for project in ["examples/point", "examples/polymorphism"] {
            let sources = sources_of(&root.join(project));
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
            let plain = crate::analyze_inputs_all(&inputs).expect(project);
            let mut capture = Capture::default();
            let captured =
                crate::analyze_inputs_captured(&inputs, &mut capture, Parallelism::library())
                    .expect(project);
            assert!(!capture.nodes.is_empty(), "{project}");
            let emit = |module| crate::llvm::emit(module, crate::llvm::Entry::Library).unwrap();
            assert_eq!(emit(&plain), emit(&captured), "{project}");
            let warnings = |module: &crate::check::CheckedModule| {
                module
                    .warnings
                    .iter()
                    .map(|warning| (warning.code, warning.span, warning.message.clone()))
                    .collect::<Vec<_>>()
            };
            assert_eq!(warnings(&plain), warnings(&captured), "{project}");
        }
        let project = crate::driver::Project::load(&root.join("tests/fixtures/kg_basic")).unwrap();
        let inputs = project.inputs();
        let plain = crate::analyze_inputs_all(&inputs).unwrap();
        let mut capture = Capture::default();
        let captured =
            crate::analyze_inputs_captured(&inputs, &mut capture, Parallelism::library()).unwrap();
        let emit = |module| crate::llvm::emit(module, crate::llvm::Entry::Library).unwrap();
        assert_eq!(emit(&plain), emit(&captured));
        assert_eq!(plain.warnings.len(), captured.warnings.len());
    }
}
