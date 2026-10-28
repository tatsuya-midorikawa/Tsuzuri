//! `tz_check` (G29 "`tz_check`") and `tz_fix_plan`: the compiler's diagnostics and fix plan, with
//! the paths relative to the project root and the root's absolute path scrubbed from every text.

use serde_json::{Map, Value, json};

use super::backend::{ProjectBackend, ToolError, ToolOutput, ToolRequest};
use super::path::{Wanted, relative_text, resolve_path, scrub};
use crate::check::ModuleOrigin;
use crate::diagnostic::{Diagnostic, DiagnosticSet, Severity};
use crate::driver::{Project, SourceFile};

/// The path a diagnostic shows for `source`: from the root, or the source's own relative path
/// for a dependency or std.
fn shown_path(root: &std::path::Path, source: &SourceFile) -> String {
    let real = crate::cache::real_path(&source.path).unwrap_or_else(|_| source.path.clone());
    if real.starts_with(root) {
        relative_text(root, &real)
    } else {
        source.relative_path.to_string_lossy().replace('\\', "/")
    }
}

fn plural(count: usize, one: &str, many: &str) -> String {
    format!("{count} {}", if count == 1 { one } else { many })
}

/// Scrubs the root out of every string of a JSON value.
fn scrub_value(value: &mut Value, root: &std::path::Path) {
    match value {
        Value::String(text) => *text = scrub(text, root),
        Value::Array(items) => items.iter_mut().for_each(|item| scrub_value(item, root)),
        Value::Object(object) => object.values_mut().for_each(|item| scrub_value(item, root)),
        _ => {}
    }
}

pub(crate) fn run(
    backend: &ProjectBackend,
    request: &ToolRequest<'_>,
) -> Result<ToolOutput, ToolError> {
    let root = &backend.root;
    let arguments = request.arguments;
    let path = arguments.get("path").and_then(Value::as_str).unwrap_or("");
    let show_warnings = arguments
        .get("warnings")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let implicit_copy = arguments
        .get("implicit_copy")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let input = resolve_path(root, path, Wanted::Source)?;
    let project = match Project::load(&input) {
        Ok(project) => project,
        Err(error) => {
            let shown = relative_text(root, &error.path);
            let mut diagnostic: Value = serde_json::from_str(&error.diagnostic.json_with(
                error.diagnostic.severity.as_str(),
                &shown,
                "",
                &|_| None,
            ))
            .unwrap_or(Value::Null);
            scrub_value(&mut diagnostic, root);
            let text = scrub(
                &error.diagnostic.render_with(
                    error.diagnostic.severity.as_str(),
                    &shown,
                    "",
                    &|_| None,
                ),
                root,
            );
            return Ok(ToolOutput {
                text: format!("{text}\n\nFAILED: 1 error, 0 warnings in 0 files"),
                structured: json!({
                    "ok": false,
                    "files": 0,
                    "errors": 1,
                    "warnings": 0,
                    "omitted": 0,
                    "diagnostics": [diagnostic],
                }),
            });
        }
    };
    backend.count_analysis();
    let (set, ok) = match project.analyze_all() {
        Ok(module) => {
            let mut warnings = module.warnings.clone();
            if implicit_copy {
                warnings.extend(crate::copies::warnings(&module));
            }
            (DiagnosticSet::from_diagnostics(warnings, 0), true)
        }
        Err(set) => (set, false),
    };
    let diagnostics: Vec<&Diagnostic> = set
        .diagnostics
        .iter()
        .filter(|diagnostic| show_warnings || diagnostic.severity == Severity::Error)
        .collect();
    let files: Vec<(String, &str)> = project
        .sources
        .iter()
        .chain(&project.manifests)
        .map(|file| (shown_path(root, file), file.text.as_str()))
        .collect();
    let lookup = |index: usize| files.get(index).map(|(path, text)| (path.as_str(), *text));
    let mut rendered = Vec::new();
    let mut json = Vec::new();
    for diagnostic in &diagnostics {
        let source = project.source_for(diagnostic);
        let shown = shown_path(root, source);
        let severity = diagnostic.severity.as_str();
        rendered.push(scrub(
            &diagnostic.render_with(severity, &shown, &source.text, &lookup),
            root,
        ));
        let mut value: Value =
            serde_json::from_str(&diagnostic.json_with(severity, &shown, &source.text, &lookup))
                .unwrap_or(Value::Null);
        scrub_value(&mut value, root);
        json.push(value);
    }
    let errors = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.severity == Severity::Error)
        .count();
    let warnings = diagnostics.len() - errors;
    let source_files = project
        .sources
        .iter()
        .filter(|source| source.origin == ModuleOrigin::User)
        .count();
    let mut text = rendered.join("\n\n");
    if let Some(note) = set.omission_note() {
        if !text.is_empty() {
            text.push_str("\n\n");
        }
        text.push_str(&note);
    }
    if !text.is_empty() {
        text.push_str("\n\n");
    }
    text.push_str(&format!(
        "{}: {}, {} in {}",
        if ok { "OK" } else { "FAILED" },
        plural(errors, "error", "errors"),
        plural(warnings, "warning", "warnings"),
        plural(source_files, "file", "files")
    ));
    Ok(ToolOutput {
        text,
        structured: json!({
            "ok": ok,
            "files": source_files,
            "errors": errors,
            "warnings": warnings,
            "omitted": set.omitted,
            "diagnostics": json,
        }),
    })
}

/// `tz_fix_plan`: `tsuzuri fix --dry-run` in memory. No file is written.
pub(crate) fn fix_plan(
    backend: &ProjectBackend,
    request: &ToolRequest<'_>,
) -> Result<ToolOutput, ToolError> {
    use crate::fix::{Options, Scope, Threshold};
    let root = &backend.root;
    let path = request
        .arguments
        .get("path")
        .and_then(Value::as_str)
        .unwrap_or("");
    let threshold = match request
        .arguments
        .get("applicability")
        .and_then(Value::as_str)
    {
        Some("likely") => Threshold::Likely,
        Some("maybe") => Threshold::Maybe,
        _ => Threshold::Certain,
    };
    let input = resolve_path(root, path, Wanted::Source)?;
    let mut project =
        Project::load(&input).map_err(|error| ToolError(scrub(&error.diagnostic.message, root)))?;
    let scope = if input.is_dir() {
        Scope::Project
    } else {
        Scope::File(project.root)
    };
    let options = Options {
        threshold,
        scope,
        only_codes: None,
    };
    backend.count_analysis();
    let report = crate::fix::run(&mut project, &options, &mut |project| {
        crate::fix::analyze_project(project, false)
    })
    .map_err(|error| ToolError(scrub(&error.diagnostic.message, root)))?;
    // The analysis is done: the plan's paths are relative to the root, as `tz_check`'s are (a
    // project at `path` below the root names its files from its own directory).
    let mut rebased: std::collections::BTreeMap<String, String> = Default::default();
    for source in &mut project.sources {
        if source.origin != ModuleOrigin::User {
            continue;
        }
        let shown = shown_path(root, source);
        let own = source.relative_path.to_string_lossy().replace('\\', "/");
        if shown != own {
            source.relative_path = std::path::PathBuf::from(&shown);
            rebased.insert(own, shown);
        }
    }
    let text = scrub(
        &crate::fix::render_text(&report, &project, true, threshold),
        root,
    );
    let mut fixes = Vec::new();
    let mut dropped = Vec::new();
    let mut diff = String::new();
    let mut summary = Value::Null;
    for line in crate::fix::render_json(&report, &project, true, threshold).lines() {
        let Ok(mut row) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        scrub_value(&mut row, root);
        if let Some(edits) = row.get_mut("edits").and_then(Value::as_array_mut) {
            for edit in edits {
                if let Some(path) = edit["path"].as_str()
                    && let Some(shown) = rebased.get(path)
                {
                    edit["path"] = Value::String(shown.clone());
                }
            }
        }
        match row["event"].as_str() {
            Some("fix") if row["status"] == "applied" => {
                let mut fix = Map::new();
                for key in ["code", "kind", "applicability", "title", "round", "edits"] {
                    if let Some(value) = row.get(key) {
                        fix.insert(key.into(), value.clone());
                    }
                }
                fixes.push(Value::Object(fix));
            }
            Some("fix") => dropped.push(json!({
                "code": row["code"],
                "applicability": row["applicability"],
                "title": row["title"],
                "reason": row["reason"],
            })),
            Some("file") => {
                if let Some(text) = row["diff"].as_str() {
                    diff.push_str(text);
                }
            }
            Some("summary") => summary = row,
            _ => {}
        }
    }
    Ok(ToolOutput {
        text,
        structured: json!({
            "applicability": threshold.name(),
            "rounds": summary["rounds"],
            "stop": summary["stop"],
            "fixes": fixes,
            "dropped": dropped,
            "diff": diff,
            "remaining_errors": summary["remaining_errors"],
            "remaining_warnings": summary["remaining_warnings"],
        }),
    })
}
