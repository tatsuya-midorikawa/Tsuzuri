use super::*;

fn failure(span: Span, message: impl Into<String>) -> Diagnostic {
    Diagnostic::new("E1013", message, span)
}

fn children(expression: &TypeExpr) -> Vec<&TypeExpr> {
    match &expression.kind {
        TypeExprKind::Apply(_, arguments) => arguments.iter().collect(),
        TypeExprKind::Regions(inner, _)
        | TypeExprKind::Reference(inner, _)
        | TypeExprKind::Array(inner)
        | TypeExprKind::List(inner)
        | TypeExprKind::Task(inner) => vec![inner],
        TypeExprKind::Tuple(elements) => elements.iter().collect(),
        TypeExprKind::Function(parameters, result) => parameters
            .iter()
            .chain(std::iter::once(result.as_ref()))
            .collect(),
        _ => Vec::new(),
    }
}

fn has_regions(expression: &TypeExpr) -> bool {
    let mut pending = vec![expression];
    while let Some(expression) = pending.pop() {
        if matches!(expression.kind, TypeExprKind::Regions(..)) {
            return true;
        }
        pending.extend(children(expression));
    }
    false
}

pub(super) fn reject_local(expression: &TypeExpr) -> Result<(), Diagnostic> {
    if has_regions(expression) {
        Err(failure(
            expression.span,
            "named regions belong in function signatures or record declarations; let local borrows be inferred",
        ))
    } else {
        Ok(())
    }
}

fn labels(
    expression: &TypeExpr,
    declared: &BTreeSet<String>,
    module: &str,
    names: &Names,
    types: TypeContext<'_>,
) -> Result<BTreeSet<String>, Diagnostic> {
    let mut labels = BTreeSet::new();
    let mut pending = vec![expression];
    while let Some(expression) = pending.pop() {
        if matches!(expression.kind, TypeExprKind::Function(..)) && has_regions(expression) {
            return Err(failure(
                expression.span,
                "named regions inside higher-order function types are not supported; use a named function with all parameters bound",
            ));
        }
        if let TypeExprKind::Regions(inner, regions) = &expression.kind {
            if regions.len() != 1 {
                return Err(failure(
                    expression.span,
                    "each borrowed value has one named region; split values with independent regions",
                ));
            }
            let ty = resolve_type(inner, module, names)?;
            if !ty.carries_loans(&types) || matches!(ty, Type::Function(..) | Type::Task(_)) {
                return Err(failure(
                    expression.span,
                    "a named region requires a reference or an aggregate carrying borrows",
                ));
            }
            for region in regions {
                if !declared.contains(&region.text) {
                    return Err(failure(
                        region.span,
                        format!(
                            "undeclared region '{}'; add it after the declaration name",
                            region.text
                        ),
                    ));
                }
                labels.insert(region.text.clone());
            }
        }
        pending.extend(children(expression));
    }
    if labels.len() > 1 {
        return Err(failure(
            expression.span,
            "one parameter or result cannot mix distinct named regions; use separate parameters",
        ));
    }
    Ok(labels)
}

fn check_used(regions: &[Ident], used: &BTreeSet<String>) -> Result<(), Diagnostic> {
    if let Some(region) = regions.iter().find(|region| !used.contains(&region.text)) {
        return Err(failure(
            region.span,
            format!(
                "unused region '{}'; remove it or use it in a borrowed type",
                region.text
            ),
        ));
    }
    Ok(())
}

pub(super) fn validate_modules(
    modules: &[ModuleInput<'_>],
    names: &Names,
    types: TypeContext<'_>,
) -> Result<(), Diagnostic> {
    for module in modules {
        for record in &module.program.records {
            if record.regions.len() > 1 {
                return Err(failure(
                    record.name.span,
                    "records currently have one shared region; use separate views for independent regions",
                ));
            }
            let declared = record
                .regions
                .iter()
                .map(|region| region.text.clone())
                .collect();
            let mut used = BTreeSet::new();
            for field in &record.fields {
                used.extend(labels(&field.ty, &declared, module.name, names, types)?);
            }
            check_used(&record.regions, &used)?;
        }
        for alias in &module.program.type_aliases {
            reject_local(&alias.target)?;
        }
        for constant in &module.program.constants {
            reject_local(&constant.ty)?;
        }
        for union in &module.program.unions {
            for case in &union.cases {
                if let Some(payload) = &case.payload {
                    reject_local(payload)?;
                }
            }
        }
        for class in &module.program.classes {
            for method in &class.methods {
                for parameter in &method.parameters {
                    reject_local(parameter)?;
                }
                reject_local(&method.result)?;
            }
        }
    }
    Ok(())
}

pub(super) fn contract(
    function: &FunctionDecl,
    module: &str,
    names: &Names,
    types: TypeContext<'_>,
) -> Result<Option<BTreeSet<usize>>, Diagnostic> {
    let declared: BTreeSet<_> = function
        .regions
        .iter()
        .map(|region| region.text.clone())
        .collect();
    let mut used = BTreeSet::new();
    let mut inputs = Vec::new();
    for parameter in &function.parameters {
        let input = labels(&parameter.ty, &declared, module, names, types)?;
        used.extend(input.iter().cloned());
        inputs.push(input);
    }
    let result = labels(&function.result, &declared, module, names, types)?;
    used.extend(result.iter().cloned());
    check_used(&function.regions, &used)?;
    let Some(region) = result.first() else {
        return Ok(None);
    };
    if !matches!(function.result.kind, TypeExprKind::Regions(..)) {
        return Err(failure(
            function.result.span,
            "annotate the whole borrowed result with one region, for example 'View {r}' or 'ref {r} T'",
        ));
    }
    let sources: BTreeSet<_> = inputs
        .iter()
        .enumerate()
        .filter_map(|(index, input)| input.contains(region).then_some(index))
        .collect();
    if sources.is_empty() {
        return Err(failure(
            function.result.span,
            format!(
                "result region '{region}' has no matching input; bind all function parameters and name an input region"
            ),
        ));
    }
    Ok(Some(sources))
}
