use super::*;

pub(super) type Cache = std::cell::RefCell<BTreeMap<Box<[Type]>, Result<bool, Diagnostic>>>;

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
                    return Err(Diagnostic::new(
                        "E1010",
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
                    Diagnostic::new(
                        "E1017",
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
        Diagnostic::new(
            "E1017",
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
        return Err(Diagnostic::new(
            "E1010",
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
        if let Some(value) = cache.borrow().get(arguments) {
            return value.clone().map_err(|mut error| {
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
                            self.records[id]
                                .recursive
                                .borrow_mut()
                                .insert(arguments, Ok(recursive));
                        }
                        Type::Union(id, arguments) => {
                            self.unions[id]
                                .recursive
                                .borrow_mut()
                                .insert(arguments, Ok(recursive));
                        }
                        _ => unreachable!(),
                    }
                }
                Ok(result)
            }
            Err(error) => {
                cache
                    .borrow_mut()
                    .insert(arguments.clone(), Err(error.clone()));
                Err(error)
            }
        }
    }

    pub(super) fn stored_all(&self, root: &Type, predicate: impl Fn(&Type) -> bool) -> bool {
        let mut seen = BTreeSet::new();
        let mut pending = vec![root.clone()];
        while let Some(ty) = pending.pop() {
            if !predicate(&ty) {
                return false;
            }
            if !seen.insert(ty.clone()) {
                continue;
            }
            match ty {
                Type::Record(id, arguments) => pending.extend(self.record_fields(id, &arguments)),
                Type::Union(id, arguments) => {
                    pending.extend(self.union_payloads(id, &arguments).into_iter().flatten())
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
        true
    }

    /// Whether `found` holds for a type reachable from `root` through stored values and the
    /// targets of references (A13). Function and task types describe calls, not stored values.
    pub(super) fn reaches(&self, root: &Type, found: impl Fn(&Type) -> bool) -> bool {
        let mut seen = BTreeSet::new();
        let mut pending = vec![root.clone()];
        while let Some(ty) = pending.pop() {
            if found(&ty) {
                return true;
            }
            if !seen.insert(ty.clone()) {
                continue;
            }
            match ty {
                Type::Record(id, arguments) => pending.extend(self.record_fields(id, &arguments)),
                Type::Union(id, arguments) => {
                    pending.extend(self.union_payloads(id, &arguments).into_iter().flatten())
                }
                Type::Reference(inner, _)
                | Type::Array(inner)
                | Type::List(inner)
                | Type::Vec(inner)
                | Type::FixedArray(inner, _)
                | Type::Shared(inner, _) => pending.push(*inner),
                Type::Tuple(elements) => pending.extend(elements),
                _ => {}
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
