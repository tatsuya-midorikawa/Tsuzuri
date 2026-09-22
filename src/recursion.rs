use super::*;

pub(super) fn check(
    functions: &[CheckedFunction],
    declarations: &[(String, FunctionDecl)],
    classes: &Classes,
) -> Result<(), Diagnostic> {
    fn references(expression: &TypedExpr, classes: &Classes, result: &mut BTreeSet<usize>) {
        if let TypedExprKind::Function(FunctionRef::User(id))
        | TypedExprKind::GenericFunction(id, _) = expression.kind
        {
            result.insert(id);
        }
        result.extend(classes.recursion_targets(expression));
        for child in expression.children() {
            references(child, classes, result);
        }
    }
    let graph: Vec<_> = functions
        .iter()
        .map(|function| {
            let mut edges = BTreeSet::new();
            references(&function.body, classes, &mut edges);
            edges.into_iter().collect::<Vec<_>>()
        })
        .collect();
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
                if declarations
                    .get(id)
                    .is_some_and(|(_, declaration)| declaration.recursion.is_none())
                {
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
