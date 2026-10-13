//! The implicit copies of arrays and lists (A15's `copies::sites`) over the projects that the
//! repository ships: `tests/fixtures`, `benchmarks` and `examples`. A measuring tool, not a
//! regression test (A17 reads its output before and after each phase):
//!
//! ```sh
//! cargo test --locked --test copy_inventory -- --ignored --nocapture
//! TZ_INVENTORY_SITES=1 cargo test --locked --test copy_inventory -- --ignored --nocapture
//! ```
//!
//! A position is counted once however many specializations copy there. Std sites are the ones
//! that the shipped projects instantiate, so they are a lower bound for the standard library.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use tsuzuri::check::{ModuleOrigin, TypedExpr, TypedExprKind};
use tsuzuri::copies::{self, CopyCost};
use tsuzuri::diagnostic::Span;

fn collect(root: &Path, directory: &Path, sources: &mut Vec<(String, String)>) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    let mut paths: Vec<PathBuf> = entries.flatten().map(|entry| entry.path()).collect();
    paths.sort();
    for path in paths {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        if path.is_dir() {
            if !name.starts_with('.') && name != "target" {
                collect(root, &path, sources);
            }
        } else if [".tz", ".tt", ".tc"]
            .iter()
            .any(|suffix| name.ends_with(suffix))
        {
            let relative = path
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .to_string();
            if let Ok(text) = fs::read_to_string(&path) {
                sources.push((relative, text));
            }
        }
    }
}

fn variant(expression: &TypedExpr) -> String {
    let text = format!("{:?}", expression.kind);
    let end = text.find(['(', ' ', '{']).unwrap_or(text.len());
    text[..end].to_string()
}

fn is_copied_expression(kind: &TypedExprKind, site_kind: &str) -> bool {
    match site_kind {
        "Local" => matches!(kind, TypedExprKind::Local(_)),
        "Field" => matches!(kind, TypedExprKind::Field(..)),
        "Element" => matches!(kind, TypedExprKind::Index(..)),
        "Tail" => matches!(kind, TypedExprKind::ListTail(..)),
        "Payload" => matches!(kind, TypedExprKind::UnionPayload { .. }),
        "Dereference" => matches!(kind, TypedExprKind::Dereference(_)),
        _ => true,
    }
}

/// What the copied expression is an operand of; a pattern binding has no expression of its own.
fn context(
    expression: &TypedExpr,
    parent: Option<&TypedExpr>,
    span: Span,
    site_kind: &str,
) -> Option<String> {
    if expression.span == span && is_copied_expression(&expression.kind, site_kind) {
        return Some(match parent {
            None => "root".to_string(),
            Some(parent) => match &parent.kind {
                TypedExprKind::Call(callee, _) => format!("Call[{}]", variant(callee)),
                TypedExprKind::Match { value, .. } => {
                    format!("Match[scrutinee={}]", variant(value))
                }
                TypedExprKind::Block { bindings, .. }
                    if bindings
                        .iter()
                        .any(|(_, value)| std::ptr::eq(value, expression)) =>
                {
                    "Block.binding".to_string()
                }
                TypedExprKind::Block { .. } => "Block.result".to_string(),
                _ => variant(parent),
            },
        });
    }
    expression
        .children()
        .into_iter()
        .find_map(|child| context(child, Some(expression), span, site_kind))
}

fn line_of(text: &str, offset: usize) -> (usize, String) {
    let offset = offset.min(text.len());
    let line = text[..offset].matches('\n').count() + 1;
    let start = text[..offset].rfind('\n').map_or(0, |index| index + 1);
    let end = text[offset..]
        .find('\n')
        .map_or(text.len(), |index| offset + index);
    (line, text[start..end].trim().to_string())
}

#[test]
#[ignore = "a measuring tool; run it with --ignored --nocapture"]
fn inventory_of_implicit_copies() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut projects = Vec::new();
    for group in ["tests/fixtures", "benchmarks", "examples"] {
        let mut paths: Vec<PathBuf> = fs::read_dir(root.join(group))
            .unwrap()
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.is_dir())
            .collect();
        paths.sort();
        projects.extend(paths);
    }
    let print_sites = std::env::var_os("TZ_INVENTORY_SITES").is_some();
    let mut positions = BTreeMap::new();
    let mut skipped = Vec::new();
    for project in &projects {
        let name = project.strip_prefix(root).unwrap().display().to_string();
        let mut sources = Vec::new();
        collect(project, project, &mut sources);
        let inputs: Vec<(&str, &str)> = sources
            .iter()
            .map(|(path, text)| (path.as_str(), text.as_str()))
            .collect();
        let std_sources = tsuzuri::stdlib::sources_for(inputs.iter().map(|(_, text)| *text));
        let module = match tsuzuri::analyze_modules_with_std(&inputs, &std_sources) {
            Ok(module) => module,
            Err(error) => {
                skipped.push(format!("{name}: {} {}", error.code, error.message));
                continue;
            }
        };
        for site in copies::sites(&module) {
            if site.cost != CopyCost::Length {
                continue;
            }
            let function = &module.functions[site.function];
            let origin = if function.origin.module == ModuleOrigin::Std {
                "std"
            } else {
                "user"
            };
            let (file, line, text) = match site.span.source {
                Some(index) if index < inputs.len() => {
                    let (line, text) = line_of(inputs[index].1, site.span.start);
                    (format!("{name}/{}", inputs[index].0), line, text)
                }
                Some(index) if index - inputs.len() < std_sources.len() => {
                    let (path, source) = std_sources[index - inputs.len()];
                    let (line, text) = line_of(source, site.span.start);
                    (path.to_string(), line, text)
                }
                _ => ("?".to_string(), 0, String::new()),
            };
            let kind = format!("{:?}", site.kind);
            let context = context(&function.body, None, site.span, &kind)
                .unwrap_or_else(|| "pattern-binding".to_string());
            positions.insert(
                (origin, file, line, kind.clone(), text.clone()),
                (
                    context,
                    function.qualified_name(),
                    site.ty.display(&module.types()),
                    name.clone(),
                ),
            );
        }
    }
    let mut table: BTreeMap<(&str, String, String), usize> = BTreeMap::new();
    let mut by_project: BTreeMap<String, usize> = BTreeMap::new();
    for ((origin, file, line, kind, text), (context, function, ty, project)) in &positions {
        *table
            .entry((origin, kind.clone(), context.clone()))
            .or_default() += 1;
        if *origin == "user" {
            *by_project.entry(project.clone()).or_default() += 1;
        }
        if print_sites {
            println!("SITE\t{origin}\t{file}:{line}\t{kind}\t{context}\t{function}\t{ty}\t{text}");
        }
    }
    for origin in ["user", "std"] {
        let total: usize = table
            .iter()
            .filter(|((o, _, _), _)| *o == origin)
            .map(|(_, count)| count)
            .sum();
        println!("\n{origin}: {total} positions that copy arrays or lists");
        for ((_, kind, context), count) in table.iter().filter(|((o, _, _), _)| *o == origin) {
            println!("  {count:4}  {kind:<12} {context}");
        }
    }
    let mut ranked: Vec<_> = by_project.iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    println!("\nuser positions per project (top 12):");
    for (project, count) in ranked.into_iter().take(12) {
        println!("  {count:4}  {project}");
    }
    let distinct: BTreeSet<_> = positions.keys().map(|key| key.0).collect();
    println!(
        "\nprojects analyzed: {}, skipped: {} {:?}, origins seen: {distinct:?}",
        projects.len() - skipped.len(),
        skipped.len(),
        skipped
    );
}
