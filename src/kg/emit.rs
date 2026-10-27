//! The files of a knowledge graph model (G27 appendix H-2, D-60): the node and edge lists, the
//! test table, and the facts and layouts of the modules that have rows, named by the stem of the
//! module's key.

use super::model::KgModel;
use super::write::{Files, unique_names};

fn render(write: impl FnOnce(&mut Vec<u8>) -> std::io::Result<()>) -> std::io::Result<Vec<u8>> {
    let mut buffer = Vec::new();
    write(&mut buffer)?;
    Ok(buffer)
}

/// Every file of the model: `emit_rows` and `emit_facts`.
pub fn emit(model: &KgModel) -> std::io::Result<Files> {
    let mut files = emit_rows(model)?;
    files.0.extend(emit_facts(model)?.0);
    Ok(files)
}

/// The node and edge lists and the test table.
pub fn emit_rows(model: &KgModel) -> std::io::Result<Files> {
    let mut files = Files::default();
    files.put("nodes.jsonl", render(|out| model.write_nodes(out))?);
    files.put("edges.jsonl", render(|out| model.write_edges(out))?);
    files.put("tests.jsonl", render(|out| model.write_tests(out))?);
    Ok(files)
}

/// The facts and layouts of the modules that have rows, named by the stems of their keys.
pub fn emit_facts(model: &KgModel) -> std::io::Result<Files> {
    let mut files = Files::default();
    let keys: Vec<String> = model
        .fact_modules()
        .into_iter()
        .map(str::to_owned)
        .collect();
    let names = unique_names(&keys);
    for key in &keys {
        files.put(
            format!("facts/{}.jsonl", names[key]),
            render(|out| model.write_facts(key, out))?,
        );
    }
    let keys: Vec<String> = model
        .layout_modules()
        .into_iter()
        .map(str::to_owned)
        .collect();
    let names = unique_names(&keys);
    for key in &keys {
        files.put(
            format!("layout/{}.jsonl", names[key]),
            render(|out| model.write_layout(key, out))?,
        );
    }
    Ok(files)
}
