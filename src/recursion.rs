use super::*;

pub(super) fn check(
    functions: &[CheckedFunction],
    declarations: &[(String, FunctionDecl)],
    classes: &Classes,
    names: &Names,
    types: &TypeContext<'_>,
) -> Result<Vec<Vec<usize>>, Diagnostic> {
    fn references(
        expression: &TypedExpr,
        classes: &Classes,
        names: &Names,
        types: &TypeContext<'_>,
        result: &mut BTreeSet<usize>,
        caller: usize,
    ) -> Result<(), Diagnostic> {
        if let TypedExprKind::Function(FunctionRef::User(id))
        | TypedExprKind::GenericFunction(id, _) = expression.kind
        {
            result.insert(id);
        }
        if let TypedExprKind::TypeFunction {
            receiver,
            name,
            module,
        } = &expression.kind
        {
            if matches!(receiver, Type::Variable(_)) {
                result.extend(
                    names
                        .functions
                        .values()
                        .filter(|info| {
                            info.name.rsplit_once('.').is_some_and(|(owner, member)| {
                                owner == info.module && member == name
                            }) && info.visible_from(module)
                                && names.searchable(module, &info.module)
                                && (types.records.iter().any(|record| {
                                    record
                                        .name
                                        .rsplit_once('.')
                                        .is_some_and(|(owner, _)| owner == info.module)
                                }) || types
                                    .unions
                                    .iter()
                                    .any(|union| union.module() == info.module))
                        })
                        .map(|info| info.id),
                );
            } else {
                result.insert(names.member_function(
                    module,
                    receiver,
                    name,
                    types,
                    expression.span,
                )?);
            }
        }
        result.extend(classes.recursion_targets(expression, types, caller));
        for child in expression.children() {
            references(child, classes, names, types, result, caller)?;
        }
        Ok(())
    }
    let graph: Vec<_> = functions
        .iter()
        .enumerate()
        .map(|(caller, function)| {
            let mut edges = BTreeSet::new();
            references(&function.body, classes, names, types, &mut edges, caller)?;
            Ok(edges.into_iter().collect::<Vec<_>>())
        })
        .collect::<Result<_, Diagnostic>>()?;
    check_graph(functions, &graph, |id| {
        declarations
            .get(id)
            .is_some_and(|(_, declaration)| declaration.recursion.is_none())
    })?;
    Ok(graph)
}

pub(super) fn check_specialized(
    functions: &[CheckedFunction],
    requires_rec: &[bool],
) -> Result<(), Diagnostic> {
    let graph: Vec<_> = functions
        .iter()
        .map(|function| {
            let mut pending = vec![&function.body];
            let mut targets = BTreeSet::new();
            while let Some(expression) = pending.pop() {
                if let TypedExprKind::Function(FunctionRef::User(function)) = expression.kind {
                    targets.insert(function);
                }
                pending.extend(expression.children());
            }
            targets.into_iter().collect()
        })
        .collect();
    check_graph(functions, &graph, |id| requires_rec[id])
}

fn check_graph(
    functions: &[CheckedFunction],
    graph: &[Vec<usize>],
    requires_rec: impl Fn(usize) -> bool,
) -> Result<(), Diagnostic> {
    let mut reverse = vec![Vec::new(); graph.len()];
    for (caller, edges) in graph.iter().enumerate() {
        for &callee in edges {
            reverse[callee].push(caller);
        }
    }
    let mut visited = BTreeSet::new();
    let mut order = Vec::new();
    for root in 0..graph.len() {
        let mut stack = vec![(root, false)];
        while let Some((node, finishing)) = stack.pop() {
            if finishing {
                order.push(node);
            } else if visited.insert(node) {
                stack.push((node, true));
                stack.extend(graph[node].iter().rev().map(|&next| (next, false)));
            }
        }
    }
    visited.clear();
    for root in order.into_iter().rev() {
        if !visited.insert(root) {
            continue;
        }
        let mut component = vec![root];
        let mut next = 0;
        while next < component.len() {
            for &caller in &reverse[component[next]] {
                if visited.insert(caller) {
                    component.push(caller);
                }
            }
            next += 1;
        }
        if component.len() > 1 || graph[root].contains(&root) {
            for id in component {
                if requires_rec(id) {
                    return Err(Diagnostic::new(
                        "E1019",
                        format!(
                            "recursive function '{}' requires 'rec' on its declaration and definition (or 'and' in a recursive group)",
                            functions[id].qualified_name()
                        ),
                        functions[id].span,
                    ));
                }
            }
        }
    }
    Ok(())
}
