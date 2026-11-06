use std::collections::{BTreeMap, BTreeSet};
use std::ops::ControlFlow;

use crate::check::{
    CallbackContract, CheckedModule, Local, MAX_RECORD_REGIONS, RegionMask, RegionSlots,
    RegionSources, Type, TypedExpr, TypedExprKind as E,
};
use crate::copies::{CopyKind, CopyRead};
use crate::diagnostic::{
    Diagnostic, DiagnosticKind, Diagnostics, MAX_UNIQUE_DIAGNOSTICS, RelatedRole, Span,
};
use crate::parallel::{Parallelism, ordered_for_each};
use crate::syntax::BinaryOp;

#[path = "ownership_control.rs"]
mod control;

/// What the checker does to a variable at a place (G26 phase 5): the knowledge graph reads these
/// events from the abstract bodies. Collecting them changes no check and no diagnostic.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct OwnershipEvent {
    pub(crate) var: String,
    pub(crate) kind: EventKind,
    /// The function that receives the value or the borrow, for an argument of a call.
    pub(crate) to: Option<EventTarget>,
    pub(crate) span: Span,
    /// The type of a moved value: a move of a type variable that the body turns out to need as
    /// Copy is a copy, not a move.
    moved: Option<Type>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum EventKind {
    Move,
    Borrow,
    BorrowMut,
    Reborrow,
    Drop,
}

impl EventKind {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Move => "move",
            Self::Borrow => "borrow",
            Self::BorrowMut => "borrow_mut",
            Self::Reborrow => "reborrow",
            Self::Drop => "drop",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum EventTarget {
    User(usize),
    Builtin(crate::check::Builtin),
    Method(usize, usize),
}

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

#[derive(Clone)]
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
    let facts = call_facts(module, true);
    let mut recovered = Vec::new();
    for function in &module.functions {
        let _ = check_body(
            module,
            function.parameters.iter(),
            &function.body,
            true,
            &facts,
            function.is_once,
            false,
            Contract::of(function),
            &mut recovered,
            None,
            None,
        );
    }
    recovered
}

/// The consuming reads of Copy values that need a drop, per function of a checked module (A15).
pub(crate) fn copy_reads(module: &CheckedModule) -> Vec<Vec<CopyRead>> {
    let facts = call_facts(module, false);
    let mut reads = Vec::with_capacity(module.functions.len());
    for (id, function) in module.functions.iter().enumerate() {
        let mut copies = Vec::new();
        let _ = check_body(
            module,
            function.parameters.iter(),
            &function.body,
            false,
            &facts,
            function.is_once,
            module.user_drops.values().any(|drop| *drop == id),
            Contract::of(function),
            &mut Vec::new(),
            Some(&mut copies),
            None,
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
    let facts = call_facts(module, infer);
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
                &facts,
                function.is_once,
                module.user_drops.values().any(|drop| *drop == id),
                Contract::of(function),
                &mut recovered,
                None,
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
    facts: &CallFacts,
    task: bool,
    user_drop: bool,
    contract: Contract<'a>,
    recovered: &mut Vec<Diagnostic>,
    copies: Option<&mut Vec<CopyRead>>,
    events: Option<&mut Vec<OwnershipEvent>>,
) -> Result<BTreeSet<String>, Diagnostic> {
    let mut checker = start_checker(
        module,
        parameters.clone(),
        body,
        infer,
        facts,
        task,
        user_drop,
        contract,
        recovered,
        copies,
        events,
    );
    let result = checker.eval(body, Use::Consume, &BTreeSet::new())?;
    if checker.events.is_some() {
        let end = Span {
            start: body.span.end,
            ..body.span
        };
        let ids: Vec<usize> = parameters.clone().map(|parameter| parameter.id).collect();
        checker.drop_events(&ids, end);
    }
    checker.check_result(parameters, body, task, contract, result)?;
    if let Some(events) = checker.events.take() {
        // With the Copy variables that the whole body needs, a move of a type that is Copy (a type
        // variable, or an array, tuple, record or union of such) is a copy.
        events.retain(|event| !event.moved.as_ref().is_some_and(|ty| checker.is_copy(ty)));
        events.sort_by_key(|event| (event.span.start, event.span.end, event.kind));
        events.dedup();
    }
    Ok(checker.copy_variables)
}

/// A checker for `body` whose parameters hold the loans that stand for their callers' borrows.
#[allow(clippy::too_many_arguments)]
fn start_checker<'a, 'run>(
    module: &'a CheckedModule,
    parameters: impl Clone + Iterator<Item = &'a Local>,
    body: &'a TypedExpr,
    infer: bool,
    facts: &'run CallFacts,
    task: bool,
    user_drop: bool,
    contract: Contract<'a>,
    recovered: &'run mut Vec<Diagnostic>,
    copies: Option<&'run mut Vec<CopyRead>>,
    events: Option<&'run mut Vec<OwnershipEvent>>,
) -> Checker<'a, 'run> {
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
        facts,
        recovered,
        copies,
        events,
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
        later_use_budget: LATER_USE_NODES,
        lends_exclusive: None,
    };
    for (index, parameter) in parameters.clone().enumerate() {
        let mut value = Value::default();
        // An `Owned.Function` holds no borrow: `Owned.function` rejects an argument that holds one
        // (A30), and its representation is opaque, so nothing else makes one. A parameter of that
        // type, or of a record, tuple, container or union that can hold borrows only in owned
        // functions, or a capture of one, lends the caller's nothing; an owned lambda or a
        // tzroutine body may then capture it, as `ErrGroup.start` does.
        let types = module.types();
        if !task && parameter.ty.carries_loans_beside_owned_functions(&types) {
            checker.external_loans(parameter, index, contract.slots, &mut value);
        }
        checker
            .state
            .locals
            .insert(parameter.id, (parameter, value));
    }
    checker
}

/// The ownership events of the abstract bodies of `functions` (G26 phase 5): each body is
/// checked once more, as the inference does, with an event sink.
pub(crate) fn events_abstract(
    module: &CheckedModule,
    functions: &[usize],
) -> BTreeMap<usize, Vec<OwnershipEvent>> {
    let facts = call_facts(module, true);
    let mut result = BTreeMap::new();
    for &id in functions {
        let Some(function) = module.functions.get(id) else {
            continue;
        };
        let mut events = Vec::new();
        let checked = check_body(
            module,
            function.parameters.iter(),
            &function.body,
            true,
            &facts,
            function.is_once,
            module.user_drops.values().any(|drop| *drop == id),
            Contract::of(function),
            &mut Vec::new(),
            None,
            Some(&mut events),
        );
        if checked.is_ok() {
            result.insert(id, events);
        }
    }
    result
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
        | E::RoutineGo(_)
        | E::SelectWait(_)
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

/// What the bodies of the functions prove about their direct calls: the results that hold no
/// borrow at all (`closed_returns`) and, for the others, the arguments whose loans a result can
/// hold (A28, `parameter_retention`).
struct CallFacts {
    closed: Vec<bool>,
    /// By function: whether its result can hold each argument's loans. `None` holds them all.
    retained: Vec<Option<Box<[bool]>>>,
}

impl CallFacts {
    /// Whether a complete direct call of `function` passes none of the loans of its argument
    /// `index` to its result.
    fn temporary(&self, function: usize, index: usize) -> bool {
        self.retained
            .get(function)
            .and_then(Option::as_deref)
            .is_some_and(|retained| retained.get(index) == Some(&false))
    }
}

fn call_facts(module: &CheckedModule, infer: bool) -> CallFacts {
    let closed = closed_returns(module);
    let retained = parameter_retention(module, &closed, infer);
    CallFacts { closed, retained }
}

/// Whether the facts of `parameter_retention` decide the loans of a call result of type `result`:
/// one that may hold borrows only through function environments (and type variables, in the
/// abstract bodies). A result whose type stores a reference keeps the borrows of every argument
/// unless named regions say otherwise: the signature, not the body, is the contract of a reference
/// (`ownership-and-memory/lifetimes.md`), so a caller does not break when the body changes which
/// input it returns. A function type cannot name the borrows of its environment, so only the body
/// can tell which arguments it captures.
fn retention_applies(result: &Type, types: &crate::check::TypeContext<'_>) -> bool {
    result.carries_loans(types) && !result.contains_stored_reference(types)
}

/// Whether `parameter_retention` computes the facts of function `id`: not those whose results
/// named regions already decide (A12, A13), the generated functions that apply builtins, tzroutine
/// bodies and `Drop.drop` bodies.
fn retention_eligible(module: &CheckedModule, id: usize) -> bool {
    let function = &module.functions[id];
    function.module != "$builtin"
        && !function.is_once
        && !function.parameters.is_empty()
        && function.region_sources.is_none()
        && function.region_slots.is_none()
        && function.callback_contracts.is_empty()
        && !module.user_drops.values().any(|drop| *drop == id)
}

/// The function that a call's callee names directly.
fn direct_callee(callee: &TypedExpr) -> Option<usize> {
    match callee.kind {
        E::Function(crate::check::FunctionRef::User(id)) | E::GenericFunction(id, _) => Some(id),
        _ => None,
    }
}

/// How many bodies `parameter_retention` may check for each function that it computes, and in
/// all beyond that; past it every result keeps all its arguments' loans, as before A28.
const RETENTION_CHECKS_PER_FUNCTION: usize = 8;
const RETENTION_EXTRA_CHECKS: usize = 1024;

/// A28: for each function that some complete direct call needs it for, which arguments its result
/// can hold the loans of. The body is checked as any body is, with a loan standing for each
/// parameter's borrows from its caller, and the parameters whose loans reach its result (directly
/// or as the parents of a loan in it) are retained. Signatures alone cannot tell `pass _key f = f`
/// from `keep text = \() -> String.length text`; the specialized bodies can, so each instance of a
/// generic function has its own facts.
///
/// The facts are the least fixed point over the call graph: every argument starts temporary and is
/// raised to retained when a check of the body, under the current facts of its callees, shows that
/// it reaches the result. A recursive call is thus assumed not to retain until its own body shows
/// that it does, which holds by induction on the depth of the calls. A body that fails its check
/// retains everything, as does a parameter that gets no loan in the body although it may hold
/// borrows: one of a type variable, whose borrows the abstract body of a generic function does not
/// see, and an `Owned.Function`, which is checked where it is made (A30). The abstract bodies give
/// facts only to the inference of the Copy constraints; the specialized bodies, whose types are
/// concrete, give the facts of the check that accepts or rejects the program.
/// Functions whose results named regions already decide (A12, A13), the generated functions that
/// apply builtins, `Drop.drop` bodies and tzroutine bodies keep all their arguments' loans.
#[inline(never)]
fn parameter_retention(
    module: &CheckedModule,
    closed: &[bool],
    infer: bool,
) -> Vec<Option<Box<[bool]>>> {
    parameter_retention_within(module, closed, infer, None)
}

/// `parameter_retention` with `budget` checks of bodies in place of the default.
fn parameter_retention_within(
    module: &CheckedModule,
    closed: &[bool],
    infer: bool,
    budget: Option<usize>,
) -> Vec<Option<Box<[bool]>>> {
    let count = module.functions.len();
    let types = module.types();
    // The complete direct calls whose results may hold loans and are not proven closed, and the
    // pipelines that complete one (`f |> pass (ref key)`).
    let mut callers: BTreeMap<usize, BTreeSet<usize>> = BTreeMap::new();
    let mut callees: BTreeMap<usize, BTreeSet<usize>> = BTreeMap::new();
    for (caller, function) in module.functions.iter().enumerate() {
        let mut pending = vec![&function.body];
        while let Some(expression) = pending.pop() {
            let call = match &expression.kind {
                E::Call(callee, arguments) => Some((callee, arguments.len())),
                E::Binary(BinaryOp::Pipe, _, right) => match &right.kind {
                    E::Call(callee, arguments) => Some((callee, arguments.len() + 1)),
                    _ => None,
                },
                _ => None,
            };
            if let Some((callee, arguments)) = call
                && let Some(id) = direct_callee(callee)
                && !closed[id]
                && retention_eligible(module, id)
                && arguments >= module.functions[id].parameters.len()
                && if arguments == module.functions[id].parameters.len() {
                    retention_applies(&expression.ty, &types)
                } else {
                    retention_applies(
                        &callee
                            .ty
                            .after_arguments(module.functions[id].parameters.len()),
                        &types,
                    )
                }
            {
                callers.entry(id).or_default().insert(caller);
                callees.entry(caller).or_default().insert(id);
            }
            expression
                .try_for_each_child(&mut |child| {
                    pending.push(child);
                    Ok(())
                })
                .expect("collecting calls cannot fail");
        }
    }
    let mut facts = CallFacts {
        closed: closed.to_vec(),
        retained: vec![None; count],
    };
    for &id in callers.keys() {
        facts.retained[id] = Some(
            module.functions[id]
                .parameters
                .iter()
                .map(|parameter| {
                    // The parameters that may hold borrows but get no loan in the body: those
                    // that `start_checker` gives no caller loan (borrows only in owned
                    // functions, as A30 has it) and type variables of an abstract body.
                    (parameter.ty.carries_loans(&types) || parameter.ty.contains_unknown())
                        && !parameter.ty.carries_loans_beside_owned_functions(&types)
                })
                .collect(),
        );
    }
    // Callees before their callers, so that a chain without recursion settles in one pass.
    let mut order = Vec::with_capacity(callers.len());
    let mut visited = BTreeSet::new();
    let children = |id: usize| -> Vec<usize> {
        callees
            .get(&id)
            .into_iter()
            .flatten()
            .copied()
            .filter(|callee| callers.contains_key(callee))
            .rev()
            .collect()
    };
    for &root in callers.keys() {
        if !visited.insert(root) {
            continue;
        }
        let mut stack = vec![(root, children(root))];
        while let Some((_, next)) = stack.last_mut() {
            match next.pop() {
                Some(child) => {
                    if visited.insert(child) {
                        stack.push((child, children(child)));
                    }
                }
                None => {
                    let (id, _) = stack.pop().expect("the stack is not empty");
                    order.push(id);
                }
            }
        }
    }
    let mut queued: BTreeSet<usize> = order.iter().copied().collect();
    let mut work: std::collections::VecDeque<usize> = order.into();
    let mut budget =
        budget.unwrap_or(callers.len() * RETENTION_CHECKS_PER_FUNCTION + RETENTION_EXTRA_CHECKS);
    while let Some(id) = work.pop_front() {
        if budget == 0 {
            // The functions still to check, and every function whose facts were computed from
            // theirs, keep all their arguments' loans. The others were checked under the facts
            // that their callees have now, so they stay a fixed point of their part of the graph.
            let mut unsettled = queued;
            let mut pending: Vec<usize> = unsettled.iter().copied().collect();
            while let Some(callee) = pending.pop() {
                for caller in callers.get(&callee).into_iter().flatten() {
                    if callers.contains_key(caller) && unsettled.insert(*caller) {
                        pending.push(*caller);
                    }
                }
            }
            for id in unsettled {
                facts.retained[id] = None;
            }
            return facts.retained;
        }
        queued.remove(&id);
        budget -= 1;
        let old = facts.retained[id].as_deref().expect("a computed function");
        let new: Box<[bool]> = match retained_parameters(module, id, infer, &facts) {
            Some(found) => old
                .iter()
                .zip(&found)
                .map(|(old, new)| *old || *new)
                .collect(),
            None => vec![true; old.len()].into_boxed_slice(),
        };
        if *old == *new {
            continue;
        }
        facts.retained[id] = Some(new);
        for caller in callers.get(&id).into_iter().flatten() {
            if callers.contains_key(caller) && queued.insert(*caller) {
                work.push_back(*caller);
            }
        }
    }
    facts.retained
}

/// The parameters of `function` whose loans its result holds, under the facts so far; `None`
/// when the body does not check, when its result holds a loan of its own locals, or when it lends
/// an exclusive borrow to a call (`lends_exclusive`).
fn retained_parameters(
    module: &CheckedModule,
    id: usize,
    infer: bool,
    facts: &CallFacts,
) -> Option<Box<[bool]>> {
    let function = &module.functions[id];
    if lends_exclusive(&function.body, &module.types()) {
        return None;
    }
    let mut recovered = Vec::new();
    let mut checker = start_checker(
        module,
        function.parameters.iter(),
        &function.body,
        infer,
        facts,
        false,
        false,
        Contract::of(function),
        &mut recovered,
        None,
        None,
    );
    let result = checker
        .eval(&function.body, Use::Consume, &BTreeSet::new())
        .ok()?;
    if !checker.recovered.is_empty() {
        return None;
    }
    let roots: BTreeMap<usize, usize> = function
        .parameters
        .iter()
        .enumerate()
        .map(|(index, parameter)| (usize::MAX - parameter.id, index))
        .collect();
    if result.loans.iter().any(|loan| {
        let root = checker.loans[*loan].place.root;
        !roots.contains_key(&root) && !is_static_root(root)
    }) {
        return None;
    }
    let mut retained = if infer {
        hidden_parameters(module, function)
    } else {
        vec![false; function.parameters.len()]
    };
    let mut pending: Vec<usize> = result.loans.iter().copied().collect();
    let mut seen = BTreeSet::new();
    while let Some(loan) = pending.pop() {
        if !seen.insert(loan) {
            continue;
        }
        let loan = &checker.loans[loan];
        if let Some(index) = roots.get(&loan.place.root) {
            retained[*index] = true;
        }
        pending.extend(&loan.parents);
    }
    Some(retained.into_boxed_slice())
}

/// Whether `body` passes to a call an argument whose type reaches an exclusive reference: an
/// exclusive borrow (`ref mut values`, `ref mut values[0..1]`), a `ref mut` parameter or local, or
/// a value that holds one (a tuple, a record with an exclusive region, a `MatrixView.Mut`), in a
/// call, a pipeline that completes one, a host or parallel call, or the captures of a closure.
///
/// A28 trusts the loans that the check of a body gives its result: the facts drop the arguments
/// that do not reach it, and `read_through` takes what a borrowed owner holds. Both are sound only
/// when the check sees every value that the body stores where its result can read it. The check
/// does not track the stores of the in-place writers `Array.write` (and `Array.set`, `Array.fill`,
/// `Array.blit` and the other std functions built on it) and `SyncMap.__replace`: the stored
/// value's loans do not reach the owner of the target (A31). Each of them takes its target as an
/// exclusive reference argument, and a function that writes into a place that its caller lends
/// it (a callback, a std function) gets that place only through an exclusive borrow too; such a
/// callee keeps all its arguments' loans because its own body lends one. Other stores are tracked
/// (an assignment, `deref r = v`, `Vec.push_mut`, a function with a region contract). So a body
/// that passes no exclusive borrow to any call stores nothing that its check misses, and a body
/// that passes one keeps the rule from before A28: its result holds every argument's loans, and
/// `Rc.share` and its kin hold the borrow of the owner.
///
/// In an abstract body a type variable counts as no exclusive reference: those facts only infer
/// Copy constraints, and each instance, whose types are concrete, is checked with its own facts.
fn lends_exclusive(body: &TypedExpr, types: &crate::check::TypeContext<'_>) -> bool {
    let exclusive = |argument: &TypedExpr| {
        argument.ty.reaches_exclusive(types) || argument.ty.holds_exclusive_record(types)
    };
    let mut pending = vec![body];
    while let Some(expression) = pending.pop() {
        let lends = match &expression.kind {
            E::Call(_, arguments)
            | E::HostCall(_, arguments)
            | E::Parallel(_, arguments)
            | E::Closure(_, arguments) => arguments.iter().any(exclusive),
            E::Binary(BinaryOp::Pipe, left, _) => exclusive(left),
            _ => false,
        };
        if lends {
            return true;
        }
        expression
            .try_for_each_child(&mut |child| {
                pending.push(child);
                Ok(())
            })
            .expect("collecting calls cannot fail");
    }
    false
}

/// The parameters of an abstract body whose loans may pass through a value whose type has a type
/// variable and carries no loans, as `x` does in `wrap (f x)` with `f x : 'v`: the check of the
/// abstract body gives such a value no loans, although an instance whose `'v` is a function holds
/// there what `x` lends. The abstract body does not see the stores of `Array.write` and of the
/// functions that write through an exclusive reference either. The inference keeps these
/// parameters, as the checks of the instances may. A parameter reaches such a value when the
/// value's expression uses it, or a local bound from it, other than through a part whose concrete
/// type carries no loans (`hash_of key : i64`); a value that such a part consumes whole cannot
/// reach the result, unless it is bound, assigned or stored.
fn hidden_parameters(
    module: &CheckedModule,
    function: &crate::check::CheckedFunction,
) -> Vec<bool> {
    let types = module.types();
    let plain = |expression: &TypedExpr| {
        !expression.ty.contains_unknown() && !expression.ty.carries_loans(&types)
    };
    let mentioned = |expression: &TypedExpr| -> BTreeSet<usize> {
        let mut found = BTreeSet::new();
        let mut pending = vec![expression];
        while let Some(expression) = pending.pop() {
            if plain(expression) {
                continue;
            }
            if let E::Local(id) = expression.kind {
                found.insert(id);
            }
            pending.extend(expression.children());
        }
        found
    };
    // A call that may store an argument through an exclusive reference.
    let stores = |callee: &TypedExpr| match &callee.ty {
        Type::Function(parameters, _) => parameters
            .iter()
            .any(|parameter| matches!(parameter, Type::Reference(_, true))),
        _ => false,
    };
    // The locals that each binding makes from the locals that its value uses, and the locals that
    // each hiding value uses.
    let mut bindings: Vec<(Vec<usize>, BTreeSet<usize>)> = Vec::new();
    let mut hiding: Vec<BTreeSet<usize>> = Vec::new();
    // Each expression, and whether a part of a plain type consumes its value whole.
    let mut pending = vec![(&function.body, false)];
    while let Some((expression, consumed)) = pending.pop() {
        if !consumed
            && !matches!(expression.kind, E::Local(_))
            && expression.ty.contains_unknown()
            && !expression.ty.carries_loans(&types)
        {
            hiding.push(mentioned(expression));
        }
        // The values that escape the expression's own value.
        let mut escaping: Vec<&TypedExpr> = Vec::new();
        match &expression.kind {
            E::Block {
                bindings: block, ..
            } => {
                for (local, value) in block {
                    bindings.push((vec![local.id], mentioned(value)));
                    escaping.push(value);
                }
            }
            E::Match { local, value, arms } => {
                bindings.push((vec![local.id], mentioned(value)));
                escaping.push(value);
                for alternative in arms.iter().flat_map(|arm| &arm.alternatives) {
                    for step in &alternative.steps {
                        if let crate::check::PatternStep::Bind(bound, value) = step {
                            bindings.push((vec![bound.id], mentioned(value)));
                        }
                    }
                    for (bound, value) in &alternative.bindings {
                        bindings.push((vec![bound.id], mentioned(value)));
                    }
                }
            }
            E::ForEach {
                owner,
                local,
                source,
                ..
            } => bindings.push((vec![owner.id, local.id], mentioned(source))),
            E::Assign(place, value) => {
                let mut root = &**place;
                while let E::Field(inner, _) | E::Index(inner, _) | E::Dereference(inner) =
                    &root.kind
                {
                    root = inner;
                }
                if let E::Local(id) = root.kind {
                    bindings.push((vec![id], mentioned(value)));
                }
                escaping.push(value);
            }
            E::Call(callee, arguments) if stores(callee) => {
                for argument in arguments {
                    hiding.push(mentioned(argument));
                }
            }
            _ => {}
        }
        let consumed = consumed || plain(expression);
        for child in expression.children() {
            let escapes = escaping.iter().any(|value| std::ptr::eq(*value, child));
            pending.push((child, consumed && !escapes));
        }
    }
    if hiding.iter().all(BTreeSet::is_empty) {
        return vec![false; function.parameters.len()];
    }
    function
        .parameters
        .iter()
        .map(|parameter| {
            let mut reached = BTreeSet::from([parameter.id]);
            loop {
                let before = reached.len();
                for (bound, uses) in &bindings {
                    if !uses.is_disjoint(&reached) {
                        reached.extend(bound);
                    }
                }
                if reached.len() == before {
                    break;
                }
            }
            hiding.iter().any(|uses| !uses.is_disjoint(&reached))
        })
        .collect()
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

fn error(kind: DiagnosticKind, message: impl Into<String>, span: Span) -> Diagnostic {
    Diagnostic::of(kind, message, span)
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
    /// What the bodies of the callees prove about their results (`call_facts`).
    facts: &'run CallFacts,
    recovered: &'run mut Vec<Diagnostic>,
    /// Collects the copied consuming reads for `copies::sites`; the checks stay the same.
    copies: Option<&'run mut Vec<CopyRead>>,
    /// Collects the ownership events for the knowledge graph (G26); the checks stay the same.
    events: Option<&'run mut Vec<OwnershipEvent>>,
    reported: BTreeSet<usize>,
    /// Parameters with region-quantified function types, by local id (A12 Phase 2).
    callbacks: BTreeMap<usize, &'a CallbackContract>,
    /// The root of the value that the checked `Drop.drop` body drops.
    drop_root: Option<usize>,
    /// How many more steps the diagnostics may take in this function to find where a moved value
    /// or a borrow came from (`Flow`).
    later_use_budget: usize,
    /// Whether the body lends an exclusive borrow to a call (`lends_exclusive`), once asked.
    lends_exclusive: Option<bool>,
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
                            return Err(error(
                                DiagnosticKind::CallbackContractUnsatisfied,
                                message,
                                argument.span,
                            ));
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
                        self.facts,
                        function.is_once,
                        false,
                        Contract {
                            result: Some(&shifted),
                            callbacks: &[],
                            slots: None,
                        },
                        &mut Vec::new(),
                        None,
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
        closed(body, self.module, &self.facts.closed, &locals)
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

    /// A28: the function whose call the pipeline `expression`, `x |> right`, completes, when its
    /// facts decide the loans of the result: `right` applies a function that has facts to all its
    /// arguments but the last.
    fn piped_callee(&self, expression: &TypedExpr, right: &TypedExpr) -> Option<usize> {
        let E::Call(callee, arguments) = &right.kind else {
            return None;
        };
        let id = direct_callee(callee)?;
        (retention_eligible(self.module, id)
            && arguments.len() + 1 == self.module.functions[id].parameters.len()
            && retention_applies(&expression.ty, &self.module.types()))
        .then_some(id)
    }

    /// The loans that `right`, the partial application of function `id` that a pipeline
    /// completes, passes to the result of the call: those of the callee and of each argument that
    /// the facts of `id` do not show to be temporary. Each argument is evaluated, and stays lent
    /// while the next ones are, as in `eval_call`.
    fn eval_piped_arguments(
        &mut self,
        id: usize,
        right: &TypedExpr,
        live: &BTreeSet<usize>,
    ) -> Result<Value, Diagnostic> {
        let E::Call(callee, arguments) = &right.kind else {
            unreachable!("piped_callee takes calls")
        };
        let mut during = live.clone();
        uses(right, &mut during);
        let start = self.held.len();
        let mut result = self.eval(callee, Use::Consume, &during)?;
        self.held.push(result.clone());
        for (index, argument) in arguments.iter().enumerate() {
            let mark = self.events.as_ref().map_or(0, |events| events.len());
            let value = self.eval(argument, Use::Consume, &during)?;
            self.event_targets(mark, argument.span, callee);
            if !self.facts.temporary(id, index) {
                result.loans.extend(&value.loans);
            }
            self.held.push(value);
        }
        self.held.truncate(start);
        result.regions = None;
        Ok(result)
    }

    /// A28: whether `callee` is `share`, `downgrade` or `upgrade` of `Rc` or `Arc` (or the generated
    /// function that applies one). Each makes a new owner of the block that its first argument
    /// borrows, so its result holds what the block holds, not the borrow of the owner: the owner
    /// may move or be dropped while the result lives. `get` returns the borrow itself, and other
    /// builtins may pass their references to callbacks, so they keep every argument's loans.
    fn reads_through(&self, callee: &TypedExpr) -> bool {
        use crate::check::Builtin::{
            ArcDowngrade, ArcShare, ArcUpgrade, RcDowngrade, RcShare, RcUpgrade,
        };
        matches!(
            self.builtin_called(callee),
            Some(RcShare | ArcShare | RcDowngrade | ArcDowngrade | RcUpgrade | ArcUpgrade)
        )
    }

    /// Whether the checked body lends an exclusive borrow to a call: an in-place writer may then
    /// have stored in the owner what its loans miss, so nothing is read through the owner's
    /// borrow (`lends_exclusive`).
    fn body_lends_exclusive(&mut self) -> bool {
        *self
            .lends_exclusive
            .get_or_insert_with(|| lends_exclusive(self.body, &self.module.types()))
    }

    /// What a value read through the borrows `loans` holds, as `read_places` reads a place: a
    /// borrow of a local holds the loans that the local held when it was borrowed (its parents);
    /// a borrow from the caller, or of static data, says nothing about what its target holds, so
    /// it stands for itself.
    ///
    /// The result holds a copy of each of those loans, with the same place, mutability and
    /// parents: it keeps the same data borrowed, and the block checks and conflicts see it as they
    /// see the original. It is not the original because a live borrow of the owner (`Rc.get (ref
    /// shared)`, `let r = ref shared`) has the original as a parent, and a value that holds a loan
    /// that some active loan reborrows cannot move; moving the new owner reborrows nothing.
    fn read_through(&mut self, loans: &BTreeSet<usize>) -> BTreeSet<usize> {
        let mut held = BTreeSet::new();
        let mut pending: Vec<usize> = loans.iter().copied().collect();
        let mut seen = BTreeSet::new();
        while let Some(id) = pending.pop() {
            if !seen.insert(id) {
                continue;
            }
            let loan = &self.loans[id];
            if self.external.contains(&loan.place.root) || is_static_root(loan.place.root) {
                held.insert(id);
                continue;
            }
            // A reborrow of the same place, as `(ref shared) |> Arc.share` makes, has the borrow
            // that it reborrows as a parent: read through that one too.
            for parent in &loan.parents {
                if self.loans[*parent].place == loan.place {
                    pending.push(*parent);
                } else {
                    held.insert(*parent);
                }
            }
        }
        held.into_iter()
            .map(|id| {
                let copy = self.loans[id].clone();
                self.loans.push(copy);
                self.loans.len() - 1
            })
            .collect()
    }

    /// What `Mutex.with_lock` returns when `callee` is that builtin, or the generated function
    /// that calls it after lowering: the type after its lock and callback, which is a function
    /// when the callback's result is one (F10). The same holds for the callbacks that run under
    /// the locks of TZ06, `RwMutex.with_read`, `RwMutex.with_write` and the body of
    /// `Cond.wait_with`. Also the index of that callback among the arguments.
    fn locked_result(&self, callee: &TypedExpr) -> Option<(Type, usize)> {
        use crate::check::Builtin::{
            CondWaitWith, MutexWith, OnceValueGet, RwMutexWithRead, RwMutexWithWrite,
        };
        let builtin = self.builtin_called(callee)?;
        let callback = match builtin {
            MutexWith | RwMutexWithRead | RwMutexWithWrite | OnceValueGet => 1,
            CondWaitWith => 3,
            _ => return None,
        };
        match &callee.ty {
            // `OnceValue.get` keeps what its initializer returns in the cell, which outlives the
            // call: that value, the `'a` of the borrowed `OnceValue<'a>`, is the one checked (TZ06;
            // a curried initializer's own type is flattened).
            Type::Function(parameters, _) if builtin == OnceValueGet && parameters.len() > 1 => {
                match &parameters[0] {
                    Type::Reference(cell, _) => match cell.as_ref() {
                        Type::Record(_, arguments) if arguments.len() == 1 => {
                            Some((arguments[0].clone(), callback))
                        }
                        _ => None,
                    },
                    _ => None,
                }
            }
            Type::Function(parameters, _) if parameters.len() > callback => {
                Some((callee.ty.after_arguments(callback + 1), callback))
            }
            _ => None,
        }
    }

    /// The type of the item that `callee` puts in a channel when it is `DataChannel.send` (F10),
    /// `Channel.send` or `Channel.try_send` (TZ03), or the generated function that calls one. The
    /// item stays in the channel after the call.
    fn sent_item(&self, callee: &TypedExpr) -> Option<Type> {
        use crate::check::Builtin::{
            ChannelSend, ChannelTrySend, DataChannelSend, SendOnlySend, SendOnlyTrySend,
        };
        match &callee.ty {
            Type::Function(parameters, _)
                if matches!(
                    self.builtin_called(callee),
                    Some(
                        DataChannelSend
                            | ChannelSend
                            | ChannelTrySend
                            | SendOnlySend
                            | SendOnlyTrySend
                    )
                ) && parameters.len() >= 2 =>
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
                    DiagnosticKind::TaskResultRetainsBorrow,
                    if task {
                        "the result of a tzroutine body cannot retain borrowed values"
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
                        DiagnosticKind::ResultRegionMismatch,
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
            let mut diagnostic = Diagnostic::of(
                DiagnosticKind::UseAfterMove,
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
                DiagnosticKind::DropReplaceWholeValue,
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
                    DiagnosticKind::DropPassExclusiveValue,
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
                let diagnostic = Diagnostic::of(
                    DiagnosticKind::MutateImmutableValue,
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

    /// The `E1013` of `Owned.function argument` whose `value` holds a borrow (A30 D2): the message
    /// says whether a lambda's capture or the function value holds it, a note shows where the
    /// borrow was taken (the parameter, for a borrow of the caller's), and the help names the
    /// parameter type that takes an owned function, `own {owned}`.
    #[cold]
    #[inline(never)]
    fn owned_function_holds_borrow(
        &self,
        argument: &TypedExpr,
        value: &Value,
        owned: &Type,
    ) -> Diagnostic {
        // The message follows the source form. A generic body is checked for an instance after its
        // lambdas are lifted: a lambda written as the argument, or piped into `Owned.function`,
        // becomes an `E::Closure` in place, which keeps the lambda's span, the span of the lifted
        // function. A `let rec` name becomes an `E::Closure` only inside the bodies of its group,
        // at the name's span, which lies inside the span of its function.
        let lambda = match &argument.kind {
            E::Lambda { .. } => true,
            E::Closure(id, _) => {
                let function = &self.module.functions[*id];
                function.owned_captures || function.span == argument.span
            }
            _ => false,
        };
        let message = if lambda {
            "an owned function captures only owned values; a captured value holds a borrow"
        } else {
            "an owned function captures only owned values; this function value holds a borrow"
        };
        let mut diagnostic = Diagnostic::of(
            DiagnosticKind::OwnedFunctionCapturesReference,
            message,
            argument.span,
        );
        // The first loan, followed through its parents to the borrow that it was taken from. The
        // loans are a set and the parents point at older loans, so the choice is deterministic and
        // the walk ends.
        if let Some(&first) = value.loans.first() {
            let mut id = first;
            for _ in 0..self.loans.len() {
                match self.loans[id].parents.first() {
                    Some(&parent) => id = parent,
                    None => break,
                }
            }
            let loan = &self.loans[id];
            if loan.span.end > loan.span.start && loan.span != argument.span {
                let note = if self.external.contains(&loan.place.root) {
                    "a parameter may hold a borrow of its caller's values"
                } else {
                    "value borrowed here"
                };
                diagnostic.add_related(RelatedRole::BorrowedHere, note, loan.span);
            }
        }
        diagnostic.with_help(format!(
            "capture an owned value, such as the owner itself or a copy, instead of a borrow; a function that receives work to run later takes an 'own {}' parameter instead of a function value",
            owned.display(&self.module.types())
        ))
    }

    /// The `E1014` of an access at `span` that conflicts with the live loan `loan`: where the loan
    /// was taken, and where the reference that keeps it alive is used later.
    #[cold]
    #[inline(never)]
    fn conflict(&mut self, loan: usize, span: Span) -> Diagnostic {
        let mut diagnostic = Diagnostic::of(
            DiagnosticKind::BorrowConflict,
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
                        DiagnosticKind::ReferenceWithoutOwner,
                        "reference has no live owner",
                        expression.span,
                    ));
                }
                Ok(places)
            }
            _ => Err(error(
                DiagnosticKind::BorrowOfTemporary,
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
                            DiagnosticKind::MutateThroughSharedReference,
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
            self.borrow_event(&place, via.is_empty(), true, span);
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
                    DiagnosticKind::StoreRegionUnnamed,
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
                        DiagnosticKind::StoreLocalBorrowThroughReference,
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
                        DiagnosticKind::StoreRegionMismatch,
                        "the stored borrow does not have the region of the reference's target; store a borrow of an input with that region",
                        span,
                    ));
                }
            }
            return Ok(());
        }
        let Some((_, owner)) = self.state.locals.get_mut(&place.root) else {
            return Err(error(DiagnosticKind::StoreRegionUnnamed, UNNAMED, span));
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
                            DiagnosticKind::SelfReferentialStore,
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
                DiagnosticKind::MoveOutOfCollectionElement,
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
                    DiagnosticKind::MoveOutOfCollectionElement,
                    "cannot move a non-Copy value out of an array or list element; borrow it instead",
                    expression.span,
                ));
            }
            if moving && !via.is_empty() {
                return Err(error(
                    DiagnosticKind::MoveOutOfReferenceBinding,
                    if via.iter().any(|id| self.loans[*id].view.is_some()) {
                        "cannot move a non-Copy pattern variable bound through a reference or a collection element; borrow it with 'ref' instead"
                    } else {
                        "cannot move a non-Copy value out of a reference"
                    },
                    expression.span,
                ));
            }
            if moving && place.through_drop {
                return Err(error(
                    DiagnosticKind::MoveOutOfDropType,
                    MOVE_OUT_OF_DROP,
                    expression.span,
                ));
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
                        DiagnosticKind::NestedBorrowUnsupported,
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
                        DiagnosticKind::MoveReborrowedExclusiveReference,
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
                self.event(
                    place.root,
                    EventKind::Move,
                    expression.span,
                    Some(&expression.ty),
                );
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
        match expression.kind {
            E::While { .. } | E::ForRange { .. } | E::ForEach { .. } | E::Match { .. } => {
                self.eval_control(expression, live)
            }
            E::Block { .. } => self.eval_block(expression, live),
            E::Call(..) => self.eval_call(expression, live),
            E::Lambda { .. } => self.eval_lambda(expression, live),
            _ => self.eval_value(expression, usage, live),
        }
    }

    /// Records an ownership event of the variable at `root` (G26 phase 5).
    fn event(&mut self, root: usize, kind: EventKind, span: Span, moved: Option<&Type>) {
        if self.events.is_none() || is_static_root(root) {
            return;
        }
        let id = if self.external.contains(&root) {
            usize::MAX - root
        } else {
            root
        };
        let Some(local) = self.declarations.get(&id) else {
            return;
        };
        // A variable that the compiler generated is no variable of the source.
        if local.provenance != crate::syntax::Provenance::User || local.name.starts_with('$') {
            return;
        }
        let var = local.name.clone();
        if let Some(events) = self.events.as_deref_mut() {
            events.push(OwnershipEvent {
                var,
                kind,
                to: None,
                span,
                moved: moved.cloned(),
            });
        }
    }

    /// A borrow event: a borrow of a variable itself, or a reborrow through a reference.
    fn borrow_event(&mut self, place: &Place, direct: bool, mutable: bool, span: Span) {
        let kind = match (direct, mutable) {
            (false, _) => EventKind::Reborrow,
            (true, false) => EventKind::Borrow,
            (true, true) => EventKind::BorrowMut,
        };
        self.event(place.root, kind, span, None);
    }

    /// `drop` events at `end` for the locals `ids` that are dropped there: declared in the source,
    /// needing a drop, and not moved (in part or whole) on the way.
    fn drop_events(&mut self, ids: &[usize], end: Span) {
        let types = self.module.types();
        for &id in ids {
            let Some(local) = self.declarations.get(&id).copied() else {
                continue;
            };
            if local.provenance != crate::syntax::Provenance::User
                || !local.ty.needs_drop(&types)
                || self.state.moved.keys().any(|place| place.root == id)
            {
                continue;
            }
            if let Some(events) = self.events.as_deref_mut() {
                events.push(OwnershipEvent {
                    var: local.name.clone(),
                    kind: EventKind::Drop,
                    to: None,
                    span: end,
                    moved: None,
                });
            }
        }
    }

    /// Gives the events of an argument at `span`, recorded from `mark` on, the callee as target.
    fn event_targets(&mut self, mark: usize, span: Span, callee: &TypedExpr) {
        let Some(events) = self.events.as_deref_mut() else {
            return;
        };
        let target = match &callee.kind {
            E::Function(crate::check::FunctionRef::User(id)) | E::GenericFunction(id, _) => {
                EventTarget::User(*id)
            }
            E::Function(crate::check::FunctionRef::Builtin(instance)) => {
                EventTarget::Builtin(instance.builtin)
            }
            E::Method(class, method, _) => EventTarget::Method(*class, *method),
            _ => return,
        };
        for event in events.iter_mut().skip(mark) {
            if event.span == span && event.to.is_none() {
                event.to = Some(target.clone());
            }
        }
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
                        Diagnostic::of(
                            DiagnosticKind::BorrowOutlivesBlock,
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
                        error(DiagnosticKind::AssignmentExtendsBorrow,
                            "assignment lets a reference outlive its owner",
                            expression.span,
                        ),
                    )
                    .with_help(
                        "declare the owner before the variable that keeps the reference, in the same block or an outer one",
                    ));
            }
        }
        if self.events.is_some() {
            let end = Span {
                start: expression.span.end,
                ..expression.span
            };
            let ids: Vec<usize> = ids.iter().copied().collect();
            self.drop_events(&ids, end);
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
        // and one of `DataChannel.send` checks its item; any other use of either builtin as a value is
        // checked where it is evaluated.
        let locked = self
            .locked_result(callee)
            .filter(|(_, callback)| arguments.len() > *callback);
        let sent = self.sent_item(callee).filter(|_| arguments.len() >= 2);
        // A direct call of `Owned.function` checks its argument for borrows below (A30), so the
        // callee is not evaluated as a value, which `eval_value` rejects.
        let owned_function = !arguments.is_empty()
            && self.builtin_called(callee) == Some(crate::check::Builtin::OwnedFunction);
        // The result of a full call of `Owned.call` borrows only what its input lends: the owned
        // function holds no borrow (A30), and its body cannot return one of its own environment.
        // `Owned.call (ref f)` alone is a function that keeps the borrow of `f`.
        let owned_call = arguments.len() >= 2
            && self.builtin_called(callee) == Some(crate::check::Builtin::OwnedCall);
        // What comes out of a channel was checked when it went in; `Channel.try_send` gives back
        // the item that it checked. Only a full call gives that: `Channel.try_send ch` is a function
        // that borrows the channel.
        let received = match self.builtin_called(callee) {
            Some(
                crate::check::Builtin::DataChannelRecv
                | crate::check::Builtin::ChannelRecv
                | crate::check::Builtin::ChannelTryRecv
                | crate::check::Builtin::ReceiveOnlyRecv
                | crate::check::Builtin::ReceiveOnlyTryRecv,
            ) => !arguments.is_empty(),
            Some(
                crate::check::Builtin::ChannelTrySend | crate::check::Builtin::SendOnlyTrySend,
            ) => arguments.len() >= 2,
            _ => false,
        };
        let value = if locked.is_some() || sent.is_some() || owned_function {
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
        // A28: the arguments of a complete call whose result holds borrows only through function
        // environments, which the callee's facts select.
        let retention = boundary.filter(|boundary| {
            !callee
                .ty
                .after_arguments(*boundary)
                .contains_stored_reference(&self.module.types())
        });
        let read_through = self.reads_through(callee) && !self.body_lends_exclusive();
        // A region-quantified parameter's result borrows only from the inputs that its type names.
        let mut current = if callback {
            Value::default()
        } else {
            value.clone()
        };
        current.regions = region_sources.and_then(call_slots);
        self.held.push(value);
        for (index, argument) in arguments.iter().enumerate() {
            let mark = self.events.as_ref().map_or(0, |events| events.len());
            let value = self.eval(argument, Use::Consume, &during)?;
            self.event_targets(mark, argument.span, callee);
            // The callback runs under the lock, so a result that holds a borrow of the locked value
            // would use it after the lock is released: the value may be replaced and freed, or be
            // written by another tzroutine, by then. Only a result that is proven free of borrows made
            // inside the callback may hold borrows at all (F10). The generated function that
            // applies the builtin to its own parameters proves nothing: its callers are checked.
            // `OnceValue.get` keeps the value in the cell, which outlives any borrow: its
            // initializer may hold none, from outside either (TZ06).
            let kept = self.builtin_called(callee) == Some(crate::check::Builtin::OnceValueGet);
            if let Some((returned, callback)) = &locked
                && index == *callback
                && returned.carries_loans(&self.module.types())
                && (!value.closed_result[0] || (kept && !value.loans.is_empty()))
                && !self.is_generated_parameter(argument)
            {
                let builtin = self.builtin_called(callee);
                let name = builtin.map_or("Mutex.with_lock", |b| b.name());
                let message = if builtin == Some(crate::check::Builtin::OnceValueGet) {
                    format!(
                        "{name} initializers must return values proven free of borrowed environments: the cell keeps the value after the borrowed data is gone; return an owned value, or a function that captures a copy"
                    )
                } else {
                    format!(
                        "{name} results must be proven free of borrowed environments: a function or other value that holds a borrow of the locked value would use it after the lock is released; return an owned value, or a function that captures a copy"
                    )
                };
                return Err(error(
                    DiagnosticKind::MutexResultRetainsBorrow,
                    message,
                    argument.span,
                ));
            }
            // A30: an `Owned.Function` owns what it captures, and `Routine.go`, `WaitGroup.go`,
            // `ErrGroup.go`, `Time.after_func` and `Context.after_func` run it later, perhaps after
            // the data it borrows is gone, without looking at its loans (D1: the check is where it
            // is made). A named function holds no loan. A lambda written here whose captures are
            // references is reported by `src/closures.rs` before this check runs; one that gets
            // here holds a loan only through a captured value, such as a function value that
            // captured a reference. A parameter of the generated function that applies the builtin
            // passes on what its callers gave it, and `eval_value` rejects those callers.
            if owned_function
                && index == 0
                && !value.loans.is_empty()
                && !self.is_generated_parameter(argument)
            {
                return Err(self.owned_function_holds_borrow(argument, &value, &expression.ty));
            }
            if !value.loans.is_empty() && owned_dyn_of(callee) {
                return Err(error(
                    DiagnosticKind::DynHoldsBorrowWithoutRegion,
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
                    DiagnosticKind::ChannelItemHoldsBorrow,
                    "a channel item cannot hold a borrow, including a borrowed function environment: it is received later, perhaps after the borrowed data is gone; send an owned value, or a function that captures a copy, or an Owned.Function built where it is sent",
                    argument.span,
                ));
            }
            match region_sources {
                _ if owned_call && index == 0 => {}
                Some(sources) => {
                    self.argument_regions(sources, slots, index, argument, &value, &mut current)
                }
                // The reference of `OnceValue.get` points into the cell, never into what its
                // initializer borrowed: that was checked above to stay out of the cell.
                None if kept && index == 1 => {}
                // A28: the callee's body never puts this argument in its result. The argument
                // stays lent until the call returns (it is on `held`).
                None if index == 0 && read_through => {
                    // A borrowed temporary's value already has only what the temporary holds.
                    let loans = if matches!(argument.kind, E::BorrowOperand(_)) {
                        value.loans.clone()
                    } else {
                        self.read_through(&value.loans)
                    };
                    current.loans.extend(loans);
                }
                None if retention.is_some_and(|boundary| index < boundary)
                    && known.is_some_and(|id| self.facts.temporary(id, index)) => {}
                None => current.loans.extend(&value.loans),
            }
            self.held.push(value);
            if boundary == Some(index + 1) {
                let id = known.unwrap();
                self.store_arguments(id, arguments, start, expression.span)?;
                if self.facts.closed[id]
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
            self.facts,
            matches!(expression.ty, Type::Once(_)),
            false,
            Contract::default(),
            self.recovered,
            None,
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
            if matches!(expression.ty, Type::Once(_)) && !value.loans.is_empty() {
                return Err(error(
                    DiagnosticKind::TaskCaptureRetainsBorrow,
                    "the captures of a tzroutine cannot retain borrowed values, including borrowed function environments",
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
                return Err(error(DiagnosticKind::MustApplyDirectly,
                    "parallel operations must be fully applied directly; their ownership boundary cannot be erased into an ordinary function value",
                    expression.span,
                ));
            }
            // A result that may hold a borrow is checked at the direct call, so the builtin
            // cannot be applied in stages or kept as a function value then.
            E::Function(_)
                if self
                    .locked_result(expression)
                    .is_some_and(|(returned, _)| returned.carries_loans(&self.module.types())) =>
            {
                let name = self.builtin_called(expression).map_or("Mutex.with_lock", |b| b.name());
                return Err(error(DiagnosticKind::MustApplyDirectly,
                    format!("{name} must be fully applied directly when its result may hold borrowed values, so that the result can be checked: call it with the lock and the callback together"),
                    expression.span,
                ));
            }
            // The same for the item of `DataChannel.send`, `Channel.send` and `Channel.try_send`.
            E::Function(_)
                if self
                    .sent_item(expression)
                    .is_some_and(|item| item.carries_loans(&self.module.types())) =>
            {
                let builtin = self.builtin_called(expression).expect("a send builtin");
                let end = if builtin.is_data_channel() { "sender" } else { "channel" };
                return Err(error(DiagnosticKind::MustApplyDirectly,
                    format!("{} must be fully applied directly when its item may hold a borrow, so that the item can be checked: call it with the {end} and the item together", builtin.name()),
                    expression.span,
                ));
            }
            // `Owned.function` checks its argument at the direct call (A30); as a function value,
            // the borrows of its argument would pass into an `Owned.Function` unchecked.
            E::Function(_)
                if self.builtin_called(expression)
                    == Some(crate::check::Builtin::OwnedFunction) =>
            {
                return Err(error(DiagnosticKind::MustApplyDirectly,
                    "Owned.function must be applied directly, so that its argument can be checked for borrows: call it with the function value together",
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
                    self.borrow_event(&place, via.is_empty(), false, expression.span);
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
                    self.borrow_event(&place, via.is_empty(), *mutable, expression.span);
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
                        return Err(error(DiagnosticKind::SelfReferentialStore,
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
                    if matches!(expression.ty, Type::Once(_)) && !value.loans.is_empty() {
                        return Err(error(DiagnosticKind::TaskCaptureRetainsBorrow,
                            "the captures of a tzroutine cannot retain borrowed values, including borrowed function environments",
                            expression.span,
                        ));
                    }
                    result.loans.extend(value.loans);
                }
            }
            E::RoutineGo(value) => {
                self.eval(value, Use::Consume, &during)?;
            }
            // A select (TZ04): the arms in order. A channel is lent until the wait, so a later
            // arm cannot consume it, and a send's value goes into the select (it is dropped there
            // when its arm is not chosen), so it holds no borrow, as the item of `Channel.send`
            // does. A false guard skips its arm's channel and value, which are then a branch, as
            // the right of `&&` is. What comes out holds none.
            E::SelectWait(wait) => {
                let start = self.held.len();
                // A place stays lent through one loan of it: an arm on a channel that an earlier
                // arm lends adds no loan, so a select of many arms on one channel stays linear.
                let mut lent = BTreeSet::new();
                for arm in &wait.arms {
                    if let Some(guard) = &arm.guard {
                        self.eval(guard, Use::Consume, &during)?;
                    }
                    let skipped = arm
                        .guard
                        .is_some()
                        .then(|| (self.state.clone(), self.reachable));
                    let mut channel = self.eval(&arm.channel, Use::Consume, &during)?;
                    channel.loans.retain(|id| {
                        let loan = &self.loans[*id];
                        lent.insert((loan.place.clone(), loan.mutable))
                    });
                    if !channel.loans.is_empty() {
                        self.held.push(channel);
                    }
                    if let Some(value) = &arm.send {
                        let sent = self.eval(value, Use::Consume, &during)?;
                        if arm.item.carries_loans(&self.module.types()) && !sent.loans.is_empty() {
                            return Err(error(
                                DiagnosticKind::ChannelItemHoldsBorrow,
                                "a channel item cannot hold a borrow, including a borrowed function environment: it is received later, perhaps after the borrowed data is gone; send an owned value, or a function that captures a copy, or an Owned.Function built where it is sent",
                                value.span,
                            ));
                        }
                    }
                    if let Some((before, before_reachable)) = skipped {
                        self.merge_reachable(&before, before_reachable);
                    }
                }
                self.held.truncate(start);
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
                let scope = operation.is_scope();
                let input = if *operation == crate::check::Builtin::ParallelInit || scope {
                    None
                } else {
                    Some(arguments.len() - 1)
                };
                // The callbacks of `reduce` and `Routine.scope` take two arguments.
                let result_arity =
                    usize::from(*operation == crate::check::Builtin::ParallelReduce || scope);
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
                            .expect("Routine.scope shares a borrow");
                        if referent.carries_loans(&self.module.types())
                            && self.shares_borrowed_environment(argument, &value)
                        {
                            return Err(error(DiagnosticKind::TaskScopeSharesBorrowedEnvironment,
                                "Routine.scope can share only values with proven owned environments",
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
                            return Err(error(DiagnosticKind::ParallelInputBorrowsEnvironment,
                                "parallel input elements must have proven owned environments",
                                argument.span,
                            ));
                        }
                    } else if !value.loans.is_empty() {
                        return Err(error(DiagnosticKind::ParallelCallbackBorrowsEnvironment,
                            "parallel callbacks and values cannot retain borrowed environments",
                            argument.span,
                        ));
                    }
                    if index == callback
                        && expression.ty.carries_loans(&self.module.types())
                        && !value.closed_result[result_arity]
                    {
                        return Err(error(DiagnosticKind::ParallelResultRetainsBorrow,
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
                let mark = self.events.as_ref().map_or(0, |events| events.len());
                let value = self.eval(left, usage, &during)?;
                if *operator == BinaryOp::Pipe {
                    // `text |> consume` gives its argument to `consume`, as `consume text` does.
                    self.event_targets(mark, left.span, right);
                }
                // `g |> Owned.function` is `Owned.function g`: its argument is checked here as in
                // `eval_call` (A30), and the builtin is not evaluated as a value.
                let owned_function = *operator == BinaryOp::Pipe
                    && self.builtin_called(right) == Some(crate::check::Builtin::OwnedFunction);
                if owned_function
                    && !value.loans.is_empty()
                    && !self.is_generated_parameter(left)
                {
                    return Err(self.owned_function_holds_borrow(left, &value, &expression.ty));
                }
                self.held.push(value.clone());
                // A28: `x |> f a` completes `f a x`, so the facts of `f` select its arguments'
                // loans as in `eval_call`.
                let completed = (*operator == BinaryOp::Pipe)
                    .then(|| self.piped_callee(expression, right))
                    .flatten();
                let callee = if owned_function {
                    Value::default()
                } else if let Some(id) = completed {
                    self.eval_piped_arguments(id, right, &during)?
                } else {
                    self.eval(right, usage, &during)?
                };
                self.held.pop();
                if *operator == BinaryOp::Pipe && expression.ty.carries_loans(&self.module.types())
                {
                    result = value;
                    // `x |> Owned.call (ref f)` is `Owned.call (ref f) x`, whose result does not
                    // borrow `f` (see `eval_call`).
                    let owned_call = matches!(&right.kind, E::Call(inner, arguments)
                        if arguments.len() == 1
                            && self.builtin_called(inner) == Some(crate::check::Builtin::OwnedCall));
                    if let Some(id) = completed {
                        let piped = match &right.kind {
                            E::Call(_, arguments) => arguments.len(),
                            _ => 0,
                        };
                        if self.facts.closed[id] {
                            result.loans.clear();
                        } else if self.facts.temporary(id, piped) {
                            result.loans = callee.loans;
                        } else {
                            result.loans.extend(callee.loans);
                        }
                    } else if self.reads_through(right) && !self.body_lends_exclusive() {
                        // `ref shared |> Arc.share` is `Arc.share (ref shared)`.
                        if !matches!(left.kind, E::BorrowOperand(_)) {
                            result.loans = self.read_through(&result.loans);
                        }
                        result.loans.extend(callee.loans);
                    } else if !owned_call {
                        result.loans.extend(callee.loans);
                    }
                    result.regions = None;
                }
            }
            E::Record(_) | E::RecordUpdate { .. } => {
                if matches!(expression.kind, E::RecordUpdate { .. })
                    && expression.ty.has_user_drop(&self.module.types())
                {
                    return Err(error(DiagnosticKind::UpdateDropTypeValue,
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
                    return Err(error(DiagnosticKind::MoveOutOfCollectionElement,
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
                        return Err(error(DiagnosticKind::MoveOutOfDropType, MOVE_OUT_OF_DROP, expression.span));
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
                E::Match { local, value, arms } => {
                    let (v_entry, v_exit) = child(&mut flow, value);
                    flow.edge(entry, v_entry);
                    items.push((&**value, v_entry, v_exit, targets));
                    // The subject is stored in the match's local.
                    let subject = flow.node(value.span);
                    flow.edge(v_exit, subject);
                    if tracked.contains(&local.id) {
                        flow.stores[subject] = Some((local.id, copied(value, tracked)));
                    }
                    // Where control goes when the previous arm does not match.
                    let mut failing = vec![subject];
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
                            // A test may fail; a binding stores its projection in its local.
                            let parts = alternative
                                .steps
                                .iter()
                                .map(|step| match step {
                                    crate::check::PatternStep::Test(test) => (test, true, None),
                                    crate::check::PatternStep::Bind(local, value) => {
                                        (value, false, Some(local.id))
                                    }
                                })
                                .chain(
                                    alternative
                                        .bindings
                                        .iter()
                                        .map(|(local, value)| (value, false, Some(local.id))),
                                );
                            for (part, fallible, bound) in parts {
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
                                if let Some(bound) = bound.filter(|bound| tracked.contains(bound)) {
                                    let store = flow.node(part.span);
                                    flow.edge(at, store);
                                    flow.stores[store] = Some((bound, copied(part, tracked)));
                                    at = store;
                                }
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
        // `locals` is sorted.
        let slot = |local: usize| locals.binary_search(&local).ok().map(|i| i + 1);
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
                        let (target, source) = (slot(stored), from.and_then(slot));
                        if at == 0 && contains(self.spans[next]) {
                            (target, None)
                        } else if target == Some(at) {
                            // `x = x` keeps what `x` holds; any other store replaces it.
                            (source.filter(|source| *source == at), None)
                        } else {
                            let handed = source.filter(|source| *source == at);
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

/// The `tracked` local whose loan a value carries on (`source_local`).
fn copied(value: &TypedExpr, tracked: &BTreeSet<usize>) -> Option<usize> {
    source_local(value).filter(|id| tracked.contains(id))
}

/// The local whose loan a value carries on: the local itself, or a borrow of a place in it (a
/// reborrow such as `ref (deref r)`, whose loan has the local's loan as its parent). A value read
/// out of a place (`deref r`) is a copy that carries no loan.
fn source_local(value: &TypedExpr) -> Option<usize> {
    let mut place = match &value.kind {
        E::Local(id) => return Some(*id),
        E::Borrow(place, _) | E::BorrowOperand(place) => place,
        _ => return None,
    };
    loop {
        match &place.kind {
            E::Local(id) => return Some(*id),
            E::Dereference(inner) | E::Field(inner, _) | E::Index(inner, _) | E::Cast(inner) => {
                place = inner;
            }
            _ => return None,
        }
    }
}

/// The locals that may hold the loan of `holders`: the holders, and the locals that one of them
/// was assigned or bound from (`holder = other`, `let holder = other`, a `match` subject or a
/// pattern variable bound from a local, or a reborrow of a place in a local), transitively.
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
                if let E::Local(to) = place.kind
                    && let Some(from) = source_local(value)
                {
                    copies.push((to, from));
                }
            }
            E::Block { bindings, .. } => {
                for (local, value) in bindings {
                    if let Some(from) = source_local(value) {
                        copies.push((local.id, from));
                    }
                }
            }
            // The subject of a `match` is copied into its local, and a pattern binds projections.
            E::Match { local, value, arms } => {
                if let Some(from) = source_local(value) {
                    copies.push((local.id, from));
                }
                for alternative in arms.iter().flat_map(|arm| &arm.alternatives) {
                    let steps = alternative.steps.iter().filter_map(|step| match step {
                        crate::check::PatternStep::Bind(local, value) => Some((local, value)),
                        crate::check::PatternStep::Test(_) => None,
                    });
                    let bindings = alternative
                        .bindings
                        .iter()
                        .map(|(local, value)| (local, value));
                    for (local, value) in steps.chain(bindings) {
                        if let Some(from) = source_local(value) {
                            copies.push((local.id, from));
                        }
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

#[cfg(test)]
mod event_tests {
    use super::*;

    /// The event sink (G26 phase 5) only records: every body of every shipped project checks
    /// with the same result, diagnostics and constraints with and without it.
    #[test]
    fn ownership_sink_does_not_change_diagnostics() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut projects = Vec::new();
        for directory in ["examples", "tests/fixtures"] {
            for entry in std::fs::read_dir(root.join(directory)).unwrap() {
                let path = entry.unwrap().path();
                if path.join("Main.tz").is_file() || path.join("Tsuzuri.toml").is_file() {
                    projects.push(path);
                }
            }
        }
        projects.sort();
        let mut checked = 0;
        for path in projects {
            let Ok(project) = crate::driver::Project::load(&path) else {
                continue;
            };
            let Ok(module) = project.analyze_all() else {
                continue;
            };
            let facts = [false, true].map(|infer| call_facts(&module, infer));
            for (id, function) in module.functions.iter().enumerate() {
                for infer in [false, true] {
                    let facts = &facts[usize::from(infer)];
                    let run = |events: Option<&mut Vec<OwnershipEvent>>| {
                        let mut recovered = Vec::new();
                        let result = check_body(
                            &module,
                            function.parameters.iter(),
                            &function.body,
                            infer,
                            facts,
                            function.is_once,
                            module.user_drops.values().any(|drop| *drop == id),
                            Contract::of(function),
                            &mut recovered,
                            None,
                            events,
                        );
                        (
                            result.map_err(|error| (error.code, error.span, error.message.clone())),
                            recovered
                                .into_iter()
                                .map(|error| (error.code, error.span, error.message.clone()))
                                .collect::<Vec<_>>(),
                        )
                    };
                    let mut events = Vec::new();
                    assert_eq!(
                        run(None),
                        run(Some(&mut events)),
                        "{} {}",
                        path.display(),
                        function.name
                    );
                }
            }
            checked += 1;
        }
        assert!(checked >= 20, "only {checked} projects were checked");
    }
}

#[cfg(test)]
mod retention_tests {
    use super::*;

    /// A28: the facts are the least fixed point of the checks of the bodies. A pair of mutually
    /// recursive functions that only pass `text` on keeps it temporary; a pair in which one
    /// captures it retains it in both.
    #[test]
    fn parameter_retention_is_the_least_fixed_point_of_the_bodies() {
        let module = crate::analyze(
            "def a28_pass :: ref i64 -> own (unit -> i64) -> (unit -> i64)\nfn a28_pass _key f = f\n\
             def a28_keep :: ref string -> (unit -> i64)\nfn a28_keep text = \\() -> String.length text\n\
             def rec a28_ping :: i64 -> ref string -> own (unit -> i64) -> (unit -> i64)\nfn rec a28_ping n text f = if n == 0 then f else a28_pong (n - 1) text f\n\
             def rec a28_pong :: i64 -> ref string -> own (unit -> i64) -> (unit -> i64)\nfn rec a28_pong n text f = a28_ping n text f\n\
             def rec a28_up :: i64 -> ref string -> (unit -> i64)\nfn rec a28_up n text = if n == 0 then \\() -> 0 else a28_down (n - 1) text\n\
             def rec a28_down :: i64 -> ref string -> (unit -> i64)\nfn rec a28_down n text = if n == 0 then \\() -> String.length text else a28_up n text\n\
             let one = 1\nlet text = \"abc\"\n\
             let a = a28_pass (ref one) (\\() -> 1)\nlet b = a28_keep (ref text)\n\
             let c = a28_ping 2 (ref text) (\\() -> 2)\nlet d = a28_up 2 (ref text)\n\
             a () + b () + c () + d ()",
        )
        .unwrap();
        for infer in [false, true] {
            let facts = call_facts(&module, infer);
            let retained = |name: &str| {
                let id = module
                    .functions
                    .iter()
                    .position(|function| function.name == name)
                    .unwrap_or_else(|| panic!("{name}"));
                facts.retained[id].as_deref().map(<[bool]>::to_vec)
            };
            assert_eq!(retained("a28_pass"), Some(vec![false, true]));
            // `a28_keep text = \() -> ...` takes its `()` too, so `a28_keep (ref text)` is a partial
            // application, which keeps every argument's loans without facts.
            assert_eq!(retained("a28_keep"), None);
            assert_eq!(retained("a28_ping"), Some(vec![false, false, true]));
            assert_eq!(retained("a28_pong"), Some(vec![false, false, true]));
            assert_eq!(retained("a28_up"), Some(vec![false, true]));
            assert_eq!(retained("a28_down"), Some(vec![false, true]));
        }
    }

    /// A28 review: a body that lends an exclusive borrow to a call has no facts, in the checks of
    /// the instances and in the inference: the check does not see what `Array.write` stores (A31).
    /// A body that only reads its array keeps them.
    #[test]
    fn a_body_that_lends_an_exclusive_borrow_has_no_facts() {
        let module = crate::analyze(
            "def a28_stash :: ref string -> own (unit -> i64) -> (unit -> i64)\nfn a28_stash text g =\n    let mut arr = [g]\n    Array.write 0 (\\() -> String.length text) (ref mut arr)\n    arr[0]\n\
             def a28_piped :: ref string -> own (unit -> i64) -> (unit -> i64)\nfn a28_piped text g =\n    let mut arr = [g]\n    ref mut arr |> Array.set 0 (\\() -> String.length text)\n    arr[0]\n\
             def a28_read :: ref string -> own (unit -> i64) -> (unit -> i64)\nfn a28_read _text g =\n    let arr = [g]\n    arr[0]\n\
             let text = \"abc\"\n\
             let a = a28_stash (ref text) (\\() -> 1)\nlet b = a28_piped (ref text) (\\() -> 2)\nlet c = a28_read (ref text) (\\() -> 3)\n\
             a () + b () + c ()",
        )
        .unwrap();
        for infer in [false, true] {
            let facts = call_facts(&module, infer);
            let retained = |name: &str| {
                let id = module
                    .functions
                    .iter()
                    .position(|function| function.name == name)
                    .unwrap_or_else(|| panic!("{name}"));
                facts.retained[id].as_deref().map(<[bool]>::to_vec)
            };
            assert_eq!(retained("a28_stash"), Some(vec![true, true]), "{infer}");
            assert_eq!(retained("a28_piped"), Some(vec![true, true]), "{infer}");
            assert_eq!(retained("a28_read"), Some(vec![false, true]), "{infer}");
        }
    }

    /// A28 review: when the checks run out, the functions still to check and their callers keep
    /// all their arguments' loans, and every other function keeps the facts of the fixed point.
    #[test]
    fn an_exhausted_budget_discards_only_the_unsettled_facts() {
        let module = crate::analyze(
            "def a28_pass :: ref i64 -> own (unit -> i64) -> (unit -> i64)\nfn a28_pass _key f = f\n\
             def rec a28_up :: i64 -> ref string -> (unit -> i64)\nfn rec a28_up n text = if n == 0 then \\() -> 0 else a28_down (n - 1) text\n\
             def rec a28_down :: i64 -> ref string -> (unit -> i64)\nfn rec a28_down n text = if n == 0 then \\() -> String.length text else a28_up n text\n\
             def a28_top :: ref string -> (unit -> i64)\nfn a28_top text = a28_up 1 text\n\
             let one = 1\nlet text = \"abc\"\n\
             let a = a28_pass (ref one) (\\() -> 1)\nlet d = a28_top (ref text)\n\
             a () + d ()",
        )
        .unwrap();
        let closed = closed_returns(&module);
        let full = parameter_retention(&module, &closed, false);
        let id = |name: &str| {
            module
                .functions
                .iter()
                .position(|function| function.name == name)
                .unwrap_or_else(|| panic!("{name}"))
        };
        let [pass, up, down, top] = ["a28_pass", "a28_up", "a28_down", "a28_top"].map(id);
        assert_eq!(full[pass].as_deref(), Some(&[false, true][..]));
        assert_eq!(full[top].as_deref(), Some(&[true][..]));
        assert!(
            parameter_retention_within(&module, &closed, false, Some(0))
                .iter()
                .all(Option::is_none)
        );
        let mut partial = false;
        for budget in 1..16 {
            let facts = parameter_retention_within(&module, &closed, false, Some(budget));
            for (function, fact) in facts.iter().enumerate() {
                assert!(
                    fact.is_none() || *fact == full[function],
                    "{budget} {function}"
                );
            }
            // The callers of an unsettled function are unsettled.
            if facts[up].is_none() || facts[down].is_none() {
                assert!(facts[top].is_none(), "{budget}");
            }
            partial |= facts[pass].is_some() && facts[up].is_none();
        }
        assert!(
            partial,
            "some budget settles `a28_pass` but not the recursive pair"
        );
    }
}
