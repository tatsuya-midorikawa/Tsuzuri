use std::path::{Path, PathBuf};

use crate::check::ModuleOrigin;
use crate::diagnostic::{Diagnostic, DiagnosticKind, Span};
use crate::driver::{Project, SourceError, replace_source_atomically};
use crate::syntax::{Edition, MAX_SOURCE_BYTES, TypeExprKind};

#[derive(Debug)]
pub struct Change {
    pub path: PathBuf,
    pub before: String,
    pub after: String,
}

#[derive(Debug)]
pub struct Migration {
    pub changes: Vec<Change>,
    pub manifest: bool,
    pub source_edition: Edition,
    pub target: Edition,
}

fn error(kind: DiagnosticKind, path: &Path, message: impl Into<String>) -> SourceError {
    SourceError::new(path, Diagnostic::of(kind, message, Span::default()))
}

fn analysis_error(project: &Project, diagnostic: Diagnostic) -> SourceError {
    SourceError::new(&project.source_for(&diagnostic).path, diagnostic)
}

fn preserve_parameters(
    project: &mut Project,
    indices: &[usize],
    semantic: &crate::check::semantic::SemanticIndex,
) -> Result<Vec<Change>, SourceError> {
    let functions: std::collections::BTreeMap<_, _> = semantic
        .definitions
        .iter()
        .enumerate()
        .filter(|(_, definition)| {
            definition.kind == crate::check::semantic::SymbolKind::Function
                && definition
                    .span
                    .source
                    .is_some_and(|source| project.sources[source].origin == ModuleOrigin::Std)
        })
        .filter_map(|(id, definition)| {
            crate::check::BORROWING_APIS
                .iter()
                .find(|(value, _, _)| {
                    value.split_once('.')
                        == Some((definition.module.as_str(), definition.name.as_str()))
                })
                .map(|(_, _, owned)| (id, owned.rsplit('.').next().unwrap()))
        })
        .collect();
    let mut changes = Vec::new();
    for &id in indices {
        let source = &mut project.sources[id];
        let program = crate::parser::parse_with_source(&source.text, id, source.edition)
            .map_err(|diagnostic| SourceError::new(&source.path, diagnostic))?;
        let mut edits = Vec::new();
        for declaration in &program.functions {
            if declaration.constant || declaration.name.text.starts_with("$instance.") {
                continue;
            }
            for parameter in &declaration.parameters {
                if parameter.mutable
                    || crate::check::has_explicit_parameter_regions(&parameter.ty)
                    || matches!(
                        parameter.ty.kind,
                        TypeExprKind::Own(_) | TypeExprKind::Reference(..)
                    )
                {
                    continue;
                }
                edits.push((parameter.ty.span.start, parameter.ty.span.start, "own "));
            }
        }
        for reference in &semantic.references {
            if reference.role == crate::check::semantic::Role::Read
                && reference.span.source == Some(id)
                && let Some(&owned) = functions.get(&reference.definition)
                && source.text.get(reference.span.start..reference.span.end)
                    == Some(semantic.definitions[reference.definition].name.as_str())
            {
                edits.push((reference.span.start, reference.span.end, owned));
            }
        }
        for (span, builtin) in &semantic.builtin_references {
            if span.source != Some(id) {
                continue;
            }
            let Some((value, _, owned)) = crate::check::BORROWING_APIS
                .iter()
                .find(|(value, _, _)| *value == builtin.name())
            else {
                continue;
            };
            let Some(spelling) = source.text.get(span.start..span.end) else {
                continue;
            };
            let Ok(tokens) = crate::lexer::lex(spelling, source.edition) else {
                continue;
            };
            let mut path = String::new();
            let mut last = Span::default();
            for token in tokens {
                match token.kind {
                    crate::syntax::TokenKind::Ident(name) => {
                        path.push_str(&name);
                        last = token.span;
                    }
                    crate::syntax::TokenKind::Dot => path.push('.'),
                    crate::syntax::TokenKind::PathSep => path.push_str("::"),
                    crate::syntax::TokenKind::End => {}
                    _ => {
                        path.clear();
                        break;
                    }
                }
            }
            let path = path
                .strip_prefix(crate::stdlib::NAMESPACE)
                .and_then(|rest| rest.strip_prefix("::"))
                .unwrap_or(&path);
            if path == *value {
                edits.push((
                    span.start + last.start,
                    span.start + last.end,
                    owned.rsplit('.').next().unwrap(),
                ));
            }
        }
        let before = source.text.clone();
        edits.sort_unstable();
        edits.dedup();
        for (start, end, replacement) in edits.into_iter().rev() {
            if !source.text.is_char_boundary(start) || !source.text.is_char_boundary(end) {
                return Err(error(
                    DiagnosticKind::MigrationInvalidEdit,
                    &source.path,
                    "invalid parameter span during migration",
                ));
            }
            source.text.replace_range(start..end, replacement);
        }
        if source.text.len() > MAX_SOURCE_BYTES {
            return Err(error(
                DiagnosticKind::MigrationSourceTooLarge,
                &source.path,
                "migration exceeds the source size limit",
            ));
        }
        if before != source.text {
            changes.push(Change {
                path: source.path.clone(),
                before,
                after: source.text.clone(),
            });
        }
    }
    Ok(changes)
}

pub fn prepare(
    input: &Path,
    target: Edition,
    from: Option<Edition>,
) -> Result<Migration, SourceError> {
    let directory = input.is_dir();
    let canonical_std = if directory && input.file_name().is_some_and(|name| name == "std") {
        let project = Project::load_for_docs(input)?;
        project.sources.iter().all(|source| {
            source.origin == ModuleOrigin::Std
                && crate::stdlib::SOURCES.iter().any(|(path, text)| {
                    Path::new(path) == source.relative_path && *text == source.text
                })
        })
    } else {
        false
    };
    let mut project = if canonical_std {
        Project::load_for_docs(input)?
    } else {
        Project::load(input)?
    };
    let root_package = project.root_package.clone();
    let indices: Vec<_> = project
        .sources
        .iter()
        .enumerate()
        .filter(|(id, source)| {
            (canonical_std
                || (source.origin == ModuleOrigin::User && source.package == root_package))
                && (directory || *id == project.root)
        })
        .map(|(id, _)| id)
        .collect();
    let current = project.sources[project.root].edition;
    if from.is_some_and(|from| project.root_package.is_some() && from != current) {
        return Err(error(
            DiagnosticKind::MigrationEditionMismatch,
            input,
            "--from disagrees with the package manifest",
        ));
    }
    let current = from.unwrap_or(current);
    if target < current {
        return Err(error(
            DiagnosticKind::MigrationEditionBackwards,
            input,
            format!("cannot migrate from edition {current} to {target}"),
        ));
    }
    if target == current {
        return Ok(Migration {
            changes: Vec::new(),
            manifest: false,
            source_edition: current,
            target,
        });
    }
    for source in &mut project.sources {
        if canonical_std || (source.origin == ModuleOrigin::User && source.package == root_package)
        {
            source.edition = current;
        }
    }
    let mut semantic = crate::check::semantic::SemanticIndex::default();
    crate::analyze_inputs_semantic(&project.inputs(), &mut semantic)
        .map_err(crate::first_error)
        .map_err(|diagnostic| analysis_error(&project, diagnostic))?;
    let mut changes = preserve_parameters(&mut project, &indices, &semantic)?;
    for &id in &indices {
        project.sources[id].edition = target;
    }
    project
        .analyze()
        .map_err(|diagnostic| analysis_error(&project, diagnostic))?;
    let mut manifest = false;
    if directory
        && let Some(source) = project.manifests.iter().find(|source| {
            source.package == root_package && source.relative_path == Path::new("Tsuzuri.toml")
        })
    {
        let after = set_manifest_edition(&source.text, target).ok_or_else(|| {
            error(
                DiagnosticKind::MigrationManifestUpdateFailed,
                &source.path,
                "cannot update the package edition",
            )
        })?;
        let parsed = crate::package::parse_manifest(&after, 0)
            .map_err(|diagnostic| SourceError::new(&source.path, diagnostic))?;
        if parsed.edition != target {
            return Err(error(
                DiagnosticKind::MigrationManifestUpdateFailed,
                &source.path,
                "manifest edition verification failed",
            ));
        }
        changes.push(Change {
            path: source.path.clone(),
            before: source.text.clone(),
            after,
        });
        manifest = true;
    }
    Ok(Migration {
        changes,
        manifest,
        source_edition: current,
        target,
    })
}

pub fn apply(migration: &Migration) -> Result<(), SourceError> {
    let mut applied: Vec<&Change> = Vec::new();
    for change in &migration.changes {
        if let Err(diagnostic) =
            replace_source_atomically(&change.path, &change.before, &change.after)
        {
            let mut diagnostic = diagnostic.recode(DiagnosticKind::MigrationWriteFailed);
            for previous in applied.into_iter().rev() {
                if let Err(rollback) =
                    replace_source_atomically(&previous.path, &previous.after, &previous.before)
                {
                    diagnostic.message.push_str(&format!(
                        "; rollback refused for '{}': {}",
                        previous.path.display(),
                        rollback.message
                    ));
                }
            }
            return Err(SourceError::new(&change.path, diagnostic));
        }
        applied.push(change);
    }
    Ok(())
}

pub fn set_manifest_edition(text: &str, edition: Edition) -> Option<String> {
    let mut package = false;
    let mut version_end = None;
    let mut offset = 0;
    let newline = if text.contains("\r\n") { "\r\n" } else { "\n" };
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            if package {
                break;
            }
            package = trimmed.split('#').next()?.trim() == "[package]";
        } else if package && let Some((key, value)) = trimmed.split_once('=') {
            if key.trim() == "edition" {
                let value = value.trim_start();
                let inner = value.strip_prefix('"')?;
                let end = inner.find('"')?;
                let start = offset + line.find(value)? + 1;
                let mut changed = text.to_owned();
                changed.replace_range(start..start + end, &edition.to_string());
                return Some(changed);
            }
            if key.trim() == "version" {
                version_end = Some(offset + line.len());
            }
        }
        offset += line.len();
    }
    let end = version_end?;
    let mut changed = text.to_owned();
    let insertion = if text[..end].ends_with('\n') {
        format!("edition = \"{edition}\"{newline}")
    } else {
        format!("{newline}edition = \"{edition}\"")
    };
    changed.insert_str(end, &insertion);
    Some(changed)
}
