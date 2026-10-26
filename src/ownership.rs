use std::collections::{BTreeMap, BTreeSet};
use std::ops::ControlFlow;

use crate::check::{
    CallbackContract, CheckedModule, Local, MAX_RECORD_REGIONS, RegionMask, RegionSlots,
    RegionSources, Type, TypedExpr, TypedExprKind as E,
};
use crate::copies::{CopyKind, CopyRead};
use crate::diagnostic::{Diagnostic, Diagnostics, MAX_UNIQUE_DIAGNOSTICS, RelatedRole, Span};
use crate::parallel::{Parallelism, ordered_for_each};
use crate::syntax::BinaryOp;

#[path = "ownership_control.rs"]
mod control;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Place {
    root: usize,
    fields: Vec<usize>,
    /// A field or payload step starts at a value whose type implements Drop. The types along
    /// `root` and `fields` decide it, so equal places always agree.
    through_drop: bool,
}

/// Collection elements are conservatively one place.
const ELEMENT: usize = usize::MAX;
/// Every case payload of a union shares the storage after the tag.
const PAYLOAD: usize = usize::MAX - 1;
/// A12: the first field of a parameter's external loan for one region slot, `REGION_SLOT - slot`.
const REGION_SLOT: usize = usize::MAX - 2;
/// D11 Phase 2: the static data `id` is the place `STATIC_ROOT + 1 + id`, far from the local ids
/// and from the parameters' roots, `usize::MAX - id`. Nobody writes, moves, or drops it. The root
/// `STATIC_ROOT` itself stands for any static data, as the loan of a call's result that holds
/// only borrows of static data.
const STATIC_ROOT: usize = usize::MAX / 2;
const STATIC_ROOTS: usize = 1 << 24;

fn is_static_root(root: usize) -> bool {
    root.wrapping_sub(STATIC_ROOT) < STATIC_ROOTS
}

fn region_field(slot: usize) -> usize {
    REGION_SLOT - slot
}

/// The region slot of an external loan's place; 0 without a slot marker.
fn place_slot(place: &Place) -> usize {
    match place.fields.first() {
        Some(field) if is_region_field(*field) => REGION_SLOT - field,
        _ => 0,
    }
}

fn is_region_field(field: usize) -> bool {
    (REGION_SLOT + 1 - MAX_RECORD_REGIONS..=REGION_SLOT).contains(&field)
}

/// The fields of `place` after the slot marker of an external loan's place (A13).
fn unmarked_fields(place: &Place) -> &[usize] {
    match place.fields.split_first() {
        Some((field, rest)) if is_region_field(*field) => rest,
        _ => &place.fields,
    }
}

fn mask_slots(mask: RegionMask) -> impl Iterator<Item = usize> {
    (0..MAX_RECORD_REGIONS).filter(move |slot| mask & (1 << slot) != 0)
}

/// Empty slots for the result of a direct call whose named result has several regions.
#[inline(never)]
fn call_slots(sources: &RegionSources) -> Option<Box<[BTreeSet<usize>]>> {
    (sources.len() >= 2).then(|| vec![BTreeSet::new(); sources.len()].into_boxed_slice())
}

/// Joins the result of one match or `if` branch into the result of the others.
#[inline(never)]
fn join(result: &mut Value, value: Value) {
    if result.loans.is_empty() && result.regions.is_none() {
        result.regions = value.regions;
    } else if let (Some(slots), Some(other)) = (result.regions.as_mut(), value.regions.as_ref())
        && slots.len() == other.len()
    {
        for (slot, other) in slots.iter_mut().zip(other) {
            slot.extend(other);
        }
    } else if !value.loans.is_empty() {
        result.regions = None;
    }
    result.loans.extend(value.loans);
}

/// B08 D6: an `Async` value holds no loans, so no borrow lives across a suspension.
const ASYNC_BORROW: &str = "async computations cannot keep borrowed values across 'let!', 'do!', or the start of the computation; move or clone the value into the async block instead";
const MOVE_OUT_OF_DROP: &str = "cannot move a field or payload out of a value whose type implements Drop; borrow it with 'ref' instead";

impl Place {
    fn overlaps(&self, other: &Self) -> bool {
        self.root == other.root && self.fields.iter().zip(&other.fields).all(|(a, b)| a == b)
    }
}

#[derive(Clone, Default)]
struct Value {
    loans: BTreeSet<usize>,
    /// A12: the loans of each region slot of a record with several regions; `None` lets every
    /// loan belong to every slot. Read only through `Checker::slots`, which validates it.
    regions: Option<Box<[BTreeSet<usize>]>>,
    closed_result: [bool; 2],
}

struct Loan {
    place: Place,
    mutable: bool,
    parents: BTreeSet<usize>,
    /// The type of the pattern view that holds this loan. Such a view would be
    /// a copy if the type were Copy, so while inferring constraints a conflict
    /// with the loan requires Copy instead of rejecting the access.
    view: Option<Type>,
    /// The expression that created the loan, for the diagnostics that name it.
    span: Span,
}

#[derive(Clone, Default)]
struct State<'a> {
    locals: BTreeMap<usize, (&'a Local, Value)>,
    aliases: BTreeMap<usize, Vec<(Place, BTreeSet<usize>)>>,
    /// The places that were moved, with the span of the move that the diagnostics point at.
    moved: BTreeMap<Place, Span>,
    generic_moves: BTreeMap<Place, Type>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Use {
    Consume,
    Read,
    Borrow,
    MutBorrow,
    Write,
}

pub fn check(module: &CheckedModule) -> Result<(), Diagnostic> {
    check_all(module).into_iter().next().map_or(Ok(()), Err)
}

pub fn check_all(module: &CheckedModule) -> Vec<Diagnostic> {
    check_all_with(module, Parallelism::library())
}

pub(crate) fn check_all_with(module: &CheckedModule, parallelism: Parallelism) -> Vec<Diagnostic> {
    check_functions(module, false, parallelism)
        .err()
        .unwrap_or_default()
}

pub(crate) fn infer_copy_all(
    module: &CheckedModule,
    parallelism: Parallelism,
) -> Result<Vec<BTreeSet<String>>, Vec<Diagnostic>> {
    check_functions(module, true, parallelism)
}

// ponytail: access errors only while types are broken; Error stubs lack loans, so lifetimes wait.
pub(crate) fn check_recovered(module: &CheckedModule) -> Vec<Diagnostic> {
    let closed = closed_returns(module);
    let mut recovered = Vec::new();
    for function in &module.functions {
        let _ = check_body(
            module,
            function.parameters.iter(),
            &function.body,
            true,
            &closed,
            function.is_task,
            false,
            Contract::of(function),
            &mut recovered,
            None,
        );
    }
    recovered
}

/// The consuming reads of Copy values that need a drop, per function of a checked module (A15).
pub(crate) fn copy_reads(module: &CheckedModule) -> Vec<Vec<CopyRead>> {
    let closed = closed_returns(module);
    let mut reads = Vec::with_capacity(module.functions.len());
    for (id, function) in module.functions.iter().enumerate() {
        let mut copies = Vec::new();
        let _ = check_body(
            module,
            function.parameters.iter(),
            &function.body,
            false,
            &closed,
            function.is_task,
            module.user_drops.values().any(|drop| *drop == id),
            Contract::of(function),
            &mut Vec::new(),
            Some(&mut copies),
        );
        reads.push(copies);
    }
    reads
}

/// The functions that one worker checks before a check uses another thread, and how many a worker
/// takes at a time.
const FUNCTIONS_PER_WORKER: usize = 64;
const FUNCTION_CHUNK: usize = 8;

/// The checks of different functions read the module and nothing else, so they run in parallel.
/// The results are consumed in function order by the loop that used to run them: it stops at the
/// first function after `Diagnostics` is full, and a stage on one thread does not even check that
/// function, so the diagnostics and the constraints do not depend on the number of threads.
fn check_functions(
    module: &CheckedModule,
    infer: bool,
    parallelism: Parallelism,
) -> Result<Vec<BTreeSet<String>>, Vec<Diagnostic>> {
    let closed = closed_returns(module);
    let mut constraints = Vec::new();
    let mut diagnostics = Diagnostics::new(0);
    let count = module.functions.len();
    ordered_for_each(
        count,
        parallelism.workers(count, FUNCTIONS_PER_WORKER),
        FUNCTION_CHUNK,
        None,
        |id| {
            let function = &module.functions[id];
            let scalar = !infer && ownership_free(&function.body);
            if scalar && !cfg!(debug_assertions) {
                return None;
            }
            let mut recovered = Vec::new();
            let checked = check_body(
                module,
                function.parameters.iter(),
                &function.body,
                infer,
                &closed,
                function.is_task,
                module.user_drops.values().any(|drop| *drop == id),
                Contract::of(function),
                &mut recovered,
                None,
            );
            if scalar {
                debug_assert!(recovered.is_empty());
                debug_assert!(matches!(&checked, Ok(required) if required.is_empty()));
            }
            for error in &mut recovered {
                default_parameter_hint(module, function, error);
            }
            let checked = checked.map_err(|mut error| {
                default_parameter_hint(module, function, &mut error);
                error
            });
            Some((recovered, checked))
        },
        |_, outcome| {
            if diagnostics.is_full() {
                return ControlFlow::Break(());
            }
            let Some((recovered, checked)) = outcome.get() else {
                constraints.push(BTreeSet::new());
                return ControlFlow::Continue(());
            };
            diagnostics.extend(recovered);
            match checked {
                Ok(required) => constraints.push(required),
                Err(error) => diagnostics.push(error),
            }
            ControlFlow::Continue(())
        },
    );
    if diagnostics.is_empty() {
        Ok(constraints)
    } else {
        Err(diagnostics.into_vec())
    }
}

fn default_parameter_hint(
    module: &CheckedModule,
    function: &crate::check::CheckedFunction,
    error: &mut Diagnostic,
) {
    if error.code != "E1012"
        || error.message != "cannot move a non-Copy value out of a reference"
        || !function.read_by_default.iter().any(|read| *read)
    {
        return;
    }
    let mut pending = vec![&function.body];
    while let Some(expression) = pending.pop() {
        if expression.span == error.span {
            let mut root = expression;
            loop {
                match &root.kind {
                    E::Field(value, _) | E::Dereference(value) => root = value,
                    E::RecordUpdate { base, .. } => root = base,
                    _ => break,
                }
            }
            if let E::Local(id) = root.kind
                && let Some((index, parameter)) = function
                    .parameters
                    .iter()
                    .enumerate()
                    .find(|(_, parameter)| parameter.id == id)
                && function.read_by_default.get(index) == Some(&true)
                && let Type::Reference(inner, _) = &parameter.ty
            {
                crate::check::read_only_hint(
                    error,
                    &parameter.name,
                    &inner.display(&module.types()),
                    function.origin.pure,
                );
                return;
            }
        }
        pending.extend(expression.children());
    }
}

/// These expressions cannot move ownership, create a loan, or observe a borrowed
/// environment. Type changes and generated call ids alone are not such a proof.
fn ownership_free(body: &TypedExpr) -> bool {
    let mut pending = vec![body];
    while let Some(expression) = pending.pop() {
        if !matches!(
            expression.ty,
            Type::Integer(..)
                | Type::Binary(_)
                | Type::Decimal(_)
                | Type::Simd(_)
                | Type::Bool
                | Type::Unit
                | Type::Char
                | Type::Utf8Char
        ) || !matches!(
            expression.kind,
            E::Int(_)
                | E::Float(_)
                | E::Bool(_)
                | E::Unit
                | E::Local(_)
                | E::Unary(..)
                | E::Cast(_)
                | E::SaturatingIndex(_)
                | E::Binary(..)
                | E::If { .. }
        ) {
            return false;
        }
        expression
            .try_for_each_child(&mut |child| {
                pending.push(child);
                Ok(())
            })
            .expect("classifying scalar ownership cannot fail");
    }
    true
}

/// The region contracts that a checked body proves (its result) and relies on (its callbacks and
/// the regions of its parameters' slots).
#[derive(Clone, Copy, Default)]
struct Contract<'a> {
    result: Option<&'a RegionSources>,
    callbacks: &'a [CallbackContract],
    slots: Option<&'a RegionSlots>,
}

impl<'a> Contract<'a> {
    fn of(function: &'a crate::check::CheckedFunction) -> Self {
        Self {
            result: function.region_sources.as_ref(),
            callbacks: &function.callback_contracts,
            slots: function.region_slots.as_ref(),
        }
    }
}

/// A13 Phase 2: what a region slot of a parameter stands for, by its external root and slot.
#[derive(Default)]
struct ExternalSlot {
    /// The declared region that the slot names.
    region: Option<usize>,
    /// The loans that stand for the borrows inside the targets of the slot's references, when
    /// the parameter's type names their region apart from the references' own.
    target: BTreeSet<usize>,
    /// The declared regions of those borrows; storing through the slot's references needs them.
    target_regions: BTreeSet<usize>,
}

#[allow(clippy::too_many_arguments)]
fn check_body<'a>(
    module: &'a CheckedModule,
    parameters: impl Clone + Iterator<Item = &'a Local>,
    body: &'a TypedExpr,
    infer: bool,
    closed: &[bool],
    task: bool,
    user_drop: bool,
    contract: Contract<'a>,
    recovered: &mut Vec<Diagnostic>,
    copies: Option<&mut Vec<CopyRead>>,
) -> Result<BTreeSet<String>, Diagnostic> {
    let mut declarations: BTreeMap<_, _> =
        parameters.clone().map(|local| (local.id, local)).collect();
    collect_declarations(body, &mut declarations);
    let mut checker = Checker {
        module,
        body,
        declarations,
        state: State::default(),
        loans: Vec::new(),
        held: Vec::new(),
        loop_flows: Vec::new(),
        try_flows: Vec::new(),
        reachable: true,
        external: BTreeSet::new(),
        slots: BTreeMap::new(),
        infer,
        copy_variables: BTreeSet::new(),
        closed,
        recovered,
        copies,
        reported: BTreeSet::new(),
        callbacks: contract
            .callbacks
            .iter()
            .map(|callback| {
                (
                    parameters.clone().nth(callback.parameter).unwrap().id,
                    callback,
                )
            })
            .collect(),
        // `Drop.drop` borrows the dropped value through its only parameter.
        drop_root: user_drop.then(|| usize::MAX - parameters.clone().next().unwrap().id),
        placeholders: BTreeSet::new(),
        later_use_budget: LATER_USE_NODES,
    };
    for (index, parameter) in parameters.clone().enumerate() {
        let mut value = Value::default();
        if !task && parameter.ty.carries_loans(&module.types()) {
            checker.external_loans(parameter, index, contract.slots, &mut value);
        }
        checker
            .state
            .locals
            .insert(parameter.id, (parameter, value));
    }
    let result = checker.eval(body, Use::Consume, &BTreeSet::new())?;
    checker.check_result(parameters, body, task, contract, result)?;
    Ok(checker.copy_variables)
}

// Unknown results retain their input loans. Only proven closed snapshots can end a curried stage's loans.
fn owned(ty: &Type, module: &CheckedModule) -> bool {
    if module.types().recursive(ty) {
        return !ty.carries_loans(&module.types());
    }
    match ty {
        Type::Variable(_) | Type::Infer(_) | Type::Reference(..) | Type::Function(..) => false,
        // `dyn C {r}` holds the borrows that it was built from (A14 Phase 2).
        Type::Dyn(dyn_type) => !dyn_type.borrowed,
        Type::Array(element)
        | Type::List(element)
        | Type::Vec(element)
        | Type::FixedArray(element, _)
        | Type::Shared(element, _) => owned(element, module),
        Type::Tuple(elements) => elements.iter().all(|ty| owned(ty, module)),
        Type::Record(id, arguments) => module
            .types()
            .record_fields_all(*id, arguments, |ty| owned(ty, module)),
        Type::Union(id, arguments) => module
            .types()
            .union_payloads_all(*id, arguments, |ty| owned(ty, module)),
        _ => true,
    }
}
fn closed(
    expression: &TypedExpr,
    module: &CheckedModule,
    known: &[bool],
    locals: &BTreeMap<usize, bool>,
) -> bool {
    if owned(&expression.ty, module) {
        return true;
    }
    match &expression.kind {
        E::Function(_)
        | E::GenericFunction(..)
        | E::CaseConstructor { .. }
        | E::Method(..)
        | E::TypeFunction { .. }
        | E::TaskRun(_)
        | E::TaskParallel(_)
        | E::TaskParallelResults(_)
        | E::Parallel(..) => true,
        E::Local(id) => locals.get(id).copied().unwrap_or(false),
        E::Construct { payload, .. } => payload
            .as_ref()
            .is_none_or(|value| closed(value, module, known, locals)),
        E::Lambda { captures, .. } => captures
            .iter()
            .all(|local| owned(&local.ty, module) || locals.get(&local.id) == Some(&true)),
        E::Closure(_, captures) | E::Array(captures) | E::List(captures) | E::Tuple(captures) => {
            captures
                .iter()
                .all(|value| closed(value, module, known, locals))
        }
        E::NewArray(_, initializer) | E::NewList(_, initializer) | E::NewLiteral(initializer) => {
            closed(initializer, module, known, locals)
        }
        E::Record(_) | E::RecordUpdate { .. } => expression
            .children()
            .into_iter()
            .all(|value| closed(value, module, known, locals)),
        E::If {
            then_branch,
            else_branch,
            ..
        } => {
            closed(then_branch, module, known, locals) && closed(else_branch, module, known, locals)
        }
        E::Block { bindings, result } => {
            let mut locals = locals.clone();
            for (local, value) in bindings {
                let value = !local.mutable && closed(value, module, known, &locals);
                locals.insert(local.id, value);
            }
            closed(result, module, known, &locals)
        }
        E::Call(callee, arguments) => {
            let complete_closed = match callee.kind {
                E::Function(crate::check::FunctionRef::User(id)) | E::GenericFunction(id, _) => {
                    arguments.len() == module.functions[id].parameters.len() && known[id]
                }
                _ => false,
            };
            complete_closed
                || (closed(callee, module, known, locals)
                    && arguments
                        .iter()
                        .all(|value| closed(value, module, known, locals)))
        }
        E::Field(value, _) | E::Index(value, _) | E::UnionPayload { value, .. } => {
            closed(value, module, known, locals)
        }
        _ => false,
    }
}
fn closed_returns(module: &CheckedModule) -> Vec<bool> {
    let mut known = vec![false; module.functions.len()];
    loop {
        let mut changed = false;
        for (id, function) in module.functions.iter().enumerate() {
            if !known[id] && closed(&function.body, module, &known, &BTreeMap::new()) {
                known[id] = true;
                changed = true;
            }
        }
        if !changed {
            return known;
        }
    }
}

/// Source declarations outlive every branch snapshot. Evaluation also visits synthetic
/// capture expressions, so its expression borrows must remain shorter than these borrows.
fn collect_declarations<'a>(body: &'a TypedExpr, locals: &mut BTreeMap<usize, &'a Local>) {
    let mut pending = vec![body];
    while let Some(expression) = pending.pop() {
        match &expression.kind {
            E::Block { bindings, .. } => {
                for (local, _) in bindings {
                    locals.insert(local.id, local);
                }
            }
            E::ForRange { local, .. } => {
                locals.insert(local.id, local);
            }
            E::ForEach { owner, local, .. } => {
                locals.insert(owner.id, owner);
                locals.insert(local.id, local);
            }
            E::Match { local, arms, .. } => {
                locals.insert(local.id, local);
                for arm in arms {
                    for alternative in &arm.alternatives {
                        for step in &alternative.steps {
                            if let crate::check::PatternStep::Bind(local, _) = step {
                                locals.insert(local.id, local);
                            }
                        }
                        for (local, _) in &alternative.bindings {
                            locals.insert(local.id, local);
                        }
                    }
                }
            }
            // A lambda starts another checker with its own declarations.
            E::Lambda { .. } => continue,
            _ => {}
        }
        expression
            .try_for_each_child(&mut |child| {
                pending.push(child);
                Ok(())
            })
            .expect("collecting declarations cannot fail");
    }
}

fn error(code: &'static str, message: impl Into<String>, span: Span) -> Diagnostic {
    Diagnostic::new(code, message, span)
}

/// Whether `callee` is `Dyn.of` building a dyn value without a region, which owns all it holds (A14).
fn owned_dyn_of(callee: &TypedExpr) -> bool {
    matches!(&callee.kind, E::Function(crate::check::FunctionRef::Builtin(instance))
        if instance.builtin == crate::check::Builtin::DynOf
            && matches!(instance.types.get(1), Some(Type::Dyn(dyn_type)) if !dyn_type.borrowed))
}

struct Checker<'a, 'run> {
    module: &'a CheckedModule,
    /// The body that is checked, searched for the later uses of a borrow when a diagnostic names them.
    body: &'a TypedExpr,
    declarations: BTreeMap<usize, &'a Local>,
    state: State<'a>,
    loans: Vec<Loan>,
    held: Vec<Value>,
    loop_flows: Vec<control::LoopFlow<'a>>,
    try_flows: Vec<control::TryFlow<'a>>,
    reachable: bool,
    external: BTreeSet<usize>,
    /// A13 Phase 2: the region slots of the parameters, by external root and slot.
    slots: BTreeMap<(usize, usize), ExternalSlot>,
    infer: bool,
    copy_variables: BTreeSet<String>,
    closed: &'run [bool],
    recovered: &'run mut Vec<Diagnostic>,
    /// Collects the copied consuming reads for `copies::sites`; the checks stay the same.
    copies: Option<&'run mut Vec<CopyRead>>,
    reported: BTreeSet<usize>,
    /// Parameters with region-quantified function types, by local id (A12 Phase 2).
    callbacks: BTreeMap<usize, &'a CallbackContract>,
    /// The root of the value that the checked `Drop.drop` body drops.
    drop_root: Option<usize>,
    /// The external roots of parameters that store no reference: their loans only stand for
    /// whatever a function or async argument captured, which the caller checks (B08 D6).
    placeholders: BTreeSet<usize>,
    /// How many more steps the diagnostics may take in this function to find where a moved value
    /// or a borrow came from (`Flow`).
    later_use_budget: usize,
}

/// How many expressions the diagnostics of one function may look at, in all, to find where a
/// borrow is used later; past it they leave `BorrowUsedHere` out.
const LATER_USE_NODES: usize = 1 << 20;

/// Whether each result slot of `actual` borrows only from inputs that `required` allows, once
/// the caller has bound the first `offset` inputs (A12 Phase 2).
fn implies(actual: &RegionSources, required: &RegionSources, offset: usize) -> bool {
    actual.len() == required.len()
        && actual.iter().zip(required).all(|(actual, required)| {
            actual
                .iter()
                .all(|&(input, slot)| input >= offset && required.contains(&(input - offset, slot)))
        })
}

/// Whether a named function, after `offset` bound arguments, keeps the region contract `callback`.
fn named_satisfies(
    function: &crate::check::CheckedFunction,
    callback: &CallbackContract,
    offset: usize,
) -> bool {
    function.parameters.len() == offset + callback.arity
        && match &function.region_sources {
            Some(sources) => implies(sources, &callback.sources, offset),
            None => callback.sources.is_empty(),
        }
}

impl<'a> Checker<'a, '_> {
    /// A call's callee function, the region contract that selects the loans of its result, and
    /// whether the callee is a region-quantified parameter. Checks first that the call passes only
    /// functions that keep the contracts of its region-quantified parameters (A12 Phase 2).
    #[inline(never)]
    fn call_contract(
        &mut self,
        callee: &TypedExpr,
        arguments: &[TypedExpr],
    ) -> Result<(Option<usize>, Option<&'a RegionSources>, bool), Diagnostic> {
        let module = self.module;
        match callee.kind {
            E::Function(crate::check::FunctionRef::User(id)) | E::GenericFunction(id, _) => {
                let function = &module.functions[id];
                if !self.infer {
                    for callback in &function.callback_contracts {
                        if let Some(argument) = arguments.get(callback.parameter)
                            && let Some(message) = self.unsatisfied(argument, callback)
                        {
                            return Err(error("E1013", message, argument.span));
                        }
                    }
                }
                let sources = (arguments.len() == function.parameters.len())
                    .then_some(function.region_sources.as_ref())
                    .flatten();
                Ok((Some(id), sources, false))
            }
            E::Local(id) => {
                let callback = self
                    .callbacks
                    .get(&id)
                    .copied()
                    .filter(|callback| callback.arity == arguments.len());
                Ok((
                    None,
                    callback.map(|callback| &callback.sources),
                    callback.is_some(),
                ))
            }
            _ => Ok((None, None, false)),
        }
    }

    /// Why `argument` cannot be passed where `callback` is expected, or `None` when it can.
    fn unsatisfied(
        &self,
        argument: &TypedExpr,
        callback: &CallbackContract,
    ) -> Option<&'static str> {
        const UNSUPPORTED: &str = "pass a named function with matching named regions, a lambda, or a parameter with the same region-quantified type here";
        let module = self.module;
        let satisfied = match &argument.kind {
            E::Local(id) => {
                let Some(own) = self.callbacks.get(id) else {
                    return Some(UNSUPPORTED);
                };
                own.arity == callback.arity && implies(&own.sources, &callback.sources, 0)
            }
            E::Function(crate::check::FunctionRef::User(id)) | E::GenericFunction(id, _) => {
                named_satisfies(&module.functions[*id], callback, 0)
            }
            E::Call(target, bound) => match target.kind {
                E::Function(crate::check::FunctionRef::User(id)) | E::GenericFunction(id, _)
                    if bound.len() < module.functions[id].parameters.len() =>
                {
                    named_satisfies(&module.functions[id], callback, bound.len())
                }
                _ => return Some(UNSUPPORTED),
            },
            E::Closure(id, captures) => {
                // A lambda's own body proves the contract; its captures are not inputs.
                let function = &module.functions[*id];
                let shifted: RegionSources = callback
                    .sources
                    .iter()
                    .map(|slot| {
                        slot.iter()
                            .map(|&(input, slot)| (input + captures.len(), slot))
                            .collect()
                    })
                    .collect();
                function.parameters.len() == captures.len() + callback.arity
                    && check_body(
                        module,
                        function.parameters.iter(),
                        &function.body,
                        false,
                        self.closed,
                        function.is_task,
                        false,
                        Contract {
                            result: Some(&shifted),
                            callbacks: &[],
                            slots: None,
                        },
                        &mut Vec::new(),
                        None,
                    )
                    .is_ok()
            }
            _ => return Some(UNSUPPORTED),
        };
        (!satisfied).then_some(
            "this function does not keep the region-quantified parameter type; its result may borrow from an input or a capture that the type does not name",
        )
    }
}

impl<'a> Checker<'a, '_> {
    fn callback_returns_closed(&self, expression: &TypedExpr, arity: usize) -> bool {
        if let E::Call(callee, arguments) = &expression.kind {
            return self.callback_returns_closed(callee, arity + arguments.len());
        }
        let (body, parameters, captures) = match &expression.kind {
            E::Lambda {
                body,
                parameters,
                captures,
                ..
            } => (body.as_ref(), parameters.as_slice(), captures.as_slice()),
            E::Function(crate::check::FunctionRef::User(id))
            | E::GenericFunction(id, _)
            | E::Closure(id, _) => {
                let function = &self.module.functions[*id];
                (
                    &function.body,
                    &function.parameters[function.capture_count..],
                    &function.parameters[..function.capture_count],
                )
            }
            _ => return false,
        };
        if matches!(body.kind, E::Error) {
            return true;
        }
        if parameters.len() > arity {
            return parameters[..arity]
                .iter()
                .all(|parameter| !parameter.ty.contains_stored_reference(&self.module.types()));
        }
        if parameters.len() < arity {
            return false;
        }
        let mut locals: BTreeMap<_, _> = parameters
            .iter()
            .map(|parameter| {
                (
                    parameter.id,
                    !parameter.ty.contains_stored_reference(&self.module.types()),
                )
            })
            .collect();
        locals.extend(captures.iter().map(|capture| (capture.id, true)));
        closed(body, self.module, self.closed, &locals)
    }

    /// The builtin that `callee` is, or that the generated function it names calls on its own
    /// parameters after lowering (every builtin is called through one).
    fn builtin_called(&self, callee: &TypedExpr) -> Option<crate::check::Builtin> {
        let E::Function(function) = &callee.kind else {
            return None;
        };
        match function {
            crate::check::FunctionRef::Builtin(instance) => Some(instance.builtin),
            crate::check::FunctionRef::User(id) => {
                let wrapper = &self.module.functions[*id];
                match &wrapper.body.kind {
                    E::Call(inner, _) if wrapper.module == "$builtin" => match &inner.kind {
                        E::Function(crate::check::FunctionRef::Builtin(instance)) => {
                            Some(instance.builtin)
                        }
                        _ => None,
                    },
                    _ => None,
                }
            }
        }
    }

    /// What `Mutex.with_lock` returns when `callee` is that builtin, or the generated function
    /// that calls it after lowering: the type after its lock and callback, which is a function
    /// when the callback's result is one (F10).
    fn locked_result(&self, callee: &TypedExpr) -> Option<Type> {
        match &callee.ty {
            Type::Function(parameters, _)
                if self.builtin_called(callee) == Some(crate::check::Builtin::MutexWith)
                    && parameters.len() >= 2 =>
            {
                Some(callee.ty.after_arguments(2))
            }
            _ => None,
        }
    }

    /// The type of the item that `callee` puts in a channel when it is `Channel.send`, or the
    /// generated function that calls it (F10). The item stays in the channel after the call.
    fn sent_item(&self, callee: &TypedExpr) -> Option<Type> {
        match &callee.ty {
            Type::Function(parameters, _)
                if self.builtin_called(callee) == Some(crate::check::Builtin::ChannelSend)
                    && parameters.len() >= 2 =>
            {
                Some(parameters[1].clone())
            }
            _ => None,
        }
    }

    /// Whether `argument` is a parameter of a generated function, which passes on what its
    /// callers gave it.
    fn is_generated_parameter(&self, argument: &TypedExpr) -> bool {
        matches!(&argument.kind, E::Local(id)
            if self.state.locals.get(id).is_some_and(|(local, _)| local.provenance == crate::syntax::Provenance::Generated))
    }

    /// Whether the tasks of a parallel operation that share the borrow `argument`, which evaluated
    /// to `value`, could reach a borrowed environment through it (F10). The loans of a place that
    /// is borrowed have the loans of the place as parents, and a loan of an external place says
    /// nothing about what the place holds. A temporary is borrowed where it is made, so the loans
    /// in `value` are the loans that the temporary holds itself: a loan with no parents on a plain
    /// local does not prove the environment owned, because the closure, not the local, is shared.
    fn shares_borrowed_environment(&self, argument: &TypedExpr, value: &Value) -> bool {
        (matches!(argument.kind, E::BorrowOperand(_)) && !value.loans.is_empty())
            || value.loans.iter().any(|id| {
                !self.loans[*id].parents.is_empty()
                    || self.external.contains(&self.loans[*id].place.root)
            })
    }

    fn is_copy(&self, ty: &Type) -> bool {
        if self.module.types().recursive(ty) || ty.is_noncopy_record(&self.module.types()) {
            return false;
        }
        match ty {
            Type::Variable(name) => self.copy_variables.contains(name),
            Type::Array(element) | Type::List(element) | Type::FixedArray(element, _) => {
                self.is_copy(element)
            }
            Type::Tuple(elements) => elements.iter().all(|ty| self.is_copy(ty)),
            Type::Record(id, arguments) if !arguments.is_empty() => self
                .module
                .types()
                .record_fields(*id, arguments)
                .iter()
                .all(|ty| self.is_copy(ty)),
            Type::Union(id, arguments) if !arguments.is_empty() => self
                .module
                .types()
                .union_payloads(*id, arguments)
                .iter()
                .flatten()
                .all(|ty| self.is_copy(ty)),
            _ => ty.is_copy(&self.module.types()),
        }
    }

    fn require_copy(&mut self, ty: &Type) -> bool {
        if ty.is_noncopy_record(&self.module.types()) {
            return false;
        }
        if self.module.types().recursive(ty) {
            return false;
        }
        if !self.infer {
            return false;
        }
        match ty {
            Type::Variable(name) => {
                self.copy_variables.insert(name.clone());
                true
            }
            Type::Array(element) | Type::List(element) | Type::FixedArray(element, _) => {
                self.require_copy(element)
            }
            Type::Tuple(elements) => {
                let mut changed = false;
                for ty in elements {
                    changed |= self.require_copy(ty);
                }
                changed
            }
            Type::Record(id, arguments) if !arguments.is_empty() => {
                let mut changed = false;
                for ty in self.module.types().record_fields(*id, arguments) {
                    changed |= self.require_copy(&ty);
                }
                changed
            }
            Type::Union(id, arguments) if !arguments.is_empty() => {
                let mut changed = false;
                for ty in self
                    .module
                    .types()
                    .union_payloads(*id, arguments)
                    .into_iter()
                    .flatten()
                {
                    changed |= self.require_copy(&ty);
                }
                changed
            }
            _ => false,
        }
    }

    fn revive_generic_moves(&mut self, place: &Place) {
        let moved: Vec<_> = self
            .state
            .generic_moves
            .iter()
            .filter(|(moved, _)| moved.overlaps(place))
            .map(|(moved, ty)| (moved.clone(), ty.clone()))
            .collect();
        for (moved, ty) in moved {
            if self.require_copy(&ty) {
                self.state.moved.remove(&moved);
                self.state.generic_moves.remove(&moved);
            }
        }
    }

    fn loan(
        &mut self,
        place: Place,
        mutable: bool,
        mut parents: BTreeSet<usize>,
        span: Span,
    ) -> usize {
        if let Some((_, value)) = self.state.locals.get(&place.root) {
            parents.extend(&value.loans);
        }
        let id = self.loans.len();
        self.loans.push(Loan {
            place,
            mutable,
            parents,
            view: None,
            span,
        });
        id
    }

    /// The loans that a parameter's value holds from its caller: one per region slot (A12 D7).
    /// Exclusive references and the exclusive regions of records allow writes (A13). A parameter
    /// written `ref {r} T {s}` also has a loan for the borrows inside its target, which reads of
    /// borrowed data through the reference return (A13 Phase 2).
    #[inline(never)]
    fn external_loans(
        &mut self,
        parameter: &Local,
        index: usize,
        slots: Option<&RegionSlots>,
        value: &mut Value,
    ) {
        let root = usize::MAX - parameter.id;
        self.external.insert(root);
        if !parameter.ty.contains_stored_reference(&self.module.types()) {
            self.placeholders.insert(root);
        }
        let target = slots.is_some_and(|slots| slots.targets.get(index) == Some(&true));
        let count = if target {
            2
        } else {
            self.region_count(&parameter.ty)
        };
        let (exclusive, targets) = match &parameter.ty {
            Type::Reference(_, mutable) => (RegionMask::from(*mutable), vec![2]),
            Type::Record(id, _) => {
                let record = &self.module.records[*id];
                (record.exclusive_regions, record.region_targets.clone())
            }
            _ => (0, Vec::new()),
        };
        let mut loans = Vec::new();
        for slot in 0..count {
            let fields = if count >= 2 {
                vec![region_field(slot)]
            } else {
                Vec::new()
            };
            let place = Place {
                root,
                fields,
                through_drop: false,
            };
            let mutable = exclusive & (1 << slot) != 0;
            loans.push(self.loan(place, mutable, BTreeSet::new(), parameter.span));
        }
        if target {
            value.loans = BTreeSet::from([loans[0]]);
        } else {
            value.loans = loans.iter().copied().collect();
            if count >= 2 {
                value.regions = Some(loans.iter().map(|id| BTreeSet::from([*id])).collect());
            }
        }
        let regions = slots
            .and_then(|slots| slots.parameters.get(index))
            .filter(|regions| regions.len() == count);
        for (slot, region) in regions.into_iter().flatten().enumerate() {
            self.slots.entry((root, slot)).or_default().region = *region;
        }
        if count < 2 || (!target && targets.len() != count) {
            return;
        }
        for (slot, mask) in targets.iter().enumerate() {
            let entry = self.slots.entry((root, slot)).or_default();
            for target in mask_slots(*mask).filter(|target| *target < count) {
                entry.target.insert(loans[target]);
                entry
                    .target_regions
                    .extend(regions.and_then(|regions| regions[target]));
            }
        }
    }

    /// A returned borrow comes from a parameter and, under a named result region, from an
    /// input slot with that region. Loans are checked in id order. The target slot of a result
    /// written `ref {r} T {s}` holds no loans of the result's own (A13 Phase 2).
    #[inline(never)]
    fn check_result(
        &self,
        parameters: impl Clone + Iterator<Item = &'a Local>,
        body: &TypedExpr,
        task: bool,
        contract: Contract<'_>,
        result: Value,
    ) -> Result<(), Diagnostic> {
        let region_sources = contract.result;
        let result_target = contract.slots.is_some_and(|slots| slots.result_target);
        let slots = self.slots(&result, &body.ty);
        for id in &result.loans {
            let place = &self.loans[*id].place;
            if self.reported.contains(&place.root) {
                continue;
            }
            // Static data lives for the whole run, so a result may borrow it (D11 Phase 2); a
            // task's result still holds no borrow.
            if is_static_root(place.root) && !task {
                continue;
            }
            if task || !self.external.contains(&place.root) {
                let diagnostic = error(
                    "E1013",
                    if task {
                        "task results cannot retain borrowed values"
                    } else {
                        "cannot return a reference to a local value"
                    },
                    body.span,
                );
                return Err(if task {
                    diagnostic
                } else {
                    self.escape(
                        *id,
                        body.span,
                        "'{name}' is declared here and is dropped when the function returns",
                        diagnostic,
                    )
                    .with_help(
                        "return the value itself, or return a reference that comes from a reference parameter",
                    )
                });
            }
            let Some(sources) = region_sources else {
                continue;
            };
            let owner = parameters
                .clone()
                .position(|parameter| usize::MAX - parameter.id == place.root);
            let slot = place_slot(place);
            for (index, allowed) in sources.iter().enumerate() {
                if result_target && index == 1 {
                    continue;
                }
                let held = slots
                    .is_none_or(|slots| slots.get(index).is_some_and(|loans| loans.contains(id)));
                if held && !owner.is_some_and(|owner| allowed.contains(&(owner, slot))) {
                    return Err(error(
                        "E1013",
                        "returned borrow does not match the declared result region; return a borrow from an input with that region",
                        body.span,
                    ));
                }
            }
        }
        Ok(())
    }

    /// The region slots of a value of `ty`: the regions of a record with several, otherwise 1.
    fn region_count(&self, ty: &Type) -> usize {
        match ty {
            Type::Record(id, _) if self.module.records[*id].region_count >= 2 => {
                self.module.records[*id].region_count
            }
            _ => 1,
        }
    }

    /// The loans of each region slot of `value`, when they partition its loans for `ty`.
    fn slots<'v>(&self, value: &'v Value, ty: &Type) -> Option<&'v [BTreeSet<usize>]> {
        let slots = value.regions.as_deref()?;
        let count = self.region_count(ty);
        (count >= 2
            && slots.len() == count
            && slots.iter().flatten().copied().collect::<BTreeSet<_>>() == value.loans)
            .then_some(slots)
    }

    /// The loans of the slots in `mask`, or every loan when `value` has no valid slots.
    fn slot_loans(&self, value: &Value, ty: &Type, mask: RegionMask) -> BTreeSet<usize> {
        match self.slots(value, ty) {
            Some(slots) => mask_slots(mask)
                .filter_map(|slot| slots.get(slot))
                .flatten()
                .copied()
                .collect(),
            None => value.loans.clone(),
        }
    }

    /// The value of the field path `path` of `value`, a value of type `root`, with type `ty`:
    /// only the loans of the slots that the path reaches, and their slots when `ty` has several.
    #[inline(never)]
    fn project(&self, value: &Value, root: &Type, path: &[usize], ty: &Type) -> Value {
        let Some(slots) = self.slots(value, root) else {
            return Value {
                loans: value.loans.clone(),
                regions: None,
                closed_result: value.closed_result,
            };
        };
        let records = &self.module.records;
        let types = self.module.types();
        // For each slot of the current type, the root slots it may hold.
        let mut maps: Vec<RegionMask> = (0..slots.len()).map(|slot| 1 << slot).collect();
        let mut current = root.clone();
        for &field in path {
            let next = match &current {
                Type::Record(id, arguments)
                    if records[*id].region_count >= 2 && field < records[*id].fields.len() =>
                {
                    let next = types.record_fields(*id, arguments).swap_remove(field);
                    let masks = &records[*id].field_regions[field];
                    let via = |mask: RegionMask| {
                        mask_slots(mask)
                            .fold(0, |all, slot| all | maps.get(slot).copied().unwrap_or(0))
                    };
                    maps = if masks.len() >= 2 && masks.len() == self.region_count(&next) {
                        masks.iter().map(|mask| via(*mask)).collect()
                    } else {
                        vec![masks.iter().fold(0, |all, mask| all | via(*mask))]
                    };
                    next
                }
                _ => {
                    maps = vec![maps.iter().fold(0, |all, mask| all | mask)];
                    break;
                }
            };
            current = next;
        }
        let loans = |mask: RegionMask| -> BTreeSet<usize> {
            mask_slots(mask)
                .filter_map(|slot| slots.get(slot))
                .flatten()
                .copied()
                .collect()
        };
        if maps.len() >= 2 && maps.len() == self.region_count(ty) {
            let regions: Box<[_]> = maps.iter().map(|mask| loans(*mask)).collect();
            Value {
                loans: regions.iter().flatten().copied().collect(),
                regions: Some(regions),
                closed_result: value.closed_result,
            }
        } else {
            Value {
                loans: loans(maps.iter().fold(0, |all, mask| all | mask)),
                regions: None,
                closed_result: value.closed_result,
            }
        }
    }

    /// The value that reading `place` yields from its owner's stored value: only the reached
    /// region slots of an immutable local with several regions (A12 D6).
    #[inline(never)]
    fn stored_value(&self, place: &Place, ty: &Type, single: bool) -> Option<Value> {
        let (local, stored) = self.state.locals.get(&place.root)?;
        Some(
            if single && !local.mutable && self.region_count(&local.ty) >= 2 {
                self.project(stored, &local.ty, &place.fields, ty)
            } else {
                Value {
                    loans: stored.loans.clone(),
                    ..Value::default()
                }
            },
        )
    }

    /// The value of a field read from the temporary record `record`.
    #[inline(never)]
    fn field_value(&self, expression: &TypedExpr, record: Value) -> Value {
        match &expression.kind {
            E::Field(inner, index) if self.region_count(&inner.ty) >= 2 => {
                self.project(&record, &inner.ty, &[*index], &expression.ty)
            }
            _ => Value {
                regions: None,
                ..record
            },
        }
    }

    /// A record literal or update: every field's loans, kept per region slot (A12).
    #[inline(never)]
    fn record_value(
        &mut self,
        expression: &TypedExpr,
        during: &BTreeSet<usize>,
    ) -> Result<Value, Diagnostic> {
        let module = self.module;
        let (base, fields) = match &expression.kind {
            E::Record(fields) => (None, fields.as_slice()),
            E::RecordUpdate { base, fields } => (Some(base.as_ref()), fields.as_slice()),
            _ => unreachable!("record values are literals or updates"),
        };
        let field_regions = match &expression.ty {
            Type::Record(id, _) if module.records[*id].region_count >= 2 => {
                Some(&module.records[*id].field_regions)
            }
            _ => None,
        };
        let mut slots =
            field_regions.map(|_| vec![BTreeSet::new(); self.region_count(&expression.ty)]);
        let mut result = Value::default();
        let start = self.held.len();
        let parts = base
            .map(|base| (None, base))
            .into_iter()
            .chain(fields.iter().map(|(index, value)| (Some(*index), value)));
        for (index, field) in parts {
            let value = self.eval(field, Use::Consume, during)?;
            if let (Some(slots), Some(field_regions)) = (slots.as_mut(), field_regions) {
                match index {
                    None => {
                        for (slot, loans) in slots.iter_mut().enumerate() {
                            loans.extend(self.slot_loans(&value, &expression.ty, 1 << slot));
                        }
                    }
                    Some(index) => {
                        let masks = &field_regions[index];
                        if masks.len() >= 2 && masks.len() == self.region_count(&field.ty) {
                            for (inner, mask) in masks.iter().enumerate() {
                                let loans = self.slot_loans(&value, &field.ty, 1 << inner);
                                for slot in mask_slots(*mask) {
                                    slots[slot].extend(&loans);
                                }
                            }
                        } else {
                            let mask = masks.iter().fold(0, |all, mask| all | mask);
                            for slot in mask_slots(mask) {
                                slots[slot].extend(&value.loans);
                            }
                        }
                    }
                }
            }
            result.loans.extend(&value.loans);
            self.held.push(value);
        }
        self.held.truncate(start);
        result.regions = slots.map(Vec::into_boxed_slice);
        Ok(result)
    }

    /// Adds to a direct call's result the loans of `argument` that its named result regions select.
    /// The target slot of a result written `ref {r} T {s}` gets none (A13 Phase 2).
    #[inline(never)]
    fn argument_regions(
        &self,
        sources: &RegionSources,
        slots: Option<&RegionSlots>,
        index: usize,
        argument: &TypedExpr,
        value: &Value,
        current: &mut Value,
    ) {
        let result_target = slots.is_some_and(|slots| slots.result_target);
        for (result_slot, allowed) in sources.iter().enumerate() {
            if result_target && result_slot == 1 {
                continue;
            }
            for &(input, slot) in allowed {
                if input != index {
                    continue;
                }
                let loans = self.input_slot_loans(slots, index, slot, argument, value);
                if let Some(slots) = current.regions.as_mut() {
                    slots[result_slot].extend(&loans);
                }
                current.loans.extend(loans);
            }
        }
    }

    fn retain_live_loans(&mut self, live: &BTreeSet<usize>) {
        let mut keep = live.clone();
        let mut pending: Vec<_> = self
            .state
            .locals
            .iter()
            .filter(|(id, _)| live.contains(id))
            .flat_map(|(_, (_, value))| value.loans.iter())
            .chain(self.held.iter().flat_map(|value| value.loans.iter()))
            .copied()
            .collect();
        let mut visited = BTreeSet::new();
        while let Some(id) = pending.pop() {
            if visited.insert(id) {
                keep.insert(self.loans[id].place.root);
                pending.extend(&self.loans[id].parents);
            }
        }
        for (id, (_, value)) in &mut self.state.locals {
            if !keep.contains(id) {
                value.loans.clear();
            }
        }
    }

    fn active(&self) -> BTreeSet<usize> {
        let mut active: BTreeSet<_> = self
            .state
            .locals
            .values()
            .flat_map(|(_, value)| value.loans.iter())
            .chain(self.held.iter().flat_map(|value| value.loans.iter()))
            .copied()
            .collect();
        let mut pending: Vec<_> = active.iter().copied().collect();
        while let Some(id) = pending.pop() {
            for parent in &self.loans[id].parents {
                if active.insert(*parent) {
                    pending.push(*parent);
                }
            }
        }
        active
    }

    fn report(&mut self, place: &Place, diagnostic: Diagnostic) -> Result<(), Diagnostic> {
        if !self.reported.insert(place.root) {
            return Ok(());
        }
        if self.recovered.len() >= MAX_UNIQUE_DIAGNOSTICS {
            return Err(diagnostic);
        }
        self.recovered.push(diagnostic);
        Ok(())
    }

    fn access(
        &mut self,
        place: &Place,
        via: &BTreeSet<usize>,
        usage: Use,
        span: Span,
    ) -> Result<(), Diagnostic> {
        if usage != Use::Write {
            self.revive_generic_moves(place);
        }
        if usage != Use::Write
            && let Some((moved, origin)) = self
                .state
                .moved
                .iter()
                .filter(|(moved, _)| moved.overlaps(place))
                .max_by_key(|(_, origin)| (origin.start, origin.end))
                .map(|(moved, origin)| (moved.clone(), *origin))
        {
            let name = self
                .state
                .locals
                .get(&place.root)
                .map_or("value", |(local, _)| local.name.as_str())
                .to_owned();
            // Only the control flow tells whether the move reached the use through a loop: the
            // `where` bindings of a guarded function run before its arms, and an assignment's value
            // before its place.
            let previous_iteration = origin.end > origin.start
                && moved_in_previous_iteration(self.body, origin, span, &mut self.later_use_budget)
                    == Some(true);
            let message = match (moved.fields.is_empty(), previous_iteration) {
                (true, false) => "value moved here",
                (false, false) => "part of the value moved here",
                (true, true) => "value moved here, in the previous iteration of the loop",
                (false, true) => {
                    "part of the value moved here, in the previous iteration of the loop"
                }
            };
            let mut diagnostic = error(
                "E1012",
                format!("use of moved or partially moved value '{name}'"),
                span,
            )
            .with_help(if moved.fields.is_empty() {
                format!("borrow the value with 'ref {name}' or move it last")
            } else {
                format!(
                    "borrow the field with 'ref' instead of moving it out of '{name}', or move it last"
                )
            });
            if origin.end > origin.start {
                diagnostic = diagnostic.related(RelatedRole::MovedHere, message, origin);
            }
            return self.report(place, diagnostic);
        }
        if usage == Use::Write && Some(place.root) == self.drop_root {
            return Err(error(
                "E1012",
                if place.fields.is_empty() {
                    "cannot replace the whole value inside Drop.drop; read or borrow its fields instead"
                } else {
                    "cannot assign to a field or element inside Drop.drop; read or borrow its fields instead"
                },
                span,
            ));
        }
        // Replacing the value reruns this drop, and a `ref mut` callee could replace it too.
        if place.fields.is_empty() {
            // A move of the parameter (`usize::MAX - drop_root`) passes the reference on.
            if (usage == Use::MutBorrow && Some(place.root) == self.drop_root)
                || (usage == Use::Consume && Some(usize::MAX - place.root) == self.drop_root)
            {
                return Err(error(
                    "E1012",
                    "cannot pass the value on as 'ref mut' inside Drop.drop; read it or borrow it with 'ref' instead",
                    span,
                ));
            }
        }
        if matches!(usage, Use::MutBorrow | Use::Write) {
            // Only the loans that lead to the place are on its path; a reference's loan also
            // keeps its owner's stored loans as parents, and those point elsewhere.
            let mut path = via
                .iter()
                .filter(|id| self.loans[**id].place.overlaps(place))
                .peekable();
            let mutable = if via.is_empty() {
                self.state
                    .locals
                    .get(&place.root)
                    .is_some_and(|(local, _)| local.mutable)
            } else if path.peek().is_some() {
                path.all(|id| self.loans[*id].mutable)
            } else {
                via.iter().all(|id| self.loans[*id].mutable)
            };
            if !mutable || (usage != Use::Write && !unmarked_fields(place).is_empty()) {
                let diagnostic = error(
                    "E1014",
                    "mutable access requires 'let mut' or an exclusive reference ('ref mut' or '&mut'); record fields, array elements, and list elements are immutable",
                    span,
                );
                let declared = self
                    .state
                    .locals
                    .get(&place.root)
                    .map(|(local, _)| *local)
                    .filter(|local| {
                        via.is_empty()
                            && !local.mutable
                            && unmarked_fields(place).is_empty()
                            && local.span.end > local.span.start
                            && !local.name.starts_with('$')
                    });
                let diagnostic = match declared {
                    Some(local) => diagnostic
                        .related(
                            RelatedRole::DeclaredHere,
                            format!("'{}' is declared here without 'mut'", local.name),
                            local.span,
                        )
                        .with_help(format!(
                            "declare '{}' with 'let mut', or take it by 'ref mut'",
                            local.name
                        )),
                    None => diagnostic.with_help(
                        "write through a 'let mut' variable that holds the whole value, or take it by 'ref mut'",
                    ),
                };
                return self.report(place, diagnostic);
            }
        }
        for id in self.active() {
            let loan = &self.loans[id];
            if via.contains(&id) || !loan.place.overlaps(place) {
                continue;
            }
            if loan.mutable || matches!(usage, Use::Consume | Use::MutBorrow | Use::Write) {
                if let Some(ty) = loan.view.clone() {
                    self.require_copy(&ty);
                    if self.is_copy(&ty) {
                        continue;
                    }
                }
                // `conflict` searches the body, so a root that is reported already is not searched again.
                if self.reported.contains(&place.root) {
                    return Ok(());
                }
                let diagnostic = self.conflict(id, span);
                return self.report(place, diagnostic);
            }
        }
        Ok(())
    }

    /// Names where the owner of `loan` is declared (`owner_message`, with `{name}` for its name)
    /// and where the loan was taken, in an `E1013` that says the borrow outlives its owner.
    #[cold]
    #[inline(never)]
    fn escape(
        &self,
        loan: usize,
        primary: Span,
        owner_message: &str,
        mut diagnostic: Diagnostic,
    ) -> Diagnostic {
        let loan = &self.loans[loan];
        if let Some(owner) = self.declarations.get(&loan.place.root)
            && owner.span.end > owner.span.start
            && !owner.name.starts_with('$')
        {
            diagnostic.add_related(
                RelatedRole::DeclaredHere,
                owner_message.replace("{name}", &owner.name),
                owner.span,
            );
        }
        if loan.span.end > loan.span.start && loan.span != primary {
            diagnostic.add_related(RelatedRole::BorrowedHere, "value borrowed here", loan.span);
        }
        diagnostic
    }

    /// The `E1014` of an access at `span` that conflicts with the live loan `loan`: where the loan
    /// was taken, and where the reference that keeps it alive is used later.
    #[cold]
    #[inline(never)]
    fn conflict(&mut self, loan: usize, span: Span) -> Diagnostic {
        let mut diagnostic = error(
            "E1014",
            "access conflicts with a live borrow; use the reference or end its last use before moving, replacing, or borrowing exclusively",
            span,
        );
        let created = &self.loans[loan];
        // A parameter's own loan was taken by the caller; a reborrow of it has parents.
        if !(self.external.contains(&created.place.root) && created.parents.is_empty())
            && created.span.end > created.span.start
            && created.span != span
        {
            diagnostic = diagnostic.related(
                RelatedRole::BorrowedHere,
                if created.mutable {
                    "value mutably borrowed here"
                } else {
                    "value borrowed here"
                },
                created.span,
            );
        }
        let holders = self.holders(loan);
        let use_of = self.later_use(loan, &holders, span);
        // The help names the holder whose use is shown, else the first one.
        let named = |id: &usize| {
            self.state
                .locals
                .get(id)
                .map(|(local, _)| local.name.as_str())
                .filter(|name| !name.starts_with('$'))
        };
        let holder = use_of
            .and_then(|(_, _, id)| named(&id))
            .or_else(|| holders.iter().find_map(named))
            .map(str::to_owned);
        if let Some((used, repeated, _)) = use_of {
            diagnostic = diagnostic.related(
                RelatedRole::BorrowUsedHere,
                if repeated {
                    "the borrow is used here, in the next iteration of the loop, so it is still alive"
                } else {
                    "the borrow is used here, so it is still alive"
                },
                used,
            );
        }
        match holder {
            Some(name) => diagnostic.with_help(format!(
                "end the borrow before this access: stop using '{name}' first, or move this access after its last use"
            )),
            None => diagnostic.with_help(
                "end the borrow before this access: evaluate this access before the borrow is taken, or borrow a copy",
            ),
        }
    }

    /// Whether an argument held by a call that is being evaluated keeps `loan` alive.
    fn held_keeps(&self, loan: usize) -> bool {
        let mut pending: Vec<usize> = self
            .held
            .iter()
            .flat_map(|value| value.loans.iter().copied())
            .collect();
        let mut seen = BTreeSet::new();
        while let Some(current) = pending.pop() {
            if current == loan {
                return true;
            }
            if seen.insert(current) {
                pending.extend(&self.loans[current].parents);
            }
        }
        false
    }

    /// The locals whose values keep `loan` alive, directly or through the parents of a loan they hold.
    fn holders(&self, loan: usize) -> BTreeSet<usize> {
        let mut holders = BTreeSet::new();
        for (id, (_, value)) in &self.state.locals {
            let mut pending: Vec<usize> = value.loans.iter().copied().collect();
            let mut seen = BTreeSet::new();
            while let Some(current) = pending.pop() {
                if !seen.insert(current) {
                    continue;
                }
                if current == loan {
                    holders.insert(*id);
                    break;
                }
                pending.extend(&self.loans[current].parents);
            }
        }
        holders
    }

    /// Where a holder keeps `loan` alive after an access at `after` (`find_later_use`): a later
    /// use, else (`true`) an earlier use in a loop that holds the access too, unless an argument
    /// held by a call keeps the loan alive instead.
    fn later_use(
        &mut self,
        loan: usize,
        holders: &BTreeSet<usize>,
        after: Span,
    ) -> Option<(Span, bool, usize)> {
        let held = self.held_keeps(loan);
        let created = (self.loans[loan].span, self.loans[loan].mutable);
        find_later_use(
            self.body,
            holders,
            after,
            created,
            held,
            &mut self.later_use_budget,
        )
    }

    fn place(
        &mut self,
        expression: &TypedExpr,
        live: &BTreeSet<usize>,
    ) -> Result<Vec<(Place, BTreeSet<usize>)>, Diagnostic> {
        match &expression.kind {
            E::Local(id) if self.state.aliases.contains_key(id) => {
                Ok(self.state.aliases[id].clone())
            }
            E::Local(id) => Ok(vec![(
                Place {
                    root: *id,
                    fields: Vec::new(),
                    through_drop: false,
                },
                BTreeSet::new(),
            )]),
            E::Static(id) => Ok(vec![(
                Place {
                    root: STATIC_ROOT + 1 + *id,
                    fields: Vec::new(),
                    through_drop: false,
                },
                BTreeSet::new(),
            )]),
            E::Field(value, field) => {
                let through_drop = value.ty.has_user_drop(&self.module.types());
                let mut places = self.place(value, live)?;
                for (place, _) in &mut places {
                    place.fields.push(*field);
                    place.through_drop |= through_drop;
                }
                Ok(places)
            }
            E::UnionPayload { value, .. } => {
                let through_drop = value.ty.has_user_drop(&self.module.types());
                let mut places = self.place(value, live)?;
                for (place, _) in &mut places {
                    place.fields.push(PAYLOAD);
                    place.through_drop |= through_drop;
                }
                Ok(places)
            }
            E::ListTail(value, _) => {
                let mut places = self.place(value, live)?;
                for (place, _) in &mut places {
                    place.fields.push(ELEMENT);
                }
                Ok(places)
            }
            E::Index(value, index)
                if matches!(
                    value.ty,
                    Type::Array(_) | Type::List(_) | Type::Vec(_) | Type::FixedArray(..)
                ) =>
            {
                let mut places = self.place(value, live)?;
                let mut guard = Value::default();
                for (place, via) in &places {
                    self.access(place, via, Use::Borrow, value.span)?;
                    guard
                        .loans
                        .insert(self.loan(place.clone(), false, via.clone(), value.span));
                }
                self.held.push(guard);
                self.eval(index, Use::Consume, live)?;
                self.held.pop();
                for (place, _) in &mut places {
                    // Collection loans conservatively cover every element.
                    place.fields.push(ELEMENT);
                }
                Ok(places)
            }
            E::Dereference(reference) => {
                let value = self.eval(reference, Use::Read, live)?;
                let mut places = Vec::new();
                for id in &value.loans {
                    let loan = &self.loans[*id];
                    let mut via = loan.parents.clone();
                    via.insert(*id);
                    places.push((loan.place.clone(), via));
                }
                if places.is_empty() {
                    if matches!(reference.kind, E::Local(id) if self.reported.contains(&id)) {
                        return Ok(places);
                    }
                    return Err(error(
                        "E1013",
                        "reference has no live owner",
                        expression.span,
                    ));
                }
                Ok(places)
            }
            _ => Err(error(
                "E1013",
                "borrow requires a local place; bind the temporary with 'let' first",
                expression.span,
            )),
        }
    }

    /// A13 M8: a write or an exclusive reborrow cannot pass through a shared reference to data
    /// that holds a record with exclusive regions. The type checker sees only the outermost
    /// reference, and a borrowed record's field reads keep the owner's loans, not the shared loan.
    #[inline(never)]
    fn shared_exclusive(&self, mut target: &TypedExpr, span: Span) -> Result<(), Diagnostic> {
        let types = self.module.types();
        loop {
            target = match &target.kind {
                E::Dereference(reference) => {
                    if let Type::Reference(inner, false) = &reference.ty
                        && inner.holds_exclusive_record(&types)
                    {
                        return Err(error(
                            "E1014",
                            "cannot mutate or exclusively reborrow through a shared reference",
                            span,
                        ));
                    }
                    reference
                }
                E::Field(value, _)
                | E::Index(value, _)
                | E::ListTail(value, _)
                | E::UnionPayload { value, .. } => value,
                _ => return Ok(()),
            };
        }
    }

    /// An exclusive slice `ref mut source[start..end]` (C08). The source descriptor is read first,
    /// so a shared guard covers the bounds, which may read but not change it; the exclusive loan
    /// starts after them.
    #[inline(never)]
    fn exclusive_slice(
        &mut self,
        source: &TypedExpr,
        start: &Option<Box<TypedExpr>>,
        end: &Option<Box<TypedExpr>>,
        during: &BTreeSet<usize>,
        span: Span,
    ) -> Result<Value, Diagnostic> {
        self.shared_exclusive(source, span)?;
        let places = self.exclusive_places(source, during)?;
        let mut guard = Value::default();
        for (place, via) in &places {
            self.access(place, via, Use::Borrow, span)?;
            guard
                .loans
                .insert(self.loan(place.clone(), false, via.clone(), span));
        }
        self.held.push(guard);
        for bound in start.iter().chain(end) {
            self.eval(bound, Use::Consume, during)?;
        }
        self.held.pop();
        let mut result = Value::default();
        for (place, via) in places {
            self.access(&place, &via, Use::MutBorrow, span)?;
            result.loans.insert(self.loan(place, true, via, span));
        }
        Ok(result)
    }

    /// The places that a write or an exclusive borrow of `target` reaches (A13). A dereferenced
    /// value that holds exclusive loans refers to their places; its shared loans come from the
    /// shared fields of a record or from the other arguments of a call that it was merged with.
    #[inline(never)]
    fn exclusive_places(
        &mut self,
        target: &TypedExpr,
        live: &BTreeSet<usize>,
    ) -> Result<Vec<(Place, BTreeSet<usize>)>, Diagnostic> {
        let places = self.place(target, live)?;
        if !matches!(target.kind, E::Dereference(_)) {
            return Ok(places);
        }
        let exclusive = |(place, via): &(Place, BTreeSet<usize>)| {
            let mut path = via
                .iter()
                .filter(|id| self.loans[**id].place.overlaps(place))
                .peekable();
            path.peek().is_some() && path.all(|id| self.loans[*id].mutable)
        };
        if places.iter().any(exclusive) {
            Ok(places
                .into_iter()
                .filter(|place| exclusive(place))
                .collect())
        } else {
            Ok(places)
        }
    }

    /// A13 Phase 2: stores a value with `value`'s loans in `place` through a reference. A local
    /// owner keeps the new loans with its old ones; a parameter's target accepts only borrows of
    /// the inputs with the region that its type names for the target, which callers keep.
    #[inline(never)]
    fn store_through(
        &mut self,
        place: &Place,
        value: &Value,
        span: Span,
    ) -> Result<(), Diagnostic> {
        const UNNAMED: &str =
            "assigning borrowed values through references requires explicit lifetimes";
        if self.external.contains(&place.root) {
            let Some(target) = self
                .slots
                .get(&(place.root, place_slot(place)))
                .filter(|slot| !slot.target_regions.is_empty())
            else {
                return Err(error(
                    "E1013",
                    format!(
                        "{UNNAMED}; name the target's region apart from the reference's, as in 'ref mut {{r}} T {{s}}', and store borrows of inputs with region 's'"
                    ),
                    span,
                ));
            };
            for id in &value.loans {
                let place = &self.loans[*id].place;
                // Static data outlives the caller's data, so storing a borrow of it is fine.
                if is_static_root(place.root) {
                    continue;
                }
                if !self.external.contains(&place.root) {
                    return Err(error(
                        "E1013",
                        "cannot store a borrow of a local value through a reference parameter; the caller's data would outlive it",
                        span,
                    ));
                }
                let region = self
                    .slots
                    .get(&(place.root, place_slot(place)))
                    .and_then(|slot| slot.region);
                if !region.is_some_and(|region| target.target_regions.contains(&region)) {
                    return Err(error(
                        "E1013",
                        "the stored borrow does not have the region of the reference's target; store a borrow of an input with that region",
                        span,
                    ));
                }
            }
            return Ok(());
        }
        let Some((_, owner)) = self.state.locals.get_mut(&place.root) else {
            return Err(error("E1013", UNNAMED, span));
        };
        owner.loans.extend(&value.loans);
        owner.regions = None;
        for (owned, stored) in owner.closed_result.iter_mut().zip(value.closed_result) {
            *owned &= stored;
        }
        Ok(())
    }

    /// A13 Phase 2: the loans of the borrows inside the targets of the references `references`:
    /// a local owner's stored loans, or a parameter's target loans.
    fn target_loans(&self, references: &BTreeSet<usize>) -> BTreeSet<usize> {
        let mut loans = BTreeSet::new();
        for id in references {
            let place = &self.loans[*id].place;
            if let Some((_, value)) = self.state.locals.get(&place.root) {
                loans.extend(&value.loans);
            } else if let Some(slot) = self
                .slots
                .get(&(place.root, place_slot(place)))
                .filter(|slot| !slot.target.is_empty())
            {
                loans.extend(&slot.target);
            } else {
                loans.insert(*id);
            }
        }
        loans
    }

    /// The loans that slot `slot` of the input `index` passes to a call of a function with named
    /// regions: the slot's own loans, and the borrows inside the targets of the references whose
    /// targets have the slot's region (A13 Phase 2).
    fn input_slot_loans(
        &self,
        slots: Option<&RegionSlots>,
        index: usize,
        slot: usize,
        argument: &TypedExpr,
        value: &Value,
    ) -> BTreeSet<usize> {
        if slots.is_some_and(|slots| slots.targets.get(index) == Some(&true)) {
            return if slot == 0 {
                value.loans.clone()
            } else {
                self.target_loans(&value.loans)
            };
        }
        let mut loans = self.slot_loans(value, &argument.ty, 1 << slot);
        if let Type::Record(id, _) = &argument.ty {
            for (references, targets) in self.module.records[*id].region_targets.iter().enumerate()
            {
                if targets & (1 << slot) != 0 {
                    let references = self.slot_loans(value, &argument.ty, 1 << references);
                    loans.extend(self.target_loans(&references));
                }
            }
        }
        loans
    }

    /// A13 Phase 2: a direct call of `function` may store the inputs with a target's region in the
    /// targets of its exclusive references, so their owners keep those loans. The callee value
    /// and the arguments are on `held` from `start`.
    #[inline(never)]
    fn store_arguments(
        &mut self,
        function: usize,
        arguments: &[TypedExpr],
        start: usize,
        span: Span,
    ) -> Result<(), Diagnostic> {
        let module = self.module;
        let function = &module.functions[function];
        let Some(slots) = function.region_slots.as_ref().filter(|slots| slots.writes) else {
            return Ok(());
        };
        for (index, parameter) in function.parameters.iter().enumerate() {
            for (slot, regions) in slots.written_targets(index, &parameter.ty, module.types()) {
                let value = &self.held[start + 1 + index];
                let references = if slots.targets[index] {
                    value.loans.clone()
                } else {
                    self.slot_loans(value, &arguments[index].ty, 1 << slot)
                };
                let mut stored = Value::default();
                for (input, input_slots) in slots.parameters.iter().enumerate() {
                    for (input_slot, region) in input_slots.iter().enumerate() {
                        if region.is_some_and(|region| regions.contains(&region)) {
                            stored.loans.extend(self.input_slot_loans(
                                Some(slots),
                                input,
                                input_slot,
                                &arguments[input],
                                &self.held[start + 1 + input],
                            ));
                        }
                    }
                }
                for reference in references {
                    let loan = &self.loans[reference];
                    if !loan.mutable {
                        continue;
                    }
                    let place = loan.place.clone();
                    if stored
                        .loans
                        .iter()
                        .any(|id| *id != reference && self.loans[*id].place.overlaps(&place))
                    {
                        return Err(error(
                            "E1013",
                            "cannot store a reference inside its own owner",
                            span,
                        ));
                    }
                    self.store_through(&place, &stored, span)?;
                }
            }
        }
        Ok(())
    }

    fn is_place(expression: &TypedExpr) -> bool {
        match &expression.kind {
            E::Local(_) | E::Dereference(_) => true,
            E::Field(value, _) | E::ListTail(value, _) | E::UnionPayload { value, .. } => {
                Self::is_place(value)
            }
            E::Index(value, _) => {
                matches!(
                    value.ty,
                    Type::Array(_) | Type::List(_) | Type::Vec(_) | Type::FixedArray(..)
                ) && Self::is_place(value)
            }
            _ => false,
        }
    }

    fn read_place(
        &mut self,
        expression: &TypedExpr,
        usage: Use,
        live: &BTreeSet<usize>,
    ) -> Result<Value, Diagnostic> {
        let places = self.place(expression, live)?;
        self.read_places(expression, usage, places)
    }

    /// Notes that the value of `expression` is copied when it is consumed (A15).
    #[inline(never)]
    fn note_copy(&mut self, expression: &TypedExpr, span: Span, kind: CopyKind, whole: bool) {
        if self.copies.is_none()
            || !self.is_copy(&expression.ty)
            || !expression.ty.needs_drop(&self.module.types())
        {
            return;
        }
        let local = whole
            .then(|| crate::llvm::call_specialization::field_root(expression))
            .flatten()
            .filter(|(id, ty)| {
                !self.state.aliases.contains_key(id)
                    && (matches!(expression.kind, E::Local(_)) || ty.is_copy(&self.module.types()))
            })
            .map(|(id, _)| id);
        let ty = expression.ty.clone();
        if let Some(copies) = self.copies.as_mut() {
            copies.push(CopyRead {
                span,
                ty,
                kind,
                local,
            });
        }
    }

    fn read_places(
        &mut self,
        expression: &TypedExpr,
        usage: Use,
        places: Vec<(Place, BTreeSet<usize>)>,
    ) -> Result<Value, Diagnostic> {
        if usage == Use::Consume {
            self.note_copy(expression, expression.span, read_kind(expression), true);
        }
        let mut moving = usage == Use::Consume && !self.is_copy(&expression.ty);
        if moving
            && matches!(
                expression.kind,
                E::Index(..) | E::ListTail(..) | E::Dereference(_)
            )
            && self.require_copy(&expression.ty)
        {
            moving = false;
        }
        if moving && matches!(expression.kind, E::Index(..)) {
            return Err(error(
                "E1012",
                "cannot move a non-Copy element out of an array or list; borrow the element instead",
                expression.span,
            ));
        }
        let mut value = Value {
            closed_result: std::array::from_fn(|index| {
                places.iter().all(|(place, _)| {
                    place.fields.is_empty()
                        && self
                            .state
                            .locals
                            .get(&place.root)
                            .is_some_and(|(_, value)| value.closed_result[index])
                })
            }),
            ..Value::default()
        };
        let single = places.len() == 1;
        for (place, via) in places {
            self.revive_generic_moves(&place);
            if moving
                && (!via.is_empty()
                    || place.fields.contains(&ELEMENT)
                    || place.through_drop
                    || self
                        .active()
                        .iter()
                        .any(|id| self.loans[*id].place.overlaps(&place)))
            {
                self.require_copy(&expression.ty);
            }
            moving = usage == Use::Consume && !self.is_copy(&expression.ty);
            if moving && place.fields.contains(&ELEMENT) {
                return Err(error(
                    "E1012",
                    "cannot move a non-Copy value out of an array or list element; borrow it instead",
                    expression.span,
                ));
            }
            if moving && !via.is_empty() {
                return Err(error(
                    "E1012",
                    if via.iter().any(|id| self.loans[*id].view.is_some()) {
                        "cannot move a non-Copy pattern variable bound through a reference or a collection element; borrow it with 'ref' instead"
                    } else {
                        "cannot move a non-Copy value out of a reference"
                    },
                    expression.span,
                ));
            }
            if moving && place.through_drop {
                return Err(error("E1012", MOVE_OUT_OF_DROP, expression.span));
            }
            self.access(
                &place,
                &via,
                if moving { Use::Consume } else { Use::Read },
                expression.span,
            )?;
            if expression.ty.carries_loans(&self.module.types()) {
                if let Some(stored) = self.stored_value(&place, &expression.ty, single) {
                    value.loans.extend(stored.loans);
                    value.regions = stored.regions;
                } else if self.external.contains(&place.root) {
                    // Borrowed data behind a reference with a named target region has that region.
                    match self
                        .slots
                        .get(&(place.root, place_slot(&place)))
                        .filter(|slot| !slot.target.is_empty())
                    {
                        Some(slot) => value.loans.extend(&slot.target),
                        None => value.loans.extend(&via),
                    }
                } else {
                    return Err(error(
                        "E1013",
                        "nested borrowed values require explicit lifetime parameters, which are not supported",
                        expression.span,
                    ));
                }
                if moving
                    && self
                        .active()
                        .iter()
                        .any(|id| !self.loans[*id].parents.is_disjoint(&value.loans))
                {
                    return Err(error(
                        "E1014",
                        "cannot move an exclusive reference while it is reborrowed",
                        expression.span,
                    ));
                }
            } else if usage == Use::Read {
                value
                    .loans
                    .insert(self.loan(place.clone(), false, via, expression.span));
            }
            if moving {
                self.state.moved.insert(place.clone(), expression.span);
                if self.infer {
                    self.state
                        .generic_moves
                        .insert(place.clone(), expression.ty.clone());
                }
                if place.fields.is_empty() {
                    self.state
                        .locals
                        .get_mut(&place.root)
                        .unwrap()
                        .1
                        .loans
                        .clear();
                }
            }
        }
        Ok(value)
    }

    fn eval(
        &mut self,
        expression: &TypedExpr,
        usage: Use,
        live: &BTreeSet<usize>,
    ) -> Result<Value, Diagnostic> {
        let value = match expression.kind {
            E::While { .. } | E::ForRange { .. } | E::ForEach { .. } | E::Match { .. } => {
                self.eval_control(expression, live)
            }
            E::Block { .. } => self.eval_block(expression, live),
            E::Call(..) => self.eval_call(expression, live),
            E::Lambda { .. } => self.eval_lambda(expression, live),
            _ => self.eval_value(expression, usage, live),
        }?;
        if expression.ty.is_async(&self.module.types())
            && value
                .loans
                .iter()
                .any(|loan| !self.placeholders.contains(&self.loans[*loan].place.root))
        {
            return Err(error("E1013", ASYNC_BORROW, expression.span));
        }
        Ok(value)
    }

    fn eval_block(
        &mut self,
        expression: &TypedExpr,
        live: &BTreeSet<usize>,
    ) -> Result<Value, Diagnostic> {
        let E::Block {
            bindings,
            result: tail,
        } = &expression.kind
        else {
            unreachable!("eval dispatches blocks here")
        };
        let mut future = BTreeMap::new();
        count_uses(tail, &mut future);
        for (_, value) in bindings {
            count_uses(value, &mut future);
        }
        let mut ids = BTreeSet::new();
        for (local, value) in bindings {
            let mut current = live.clone();
            current.extend(future.keys().copied());
            let value_result = self.eval(value, Use::Consume, &current)?;
            let mut consumed = BTreeMap::new();
            count_uses(value, &mut consumed);
            for (id, count) in consumed {
                let remaining = future.get_mut(&id).unwrap();
                *remaining -= count;
                if *remaining == 0 {
                    future.remove(&id);
                }
            }
            self.state
                .locals
                .insert(local.id, (self.declarations[&local.id], value_result));
            self.state.moved.retain(|place, _| place.root != local.id);
            self.state
                .generic_moves
                .retain(|place, _| place.root != local.id);
            ids.insert(local.id);
            let mut keep = live.clone();
            keep.extend(future.keys().copied());
            self.retain_live_loans(&keep);
        }
        let result = self.eval(tail, Use::Consume, live)?;
        for id in &result.loans {
            if ids.contains(&self.loans[*id].place.root)
                && !self.reported.contains(&self.loans[*id].place.root)
            {
                return Err(self
                    .escape(
                        *id,
                        tail.span,
                        "'{name}' is declared here and dropped at the end of this block",
                        error(
                            "E1013",
                            "borrowed value does not live long enough to leave this block",
                            tail.span,
                        ),
                    )
                    .with_help(
                        "return the value itself instead of a reference to it, or declare it in an outer scope",
                    ));
            }
        }
        for (id, (_, value)) in &self.state.locals {
            if !ids.contains(id)
                && let Some(loan) = value.loans.iter().find(|loan| {
                    ids.contains(&self.loans[**loan].place.root)
                        && !self.reported.contains(&self.loans[**loan].place.root)
                })
            {
                return Err(self
                    .escape(
                        *loan,
                        expression.span,
                        "'{name}' is declared here and dropped at the end of this block",
                        error(
                            "E1013",
                            "assignment lets a reference outlive its owner",
                            expression.span,
                        ),
                    )
                    .with_help(
                        "declare the owner before the variable that keeps the reference, in the same block or an outer one",
                    ));
            }
        }
        for id in ids {
            self.state.locals.remove(&id);
        }
        Ok(result)
    }

    fn eval_call(
        &mut self,
        expression: &TypedExpr,
        live: &BTreeSet<usize>,
    ) -> Result<Value, Diagnostic> {
        let E::Call(callee, arguments) = &expression.kind else {
            unreachable!("eval dispatches calls here")
        };
        let mut during = live.clone();
        uses(expression, &mut during);
        let mut result = Value::default();
        let start = self.held.len();
        // A direct call of `Mutex.with_lock` checks its callback's result with its arguments below,
        // and one of `Channel.send` checks its item; any other use of either builtin as a value is
        // checked where it is evaluated.
        let locked = self.locked_result(callee).filter(|_| arguments.len() >= 2);
        let sent = self.sent_item(callee).filter(|_| arguments.len() >= 2);
        let received = self.builtin_called(callee) == Some(crate::check::Builtin::ChannelRecv);
        let value = if locked.is_some() || sent.is_some() {
            Value::default()
        } else {
            self.eval(callee, Use::Consume, &during)?
        };
        let (known, region_sources, callback) = self.call_contract(callee, arguments)?;
        let module = self.module;
        let slots = known
            .filter(|_| !callback)
            .and_then(|id| module.functions[id].region_slots.as_ref());
        let boundary = known
            .map(|id| self.module.functions[id].parameters.len())
            .filter(|count| *count <= arguments.len());
        // A region-quantified parameter's result borrows only from the inputs that its type names.
        let mut current = if callback {
            Value::default()
        } else {
            value.clone()
        };
        current.regions = region_sources.and_then(call_slots);
        self.held.push(value);
        for (index, argument) in arguments.iter().enumerate() {
            let value = self.eval(argument, Use::Consume, &during)?;
            // The callback runs under the lock, so a result that holds a borrow of the locked value
            // would use it after the lock is released: the value may be replaced and freed, or be
            // written by another task, by then. Only a result that is proven free of borrows made
            // inside the callback may hold borrows at all (F10). The generated function that
            // applies the builtin to its own parameters proves nothing: its callers are checked.
            if index == 1
                && let Some(returned) = &locked
                && returned.carries_loans(&self.module.types())
                && !value.closed_result[0]
                && !self.is_generated_parameter(argument)
            {
                return Err(error(
                    "E1013",
                    "Mutex.with_lock results must be proven free of borrowed environments: a function or other value that holds a borrow of the locked value would use it after the lock is released; return an owned value, or a function that captures a copy",
                    argument.span,
                ));
            }
            if !value.loans.is_empty() && owned_dyn_of(callee) {
                return Err(error(
                    "E1013",
                    "a dyn value without a region cannot hold borrowed data; pass an owned value to Dyn.of, or name a region, as in 'dyn Shapes.Shape {r}'",
                    argument.span,
                ));
            }
            // The item stays in the channel and is received later, perhaps on another thread and
            // after the data it borrows is gone. The type of the channel's end does not show the
            // item type (`Sender<T>` stores one integer), so no later check can see the borrow; a
            // parameter of a generated function passes on what its callers gave it, and they are
            // checked (F10).
            if index == 1
                && let Some(item) = &sent
                && item.carries_loans(&self.module.types())
                && !value.loans.is_empty()
                && !self.is_generated_parameter(argument)
            {
                return Err(error(
                    "E1013",
                    "a channel item cannot hold a borrow, including a borrowed function environment: it is received later, perhaps after the borrowed data is gone; send an owned value, or a function that captures a copy, or an Owned.Function built where it is sent",
                    argument.span,
                ));
            }
            match region_sources {
                Some(sources) => {
                    self.argument_regions(sources, slots, index, argument, &value, &mut current)
                }
                None => current.loans.extend(&value.loans),
            }
            self.held.push(value);
            if boundary == Some(index + 1) {
                let id = known.unwrap();
                self.store_arguments(id, arguments, start, expression.span)?;
                if self.closed[id]
                    || !callee
                        .ty
                        .after_arguments(index + 1)
                        .carries_loans(&self.module.types())
                {
                    current = Value::default();
                }
                self.held.truncate(start);
                self.held.push(current.clone());
            }
        }
        if expression.ty.carries_loans(&self.module.types()) {
            // An item that a receiver takes out of a channel was checked when it went in: it holds
            // no borrow, and it was moved out of the channel, so it does not borrow the receiver.
            result = if received { Value::default() } else { current };
            // A reference that no argument lent can only point at static data, which no call
            // result outlives (D11 Phase 2); a callee returns only borrows of its inputs.
            if !received
                && result.loans.is_empty()
                && expression
                    .ty
                    .contains_stored_reference(&self.module.types())
            {
                let loan = self.loan(
                    Place {
                        root: STATIC_ROOT,
                        fields: Vec::new(),
                        through_drop: false,
                    },
                    false,
                    BTreeSet::new(),
                    expression.span,
                );
                result.loans.insert(loan);
                for slot in result.regions.iter_mut().flat_map(|slots| slots.iter_mut()) {
                    slot.insert(loan);
                }
            }
            result.closed_result =
                std::array::from_fn(|index| self.callback_returns_closed(expression, index + 1));
        }
        self.held.truncate(start);
        Ok(result)
    }

    fn eval_lambda(
        &mut self,
        expression: &TypedExpr,
        live: &BTreeSet<usize>,
    ) -> Result<Value, Diagnostic> {
        let E::Lambda {
            parameters,
            captures,
            body,
            recursive,
            ..
        } = &expression.kind
        else {
            unreachable!("eval dispatches lambdas here")
        };
        let mut during = live.clone();
        uses(expression, &mut during);
        let mut result = Value {
            closed_result: std::array::from_fn(|index| {
                self.callback_returns_closed(expression, index + 1)
            }),
            ..Value::default()
        };
        // The functions of a `let rec` group are in scope in the body like captured function values.
        let locals: Vec<_> = captures.iter().chain(recursive).chain(parameters).collect();
        self.copy_variables.extend(check_body(
            self.module,
            locals.into_iter(),
            body,
            self.infer,
            self.closed,
            matches!(expression.ty, Type::Task(_)),
            false,
            Contract::default(),
            self.recovered,
            None,
        )?);
        self.capture_loans(expression, &during, &mut result)?;
        Ok(result)
    }

    /// Evaluates the captures of the lambda `expression` into the loans of its function value. It
    /// is a function of its own so that a lambda in a lambda does not pay for its locals on the
    /// stack while the body is checked.
    fn capture_loans(
        &mut self,
        expression: &TypedExpr,
        during: &BTreeSet<usize>,
        result: &mut Value,
    ) -> Result<(), Diagnostic> {
        let E::Lambda {
            captures,
            recursive,
            ..
        } = &expression.kind
        else {
            unreachable!("eval dispatches lambdas here")
        };
        let types = self.module.types();
        for capture in captures {
            let value = TypedExpr {
                kind: E::Local(capture.id),
                ty: capture.ty.clone(),
                span: expression.span,
            };
            // A `let rec` function holds a shared borrow of what it captures, unless the capture
            // copies for free or is a function value, which the lowering expands (B09). So the
            // functions of a group can capture one value, and none of them moves it.
            let value = if recursive.is_empty()
                || matches!(capture.ty, Type::Function(..))
                || (capture.ty.is_copy(&types) && !capture.ty.needs_drop(&types))
            {
                value
            } else {
                TypedExpr {
                    ty: Type::Reference(Box::new(capture.ty.clone()), false),
                    kind: E::Borrow(Box::new(value), false),
                    span: expression.span,
                }
            };
            let value = self.eval(&value, Use::Consume, during)?;
            if matches!(expression.ty, Type::Task(_)) && !value.loans.is_empty() {
                return Err(error(
                    "E1013",
                    "task captures cannot retain borrowed values, including borrowed function environments",
                    expression.span,
                ));
            }
            result.loans.extend(value.loans);
        }
        Ok(())
    }

    fn eval_value(
        &mut self,
        expression: &TypedExpr,
        usage: Use,
        live: &BTreeSet<usize>,
    ) -> Result<Value, Diagnostic> {
        if Self::is_place(expression)
            && !matches!(expression.kind, E::Index(ref value, _) if value.ty.is_string())
        {
            return self.read_place(expression, usage, live);
        }
        let usage = Use::Consume;
        let mut during = live.clone();
        uses(expression, &mut during);
        let mut result = Value::default();
        match &expression.kind {
            E::Function(crate::check::FunctionRef::Builtin(instance))
                if instance.builtin.is_parallel() =>
            {
                return Err(error(
                    "E1013",
                    "parallel operations must be fully applied directly; their ownership boundary cannot be erased into an ordinary function value",
                    expression.span,
                ));
            }
            // A result that may hold a borrow is checked at the direct call, so the builtin
            // cannot be applied in stages or kept as a function value then.
            E::Function(_)
                if self
                    .locked_result(expression)
                    .is_some_and(|returned| returned.carries_loans(&self.module.types())) =>
            {
                return Err(error(
                    "E1013",
                    "Mutex.with_lock must be fully applied directly when its result may hold borrowed values, so that the result can be checked: call it with the lock and the callback together",
                    expression.span,
                ));
            }
            // The same for the item of `Channel.send`.
            E::Function(_)
                if self
                    .sent_item(expression)
                    .is_some_and(|item| item.carries_loans(&self.module.types())) =>
            {
                return Err(error(
                    "E1013",
                    "Channel.send must be fully applied directly when its item may hold a borrow, so that the item can be checked: call it with the sender and the item together",
                    expression.span,
                ));
            }
            E::Function(_) | E::GenericFunction(..) | E::Method(..) | E::TypeFunction { .. } => {
                result.closed_result = std::array::from_fn(|index| {
                    self.callback_returns_closed(expression, index + 1)
                });
            }
            E::Break | E::Continue => {
                self.eval_loop_jump(matches!(expression.kind, E::Break), expression.span)?;
            }
            E::Checked(value) => {
                result = self.eval(value, Use::Consume, &during)?;
                self.raise_edge(expression.span)?;
            }
            E::Try(handled) => {
                result = self.eval_try(handled, &during, expression.span)?;
            }
            E::Reraise => {
                self.raise_edge(expression.span)?;
                self.reachable = false;
            }
            E::RaisedException => {}
            E::BorrowOperand(value) => {
                result = self.eval(value, Use::Read, &during)?;
            }
            E::Slice { value, start, end } if expression.ty.is_view() => {
                result = self.exclusive_slice(value, start, end, &during, expression.span)?;
            }
            E::Slice { value, start, end } => {
                for (place, via) in self.place(value, &during)? {
                    self.access(&place, &via, Use::Borrow, expression.span)?;
                    result
                        .loans
                        .insert(self.loan(place, false, via, expression.span));
                }
                self.held.push(result.clone());
                for bound in start.iter().chain(end) {
                    self.eval(bound, Use::Consume, &during)?;
                }
                self.held.pop();
            }
            E::Borrow(value, mutable) => {
                let places = if *mutable {
                    self.shared_exclusive(value, expression.span)?;
                    self.exclusive_places(value, &during)?
                } else {
                    self.place(value, &during)?
                };
                for (place, via) in places {
                    self.access(
                        &place,
                        &via,
                        if *mutable {
                            Use::MutBorrow
                        } else {
                            Use::Borrow
                        },
                        expression.span,
                    )?;
                    result
                        .loans
                        .insert(self.loan(place, *mutable, via, expression.span));
                }
            }
            E::Assign(place, value) => {
                self.shared_exclusive(place, expression.span)?;
                let value = self.eval(value, Use::Consume, &during)?;
                self.held.push(value.clone());
                for (place, via) in self.exclusive_places(place, &during)? {
                    if !unmarked_fields(&place).is_empty() {
                        self.access(&place, &via, Use::Borrow, expression.span)?;
                    }
                    self.access(&place, &via, Use::Write, expression.span)?;
                    if value.loans.iter().any(|id| {
                        let source = &self.loans[*id].place;
                        source.overlaps(&place)
                            || (!unmarked_fields(&place).is_empty()
                                && source.root == place.root)
                    })
                    {
                        return Err(error(
                            "E1013",
                            "cannot store a reference inside its own owner",
                            expression.span,
                        ));
                    }
                    self.state.moved.retain(|moved, _| !moved.overlaps(&place));
                    self.state
                        .generic_moves
                        .retain(|moved, _| !moved.overlaps(&place));
                    if via.is_empty() {
                        let stored = &mut self.state.locals.get_mut(&place.root).unwrap().1;
                        if place.fields.is_empty() {
                            *stored = value.clone();
                        } else {
                            join(stored, value.clone());
                            stored.regions = None;
                            stored.closed_result = [false; 2];
                        }
                    } else if !value.loans.is_empty() {
                        self.store_through(&place, &value, expression.span)?;
                    }
                }
                self.held.pop();
            }
            E::If {
                condition,
                then_branch,
                else_branch,
            } => {
                self.eval(condition, Use::Consume, &during)?;
                let before = self.state.clone();
                let before_reachable = self.reachable;
                let then_value = self.eval(then_branch, usage, live)?;
                let then_state = self.state.clone();
                let then_reachable = self.reachable;
                self.state = before;
                self.reachable = before_reachable;
                let else_value = self.eval(else_branch, usage, live)?;
                result.closed_result = std::array::from_fn(|index| {
                    then_value.closed_result[index] && else_value.closed_result[index]
                });
                if self.reachable {
                    join(&mut result, else_value);
                }
                self.merge_reachable(&then_state, then_reachable);
                if then_reachable {
                    join(&mut result, then_value);
                }
            }
            E::Binary(BinaryOp::And | BinaryOp::Or, left, right) => {
                self.eval(left, Use::Consume, &during)?;
                let before = self.state.clone();
                let before_reachable = self.reachable;
                self.eval(right, Use::Consume, &during)?;
                self.merge_reachable(&before, before_reachable);
            }
            E::Closure(_, captures) => {
                result.closed_result = std::array::from_fn(|index| {
                    self.callback_returns_closed(expression, index + 1)
                });
                for capture in captures {
                    let value = self.eval(capture, Use::Consume, &during)?;
                    if matches!(expression.ty, Type::Task(_)) && !value.loans.is_empty() {
                        return Err(error(
                            "E1013",
                            "task captures cannot retain borrowed values, including borrowed function environments",
                            expression.span,
                        ));
                    }
                    result.loans.extend(value.loans);
                }
            }
            E::TaskRun(value) | E::TaskParallel(value) | E::TaskParallelResults(value) => {
                self.eval(value, Use::Consume, &during)?;
            }
            E::StructuralCompare(_, arguments)
            | E::StructuralHash(arguments)
            | E::StructuralDisplay(arguments) => {
                for argument in arguments {
                    self.eval(argument, Use::Consume, &during)?;
                }
            }
            E::Parallel(operation, arguments) => {
                let callback = operation.parallel_callback();
                let scope = *operation == crate::check::Builtin::TaskScope;
                let input = if matches!(
                    operation,
                    crate::check::Builtin::ParallelInit | crate::check::Builtin::TaskScope
                ) {
                    None
                } else {
                    Some(arguments.len() - 1)
                };
                // The callbacks of `reduce` and `Task.scope` take two arguments.
                let result_arity = usize::from(matches!(
                    operation,
                    crate::check::Builtin::ParallelReduce | crate::check::Builtin::TaskScope
                ));
                let base = self.held.len();
                for (index, argument) in arguments.iter().enumerate() {
                    let value = self.eval(argument, Use::Consume, &during)?;
                    if scope && index == 0 {
                        // Every child reads the shared borrow, which lasts for the call. A value
                        // that may hold function values needs an environment proven to hold no
                        // loans, as the elements of `Parallel.map_ref` do.
                        let referent = argument
                            .ty
                            .dereferenced()
                            .expect("a task scope shares a borrow");
                        if referent.carries_loans(&self.module.types())
                            && self.shares_borrowed_environment(argument, &value)
                        {
                            return Err(error(
                                "E1013",
                                "task scopes can share only values with proven owned environments",
                                argument.span,
                            ));
                        }
                    } else if Some(index) == input {
                        let element = argument
                            .ty
                            .slice_element()
                            .expect("parallel input is a slice");
                        if element.carries_loans(&self.module.types())
                            && self.shares_borrowed_environment(argument, &value)
                        {
                            return Err(error(
                                "E1013",
                                "parallel input elements must have proven owned environments",
                                argument.span,
                            ));
                        }
                    } else if !value.loans.is_empty() {
                        return Err(error(
                            "E1013",
                            "parallel callbacks and values cannot retain borrowed environments",
                            argument.span,
                        ));
                    }
                    if index == callback
                        && expression.ty.carries_loans(&self.module.types())
                        && !value.closed_result[result_arity]
                    {
                        return Err(error(
                            "E1013",
                            "parallel callback results must be proven free of borrowed environments",
                            argument.span,
                        ));
                    }
                    self.held.push(value);
                }
                self.held.truncate(base);
            }
            E::Binary(operator, left, right) => {
                let usage = if matches!(
                    operator,
                    BinaryOp::Equal
                        | BinaryOp::NotEqual
                        | BinaryOp::Less
                        | BinaryOp::LessEqual
                        | BinaryOp::Greater
                        | BinaryOp::GreaterEqual
                ) {
                    Use::Read
                } else {
                    Use::Consume
                };
                let value = self.eval(left, usage, &during)?;
                self.held.push(value.clone());
                let callee = self.eval(right, usage, &during)?;
                self.held.pop();
                if *operator == BinaryOp::Pipe && expression.ty.carries_loans(&self.module.types())
                {
                    result = value;
                    result.loans.extend(callee.loans);
                    result.regions = None;
                }
            }
            E::Record(_) | E::RecordUpdate { .. } => {
                if matches!(expression.kind, E::RecordUpdate { .. })
                    && expression.ty.has_user_drop(&self.module.types())
                {
                    return Err(error(
                        "E1012",
                        "cannot update a value whose type implements Drop; construct a new value instead",
                        expression.span,
                    ));
                }
                result = self.record_value(expression, &during)?;
            }
            E::Array(elements)
            | E::List(elements)
            | E::Tuple(elements)
            | E::HostCall(_, elements) => {
                let start = self.held.len();
                for element in elements {
                    let value = self.eval(element, Use::Consume, &during)?;
                    if !matches!(expression.kind, E::HostCall(..)) {
                        result.loans.extend(&value.loans);
                    }
                    self.held.push(value);
                }
                self.held.truncate(start);
            }
            E::Interpolated(interpolation) => {
                // Every hole stays borrowed until the pieces are joined, as call arguments do.
                let start = self.held.len();
                for hole in &interpolation.holes {
                    let value = self.eval(&hole.operand, Use::Consume, &during)?;
                    self.held.push(value);
                }
                self.held.truncate(start);
            }
            E::NewArray(length, initializer) | E::NewList(length, initializer) => {
                self.eval(length, Use::Consume, &during)?;
                let value = self.eval(initializer, Use::Consume, &during)?;
                if expression.ty.carries_loans(&self.module.types()) {
                    result = value;
                }
            }
            E::NewLiteral(literal) => {
                result = self.eval(literal, Use::Consume, &during)?;
            }
            E::Construct { payload, .. } => {
                if let Some(payload) = payload {
                    result = self.eval(payload, Use::Consume, &during)?;
                    // A union payload keeps every region of its value together (A12 D6).
                    result.regions = None;
                }
            }
            E::Length(value) | E::StringLength(value) | E::UnionTag(value) => {
                self.eval(value, Use::Read, &during)?;
            }
            E::Index(value, index) => {
                if !self.is_copy(&expression.ty) && !self.require_copy(&expression.ty) {
                    return Err(error(
                        "E1012",
                        "cannot move a non-Copy array or list element; borrow it instead",
                        expression.span,
                    ));
                }
                self.note_copy(expression, expression.span, CopyKind::Temporary, false);
                let container = self.eval(value, Use::Read, &during)?;
                self.held.push(container.clone());
                self.eval(index, Use::Consume, &during)?;
                self.held.pop();
                if expression.ty.carries_loans(&self.module.types()) {
                    result = container;
                    result.regions = None;
                }
            }
            E::Field(value, _) | E::UnionPayload { value, .. } => {
                // The glue drops a temporary Drop value whole, so only Copy parts leave it.
                if value.ty.has_user_drop(&self.module.types()) {
                    if !self.is_copy(&expression.ty) && !self.require_copy(&expression.ty) {
                        return Err(error("E1012", MOVE_OUT_OF_DROP, expression.span));
                    }
                    self.note_copy(expression, expression.span, CopyKind::Temporary, false);
                }
                let value = self.eval(value, Use::Consume, &during)?;
                if expression.ty.carries_loans(&self.module.types()) {
                    result = self.field_value(expression, value);
                }
            }
            E::Cast(value) if expression.ty.function_purity().is_some() => {
                result = self.eval(value, Use::Consume, &during)?;
            }
            E::Unary(_, value) | E::Cast(value) | E::SaturatingIndex(value) => {
                self.eval(value, Use::Consume, &during)?;
            }
            E::Int(_)
            | E::Float(_)
            | E::String(_)
            | E::Bool(_)
            | E::Unit
            | E::Error
            // Static data is a value that no one moves; its borrows are made by `place`.
            | E::Static(_)
            | E::CaseConstructor { .. }
            | E::GenericInteger(..)
            | E::GenericFloat(_)
            // A dispatch passes its parameters on; `closed` keeps its result tied to them (A14).
            | E::DynDispatch { .. } => {}
            E::Local(_) | E::Dereference(_) | E::ListTail(..) => {
                unreachable!("places handled above")
            }
            E::While { .. } | E::ForRange { .. } | E::ForEach { .. } | E::Match { .. } => {
                unreachable!("control expressions use their own evaluator")
            }
            E::Block { .. } | E::Call(..) | E::Lambda { .. } => {
                unreachable!("composed expressions use their own evaluator")
            }
        }
        Ok(result)
    }

    fn merge(&mut self, other: &State<'a>) {
        // The latest move of a place is kept, so the result does not depend on the order of the
        // merged branches.
        for (place, span) in &other.moved {
            self.state
                .moved
                .entry(place.clone())
                .and_modify(|kept| {
                    if (span.start, span.end) > (kept.start, kept.end) {
                        *kept = *span;
                    }
                })
                .or_insert(*span);
        }
        self.state.generic_moves.extend(other.generic_moves.clone());
        for (id, (_, value)) in &mut self.state.locals {
            if let Some((_, other_value)) = other.locals.get(id) {
                value.loans.extend(&other_value.loans);
                if value.regions != other_value.regions {
                    value.regions = None;
                }
            }
        }
    }
}

fn read_kind(expression: &TypedExpr) -> CopyKind {
    match expression.kind {
        E::Local(_) => CopyKind::Local,
        E::Field(..) => CopyKind::Field,
        E::Index(..) => CopyKind::Element,
        E::ListTail(..) => CopyKind::Tail,
        E::UnionPayload { .. } => CopyKind::Payload,
        _ => CopyKind::Dereference,
    }
}

fn count_uses(expression: &TypedExpr, counts: &mut BTreeMap<usize, usize>) {
    match &expression.kind {
        E::Local(id) => {
            *counts.entry(*id).or_default() += 1;
        }
        E::Lambda { captures, .. } => {
            for capture in captures {
                *counts.entry(capture.id).or_default() += 1;
            }
        }
        _ => {
            expression
                .try_for_each_child(&mut |child| {
                    count_uses(child, counts);
                    Ok(())
                })
                .expect("counting uses cannot fail");
        }
    }
}

/// The control flow of a function body, built when a diagnostic explains where a moved value or
/// a borrow came from. Every expression has an entry and an exit node; the edges follow the
/// evaluation order, the arms of `if` and `match` (a failing pattern or guard falls through to the
/// next arm), `&&` and `||`, loops (an edge back to the loop's head is a back edge), `break`,
/// `continue`, re-raises, and the exceptions that a `try` body may raise to its handler.
struct Flow {
    /// The successors of each node, and the loop (an index of `loops`) whose head an edge goes
    /// back to.
    edges: Vec<Vec<(usize, Option<usize>)>>,
    /// The local that each node stores a value in (`local = ...`, `let local = ...`), and the
    /// local whose value it copies, when the value is just a local.
    stores: Vec<Option<(usize, Option<usize>)>>,
    /// The nodes that read each local: the exit of the local, or of a lambda that captures it.
    readers: BTreeMap<usize, Vec<usize>>,
    /// The spans of the nodes that read, for the diagnostics.
    spans: Vec<Span>,
    /// The entries and exits of the expressions with each span: one per copy (a `finally` has
    /// two) and per expression that shares the span.
    by_span: BTreeMap<SpanKey, Vec<(usize, usize)>>,
    /// Where a slice reported at a span reaches its source: before its bounds, and (for an
    /// exclusive slice, `true`) again after them.
    accesses: BTreeMap<SpanKey, Vec<(usize, usize, bool)>>,
    /// The spans of the loops.
    loops: Vec<Span>,
}

/// A span as a map key: its source, start, and end.
type SpanKey = (Option<usize>, usize, usize);

/// Where control goes from inside an expression: the targets of `break` and `continue`, and the
/// handler that an exception reaches.
#[derive(Clone, Copy, Default)]
struct Targets {
    break_to: Option<usize>,
    /// The head of the loop that `continue` repeats, and the loop.
    continue_to: Option<(usize, usize)>,
    handler: Option<usize>,
}

/// Takes `cost` from `budget`; `None` once it runs out.
fn spend(budget: &mut usize, cost: usize) -> Option<()> {
    *budget = budget.checked_sub(cost)?;
    Some(())
}

impl Flow {
    fn node(&mut self, span: Span) -> usize {
        self.edges.push(Vec::new());
        self.stores.push(None);
        self.spans.push(span);
        self.edges.len() - 1
    }

    fn edge(&mut self, from: usize, to: usize) {
        self.edges[from].push((to, None));
    }

    /// Entries and exits for `parts`, run one after another from `from` to `to`.
    fn sequence<'e>(
        &mut self,
        parts: Vec<&'e TypedExpr>,
        from: usize,
        to: usize,
    ) -> Vec<(&'e TypedExpr, usize, usize)> {
        let mut at = from;
        let mut items = Vec::new();
        for part in parts {
            let (part_entry, part_exit) = (self.node(part.span), self.node(part.span));
            self.edge(at, part_entry);
            items.push((part, part_entry, part_exit));
            at = part_exit;
        }
        self.edge(at, to);
        items
    }

    /// Builds the flow of `body`; the reads and stores of the `tracked` locals are recorded.
    /// `None` when it would take more than `*budget` steps.
    fn build(body: &TypedExpr, tracked: &BTreeSet<usize>, budget: &mut usize) -> Option<Self> {
        let mut flow = Flow {
            edges: Vec::new(),
            stores: Vec::new(),
            readers: BTreeMap::new(),
            spans: Vec::new(),
            by_span: BTreeMap::new(),
            accesses: BTreeMap::new(),
            loops: Vec::new(),
        };
        let (entry, exit) = (flow.node(body.span), flow.node(body.span));
        let mut pending = vec![(body, entry, exit, Targets::default())];
        while let Some((expression, entry, exit, targets)) = pending.pop() {
            spend(budget, 1)?;
            let span = expression.span;
            flow.by_span
                .entry((span.source, span.start, span.end))
                .or_default()
                .push((entry, exit));
            // Children are added in evaluation order: each gets an entry and an exit first.
            let child =
                |flow: &mut Flow, part: &TypedExpr| (flow.node(part.span), flow.node(part.span));
            let sequence = Flow::sequence;
            let mut items: Vec<(&TypedExpr, usize, usize, Targets)> = Vec::new();
            match &expression.kind {
                E::Local(id) if tracked.contains(id) => {
                    flow.edge(entry, exit);
                    flow.readers.entry(*id).or_default().push(exit);
                }
                E::Lambda { captures, .. } => {
                    // The body runs when the function is called; only the captures are read here.
                    flow.edge(entry, exit);
                    for local in captures.iter().filter(|local| tracked.contains(&local.id)) {
                        flow.readers.entry(local.id).or_default().push(exit);
                    }
                }
                E::Slice { value, start, end } => {
                    let parts = std::iter::once(&**value)
                        .chain(start.as_deref())
                        .chain(end.as_deref())
                        .collect();
                    let parts = sequence(&mut flow, parts, entry, exit);
                    flow.accesses
                        .entry((span.source, span.start, span.end))
                        .or_default()
                        .push((parts[0].2, exit, expression.ty.is_view()));
                    for (part, part_entry, part_exit) in parts {
                        items.push((part, part_entry, part_exit, targets));
                    }
                }
                E::Checked(value) => {
                    // Checked arithmetic raises after its operands, to the `try` around it.
                    for (part, part_entry, part_exit) in
                        sequence(&mut flow, vec![value], entry, exit)
                    {
                        if let Some(handler) = targets.handler {
                            flow.edge(part_exit, handler);
                        }
                        items.push((part, part_entry, part_exit, targets));
                    }
                }
                E::Break => {
                    if let Some(to) = targets.break_to {
                        flow.edge(entry, to);
                    }
                }
                E::Continue => {
                    if let Some((to, repeated)) = targets.continue_to {
                        flow.edges[entry].push((to, Some(repeated)));
                    }
                }
                E::Reraise => {
                    if let Some(to) = targets.handler {
                        flow.edge(entry, to);
                    }
                }
                E::Assign(place, value) => {
                    if let E::Local(stored) = place.kind
                        && tracked.contains(&stored)
                    {
                        // The value is evaluated, then stored; the target is not read.
                        for (part, part_entry, part_exit) in
                            sequence(&mut flow, vec![value], entry, exit)
                        {
                            items.push((part, part_entry, part_exit, targets));
                        }
                        flow.stores[exit] = Some((stored, copied(value, tracked)));
                    } else {
                        for (part, part_entry, part_exit) in
                            sequence(&mut flow, vec![value, place], entry, exit)
                        {
                            items.push((part, part_entry, part_exit, targets));
                        }
                    }
                }
                E::If {
                    condition,
                    then_branch,
                    else_branch,
                } => {
                    let (c_entry, c_exit) = child(&mut flow, condition);
                    let (t_entry, t_exit) = child(&mut flow, then_branch);
                    let (e_entry, e_exit) = child(&mut flow, else_branch);
                    flow.edge(entry, c_entry);
                    flow.edge(c_exit, t_entry);
                    flow.edge(c_exit, e_entry);
                    flow.edge(t_exit, exit);
                    flow.edge(e_exit, exit);
                    items.extend([
                        (&**condition, c_entry, c_exit, targets),
                        (&**then_branch, t_entry, t_exit, targets),
                        (&**else_branch, e_entry, e_exit, targets),
                    ]);
                }
                E::Match { value, arms, .. } => {
                    let (v_entry, v_exit) = child(&mut flow, value);
                    flow.edge(entry, v_entry);
                    items.push((&**value, v_entry, v_exit, targets));
                    // Where control goes when the previous arm does not match.
                    let mut failing = vec![v_exit];
                    for arm in arms {
                        let arm_entry = flow.node(arm.body.span);
                        for from in failing {
                            flow.edge(from, arm_entry);
                        }
                        failing = Vec::new();
                        let (b_entry, b_exit) = child(&mut flow, &arm.body);
                        let guard = arm
                            .guard
                            .as_ref()
                            .map(|guard| (guard, child(&mut flow, guard)));
                        let success = guard.map_or(b_entry, |(_, (g_entry, _))| g_entry);
                        // Each alternative starts where the previous one failed; a test that fails
                        // goes on to the next alternative, or to the next arm after the last one.
                        let mut start = arm_entry;
                        for (index, alternative) in arm.alternatives.iter().enumerate() {
                            let next = (index + 1 < arm.alternatives.len())
                                .then(|| flow.node(arm.body.span));
                            let mut at = start;
                            let parts = alternative
                                .steps
                                .iter()
                                .map(|step| {
                                    (
                                        step.expression(),
                                        matches!(step, crate::check::PatternStep::Test(_)),
                                    )
                                })
                                .chain(
                                    alternative.bindings.iter().map(|(_, value)| (value, false)),
                                );
                            for (part, fallible) in parts {
                                let (part_entry, part_exit) = child(&mut flow, part);
                                flow.edge(at, part_entry);
                                if fallible {
                                    match next {
                                        Some(next) => flow.edge(part_exit, next),
                                        None => failing.push(part_exit),
                                    }
                                }
                                items.push((part, part_entry, part_exit, targets));
                                at = part_exit;
                            }
                            flow.edge(at, success);
                            match next {
                                Some(next) => start = next,
                                None => break,
                            }
                        }
                        // A false guard goes on to the next arm.
                        if let Some((guard, (g_entry, g_exit))) = guard {
                            flow.edge(g_exit, b_entry);
                            failing.push(g_exit);
                            items.push((guard, g_entry, g_exit, targets));
                        }
                        flow.edge(b_exit, exit);
                        items.push((&arm.body, b_entry, b_exit, targets));
                    }
                }
                E::While { condition, body } => {
                    flow.loops.push(span);
                    let repeated = flow.loops.len() - 1;
                    let (c_entry, c_exit) = child(&mut flow, condition);
                    let (b_entry, b_exit) = child(&mut flow, body);
                    flow.edge(entry, c_entry);
                    flow.edge(c_exit, b_entry);
                    flow.edge(c_exit, exit);
                    flow.edges[b_exit].push((c_entry, Some(repeated)));
                    let inner = Targets {
                        break_to: Some(exit),
                        continue_to: Some((c_entry, repeated)),
                        handler: targets.handler,
                    };
                    // A `break` or `continue` in the condition belongs to the loop around this one.
                    items.extend([
                        (&**condition, c_entry, c_exit, targets),
                        (&**body, b_entry, b_exit, inner),
                    ]);
                }
                E::ForRange {
                    start,
                    step,
                    finish,
                    body,
                    ..
                } => {
                    flow.loops.push(span);
                    let repeated = flow.loops.len() - 1;
                    let head = flow.node(body.span);
                    for (part, part_entry, part_exit) in
                        sequence(&mut flow, vec![start, step, finish], entry, head)
                    {
                        items.push((part, part_entry, part_exit, targets));
                    }
                    let (b_entry, b_exit) = child(&mut flow, body);
                    flow.edge(head, b_entry);
                    flow.edge(head, exit);
                    flow.edges[b_exit].push((head, Some(repeated)));
                    let inner = Targets {
                        break_to: Some(exit),
                        continue_to: Some((head, repeated)),
                        handler: targets.handler,
                    };
                    items.push((&**body, b_entry, b_exit, inner));
                }
                E::ForEach { source, body, .. } => {
                    flow.loops.push(span);
                    let repeated = flow.loops.len() - 1;
                    let head = flow.node(body.span);
                    for (part, part_entry, part_exit) in
                        sequence(&mut flow, vec![source], entry, head)
                    {
                        items.push((part, part_entry, part_exit, targets));
                    }
                    let (b_entry, b_exit) = child(&mut flow, body);
                    flow.edge(head, b_entry);
                    flow.edge(head, exit);
                    flow.edges[b_exit].push((head, Some(repeated)));
                    let inner = Targets {
                        break_to: Some(exit),
                        continue_to: Some((head, repeated)),
                        handler: targets.handler,
                    };
                    items.push((&**body, b_entry, b_exit, inner));
                }
                E::Binary(BinaryOp::And | BinaryOp::Or, left, right) => {
                    let (l_entry, l_exit) = child(&mut flow, left);
                    let (r_entry, r_exit) = child(&mut flow, right);
                    flow.edge(entry, l_entry);
                    flow.edge(l_exit, r_entry);
                    flow.edge(l_exit, exit);
                    flow.edge(r_exit, exit);
                    items.extend([
                        (&**left, l_entry, l_exit, targets),
                        (&**right, r_entry, r_exit, targets),
                    ]);
                }
                E::Try(handled) => {
                    let (b_entry, b_exit) = child(&mut flow, &handled.body);
                    let (h_entry, h_exit) = child(&mut flow, &handled.handler);
                    flow.edge(entry, b_entry);
                    let mut handler_targets = targets;
                    let after = match &handled.finally {
                        Some(finally) => {
                            // `finally` runs on the normal exit, and before an exception that
                            // leaves the handler reaches the next `try`: two copies of its flow.
                            let (f_entry, f_exit) = child(&mut flow, finally);
                            flow.edge(f_exit, exit);
                            items.push((finally, f_entry, f_exit, targets));
                            let (e_entry, e_exit) = child(&mut flow, finally);
                            if let Some(outer) = targets.handler {
                                flow.edge(e_exit, outer);
                            }
                            items.push((finally, e_entry, e_exit, targets));
                            handler_targets.handler = Some(e_entry);
                            f_entry
                        }
                        None => exit,
                    };
                    flow.edge(b_exit, after);
                    flow.edge(h_exit, after);
                    items.push((
                        &handled.body,
                        b_entry,
                        b_exit,
                        Targets {
                            handler: Some(h_entry),
                            ..targets
                        },
                    ));
                    items.push((&handled.handler, h_entry, h_exit, handler_targets));
                }
                E::Block { bindings, result } => {
                    // Each binding stores its value in its local after evaluating it.
                    let mut at = entry;
                    for (local, value) in bindings {
                        let (v_entry, v_exit) = child(&mut flow, value);
                        flow.edge(at, v_entry);
                        let bind = flow.node(value.span);
                        flow.edge(v_exit, bind);
                        if tracked.contains(&local.id) {
                            flow.stores[bind] = Some((local.id, copied(value, tracked)));
                        }
                        items.push((value, v_entry, v_exit, targets));
                        at = bind;
                    }
                    let (r_entry, r_exit) = child(&mut flow, result);
                    flow.edge(at, r_entry);
                    flow.edge(r_exit, exit);
                    items.push((&**result, r_entry, r_exit, targets));
                }
                _ => {
                    let mut parts = Vec::new();
                    expression
                        .try_for_each_child(&mut |part| {
                            parts.push(part);
                            Ok(())
                        })
                        .expect("collecting parts cannot fail");
                    for (part, part_entry, part_exit) in sequence(&mut flow, parts, entry, exit) {
                        items.push((part, part_entry, part_exit, targets));
                    }
                }
            }
            pending.extend(items.into_iter().rev());
        }
        Some(flow)
    }

    /// The nodes reachable from `from`, each with whether a path to it crosses no back edge of
    /// the loops in `counting` and whether one crosses such a back edge. The back edges of the
    /// other loops are ordinary edges.
    fn reach(
        &self,
        from: &[usize],
        counting: &[bool],
        budget: &mut usize,
    ) -> Option<Vec<[bool; 2]>> {
        spend(budget, self.edges.len() + from.len())?;
        let mut seen = vec![[false; 2]; self.edges.len()];
        let mut pending = std::collections::VecDeque::new();
        for &node in from {
            seen[node][0] = true;
            pending.push_back((node, false));
        }
        while let Some((node, crossed)) = pending.pop_front() {
            spend(budget, 1 + self.edges[node].len())?;
            for &(next, back) in &self.edges[node] {
                let crossed = crossed || back.is_some_and(|repeated| counting[repeated]);
                if !seen[next][usize::from(crossed)] {
                    seen[next][usize::from(crossed)] = true;
                    pending.push_back((next, crossed));
                }
            }
        }
        Some(seen)
    }

    /// Which loops hold both spans: only their back edges lead to a next iteration in which the
    /// second runs after the first (the back edges of a loop that holds one of them only leave it).
    fn around_both(&self, a: Span, b: Span, budget: &mut usize) -> Option<Vec<bool>> {
        spend(budget, self.loops.len())?;
        let within = |outer: &Span, inner: Span| {
            outer.source == inner.source && outer.start <= inner.start && inner.end <= outer.end
        };
        Some(
            self.loops
                .iter()
                .map(|outer| within(outer, a) && within(outer, b))
                .collect(),
        )
    }

    /// Where the loan holds a value: the states `(node, slot)` reachable from `start`, each with
    /// whether a path crosses no back edge of the loops in `counting` and whether one does. Slot 0
    /// is the value of the loan before a local holds it, created at `created`: the store whose
    /// value holds that span puts it in a local. Slot `i + 1` is the local `locals[i]`: a store
    /// into the local replaces the loan, unless it copies a local that holds the loan here, which
    /// hands it over (`x = y`). The result is indexed by `node * (locals.len() + 1) + slot`.
    fn holds(
        &self,
        start: &[(usize, usize)],
        locals: &[usize],
        created: Span,
        counting: &[bool],
        budget: &mut usize,
    ) -> Option<Vec<[bool; 2]>> {
        let width = locals.len() + 1;
        spend(budget, self.edges.len() * width + start.len())?;
        let slot = |local: usize| {
            locals
                .iter()
                .position(|other| *other == local)
                .map(|i| i + 1)
        };
        let contains = |outer: Span| {
            outer.source == created.source
                && outer.start <= created.start
                && created.end <= outer.end
        };
        let mut seen = vec![[false; 2]; self.edges.len() * width];
        let mut pending = std::collections::VecDeque::new();
        for &(node, at) in start {
            seen[node * width + at][0] = true;
            pending.push_back((node, at, false));
        }
        while let Some((node, at, crossed)) = pending.pop_front() {
            spend(budget, 1 + self.edges[node].len())?;
            for &(next, back) in &self.edges[node] {
                let crossed = crossed || back.is_some_and(|repeated| counting[repeated]);
                // What the loan's holder is after `next` runs, and the local it hands it to.
                let (kept, handed) = match self.stores[next] {
                    Some((stored, from)) => {
                        let target = slot(stored);
                        if at == 0 && contains(self.spans[next]) {
                            (target, None)
                        } else if target == Some(at) {
                            (None, None)
                        } else {
                            let handed = from.and_then(slot).filter(|from| *from == at);
                            (Some(at), handed.and(target))
                        }
                    }
                    None => (Some(at), None),
                };
                for at in kept.into_iter().chain(handed) {
                    let index = next * width + at;
                    if !seen[index][usize::from(crossed)] {
                        seen[index][usize::from(crossed)] = true;
                        pending.push_back((next, at, crossed));
                    }
                }
            }
        }
        Some(seen)
    }

    /// The nodes of `nodes` that control can reach from the start of the body: a copy of a
    /// `finally` that no exception reaches never runs.
    fn live(&self, nodes: Vec<usize>, budget: &mut usize) -> Option<Vec<usize>> {
        // The entry of the body is the first node.
        let reached = self.reach(&[0], &vec![false; self.loops.len()], budget)?;
        Some(
            nodes
                .into_iter()
                .filter(|node| reached[*node] != [false; 2])
                .collect(),
        )
    }
}

/// Where a local that holds a borrow uses it after an access at `after`, so that the borrow is
/// still alive there, and the local: a read of a local that holds the loan at that point, which
/// control reaches from the access. Which locals hold the loan where follows its value from the
/// span where it was `created`, through stores and copies (`Flow::holds`), over the `holders` and
/// the locals they were copied from. The access is searched from where the ownership check makes
/// it: the end of the expression, except that a slice reaches its source before its bounds (an
/// exclusive slice also after them, which is where a shared loan conflicts, when not `mutable`),
/// in the copies (of a `finally`) where a holder holds the loan. A use is in the next iteration
/// of a loop (`true`) when every path to it goes back to the head of a loop that holds both; such
/// a use is not named when `held` (an argument held by a call keeps the borrow alive instead).
/// One reached without a back edge is preferred, then the first in the source. Looks at no more
/// than `*budget` steps, and `None` once it runs out or does not find the access.
fn find_later_use(
    body: &TypedExpr,
    holders: &BTreeSet<usize>,
    after: Span,
    (created, mutable): (Span, bool),
    held: bool,
    budget: &mut usize,
) -> Option<(Span, bool, usize)> {
    if holders.is_empty() {
        return None;
    }
    let tracked = carriers(body, holders, budget)?;
    let flow = Flow::build(body, &tracked, budget)?;
    let key = (after.source, after.start, after.end);
    // Every copy of the access (a `finally` has two), where the access happens in it.
    let access: Vec<usize> = match flow.accesses.get(&key) {
        Some(points) => points
            .iter()
            .map(|&(before, end, exclusive)| if exclusive && !mutable { end } else { before })
            .collect(),
        None => flow
            .by_span
            .get(&key)?
            .iter()
            .map(|(_, exit)| *exit)
            .collect(),
    };
    let access = flow.live(access, budget)?;
    let locals: Vec<usize> = tracked.iter().copied().collect();
    let width = locals.len() + 1;
    let none = vec![false; flow.loops.len()];
    // Where the holders hold the loan at the access: from where it was created, or (for a loan
    // that a parameter brings, created outside the body) at every copy of the access.
    let holding = match flow
        .by_span
        .get(&(created.source, created.start, created.end))
    {
        Some(copies) => {
            let creation: Vec<(usize, usize)> = copies.iter().map(|(_, exit)| (*exit, 0)).collect();
            Some(flow.holds(&creation, &locals, created, &none, budget)?)
        }
        None => None,
    };
    let mut seeds = Vec::new();
    for &node in &access {
        for (index, local) in locals.iter().enumerate() {
            spend(budget, 1)?;
            if holders.contains(local)
                && holding
                    .as_ref()
                    .is_none_or(|holding| holding[node * width + index + 1] != [false; 2])
            {
                seeds.push((node, index + 1));
            }
        }
    }
    if seeds.is_empty() {
        return None;
    }
    // One search per set of loops that hold both the access and a use (a chain of the loops
    // around the access, so no more searches than its depth).
    let mut searches: BTreeMap<Vec<bool>, Vec<[bool; 2]>> = BTreeMap::new();
    let mut best: Option<(bool, usize, usize, Span)> = None;
    for (index, local) in locals.iter().enumerate() {
        for &node in flow.readers.get(local).into_iter().flatten() {
            spend(budget, 1)?;
            let span = flow.spans[node];
            if span.source != after.source || span.end <= span.start {
                continue;
            }
            let counting = flow.around_both(after, span, budget)?;
            if !searches.contains_key(&counting) {
                let reached = flow.holds(&seeds, &locals, created, &counting, budget)?;
                searches.insert(counting.clone(), reached);
            }
            // Only across a back edge of a loop around both is the use in its next iteration.
            let [direct, repeated] = searches[&counting][node * width + index + 1];
            if !direct && !repeated {
                continue;
            }
            let repeated = !direct;
            if repeated && held {
                continue;
            }
            let key = (repeated, span.start, *local, span);
            if best.is_none_or(|kept| (key.0, key.1, key.2) < (kept.0, kept.1, kept.2)) {
                best = Some(key);
            }
        }
    }
    best.map(|(repeated, _, local, span)| (span, repeated, local))
}

/// The local that a value copies when the value is just one of the `tracked` locals.
fn copied(value: &TypedExpr, tracked: &BTreeSet<usize>) -> Option<usize> {
    match value.kind {
        E::Local(id) if tracked.contains(&id) => Some(id),
        _ => None,
    }
}

/// The locals that may hold the loan of `holders`: the holders, and the locals that one of them
/// was assigned or bound from (`holder = other`, `let holder = other`), transitively.
fn carriers(
    body: &TypedExpr,
    holders: &BTreeSet<usize>,
    budget: &mut usize,
) -> Option<BTreeSet<usize>> {
    let mut copies: Vec<(usize, usize)> = Vec::new();
    let mut pending = vec![body];
    while let Some(expression) = pending.pop() {
        spend(budget, 1)?;
        match &expression.kind {
            E::Assign(place, value) => {
                if let (E::Local(to), E::Local(from)) = (&place.kind, &value.kind) {
                    copies.push((*to, *from));
                }
            }
            E::Block { bindings, .. } => {
                for (local, value) in bindings {
                    if let E::Local(from) = value.kind {
                        copies.push((local.id, from));
                    }
                }
            }
            _ => {}
        }
        expression
            .try_for_each_child(&mut |child| {
                pending.push(child);
                Ok(())
            })
            .expect("collecting copies cannot fail");
    }
    let mut carriers = holders.clone();
    loop {
        spend(budget, copies.len())?;
        let before = carriers.len();
        for (to, from) in &copies {
            if carriers.contains(to) {
                carriers.insert(*from);
            }
        }
        if carriers.len() == before {
            return Some(carriers);
        }
    }
}

/// Whether the use at `used` reads a value that the move at `moved` took in the previous
/// iteration of a loop: the use is reachable from the move only across a back edge. `None` when
/// it is not reachable, or the search runs out of `*budget`.
fn moved_in_previous_iteration(
    body: &TypedExpr,
    moved: Span,
    used: Span,
    budget: &mut usize,
) -> Option<bool> {
    let flow = Flow::build(body, &BTreeSet::new(), budget)?;
    let from: Vec<usize> = flow
        .by_span
        .get(&(moved.source, moved.start, moved.end))?
        .iter()
        .map(|(_, exit)| *exit)
        .collect();
    let from = flow.live(from, budget)?;
    // Only the back edges of a loop that holds both lead to the use in a next iteration.
    let counting = flow.around_both(moved, used, budget)?;
    let reached = flow.reach(&from, &counting, budget)?;
    let (mut direct, mut crossed) = (false, false);
    for (entry, _) in flow.by_span.get(&(used.source, used.start, used.end))? {
        direct |= reached[*entry][0];
        crossed |= reached[*entry][1];
    }
    (direct || crossed).then_some(!direct)
}

fn uses(expression: &TypedExpr, ids: &mut BTreeSet<usize>) {
    match &expression.kind {
        E::Local(id) => {
            ids.insert(*id);
        }
        E::Lambda { captures, .. } => ids.extend(captures.iter().map(|local| local.id)),
        _ => expression
            .try_for_each_child(&mut |child| {
                uses(child, ids);
                Ok(())
            })
            .expect("collecting uses cannot fail"),
    }
}

#[cfg(test)]
mod allocation_tests {
    #[test]
    fn scalar_recheck_elision_excludes_borrowing_calls_and_bindings() {
        let module = crate::analyze(
            "def scalar :: i64 -> i64 = \\x -> x + 1\n\
             def branch :: i64 -> i64 = \\x -> if x > 0 then x else -x\n\
             def ignored :: ref i64 -> i64 = \\_x -> 1\n\
             def call :: i64 -> i64 = \\x -> scalar x\n\
             def read :: ref i64 -> i64 = \\x -> *x\n\
             def binding :: i64 -> i64 = \\x -> { let y = x; y }",
        )
        .unwrap();
        for (name, expected) in [
            ("scalar", true),
            ("branch", true),
            ("ignored", true),
            ("call", false),
            ("read", false),
            ("binding", false),
        ] {
            let function = module
                .functions
                .iter()
                .find(|function| function.name == name)
                .unwrap();
            assert_eq!(super::ownership_free(&function.body), expected, "{name}");
        }
    }
}

#[cfg(test)]
mod later_use_tests {
    use super::*;

    #[test]
    fn finds_the_use_after_and_before_a_span_and_honors_the_budget() {
        let source = "def f :: i64 -> i64 = \\x -> 1 + x + 2\n\ndef g :: bool -> i64 -> i64 = \\c x -> if c then 3 else x\n\ndef h :: i64 -> i64 = \\x ->\n    let mut total = 0\n    for i in 0 .. 2 do\n        total = total + x + 4\n    total\n";
        let module = crate::analyze(source).unwrap();
        let function = |name: &str| {
            module
                .functions
                .iter()
                .find(|function| function.name == name)
                .unwrap()
        };
        // The holder is the last parameter, `x`.
        let x = |name: &str| function(name).parameters.last().unwrap().id;
        let at = |text: &str| {
            let start = source.find(text).unwrap();
            Span::new(start, start + 1).in_source(0)
        };
        let find = |name: &str, after: Span, held: bool, mut budget: usize| {
            let holders = BTreeSet::from([x(name)]);
            find_later_use(
                &function(name).body,
                &holders,
                after,
                (Span::default(), false),
                held,
                &mut budget,
            )
        };
        let used = |text: &str| {
            let start = source.find(text).unwrap();
            Span::new(start, start + 1).in_source(0)
        };
        assert_eq!(
            find("f", at("1 +"), false, LATER_USE_NODES),
            Some((used("x + 2"), false, x("f")))
        );
        // Before the access and with no loop around it, no use keeps the borrow alive.
        assert_eq!(find("f", at("2\n"), false, LATER_USE_NODES), None);
        // The other arm of an `if` does not run after this one.
        assert_eq!(find("g", at("3 else"), false, LATER_USE_NODES), None);
        // The next iteration of the loop runs the use again, unless a held argument keeps it.
        assert_eq!(
            find("h", at("4\n"), false, LATER_USE_NODES),
            Some((used("x + 4"), true, x("h")))
        );
        assert_eq!(find("h", at("4\n"), true, LATER_USE_NODES), None);
        assert_eq!(find("f", at("1 +"), false, 0), None);
        assert_eq!(
            find_later_use(
                &function("f").body,
                &BTreeSet::new(),
                at("1 +"),
                (Span::default(), false),
                false,
                &mut LATER_USE_NODES.clone()
            ),
            None
        );
    }
}
