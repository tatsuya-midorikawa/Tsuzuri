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

/// The functions that each function names, in id order: by a call or a function value, and, if
/// `closures` is true, by a closure too.
fn edges(module: &CheckedModule, closures: bool) -> Vec<Vec<usize>> {
    module
        .functions
        .iter()
        .map(|function| {
            let mut pending = vec![&function.body];
            let mut targets = BTreeSet::new();
            while let Some(expression) = pending.pop() {
                match expression.kind {
                    TypedExprKind::Function(FunctionRef::User(id)) => {
                        targets.insert(id);
                    }
                    TypedExprKind::Closure(id, _) if closures => {
                        targets.insert(id);
                    }
                    _ => {}
                }
                pending.extend(expression.children());
            }
            targets.into_iter().collect()
        })
        .collect()
}

/// The functions that each function names, by a call or a closure, in id order.
pub(super) fn references(module: &CheckedModule) -> Vec<Vec<usize>> {
    edges(module, true)
}

/// The strongly connected components of `graph` without using the Rust stack: the smallest node of
/// the component of each node, which does not depend on the order of the search.
fn components(graph: &[Vec<usize>]) -> Vec<usize> {
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
    let mut head = vec![0; graph.len()];
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
        let first = component.iter().copied().min().unwrap_or(root);
        for id in component {
            head[id] = first;
        }
    }
    head
}

/// Marks cycles, including indirect recursion through lifted closures, without using the Rust stack.
pub(super) fn recursive_functions(module: &CheckedModule) -> Vec<bool> {
    let graph = references(module);
    let component = components(&graph);
    let mut size = vec![0usize; graph.len()];
    for &head in &component {
        size[head] += 1;
    }
    (0..graph.len())
        .map(|id| size[component[id]] > 1 || graph[id].contains(&id))
        .collect()
}

/// The group of each function for guaranteed tail calls (PM09): the functions that name one another
/// by a call or a function value, which is the cycle of a mutual recursion. A closure is not a
/// name here. Two functions are in a group when they have the same number, the smallest id in it.
pub(super) fn tail_groups(module: &CheckedModule) -> Vec<usize> {
    components(&edges(module, false))
}

/// Whether a call of a function that returns `ty` can be a guaranteed tail call (PM09 D1): the
/// language type is a `bool`, a character, `unit`, or a number of at most 64 bits (an integer, an
/// `f16`, `f32`, `f64`, a `d32` or a `d64`). The type decides and not its representation, so that a
/// user can tell from the type of a function whether its calls are guaranteed: a union is not, be it
/// a tag, a pointer (a recursive union) or its payload itself (a niche), and neither is an `Rc`, an
/// `Arc`, a handle, a function value, a vector, or a value that a target may return in memory.
pub(super) fn guaranteed_result(ty: &Type) -> bool {
    match ty {
        Type::Bool | Type::Unit | Type::Char | Type::Utf8Char => true,
        Type::Integer(bits, _) | Type::Binary(bits) | Type::Decimal(bits) => *bits <= 64,
        _ => false,
    }
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

    #[test]
    fn tail_groups_are_the_cycles_of_names_and_not_of_closures() {
        let module = crate::analyze(
            "def rec even :: i64 -> bool
             def and odd :: i64 -> bool
             fn rec even n = if n == 0 then true else odd (n - 1)
             and odd n = if n == 0 then false else even (n - 1)
             def asks :: i64 -> bool
             fn asks n = even n
             def rec f :: i64 -> i64
             fn rec f n = {
                 let next = \\i -> f (i - 1);
                 if n == 0 then 0 else next n
             }",
        )
        .unwrap();
        let groups = tail_groups(&module);
        let id = |name: &str| {
            module
                .functions
                .iter()
                .position(|function| function.name == name)
                .unwrap()
        };
        // The number of a group is the smallest id in it, so it does not depend on the search.
        assert_eq!(groups[id("even")], id("even").min(id("odd")));
        assert_eq!(groups[id("odd")], groups[id("even")]);
        assert_eq!(groups[id("asks")], id("asks"));
        // `f` names itself only through the lambda that it builds: a closure is not a name, so the
        // lambda that calls `f` is no member of its group, though both are recursive.
        let lambda = module
            .functions
            .iter()
            .position(|function| function.module == "$lambda")
            .unwrap();
        assert_eq!(groups[id("f")], id("f"));
        assert_eq!(groups[lambda], lambda);
        assert!(recursive_functions(&module)[lambda]);
        assert_eq!(groups, tail_groups(&module));
    }

    #[test]
    fn guaranteed_results_are_named_by_the_language_type() {
        for ty in [
            Type::Bool,
            Type::Unit,
            Type::Char,
            Type::Utf8Char,
            Type::Integer(8, true),
            Type::Integer(64, false),
            Type::Binary(16),
            Type::Binary(32),
            Type::Binary(64),
            Type::Decimal(32),
            Type::Decimal(64),
        ] {
            assert!(guaranteed_result(&ty), "{ty:?}");
        }
        for ty in [
            Type::Integer(128, true),
            Type::Binary(128),
            Type::Decimal(128),
            Type::String,
            Type::Utf8String,
            Type::Array(Box::new(Type::Bool)),
            Type::List(Box::new(Type::Bool)),
            Type::Tuple(vec![Type::Bool, Type::Bool]),
            Type::Handle("Host.Handle".into()),
            Type::Shared(Box::new(Type::Bool), SharedKind::Arc),
            Type::function(vec![Type::Bool], Type::Bool),
        ] {
            assert!(!guaranteed_result(&ty), "{ty:?}");
        }
    }
}
