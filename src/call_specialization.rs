use super::*;

const MAX_SPECIALIZATIONS: usize = 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct ClosureTarget {
    pub function: usize,
    pub bound: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Specialization {
    pub function: usize,
    pub callbacks: Vec<(usize, ClosureTarget)>,
    pub borrowed: usize,
}

pub(super) struct Specializations {
    pub eligible: Vec<BTreeSet<usize>>,
    pub requests: Vec<Specialization>,
    keys: BTreeMap<Specialization, usize>,
    readable: BTreeMap<ClosureTarget, bool>,
}

impl Specializations {
    pub fn new(module: &CheckedModule) -> Self {
        let mut eligible: Vec<BTreeSet<_>> = module
            .functions
            .iter()
            .map(|function| {
                function
                    .parameters
                    .iter()
                    .enumerate()
                    .filter(|(_, parameter)| {
                        matches!(parameter.ty, Type::Function(..))
                            && !function.signature.result.carries_loans(&module.records)
                    })
                    .map(|(index, _)| index)
                    .collect()
            })
            .collect();
        loop {
            let mut rejected = Vec::new();
            for (id, function) in module.functions.iter().enumerate() {
                for &index in &eligible[id] {
                    if !non_escaping(
                        &function.body,
                        function.parameters[index].id,
                        &eligible,
                        module,
                    ) {
                        rejected.push((id, index));
                    }
                }
            }
            if rejected.is_empty() {
                break;
            }
            for (id, index) in rejected {
                eligible[id].remove(&index);
            }
        }
        Self {
            eligible,
            requests: Vec::new(),
            keys: BTreeMap::new(),
            readable: BTreeMap::new(),
        }
    }

    pub fn can_borrow(&mut self, target: ClosureTarget, module: &CheckedModule) -> bool {
        *self.readable.entry(target).or_insert_with(|| {
            let function = &module.functions[target.function];
            !function.is_task
                && target.bound <= function.parameters.len()
                && !function.signature.result.carries_loans(&module.records)
                && function.parameters[..target.bound].iter().all(|parameter| {
                    parameter.ty.can_capture(&module.records)
                        && (!parameter.ty.needs_drop(&module.records)
                            || read_only(&function.body, parameter.id, Access::Consume, module))
                })
        })
    }

    pub fn request(&mut self, key: Specialization) -> Option<usize> {
        if let Some(id) = self.keys.get(&key) {
            return Some(*id);
        }
        if self.requests.len() == MAX_SPECIALIZATIONS {
            // This is an optimization budget, not a limit on valid source programs.
            return None;
        }
        let id = self.requests.len();
        self.requests.push(key.clone());
        self.keys.insert(key, id);
        Some(id)
    }
}

pub(super) fn target(
    expression: &TypedExpr,
    known: &BTreeMap<usize, ClosureTarget>,
    module: &CheckedModule,
) -> Option<ClosureTarget> {
    let expression = transparent(expression, module);
    match &expression.kind {
        TypedExprKind::Local(id) => known.get(id).copied(),
        TypedExprKind::Function(FunctionRef::User(function)) => Some(ClosureTarget {
            function: *function,
            bound: 0,
        }),
        TypedExprKind::Closure(function, values) => Some(ClosureTarget {
            function: *function,
            bound: values.len(),
        }),
        TypedExprKind::Call(callee, arguments) => match callee.kind {
            TypedExprKind::Function(FunctionRef::User(function))
                if arguments.len() < module.functions[function].parameters.len() =>
            {
                Some(ClosureTarget {
                    function,
                    bound: arguments.len(),
                })
            }
            _ => None,
        },
        _ => None,
    }
}

pub(super) fn transparent<'a>(
    mut expression: &'a TypedExpr,
    module: &CheckedModule,
) -> &'a TypedExpr {
    while let TypedExprKind::Call(callee, arguments) = &expression.kind {
        let TypedExprKind::Function(FunctionRef::User(id)) = callee.kind else {
            break;
        };
        let function = &module.functions[id];
        if arguments.len() != 1 || !is_identity(function) {
            break;
        }
        expression = &arguments[0];
    }
    expression
}

pub(super) fn is_identity(function: &CheckedFunction) -> bool {
    function.parameters.len() == 1
        && matches!(function.body.kind, TypedExprKind::Local(id) if id == function.parameters[0].id)
}

pub(super) fn single_use_locals(expression: &TypedExpr) -> BTreeSet<usize> {
    fn count(expression: &TypedExpr, uses: &mut BTreeMap<usize, usize>) {
        if let TypedExprKind::Local(id) = expression.kind {
            *uses.entry(id).or_default() += 1;
        }
        all_children(expression, &mut |child| {
            count(child, uses);
            true
        });
        if matches!(
            expression.kind,
            TypedExprKind::While { .. }
                | TypedExprKind::ForRange { .. }
                | TypedExprKind::ForEach { .. }
        ) {
            for count in uses.values_mut() {
                *count = (*count).max(2);
            }
        }
    }
    let mut uses = BTreeMap::new();
    count(expression, &mut uses);
    uses.into_iter()
        .filter_map(|(id, count)| (count == 1).then_some(id))
        .collect()
}

pub(super) fn may_mutate(expression: &TypedExpr, module: &CheckedModule) -> bool {
    fn mutable_reference(ty: &Type, module: &CheckedModule) -> bool {
        match ty {
            Type::Reference(_, true) => true,
            Type::Array(ty) | Type::List(ty) | Type::Reference(ty, false) => {
                mutable_reference(ty, module)
            }
            Type::Record(id) => module.records[*id]
                .fields
                .iter()
                .any(|(_, ty)| mutable_reference(ty, module)),
            Type::Tuple(elements) => elements.iter().any(|ty| mutable_reference(ty, module)),
            _ => false,
        }
    }
    matches!(
        expression.kind,
        TypedExprKind::Assign(..) | TypedExprKind::Borrow(_, true)
    ) || mutable_reference(&expression.ty, module)
        || !all_children(expression, &mut |child| !may_mutate(child, module))
}

fn non_escaping(
    expression: &TypedExpr,
    local: usize,
    eligible: &[BTreeSet<usize>],
    module: &CheckedModule,
) -> bool {
    use TypedExprKind::*;
    match &expression.kind {
        Local(id) => *id != local,
        Call(callee, arguments) if matches!(callee.kind, Local(id) if id == local) => {
            matches!(&callee.ty, Type::Function(parameters, _) if parameters.len() == arguments.len())
                && arguments
                    .iter()
                    .all(|argument| non_escaping(argument, local, eligible, module))
        }
        Binary(BinaryOp::Pipe, argument, callee) if matches!(callee.kind, Local(id) if id == local) => {
            matches!(&callee.ty, Type::Function(parameters, _) if parameters.len() == 1)
                && non_escaping(argument, local, eligible, module)
        }
        Binary(BinaryOp::Pipe, argument, callee) if matches!(argument.kind, Local(id) if id == local) =>
        {
            matches!(callee.kind, Function(FunctionRef::User(id))
                if module.functions[id].parameters.len() == 1 && eligible[id].contains(&0))
        }
        NewArray(length, initializer) | NewList(length, initializer) if matches!(initializer.kind, Local(id) if id == local) => {
            non_escaping(length, local, eligible, module)
        }
        Call(callee, arguments) => {
            non_escaping(callee, local, eligible, module)
                && arguments.iter().enumerate().all(|(index, argument)| {
                    if matches!(argument.kind, Local(id) if id == local) {
                        matches!(callee.kind, Function(FunctionRef::User(id))
                            if arguments.len() == module.functions[id].parameters.len()
                                && eligible[id].contains(&index))
                    } else {
                        non_escaping(argument, local, eligible, module)
                    }
                })
        }
        _ => all_children(expression, &mut |child| {
            non_escaping(child, local, eligible, module)
        }),
    }
}

#[derive(Clone, Copy)]
enum Access {
    Consume,
    Read,
    Write,
}

fn read_only(expression: &TypedExpr, local: usize, access: Access, module: &CheckedModule) -> bool {
    use TypedExprKind::*;
    match &expression.kind {
        Local(id) if *id == local => match access {
            Access::Consume => expression.ty.is_copy(&module.records),
            Access::Read => true,
            Access::Write => false,
        },
        Borrow(value, mutable) => read_only(
            value,
            local,
            if *mutable {
                Access::Write
            } else {
                Access::Read
            },
            module,
        ),
        Assign(place, value) => {
            read_only(place, local, Access::Write, module)
                && read_only(value, local, Access::Consume, module)
        }
        Field(value, _) | Index(value, _) if FunctionEmitter::is_place(expression) => {
            let access = match access {
                Access::Consume if expression.ty.is_copy(&module.records) => Access::Read,
                other => other,
            };
            read_only(value, local, access, module)
                && match &expression.kind {
                    Index(_, index) => read_only(index, local, Access::Consume, module),
                    _ => true,
                }
        }
        Length(value) | StringLength(value) => read_only(value, local, Access::Read, module),
        Binary(BinaryOp::Equal | BinaryOp::NotEqual, left, right) if left.ty == Type::String => {
            read_only(left, local, Access::Read, module)
                && read_only(right, local, Access::Read, module)
        }
        _ => all_children(expression, &mut |child| {
            read_only(child, local, Access::Consume, module)
        }),
    }
}

fn all_children(expression: &TypedExpr, visit: &mut impl FnMut(&TypedExpr) -> bool) -> bool {
    expression.children().into_iter().all(visit)
}
