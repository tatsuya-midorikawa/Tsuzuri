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

/// The order in which the requested workers are emitted. A program that stays within the budget is
/// always emitted `InRequestOrder`. One that runs out of the budget is emitted again `DepthFirst`,
/// so that the workers of one function are complete before the next function asks for its own:
/// in request order, a budget that is too small for every chain cuts all of them part-way.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Drain {
    InRequestOrder,
    DepthFirst,
}

pub(super) struct Specializations {
    pub eligible: Vec<BTreeSet<usize>>,
    pub requests: Vec<Specialization>,
    keys: BTreeMap<Specialization, usize>,
    readable: BTreeMap<ClosureTarget, bool>,
    /// A request found the budget spent.
    refused: bool,
    /// `InRequestOrder`: the next request to emit.
    next: usize,
    /// `DepthFirst`: the requests that wait for emission, the next one last, and how many requests
    /// have been put there.
    pending: Vec<usize>,
    scheduled: usize,
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
                            && !function.signature.result.carries_loans(&module.types())
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
            refused: false,
            next: 0,
            pending: Vec::new(),
            scheduled: 0,
        }
    }

    pub fn can_borrow(&mut self, target: ClosureTarget, module: &CheckedModule) -> bool {
        *self.readable.entry(target).or_insert_with(|| {
            let function = &module.functions[target.function];
            !function.is_task
                && !function.owned_captures
                && target.bound <= function.parameters.len()
                && !function.signature.result.carries_loans(&module.types())
                && function.parameters[..target.bound].iter().all(|parameter| {
                    parameter.ty.can_capture(&module.types())
                        && (!parameter.ty.needs_drop(&module.types())
                            || function
                                .body
                                .consuming_use(parameter.id, &module.types())
                                .is_none())
                })
        })
    }

    pub fn request(&mut self, key: Specialization) -> Option<usize> {
        if let Some(id) = self.keys.get(&key) {
            return Some(*id);
        }
        if self.requests.len() == MAX_SPECIALIZATIONS {
            // This is an optimization budget, not a limit on valid source programs.
            self.refused = true;
            return None;
        }
        let id = self.requests.len();
        self.requests.push(key.clone());
        self.keys.insert(key, id);
        Some(id)
    }

    /// Whether a request found the budget spent, so that some call keeps its generic path.
    pub fn exhausted(&self) -> bool {
        self.refused
    }

    /// The next requested worker to emit, with its id.
    fn next_worker(&mut self, drain: Drain) -> Option<(usize, Specialization)> {
        let id = match drain {
            Drain::InRequestOrder => {
                let id = self.next;
                if id == self.requests.len() {
                    return None;
                }
                self.next += 1;
                id
            }
            Drain::DepthFirst => {
                // The requests that the last emission made, the first of them on top.
                self.pending
                    .extend((self.scheduled..self.requests.len()).rev());
                self.scheduled = self.requests.len();
                self.pending.pop()?
            }
        };
        Some((id, self.requests[id].clone()))
    }
}

/// The order in which `DepthFirst` emits the functions: each one is followed by the functions it
/// names, so that the lifted closures of a chain come right after the function that builds it.
pub(super) fn reference_order(module: &CheckedModule) -> Vec<usize> {
    let graph = escape::references(module);
    let mut seen = vec![false; graph.len()];
    let mut order = Vec::with_capacity(graph.len());
    for root in 0..graph.len() {
        let mut pending = vec![root];
        while let Some(id) = pending.pop() {
            if !std::mem::replace(&mut seen[id], true) {
                order.push(id);
                pending.extend(graph[id].iter().rev());
            }
        }
    }
    order
}

/// Emits the workers that have been requested and not yet emitted, and those that their emission requests.
pub(super) fn emit_workers(
    module: &CheckedModule,
    builtins: &mut Builtins,
    intrinsics: &mut BTreeSet<String>,
    globals: &mut Globals,
    specializations: &mut Specializations,
    drain: Drain,
    output: &mut String,
) {
    while let Some((id, key)) = specializations.next_worker(drain) {
        let start = output.len();
        let emitter = FunctionEmitter::new(
            module,
            &module.functions[key.function],
            key.function,
            builtins,
            intrinsics,
            globals,
            specializations,
        )
        .specialized(id, &key);
        output.push_str(&emitter.emit());
        if let Some(marks) = &mut globals.traps {
            marks.source(&output[start..], &module.functions[key.function]);
        }
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

pub(super) fn transparent<'a>(expression: &'a TypedExpr, module: &CheckedModule) -> &'a TypedExpr {
    transparent_calls(expression, module, |_| {})
}

/// `transparent`, passing `skipped` each identity function that it looks through.
pub(super) fn transparent_calls<'a>(
    mut expression: &'a TypedExpr,
    module: &CheckedModule,
    mut skipped: impl FnMut(usize),
) -> &'a TypedExpr {
    loop {
        if let TypedExprKind::Cast(value) = &expression.kind
            && let (Type::Function(from, from_result), Type::Function(to, to_result)) =
                (&value.ty, &expression.ty)
            && from.types == to.types
            && from_result == to_result
            && (0..from.len()).all(|index| from.convention(index) == to.convention(index))
        {
            expression = value;
            continue;
        }
        let TypedExprKind::Call(callee, arguments) = &expression.kind else {
            break;
        };
        let TypedExprKind::Function(FunctionRef::User(id)) = callee.kind else {
            break;
        };
        let function = &module.functions[id];
        if arguments.len() != 1 || !is_identity(function) {
            break;
        }
        skipped(id);
        expression = &arguments[0];
    }
    expression
}

pub(super) fn is_identity(function: &CheckedFunction) -> bool {
    function.parameters.len() == 1
        && matches!(function.body.kind, TypedExprKind::Local(id) if id == function.parameters[0].id)
}

/// `left op right`, written directly or as a call of a two-parameter user function whose body is
/// `x op y`; the last element is that function, which the caller does not call.
pub(super) fn binary_operation<'a>(
    expression: &'a TypedExpr,
    module: &CheckedModule,
) -> Option<(BinaryOp, &'a TypedExpr, &'a TypedExpr, Option<usize>)> {
    fn unwrapped(mut expression: &TypedExpr) -> &TypedExpr {
        while let TypedExprKind::Block { bindings, result } = &expression.kind {
            if !bindings.is_empty() {
                break;
            }
            expression = result;
        }
        expression
    }
    match &unwrapped(expression).kind {
        TypedExprKind::Binary(operator, left, right) => Some((*operator, left, right, None)),
        TypedExprKind::Call(callee, arguments) if arguments.len() == 2 => {
            let TypedExprKind::Function(FunctionRef::User(id)) = callee.kind else {
                return None;
            };
            let function = &module.functions[id];
            if function.parameters.len() != 2 {
                return None;
            }
            let TypedExprKind::Binary(operator, left, right) = &unwrapped(&function.body).kind
            else {
                return None;
            };
            if !matches!(left.kind, TypedExprKind::Local(id) if id == function.parameters[0].id)
                || !matches!(right.kind, TypedExprKind::Local(id) if id == function.parameters[1].id)
            {
                return None;
            }
            Some((*operator, &arguments[0], &arguments[1], Some(id)))
        }
        _ => None,
    }
}

/// The operator of a compiler-generated method `Add.add`, `Sub.sub` or `Mul.mul` of a scalar number,
/// whose body is `x op y` of its two parameters (PR02 D10). A direct call of it is the operator
/// itself, so the emitter writes `x op y` instead of calling it: the builtin and the operator form
/// then take the same path. A function written in source code never qualifies, and neither does a
/// string, whose `+` takes its operands.
pub(super) fn intrinsic_operation(function: &CheckedFunction) -> Option<BinaryOp> {
    if function.module != "$intrinsic" || function.parameters.len() != 2 {
        return None;
    }
    let TypedExprKind::Binary(
        operator @ (BinaryOp::Add | BinaryOp::Subtract | BinaryOp::Multiply),
        left,
        right,
    ) = &function.body.kind
    else {
        return None;
    };
    let operand = |expression: &TypedExpr, index: usize| matches!(expression.kind, TypedExprKind::Local(id) if id == function.parameters[index].id);
    let scalar = |ty: &Type| matches!(ty, Type::Integer(..) | Type::Binary(_) | Type::Decimal(_));
    (operand(left, 0)
        && operand(right, 1)
        && function
            .parameters
            .iter()
            .all(|parameter| scalar(&parameter.ty))
        && scalar(&function.signature.result))
    .then_some(*operator)
}

pub(crate) fn field_root(mut expression: &TypedExpr) -> Option<(usize, &Type)> {
    loop {
        match &expression.kind {
            TypedExprKind::Local(id) => return Some((*id, &expression.ty)),
            TypedExprKind::Field(value, _) => expression = value,
            _ => return None,
        }
    }
}

pub(crate) fn single_use_locals(expression: &TypedExpr) -> BTreeSet<usize> {
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
            Type::Array(ty)
            | Type::List(ty)
            | Type::Vec(ty)
            | Type::FixedArray(ty, _)
            | Type::Shared(ty, _)
            | Type::Reference(ty, false) => mutable_reference(ty, module),
            Type::Record(id, arguments) => module
                .types()
                .record_fields(*id, arguments)
                .iter()
                .any(|ty| mutable_reference(ty, module)),
            Type::Tuple(elements) => elements.iter().any(|ty| mutable_reference(ty, module)),
            _ => false,
        }
    }
    matches!(
        expression.kind,
        TypedExprKind::Assign(..)
            | TypedExprKind::HostCall(..)
            | TypedExprKind::Borrow(_, true)
            | TypedExprKind::StructuralCompare(..)
            | TypedExprKind::StructuralHash(_)
            | TypedExprKind::StructuralDisplay(_)
            | TypedExprKind::Interpolated(_)
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

fn all_children(expression: &TypedExpr, visit: &mut impl FnMut(&TypedExpr) -> bool) -> bool {
    expression.children().into_iter().all(visit)
}
