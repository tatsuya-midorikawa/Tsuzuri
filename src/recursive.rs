use super::*;

/// The memo of `TypeContext::recursive_checked` for one declaration, by the arguments of its
/// instances. The result for an instance depends only on the instance and the declarations,
/// never on which queries came first, so threads may share it (PB04). The lock is held only to
/// read or insert an entry, never while a type is analyzed.
#[derive(Default)]
pub(super) struct Cache(std::sync::Mutex<Memo>);

type Memo = BTreeMap<Box<[Type]>, Result<bool, Diagnostic>>;

impl Cache {
    fn entries(&self) -> std::sync::MutexGuard<'_, Memo> {
        // A panic while the lock is held cannot leave an entry half written.
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn get(&self, arguments: &[Type]) -> Option<Result<bool, Diagnostic>> {
        self.entries().get(arguments).cloned()
    }

    fn insert(&self, arguments: Box<[Type]>, value: Result<bool, Diagnostic>) {
        self.entries().insert(arguments, value);
    }
}

impl Clone for Cache {
    fn clone(&self) -> Self {
        Self(std::sync::Mutex::new(self.entries().clone()))
    }
}

/// Prints as the `RefCell` that this type replaced, so that the `Debug` dumps of a checked
/// module stay comparable between revisions.
impl std::fmt::Debug for Cache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RefCell")
            .field("value", &*self.entries())
            .finish()
    }
}

/// Whether `ty` is a generic record or union at its own parameters, the type that its
/// declaration is checked as.
fn generic_declaration(ty: &Type, types: TypeContext<'_>) -> bool {
    let (parameters, arguments) = match ty {
        Type::Record(id, arguments) => (&types.records[*id].parameters, arguments),
        Type::Union(id, arguments) => (&types.unions[*id].parameters, arguments),
        _ => return false,
    };
    !parameters.is_empty()
        && parameters.len() == arguments.len()
        && parameters.iter().zip(arguments).all(
            |(parameter, argument)| matches!(argument, Type::Variable(name) if name == parameter),
        )
}

pub(super) fn analyze(
    root: &Type,
    types: TypeContext<'_>,
    span: Span,
) -> Result<BTreeMap<Type, bool>, Diagnostic> {
    struct Graph<'a> {
        types: TypeContext<'a>,
        span: Span,
        nodes: BTreeMap<Type, Vec<Type>>,
        active: Vec<Type>,
        /// The length of `active` where each shared pointer on the current path was entered.
        shared: Vec<usize>,
        /// A generic declaration at its own parameters, which must not reach itself at other
        /// arguments. Instances are not checked again: a type that only contains a recursive
        /// one, such as the `Maybe<Rc<Node>>` that `Rc.upgrade` returns for a `Node` with
        /// `Maybe<Rc.Weak<Node>>` fields, may meet another instance of its declaration.
        generic_root: Option<Type>,
        recursive: BTreeSet<Type>,
    }
    impl Graph<'_> {
        fn visit(&mut self, ty: &Type) -> Result<(), Diagnostic> {
            let (is_union, id, arguments) = match ty {
                Type::Record(id, arguments) => (false, *id, arguments),
                Type::Union(id, arguments) => (true, *id, arguments),
                Type::Array(inner)
                | Type::List(inner)
                | Type::Vec(inner)
                | Type::FixedArray(inner, _) => {
                    return self.visit(inner);
                }
                // A shared block holds its value like a collection holds its elements, and
                // its pointer is a heap indirection like a union node (C10).
                Type::Shared(inner, _) => {
                    self.shared.push(self.active.len());
                    let visited = self.visit(inner);
                    self.shared.pop();
                    return visited;
                }
                Type::Tuple(elements) => {
                    for element in elements {
                        self.visit(element)?;
                    }
                    return Ok(());
                }
                _ => return Ok(()),
            };
            if let Some(start) = self.active.iter().position(|active| active == ty) {
                let cycle = &self.active[start..];
                if !cycle.iter().any(|ty| matches!(ty, Type::Union(..)))
                    && !self.shared.iter().any(|entered| *entered > start)
                {
                    return Err(Diagnostic::of(
                        DiagnosticKind::InfiniteRecursiveLayout,
                        "recursive value layout must pass through a union with a finite alternative",
                        self.span,
                    ));
                }
                self.recursive.extend(cycle.iter().cloned());
                return Ok(());
            }
            if let Some(root) = &self.generic_root
                && root != ty
                && same_declaration(root, ty)
            {
                return Err(changes_arguments(self.span));
            }
            if self.nodes.contains_key(ty) {
                return Ok(());
            }
            if self.nodes.len() >= 4096 || self.active.len() >= MAX_NESTING {
                // An expansion that keeps instantiating one declaration with new arguments
                // never ends; the cycles of a finite expansion are checked after the search.
                let growing = self
                    .active
                    .iter()
                    .any(|active| active != ty && same_declaration(active, ty));
                return Err(if growing {
                    changes_arguments(self.span)
                } else {
                    Diagnostic::of(
                        DiagnosticKind::RecursiveTypeExpansionLimit,
                        "recursive type expansion exceeds the compiler limit",
                        self.span,
                    )
                });
            }
            let fields = if is_union {
                self.types
                    .union_payloads(id, arguments)
                    .into_iter()
                    .flatten()
                    .collect()
            } else {
                self.types.record_fields(id, arguments)
            };
            self.nodes.insert(ty.clone(), fields);
            self.active.push(ty.clone());
            for field in self.nodes[ty].clone() {
                polymorph::bounded_type(&field, self.span)?;
                self.visit(&field)?;
            }
            self.active.pop();
            Ok(())
        }
    }
    fn finite(ty: &Type, known: &BTreeSet<Type>) -> bool {
        match ty {
            Type::Record(..) | Type::Union(..) => known.contains(ty),
            Type::Tuple(elements) => elements.iter().all(|element| finite(element, known)),
            // Elements are inline (A16), unlike the heap elements of `[T]`. A shared block always
            // holds a value (C10).
            Type::FixedArray(element, _) | Type::Shared(element, _) => finite(element, known),
            _ => true,
        }
    }
    /// Whether two record or union types instantiate the same declaration.
    fn same_declaration(left: &Type, right: &Type) -> bool {
        match (left, right) {
            (Type::Record(left, _), Type::Record(right, _))
            | (Type::Union(left, _), Type::Union(right, _)) => left == right,
            _ => false,
        }
    }
    fn changes_arguments(span: Span) -> Diagnostic {
        Diagnostic::of(
            DiagnosticKind::RecursiveTypeChangesArguments,
            "recursive generic type changes its arguments; use a non-growing recursive occurrence",
            span,
        )
    }
    let generic_root = generic_declaration(root, types).then(|| root.clone());
    let mut graph = Graph {
        types,
        span,
        nodes: BTreeMap::new(),
        active: Vec::new(),
        shared: Vec::new(),
        generic_root,
        recursive: BTreeSet::new(),
    };
    graph.visit(root)?;
    let ordered: Vec<_> = graph.nodes.keys().cloned().collect();
    let indices: BTreeMap<_, _> = ordered
        .iter()
        .cloned()
        .enumerate()
        .map(|(index, ty)| (ty, index))
        .collect();
    let mut edges = vec![Vec::new(); ordered.len()];
    let mut reverse = edges.clone();
    for (index, ty) in ordered.iter().enumerate() {
        let mut pending = graph.nodes[ty].clone();
        while let Some(child) = pending.pop() {
            match child {
                Type::Record(..) | Type::Union(..) => {
                    let target = indices[&child];
                    edges[index].push(target);
                    reverse[target].push(index);
                }
                Type::Array(inner)
                | Type::List(inner)
                | Type::Vec(inner)
                | Type::FixedArray(inner, _)
                | Type::Shared(inner, _) => pending.push(*inner),
                Type::Tuple(elements) => pending.extend(elements),
                _ => {}
            }
        }
    }
    let mut visited = BTreeSet::new();
    let mut order = Vec::new();
    for root in 0..ordered.len() {
        let mut pending = vec![(root, false)];
        while let Some((index, finished)) = pending.pop() {
            if finished {
                order.push(index);
            } else if visited.insert(index) {
                pending.push((index, true));
                pending.extend(edges[index].iter().map(|next| (*next, false)));
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
            for target in &reverse[component[next]] {
                if visited.insert(*target) {
                    component.push(*target);
                }
            }
            next += 1;
        }
        if component.len() > 1 || edges[root].contains(&root) {
            graph
                .recursive
                .extend(component.into_iter().map(|index| ordered[index].clone()));
        }
    }
    let mut inhabited = BTreeSet::new();
    loop {
        let before = inhabited.len();
        for (ty, fields) in &graph.nodes {
            let valid = match ty {
                Type::Union(id, arguments) => {
                    types.union_payloads(*id, arguments).iter().any(|payload| {
                        payload
                            .as_ref()
                            .is_none_or(|payload| finite(payload, &inhabited))
                    })
                }
                _ => fields.iter().all(|field| finite(field, &inhabited)),
            };
            if valid {
                inhabited.insert(ty.clone());
            }
        }
        if inhabited.len() == before {
            break;
        }
    }
    if graph.recursive.iter().any(|ty| !inhabited.contains(ty)) {
        return Err(Diagnostic::of(
            DiagnosticKind::RecursiveUnionWithoutBaseCase,
            "recursive union has no finite value; add a base case or an empty collection alternative",
            span,
        ));
    }
    Ok(graph
        .nodes
        .into_keys()
        .map(|ty| {
            let recursive = graph.recursive.contains(&ty);
            (ty, recursive)
        })
        .collect())
}

impl TypeContext<'_> {
    pub(crate) fn recursive(&self, ty: &Type) -> bool {
        self.recursive_checked(ty, Span::default()).unwrap_or(true)
    }

    pub(super) fn recursive_checked(&self, ty: &Type, span: Span) -> Result<bool, Diagnostic> {
        let (cache, arguments) = match ty {
            Type::Record(id, arguments) => (&self.records[*id].recursive, arguments),
            Type::Union(id, arguments) => (&self.unions[*id].recursive, arguments),
            _ => return Ok(false),
        };
        if let Some(value) = cache.get(arguments) {
            return value.map_err(|mut error| {
                error.span = span;
                error
            });
        }
        match analyze(ty, *self, span) {
            Ok(nodes) => {
                let result = nodes[ty];
                for (node, recursive) in nodes {
                    // Another declaration may reach a generic declaration at its own parameters
                    // first; only the analysis rooted there checks it for changed arguments.
                    if node != *ty && generic_declaration(&node, *self) {
                        continue;
                    }
                    match node {
                        Type::Record(id, arguments) => {
                            self.records[id].recursive.insert(arguments, Ok(recursive));
                        }
                        Type::Union(id, arguments) => {
                            self.unions[id].recursive.insert(arguments, Ok(recursive));
                        }
                        _ => unreachable!(),
                    }
                }
                Ok(result)
            }
            Err(error) => {
                cache.insert(arguments.clone(), Err(error.clone()));
                Err(error)
            }
        }
    }

    pub(super) fn stored_all(&self, root: &Type, predicate: impl Fn(&Type) -> bool) -> bool {
        self.stored_all_closed(root, predicate, |_| false)
    }

    /// `stored_all`, except that it does not look inside a type for which `closed` holds: the
    /// predicate has already judged what that type holds (F10's `Sync` of a `Mutex`).
    pub(super) fn stored_all_closed(
        &self,
        root: &Type,
        predicate: impl Fn(&Type) -> bool,
        closed: impl Fn(&Type) -> bool,
    ) -> bool {
        self.walk_types(root, predicate, closed, false)
    }

    /// Whether `found` holds for a type reachable from `root` through stored values and the
    /// targets of references (A13). Function and task types describe calls, not stored values.
    pub(crate) fn reaches(&self, root: &Type, found: impl Fn(&Type) -> bool) -> bool {
        !self.walk_types(root, |ty| !found(ty), |_| false, true)
    }

    pub(crate) fn reaches_closed(
        &self,
        root: &Type,
        found: impl Fn(&Type) -> bool,
        closed: impl Fn(&Type) -> bool,
    ) -> bool {
        !self.walk_types(root, |ty| !found(ty), closed, true)
    }

    fn walk_types(
        &self,
        root: &Type,
        predicate: impl Fn(&Type) -> bool,
        closed: impl Fn(&Type) -> bool,
        references: bool,
    ) -> bool {
        if !predicate(root) {
            return false;
        }
        if closed(root)
            || !matches!(
                root,
                Type::Record(..)
                    | Type::Union(..)
                    | Type::Array(_)
                    | Type::List(_)
                    | Type::Vec(_)
                    | Type::FixedArray(..)
                    | Type::Shared(..)
                    | Type::Tuple(_)
            ) && !(references && matches!(root, Type::Reference(..)))
        {
            return true;
        }
        let mut arena = type_arena::TypeArena::default();
        let root_id = arena.borrowed(root);
        arena.visit(root_id);
        let mut pending = Vec::new();
        arena.children(root_id, *self, references, &mut pending);
        while let Some(id) = pending.pop() {
            let ty = arena.ty(id);
            if !predicate(ty) {
                return false;
            }
            if closed(ty) || !arena.visit(id) {
                continue;
            }
            arena.children(id, *self, references, &mut pending);
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reference_walk(
        types: TypeContext<'_>,
        root: &Type,
        references: bool,
        closed: bool,
    ) -> (bool, Vec<Type>) {
        let mut seen = BTreeSet::new();
        let mut pending = vec![root.clone()];
        let mut trace = Vec::new();
        while let Some(ty) = pending.pop() {
            trace.push(ty.clone());
            if matches!(ty, Type::String) {
                return (false, trace);
            }
            if closed && matches!(ty, Type::Array(_)) || !seen.insert(ty.clone()) {
                continue;
            }
            match ty {
                Type::Record(id, arguments) => pending.extend(types.record_fields(id, &arguments)),
                Type::Union(id, arguments) => {
                    pending.extend(types.union_payloads(id, &arguments).into_iter().flatten())
                }
                Type::Reference(inner, _) if references => pending.push(*inner),
                Type::Array(inner)
                | Type::List(inner)
                | Type::Vec(inner)
                | Type::FixedArray(inner, _)
                | Type::Shared(inner, _) => pending.push(*inner),
                Type::Tuple(elements) => pending.extend(elements),
                _ => {}
            }
        }
        (true, trace)
    }

    #[test]
    fn type_arena_preserves_predicate_order_cycles_and_closed_boundaries() {
        let module = crate::analyze(
            "record Plain { values: [[i64]], flag: bool }\n\
             record Generic<'a> { values: [['a]] }\n\
             union Tree = Empty | Branch of [Tree]\n\
             def plain :: Plain -> unit = \\_x -> ()\n\
             def generic :: Generic<i64> -> unit = \\_x -> ()\n\
             def tree :: Tree -> unit = \\_x -> ()\n\
             def reference :: ref Plain -> unit = \\_x -> ()\n\
             def string :: (i64 * string) -> unit = \\_x -> ()",
        )
        .unwrap();
        let types = module.types();
        for function in &module.functions {
            let root = &function.parameters[0].ty;
            for references in [false, true] {
                for closed in [false, true] {
                    let expected = reference_walk(types, root, references, closed);
                    let trace = std::cell::RefCell::new(Vec::new());
                    let actual = types.walk_types(
                        root,
                        |ty| {
                            trace.borrow_mut().push(ty.clone());
                            !matches!(ty, Type::String)
                        },
                        |ty| closed && matches!(ty, Type::Array(_)),
                        references,
                    );
                    assert_eq!((actual, trace.into_inner()), expected, "{}", function.name);
                }
            }
        }
    }

    #[test]
    fn recursive_components_include_cross_edges() {
        let module = crate::analyze(
            "union A = End | ToB of B | ToC of C\nunion B = ToA of A\nunion C = BackB of B",
        )
        .unwrap();
        for (id, union) in module
            .unions
            .iter()
            .enumerate()
            .filter(|(_, union)| union.origin == ModuleOrigin::User)
        {
            assert!(
                module.types().recursive(&Type::Union(id, Box::default())),
                "{}",
                union.name
            );
        }
    }

    #[test]
    fn cache_prints_as_the_refcell_it_replaced_and_clones_its_entries() {
        let cache = Cache::default();
        let reference =
            std::cell::RefCell::new(BTreeMap::<Box<[Type]>, Result<bool, Diagnostic>>::new());
        assert_eq!(format!("{cache:?}"), format!("{reference:?}"));
        let key: Box<[Type]> = vec![Type::Integer(64, true), Type::Bool].into();
        let error = Diagnostic::of(
            DiagnosticKind::InfiniteRecursiveLayout,
            "recursive",
            Span::new(1, 2),
        );
        cache.insert(key.clone(), Ok(true));
        cache.insert(Box::default(), Err(error.clone()));
        reference.borrow_mut().insert(key.clone(), Ok(true));
        reference.borrow_mut().insert(Box::default(), Err(error));
        assert_eq!(format!("{cache:?}"), format!("{reference:?}"));
        assert_eq!(cache.get(&key), Some(Ok(true)));
        assert_eq!(cache.get(&[Type::Bool]), None);
        let copy = cache.clone();
        copy.insert(vec![Type::Bool].into(), Ok(false));
        assert_eq!(
            cache.get(&[Type::Bool]),
            None,
            "a clone does not share entries"
        );
        assert_eq!(copy.get(&key), Some(Ok(true)));
    }

    const MEMO_SOURCE: &str = "record Plain { values: [[i64]], flag: bool }\n\
        record Pair<'a, 'b> { left: 'a, right: 'b }\n\
        union Tree = Empty | Branch of [Tree]\n\
        union Chain<'a> = End | Link of 'a * Chain<'a>\n\
        union A = Stop | ToB of B | ToC of C\n\
        union B = ToA of A\n\
        union C = BackB of B\n\
        def plain :: Plain -> unit = \\_x -> ()\n\
        def pair :: Pair<i64, Tree> -> unit = \\_x -> ()\n\
        def chain :: Chain<i64> -> unit = \\_x -> ()\n\
        def nested :: Chain<Chain<bool>> -> unit = \\_x -> ()\n\
        def mixed :: Pair<Chain<i64>, A> -> unit = \\_x -> ()";

    /// The same declarations with empty memos.
    fn fresh(module: &CheckedModule) -> (Vec<CheckedRecord>, Vec<CheckedUnion>) {
        let mut records = module.records.clone();
        let mut unions = module.unions.clone();
        records
            .iter_mut()
            .for_each(|record| record.recursive = Cache::default());
        unions
            .iter_mut()
            .for_each(|union| union.recursive = Cache::default());
        (records, unions)
    }

    fn shuffled(queries: &[Type], seed: u64) -> Vec<Type> {
        let mut order = queries.to_vec();
        let mut state = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        for index in (1..order.len()).rev() {
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            order.swap(index, (state >> 33) as usize % (index + 1));
        }
        order
    }

    #[test]
    fn recursive_memo_does_not_depend_on_the_order_or_thread_of_queries() {
        let module = crate::analyze(MEMO_SOURCE).unwrap();
        let mut queries = BTreeSet::new();
        for (id, record) in module.records.iter().enumerate() {
            if record.origin == ModuleOrigin::User {
                let own = record
                    .parameters
                    .iter()
                    .cloned()
                    .map(Type::Variable)
                    .collect();
                queries.insert(Type::Record(id, own));
            }
        }
        for (id, union) in module.unions.iter().enumerate() {
            if union.origin == ModuleOrigin::User {
                let own = union
                    .parameters
                    .iter()
                    .cloned()
                    .map(Type::Variable)
                    .collect();
                queries.insert(Type::Union(id, own));
            }
        }
        for function in module
            .functions
            .iter()
            .filter(|function| function.origin.module == ModuleOrigin::User)
        {
            queries.insert(function.parameters[0].ty.clone());
        }
        let queries: Vec<_> = queries.into_iter().collect();
        let memos = |records: &[CheckedRecord], unions: &[CheckedUnion]| {
            let records: Vec<_> = records
                .iter()
                .map(|record| format!("{:?}", record.recursive))
                .collect();
            let unions: Vec<_> = unions
                .iter()
                .map(|union| format!("{:?}", union.recursive))
                .collect();
            (records, unions)
        };
        let run = |records: &[CheckedRecord], unions: &[CheckedUnion], order: &[Type]| {
            let types = TypeContext { records, unions };
            let mut answers: Vec<_> = order
                .iter()
                .map(|ty| (ty.clone(), types.recursive_checked(ty, Span::default())))
                .collect();
            answers.sort_by(|left, right| left.0.cmp(&right.0));
            answers
        };
        let (records, unions) = fresh(&module);
        let expected = run(&records, &unions, &queries);
        let expected_memos = memos(&records, &unions);
        // The recursion of each root, derived by hand from the declarations.
        let flags: BTreeMap<_, _> = expected
            .iter()
            .map(|(ty, answer)| (ty.display(&module.types()), answer.clone().unwrap()))
            .collect();
        for (name, recursive) in [
            ("Main.Plain", false),
            ("Main.Pair<i64, Main.Tree>", false),
            ("Main.Chain<i64>", true),
            ("Main.Chain<Main.Chain<bool>>", true),
            ("Main.Pair<Main.Chain<i64>, Main.A>", false),
            ("Main.Tree", true),
            ("Main.A", true),
            ("Main.B", true),
            ("Main.C", true),
        ] {
            assert_eq!(flags.get(name), Some(&recursive), "{name} in {flags:?}");
        }
        for seed in 1..=12 {
            let (records, unions) = fresh(&module);
            let order = shuffled(&queries, seed);
            assert_eq!(run(&records, &unions, &order), expected, "order {seed}");
            assert_eq!(
                memos(&records, &unions),
                expected_memos,
                "memo after order {seed}"
            );
        }
        for round in 0..4 {
            let (records, unions) = fresh(&module);
            let types = TypeContext {
                records: &records,
                unions: &unions,
            };
            let answers: Vec<_> = std::thread::scope(|scope| {
                let workers: Vec<_> = (0..4)
                    .map(|worker| {
                        let order = shuffled(&queries, 100 * round + worker);
                        scope.spawn(move || {
                            let mut answers: Vec<_> = order
                                .iter()
                                .map(|ty| {
                                    (ty.clone(), types.recursive_checked(ty, Span::default()))
                                })
                                .collect();
                            answers.sort_by(|left, right| left.0.cmp(&right.0));
                            answers
                        })
                    })
                    .collect();
                workers
                    .into_iter()
                    .map(|worker| worker.join().unwrap())
                    .collect()
            });
            for answer in answers {
                assert_eq!(answer, expected, "thread round {round}");
            }
            assert_eq!(
                memos(&records, &unions),
                expected_memos,
                "memo after thread round {round}"
            );
        }
    }
}
