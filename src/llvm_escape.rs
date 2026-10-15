use super::*;

pub(super) fn staying_locals(body: &TypedExpr) -> BTreeSet<usize> {
    let mut candidates = BTreeSet::new();
    let mut escaping = BTreeSet::new();
    let mut pending = vec![(body, false)];
    while let Some((expression, inspected)) = pending.pop() {
        if let TypedExprKind::Local(id) = expression.kind {
            if !inspected {
                escaping.insert(id);
            }
            continue;
        }
        if let TypedExprKind::Block { bindings, .. } = &expression.kind {
            candidates.extend(
                bindings
                    .iter()
                    .filter(|(local, _)| !local.mutable)
                    .map(|(local, _)| local.id),
            );
        }
        match &expression.kind {
            TypedExprKind::Length(value)
            | TypedExprKind::StringLength(value)
            | TypedExprKind::Borrow(value, false) => pending.push((value, true)),
            TypedExprKind::Index(value, index) => {
                pending.push((value, true));
                pending.push((index, false));
            }
            TypedExprKind::ForEach { source, body, .. } => {
                pending.push((source, true));
                pending.push((body, false));
            }
            TypedExprKind::Binary(_, left, right) if left.ty.is_string() => {
                pending.push((left, true));
                pending.push((right, true));
            }
            TypedExprKind::Call(callee, arguments) => {
                pending.push((callee, true));
                pending.extend(arguments.iter().map(|argument| (argument, false)));
            }
            _ => pending.extend(
                expression
                    .children()
                    .into_iter()
                    .map(|child| (child, false)),
            ),
        }
    }
    candidates.retain(|id| !escaping.contains(id));
    candidates
}

/// Marks cycles, including indirect recursion through lifted closures, without using the Rust stack.
pub(super) fn recursive_functions(module: &CheckedModule) -> Vec<bool> {
    let graph: Vec<Vec<usize>> = module
        .functions
        .iter()
        .map(|function| {
            let mut pending = vec![&function.body];
            let mut targets = BTreeSet::new();
            while let Some(expression) = pending.pop() {
                match expression.kind {
                    TypedExprKind::Function(FunctionRef::User(id))
                    | TypedExprKind::Closure(id, _) => {
                        targets.insert(id);
                    }
                    _ => {}
                }
                pending.extend(expression.children());
            }
            targets.into_iter().collect()
        })
        .collect();
    let mut reverse = vec![Vec::new(); graph.len()];
    for (caller, edges) in graph.iter().enumerate() {
        for &callee in edges {
            reverse[callee].push(caller);
        }
    }
    let mut seen = vec![false; graph.len()];
    let mut order = Vec::new();
    for root in 0..graph.len() {
        let mut pending = vec![(root, false)];
        while let Some((id, done)) = pending.pop() {
            if done {
                order.push(id);
            } else if !std::mem::replace(&mut seen[id], true) {
                pending.push((id, true));
                pending.extend(graph[id].iter().rev().map(|&next| (next, false)));
            }
        }
    }
    seen.fill(false);
    let mut recursive = vec![false; graph.len()];
    for root in order.into_iter().rev() {
        if std::mem::replace(&mut seen[root], true) {
            continue;
        }
        let mut component = vec![root];
        let mut cursor = 0;
        while cursor < component.len() {
            for &caller in &reverse[component[cursor]] {
                if !std::mem::replace(&mut seen[caller], true) {
                    component.push(caller);
                }
            }
            cursor += 1;
        }
        if component.len() > 1 || graph[root].contains(&root) {
            for id in component {
                recursive[id] = true;
            }
        }
    }
    recursive
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn staying_locals_accept_reads_and_reject_moves() {
        let module = crate::analyze(
            "def use :: string -> i64\nfn use text = text.length
             def f :: i64 -> i64
             fn f n = {
                 let kept = if n > 0 then \"ab\" else \"abc\";
                 let moved = if n > 0 then \"ab\" else \"abc\";
                 kept.length + use moved
             }",
        )
        .unwrap();
        let function = module
            .functions
            .iter()
            .find(|function| function.name == "f")
            .unwrap();
        let TypedExprKind::Block { bindings, .. } = &function.body.kind else {
            panic!()
        };
        let staying = staying_locals(&function.body);
        assert!(staying.contains(&bindings[0].0.id));
        assert!(!staying.contains(&bindings[1].0.id));
    }

    #[test]
    fn recursive_functions_mark_cycles_through_lambdas() {
        let module = crate::analyze(
            "def rec f :: i64 -> i64
             fn rec f n = {
                 let next = \\i -> f (i - 1);
                 if n == 0 then 0 else next n
             }",
        )
        .unwrap();
        let recursive = recursive_functions(&module);
        let id = module
            .functions
            .iter()
            .position(|function| function.name == "f")
            .unwrap();
        assert!(recursive[id]);
        assert!(recursive.iter().filter(|&&recursive| recursive).count() >= 2);
    }
}
