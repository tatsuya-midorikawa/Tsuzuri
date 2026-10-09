use crate::check::ModuleOrigin;
use crate::diagnostic::{Diagnostic, Span};
use crate::driver::Project;
use crate::syntax::*;
use std::collections::{BTreeMap, BTreeSet};

pub(crate) fn render_project(project: &Project) -> Result<BTreeMap<String, String>, Diagnostic> {
    let has_user = project
        .sources
        .iter()
        .any(|source| source.origin == ModuleOrigin::User);
    let mut pages: BTreeMap<String, String> = BTreeMap::new();
    let mut filenames = BTreeSet::from(["index.md".to_owned()]);
    let mut modules = BTreeMap::new();
    for (id, source) in project.sources.iter().enumerate() {
        if has_user && source.origin != ModuleOrigin::User {
            continue;
        }
        let program = crate::parser::parse_with_source(&source.text, id)?;
        let name = match (source.origin, &program.namespace) {
            (ModuleOrigin::User, Some(declared)) => {
                crate::module_identity(
                    &source.relative_path.to_string_lossy(),
                    Some(&declared.path),
                    &source.namespace,
                )
                .map_err(|error| Diagnostic {
                    span: error.span.in_source(id),
                    ..error
                })?
                .0
            }
            _ => source.name.clone(),
        };
        let filename = format!("{name}.md");
        if !pages.contains_key(&filename) && !filenames.insert(filename.to_ascii_lowercase()) {
            return Err(Diagnostic::new(
                "E2003",
                "documentation filenames collide, including the reserved index.md name",
                Span::default().in_source(id),
            ));
        }
        if let Some(page) = pages.get_mut(&filename) {
            page.push_str(&render_declarations(&program));
        } else {
            let namespace = match source.origin {
                ModuleOrigin::Std => Some(crate::stdlib::NAMESPACE),
                ModuleOrigin::User => program
                    .namespace
                    .as_ref()
                    .map(|declared| declared.path.text.as_str()),
            };
            pages.insert(
                filename.clone(),
                module_page(&name.replace('.', "::"), namespace, &program),
            );
        }
        modules.insert(name, filename);
    }
    let mut index = "# Modules\n\n".to_owned();
    for (name, filename) in modules {
        index.push_str(&format!("- [{}]({filename})\n", name.replace('.', "::")));
    }
    pages.insert("index.md".into(), index);
    Ok(pages)
}

fn type_text(ty: &TypeExpr) -> String {
    match &ty.kind {
        TypeExprKind::Named(name) => name.clone(),
        TypeExprKind::Variable(name) => format!("'{name}"),
        TypeExprKind::Apply(name, arguments) => format!(
            "{}<{}>",
            name.text,
            arguments
                .iter()
                .map(type_text)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        TypeExprKind::Regions(inner, names) => {
            let regions = region_text(names);
            if let TypeExprKind::Reference(value, mutable) = &inner.kind {
                format!(
                    "ref{}{regions} {}",
                    if *mutable { " mut" } else { "" },
                    type_text(value)
                )
            } else {
                format!("{}{regions}", type_text(inner))
            }
        }
        TypeExprKind::Array(element) => format!("[{}]", type_text(element)),
        TypeExprKind::ArrayView(element) => format!("[{}..]", type_text(element)),
        TypeExprKind::FixedArray(element, length) => {
            format!("[{}; {}]", type_text(element), type_text(length))
        }
        TypeExprKind::Length(length) => length.to_string(),
        TypeExprKind::Dyn(dyn_type) => dyn_text(dyn_type),
        TypeExprKind::Quantified(names, inner) => {
            let regions = names
                .iter()
                .map(|name| name.text.as_str())
                .collect::<Vec<_>>()
                .join(" ");
            let inner = type_text(inner);
            // A function type's text is already parenthesized.
            let inner = inner
                .strip_prefix('(')
                .and_then(|text| text.strip_suffix(')'))
                .unwrap_or(&inner);
            format!("({{{regions}}} {inner})")
        }
        TypeExprKind::List(element) => format!("[|{}|]", type_text(element)),
        TypeExprKind::Tuple(elements) => format!(
            "({})",
            elements
                .iter()
                .map(type_text)
                .collect::<Vec<_>>()
                .join(" * ")
        ),
        TypeExprKind::Task(result) => format!("Task<{}>", type_text(result)),
        TypeExprKind::Function(parameters, result) => {
            let arguments = if parameters.is_empty() {
                "fn()".into()
            } else {
                parameters
                    .iter()
                    .map(type_text)
                    .collect::<Vec<_>>()
                    .join(" -> ")
            };
            format!("({arguments} -> {})", type_text(result))
        }
        TypeExprKind::Reference(inner, mutable) => format!(
            "ref{} {}",
            if *mutable { " mut" } else { "" },
            type_text(inner)
        ),
    }
}

fn region_text(regions: &[Ident]) -> String {
    if regions.is_empty() {
        String::new()
    } else {
        format!(
            " {{{}}}",
            regions
                .iter()
                .map(|name| name.text.as_str())
                .collect::<Vec<_>>()
                .join(" ")
        )
    }
}

/// `dyn Shape` or `dyn (Shape, Send)` as written (A14).
pub(crate) fn dyn_text(dyn_type: &DynTypeExpr) -> String {
    let names: Vec<&str> = dyn_type
        .classes
        .iter()
        .map(|name| name.text.as_str())
        .collect();
    match names.as_slice() {
        [name] => format!("dyn {name}"),
        _ => format!("dyn ({})", names.join(", ")),
    }
}

fn parameters_text(parameters: &[Ident]) -> String {
    if parameters.is_empty() {
        String::new()
    } else {
        format!(
            "<{}>",
            parameters
                .iter()
                .map(|name| format!("'{}", name.text))
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
}

fn constraints_text(constraints: &[ConstraintExpr]) -> String {
    let constraints = constraints
        .iter()
        .filter_map(|constraint| {
            if let ConstraintName::Class(name) = &constraint.name {
                Some(format!("{}<{}>", name.text, type_text(&constraint.ty)))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    match constraints.len() {
        0 => String::new(),
        1 => format!("{} => ", constraints[0]),
        _ => format!("({}) => ", constraints.join(", ")),
    }
}

fn signature<'a>(
    prefix: &str,
    name: &str,
    regions: &[Ident],
    constraints: &[ConstraintExpr],
    parameters: impl Iterator<Item = &'a TypeExpr>,
    result: &'a TypeExpr,
) -> String {
    let types = parameters
        .chain([result])
        .map(type_text)
        .collect::<Vec<_>>()
        .join(" -> ");
    let mut text = format!(
        "{prefix} {name}{} :: {}{types}",
        region_text(regions),
        constraints_text(constraints)
    );
    for constraint in constraints {
        if let ConstraintName::Function(name, annotation) = &constraint.name {
            let function = match annotation {
                Some(ty) => format!("(#{}: {})", name.text, type_text(ty)),
                None => format!("#{}", name.text),
            };
            text.push_str(&format!("\n    @{}: {function}", type_text(&constraint.ty)));
        }
    }
    text
}

fn section(name: &str, code: &str, doc: Option<&Documentation>, level: usize) -> String {
    let mut text = format!(
        "{} `{name}`\n\n```tsuzuri\n{code}\n```\n",
        "#".repeat(level)
    );
    if let Some(doc) = doc {
        text.push('\n');
        text.push_str(&doc.text);
        text.push('\n');
    }
    text.push('\n');
    text
}

/// `@json "name" ` before a field or case, with the name escaped as a string literal.
fn json_text(json: Option<&JsonName>) -> String {
    let Some(json) = json else {
        return String::new();
    };
    let mut text = String::from("@json \"");
    for unit in char::decode_utf16(json.units.iter().copied()) {
        match unit {
            Ok('"') => text.push_str("\\\""),
            Ok('\\') => text.push_str("\\\\"),
            Ok(character) if character.is_control() => {
                text.push_str(&format!("\\u{{{:x}}}", u32::from(character)))
            }
            Ok(character) => text.push(character),
            Err(error) => text.push_str(&format!("\\u{{{:x}}}", error.unpaired_surrogate())),
        }
    }
    text.push_str("\" ");
    text
}

fn derives_text(derives: &[(DeriveClass, crate::diagnostic::Span)]) -> String {
    if derives.is_empty() {
        String::new()
    } else {
        format!(
            " deriving ({})",
            derives
                .iter()
                .map(|(class, _)| class.name())
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
}

pub fn render_module(name: &str, program: &Program) -> String {
    let namespace = program
        .namespace
        .as_ref()
        .map(|declared| declared.path.text.as_str());
    module_page(name, namespace, program)
}

fn module_page(name: &str, namespace: Option<&str>, program: &Program) -> String {
    let namespace = namespace
        .map(|namespace| format!("Namespace: `{namespace}`\n\n"))
        .unwrap_or_default();
    let aliases = match program.aliases.as_slice() {
        [] => String::new(),
        aliases => format!(
            "Builder {}: {}\n\n",
            if aliases.len() == 1 {
                "alias"
            } else {
                "aliases"
            },
            aliases
                .iter()
                .map(|alias| format!("`{}`", alias.text))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    };
    format!(
        "# {name}\n\n{namespace}{aliases}{}",
        render_declarations(program)
    )
}

fn render_declarations(program: &Program) -> String {
    let mut declarations = Vec::new();
    for declaration in &program.functions {
        if declaration.visibility == Visibility::Private {
            continue;
        }
        let prefix = format!(
            "{}def{}",
            if declaration.exported { "export " } else { "" },
            if declaration.recursion.is_some() {
                " rec"
            } else {
                ""
            }
        );
        let pattern = program
            .active_patterns
            .iter()
            .find(|pattern| pattern.function == declaration.name.text);
        let name = if let Some(pattern) = pattern {
            let mut cases: Vec<_> = pattern
                .cases
                .iter()
                .map(|case| case.text.as_str())
                .collect();
            if pattern.partial {
                cases.push("_");
            }
            format!("(|{}|)", cases.join("|"))
        } else {
            declaration.name.text.clone()
        };
        let implicit_result = pattern
            .filter(|pattern| pattern.cases.len() > 1)
            .map(|_| TypeExpr {
                kind: TypeExprKind::Variable("T".into()),
                span: declaration.result.span,
            });
        let code = signature(
            &prefix,
            &name,
            &declaration.regions,
            &declaration.constraints,
            declaration.parameters.iter().map(|parameter| &parameter.ty),
            implicit_result.as_ref().unwrap_or(&declaration.result),
        );
        let position = declaration
            .parameters
            .first()
            .map_or(declaration.result.span.start, |parameter| {
                parameter.ty.span.start
            });
        declarations.push((position, section(&name, &code, declaration.doc.as_ref(), 2)));
    }
    for declaration in &program.externs {
        if declaration.visibility == Visibility::Private {
            continue;
        }
        let code = signature(
            "extern def",
            &declaration.name.text,
            &declaration.regions,
            &declaration.constraints,
            declaration.parameters.iter(),
            &declaration.result,
        );
        declarations.push((
            declaration.name.span.start,
            section(&declaration.name.text, &code, declaration.doc.as_ref(), 2),
        ));
    }
    for declaration in &program.extern_types {
        if declaration.visibility == Visibility::Private {
            continue;
        }
        let code = format!("extern type {}", declaration.name.text);
        declarations.push((
            declaration.name.span.start,
            section(&declaration.name.text, &code, declaration.doc.as_ref(), 2),
        ));
    }
    for declaration in &program.records {
        if declaration.visibility == Visibility::Private {
            continue;
        }
        let fields = declaration
            .fields
            .iter()
            .map(|field| {
                format!(
                    "  {}{}: {}",
                    json_text(field.json.as_ref()),
                    field.name.text,
                    type_text(&field.ty)
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        let code = format!(
            "record {}{}{} {{\n{fields}\n}}{}",
            declaration.name.text,
            parameters_text(&declaration.parameters),
            region_text(&declaration.regions),
            derives_text(&declaration.derives)
        );
        declarations.push((
            declaration.name.span.start,
            section(&declaration.name.text, &code, declaration.doc.as_ref(), 2),
        ));
    }
    for declaration in &program.unions {
        if declaration.visibility == Visibility::Private
            || declaration.name.provenance == Provenance::Generated
        {
            continue;
        }
        let cases = declaration
            .cases
            .iter()
            .map(|case| {
                format!(
                    "  | {}{}{}",
                    json_text(case.json.as_ref()),
                    case.name.text,
                    case.payload
                        .as_ref()
                        .map_or(String::new(), |ty| format!(" of {}", type_text(ty)))
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        let code = format!(
            "union {}{} =\n{cases}{}",
            declaration.name.text,
            parameters_text(&declaration.parameters),
            derives_text(&declaration.derives)
        );
        declarations.push((
            declaration.name.span.start,
            section(&declaration.name.text, &code, declaration.doc.as_ref(), 2),
        ));
    }
    for declaration in &program.type_aliases {
        if declaration.visibility == Visibility::Private {
            continue;
        }
        let code = format!(
            "type {}{} = {}",
            declaration.name.text,
            parameters_text(&declaration.parameters),
            type_text(&declaration.target)
        );
        declarations.push((
            declaration.name.span.start,
            section(&declaration.name.text, &code, declaration.doc.as_ref(), 2),
        ));
    }
    for declaration in &program.constants {
        if declaration.visibility == Visibility::Private {
            continue;
        }
        let code = format!(
            "const {}: {}",
            declaration.name.text,
            type_text(&declaration.ty)
        );
        declarations.push((
            declaration.name.span.start,
            section(&declaration.name.text, &code, declaration.doc.as_ref(), 2),
        ));
    }
    for declaration in &program.classes {
        let methods = declaration
            .methods
            .iter()
            .map(|method| {
                signature(
                    "def",
                    &method.name.text,
                    &method.regions,
                    &method.constraints,
                    method.parameters.iter(),
                    &method.result,
                )
            })
            .collect::<Vec<_>>();
        let code = format!(
            "class {}{}<'{}{}> {{\n{}\n}}",
            constraints_text(&declaration.superclasses),
            declaration.name.text,
            declaration.variable.text,
            if declaration.kind == crate::syntax::Kind::Type {
                String::new()
            } else {
                format!(": {}", declaration.kind.display())
            },
            methods
                .iter()
                .map(|method| format!("  {method}"))
                .collect::<Vec<_>>()
                .join("\n")
        );
        let mut text = section(&declaration.name.text, &code, declaration.doc.as_ref(), 2);
        for (method, code) in declaration.methods.iter().zip(methods) {
            if method.doc.is_some() {
                text.push_str(&section(
                    &format!("{}.{}", declaration.name.text, method.name.text),
                    &code,
                    method.doc.as_ref(),
                    3,
                ));
            }
        }
        declarations.push((declaration.name.span.start, text));
    }
    declarations.sort_by_key(|(position, _)| *position);
    let mut text = String::new();
    for (_, declaration) in declarations {
        text.push_str(&declaration);
    }
    text
}
