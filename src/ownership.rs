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
        result.regions = value.regions.clone();
    } else if let (Some(slots), Some(other)) = (result.regions.as_mut(), value.regions.as_ref())
        && slots.len() == other.len()
    {
        for (slot, other) in slots.iter_mut().zip(other) {
            slot.extend(other);
        }
    } else if !value.loans.is_empty() {
        result.regions = None;
    }
    result.absorb(&value);
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
    /// A31: the loans of `loans` that the value holds only as what it read through a reference
    /// parameter (`read_places`): they stand for the borrows inside the parameter's target, not
    /// for the reference itself (`Part::Contents`). A loan is in it only when every way the value
    /// got the loan was such a read; a site that does not keep it leaves the loan out, which
    /// counts the loan as held itself (`Part::Value`), the conservative choice.
    through: BTreeSet<usize>,
}

impl Value {
    /// Adds `loans` as loans that the value holds itself.
    fn hold(&mut self, loans: impl IntoIterator<Item = usize>) {
        for loan in loans {
            self.loans.insert(loan);
            self.through.remove(&loan);
        }
    }

    /// Adds what `other` holds: a loan stays read through only when both values hold it so, or
    /// only one holds it and holds it so.
    fn absorb(&mut self, other: &Value) {
        // One loan of `other` at a time, so that adding a few loans to a value that holds many
        // costs as many steps as the few (`Checker::store_hidden` adds to every local).
        if !self.through.is_empty() {
            let loans = &self.loans;
            self.through.retain(|loan| loans.contains(loan));
        }
        for &loan in &other.loans {
            let through = other.through.contains(&loan);
            if self.loans.insert(loan) {
                if through {
                    self.through.insert(loan);
                }
            } else if !through {
                self.through.remove(&loan);
            }
        }
    }

    /// Drops every loan.
    fn clear_loans(&mut self) {
        self.loans.clear();
        self.through.clear();
    }
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
    /// A31: the loan stands for a share of the block that the `Rc` or `Arc` owner at its place
    /// owns: the result of `Rc.share (ref owner)` and its kin holds it in place of what the block
    /// holds when the check does not read through (A28). It is no pointer to the owner's storage.
    share: bool,
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
    for (id, function) in module.functions.iter().enumerate() {
        let _ = check_body(
            module,
            function.parameters.iter(),
            &function.body,
            true,
            &facts,
            function.is_once,
            false,
            facts.contract(module, id),
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
            facts.contract(module, id),
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
                facts.contract(module, id),
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
    /// The body is the generated function that applies a builtin to its own parameters
    /// (`src/closures.rs`, module `$builtin`), whose callers are checked instead.
    builtin_wrapper: bool,
    /// Whether the body, or one that it is lifted from or that is lifted from it, lends a way to
    /// write (an exclusive reference or a mutable cell) to other code (A28, `lending_bodies`);
    /// `None` asks the body alone when a read through needs it.
    lends_mutable: Option<bool>,
}

impl<'a> Contract<'a> {
    fn of(function: &'a crate::check::CheckedFunction) -> Self {
        Self {
            result: function.region_sources.as_ref(),
            callbacks: &function.callback_contracts,
            slots: function.region_slots.as_ref(),
            builtin_wrapper: function.module == "$builtin",
            lends_mutable: None,
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
    checker.drops_stop_reads = drops_stop_reads(
        module,
        parameters.clone(),
        body,
        infer,
        facts,
        task,
        user_drop,
        contract,
    );
    // The body's value goes to the caller, which applies the drop hook where it keeps it.
    checker.bound_next = true;
    let mut result = checker.eval(body, Use::Consume, &BTreeSet::new())?;
    if let Some(deferred) = checker.deferred.first() {
        return Err(deferred.clone());
    }
    checker.absorb_hidden(0, &mut result, &body.ty);
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
        builtin_wrapper: contract.builtin_wrapper,
        lends_mutable: contract.lends_mutable,
        dropped: None,
        drop_types: BTreeMap::new(),
        read_through_used: false,
        drops_stop_reads: false,
        stores: None,
        deferred: Vec::new(),
        parameter_roots: parameters
            .clone()
            .enumerate()
            .map(|(index, parameter)| (usize::MAX - parameter.id, index))
            .collect(),
        cell_types: BTreeMap::new(),
        hidden_types: BTreeMap::new(),
        shared_targets: BTreeMap::new(),
        argument_types: BTreeMap::new(),
        known: BTreeMap::new(),
        known_parts: BTreeMap::new(),
        store_memo: None,
        store_changes: 0,
        hidden_log: Vec::new(),
        drop_site: None,
        bound_next: false,
        tail_bound: false,
    };
    if !infer && facts.seeded {
        known_functions(module, body, &mut checker.known, &mut checker.known_parts);
    }
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
            facts.contract(module, id),
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

/// A31: which loans of an argument a store fact stores: those of the argument's value itself, or
/// those of the borrows inside the target of the reference that it is (`Value::through`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Part {
    Value,
    Contents,
}

/// A31: where an argument of a call of a function-valued parameter comes from: the parameters
/// (or the contents of their targets) whose loans it holds, whether it holds a loan of a local
/// value of the body, and whether it is a borrowed temporary (`Arg::temporary`, `Arg::dying`).
#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
struct Source {
    parameters: BTreeSet<(usize, Part)>,
    local: bool,
    temporary: bool,
    /// The argument is a reference to a temporary of the body (`Arg::dying`).
    dying: bool,
    /// The argument is a reference itself (`Arg::lent`).
    lent: bool,
    /// The argument's type may hold a function value that owns the cell that it writes, or
    /// reaches a cell (`Arg::hides`).
    hides: bool,
    /// The argument's type may be a place that a store reaches through it (`Arg::target`).
    target: bool,
}

/// A31: a call of the function-valued parameter `callee` of a body with `arguments`.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct HigherOrderCall {
    callee: usize,
    arguments: Vec<Source>,
}

/// A31: what a call of a function may store, from its body (`store_facts`): `(from, part, to)`
/// stores the loans of argument `from` (`Part`) where the target of argument `to` reaches; a call
/// in `calls` runs a function-valued argument, whose own facts the caller substitutes; `top` may
/// store any argument that may hold loans wherever any argument reaches (a body that does not
/// check, or one that the budget did not settle).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct StoreFacts {
    stores: BTreeSet<(usize, Part, usize)>,
    calls: BTreeSet<HigherOrderCall>,
    /// The loans of these arguments may be stored in an environment that the check cannot name
    /// (`Checker::store_hidden`): every value of the caller that may share one holds them.
    hidden: BTreeSet<(usize, Part)>,
    top: bool,
}

impl StoreFacts {
    fn top() -> Self {
        Self {
            top: true,
            ..Self::default()
        }
    }

    fn is_empty(&self) -> bool {
        !self.top && self.stores.is_empty() && self.calls.is_empty() && self.hidden.is_empty()
    }

    /// The union of `self` and `other`; whether it grew.
    fn join(&mut self, other: &StoreFacts) -> bool {
        if self.top {
            return false;
        }
        if other.top {
            *self = Self::top();
            return true;
        }
        let before = (self.stores.len(), self.calls.len(), self.hidden.len());
        self.stores.extend(other.stores.iter().copied());
        self.calls.extend(other.calls.iter().cloned());
        self.hidden.extend(other.hidden.iter().copied());
        before != (self.stores.len(), self.calls.len(), self.hidden.len())
    }
}

/// What the bodies of the functions prove about their direct calls: the results that hold no
/// borrow at all (`closed_returns`) and, for the others, the arguments whose loans a result can
/// hold (A28, `parameter_retention`).
struct CallFacts {
    closed: Vec<bool>,
    /// By function: whether its result can hold each argument's loans. `None` holds them all.
    retained: Vec<Option<Box<[bool]>>>,
    /// By function: whether its body, a body that encloses it (that of the function a lambda is
    /// lifted from) or the body of a lambda lifted from it, lends a way to write or hands a value
    /// to code that the analysis does not see (`lending_bodies`).
    lending: Lending,
    /// A31: by function, what a complete call of it may store (`store_facts`); empty in the
    /// abstract bodies of the inference, which keep A28's guard instead (`lending_bodies`).
    stores: Vec<StoreFacts>,
    /// A31: some `Drop.drop` of the module may store (`store_candidates`), so dropping a value
    /// whose type may run code when it is dropped stores what it holds where it reaches
    /// (`Checker::drop_stores`).
    drops_write: bool,
    /// A31: some function of the specialized module stores a value that may hold loans with a
    /// seed builtin (`StoreCandidates::seeded`); without one, no store escapes the checks and the
    /// store facts are not applied at all.
    seeded: bool,
    /// A31: by function, whether `joint_facts` revoked the closed result that `closed_returns`
    /// proved: its check returns a loan that a store into a local cell put there, so the same
    /// syntactic proof of a callback's result (`Checker::callback_returns_closed`) fails as well.
    revoked: Vec<bool>,
    /// A31: some function value of the module stores what it is given (or what its environment
    /// holds) where its environment reaches (`sink_flags`): a call of a function value that the
    /// check does not see may then store its arguments' loans where the value's loans reach. In a
    /// module without one, no function value does, so such a call stores only where its arguments
    /// reach.
    sinks: bool,
    /// A31: one of them owns the cell that it writes (a lambda that captured an
    /// `Arc<Mutex<...>>` by value, a dyn value that stores one): its copies share the cell
    /// without holding a loan, so a store into it reaches every value that may be a copy
    /// (`Checker::store_hidden`).
    owned_sinks: bool,
    /// A31: the types (`sink_type`) of the function values that may be such an owner, or may
    /// hold one in their environment (`sink_flags`): a call of a function value of any other type,
    /// with arguments that hide no environment, reaches none.
    owned_types: BTreeSet<Type>,
    /// A31: the types (`sink_type`) of the function values that may store into what their
    /// environment reaches, or hold such a value in their environment (`sink_flags`): a call of a
    /// function value of any other type stores nothing where the value's environment reaches, and
    /// a place that holds only function values of other types is no place to store into.
    sink_types: BTreeSet<Type>,
    /// A31: the module hands a value that may hold such a function value to code that runs it
    /// later, where no check follows it: the captures of a tzroutine, the item of a channel, an
    /// owned function that a builtin takes (`Routine.go`), the result of an `OnceValue`
    /// initializer, a host function with callbacks, the value of a dyn value without a region.
    /// Those are checked to hold no borrow where they are handed over (A30), which a later store
    /// into the cell that they share would undo, so no borrow may be stored in a hidden
    /// environment at all (`Checker::store_hidden`).
    escapes: bool,
    /// A31: some function of the module stores a shared reference that it gets as a parameter
    /// (the reference itself, `Part::Value`, not what its target holds), or stores anything
    /// (`top`): only then may code that the check does not see keep the reference that a read
    /// parameter or an operator lends (`Arg::reference`, `Arg::dying`).
    reference_sinks: bool,
    /// A31: the dyn types (`DynType::vtable_key`) that have vtables in the module, and those
    /// whose methods may store or give back a value through which a store may reach
    /// (`sink_flags`): a store through a borrow of a place that holds a dyn value reaches what the
    /// value stores only for those (`Checker::store_target`). A dyn type without vtables counts
    /// as one of them.
    dyn_types: BTreeSet<crate::check::DynType>,
    dyn_targets: BTreeSet<crate::check::DynType>,
    /// A31: by function, whether its body does not check (`joint_facts`): the program is
    /// rejected there, so a call of it stores nothing more for its callers to report.
    failed: Vec<bool>,
}

impl CallFacts {
    /// A31: whether a store may reach through a dyn value of type `dyn_type` (`dyn_targets`).
    fn dyn_target(&self, dyn_type: &crate::check::DynType) -> bool {
        let key = dyn_type.vtable_key();
        !self.dyn_types.contains(&key) || self.dyn_targets.contains(&key)
    }

    /// Whether a complete direct call of `function` passes none of the loans of its argument
    /// `index` to its result.
    fn temporary(&self, function: usize, index: usize) -> bool {
        self.retained
            .get(function)
            .and_then(Option::as_deref)
            .is_some_and(|retained| retained.get(index) == Some(&false))
    }

    /// The contract of the body of function `id`, with whether it or a related body lends a way
    /// to write or hands a value that may hold loans to code that the analysis does not see.
    fn contract<'a>(&self, module: &'a CheckedModule, id: usize) -> Contract<'a> {
        Contract {
            lends_mutable: Some(self.lending.no_reads(id)),
            ..Contract::of(&module.functions[id])
        }
    }
}

fn call_facts(module: &CheckedModule, infer: bool) -> CallFacts {
    let closed = closed_returns(module);
    let lending = lending_bodies(module, infer);
    let joint = joint_facts(module, &closed, &lending, infer, None);
    CallFacts {
        revoked: closed
            .iter()
            .zip(&joint.closed)
            .map(|(proved, kept)| *proved && !*kept)
            .collect(),
        closed: joint.closed,
        retained: joint.retained,
        lending,
        stores: joint.stores,
        drops_write: joint.drops_write,
        seeded: joint.seeded,
        sinks: joint.sinks,
        owned_sinks: joint.owned_sinks,
        owned_types: joint.owned_types,
        sink_types: joint.sink_types,
        escapes: joint.escapes,
        reference_sinks: joint.reference_sinks,
        dyn_types: joint.dyn_types,
        dyn_targets: joint.dyn_targets,
        failed: joint.failed,
    }
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

/// The callee and the arguments that the right side of a pipeline, `x |> right`, applies before
/// the piped one: those of `f a` in `x |> f a`, and none for a bare `f` in `x |> f`.
fn piped_application(right: &TypedExpr) -> (&TypedExpr, &[TypedExpr]) {
    match &right.kind {
        E::Call(callee, arguments) => (callee, arguments),
        _ => (right, &[]),
    }
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
/// see, and one whose borrows can only be in owned functions (`Owned.Function`, or a record,
/// tuple, container or union of them: `carries_loans_beside_owned_functions` is false), which
/// `start_checker` gives no caller loan as A30 has it. The abstract bodies give
/// facts only to the inference of the Copy constraints; the specialized bodies, whose types are
/// concrete, give the facts of the check that accepts or rejects the program.
/// Functions whose results named regions already decide (A12, A13), the generated functions that
/// apply builtins, `Drop.drop` bodies and tzroutine bodies keep all their arguments' loans.
#[cfg(test)]
fn parameter_retention(
    module: &CheckedModule,
    closed: &[bool],
    lending: &Lending,
    infer: bool,
) -> Vec<Option<Box<[bool]>>> {
    parameter_retention_within(module, closed, lending, infer, None)
}

/// `parameter_retention` with `budget` checks of bodies in place of the default.
#[cfg(test)]
fn parameter_retention_within(
    module: &CheckedModule,
    closed: &[bool],
    lending: &Lending,
    infer: bool,
    budget: Option<usize>,
) -> Vec<Option<Box<[bool]>>> {
    joint_facts(module, closed, lending, infer, budget).retained
}

/// What `joint_facts` finds: the closed results that survive the store facts, A28's retention
/// facts and A31's store facts.
struct JointFacts {
    closed: Vec<bool>,
    retained: Vec<Option<Box<[bool]>>>,
    stores: Vec<StoreFacts>,
    drops_write: bool,
    seeded: bool,
    sinks: bool,
    owned_sinks: bool,
    owned_types: BTreeSet<Type>,
    sink_types: BTreeSet<Type>,
    escapes: bool,
    reference_sinks: bool,
    dyn_types: BTreeSet<crate::check::DynType>,
    dyn_targets: BTreeSet<crate::check::DynType>,
    failed: Vec<bool>,
}

/// A31: the builtins that store a value that they get where an exclusive reference that they get
/// reaches: (stored argument, target argument). `Array.write` (which `Array.set`, `Array.fill`,
/// `Array.blit` and the other std functions that write in place call), `SyncMap.__replace` (through
/// the `ref mut` that `RwMutex.with_write` lends) and `Vec.push_mut`, whose region slots add the
/// pushed value's loans to the vector's owner only at a direct call (`store_arguments`), not when a
/// lock or another function calls it.
///
/// Every builtin of `Builtin::ALL` whose scheme has a `ref mut` parameter, or that takes a shared
/// reference to a cell and a value, was decided (A28, 8th review; A31):
///
/// - Seeds: the three above.
/// - Store numbers only, so never a loan: `Simd.store` (a lane value: `SimdNumeric` holds only for
///   SIMD vectors of numbers), and `Atomic.store`, `Atomic.swap`, `Atomic.compare_exchange` and the
///   `Atomic.fetch_*` operations (`AtomicValue` holds only for the integers and `bool`, and a
///   program cannot give a builtin class an instance).
/// - Tracked otherwise: `Channel.send`, `Channel.try_send`, `SendOnly.send`, `SendOnly.try_send`
///   and `DataChannel.send` (the item is checked to hold no borrow, `Checker::sent_item`),
///   `OnceValue.get` (the initializer's result is checked to hold no borrow,
///   `Checker::locked_result`), `WaitGroup.go` and `Context.__after_func` (an `Owned.Function`,
///   which holds no borrow, A30).
/// - Store nothing that they get: `Array.swap_in` (exchanges two elements of the same view),
///   `Array.split_at_mut` and `Vec.__view` (return views of what they get), `RwMutex.with_read`
///   (lends a shared reference), `Rc.ptr_eq` and `Arc.ptr_eq` (compare), and `Timer.__reset`,
///   `Ticker.__reset`,
///   `WaitGroup.add`, `Semaphore.acquire`, `Semaphore.try_acquire`, `Semaphore.release`,
///   `Context.__with_timeout` and `Context.__with_deadline` (take integers).
/// - Lend a `ref mut` to a callback and store nothing themselves: `Mutex.with_lock`,
///   `RwMutex.with_write`, `Cond.wait_with` and `Parallel.for_each_chunk`. What the callback
///   stores, it stores with an assignment (tracked), a seed or a call, and its body is a function
///   of the module: the facts of the call run it (`Checker::builtin_stores`).
/// - Run a function value that they get with an input that they get: `Owned.call`, which is a
///   call of the owned function with the input (`Checker::builtin_stores`).
///
/// The builtins that take an array, a vector or a map by value and return the updated one
/// (`Array.__set`, `Array.update`, `Array.swap`, `Vec.push`, ...) store nothing through a reference:
/// their result holds the loans of every argument.
fn store_seed(builtin: crate::check::Builtin) -> Option<(usize, usize)> {
    use crate::check::Builtin::{ArrayWrite, SyncMapReplace, VecPushMut};
    match builtin {
        ArrayWrite => Some((1, 2)),
        SyncMapReplace | VecPushMut => Some((0, 1)),
        _ => None,
    }
}

/// A31: which functions may store, from their bodies alone: `may_store` names a builtin that
/// stores a value that may hold loans (`Array.write`, `SyncMap.__replace`) or runs a callback,
/// calls a function value (or the result of a call given more arguments than the named function
/// has parameters), runs an initializer, a host function with callbacks, or the methods of dyn
/// values, or names a function that may store (the least fixed point over `mentions`). A body that
/// dispatches to a dyn method runs one of the slot functions of the vtables (`dispatch`), and
/// may store anything (`top`) when none takes as many parameters. When some `Drop.drop` may store,
/// a body that holds a value of a type that may run it does too. The other bodies store nothing
/// that may hold loans: their stores are assignments and region contracts, which every check
/// tracks.
///
/// Every store that the check of a body does not track goes through a seed (`store_seed`) whose
/// stored value may hold loans: assignments are tracked, a store through a reference that a body
/// gets needs named regions, which the check of that body enforces, and the code of a function
/// value, a dyn method, a callback or a `Drop.drop` is a body of the module. So a module with no
/// such seed (`seeded` false) has no store facts at all, and none of the rest is needed.
struct StoreCandidates {
    seeded: bool,
    may_store: Vec<bool>,
    top: Vec<bool>,
    mentions: Vec<BTreeSet<usize>>,
    /// The functions that dispatch a dyn method, with the slot functions of the vtables that
    /// take as many parameters: the dispatch runs one of them, so its facts are their join.
    dispatch: BTreeMap<usize, Vec<usize>>,
}

impl StoreCandidates {
    fn none(count: usize) -> Self {
        Self {
            seeded: false,
            may_store: vec![false; count],
            top: vec![false; count],
            mentions: vec![BTreeSet::new(); count],
            dispatch: BTreeMap::new(),
        }
    }
}

fn store_candidates(module: &CheckedModule, infer: bool) -> StoreCandidates {
    let types = module.types();
    let count = module.functions.len();
    let mut seeded = false;
    let mut may_store = vec![false; count];
    let mut top = vec![false; count];
    let mut mentions = vec![BTreeSet::new(); count];
    // `Owned.call` runs the owned function that it gets.
    let callback_builtin = |builtin: crate::check::Builtin| {
        builtin == crate::check::Builtin::OwnedCall
            || builtin
                .scheme()
                .parameters
                .iter()
                .any(|parameter| matches!(parameter, crate::check::BuiltinType::Function(..)))
    };
    let mut dispatch: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    // In the abstract bodies of the inference a type variable carries no loans: each instance of
    // a generic writer is a function of the specialized module, which has its own facts.
    let stores_loans = |expression: &TypedExpr, index: usize| match &expression.ty {
        Type::Function(parameters, _) => parameters.get(index).is_none_or(|stored| {
            stored.carries_loans(&types) || (!infer && stored.contains_unknown())
        }),
        _ => true,
    };
    // Whether a call of `callee` with `arguments` arguments runs code that is not a complete call
    // of a named function.
    let opaque = |callee: &TypedExpr, arguments: usize| match &callee.kind {
        E::Function(crate::check::FunctionRef::User(id)) | E::GenericFunction(id, _) => {
            arguments > module.functions[*id].parameters.len()
        }
        E::Function(crate::check::FunctionRef::Builtin(_)) | E::CaseConstructor { .. } => false,
        _ => true,
    };
    for (id, function) in module.functions.iter().enumerate() {
        let mut pending = vec![&function.body];
        while let Some(expression) = pending.pop() {
            match &expression.kind {
                E::Function(crate::check::FunctionRef::User(callee))
                | E::GenericFunction(callee, _)
                | E::Closure(callee, _) => {
                    mentions[id].insert(*callee);
                }
                E::Function(crate::check::FunctionRef::Builtin(instance)) => {
                    let seed = store_seed(instance.builtin)
                        .is_some_and(|(stored, _)| stores_loans(expression, stored));
                    seeded |= seed;
                    if seed || callback_builtin(instance.builtin) {
                        may_store[id] = true;
                    }
                }
                E::Call(callee, arguments) => may_store[id] |= opaque(callee, arguments.len()),
                E::Binary(BinaryOp::Pipe, _, right) => {
                    let (callee, arguments) = piped_application(right);
                    may_store[id] |= opaque(callee, arguments.len() + 1);
                }
                E::NewArray(..) | E::NewList(..) | E::Parallel(..) => may_store[id] = true,
                E::HostCall(import, _) => may_store[id] |= import.callbacks,
                E::Interpolated(interpolation) => {
                    may_store[id] |= interpolation
                        .holes
                        .iter()
                        .any(|hole| hole.method.as_ref().is_some_and(|method| opaque(method, 1)));
                }
                E::StructuralCompare(_, operands)
                | E::StructuralHash(operands)
                | E::StructuralDisplay(operands) => {
                    may_store[id] |= operands
                        .iter()
                        .any(|operand| types.reaches(&operand.ty, |ty| matches!(ty, Type::Dyn(_))));
                }
                E::DynDispatch { .. } => {
                    // The slot functions take the dispatch's parameters in the same order
                    // (`llvm::dyn_dispatch` passes them on), so their facts are the dispatch's.
                    let targets: BTreeSet<usize> = module
                        .vtables
                        .values()
                        .flatten()
                        .copied()
                        .filter(|slot| {
                            module.functions[*slot].parameters.len() == function.parameters.len()
                                && !matches!(
                                    module.functions[*slot].body.kind,
                                    E::DynDispatch { .. }
                                )
                        })
                        .collect();
                    if targets.is_empty() {
                        top[id] = true;
                    } else {
                        mentions[id].extend(&targets);
                        dispatch.insert(id, targets.into_iter().collect());
                    }
                }
                _ => {}
            }
            expression
                .try_for_each_child(&mut |child| {
                    pending.push(child);
                    Ok(())
                })
                .expect("collecting calls cannot fail");
        }
    }
    let mut mentioned_by = vec![Vec::new(); count];
    for (id, mentioned) in mentions.iter().enumerate() {
        for &callee in mentioned {
            mentioned_by[callee].push(id);
        }
    }
    if !seeded {
        return StoreCandidates::none(count);
    }
    let mut marks: Vec<bool> = (0..count).map(|id| may_store[id] || top[id]).collect();
    close_marks(&mut marks, &mentioned_by);
    // A value whose drop may store stores where it is dropped, with no call in the body: a body
    // that holds a value of a type that may run such a `Drop.drop` (`drop_runs_code`) may store
    // (`Checker::drop_stores`), and so may the bodies that name it.
    if module.user_drops.values().any(|drop| marks[*drop]) {
        let mut runs: BTreeMap<&Type, bool> = BTreeMap::new();
        for (id, function) in module.functions.iter().enumerate() {
            if marks[id] {
                continue;
            }
            let drops = |ty: &'_ Type| -> bool { drop_runs_code(ty, &types) };
            let mut found = function
                .parameters
                .iter()
                .any(|parameter| *runs.entry(&parameter.ty).or_insert_with_key(|ty| drops(ty)));
            let mut pending = vec![&function.body];
            while let Some(expression) = pending.pop() {
                if found {
                    break;
                }
                found = *runs
                    .entry(&expression.ty)
                    .or_insert_with_key(|ty| drops(ty));
                expression
                    .try_for_each_child(&mut |child| {
                        pending.push(child);
                        Ok(())
                    })
                    .expect("collecting types cannot fail");
            }
            marks[id] = found;
        }
        close_marks(&mut marks, &mentioned_by);
    }
    for (id, mark) in marks.into_iter().enumerate() {
        may_store[id] = mark && !top[id];
    }
    StoreCandidates {
        seeded,
        may_store,
        top,
        mentions,
        dispatch,
    }
}

/// A28 and A31: the facts of the calls, the least fixed point over the call graph of A28's
/// retention facts (`parameter_retention`), A31's store facts (`StoreFacts`) and the closed
/// results that survive them. One check of a body under the facts so far gives all three:
///
/// - the retention facts of a function that a complete call needs them for (`callers`), when the
///   body may have them (`Lending::no_facts`);
/// - the store facts of a function that may store (`store_candidates`): what the check stores
///   where a parameter reaches (`Checker::store_external`), and the calls of its function-valued
///   parameters (`Checker::apply_higher`); a body that does not check stores anything (`top`);
/// - whether a closed function (`closed_returns`) that may store and whose result may hold loans
///   still returns no loan: `closed` assumes that a local's loans do not grow after it is bound,
///   and a store into a cell that the local is (`let m = Mutex.create ...` written by a lock's
///   callback) breaks that. A check whose result holds a loan revokes the closed result, which then
///   goes through the retention facts like any other.
///
/// Every fact only grows (closed only goes false), so the iteration ends. A change of a function's
/// facts requeues the functions that name it. The store facts decide three flags of the module
/// (`sink_flags`: whether some function value stores into what its environment reaches, whether
/// one owns the cell that it writes, whether some function keeps a shared reference parameter),
/// which widen what a call of a function value that the check does not see may store; when one
/// rises, every body is checked again under it (they only rise). Past the budget, every function
/// still to check, and every function whose facts were computed from theirs, keeps all its
/// arguments' loans, stores anything and returns what it may (every function that may store,
/// when the flags were not yet the highest). The abstract bodies of the inference (`infer`) have
/// no store facts and no revocation: their type variables carry no loans, and each instance of a
/// generic body is a function of the specialized module, which has them.
fn joint_facts(
    module: &CheckedModule,
    closed: &[bool],
    lending: &Lending,
    infer: bool,
    budget: Option<usize>,
) -> JointFacts {
    let count = module.functions.len();
    let types = module.types();
    // The complete direct calls whose results may hold loans, and the pipelines that complete one
    // (`f |> pass (ref key)`, and `ref key |> make` into a unary function, which binds no argument
    // before the piped one). A closed callee needs no facts until a check revokes its result.
    let mut callers: BTreeMap<usize, BTreeSet<usize>> = BTreeMap::new();
    let mut callees: BTreeMap<usize, BTreeSet<usize>> = BTreeMap::new();
    for (caller, function) in module.functions.iter().enumerate() {
        let mut pending = vec![&function.body];
        while let Some(expression) = pending.pop() {
            let call = match &expression.kind {
                E::Call(callee, arguments) => Some((&**callee, arguments.len())),
                E::Binary(BinaryOp::Pipe, _, right) => {
                    let (callee, arguments) = piped_application(right);
                    Some((callee, arguments.len() + 1))
                }
                _ => None,
            };
            if let Some((callee, arguments)) = call
                && let Some(id) = direct_callee(callee)
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
    let candidates = if infer {
        StoreCandidates::none(count)
    } else {
        store_candidates(module, false)
    };
    let drops_write = candidates.seeded
        && module
            .user_drops
            .values()
            .any(|drop| candidates.may_store[*drop] || candidates.top[*drop]);
    let mut facts = CallFacts {
        closed: closed.to_vec(),
        retained: vec![None; count],
        lending: lending.clone(),
        drops_write,
        seeded: candidates.seeded,
        sinks: false,
        owned_sinks: false,
        owned_types: BTreeSet::new(),
        sink_types: BTreeSet::new(),
        escapes: false,
        reference_sinks: false,
        dyn_types: module.vtables.keys().map(|(key, _)| key.clone()).collect(),
        dyn_targets: BTreeSet::new(),
        failed: vec![false; count],
        revoked: vec![false; count],
        stores: (0..count)
            .map(|id| {
                if candidates.top[id] {
                    StoreFacts::top()
                } else {
                    StoreFacts::default()
                }
            })
            .collect(),
    };
    let initial = |id: usize| -> Box<[bool]> {
        module.functions[id]
            .parameters
            .iter()
            .map(|parameter| {
                // The parameters that may hold borrows but get no loan in the body: those that
                // `start_checker` gives no caller loan (borrows only in owned functions, as A30
                // has it) and type variables of an abstract body.
                (parameter.ty.carries_loans(&types) || parameter.ty.contains_unknown())
                    && !parameter.ty.carries_loans_beside_owned_functions(&types)
            })
            .collect()
    };
    let revocable =
        |id: usize| candidates.may_store[id] && !owned(&module.functions[id].body.ty, module);
    let mut retention: BTreeSet<usize> =
        callers.keys().copied().filter(|id| !closed[*id]).collect();
    for &id in &retention {
        facts.retained[id] = Some(initial(id));
    }
    // A store fact stores where a parameter reaches, or runs a function-valued parameter: a
    // function none of whose parameters a store may reach through (`may_reach_store`: an `i64`, a
    // `ref string`) has no store facts to compute, whatever its body stores into its own locals.
    let storing: Vec<bool> = (0..count)
        .map(|id| {
            candidates.may_store[id]
                && module.functions[id]
                    .parameters
                    .iter()
                    .any(|parameter| may_reach_store(&parameter.ty, &types))
        })
        .collect();
    let checked: BTreeSet<usize> = callers
        .keys()
        .copied()
        .chain((0..count).filter(|id| storing[*id] || (closed[*id] && revocable(*id))))
        .collect();
    // The functions whose checks read the facts of each function.
    let mut dependents: BTreeMap<usize, BTreeSet<usize>> = BTreeMap::new();
    for (&callee, users) in &callers {
        dependents
            .entry(callee)
            .or_default()
            .extend(users.iter().filter(|user| checked.contains(user)));
    }
    for &user in &checked {
        for &callee in &candidates.mentions[user] {
            dependents.entry(callee).or_default().insert(user);
        }
    }
    // Callees before their callers, so that a chain without recursion settles in one pass.
    let mut order = Vec::with_capacity(checked.len());
    let mut visited = BTreeSet::new();
    let children = |id: usize| -> Vec<usize> {
        callees
            .get(&id)
            .into_iter()
            .flatten()
            .chain(&candidates.mentions[id])
            .copied()
            .filter(|callee| checked.contains(callee))
            .collect::<BTreeSet<usize>>()
            .into_iter()
            .rev()
            .collect()
    };
    for &root in &checked {
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
    // The functions whose facts the sink flags read (`sink_flags`: the code of the function
    // values that the module makes, and the slot functions of the vtables), with the functions
    // whose facts theirs come from. They settle first, with the flags, before the other bodies
    // are checked under the flags that result: a large body that calls function values is then
    // checked once rather than once more for each rise of the flags.
    let mut first = if infer || !candidates.seeded {
        BTreeSet::new()
    } else {
        flag_sites(module)
    };
    let mut pending: Vec<usize> = first.iter().copied().collect();
    while let Some(id) = pending.pop() {
        for &callee in callees
            .get(&id)
            .into_iter()
            .flatten()
            .chain(&candidates.mentions[id])
        {
            if first.insert(callee) {
                pending.push(callee);
            }
        }
    }
    first.retain(|id| checked.contains(id));
    let mut staged = !first.is_empty() && first.len() < order.len();
    let mut queued: BTreeSet<usize> = order
        .iter()
        .copied()
        .filter(|id| !staged || first.contains(id))
        .collect();
    let mut work: std::collections::VecDeque<usize> = order
        .iter()
        .copied()
        .filter(|id| queued.contains(id))
        .collect();
    let mut budget =
        budget.unwrap_or(checked.len() * RETENTION_CHECKS_PER_FUNCTION + RETENTION_EXTRA_CHECKS);
    let mut exhausted = false;
    // The bodies that do not check (`CallFacts::failed`): the program is rejected, so what they
    // would store (`top`) tells the sink flags nothing, and no other body gets a diagnostic for it.
    // The sink flags (`sink_flags`) come from the store facts and widen what the checks store, so
    // the fixed point is taken again when they rise; they only rise.
    loop {
        while let Some(id) = work.pop_front() {
            if budget == 0 {
                exhausted = true;
                // The functions still to check, and every function whose facts were computed from
                // theirs, keep all their arguments' loans, store anything and return what they may.
                // The others were checked under the facts that their callees have now, so they stay a
                // fixed point of their part of the graph.
                let mut unsettled = std::mem::take(&mut queued);
                let mut pending: Vec<usize> = unsettled.iter().copied().collect();
                while let Some(callee) = pending.pop() {
                    for user in dependents.get(&callee).into_iter().flatten() {
                        if unsettled.insert(*user) {
                            pending.push(*user);
                        }
                    }
                }
                for id in unsettled {
                    facts.retained[id] = None;
                    if candidates.may_store[id] {
                        facts.stores[id] = StoreFacts::top();
                    }
                    if revocable(id) && facts.closed[id] {
                        facts.closed[id] = false;
                        facts.revoked[id] = true;
                    }
                }
                break;
            }
            queued.remove(&id);
            budget -= 1;
            let outcome = check_facts(
                module,
                id,
                infer,
                &facts,
                retention.contains(&id),
                storing[id],
                facts.closed[id] && revocable(id),
            );
            facts.failed[id] |= outcome.failed;
            let mut changed = false;
            if retention.contains(&id) {
                let old = facts.retained[id].as_deref().expect("a computed function");
                let new: Box<[bool]> = match &outcome.retained {
                    Some(found) => old
                        .iter()
                        .zip(found.iter())
                        .map(|(old, new)| *old || *new)
                        .collect(),
                    None => vec![true; old.len()].into_boxed_slice(),
                };
                if *old != *new {
                    facts.retained[id] = Some(new);
                    changed = true;
                }
            }
            if storing[id] {
                changed |= facts.stores[id].join(&outcome.stores);
                // A dyn method's dispatch runs one of the slot functions of the vtables.
                if let Some(targets) = candidates.dispatch.get(&id) {
                    for target in targets {
                        let found = facts.stores[*target].clone();
                        changed |= facts.stores[id].join(&found);
                    }
                }
            }
            if outcome.revoke {
                facts.closed[id] = false;
                facts.revoked[id] = true;
                changed = true;
                // The callers now need the retention facts that the closed result made needless.
                if callers.contains_key(&id) && retention.insert(id) {
                    facts.retained[id] = Some(initial(id));
                    if queued.insert(id) {
                        work.push_back(id);
                    }
                }
            }
            if changed {
                for user in dependents.get(&id).into_iter().flatten() {
                    if (!staged || first.contains(user)) && queued.insert(*user) {
                        work.push_back(*user);
                    }
                }
            }
        }
        if infer || !candidates.seeded {
            break;
        }
        if exhausted {
            // The checks that were made saw the flags of their time: when those were not the
            // highest, every function that may store stores anything.
            if !(facts.sinks && facts.owned_sinks && facts.reference_sinks && facts.escapes) {
                for id in 0..count {
                    if candidates.may_store[id] {
                        facts.stores[id] = StoreFacts::top();
                        facts.retained[id] = None;
                    }
                    if revocable(id) && facts.closed[id] {
                        facts.closed[id] = false;
                        facts.revoked[id] = true;
                    }
                }
                facts.sinks = true;
                facts.owned_sinks = true;
                facts.reference_sinks = true;
                facts.escapes = true;
            }
            // The facts that stand now (`top` for the unsettled functions) may make more
            // function values owners than the checks saw, and the calls that the settled
            // functions recorded judged their arguments by the types of their time: every
            // argument may now hold an owner or be a place to store into, and every dyn type
            // may be one.
            let found = sink_flags(module, &facts.stores, &facts.failed);
            facts.owned_types.extend(found.owned_types);
            facts.sink_types.extend(found.sink_types);
            for stores in &mut facts.stores {
                if !stores.calls.is_empty() {
                    stores.calls = std::mem::take(&mut stores.calls)
                        .into_iter()
                        .map(|mut call| {
                            for source in &mut call.arguments {
                                source.hides = true;
                                source.target = true;
                            }
                            call
                        })
                        .collect();
                }
            }
            facts.dyn_targets = facts.dyn_types.clone();
            break;
        }
        let found = sink_flags(module, &facts.stores, &facts.failed);
        let flags = (
            found.sinks || found.owned || facts.sinks,
            found.owned || facts.owned_sinks,
            found.references || facts.reference_sinks,
        );
        let mut types = std::mem::take(&mut facts.owned_types);
        let known = types.len();
        if flags.1 {
            types.extend(found.owned_types.iter().cloned());
        }
        let escapes = facts.escapes || (flags.1 && found.escapes(module, &types));
        let dyn_known = facts.dyn_targets.len();
        facts.dyn_targets.extend(found.dyn_targets);
        let sinks_known = facts.sink_types.len();
        if flags.0 {
            facts.sink_types.extend(found.sink_types);
        }
        let grew = types.len() != known
            || escapes != facts.escapes
            || facts.dyn_targets.len() != dyn_known
            || facts.sink_types.len() != sinks_known;
        facts.owned_types = types;
        facts.escapes = escapes;
        if flags == (facts.sinks, facts.owned_sinks, facts.reference_sinks) && !grew {
            if !staged {
                break;
            }
            // The functions that the flags read have settled: the others now get their checks.
            staged = false;
            for &id in &order {
                if !first.contains(&id) && queued.insert(id) {
                    work.push_back(id);
                }
            }
            continue;
        }
        (facts.sinks, facts.owned_sinks, facts.reference_sinks) = flags;
        for &id in &order {
            if (!staged || first.contains(&id)) && queued.insert(id) {
                work.push_back(id);
            }
        }
    }
    JointFacts {
        closed: facts.closed,
        retained: facts.retained,
        stores: facts.stores,
        drops_write,
        seeded: candidates.seeded,
        sinks: facts.sinks,
        owned_sinks: facts.owned_sinks,
        owned_types: facts.owned_types,
        sink_types: facts.sink_types,
        escapes: facts.escapes,
        reference_sinks: facts.reference_sinks,
        dyn_types: facts.dyn_types,
        dyn_targets: facts.dyn_targets,
        failed: facts.failed,
    }
}

/// A31: whether some function value of the module stores into what its environment reaches
/// (`CallFacts::sinks`), and whether one owns the cell that it writes (`CallFacts::owned_sinks`),
/// from the store facts `stores` of the functions and the places where the module makes a
/// function value with an environment:
///
/// - a lifted lambda with its captures and a partial application of a named function with the
///   arguments that it binds (a pipeline that leaves parameters open too): the value is a sink
///   when the facts of its code store into a bound parameter, pass a bound parameter to a call of
///   a function value (which may store there), store in a hidden environment, or are unknown
///   (`top`); it owns its cell when a bound value owns a mutable cell (`owns_mutable_cell`);
/// - a partial application of a builtin that runs a callback (`Mutex.with_lock (ref m)`), whose
///   callback stores where the bound lock reaches;
/// - a call of a function value that this scan does not see (or of the function that a named
///   function returns) whose result is a function: the result may be that value with the
///   arguments bound, so it is a sink when an argument can be stored into or holds a loan, and
///   owns its cell when an argument does;
/// - `Dyn.of value`, whose methods get the stored value through the receiver: it owns its cell
///   when the value owns a mutable cell or hides an environment itself.
///
/// A named function has no environment, and a function value that only calls a captured function
/// value with its own arguments stores where that value's code does, which is a site of its own.
/// So in a module where no site is a sink, a call of any function value stores only where its
/// arguments reach, by induction over the calls that it makes.
///
/// `references` (`CallFacts::reference_sinks`) says that some function stores a shared
/// reference parameter itself, or anything. `owned_types` (`CallFacts::owned_types`) are the types
/// of the function values that own their cell and of those that hold, in their environment, a
/// value that may hold one of them (to a fixed point); `sink_types` (`CallFacts::sink_types`) are
/// those of the sinks and of the function values that hold one in the same way. A bound value that
/// names a function holds nothing. The dyn types whose methods may store or give back a place to
/// store into are `dyn_targets` (`CallFacts::dyn_targets`).
fn sink_flags(module: &CheckedModule, stores: &[StoreFacts], failed: &[bool]) -> SinkFlags {
    let types = module.types();
    // A function keeps a shared reference parameter when it stores the parameter's own loan, or
    // hands a value that holds it (not the reference itself, which the function value gets as a
    // parameter of its own) to a function value, which may store that value.
    let references =
        module
            .functions
            .iter()
            .zip(stores)
            .enumerate()
            .any(|(id, (function, facts))| {
                let reference = |from: &usize| {
                    matches!(
                        function
                            .parameters
                            .get(*from)
                            .map(|parameter| &parameter.ty),
                        Some(Type::Reference(_, false))
                    )
                };
                (facts.top && !failed[id])
                    || facts
                        .stores
                        .iter()
                        .any(|(from, part, _)| *part == Part::Value && reference(from))
                    || facts
                        .hidden
                        .iter()
                        .any(|(from, part)| *part == Part::Value && reference(from))
                    || facts.calls.iter().any(|call| {
                        call.arguments.iter().any(|source| {
                            !source.lent
                                && source
                                    .parameters
                                    .iter()
                                    .any(|(from, part)| *part == Part::Value && reference(from))
                        })
                    })
            });
    let (mut sinks, mut owned) = (false, false);
    // The types of the function values that own the cell that they write, and of those that
    // may hold a function value in their environment, which may be such an owner.
    let mut owned_types = BTreeSet::new();
    // The types of the function values that store into what their environment reaches, and of
    // those that may hold such a value in their environment.
    let mut sink_types = BTreeSet::new();
    // The function values that hold values that hide an environment, with the types of those
    // values: such a function value reaches the owners and the sinks that those values reach.
    let mut holders: Vec<(Type, Vec<Type>)> = Vec::new();
    // The types of the values that the module hands to code that runs later (`SinkFlags::escapes`).
    let mut escaping: BTreeSet<Type> = BTreeSet::new();
    // The types of the function values that `Owned.function` wraps, and whether a builtin takes
    // an owned function to run it later.
    let mut wrapped: BTreeSet<Type> = BTreeSet::new();
    let mut takes_owned = false;
    // A bound value that names a function (`plus_one`, or the local that the adapter of a
    // function with other parameter conventions binds it to) has no environment.
    let named = |value: &TypedExpr, locals: &BTreeSet<usize>| match &value.kind {
        E::Function(_) | E::GenericFunction(..) => true,
        E::Local(id) => locals.contains(id),
        _ => false,
    };
    let site = |id: usize,
                bound: &mut dyn Iterator<Item = &TypedExpr>,
                locals: &BTreeSet<usize>|
     -> (bool, bool, Vec<Type>) {
        let bound: Vec<&TypedExpr> = bound.collect();
        let facts = &stores[id];
        let count = bound.len();
        // A bound value that a call of a function value gets counts only when a store may reach
        // through it (`Source::target`): the function value stores what it gets where its own
        // arguments reach or where its environment does, and the latter is a site of its own.
        let sink = (facts.top && !failed[id])
            || !facts.hidden.is_empty()
            || facts.stores.iter().any(|(_, _, to)| *to < count)
            || facts.calls.iter().any(|call| {
                call.arguments.iter().any(|source| {
                    source.target
                        && source
                            .parameters
                            .iter()
                            .any(|(parameter, _)| *parameter < count)
                })
            });
        let owns = sink
            && (!facts.hidden.is_empty()
                || bound
                    .iter()
                    .any(|value| !named(value, locals) && value.ty.owns_mutable_cell(&types)));
        let held = bound
            .iter()
            .filter(|value| !named(value, locals))
            .map(|value| without_views(&value.ty))
            .filter(|ty| hides_environment(ty, &types))
            .collect();
        (sink, owns, held)
    };
    // A call of a function value that the scan does not see (or of the function that a named
    // function returns) whose result is a function: it may be that function value with the
    // arguments bound, which the rest of its parameters complete, so it is a sink when an argument
    // is a place that a store may reach.
    let staged = |bound: &mut dyn Iterator<Item = &TypedExpr>,
                  result: &Type|
     -> (bool, bool, Vec<Type>) {
        if !matches!(result, Type::Function(..)) {
            return (false, false, Vec::new());
        }
        let bound: Vec<Type> = bound.map(|value| without_views(&value.ty)).collect();
        let held: Vec<Type> = bound
            .iter()
            .filter(|ty| hides_environment(ty, &types))
            .cloned()
            .collect();
        let sink = !held.is_empty()
            || bound.iter().any(|ty| {
                ty.carries_loans(&types) || types.reaches(ty, |part| part.is_mutable_cell(&types))
            });
        let owns = sink && bound.iter().any(|ty| ty.owns_mutable_cell(&types));
        (sink, owns, held)
    };
    let mut found = |(sink, owns, held): (bool, bool, Vec<Type>), ty: &Type| {
        sinks |= sink;
        owned |= owns;
        if sink {
            sink_types.insert(sink_type(ty));
        }
        if owns {
            owned_types.insert(sink_type(ty));
        }
        if !held.is_empty() {
            holders.push((sink_type(ty), held));
        }
    };
    for function in &module.functions {
        let locals = named_locals(&function.body);
        let mut pending = vec![&function.body];
        while let Some(expression) = pending.pop() {
            match &expression.kind {
                E::Closure(id, captures) => {
                    found(site(*id, &mut captures.iter(), &locals), &expression.ty);
                    if matches!(expression.ty, Type::Once(_)) {
                        escaping.extend(captures.iter().map(|capture| without_views(&capture.ty)));
                    }
                }
                // A send keeps its item and `OnceValue.get` the result of its initializer: the
                // parameter types of the builtin name them. `Owned.function` wraps its argument
                // in a value that the builtins that take an owned function run later
                // (`Routine.go`, `WaitGroup.go`, `Time.after_func`).
                E::Function(_) => {
                    use crate::check::Builtin::{
                        ChannelSend, ChannelTrySend, DataChannelSend, OnceValueGet, OwnedCall,
                        OwnedFunction, SendOnlySend, SendOnlyTrySend,
                    };
                    let builtin = builtin_called(module, expression);
                    if let (Some(builtin), Type::Function(parameters, _)) =
                        (builtin, &expression.ty)
                    {
                        let kept = match builtin {
                            OnceValueGet => Some(0),
                            ChannelSend | ChannelTrySend | DataChannelSend | SendOnlySend
                            | SendOnlyTrySend => Some(1),
                            _ => None,
                        };
                        if let Some(ty) = kept.and_then(|kept| parameters.types.get(kept)) {
                            escaping.insert(without_views(ty));
                        }
                        if builtin == OwnedFunction {
                            wrapped.extend(parameters.types.first().map(without_views));
                        } else if builtin != OwnedCall {
                            takes_owned |= parameters.types.iter().any(|ty| {
                                types.reaches(&without_views(ty), |part| {
                                    part.is_owned_function(&types)
                                })
                            });
                        }
                    }
                }
                E::SelectWait(wait) => {
                    escaping.extend(wait.arms.iter().map(|arm| without_views(&arm.item)));
                }
                E::HostCall(import, arguments) if import.callbacks => {
                    escaping.extend(arguments.iter().map(|value| without_views(&value.ty)));
                }
                E::Call(callee, arguments) if !arguments.is_empty() => {
                    if let Some(id) = direct_callee(callee) {
                        let count = module.functions[id].parameters.len();
                        if arguments.len() < count {
                            found(site(id, &mut arguments.iter(), &locals), &expression.ty);
                        } else if arguments.len() > count {
                            // The function that the call returns binds the arguments beyond.
                            found(
                                staged(&mut arguments[count..].iter(), &expression.ty),
                                &expression.ty,
                            );
                        }
                    } else if !matches!(
                        callee.kind,
                        E::Function(crate::check::FunctionRef::Builtin(_))
                            | E::CaseConstructor { .. }
                    ) {
                        found(
                            staged(&mut arguments.iter(), &expression.ty),
                            &expression.ty,
                        );
                    } else if let E::Function(crate::check::FunctionRef::Builtin(instance)) =
                        &callee.kind
                    {
                        // The builtin runs the callback with what its other arguments reach (the
                        // contents that a lock lends): the partial application stores into what
                        // it binds only when a store may reach through a bound value.
                        let scheme = instance.builtin.scheme();
                        if arguments.len() < scheme.parameters.len()
                            && scheme.parameters.iter().any(|parameter| {
                                matches!(parameter, crate::check::BuiltinType::Function(..))
                            })
                            && arguments
                                .iter()
                                .any(|value| may_reach_store(&value.ty, &types))
                        {
                            found((true, false, Vec::new()), &expression.ty);
                        }
                        // A dyn value without a region holds no borrow where it is made
                        // (A30), and its type says so wherever it goes afterwards.
                        if owned_dyn_of(callee) {
                            escaping.extend(arguments.iter().map(|value| without_views(&value.ty)));
                        }
                        if instance.builtin == crate::check::Builtin::DynOf
                            && arguments.iter().any(|value| {
                                value.ty.owns_mutable_cell(&types)
                                    || hides_environment(&value.ty, &types)
                            })
                        {
                            found((true, true, Vec::new()), &expression.ty);
                        }
                    }
                }
                E::Binary(BinaryOp::Pipe, left, right) => {
                    let (callee, arguments) = piped_application(right);
                    let mut all = arguments.iter().chain([&**left]);
                    match direct_callee(callee) {
                        Some(id) => {
                            let count = module.functions[id].parameters.len();
                            if arguments.len() + 1 < count {
                                found(site(id, &mut all, &locals), &expression.ty);
                            } else if arguments.len() + 1 > count {
                                found(staged(&mut all, &expression.ty), &expression.ty);
                            }
                        }
                        None if !matches!(
                            callee.kind,
                            E::Function(crate::check::FunctionRef::Builtin(_))
                                | E::CaseConstructor { .. }
                        ) =>
                        {
                            found(staged(&mut all, &expression.ty), &expression.ty);
                        }
                        None => {}
                    }
                }
                _ => {}
            }
            expression
                .try_for_each_child(&mut |child| {
                    pending.push(child);
                    Ok(())
                })
                .expect("collecting function values cannot fail");
        }
    }
    // A function value that holds another in its environment reaches whatever that one does:
    // its type joins those of the owners (and of the sinks) when a value that it holds may hold
    // one of them, until no more do. A closure that captured a `ref` to a record whose cell holds
    // functions of no such type reaches none.
    let close = |set: &mut BTreeSet<Type>| loop {
        let dynamic = set.iter().any(|ty| matches!(ty, Type::Dyn(_)));
        let mut grew = false;
        for (ty, held) in &holders {
            if set.contains(ty) {
                continue;
            }
            let reaches = held.iter().any(|value| {
                types.reaches(value, |part| match part {
                    Type::Function(..) => owned_suffix(part, usize::MAX, set),
                    Type::Dyn(_) => dynamic,
                    _ => part.is_owned_function(&types),
                })
            });
            if reaches {
                set.insert(ty.clone());
                grew = true;
            }
        }
        if !grew {
            break;
        }
    };
    if sinks {
        close(&mut sink_types);
    }
    if owned {
        close(&mut owned_types);
    }
    // Code that gets a dyn value reaches the value that it stores only through the methods of
    // the vtables: a store reaches through it when one of them may store (its facts), or gives
    // back a value through which a store may reach (a reference to a cell inside the value).
    let mut dyn_targets = BTreeSet::new();
    for ((key, _), slots) in &module.vtables {
        if slots.iter().any(|&slot| {
            stores.get(slot).is_none_or(|facts| {
                (facts.top && !failed[slot])
                    || !facts.stores.is_empty()
                    || !facts.calls.is_empty()
                    || !facts.hidden.is_empty()
            }) || module
                .functions
                .get(slot)
                .is_none_or(|function| may_reach_store(&function.body.ty, &types))
        }) {
            dyn_targets.insert(key.clone());
        }
    }
    SinkFlags {
        sinks,
        owned,
        owned_types,
        sink_types,
        references,
        escaping,
        wrapped,
        takes_owned,
        dyn_targets,
    }
}

/// A31: the functions whose store facts `sink_flags` reads: the lifted lambdas that the module
/// makes into function values, the named functions that it applies partially (in a call or a
/// pipeline), and the slot functions of the vtables.
fn flag_sites(module: &CheckedModule) -> BTreeSet<usize> {
    let mut sites: BTreeSet<usize> = module.vtables.values().flatten().copied().collect();
    for function in &module.functions {
        let mut pending = vec![&function.body];
        while let Some(expression) = pending.pop() {
            match &expression.kind {
                E::Closure(id, _) => {
                    sites.insert(*id);
                }
                E::Call(callee, arguments) => {
                    if let Some(id) = direct_callee(callee)
                        && arguments.len() < module.functions[id].parameters.len()
                    {
                        sites.insert(id);
                    }
                }
                E::Binary(BinaryOp::Pipe, _, right) => {
                    let (callee, arguments) = piped_application(right);
                    if let Some(id) = direct_callee(callee)
                        && arguments.len() + 1 < module.functions[id].parameters.len()
                    {
                        sites.insert(id);
                    }
                }
                _ => {}
            }
            expression
                .try_for_each_child(&mut |child| {
                    pending.push(child);
                    Ok(())
                })
                .expect("collecting function values cannot fail");
        }
    }
    sites
}

/// A31: the immutable locals of `body` bound to a named function, which has no environment.
fn named_locals(body: &TypedExpr) -> BTreeSet<usize> {
    let mut locals = BTreeSet::new();
    let mut pending = vec![body];
    while let Some(expression) = pending.pop() {
        if let E::Block { bindings, .. } = &expression.kind {
            for (local, value) in bindings {
                if !local.mutable && matches!(value.kind, E::Function(_) | E::GenericFunction(..)) {
                    locals.insert(local.id);
                }
            }
        }
        expression
            .try_for_each_child(&mut |child| {
                pending.push(child);
                Ok(())
            })
            .expect("collecting bindings cannot fail");
    }
    locals
}

/// What `sink_flags` finds.
struct SinkFlags {
    sinks: bool,
    owned: bool,
    owned_types: BTreeSet<Type>,
    /// The types (`sink_type`) of the function values that store into what their environment
    /// reaches, and of those that may hold one in their environment (`CallFacts::sink_types`).
    sink_types: BTreeSet<Type>,
    references: bool,
    /// The types of the values that the module hands to code that runs later.
    escaping: BTreeSet<Type>,
    /// The types of the function values that the module wraps with `Owned.function`.
    wrapped: BTreeSet<Type>,
    /// Some builtin of the module takes an owned function, to run it later.
    takes_owned: bool,
    /// The dyn types (`DynType::vtable_key`) whose methods may store, or give back a value
    /// through which a store may reach (`CallFacts::dyn_targets`).
    dyn_targets: BTreeSet<crate::check::DynType>,
}

impl SinkFlags {
    /// Whether a value that the module hands to code that runs later may hold a function value
    /// of one of the types `owned` (`CallFacts::owned_types`): directly, in a dyn value (when
    /// some dyn type is among them), or in an `Owned.Function` (when `Owned.function` wraps such
    /// a value somewhere), whose types do not show what they store. An owned function that wraps
    /// such a value is itself handed over when a builtin takes one to run it later.
    fn escapes(&self, module: &CheckedModule, owned: &BTreeSet<Type>) -> bool {
        let types = module.types();
        let dynamic = owned.iter().any(|ty| matches!(ty, Type::Dyn(_)));
        let holds = |ty: &Type, wrapped: bool| {
            types.reaches(ty, |part| match part {
                Type::Function(..) => owned.contains(&sink_type(part)),
                Type::Dyn(_) => dynamic,
                _ => wrapped && part.is_owned_function(&types),
            })
        };
        let wrapped = self.wrapped.iter().any(|ty| holds(ty, false));
        (wrapped && self.takes_owned) || self.escaping.iter().any(|ty| holds(ty, wrapped))
    }
}

/// A31: the type `ty` of a function value as `CallFacts::owned_types` compares it: the purity,
/// which a cast changes, is left out, and so are the parameter conventions and the references
/// around a parameter's type (a lambda's read parameter of edition 2027 is `ref (unit -> i64)`
/// where the declared type of a value that holds the lambda says `unit -> i64`); a curried chain
/// is one list of parameters. Two types that a value may have both compare equal, which is what
/// a test of membership needs; a few more compare equal too, which only widens it.
fn sink_type(ty: &Type) -> Type {
    match ty {
        Type::Function(parameters, result) => {
            let mut types: Vec<Type> = parameters
                .types
                .iter()
                .map(|parameter| {
                    let mut parameter = parameter;
                    while let Type::Reference(target, _) = parameter {
                        parameter = target;
                    }
                    sink_type(parameter)
                })
                .collect();
            let mut result = sink_type(result);
            while let Type::Function(more, last) = result {
                types.extend(more.types);
                result = *last;
            }
            Type::Function(
                Box::new(crate::check::FunctionParameters {
                    types,
                    conventions: Box::default(),
                    pure: false,
                }),
                Box::new(result),
            )
        }
        Type::Reference(target, mutable) => Type::Reference(Box::new(sink_type(target)), *mutable),
        Type::Array(element) | Type::ArrayView(element) => {
            Type::Array(Box::new(sink_type(element)))
        }
        Type::List(element) => Type::List(Box::new(sink_type(element))),
        Type::Vec(element) => Type::Vec(Box::new(sink_type(element))),
        Type::Shared(element, kind) => Type::Shared(Box::new(sink_type(element)), *kind),
        Type::Tuple(elements) => Type::Tuple(elements.iter().map(sink_type).collect()),
        Type::Record(id, arguments) => Type::Record(*id, arguments.iter().map(sink_type).collect()),
        Type::Union(id, arguments) => Type::Union(*id, arguments.iter().map(sink_type).collect()),
        _ => ty.clone(),
    }
}

/// A31: whether the function type `ty`, or the type of the function that a value of it gives after
/// at most `arguments` arguments, is one of `owned` (`CallFacts::owned_types`, compared as
/// `sink_type` has it).
fn owned_suffix(ty: &Type, arguments: usize, owned: &BTreeSet<Type>) -> bool {
    let Type::Function(parameters, result) = sink_type(ty) else {
        return owned.contains(&sink_type(ty));
    };
    (0..=arguments.min(parameters.types.len())).any(|applied| {
        let rest = if applied == parameters.types.len() {
            (*result).clone()
        } else {
            Type::Function(
                Box::new(crate::check::FunctionParameters {
                    types: parameters.types[applied..].to_vec(),
                    conventions: Box::default(),
                    pure: false,
                }),
                result.clone(),
            )
        };
        owned.contains(&rest)
    })
}

/// What one check of a body gives `joint_facts`.
struct FactsOutcome {
    /// The parameters whose loans the result holds; `None` holds them all.
    retained: Option<Box<[bool]>>,
    stores: StoreFacts,
    /// The closed result is revoked: the result holds a loan.
    revoke: bool,
    /// The body does not check: the program is rejected, whatever the facts become.
    failed: bool,
}

/// One check of the body of function `id` under `facts`, for `joint_facts`: the parameters whose
/// loans its result holds (`retention`; `None` when the body does not check, when its result holds
/// a loan of its own locals, or when it lends a way to write or makes an opaque call,
/// `lending_bodies`), its store facts (`stores`; `top` when it does not check), and whether its
/// result holds a loan (`revocable`).
fn check_facts(
    module: &CheckedModule,
    id: usize,
    infer: bool,
    facts: &CallFacts,
    retention: bool,
    stores: bool,
    revocable: bool,
) -> FactsOutcome {
    let function = &module.functions[id];
    let retention = retention && !facts.lending.no_facts(id);
    if !retention && !stores && !revocable {
        return FactsOutcome {
            retained: None,
            stores: StoreFacts::default(),
            revoke: false,
            failed: false,
        };
    }
    let user_drop = module.user_drops.values().any(|drop| *drop == id);
    let mut recovered = Vec::new();
    let mut checker = start_checker(
        module,
        function.parameters.iter(),
        &function.body,
        infer,
        facts,
        function.is_once,
        user_drop,
        facts.contract(module, id),
        &mut recovered,
        None,
        None,
    );
    checker.drops_stop_reads = drops_stop_reads(
        module,
        function.parameters.iter(),
        &function.body,
        infer,
        facts,
        function.is_once,
        user_drop,
        facts.contract(module, id),
    );
    checker.collect_drops();
    if stores {
        checker.stores = Some(StoreFacts::default());
    }
    // The body's value goes to the caller, which applies the drop hook where it keeps it.
    checker.bound_next = true;
    let evaluated = checker
        .eval(&function.body, Use::Consume, &BTreeSet::new())
        .map(|mut result| {
            checker.absorb_hidden(0, &mut result, &function.body.ty);
            result
        });
    let result = match evaluated {
        Ok(result) if checker.recovered.is_empty() && checker.deferred.is_empty() => result,
        _ => {
            return FactsOutcome {
                retained: None,
                stores: StoreFacts::top(),
                revoke: revocable,
                failed: true,
            };
        }
    };
    let found = checker.stores.take().unwrap_or_default();
    let revoke = revocable
        && result
            .loans
            .iter()
            .any(|loan| !is_static_root(checker.loans[*loan].place.root));
    if !retention {
        return FactsOutcome {
            retained: None,
            stores: found,
            revoke,
            failed: false,
        };
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
        return FactsOutcome {
            retained: None,
            stores: found,
            revoke,
            failed: false,
        };
    }
    let mut retained = if infer {
        hidden_parameters(module, function)
    } else {
        vec![false; function.parameters.len()]
    };
    // A value that the body drops, or hands to a callee that may drop it, runs its `Drop.drop`,
    // which may store what the value holds in a cell inside it that the result reaches as well
    // (`drop_runs_code`): the parameters whose loans such a value holds are retained.
    let dropped = checker.dropped.take().unwrap_or_default();
    let mut pending: Vec<usize> = result.loans.iter().chain(&dropped).copied().collect();
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
    FactsOutcome {
        retained: Some(retained.into_boxed_slice()),
        stores: found,
        revoke,
        failed: false,
    }
}

/// Whether `body` hands to other code a way to write into a place that its values reach: an
/// operand of a call, a pipeline, an operator, a host or parallel call, or the captures of a
/// closure, whose type `lends_mutable` (`Type::lends_mutable`): it reaches an exclusive reference
/// (`ref mut values`, `ref mut values[0..1]`, a `ref mut` parameter or local, a tuple or a record
/// that holds one, a `MatrixView.Mut`) or a mutable cell (a `Mutex`, `RwMutex`, `OnceValue`,
/// `SyncMap`, channel or `ErrGroup` whose contents may hold a borrow), through stored values, the
/// targets of references, the contents of `Rc` and `Arc`, and the parameter and result types of
/// function types (a callback `put text : ref mut [T] -> unit`). Exclusive references count when
/// `exclusive` is true.
///
/// A28 trusts the loans that the check of a body gives its result: the facts drop the arguments
/// that do not reach it, and `read_through` takes what a borrowed local owner holds. Both are
/// sound when the loans that the check gives the values are complete: a value that holds another
/// holds its loans.
///
/// The primary argument is A31's store facts. The check tracks every store that the language
/// makes: an assignment, `deref r = v` and a function with a region contract add the stored
/// value's loans to the owner of the place; a store through a reference that a body gets (a
/// `ref mut` parameter, or the `ref mut` that a lock lends its callback) needs a named region for
/// a value that holds loans, and a function with such a contract runs only in direct calls
/// (`regions::validate_contract_calls`); a channel's items, an `OnceValue`'s initializer and an
/// `Owned.Function` are checked to hold no borrow; and the builtins that store a value where an
/// exclusive reference or a cell that they get reaches (`store_seed`: `Array.write`, which
/// `Array.set`, `Array.fill`, `Array.blit` and the other std functions that write in place call,
/// `SyncMap.__replace` and `Vec.push_mut`) have store facts, which every call of them or of a
/// function that calls them applies, directly, through a pipeline, as a callback or as a function
/// value (`StoreFacts`, `Checker::apply_store`); the other writers store numbers. Code that runs
/// without a call in the source (a drop, an operator, a class method, a dyn method) and the code of
/// a function value are bodies of the module, checked under the same rules, and a store of a value
/// that holds no loan cannot make a loan incomplete. So every value's loans are complete wherever
/// the check meets it, through any number of calls and callbacks: the facts hold (a callee leaves
/// out of its result the arguments that do not reach it, whatever they hold) and so does the
/// read-through (a share holds what its owner's loans say the owner holds), beside the stores of
/// code whose body the analysis does not see and of drops, which the local guards below handle.
/// Before A31 the seeds had no facts, and a program that stored a value that may hold loans with
/// one had no facts at all and read nothing through (A28, 8th review).
///
/// The local guards of the third to seventh reviews keep a body that may make a store its check
/// misses from using facts or reading through, whatever the rest of the program does. The rest of
/// this comment is their argument, which enumerates the ways a value can reach a place that the
/// check misses rather than the writers. A store that the check does not track writes through an
/// exclusive reference or through a mutable cell: other stores are tracked, and a shared reference
/// to anything else lends no way to write.
///
/// A body's value (a parameter's loan included) can thus reach a place that its result does not
/// show in four ways only. Each of the first three makes the body lending (`lending_bodies`: no
/// facts, no read-through, the rule from before A28; a call that is opaque only through the
/// callee's environment drops the facts alone, `OpaqueCalls::environment`), and the fourth keeps
/// the facts but retains what the dropped values hold. Since A31, (a) keeps only part of its
/// guard (`lending_bodies`): what code stores through an exclusive reference or a cell that the
/// body hands it is applied at the call (the store facts), so the body keeps its facts, and it
/// reads through unless it hands out a cell, which an `Rc` or `Arc` may share with other owners
/// whose loans a store into it does not reach. `Lending::drops_store` keeps all four:
///
/// - (a) A store through an exclusive reference or a cell that the body hands to code as one of
///   the operands above, which this function finds. The reference or the cell comes from the body:
///   a parameter, a capture, a local, or a value made from them, and the only ways a body hands a
///   value to code are those operands and the captures of the lambdas that it makes.
/// - (b) Code whose body the analysis does not see, which receives the value and may store it in a
///   cell that it captured and that another input of the body reaches as well (a writer and a
///   reader closure over one `Arc<Mutex<…>>`, `w` and `r` of `pass w r text`): a function type,
///   a dyn type or an `Owned.Function` does not show its environment, so neither its parameter
///   types nor `lends_mutable` can tell. `opaque_calls` finds these calls, and a named callee
///   that makes one (`opaque_bodies`) counts as one when it gets a value that can run code.
/// - (c) A lambda that the body makes, whose body does (a) or (b) with what it captured or gets:
///   `lending_bodies` marks the bodies related to it.
/// - (d) A drop. Dropping a value runs the `Drop.drop` of every value with a user `Drop` instance
///   that it stores, with no call in the body: at the end of a scope, at a reassignment, for a
///   moved-in parameter that is not moved on, for a temporary, for the elements of a container, in
///   `Owned.drop`, before a tail call, and in a callee that the value is handed to. `Drop.drop`
///   gets only the dropped value, so what it stores is what that value holds, and the place is a
///   cell inside the value, which another input or the result may share (`Stash { cell: a, f }`
///   with `r` reading `a`). Its body makes a store that its check misses only as (a), (b) or (c)
///   do, which `Lending::drops_store` asks of every `Drop.drop` of the module. When one may, every
///   value that the body gets or makes whose type may run code when it is dropped
///   (`Type::drop_runs_code`: it stores a user `Drop` type, or a function value, dyn value,
///   `Owned.Function` or tzroutine body, whose environment or vtable may hold one) counts as
///   dropped: `retained_parameters` retains the parameters whose loans it holds, and
///   `drops_stop_reads` keeps a body that drops one that holds loans from reading through. A
///   parameter that reaches such a value through an exclusive reference counts too
///   (`deref target = value` drops the old one). Counting every such value, not only those that
///   the body drops itself, covers a value handed to a named callee that drops it and returns
///   what cannot hold its loans (`consume s; r`): the callee's facts keep the loans in its result,
///   and that result has none. `HashMap.remove` keeps its facts: the entry that it drops holds
///   the loans of `map`, which its result holds anyway.
///
/// Nothing else runs code with the body's values. The code that the language runs without a call
/// in the source is analyzed as the body of a named function, counted as code that the analysis
/// does not see, or checked to get no borrow:
///
/// - Operators on user types (`+`, `-x`, `==`, `<`) are direct calls of the named instance
///   functions (`polymorph::operator_call`).
/// - Class methods (`Display` in an interpolation hole, the `Eq`, `Hash`, `Ord` and `Default`
///   that std collections call, the instances that `src/derive.rs` derives, which are checked as
///   written instances) are named instance functions after specialization, or the vtable's
///   dispatch of a dyn value (`E::DynDispatch`, opaque). An interpolation hole's method is a call
///   for `opaque_calls`; structural comparison, hashing and display of an array, a list or a
///   tuple run the methods of the element types, which the expression lists as function values
///   after its operands (the store facts apply them, `Checker::eval_value`), and the methods of
///   the dyn values that they reach, which `opaque_calls` counts.
/// - `for x in seq` is a loop of calls of `Seq` functions (`control::sequence_loop`), and `for x
///   in channel` calls `recv`; a loop over an array, list or string runs no code.
/// - `as` (`E::Cast`) converts numbers and the purity of a function type only, `deref` reads a
///   place, and a pattern guard or a `finally` block is an expression of the body.
/// - `new [T](n, f)` runs `f` (`opaque_calls`).
/// - A tzroutine's captures, a channel's items, an `OnceValue`'s initializer and an
///   `Owned.function`'s argument are checked to hold no borrow.
/// - A named callee that does none of the four keeps its facts, which hold whatever its arguments
///   hold.
///
/// So a body for which neither this function nor `opaque_bodies` is true makes no store that its
/// check misses into a place that its result or another input reaches, beside those of its drops,
/// whose loans its facts keep.
///
/// That makes the loans of the body's own values complete, not those of its parameters: a
/// parameter's external loans stand for what the caller's value holds, and a caller that lends a
/// way to write may have stored in that value what its loans miss. The facts do not depend on
/// that (a callee that leaves an argument out of its result leaves it out whatever it holds), but
/// a read through a parameter's contents would, so `read_through` keeps the borrow of an owner
/// whose contents come from the caller.
///
/// In an abstract body a type variable counts as neither an exclusive reference nor the contents
/// of a cell that may hold a borrow: those facts only infer Copy constraints, and each instance,
/// whose types are concrete, is checked with its own facts. The same holds for `opaque_calls`.
fn lends_mutable(body: &TypedExpr, types: &crate::check::TypeContext<'_>, exclusive: bool) -> bool {
    let lends = |operand: &TypedExpr| operand.ty.lends_mutable(types, exclusive);
    let mut pending = vec![body];
    while let Some(expression) = pending.pop() {
        let lending = match &expression.kind {
            E::Call(_, arguments)
            | E::HostCall(_, arguments)
            | E::Parallel(_, arguments)
            | E::Closure(_, arguments) => arguments.iter().any(lends),
            // The right side of a pipeline is the callee; its argument is the left side.
            E::Binary(BinaryOp::Pipe, left, _) => lends(left),
            E::Binary(_, left, right) => lends(left) || lends(right),
            _ => false,
        };
        if lending {
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

/// Whether a value of type `ty` can run code when other code gets it: it reaches a function value
/// (an `Owned.Function`, a lazy sequence and an `IO` action hold one), a dyn value (its methods
/// are the vtable's) or a tzroutine body. A type variable counts as none, as in `lends_mutable`.
fn may_run_code(ty: &Type, types: &crate::check::TypeContext<'_>) -> bool {
    types.reaches(ty, |ty| {
        matches!(ty, Type::Function(..) | Type::Dyn(_) | Type::Once(_))
    })
}

/// Whether dropping a value of type `ty` may run code that the body does not show: the value
/// stores (not through a reference) a value of a type with a user `Drop` instance, a function
/// value (its environment may hold one), an `Owned.Function`, a dyn value (its drop is the
/// vtable's) or a tzroutine body (`runs_on_drop`). A type variable counts as none, as in
/// `lends_mutable`.
fn drop_runs_code(ty: &Type, types: &crate::check::TypeContext<'_>) -> bool {
    ty.drop_runs_code(types)
}

/// Whether the value of `expression` may hold loans: a closure holds those of its captures, a
/// named function none; anything else as its type says.
fn value_may_carry_loans(expression: &TypedExpr, types: &crate::check::TypeContext<'_>) -> bool {
    match &expression.kind {
        E::Closure(_, captures) => captures
            .iter()
            .any(|capture| capture.ty.carries_loans(types)),
        E::Function(_) | E::GenericFunction(..) => false,
        _ => expression.ty.carries_loans(types),
    }
}

/// Whether a parameter of a builtin's scheme can hold code that the builtin may run: a function
/// type, a std type whose declaration holds one (`Owned.Function`, `Seq`), or a container of
/// them. A scheme variable cannot: the builtin knows nothing of its values, beyond the instances
/// of its constraints (`builtin_runs_arguments`).
fn scheme_runs_code(ty: &crate::check::BuiltinType, types: &crate::check::TypeContext<'_>) -> bool {
    use crate::check::BuiltinType as B;
    match ty {
        B::Function(..) => true,
        B::Var(_) => false,
        B::Concrete(ty) => may_run_code(ty, types),
        B::Std { module, name, args } => {
            let name = format!("{module}.{name}");
            let declared = types
                .records
                .iter()
                .find(|record| {
                    record.origin == crate::check::ModuleOrigin::Std && record.name == name
                })
                .map(|record| record.fields.iter().any(|(_, ty)| may_run_code(ty, types)))
                .or_else(|| {
                    types
                        .unions
                        .iter()
                        .find(|union| {
                            union.origin == crate::check::ModuleOrigin::Std && union.name == name
                        })
                        .map(|union| {
                            union
                                .cases
                                .iter()
                                .flat_map(|(_, payload)| payload)
                                .any(|ty| may_run_code(ty, types))
                        })
                });
            // A std type that this module does not declare cannot be an argument here; judge it
            // as one that holds code.
            declared.unwrap_or(true) || args.iter().any(|ty| scheme_runs_code(ty, types))
        }
        B::Array(element)
        | B::ArrayView(element)
        | B::List(element)
        | B::Vec(element)
        | B::Shared(element, _)
        | B::Reference(element, _)
        | B::FixedArrayOf(element) => scheme_runs_code(element, types),
        B::Tuple(elements) => elements.iter().any(|ty| scheme_runs_code(ty, types)),
        B::UnsignedOf(_) | B::WidenOf(_) | B::SimdLane(..) | B::SimdMask(_) => false,
    }
}

/// Whether `builtin` may run code that it gets: a parameter of its scheme holds code
/// (`scheme_runs_code`: `Array.map`, `Mutex.with_lock`, `Owned.call`, `Seq.next`), or it has a
/// constraint, whose instance for a dyn value is the vtable's.
fn builtin_runs_arguments(
    builtin: crate::check::Builtin,
    types: &crate::check::TypeContext<'_>,
    cache: &mut BTreeMap<crate::check::Builtin, bool>,
) -> bool {
    *cache.entry(builtin).or_insert_with(|| {
        let scheme = builtin.scheme();
        !scheme.constraints.is_empty()
            || scheme
                .parameters
                .iter()
                .any(|parameter| scheme_runs_code(parameter, types))
    })
}

/// The calls of a body that hand its values to code whose body the analysis does not see
/// (`lends_mutable`, way (b)).
#[derive(Default)]
struct OpaqueCalls {
    /// The body makes such a call itself, with an argument that may hold loans.
    direct: bool,
    /// The body calls a value whose environment may hold loans, with no argument that may: the
    /// environment may store what it holds where another input of the body reaches it, so the
    /// body has no facts, but none of the body's own values gets a loan that it lacks (what the
    /// environment holds came to it through a value that the check followed), so the body still
    /// reads through.
    environment: bool,
    /// The named functions that the body calls with all their arguments, one of which may hold
    /// loans and one of which can run code: the body makes such a call when one of them does
    /// (`opaque_bodies`).
    callees: Vec<usize>,
}

/// The calls of `body` that may hand a value that holds loans to code whose body the analysis does
/// not see. A call is opaque when its callee is not a direct reference to a named function or
/// builtin (a parameter, capture, local, field, element, call result, or a named function given
/// more arguments than it has parameters, whose result is called), when it is the dispatch of a
/// dyn method (`E::DynDispatch`), when a builtin that may run its arguments
/// (`builtin_runs_arguments`: `Owned.call`, `Array.map`, `Mutex.with_lock`, `Parallel.*`, a host
/// function that takes a callback) gets one that can run code, and when a collection made with
/// `new` runs its initializer. The callee's environment counts as well (`environment`): a closure
/// that captured a writer and a borrow (`\() -> w (\() -> String.length text)`) stores the borrow
/// when it is called with `()`. In an abstract body, a method of a class whose receiver reaches a dyn
/// value is opaque too. A type variable counts as neither a value that can run code nor one that
/// holds loans, and a method of a class whose receiver is one as a named function, as in
/// `lends_mutable`: the abstract body's facts only infer Copy constraints, and the abstract check
/// must not reject what its instances accept (a body that is not generic is checked in both, and
/// `HashMap.remove_ref` calls `Hash.hash` and `Eq.eq` on its key). Each instance, where the
/// method is a named instance or the vtable's dispatch, is checked with its own facts. A call of a
/// named function that may make such a call itself is recorded in `callees`.
fn opaque_calls(
    body: &TypedExpr,
    module: &CheckedModule,
    cache: &mut BTreeMap<crate::check::Builtin, bool>,
) -> OpaqueCalls {
    let types = module.types();
    let mut found = OpaqueCalls::default();
    let loans = |arguments: &[&TypedExpr]| {
        arguments
            .iter()
            .any(|argument| value_may_carry_loans(argument, &types))
    };
    let code = |arguments: &[&TypedExpr]| {
        arguments
            .iter()
            .any(|argument| may_run_code(&argument.ty, &types))
    };
    let call = |callee: &TypedExpr,
                arguments: &[&TypedExpr],
                found: &mut OpaqueCalls,
                cache: &mut BTreeMap<crate::check::Builtin, bool>| {
        match &callee.kind {
            E::Function(crate::check::FunctionRef::User(id)) | E::GenericFunction(id, _) => {
                let arity = module.functions[*id].parameters.len();
                if arguments.len() > arity {
                    // The result, which holds only the arguments, is called with the rest.
                    found.direct |= loans(arguments);
                } else if arguments.len() == arity && loans(arguments) && code(arguments) {
                    found.callees.push(*id);
                }
            }
            E::Function(crate::check::FunctionRef::Builtin(instance)) => {
                if loans(arguments)
                    && (arguments.len() > instance.builtin.scheme().parameters.len()
                        || (builtin_runs_arguments(instance.builtin, &types, cache)
                            && code(arguments)))
                {
                    found.direct = true;
                }
            }
            E::Method(_, _, receiver) | E::TypeFunction { receiver, .. } => {
                if types.reaches(receiver, |ty| matches!(ty, Type::Dyn(_))) && loans(arguments) {
                    found.direct = true;
                }
            }
            E::CaseConstructor { .. } => {}
            _ => {
                if loans(arguments) {
                    found.direct = true;
                } else if value_may_carry_loans(callee, &types) {
                    found.environment = true;
                }
            }
        }
    };
    let mut pending = vec![body];
    while let Some(expression) = pending.pop() {
        match &expression.kind {
            E::Call(callee, arguments) => {
                let arguments: Vec<&TypedExpr> = arguments.iter().collect();
                call(callee, &arguments, &mut found, cache);
            }
            E::Binary(BinaryOp::Pipe, left, right) => match &right.kind {
                E::Call(callee, arguments) => {
                    let arguments: Vec<&TypedExpr> = arguments.iter().chain([&**left]).collect();
                    call(callee, &arguments, &mut found, cache);
                }
                _ => call(right, &[&**left], &mut found, cache),
            },
            E::HostCall(import, arguments) => {
                let arguments: Vec<&TypedExpr> = arguments.iter().collect();
                if import.callbacks && loans(&arguments) && code(&arguments) {
                    found.direct = true;
                }
            }
            E::Parallel(_, arguments) => {
                let arguments: Vec<&TypedExpr> = arguments.iter().collect();
                if loans(&arguments) {
                    found.direct = true;
                }
            }
            E::NewArray(_, initializer) | E::NewList(_, initializer) => {
                call(initializer, &[], &mut found, cache);
            }
            E::Interpolated(interpolation) => {
                for hole in &interpolation.holes {
                    if let Some(method) = &hole.method {
                        call(method, &[&hole.operand], &mut found, cache);
                    }
                }
            }
            E::StructuralCompare(_, operands)
            | E::StructuralHash(operands)
            | E::StructuralDisplay(operands) => {
                let operands: Vec<&TypedExpr> = operands.iter().collect();
                if loans(&operands)
                    && operands
                        .iter()
                        .any(|operand| types.reaches(&operand.ty, |ty| matches!(ty, Type::Dyn(_))))
                {
                    found.direct = true;
                }
            }
            E::DynDispatch { .. } => found.direct = true,
            _ => {}
        }
        expression
            .try_for_each_child(&mut |child| {
                pending.push(child);
                Ok(())
            })
            .expect("collecting calls cannot fail");
    }
    found
}

/// The lambdas lifted from each function and the functions they are lifted from, in both
/// directions, through the lambdas in lambdas and the groups of `let rec`.
fn related_bodies(module: &CheckedModule) -> Vec<Vec<usize>> {
    let mut related: Vec<Vec<usize>> = vec![Vec::new(); module.functions.len()];
    for (id, function) in module.functions.iter().enumerate() {
        let mut pending = vec![&function.body];
        while let Some(expression) = pending.pop() {
            if let E::Closure(lambda, _) = expression.kind
                && lambda != id
            {
                related[id].push(lambda);
                related[lambda].push(id);
            }
            pending.extend(expression.children());
        }
    }
    related
}

/// Marks every function that `edges` reaches from a marked one.
fn close_marks(marks: &mut [bool], edges: &[Vec<usize>]) {
    let mut work: Vec<usize> = (0..marks.len()).filter(|id| marks[*id]).collect();
    while let Some(id) = work.pop() {
        for &other in &edges[id] {
            if !marks[other] {
                marks[other] = true;
                work.push(other);
            }
        }
    }
}

/// By function: whether calling it may hand an argument's value to code whose body the analysis
/// does not see (`opaque_calls`), or the body of a lambda lifted from it, or of the function that
/// it was lifted from, does. Upward: `Array.map` runs the lambda that it makes (in `new`), whose
/// body calls `transform`. Through calls: a body that calls such a function with all its arguments,
/// one of which may hold loans and one of which can run code, may hand the first to the second
/// (`fn pass w text = Array.iter w (ref [\() -> String.length text])` calls `Array.iter`, whose
/// body calls `w`). The least fixed points over these edges: the first vector counts the calls
/// that only an environment makes opaque (`OpaqueCalls::environment`), the second does not.
fn opaque_bodies(module: &CheckedModule, related: &[Vec<usize>]) -> (Vec<bool>, Vec<bool>) {
    let mut cache = BTreeMap::new();
    let mut edges = related.to_vec();
    let mut any = Vec::with_capacity(module.functions.len());
    let mut direct = Vec::with_capacity(module.functions.len());
    for (id, function) in module.functions.iter().enumerate() {
        let found = opaque_calls(&function.body, module, &mut cache);
        any.push(found.direct || found.environment);
        direct.push(found.direct);
        for callee in found.callees {
            edges[callee].push(id);
        }
    }
    close_marks(&mut any, &edges);
    close_marks(&mut direct, &edges);
    (any, direct)
}

/// What `lending_bodies` finds, by function.
#[derive(Clone)]
struct Lending {
    /// The body has no facts: it makes an opaque call.
    facts: Vec<bool>,
    /// Nothing is read through in the body: it lends a mutable cell, or makes an opaque call with
    /// an argument that may hold loans.
    reads: Vec<bool>,
    /// `opaque_bodies` without the environments, for the bodies that are not functions of the
    /// module.
    opaque: Vec<bool>,
    /// Some `Drop.drop` body of the module lends a way to write or hands a value to code that the
    /// analysis does not see (A28's guard, which A31 keeps), so dropping a value may make a store
    /// that the check does not track where its result does not show it (`drop_runs_code`).
    drops_store: bool,
}

impl Lending {
    /// Whether the body of function `id` has no facts: it makes an opaque call.
    fn no_facts(&self, id: usize) -> bool {
        self.facts[id]
    }

    /// Whether nothing is read through in the body of function `id` (`reads`).
    fn no_reads(&self, id: usize) -> bool {
        self.reads[id]
    }
}

/// By function: whether its body has no facts and whether it reads nothing through (`Lending`),
/// because it may hand a value to code whose body the analysis does not see (`opaque_bodies`) or
/// lends a mutable cell (`lends_mutable`), or the body of a lambda lifted from it, or of the
/// function that it was lifted from, does.
///
/// The checks apply the store facts of every call (A31), so a body that lends an exclusive
/// reference or a cell to code keeps its facts: what the code stores there is in the owner's loans
/// after the call. It reads nothing through when it lends a cell: the cell may be in a block that
/// an `Rc` or `Arc` shares, and a store into it through one owner reaches that owner's loans, not
/// the copies that a share read through another owner holds (`Arc.share (ref a)` and then
/// `Mutex.with_lock (Arc.get (ref a)) ...`). An exclusive reference cannot reach into such a block:
/// the contents of `Rc` and `Arc` are read-only but for cells. A `Drop.drop` that lends any way to
/// write keeps A28's guard (`drops_store`), and so does a body of the inference that lends any way
/// to write and may store, since the abstract bodies apply no store facts.
///
/// Downward: a lambda's captures hold what the enclosing body gave them, and that misses what a
/// store that the check misses put there, so a lambda whose captures were made in such a body reads
/// nothing through them either (and `read_through` keeps the borrow of a capture in any lambda,
/// since a capture is a parameter of the lifted function; this mark also drops the lambda's facts,
/// as for its enclosing body). Upward: a lambda that hands a value to unseen code, or lends a cell,
/// may write into what the enclosing body gave it, so the enclosing body keeps the same rule; the
/// scan of an unlifted body sees the bodies of its `E::Lambda`s already. The lifted lambdas follow
/// the `E::Closure` that makes them, through the lambdas in lambdas and the groups of `let rec`.
fn lending_bodies(module: &CheckedModule, infer: bool) -> Lending {
    let types = module.types();
    let related = related_bodies(module);
    let (any, direct) = opaque_bodies(module, &related);
    // Only a body that reads something through, or one related to it, asks `reads`
    // (`Checker::may_read_through`, and the lambdas that `eval_lambda` checks with its answer), so
    // the cells are looked for there alone.
    let mut sharing: Vec<bool> = module
        .functions
        .iter()
        .map(|function| calls_read_through(module, &function.body))
        .collect();
    close_marks(&mut sharing, &related);
    let cells: Vec<bool> = module
        .functions
        .iter()
        .zip(&sharing)
        .map(|(function, sharing)| *sharing && lends_mutable(&function.body, &types, false))
        .collect();
    // Whether the body of function `id` lends any way to write; asked only where it matters.
    let lends = |id: usize| cells[id] || lends_mutable(&module.functions[id].body, &types, true);
    let or =
        |a: &[bool], b: &[bool]| -> Vec<bool> { a.iter().zip(b).map(|(a, b)| *a || *b).collect() };
    // A28's guard for the drops: some body related to a `Drop.drop` (`related` is symmetric, so
    // these are the bodies that the marks of `close_marks` would carry to it) lends a way to write
    // or makes an opaque call.
    let mut dropping = vec![false; module.functions.len()];
    for drop in module.user_drops.values() {
        dropping[*drop] = true;
    }
    close_marks(&mut dropping, &related);
    let drops_store = (0..module.functions.len()).any(|id| dropping[id] && (any[id] || lends(id)));
    // The abstract bodies of the inference apply no store facts (their lambdas are not lifted, and
    // a type variable carries no loans): a body there that lends a way to write and may store
    // (`store_candidates`, with the concrete types of the abstract module) keeps the rule from
    // before A28.
    let (mut facts, mut reads) = if infer {
        let candidates = store_candidates(module, true);
        let guarded: Vec<bool> = (0..module.functions.len())
            .map(|id| (candidates.may_store[id] || candidates.top[id]) && lends(id))
            .collect();
        (or(&any, &guarded), or(&or(&cells, &guarded), &direct))
    } else {
        (any, or(&cells, &direct))
    };
    close_marks(&mut facts, &related);
    close_marks(&mut reads, &related);
    Lending {
        facts,
        reads,
        opaque: direct,
        drops_store,
    }
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

/// A31: the immutable locals of `body` that are bound to a function value whose code the body
/// shows, with the expressions that the value may be (`Checker::known`): a lifted lambda, a named
/// function, a partial application of a named function, another local, the argument of
/// `Owned.function`, or an `if` between such. A call of the local runs one of them with the
/// environment that the local holds, so `Checker::apply_higher` substitutes their facts where it
/// would otherwise take the call for one of code that it does not see. The local is immutable and
/// its binding comes before every use, so the expressions name the code of every call. An
/// immutable local bound to an array literal of such values, or to a record literal, gives
/// `parts` the expressions that an element (`(local, None)`) or a field (`(local, Some(field))`)
/// may be: `hs[0] x` with `let hs = [\f -> f () + 1, \f -> f () * 2]` runs one of the two.
fn known_functions<'a>(
    module: &CheckedModule,
    body: &'a TypedExpr,
    known: &mut BTreeMap<usize, Vec<&'a TypedExpr>>,
    parts: &mut BTreeMap<(usize, Option<usize>), Vec<&'a TypedExpr>>,
) {
    fn shown<'a>(
        module: &CheckedModule,
        value: &'a TypedExpr,
        found: &mut Vec<&'a TypedExpr>,
    ) -> bool {
        match &value.kind {
            E::Closure(..)
            | E::Function(crate::check::FunctionRef::User(_))
            | E::GenericFunction(..)
            | E::Local(_) => {
                found.push(value);
                true
            }
            E::Call(callee, arguments) => {
                if let Some(id) = direct_callee(callee) {
                    if arguments.len() < module.functions[id].parameters.len() {
                        found.push(value);
                        return true;
                    }
                    return false;
                }
                arguments.len() == 1
                    && builtin_called(module, callee) == Some(crate::check::Builtin::OwnedFunction)
                    && shown(module, &arguments[0], found)
            }
            E::If {
                then_branch,
                else_branch,
                ..
            } => shown(module, then_branch, found) && shown(module, else_branch, found),
            _ => false,
        }
    }
    let mut pending = vec![body];
    while let Some(expression) = pending.pop() {
        match &expression.kind {
            E::Block { bindings, .. } => {
                for (local, value) in bindings {
                    if local.mutable {
                        continue;
                    }
                    let mut found = Vec::new();
                    if shown(module, value, &mut found) {
                        known.insert(local.id, found);
                        continue;
                    }
                    // An immutable array or record made of such values: an element or a field
                    // of it is one of them (nothing writes into an immutable local).
                    match &value.kind {
                        E::Array(elements) if !elements.is_empty() => {
                            let mut found = Vec::new();
                            if elements
                                .iter()
                                .all(|element| shown(module, element, &mut found))
                            {
                                parts.insert((local.id, None), found);
                            }
                        }
                        E::Record(fields) => {
                            for (field, value) in fields {
                                let mut found = Vec::new();
                                if shown(module, value, &mut found) {
                                    parts.insert((local.id, Some(*field)), found);
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            E::Lambda { .. } => continue,
            _ => {}
        }
        expression
            .try_for_each_child(&mut |child| {
                pending.push(child);
                Ok(())
            })
            .expect("collecting bindings cannot fail");
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

/// The builtin that `callee` is, or that the generated function it names calls on its own
/// parameters after lowering (every builtin is called through one).
fn builtin_called(module: &CheckedModule, callee: &TypedExpr) -> Option<crate::check::Builtin> {
    let E::Function(function) = &callee.kind else {
        return None;
    };
    match function {
        crate::check::FunctionRef::Builtin(instance) => Some(instance.builtin),
        crate::check::FunctionRef::User(id) => {
            let wrapper = &module.functions[*id];
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

/// A28: whether `callee` is `share`, `downgrade` or `upgrade` of `Rc` or `Arc`, whose result the
/// check may read through the borrow of its owner (`Checker::reads_through`).
fn reads_through(module: &CheckedModule, callee: &TypedExpr) -> bool {
    use crate::check::Builtin::{
        ArcDowngrade, ArcShare, ArcUpgrade, RcDowngrade, RcShare, RcUpgrade,
    };
    matches!(
        builtin_called(module, callee),
        Some(RcShare | ArcShare | RcDowngrade | ArcDowngrade | RcUpgrade | ArcUpgrade)
    )
}

/// Whether `body` calls a function whose result the check may read through (`reads_through`),
/// directly or as the right side of a pipeline, in the body or in a lambda in it.
fn calls_read_through(module: &CheckedModule, body: &TypedExpr) -> bool {
    let mut pending = vec![body];
    while let Some(expression) = pending.pop() {
        let found = match &expression.kind {
            E::Call(callee, _) => reads_through(module, callee),
            E::Binary(BinaryOp::Pipe, _, right) => reads_through(module, right),
            _ => false,
        };
        if found {
            return true;
        }
        pending.extend(expression.children());
    }
    false
}

/// A28, implicit drops: whether the body of a check drops a value that holds loans and whose type
/// may run code when it is dropped (`drop_runs_code`), under the read-through: a `Drop.drop` that
/// it runs may then store what the value holds in a cell inside the value, and that cell may be in
/// a block that the body reads through (`Rc.share (ref owner)`), whose loans miss the store. Only a
/// body that reads something through (`calls_read_through`) is checked for it, once without the
/// guard; a body whose check fails keeps the read-through, since the check with it fails as well.
#[allow(clippy::too_many_arguments)]
fn drops_stop_reads<'a>(
    module: &'a CheckedModule,
    parameters: impl Clone + Iterator<Item = &'a Local>,
    body: &'a TypedExpr,
    infer: bool,
    facts: &CallFacts,
    task: bool,
    user_drop: bool,
    contract: Contract<'a>,
) -> bool {
    if !facts.lending.drops_store
        || contract.lends_mutable == Some(true)
        || !calls_read_through(module, body)
    {
        return false;
    }
    let mut recovered = Vec::new();
    let mut checker = start_checker(
        module,
        parameters,
        body,
        infer,
        facts,
        task,
        user_drop,
        contract,
        &mut recovered,
        None,
        None,
    );
    checker.collect_drops();
    checker.eval(body, Use::Consume, &BTreeSet::new()).is_ok()
        && checker.read_through_used
        && checker
            .dropped
            .as_ref()
            .is_some_and(|dropped| !dropped.is_empty())
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
    /// The body is a generated function that applies a builtin to its own parameters (A30).
    builtin_wrapper: bool,
    /// Whether the body, or one that it is lifted from or that is lifted from it, lends a way to
    /// write (`lending_bodies`); the body alone is asked when the contract does not say.
    lends_mutable: Option<bool>,
    /// A28, implicit drops: the loans of the values that the body gets or makes whose types may
    /// run code when they are dropped (`drop_runs_code`), when the caller asks for them
    /// (`retained_parameters`, `drops_stop_reads`).
    dropped: Option<BTreeSet<usize>>,
    /// `drop_runs_code` by type, for `dropped`.
    drop_types: BTreeMap<Type, bool>,
    /// Whether the check read something through the borrow of an owner (`read_through`).
    read_through_used: bool,
    /// Nothing is read through: the body drops a value that holds loans and may run code when it
    /// is dropped (`drops_stop_reads`).
    drops_stop_reads: bool,
    /// A31: the store facts of the body, when the caller asks for them (`store_facts`).
    stores: Option<StoreFacts>,
    /// A31: the errors that the check finds where the body stores a borrow of its own local
    /// value where its caller's data reaches; `check_body` returns the first one after the
    /// body's evaluation, so the errors that the evaluation finds come first.
    deferred: Vec<Diagnostic>,
    /// A31: the index of each parameter, by its external root.
    parameter_roots: BTreeMap<usize, usize>,
    /// A31: whether a type reaches a mutable cell whose contents may hold a loan, by type.
    cell_types: BTreeMap<Type, bool>,
    /// A31: whether a type reaches a value that hides an environment (`hides_environment`), and
    /// whether a place of the type can hold a stored loan (`holds_stored`), by type.
    hidden_types: BTreeMap<Type, (bool, bool, bool)>,
    /// A31: whether a store through a shared borrow of a place of a type that hides an
    /// environment reaches what the place holds (`store_target`), by type.
    shared_targets: BTreeMap<Type, bool>,
    /// A31: whether a value of a type may be a place that a store reaches through it
    /// (`may_reach_store`), and whether it may hold a function value of one of the types that own
    /// the cell that they write, or reach a cell (`argument_hides`), by type.
    argument_types: BTreeMap<Type, (bool, bool)>,
    /// A31: the immutable locals of the body that are bound to a function value whose code the
    /// body shows (a lambda, a named function, a partial application of one, an `Owned.function`
    /// of one, or a choice between such), with those expressions (`known_functions`): a call of
    /// such a local substitutes their facts.
    known: BTreeMap<usize, Vec<&'a TypedExpr>>,
    /// A31: the elements (`(local, None)`) and fields (`(local, Some(field))`) of the immutable
    /// locals bound to an array or record literal of such values (`known_functions`).
    known_parts: BTreeMap<(usize, Option<usize>), Vec<&'a TypedExpr>>,
    /// A31: the applications of store facts that the current pass over a call has made
    /// (`settle`); one made again in the same pass changes nothing.
    store_memo: Option<BTreeSet<StoreKey>>,
    /// A31: how many times a store changed what a place holds or what the body's facts say.
    store_changes: usize,
    /// A31: what was stored in environments that the check cannot name (`store_hidden`), in the
    /// order of the stores: a value that is bound afterwards may be a copy made before the store,
    /// so a binding absorbs what was stored while its value was evaluated.
    hidden_log: Vec<Value>,
    /// A31: the span that reports what the drop of a temporary stores (`drop_stores`): the
    /// declaration or the call that the temporary is evaluated for.
    drop_site: Option<Span>,
    /// A31: the next value that `eval` gives is bound to a local, whose binding applies the drop
    /// hook itself (`eval_block`).
    bound_next: bool,
    /// A31: `bound_next` of the expression that `eval` is evaluating, for the block or `if` that
    /// passes it on to its result.
    tail_bound: bool,
}

/// A31: one application of the store facts of a function to arguments, for `Checker::store_memo`:
/// the function and, by argument, its expression (by address), loans, loans read through, and
/// flags.
type StoreKey = (usize, Vec<(usize, Vec<usize>, Vec<usize>, u8)>);

/// A31: an argument of a call whose store facts are applied: its expression when the call has
/// one, its value, and, for an argument of a call of a function-valued parameter that a callee's
/// facts substitute, whether it holds a loan of a local value of that callee (`local`) and whether
/// it comes from the callee's parameters (`external`).
///
/// `temporary`: the argument is a borrowed temporary (`E::BorrowOperand`: the implicit borrow of a
/// value that a read parameter of edition 2027 gets, `put (\() -> ...) (ref mut arr)` with
/// `put :: (unit -> i64) -> ...`), whose value is what the temporary holds, not a borrow of a
/// place: those loans are already the contents of the reference that the parameter gets, so a
/// fact that stores the contents (`Part::Contents`) stores them (`Checker::argument_part`). The
/// captures of a lambda and the arguments of a partial application that `apply_higher` binds
/// count as temporaries too: their value is that of the whole function value.
///
/// A fact that stores the reference itself (`Part::Value`) of a borrowed operand stores a borrow
/// of the operand: `reference` holds its loans when the operand is a place (`a == b` lends both
/// operands to a user `Eq.eq`, `Checker::argument`), and `dying` says that it is a temporary,
/// which is dropped when the statement ends, so that nothing may keep the reference.
///
/// `environment`: the argument is a capture of the function value that is called (or an argument
/// that a partial application bound), so a store into it goes into that value's environment.
#[derive(Clone)]
struct Arg<'e> {
    expression: Option<&'e TypedExpr>,
    value: Value,
    local: bool,
    external: bool,
    temporary: bool,
    dying: bool,
    reference: Option<BTreeSet<usize>>,
    environment: bool,
    /// The argument is a reference itself (its expression has a reference type, or a builtin
    /// lends it to a callback), not a value that holds one: `sink_flags` asks which function
    /// hands a value that holds a shared reference parameter to a function value.
    lent: bool,
    /// Without an expression: whether the value's type may hold a function value of one of the
    /// types that own the cell that they write, or reach a cell (`Checker::argument_hides`), as
    /// the callee that passed it on saw it. With one, its type says.
    hides: bool,
    /// Without an expression: whether the value's type may be a place that a store reaches
    /// through it (`may_reach_store`), as the callee that passed it on saw it. With one, its type
    /// says.
    target: bool,
}

impl<'e> Arg<'e> {
    fn of(expression: Option<&'e TypedExpr>, value: Value) -> Self {
        let operand = expression.and_then(|expression| match &expression.kind {
            E::BorrowOperand(inner) => Some(&**inner),
            _ => None,
        });
        Self {
            expression,
            value,
            local: false,
            external: false,
            temporary: operand.is_some(),
            dying: operand.is_some_and(|inner| !Checker::is_place(inner)),
            reference: None,
            environment: false,
            lent: expression.is_some_and(|e| matches!(e.ty, Type::Reference(..))),
            hides: true,
            target: true,
        }
    }
}

/// A31: how many passes `Checker::settle` makes over the facts of one call before it takes the
/// call for one of code that the check does not see.
const STORE_PASSES: usize = 16;

/// A31: whether a value of type `ty` reaches (through stored values, references and shared
/// pointers) a value whose type does not show what it holds or reaches: a function value (its
/// captures), a dyn value (the value that it stores) or an `Owned.Function`.
fn hides_environment(ty: &Type, types: &crate::check::TypeContext<'_>) -> bool {
    types.reaches(ty, |part| {
        matches!(part, Type::Function(..) | Type::Dyn(_)) || part.is_owned_function(types)
    })
}

/// A31: whether a value of type `ty` may be a place that a store reaches through it: it reaches a
/// mutable cell whose contents may hold a loan, an exclusive reference, or a value whose type
/// hides an environment (a function value, a dyn value, an `Owned.Function`). Code that gets a
/// value of any other type (an `i64`, a `ref string`, a `[i64]`) cannot store into what it reaches.
fn may_reach_store(ty: &Type, types: &crate::check::TypeContext<'_>) -> bool {
    let plain = without_views(ty);
    hides_environment(&plain, types)
        || types.reaches(&plain, |part| {
            part.is_mutable_cell(types) || matches!(part, Type::Reference(_, true))
        })
}

/// A31: `ty` with the target `[T..]` of each exclusive slice (C08) written as the array `[T]`:
/// the walks of `TypeContext::reaches` and `Type::carries_loans` do not look inside a view type,
/// and what a view can hold is what its elements can.
fn without_views(ty: &Type) -> Type {
    match ty {
        Type::ArrayView(element) | Type::Array(element) => {
            Type::Array(Box::new(without_views(element)))
        }
        Type::Reference(target, mutable) => {
            Type::Reference(Box::new(without_views(target)), *mutable)
        }
        Type::List(element) => Type::List(Box::new(without_views(element))),
        Type::Vec(element) => Type::Vec(Box::new(without_views(element))),
        Type::Shared(element, kind) => Type::Shared(Box::new(without_views(element)), *kind),
        Type::Tuple(elements) => Type::Tuple(elements.iter().map(without_views).collect()),
        _ => ty.clone(),
    }
}

/// How deep the substitution of the facts of function-valued arguments goes before it treats the
/// call as one of code that the analysis does not see.
const STORE_SUBSTITUTION_DEPTH: usize = 16;

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
                            builtin_wrapper: function.module == "$builtin",
                            lends_mutable: Some(self.facts.lending.no_reads(*id)),
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
                if self.facts.revoked.get(*id) == Some(&true) {
                    return false;
                }
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
        builtin_called(self.module, callee)
    }

    /// A28: the function whose call the pipeline `expression`, `x |> right`, completes, when its
    /// facts decide the loans of the result: `right` applies a function that has facts to all its
    /// arguments but the last, or is a unary function itself (`ref key |> make`).
    fn piped_callee(&self, expression: &TypedExpr, right: &TypedExpr) -> Option<usize> {
        let (callee, arguments) = piped_application(right);
        let id = direct_callee(callee)?;
        (retention_eligible(self.module, id)
            && arguments.len() + 1 == self.module.functions[id].parameters.len()
            && retention_applies(&expression.ty, &self.module.types()))
        .then_some(id)
    }

    /// The loans that `right`, the partial application of function `id` that a pipeline
    /// completes (or the unary function itself), passes to the result of the call: those of the
    /// callee and of each argument that the facts of `id` do not show to be temporary. Each
    /// argument is evaluated, and stays lent while the next ones are, as in `eval_call`.
    fn eval_piped_arguments(
        &mut self,
        id: usize,
        right: &TypedExpr,
        live: &BTreeSet<usize>,
    ) -> Result<(Value, Vec<Value>), Diagnostic> {
        let (callee, arguments) = piped_application(right);
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
                result.absorb(&value);
            }
            self.held.push(value);
        }
        let values = self.held.split_off(start + 1);
        self.held.truncate(start);
        result.regions = None;
        Ok((result, values))
    }

    /// A31: the stores of the pipeline `left |> right`, with `value` and `callee` the values of
    /// its sides (`arguments`, those of the arguments that `right` applies, when they were
    /// evaluated one by one): the facts of a named function that the pipeline completes, with the
    /// value of `right` standing for each argument that it applies when they were not, and the
    /// rule of a function value for anything else that the pipeline calls.
    #[allow(clippy::too_many_arguments)]
    fn pipe_stores(
        &mut self,
        left: &TypedExpr,
        right: &TypedExpr,
        value: &Value,
        callee: &Value,
        arguments: Option<Vec<Value>>,
        span: Span,
        live: &BTreeSet<usize>,
    ) {
        let (head, bound) = piped_application(right);
        if let Some(id) = direct_callee(head) {
            let count = self.module.functions[id].parameters.len();
            if bound.len() + 1 < count {
                return;
            }
            if bound.len() + 1 == count {
                if self.facts.stores.get(id).is_none_or(StoreFacts::is_empty) {
                    return;
                }
                let mut all: Vec<Arg<'_>> = match arguments {
                    Some(values) if values.len() == bound.len() => bound
                        .iter()
                        .zip(values)
                        .map(|(argument, value)| Arg::of(Some(argument), value))
                        .collect(),
                    _ => bound
                        .iter()
                        .map(|argument| Arg::of(Some(argument), callee.clone()))
                        .collect(),
                };
                all.push(self.argument(left, value.clone(), live));
                self.apply_facts(id, &all, span, 0);
                return;
            }
        }
        let function = Arg::of(Some(right), callee.clone());
        let piped = self.argument(left, value.clone(), live);
        self.apply_higher(&function, &[piped], span, 0);
    }

    /// A28: whether `callee` is `share`, `downgrade` or `upgrade` of `Rc` or `Arc` (or the generated
    /// function that applies one). Each makes a new owner of the block that its first argument
    /// borrows, so its result holds what the block holds, not the borrow of the owner: the owner
    /// may move or be dropped while the result lives. `get` returns the borrow itself, and other
    /// builtins may pass their references to callbacks, so they keep every argument's loans.
    fn reads_through(&self, callee: &TypedExpr) -> bool {
        reads_through(self.module, callee)
    }

    /// Whether the checked body, or a related one, lends a mutable cell (`lends_mutable`) or hands
    /// a value to code that the analysis does not see (`opaque_calls`, `opaque_bodies`): a store
    /// that the check does not track may then have put in the owner, or in a value that a lambda
    /// captured, what its loans miss, so nothing is read through the owner's borrow
    /// (`lending_bodies`).
    fn body_lends_mutable(&mut self) -> bool {
        if let Some(lends) = self.lends_mutable {
            return lends;
        }
        let lends = lends_mutable(self.body, &self.module.types(), false) || {
            let found = opaque_calls(self.body, self.module, &mut BTreeMap::new());
            found.direct
                || found
                    .callees
                    .iter()
                    .any(|callee| self.facts.lending.opaque[*callee])
        };
        self.lends_mutable = Some(lends);
        lends
    }

    /// What a value read through the borrows `loans` holds, as `read_places` reads a place: a
    /// borrow of a local holds the loans that the local held when it was borrowed (its parents);
    /// a borrow from the caller, or of static data, says nothing about what its target holds, so
    /// it stands for itself.
    ///
    /// A borrow whose target holds a loan from the caller (an external loan: the target is a
    /// parameter, a capture of a lifted lambda, or a local that took a value from one) is not read
    /// through: it stands for itself, as before A28. The store facts make the loans of this body's
    /// own values complete (A31, the argument at `lends_mutable`), and, as defense in depth, the
    /// body lends no way to write and drops no value that may store what it holds
    /// (`body_lends_mutable`, `drops_stop_reads`). An external loan stands for what the caller's
    /// value holds, which this body cannot see: in `fn wrap arr = Arc.share (ref arr[0])`, a caller
    /// whose loans missed what it stored into `arr` (as with `Array.write` before A31) passes `arr`
    /// with the loans that it held before the store. Reading through, the share held only `arr`'s
    /// external loan, the caller's result held the caller's incomplete loans, and the share
    /// outlived the borrowed value. Kept as the borrow of `arr`, the share is a borrow of a local
    /// that `wrap` drops, which the check rejects as the A30 tip does.
    ///
    /// The result holds a copy of each loan read through, with the same place, mutability and
    /// parents: it keeps the same data borrowed, and the block checks and conflicts see it as they
    /// see the original. It is not the original because a live borrow of the owner (`Rc.get (ref
    /// shared)`, `let r = ref shared`) has the original as a parent, and a value that holds a loan
    /// that some active loan reborrows cannot move; moving the new owner reborrows nothing. A
    /// borrow kept for itself stays the original, as it is without the read-through.
    fn read_through(&mut self, loans: &BTreeSet<usize>) -> BTreeSet<usize> {
        let mut kept = BTreeSet::new();
        let mut held = BTreeSet::new();
        for &loan in loans {
            let through = self.read_through_loan(loan);
            let from_caller = !self.external.contains(&self.loans[loan].place.root)
                && through
                    .iter()
                    .any(|id| self.external.contains(&self.loans[*id].place.root));
            if from_caller {
                // A31: the borrow of the owner stands for a share of its block (`Loan::share`).
                kept.extend(self.share_loans(&BTreeSet::from([loan])));
            } else {
                held.extend(through);
            }
        }
        let copies: Vec<usize> = held
            .into_iter()
            .map(|id| {
                let copy = self.loans[id].clone();
                self.loans.push(copy);
                self.loans.len() - 1
            })
            .collect();
        kept.extend(copies);
        kept
    }

    /// A31: copies of the loans `loans` that stand for shares of the blocks that their places
    /// own (`Loan::share`): what the result of `Rc.share (ref owner)` and its kin holds when the
    /// check does not read through the borrow of the owner. A copy keeps the same data borrowed,
    /// as the original does, and has it as its parent.
    fn share_loans(&mut self, loans: &BTreeSet<usize>) -> BTreeSet<usize> {
        loans
            .iter()
            .map(|&id| {
                let root = self.loans[id].place.root;
                if is_static_root(root) || self.external.contains(&root) || self.loans[id].mutable {
                    return id;
                }
                let mut copy = self.loans[id].clone();
                copy.share = true;
                copy.parents.insert(id);
                self.loans.push(copy);
                self.loans.len() - 1
            })
            .collect()
    }

    /// The loans that the target of the borrow `loan` holds (`read_through`).
    fn read_through_loan(&self, loan: usize) -> BTreeSet<usize> {
        let mut held = BTreeSet::new();
        let mut pending = vec![loan];
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
        held
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

    /// A parameter of the generated function that applies a builtin to its own parameters
    /// (`$builtin`): it passes on what its callers gave it, and they are checked. Other generated
    /// locals, such as the result of `do!`, hold what the program put in them (A30).
    fn is_builtin_wrapper_parameter(&self, argument: &TypedExpr) -> bool {
        self.builtin_wrapper && self.is_generated_parameter(argument)
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
            share: false,
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
                through: value.through.clone(),
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
        let mut projected = if maps.len() >= 2 && maps.len() == self.region_count(ty) {
            let regions: Box<[_]> = maps.iter().map(|mask| loans(*mask)).collect();
            Value {
                loans: regions.iter().flatten().copied().collect(),
                regions: Some(regions),
                closed_result: value.closed_result,
                through: BTreeSet::new(),
            }
        } else {
            Value {
                loans: loans(maps.iter().fold(0, |all, mask| all | mask)),
                regions: None,
                closed_result: value.closed_result,
                through: BTreeSet::new(),
            }
        };
        projected.through = value
            .through
            .intersection(&projected.loans)
            .copied()
            .collect();
        projected
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
                    through: stored.through.clone(),
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
            result.absorb(&value);
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
                current.hold(loans);
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
                let root = self.loans[id].place.root;
                pending.extend(&self.loans[id].parents);
                // A31: a local that stays borrowed keeps what it holds now, not only what it
                // held when the borrow was made: a store through a shared borrow of a cell adds
                // loans to it afterwards, and those keep their own roots alive in turn.
                if keep.insert(root)
                    && let Some((_, value)) = self.state.locals.get(&root)
                {
                    pending.extend(&value.loans);
                }
            }
        }
        for (id, (_, value)) in &mut self.state.locals {
            if !keep.contains(id) {
                value.clear_loans();
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
        let Some((local, owner)) = self.state.locals.get_mut(&place.root) else {
            return Err(error(DiagnosticKind::StoreRegionUnnamed, UNNAMED, span));
        };
        let ty = local.ty.clone();
        owner.absorb(value);
        owner.regions = None;
        for (owned, stored) in owner.closed_result.iter_mut().zip(value.closed_result) {
            *owned &= stored;
        }
        // A31: an owner that may run code when it is dropped now holds what was stored (A28's
        // `dropped`).
        if self.dropped.is_some() && !value.loans.is_empty() && self.drop_runs(&ty) {
            self.dropped
                .as_mut()
                .expect("checked above")
                .extend(&value.loans);
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

    /// A31: whether store facts apply in this check: the specialized bodies, whose types are
    /// concrete and whose lambdas are lifted, of a module that has a seed (`CallFacts::seeded`).
    /// The abstract bodies of the inference keep A28's guard where they lend a way to write and
    /// may store (`lending_bodies`).
    fn tracks_stores(&self) -> bool {
        !self.infer && self.facts.seeded
    }

    /// A31: whether a value of type `ty` reaches a mutable cell whose contents may hold a loan
    /// (`Type::is_mutable_cell`), through stored values, references and shared pointers.
    fn reaches_cell(&mut self, ty: &Type) -> bool {
        if let Some(known) = self.cell_types.get(ty) {
            return *known;
        }
        let types = self.module.types();
        let found = types.reaches(&without_views(ty), |part| part.is_mutable_cell(&types));
        self.cell_types.insert(ty.clone(), found);
        found
    }

    /// A31: what the type `ty` says about the stores that reach a value of it, cached by type:
    ///
    /// - `hides`: it reaches (through stored values, references and shared pointers) a value
    ///   whose type does not show what it holds or reaches: a function value, a dyn value or an
    ///   `Owned.Function` (`hides_environment`);
    /// - `holds`: a place of the type can hold a stored loan: the type carries loans, hides an
    ///   environment or reaches a cell whose contents may hold one (a `[i64]` or an `i64` cannot);
    /// - `dynamic`: it reaches a dyn value, whose stored value a method gets through its receiver.
    fn hidden_type(&mut self, ty: &Type) -> (bool, bool, bool) {
        if let Some(known) = self.hidden_types.get(ty) {
            return *known;
        }
        let types = self.module.types();
        let plain = without_views(ty);
        let hides = hides_environment(&plain, &types);
        let dynamic = types.reaches(&plain, |part| matches!(part, Type::Dyn(_)));
        let holds = hides || plain.carries_loans(&types) || self.reaches_cell(ty);
        self.hidden_types
            .insert(ty.clone(), (hides, holds, dynamic));
        (hides, holds, dynamic)
    }

    /// A31: whether a value of type `ty` may share, with values that hold no loan of it, the cell
    /// that a function value that owns it writes (`store_hidden`): it may hold such a function
    /// value (a type of `CallFacts::owned_types`, a dyn value when one is, an `Owned.Function`),
    /// or reaches a cell (`argument_type`). A closure that captured a `ref string`, and a function
    /// of a type that no such owner has, share nothing.
    fn shares_hidden(&mut self, ty: &Type) -> bool {
        self.argument_type(ty).1
    }

    /// A31: whether a store through the borrow `loan` reaches what its place holds. An exclusive
    /// borrow does when a place of the type that it borrows can hold a stored loan (`ref mut
    /// [i64..]` and `ref mut i64` cannot). A shared borrow does when the type of its place reaches
    /// a mutable cell, a dyn value whose methods may store or give back a place to store into
    /// (`CallFacts::dyn_target`), or, in a module where some function value stores into what its
    /// environment reaches (`CallFacts::sinks`), a function value or an `Owned.Function`. The
    /// type of a local root is its declaration's and that of an external root its parameter's
    /// (the target of a reference parameter, for an exclusive borrow); a root without a
    /// declaration counts as reaching one.
    fn store_target(&mut self, loan: usize) -> bool {
        let root = self.loans[loan].place.root;
        if is_static_root(root) {
            return false;
        }
        let external = self.external.contains(&root);
        let id = if external { usize::MAX - root } else { root };
        let Some(ty) = self.declarations.get(&id).map(|local| local.ty.clone()) else {
            return true;
        };
        if self.loans[loan].mutable {
            let place = match ty {
                Type::Reference(target, _) if external => *target,
                ty => ty,
            };
            return self.hidden_type(&place).1;
        }
        if self.reaches_cell(&ty) {
            return true;
        }
        let (hides, _, dynamic) = self.hidden_type(&ty);
        if !hides {
            return false;
        }
        if let Some(known) = self.shared_targets.get(&ty) {
            return *known;
        }
        let types = self.module.types();
        let facts: &CallFacts = self.facts;
        let plain = without_views(&ty);
        // A dyn value is reached through the methods of its vtables (`CallFacts::dyn_targets`);
        // a function value of a type that may store into what its environment reaches
        // (`CallFacts::sink_types`), or an `Owned.Function`, in a module where some function value
        // does.
        let found = (dynamic
            && types.reaches(
                &plain,
                |part| matches!(part, Type::Dyn(dyn_type) if facts.dyn_target(dyn_type)),
            ))
            || (facts.sinks
                && types.reaches(&plain, |part| match part {
                    Type::Function(..) => owned_suffix(part, usize::MAX, &facts.sink_types),
                    _ => part.is_owned_function(&types),
                }));
        self.shared_targets.insert(ty, found);
        found
    }

    /// A31: whether a store where `target` reaches stores anywhere (`store_target`).
    fn has_targets(&mut self, target: &Value) -> bool {
        let loans: Vec<usize> = target.loans.iter().copied().collect();
        loans.into_iter().any(|loan| self.store_target(loan))
    }

    /// A31: the first deferred error of the body (`deferred`).
    fn defer(&mut self, diagnostic: Diagnostic) {
        if self.deferred.is_empty() {
            self.deferred.push(diagnostic);
            self.store_changes += 1;
        }
    }

    #[cold]
    fn local_borrow_stored(span: Span) -> Diagnostic {
        error(
            DiagnosticKind::StoreLocalBorrowThroughReference,
            "cannot store a borrow of a local value through a reference parameter; the caller's data would outlive it",
            span,
        )
    }

    /// A31: the error of a call of a callee whose body passes a borrow of its own local value to
    /// the function value that the call hands it, when that function value may keep the borrow
    /// where the caller's data reaches: one whose body the check does not see (`seen` false), or
    /// one whose facts store it there. The local value is the callee's, so the error names the
    /// callee (`callee`, the named function of the call) rather than a local of this body.
    #[cold]
    fn callee_borrow_stored(&self, callee: Option<usize>, span: Span, seen: bool) -> Diagnostic {
        // The declared name: an instance of a generic function is `leak.$mono.2`.
        let name = callee.map(|id| {
            let name = self.module.functions[id].name.as_str();
            name.split(".$mono.").next().unwrap_or(name)
        });
        let subject = match name {
            Some(name) => format!("'{name}'"),
            None => "the callee".to_owned(),
        };
        let message = if seen {
            format!(
                "cannot pass this function value here: {subject} passes it a borrow of a local value of its own, and the function value stores what it is given where the caller's data would outlive that value"
            )
        } else {
            format!(
                "cannot pass this function value here: {subject} passes it a borrow of a local value of its own, and a function value whose body the check does not see may keep what it is given where the caller's data would outlive that value"
            )
        };
        error(DiagnosticKind::StoreLocalBorrowThroughReference, message, span).with_help(
            if seen {
                "pass a function value that does not store its argument, or make the callee pass an owned value"
            } else {
                "pass a lambda or a named function directly, so that the check sees what it stores, or make the callee pass an owned value"
            },
        )
    }

    /// A31: the error of a call that stores a reference to a temporary argument: the callee keeps
    /// the reference that a read parameter or an operator gets (not what the value holds) where
    /// one of its arguments reaches, and the temporary is dropped when the statement ends.
    #[cold]
    fn temporary_borrow_stored(span: Span) -> Diagnostic {
        error(
            DiagnosticKind::StoreLocalBorrowThroughReference,
            "cannot store a borrow of a temporary value: the call keeps a reference to this argument where another argument reaches, and the temporary is dropped at the end of the statement",
            span,
        )
        .with_help("bind the value with 'let' before the call, in a scope that outlives the place that keeps the reference")
    }

    /// A31: adds what `kept` holds to the local owner `root`, as an assignment into it does.
    fn absorb_owner(&mut self, root: usize, kept: &Value) {
        if kept.loans.is_empty() {
            return;
        }
        let Some((local, owner)) = self.state.locals.get_mut(&root) else {
            return;
        };
        let ty = local.ty.clone();
        let before = (owner.loans.len(), owner.through.len());
        owner.absorb(kept);
        if before != (owner.loans.len(), owner.through.len()) {
            self.store_changes += 1;
        }
        owner.regions = None;
        for (owned, stored) in owner.closed_result.iter_mut().zip(kept.closed_result) {
            *owned &= stored;
        }
        if self.dropped.is_some() && self.drop_runs(&ty) {
            let loans = kept.loans.clone();
            self.dropped.as_mut().expect("checked above").extend(loans);
        }
    }

    /// A31: stores a value that holds `stored`'s loans where `target` reaches, as `arr[i] = v`
    /// does: the owner of each local place that a target loan (`store_target`) borrows keeps the
    /// loans, and so do the places that its own target loans borrow (the array behind a
    /// `MatrixView.Mut` window, the block that several `Arc`s share). A target of the caller's
    /// (an external root) becomes a store fact of the body, unless what is stored borrows a local
    /// value of the body, which is an error: the caller's data would outlive it.
    fn apply_store(&mut self, stored: &Value, target: &Value, span: Span) {
        self.apply_store_within(stored, target, span, false);
    }

    /// `apply_store`; with `local_only`, the targets of the caller's (the external roots) are
    /// left out: the caller substitutes the code that stores there.
    fn apply_store_within(&mut self, stored: &Value, target: &Value, span: Span, local_only: bool) {
        if stored.loans.is_empty() || target.loans.is_empty() {
            return;
        }
        // The places that the store reaches: (root, whether through exclusive borrows only).
        let mut pending: Vec<(usize, bool)> = Vec::new();
        for &loan in &target.loans {
            if self.store_target(loan) {
                pending.push((loan, self.loans[loan].mutable));
            }
        }
        let mut seen = BTreeSet::new();
        let mut reached: Vec<(usize, bool)> = Vec::new();
        // The external roots, and whether only exclusive borrows reach each.
        let mut external: BTreeMap<usize, bool> = BTreeMap::new();
        while let Some((loan, exclusive)) = pending.pop() {
            if !seen.insert((loan, exclusive)) {
                continue;
            }
            let root = self.loans[loan].place.root;
            if is_static_root(root) {
                continue;
            }
            if self.external.contains(&root) {
                *external.entry(root).or_insert(true) &= exclusive;
                continue;
            }
            if reached.contains(&(root, exclusive)) {
                continue;
            }
            reached.push((root, exclusive));
            let Some((_, owner)) = self.state.locals.get(&root) else {
                continue;
            };
            let held: Vec<usize> = owner.loans.iter().copied().collect();
            for loan in held {
                if self.store_target(loan) {
                    let mutable = self.loans[loan].mutable;
                    pending.push((loan, exclusive && mutable));
                }
            }
        }
        if !local_only {
            for (root, exclusive) in external {
                self.store_external(root, exclusive, stored, span);
            }
        }
        // Through exclusive borrows, a loan of a place on the way can only be one of those
        // borrows or a reborrow of one (any other borrow of the place would conflict with them):
        // storing it writes back what the place holds. Through a shared borrow of a cell, it is a
        // borrow of the cell kept inside it, which the owner keeps like any other.
        let exclusive_roots: BTreeSet<usize> = reached
            .iter()
            .filter(|(_, exclusive)| *exclusive)
            .map(|(root, _)| *root)
            .collect();
        let mut through_exclusive = stored.clone();
        {
            let loans = &self.loans;
            through_exclusive
                .loans
                .retain(|id| !exclusive_roots.contains(&loans[*id].place.root));
            let kept = &through_exclusive.loans;
            through_exclusive.through.retain(|id| kept.contains(id));
        }
        for (root, exclusive) in reached {
            let mut kept = if exclusive {
                through_exclusive.clone()
            } else {
                stored.clone()
            };
            // The loan that the result of `Rc.share (ref owner)` or `Arc.share (ref owner)` holds
            // in place of what the block holds (`Loan::share`; A28 did not read it through) is no
            // pointer to the owner: kept in the block that the owner shares, it is a cycle that
            // keeps the block alive, which moving the owner does not invalidate. A real borrow
            // of the owner (`ref owner`) stays: it points at the owner's own storage.
            if !exclusive {
                let loans = &self.loans;
                kept.loans
                    .retain(|id| !(loans[*id].share && loans[*id].place.root == root));
                let held = &kept.loans;
                kept.through.retain(|id| held.contains(id));
            }
            self.absorb_owner(root, &kept);
        }
    }

    /// A31: a store of `stored` where the parameter with external root `root` reaches: a store
    /// fact from each parameter whose loan it holds, and the error of a borrow of a local value.
    /// A store of the contents of a parameter's own target into that target, through exclusive
    /// borrows only (`exclusive`), writes back what the caller's value holds (`rotate`, `sort`),
    /// which is no fact. Through a cell it is one: the cell may be shared with other owners
    /// (an `Arc<Mutex<...>>` in the value), which do not hold what the value holds. A share of a
    /// local `Rc` or `Arc` owner (`Loan::share`) is an owner of the block, not a borrow of the
    /// local: it stores what the local holds.
    fn store_external(&mut self, root: usize, exclusive: bool, stored: &Value, span: Span) {
        let Some(&to) = self.parameter_roots.get(&root) else {
            return;
        };
        let mut pending: Vec<(usize, bool)> = stored
            .loans
            .iter()
            .map(|loan| (*loan, stored.through.contains(loan)))
            .collect();
        let mut seen = BTreeSet::new();
        while let Some((loan, through)) = pending.pop() {
            if !seen.insert(loan) {
                continue;
            }
            let source = self.loans[loan].place.root;
            if is_static_root(source) {
                continue;
            }
            if let Some(&from) = self.parameter_roots.get(&source) {
                let part = if through { Part::Contents } else { Part::Value };
                if exclusive && from == to && part == Part::Contents {
                    continue;
                }
                if let Some(stores) = self.stores.as_mut()
                    && stores.stores.insert((from, part, to))
                {
                    self.store_changes += 1;
                }
            } else if !self.external.contains(&source) {
                if self.loans[loan].share
                    && let Some((_, owner)) = self.state.locals.get(&source)
                {
                    pending.extend(
                        owner
                            .loans
                            .iter()
                            .map(|held| (*held, owner.through.contains(held))),
                    );
                    continue;
                }
                self.defer(Self::local_borrow_stored(span));
                return;
            }
        }
    }

    /// A31: a store of `stored` in an environment that the check cannot name: that of a function
    /// value, a dyn value or an `Owned.Function` that owns the cell that its code writes (a
    /// lambda that captured an `Arc<Mutex<...>>` by value), in a module that makes such a value
    /// (`CallFacts::owned_sinks`). The value may be a copy, and a copy shares the cell without
    /// holding a loan of anything, so every value that may share it holds what was stored: every
    /// local whose type may hold such a function value or reaches a cell (`shares_hidden`), the values bound
    /// later that were made before (`hidden_log`), and the caller's values of such types (a fact of
    /// the body, `StoreFacts::hidden`, when a parameter has such a type). A borrow of a local value
    /// of the body cannot go where the caller's data reaches.
    fn store_hidden(&mut self, stored: &Value, span: Span) {
        if stored.loans.is_empty() || !self.facts.owned_sinks {
            return;
        }
        // A value that may share the environment is handed, somewhere in the module, to code
        // that runs it later: it was checked to hold no borrow there, and no check follows it.
        if self.facts.escapes
            && stored
                .loans
                .iter()
                .any(|loan| !is_static_root(self.loans[*loan].place.root))
        {
            self.defer(error(
                DiagnosticKind::TaskCaptureRetainsBorrow,
                "cannot store a borrow in the environment of a function value here: this program hands a function value that may share that environment to a tzroutine, a channel or other code that runs it later, which may use the borrow after the borrowed value is gone",
                span,
            ).with_help("store an owned value, or a function that captures a copy"));
        }
        let kept = Value {
            loans: stored.loans.clone(),
            through: stored.through.clone(),
            ..Value::default()
        };
        if !self
            .hidden_log
            .iter()
            .any(|logged| kept.loans.is_subset(&logged.loans))
        {
            self.hidden_log.push(kept.clone());
            self.store_changes += 1;
        }
        // A parameter keeps only the caller's loans here: a loan of a local value of the body
        // that reaches the caller's values is the error below, which names what happened.
        let outside = {
            let loans = &self.loans;
            let external = &self.external;
            let mut outside = kept.clone();
            outside.loans.retain(|loan| {
                let root = loans[*loan].place.root;
                is_static_root(root) || external.contains(&root)
            });
            let held = &outside.loans;
            outside.through.retain(|loan| held.contains(loan));
            outside
        };
        let ids: Vec<usize> = self.state.locals.keys().copied().collect();
        for id in ids {
            let local = self.state.locals[&id].0;
            if !self.shares_hidden(&local.ty) {
                continue;
            }
            if self.parameter_roots.contains_key(&(usize::MAX - id)) {
                self.absorb_owner(id, &outside);
            } else {
                self.absorb_owner(id, &kept);
            }
        }
        let parameters: Vec<Type> = self
            .parameter_roots
            .keys()
            .filter_map(|root| self.declarations.get(&(usize::MAX - *root)))
            .map(|local| local.ty.clone())
            .collect();
        if !parameters.into_iter().any(|ty| self.shares_hidden(&ty)) {
            return;
        }
        let mut pending: Vec<(usize, bool)> = kept
            .loans
            .iter()
            .map(|loan| (*loan, kept.through.contains(loan)))
            .collect();
        let mut seen = BTreeSet::new();
        while let Some((loan, through)) = pending.pop() {
            if !seen.insert(loan) {
                continue;
            }
            let source = self.loans[loan].place.root;
            if is_static_root(source) {
                continue;
            }
            if let Some(&from) = self.parameter_roots.get(&source) {
                let part = if through { Part::Contents } else { Part::Value };
                if let Some(stores) = self.stores.as_mut()
                    && stores.hidden.insert((from, part))
                {
                    self.store_changes += 1;
                }
            } else if !self.external.contains(&source) {
                if self.loans[loan].share
                    && let Some((_, owner)) = self.state.locals.get(&source)
                {
                    pending.extend(
                        owner
                            .loans
                            .iter()
                            .map(|held| (*held, owner.through.contains(held))),
                    );
                    continue;
                }
                self.defer(Self::local_borrow_stored(span));
                return;
            }
        }
    }

    /// A31: gives `value`, which a local of type `ty` is bound to (or which the body returns),
    /// what was stored in hidden environments since `mark` (`store_hidden`): the value may be a
    /// copy, made before the store, of a value that shares such an environment.
    fn absorb_hidden(&mut self, mark: usize, value: &mut Value, ty: &Type) {
        if mark >= self.hidden_log.len() || !self.shares_hidden(ty) {
            return;
        }
        for logged in &self.hidden_log[mark..] {
            value.absorb(logged);
        }
    }

    /// A31: the value of the borrows inside the targets of the references `references`: what a
    /// local owner holds, the target loans of a parameter's slot with a named target region, and a
    /// parameter's own loan as what it reads through (`Value::through`).
    fn contents_value(&self, references: &Value) -> Value {
        let mut value = Value::default();
        for &id in &references.loans {
            let place = &self.loans[id].place;
            if let Some((_, owner)) = self.state.locals.get(&place.root) {
                value.absorb(&Value {
                    loans: owner.loans.clone(),
                    through: owner.through.clone(),
                    ..Value::default()
                });
            } else if let Some(slot) = self
                .slots
                .get(&(place.root, place_slot(place)))
                .filter(|slot| !slot.target.is_empty())
            {
                value.hold(slot.target.iter().copied());
            } else {
                value.absorb(&Value {
                    loans: BTreeSet::from([id]),
                    through: BTreeSet::from([id]),
                    ..Value::default()
                });
            }
        }
        value
    }

    fn part_value(&self, value: &Value, part: Part) -> Value {
        match part {
            Part::Value => Value {
                loans: value.loans.clone(),
                through: value.through.clone(),
                ..Value::default()
            },
            Part::Contents => self.contents_value(value),
        }
    }

    /// A31: what a fact's `part` of `argument` stores. The contents of a borrowed temporary
    /// (`Arg::temporary`) are its value: its loans are what the temporary holds, which is what the
    /// reference that the parameter gets points at. The targets of those loans are kept as well,
    /// for a temporary that `apply_higher` approximates by a whole function value. The value of
    /// a borrowed operand that is a place is the borrow of the place (`Arg::reference`), which the
    /// operand's own value, what the place holds, does not show.
    fn argument_part(&self, argument: &Arg<'_>, part: Part) -> Value {
        let mut value = self.part_value(&argument.value, part);
        if argument.temporary && part == Part::Contents {
            value.absorb(&self.part_value(&argument.value, Part::Value));
        }
        if part == Part::Value
            && let Some(reference) = &argument.reference
        {
            value.hold(reference.iter().copied());
        }
        value
    }

    /// A31: what code that the check does not see may keep of `argument`: its value, and the
    /// borrow of a borrowed operand that is a place (`Arg::reference`) in a module where some
    /// function keeps a shared reference that it gets (`CallFacts::reference_sinks`). In any other
    /// module the code, which is a function of the module, keeps at most what the operand holds.
    fn handed(&self, argument: &Arg<'_>) -> Value {
        if self.facts.reference_sinks {
            self.argument_part(argument, Part::Value)
        } else {
            self.part_value(&argument.value, Part::Value)
        }
    }

    /// A31: what the type `ty` of an argument says (`argument_types`): whether a store may reach
    /// through a value of it (`may_reach_store`), and whether it may hold a function value that
    /// may own the cell that it writes, or reaches a cell (`argument_hides`).
    fn argument_type(&mut self, ty: &Type) -> (bool, bool) {
        if let Some(known) = self.argument_types.get(ty) {
            return *known;
        }
        let types = self.module.types();
        let target = may_reach_store(ty, &types);
        let facts: &CallFacts = self.facts;
        let dynamic = facts
            .owned_types
            .iter()
            .any(|owned| matches!(owned, Type::Dyn(_)));
        let plain = without_views(ty);
        let hides = target
            && (types.reaches(&plain, |part| match part {
                Type::Function(..) => owned_suffix(part, usize::MAX, &facts.owned_types),
                Type::Dyn(_) => dynamic,
                _ => part.is_owned_function(&types),
            }) || self.reaches_cell(ty));
        self.argument_types.insert(ty.clone(), (target, hides));
        (target, hides)
    }

    /// A31: whether the value of `argument` may hold a function value of one of the types that
    /// may own the cell that they write (`CallFacts::owned_types`, or a type whose function after
    /// some arguments is one), a dyn value when a dyn type is one of them, or an `Owned.Function`,
    /// or reaches a cell. Unseen code that gets the argument reaches an environment that the check
    /// cannot name only through such a value: a closure that captured a `ref string` and a
    /// function of a type that no owner has are neither.
    fn argument_hides(&mut self, argument: &Arg<'_>) -> bool {
        match argument.expression {
            Some(expression) => self.argument_type(&expression.ty).1,
            None => argument.hides,
        }
    }

    /// A31: whether a store may reach through the value of `argument` (`may_reach_store`).
    fn argument_target(&mut self, argument: &Arg<'_>) -> bool {
        match argument.expression {
            Some(expression) => self.argument_type(&expression.ty).0,
            None => argument.target,
        }
    }

    /// A31: whether a call of `callee` with `arguments` arguments may run a function value that
    /// owns the cell that it writes, or reach one through its environment: its type, or the type
    /// of the function that it gives after some of the arguments, is one of
    /// `CallFacts::owned_types`. A value that is not of a function type (an `Owned.Function`) may.
    /// A value without an expression (an argument that a callee's facts pass on) may when the
    /// callee saw that its type may (`Arg::hides`).
    fn may_own_sink(&self, callee: &Arg<'_>, arguments: usize) -> bool {
        let Some(expression) = callee.expression else {
            return callee.hides;
        };
        let mut ty = &expression.ty;
        while let Type::Reference(target, _) = ty {
            ty = target;
        }
        if !matches!(ty, Type::Function(..)) {
            return true;
        }
        owned_suffix(ty, arguments, &self.facts.owned_types)
    }

    /// A31: whether a call of `callee` with `arguments` arguments may run code that stores into
    /// what the function value's environment reaches: in a module where some function value does
    /// (`CallFacts::sinks`), when its type, or the type of the function that it gives after some of
    /// the arguments, is one of `CallFacts::sink_types`. A value that is not of a function type
    /// (an `Owned.Function`), or whose expression is not known here, may.
    fn may_be_sink(&self, callee: &Arg<'_>, arguments: usize) -> bool {
        if !self.facts.sinks {
            return false;
        }
        let Some(expression) = callee.expression else {
            return true;
        };
        let mut ty = &expression.ty;
        while let Type::Reference(target, _) = ty {
            ty = target;
        }
        !matches!(ty, Type::Function(..)) || owned_suffix(ty, arguments, &self.facts.sink_types)
    }

    /// A31: where the loans of `value` come from, for a fact of a call of a function-valued
    /// parameter: the parameters (or the contents of their targets) and the local values.
    fn source_of(&self, value: &Value) -> Source {
        let mut source = Source::default();
        for &loan in &value.loans {
            let root = self.loans[loan].place.root;
            if is_static_root(root) {
                continue;
            }
            match self.parameter_roots.get(&root) {
                Some(&index) => {
                    let part = if value.through.contains(&loan) {
                        Part::Contents
                    } else {
                        Part::Value
                    };
                    source.parameters.insert((index, part));
                }
                None => source.local = true,
            }
        }
        source
    }

    /// A31: the key of an application of the facts of `id` to `arguments` (`store_memo`).
    fn store_key(id: usize, arguments: &[Arg<'_>]) -> StoreKey {
        (
            id,
            arguments
                .iter()
                .map(|argument| {
                    let mut loans: Vec<usize> = argument.value.loans.iter().copied().collect();
                    if let Some(reference) = &argument.reference {
                        loans.push(usize::MAX);
                        loans.extend(reference.iter().copied());
                    }
                    (
                        argument
                            .expression
                            .map_or(0, |expression| std::ptr::from_ref(expression) as usize),
                        loans,
                        argument.value.through.iter().copied().collect(),
                        u8::from(argument.local)
                            | u8::from(argument.external) << 1
                            | u8::from(argument.temporary) << 2
                            | u8::from(argument.dying) << 3
                            | u8::from(argument.environment) << 4
                            | u8::from(argument.lent) << 5,
                    )
                })
                .collect(),
        )
    }

    /// A31: runs `apply`, which applies store facts to a call, until a pass changes nothing: a
    /// store of one fact can widen what the target of another reaches (a reference stored into a
    /// cell, then a value stored through it), and a callee runs its stores in any order and any
    /// number of times. Within a pass, an application of the same facts to the same arguments is
    /// made once (`store_memo`), which keeps the substitution of nested function values linear in
    /// the distinct applications. Past the limit the call counts as one of code that the check
    /// does not see (`fallback`).
    fn settle(&mut self, mut apply: impl FnMut(&mut Self), fallback: impl FnOnce(&mut Self)) {
        if self.store_memo.is_some() {
            apply(self);
            return;
        }
        let mut settled = false;
        for _ in 0..STORE_PASSES {
            self.store_memo = Some(BTreeSet::new());
            let before = self.store_changes;
            apply(self);
            if self.store_changes == before {
                settled = true;
                break;
            }
        }
        self.store_memo = None;
        if !settled {
            fallback(self);
        }
    }

    /// A31: applies the store facts of function `id` to a complete call of it with `arguments`.
    fn apply_facts<'e>(&mut self, id: usize, arguments: &[Arg<'e>], span: Span, depth: usize)
    where
        'a: 'e,
    {
        self.settle(
            |checker| checker.apply_facts_once(id, arguments, span, depth, id),
            |checker| checker.apply_opaque(None, arguments, span, false, Some(id)),
        );
    }

    /// One pass of `apply_facts`. `named` is the named callee of the call in the source, for the
    /// errors that name it.
    fn apply_facts_once<'e>(
        &mut self,
        id: usize,
        arguments: &[Arg<'e>],
        span: Span,
        depth: usize,
        named: usize,
    ) where
        'a: 'e,
    {
        let facts: &CallFacts = self.facts;
        let Some(stores) = facts.stores.get(id) else {
            return;
        };
        if stores.is_empty() {
            return;
        }
        let key = Self::store_key(id, arguments);
        if let Some(memo) = self.store_memo.as_mut()
            && !memo.insert(key)
        {
            return;
        }
        if stores.top {
            // A body that does not check is reported there.
            if facts.failed.get(id) != Some(&true) {
                self.apply_opaque(None, arguments, span, false, Some(named));
            }
            return;
        }
        for &(from, part, to) in &stores.stores {
            let (Some(source), Some(target)) = (arguments.get(from), arguments.get(to)) else {
                self.apply_opaque(None, arguments, span, false, Some(named));
                return;
            };
            if source.local && target.external {
                self.defer(self.callee_borrow_stored(Some(named), span, true));
            }
            let stored = self.argument_part(source, part);
            let reached = target.value.clone();
            if part == Part::Value
                && source.dying
                && (self.has_targets(&reached) || target.environment)
            {
                self.defer(Self::temporary_borrow_stored(span));
            }
            self.apply_store(&stored, &reached, span);
            // A store into a capture of a function value goes into its environment, which its
            // copies share when the capture owns the cell.
            if target.environment
                && target
                    .expression
                    .is_some_and(|capture| self.shares_hidden(&capture.ty))
            {
                self.store_hidden(&stored, span);
            }
        }
        for &(from, part) in &stores.hidden {
            let Some(source) = arguments.get(from) else {
                self.apply_opaque(None, arguments, span, false, Some(named));
                return;
            };
            if source.local {
                self.defer(self.callee_borrow_stored(Some(named), span, false));
            }
            if part == Part::Value
                && source.dying
                && self.facts.reference_sinks
                && self.facts.owned_sinks
            {
                self.defer(Self::temporary_borrow_stored(span));
            }
            let stored = if part == Part::Value {
                self.handed(source)
            } else {
                self.argument_part(source, part)
            };
            self.store_hidden(&stored, span);
        }
        for call in &stores.calls {
            let Some(callee) = arguments.get(call.callee) else {
                self.apply_opaque(None, arguments, span, false, Some(named));
                return;
            };
            let mut sources = Vec::with_capacity(call.arguments.len());
            for source in &call.arguments {
                let mut value = Value::default();
                let mut local = source.local;
                let mut external = false;
                let mut dying = source.dying;
                // The value passed is at the level of a temporary's contents when the callee
                // passed a temporary, or passed on the value of an argument that is one.
                let mut temporary = source.temporary;
                for &(index, part) in &source.parameters {
                    if let Some(argument) = arguments.get(index) {
                        value.absorb(&self.argument_part(argument, part));
                        local |= argument.local;
                        external |= argument.external || argument.expression.is_some();
                        temporary |= argument.temporary && part == Part::Value;
                        dying |= argument.dying && part == Part::Value;
                    }
                }
                sources.push(Arg {
                    expression: None,
                    value,
                    local,
                    external,
                    temporary,
                    dying,
                    reference: None,
                    environment: false,
                    lent: source.lent,
                    hides: source.hides,
                    target: source.target,
                });
            }
            let callee = callee.clone();
            self.apply_higher_once(&callee, &sources, span, depth + 1, Some(named));
        }
    }

    /// A31: a call of the function value `callee` with `arguments`: the facts of the lambda,
    /// named function or partial application that it is, substituted; those of the expressions
    /// that an immutable local is bound to (`known`); a record of the call in this body's facts
    /// when it is a parameter of the body (and the conservative rule for its local targets); the
    /// conservative rule for any other value (`apply_opaque`).
    fn apply_higher<'e>(
        &mut self,
        callee: &Arg<'e>,
        arguments: &[Arg<'e>],
        span: Span,
        depth: usize,
    ) where
        'a: 'e,
    {
        self.settle(
            |checker| checker.apply_higher_once(callee, arguments, span, depth, None),
            |checker| checker.apply_opaque(Some(callee), arguments, span, false, None),
        );
    }

    /// One pass of `apply_higher`.
    fn apply_higher_once<'e>(
        &mut self,
        callee: &Arg<'e>,
        arguments: &[Arg<'e>],
        span: Span,
        depth: usize,
        named: Option<usize>,
    ) where
        'a: 'e,
    {
        let module = self.module;
        if depth > STORE_SUBSTITUTION_DEPTH {
            self.apply_opaque(Some(callee), arguments, span, false, named);
            return;
        }
        // The captures of a lambda and the arguments of a partial application come from where the
        // function value was made: outside any body whose locals a substitution tracks.
        let bound = |expressions: &'e [TypedExpr], value: &Value| -> Vec<Arg<'e>> {
            expressions
                .iter()
                .map(|expression| Arg {
                    expression: Some(expression),
                    value: value.clone(),
                    local: callee.local,
                    external: true,
                    // A bound value that was lent as a borrowed temporary is one here too. Any
                    // other is the value that the parameter gets: its contents are those of
                    // the whole function value's loans (`contents_value`), so a reference that
                    // a lambda captured keeps the level of what it points at.
                    temporary: matches!(expression.kind, E::BorrowOperand(_)),
                    dying: false,
                    reference: None,
                    environment: true,
                    lent: matches!(expression.ty, Type::Reference(..)),
                    hides: true,
                    target: true,
                })
                .collect()
        };
        // A function value lent to a read parameter is a borrowed temporary whose value is the
        // function value's own.
        let expression = callee.expression.map(|expression| match &expression.kind {
            E::BorrowOperand(inner) => &**inner,
            _ => expression,
        });
        // A shared borrow of a local whose code the body shows (`Owned.call (ref w) x`, `apply
        // (ref w) f`) calls that code with what the local holds.
        if let Some(E::Borrow(inner, false)) = expression.map(|e| &e.kind)
            && let E::Local(id) = &inner.kind
            && self.known.contains_key(id)
        {
            let lent = Arg {
                expression: Some(&**inner),
                value: self.contents_value(&callee.value),
                ..callee.clone()
            };
            self.apply_higher_once(&lent, arguments, span, depth + 1, named);
            return;
        }
        // An element or a field of an immutable local bound to a literal of such values.
        let part = expression.and_then(|expression| match &expression.kind {
            E::Index(base, _) => match &base.kind {
                E::Local(id) => self.known_parts.get(&(*id, None)),
                _ => None,
            },
            E::Field(base, field) => match &base.kind {
                E::Local(id) => self.known_parts.get(&(*id, Some(*field))),
                _ => None,
            },
            _ => None,
        });
        let shown = match expression.map(|e| &e.kind) {
            Some(E::Local(id)) => self.known.get(id).cloned(),
            _ => part.cloned(),
        };
        if let Some(shown) = shown {
            for expression in shown {
                let one = Arg {
                    expression: Some(expression),
                    ..callee.clone()
                };
                self.apply_higher_once(&one, arguments, span, depth + 1, named);
            }
            return;
        }
        let substituted: Option<(usize, Vec<Arg<'e>>)> = match expression.map(|e| &e.kind) {
            Some(E::Closure(id, captures))
                if module.functions[*id].parameters.len() == captures.len() + arguments.len() =>
            {
                Some((*id, bound(captures, &callee.value)))
            }
            Some(E::Function(crate::check::FunctionRef::User(id)) | E::GenericFunction(id, _))
                if module.functions[*id].parameters.len() == arguments.len() =>
            {
                Some((*id, Vec::new()))
            }
            Some(E::Call(function, partial))
                if direct_callee(function).is_some_and(|id| {
                    module.functions[id].parameters.len() == partial.len() + arguments.len()
                }) =>
            {
                Some((
                    direct_callee(function).expect("checked above"),
                    bound(partial, &callee.value),
                ))
            }
            _ => None,
        };
        if let Some((id, mut all)) = substituted {
            all.extend(arguments.iter().cloned());
            self.apply_facts_once(id, &all, span, depth, named.unwrap_or(id));
            return;
        }
        // A parameter of the body that is the function value, or a shared reference to it (a read
        // parameter of edition 2027, or a `ref` parameter, is called through `deref`): the caller's
        // argument is the function that runs.
        let parameter = expression.and_then(|expression| match &expression.kind {
            E::Local(id) => Some(*id),
            E::Dereference(reference) => match (&reference.kind, &reference.ty) {
                (E::Local(id), Type::Reference(_, false)) => Some(*id),
                _ => None,
            },
            _ => None,
        });
        if let Some(id) = parameter
            && let Some(&index) = self.parameter_roots.get(&(usize::MAX - id))
            && self
                .declarations
                .get(&id)
                .is_some_and(|local| !local.mutable)
        {
            if self.stores.is_some() {
                let call = HigherOrderCall {
                    callee: index,
                    arguments: arguments
                        .iter()
                        .map(|argument| {
                            let mut source =
                                self.source_of(&self.argument_part(argument, Part::Value));
                            source.local |= argument.local;
                            source.temporary |= argument.temporary;
                            source.dying |= argument.dying;
                            source.lent = argument.lent;
                            source
                        })
                        .collect::<Vec<Source>>()
                        .into_iter()
                        .zip(arguments)
                        .map(|(mut source, argument)| {
                            source.hides = self.argument_hides(argument);
                            source.target = self.argument_target(argument);
                            source
                        })
                        .collect(),
                };
                if let Some(stores) = self.stores.as_mut()
                    && stores.calls.insert(call)
                {
                    self.store_changes += 1;
                }
            }
            self.apply_opaque(Some(callee), arguments, span, true, named);
            return;
        }
        self.apply_opaque(Some(callee), arguments, span, false, named);
    }

    /// A31: the conservative rule for code that the analysis does not see, given `callee` (the
    /// function value that runs, when there is one) and `arguments`:
    ///
    /// - what any of them holds may be stored wherever an argument reaches (`store_target`): the
    ///   code gets the arguments, and may call the function values that it finds in them;
    /// - what the arguments and the function value hold may be stored where the function value's
    ///   environment reaches, in a module where some function value stores into what its
    ///   environment reaches (`CallFacts::sinks`; in any other module no function value does,
    ///   whatever value this is), when the value's type is one that such a function value may
    ///   have (`may_be_sink`);
    /// - and in the environment itself, in a module where one owns the cell that it writes
    ///   (`store_hidden`), when the function value's type, or the type of an argument, may hold
    ///   such an owner (`may_own_sink`, `argument_hides`).
    ///
    /// With `local_only`, only the local targets get it (the caller substitutes the code for the
    /// rest). `named` is the named callee whose facts lead here, for the error that names it.
    fn apply_opaque(
        &mut self,
        callee: Option<&Arg<'_>>,
        arguments: &[Arg<'_>],
        span: Span,
        local_only: bool,
        named: Option<usize>,
    ) {
        // Unseen code keeps the reference that an operand lends only in a module where some
        // function stores a reference parameter itself (`handed`); it may write wherever the
        // operand reaches in any module.
        let references = self.facts.reference_sinks;
        let mut stored = Value::default();
        for argument in arguments {
            stored.absorb(&self.handed(argument));
        }
        if let Some(callee) = callee {
            stored.absorb(&self.handed(callee));
        }
        let mut reached = Value::default();
        for argument in arguments {
            reached.hold(self.argument_part(argument, Part::Value).loans);
        }
        // The function value's own loans are where its environment reaches, when its code, or
        // the code of the function that it gives after some of the arguments, may store there.
        let sink = callee.is_some_and(|callee| self.may_be_sink(callee, arguments.len()));
        let mut environment = Value::default();
        if let Some(callee) = callee.filter(|_| sink) {
            environment.hold(self.argument_part(callee, Part::Value).loans);
        }
        // A loan of a local value of the body that a callee's facts substitute here may go
        // wherever the code reaches: an argument from outside that body, or the environment of
        // the function value. A reference to a temporary argument may go wherever any does.
        // The store reaches a hidden environment when the function value that runs may own a
        // cell or hold a value that does (`may_own_sink`), or an argument may: unseen code may
        // call the function values that it finds in its arguments.
        let mut hidden = !local_only && self.facts.owned_sinks;
        if hidden && callee.is_some_and(|callee| !self.may_own_sink(callee, arguments.len())) {
            hidden = false;
            for argument in arguments {
                hidden |= self.argument_hides(argument);
            }
        }
        let operands = || callee.into_iter().chain(arguments);
        if !local_only && operands().any(|operand| operand.local) {
            // The places that the code may store the local loan into: what an argument from
            // outside the callee's body reaches, and, in a module where a function value may store
            // into what its environment reaches, what the function value's own loans reach. A
            // function value made in the callee's body (a local operand with no other loans) has
            // an environment of the callee's own values, and a call with no argument hands it
            // nothing: `get ()` in `\get -> get () * 2` stores nothing anywhere.
            let outside: Vec<Value> = operands()
                .filter(|operand| operand.external)
                .map(|operand| operand.value.clone())
                .collect();
            if callee.is_some_and(|callee| sink && self.has_targets(&callee.value))
                || hidden
                || outside.iter().any(|value| self.has_targets(value))
            {
                self.defer(self.callee_borrow_stored(named, span, false));
            }
        }
        // A reference to a temporary: an argument before the last may be kept by the function
        // that the first arguments give (`w (temporary) (ref mut arr)` applies `w` in stages),
        // and the last one by code that keeps a reference parameter (`reference_sinks`).
        let last = arguments.len().saturating_sub(1);
        let dying = arguments
            .iter()
            .enumerate()
            .any(|(index, argument)| argument.dying && (references || index < last));
        if !local_only
            && dying
            && (self.has_targets(&reached) || self.has_targets(&environment) || hidden)
        {
            self.defer(Self::temporary_borrow_stored(span));
        }
        // The code of the function value may store what its own environment holds where that
        // environment reaches, as well as what it is given: a closure that captured a borrow and
        // a cell puts the borrow into the cell (`ts[0] ()`).
        self.apply_store_within(&stored, &reached, span, local_only);
        self.apply_store_within(&stored, &environment, span, local_only);
        if hidden {
            // Unseen code may call the function values that it finds in its arguments, whose
            // environments are as hidden as that of the function value that runs.
            self.store_hidden(&stored, span);
        }
    }

    /// A31: the stores of a call that the facts of a named callee do not cover (`eval_call`, whose
    /// argument values are on `held` from `start`): those of a builtin (`builtin_stores`), of a
    /// function value (`apply_higher`), and of the function that a complete call of a named
    /// function returns, called with the arguments beyond its parameters.
    #[allow(clippy::too_many_arguments)]
    fn call_stores(
        &mut self,
        callee: &TypedExpr,
        arguments: &[TypedExpr],
        known: Option<usize>,
        boundary: Option<usize>,
        start: usize,
        span: Span,
        live: &BTreeSet<usize>,
    ) {
        if let E::Function(crate::check::FunctionRef::Builtin(instance)) = &callee.kind {
            self.builtin_stores(callee, instance.builtin, arguments, start, span, live);
            return;
        }
        let (callee, first) = match (known, boundary) {
            (Some(_), Some(boundary)) if arguments.len() > boundary => (None, boundary),
            (None, _) if !matches!(callee.kind, E::CaseConstructor { .. }) => (Some(callee), 0),
            _ => return,
        };
        let function = Arg::of(callee, self.held[start].clone());
        let rest: Vec<Arg<'_>> = arguments[first..]
            .iter()
            .enumerate()
            .map(|(at, argument)| self.argument(argument, self.held[start + 1 + at].clone(), live))
            .collect();
        self.apply_higher(&function, &rest, span, 0);
    }

    /// A31: the argument `expression` of a call, whose value is `value`. For a borrowed operand
    /// that is a place (`a == b` with a user instance, a place lent to a read parameter by an
    /// operator), the loans of the borrow of the place are found here, where the liveness is
    /// known, for the facts that store the reference itself (`Arg::reference`).
    fn argument<'e>(
        &mut self,
        expression: &'e TypedExpr,
        value: Value,
        live: &BTreeSet<usize>,
    ) -> Arg<'e> {
        let mut argument = Arg::of(Some(expression), value);
        if let E::BorrowOperand(inner) = &expression.kind
            && Self::is_place(inner)
            && inner.ty.carries_loans(&self.module.types())
            && let Ok(places) = self.place(inner, live)
        {
            let mut loans = BTreeSet::new();
            for (place, via) in places {
                loans.insert(self.loan(place, false, via, expression.span));
            }
            argument.reference = Some(loans);
        }
        argument
    }

    /// A31: the stores of a direct call of `builtin` with the argument values on `held` from
    /// `start` (the callee first): `Array.write` stores its value where its view reaches, and
    /// `SyncMap.__replace` its value where its `ref mut` reaches, when the value's type may hold a
    /// loan; a builtin that runs a callback runs it with what its other arguments reach (the locks
    /// lend their contents), and with what the callback itself holds when its result comes back
    /// as an argument of a later call (the state of `List.foldr`); `Owned.call` runs the owned
    /// function with its input. The other builtins store nothing that may hold a loan (the audit
    /// at `store_seed`).
    fn builtin_stores(
        &mut self,
        callee: &TypedExpr,
        builtin: crate::check::Builtin,
        arguments: &[TypedExpr],
        start: usize,
        span: Span,
        live: &BTreeSet<usize>,
    ) {
        let scheme = builtin.scheme();
        if arguments.len() < scheme.parameters.len() {
            return;
        }
        let value = |checker: &Self, index: usize| checker.held[start + 1 + index].clone();
        if let Some((from, to)) = store_seed(builtin) {
            let parameter = match &callee.ty {
                Type::Function(parameters, _) => parameters.get(from),
                _ => None,
            };
            if parameter
                .is_none_or(|ty| ty.carries_loans(&self.module.types()) || ty.contains_unknown())
            {
                let (stored, target) = (value(self, from), value(self, to));
                self.apply_store(&stored, &target, span);
            }
            return;
        }
        if builtin == crate::check::Builtin::OwnedCall {
            let function = Arg::of(Some(&arguments[0]), value(self, 0));
            let input = self.argument(&arguments[1], value(self, 1), live);
            self.apply_higher(&function, &[input], span, 0);
            return;
        }
        let callbacks: Vec<(usize, usize, bool)> = scheme
            .parameters
            .iter()
            .enumerate()
            .filter_map(|(index, parameter)| match parameter {
                crate::check::BuiltinType::Function(parameters, result) => {
                    let returned = format!("{result:?}");
                    let feeds_back = parameters
                        .iter()
                        .any(|parameter| format!("{parameter:?}") == returned);
                    Some((index, parameters.len(), feeds_back))
                }
                _ => None,
            })
            .collect();
        if callbacks.is_empty() {
            return;
        }
        let mut others = Value::default();
        for index in 0..scheme.parameters.len() {
            if !callbacks.iter().any(|(callback, ..)| *callback == index) {
                others.absorb(&value(self, index));
            }
        }
        for (index, arity, feeds_back) in callbacks {
            let callee = Arg::of(Some(&arguments[index]), value(self, index));
            let mut given = others.clone();
            if feeds_back {
                given.absorb(&callee.value);
            }
            // What a builtin gives its callback is a reference that it makes or an element.
            let source = Arg {
                lent: true,
                ..Arg::of(None, given)
            };
            let sources = vec![source; arity];
            self.apply_higher(&callee, &sources, span, 0);
        }
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
                    value.absorb(&stored);
                    value.regions = stored.regions;
                } else if self.external.contains(&place.root) {
                    // Borrowed data behind a reference with a named target region has that region.
                    match self
                        .slots
                        .get(&(place.root, place_slot(&place)))
                        .filter(|slot| !slot.target.is_empty())
                    {
                        Some(slot) => value.hold(slot.target.iter().copied()),
                        // A31: the reference's loans stand for what its target holds.
                        None => value.absorb(&Value {
                            loans: via.clone(),
                            through: via.clone(),
                            ..Value::default()
                        }),
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
                        .clear_loans();
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
        // A31: whether the value goes into a binding, which applies the drop hook itself; a
        // block and an `if` pass that on to their results (`tail_bound`).
        let bound = std::mem::take(&mut self.bound_next);
        self.tail_bound = bound;
        let value = match expression.kind {
            E::While { .. } | E::ForRange { .. } | E::ForEach { .. } | E::Match { .. } => {
                self.eval_control(expression, live)
            }
            E::Block { .. } => self.eval_block(expression, live),
            E::Call(..) => self.eval_call(expression, live),
            E::Lambda { .. } => self.eval_lambda(expression, live),
            _ => self.eval_value(expression, usage, live),
        }?;
        self.bound_next = false;
        if self.dropped.is_some() && !value.loans.is_empty() && self.drop_runs(&expression.ty) {
            let dropped = self.dropped.as_mut().expect("checked above");
            dropped.extend(&value.loans);
        }
        // A31: a value that is not bound is a temporary, dropped where the statement ends at the
        // latest (the scrutinee of a `match`, a value lent to a call, a record that a field is
        // read from), or handed to code that drops it.
        if !bound && !value.loans.is_empty() && self.facts.drops_write && self.tracks_stores() {
            let span = self.drop_site.unwrap_or(expression.span);
            self.drop_stores(
                &value,
                &expression.ty,
                span,
                Some(expression),
                Some(expression),
            );
        }
        Ok(value)
    }

    /// `drop_runs_code`, cached by type; no type when no `Drop.drop` of the module may make a store
    /// that the check does not track (`Lending::drops_store`).
    fn drop_runs(&mut self, ty: &Type) -> bool {
        self.facts.lending.drops_store && self.runs_drop_code(ty)
    }

    /// `drop_runs_code`, cached by type.
    fn runs_drop_code(&mut self, ty: &Type) -> bool {
        if let Some(known) = self.drop_types.get(ty) {
            return *known;
        }
        let runs = drop_runs_code(ty, &self.module.types());
        self.drop_types.insert(ty.clone(), runs);
        runs
    }

    /// A31: a value of type `ty` that is dropped, or handed by value to code that may drop it,
    /// runs the `Drop.drop` of the values with a user `Drop` instance that it stores (or that the
    /// environment of a function value or the vtable of a dyn value holds, `drop_runs_code`), and
    /// that `Drop.drop` gets only the value: what it stores is what the value holds, where the
    /// value reaches (`Stash { cell: a, f }` puts `f` into the cell that `a` shares). When some
    /// `Drop.drop` of the module may store (`CallFacts::drops_write`), the value's loans are stored
    /// where its own target loans reach, as the check of an assignment would: a local cell keeps
    /// them, and one of the caller's records a fact, or the error of a borrow of a local value.
    /// The hook runs where the value is bound or assigned over and wherever a value that is not
    /// bound is evaluated (`eval`: a temporary, or an argument that a callee drops), which is no
    /// later than its drop.
    ///
    /// `temporary` is the expression of a value that is not bound. A `Drop.drop` may also store a
    /// borrow of the dropped value itself (`let nm = ref s.name`, kept in a function that it
    /// writes into `s.cell`), which dangles once the value is gone: for a temporary, or a value
    /// that a callee drops, that is an error wherever the value reaches a place to store in; a
    /// bound value stores a borrow of its local instead (`drop_keeps_self`).
    ///
    /// `origin` is the expression that gives the value, when it is known: a lambda whose captures
    /// run no code when they are dropped (`\() -> String.length bt`) runs none either, whatever its
    /// type says (`value_runs_drop_code`).
    fn drop_stores(
        &mut self,
        value: &Value,
        ty: &Type,
        span: Span,
        temporary: Option<&TypedExpr>,
        origin: Option<&TypedExpr>,
    ) {
        if !self.tracks_stores()
            || !self.facts.drops_write
            || value.loans.is_empty()
            || !self.value_runs_drop_code(origin, ty, 0)
        {
            return;
        }
        self.apply_store(value, value, span);
        if temporary.is_some_and(|expression| !Self::is_place(expression) || !self.is_copy(ty))
            && self.drop_borrows_self(ty)
            && self.has_targets(value)
        {
            self.defer(error(
                DiagnosticKind::StoreLocalBorrowThroughReference,
                "cannot drop this value here: its Drop.drop stores a borrow of the dropped value in a place that the value shares, and the value is a temporary or is handed to a callee, so the borrow would outlive it",
                span,
            ).with_help("bind the value with 'let' in a scope that outlives every other owner of the place, or make Drop.drop store owned values only"));
        }
    }

    /// A31: whether dropping the value of `origin`, of type `ty`, may run code (`drop_runs_code`).
    /// A function value's type does not show its environment, but the expression that makes it
    /// may: a lambda drops its captures, a partial application of a named function the arguments
    /// that it binds, a named function nothing, and an immutable local bound to such values
    /// (`Checker::known`) what they drop. Any other value counts as its type says.
    fn value_runs_drop_code(
        &mut self,
        origin: Option<&TypedExpr>,
        ty: &Type,
        depth: usize,
    ) -> bool {
        if !self.runs_drop_code(ty) {
            return false;
        }
        let Some(origin) = origin.filter(|_| depth <= STORE_SUBSTITUTION_DEPTH) else {
            return true;
        };
        let module = self.module;
        match &origin.kind {
            E::Closure(_, captures) => captures
                .iter()
                .any(|capture| self.value_runs_drop_code(Some(capture), &capture.ty, depth + 1)),
            E::Function(crate::check::FunctionRef::User(_)) | E::GenericFunction(..) => false,
            E::Call(callee, bound)
                if direct_callee(callee)
                    .is_some_and(|id| bound.len() < module.functions[id].parameters.len()) =>
            {
                bound
                    .iter()
                    .any(|value| self.value_runs_drop_code(Some(value), &value.ty, depth + 1))
            }
            E::Local(id) if self.known.contains_key(id) => {
                let shown = self.known[id].clone();
                shown
                    .into_iter()
                    .any(|value| self.value_runs_drop_code(Some(value), &value.ty, depth + 1))
            }
            _ => true,
        }
    }

    /// A31: whether dropping a value of type `ty` may run a `Drop.drop` whose facts store a
    /// borrow of the dropped value itself (its parameter's own loan, `Part::Value`), or anything
    /// (`top`): that of a type with a user `Drop` instance that `ty` stores, or any of them when
    /// `ty` stores a dyn value, whose stored type it does not show.
    fn drop_borrows_self(&mut self, ty: &Type) -> bool {
        let module = self.module;
        let facts: &CallFacts = self.facts;
        let types = module.types();
        let keeps = |id: usize| {
            facts.stores.get(id).is_some_and(|stores| {
                stores.top
                    || stores
                        .stores
                        .iter()
                        .any(|(from, part, _)| *from == 0 && *part == Part::Value)
                    || stores.hidden.contains(&(0, Part::Value))
                    || stores.calls.iter().any(|call| {
                        call.arguments
                            .iter()
                            .any(|source| source.parameters.contains(&(0, Part::Value)))
                    })
            })
        };
        let any = ty.stores_some(&types, |part| matches!(part, Type::Dyn(_)));
        module.user_drops.iter().any(|(dropped, id)| {
            keeps(*id) && (any || ty.stores_some(&types, |part| part == dropped))
        })
    }

    /// A31: the local `id` of type `ty` was bound to a value whose `Drop.drop` may store a borrow
    /// of the dropped value itself (`drop_borrows_self`): a borrow of the local goes where the
    /// value's own target loans reach, so that a place that keeps it cannot outlive the local.
    fn drop_keeps_self(&mut self, id: usize, ty: &Type, span: Span) {
        if !self.tracks_stores() || !self.facts.drops_write || !self.runs_drop_code(ty) {
            return;
        }
        let Some((_, value)) = self.state.locals.get(&id) else {
            return;
        };
        let value = value.clone();
        if value.loans.is_empty() || !self.drop_borrows_self(ty) || !self.has_targets(&value) {
            return;
        }
        let loan = self.loan(
            Place {
                root: id,
                fields: Vec::new(),
                through_drop: false,
            },
            false,
            BTreeSet::new(),
            span,
        );
        let stored = Value {
            loans: BTreeSet::from([loan]),
            ..Value::default()
        };
        self.apply_store(&stored, &value, span);
    }

    /// Starts `dropped` with the parameters' loans: a parameter whose type may run code when it is
    /// dropped, or that reaches through an exclusive reference a value whose type may (the body
    /// can replace it, `deref target = value`, and so drop it), may be dropped in the body.
    fn collect_drops(&mut self) {
        let types = self.module.types();
        let mut dropped = BTreeSet::new();
        let locals = self
            .state
            .locals
            .values()
            .filter(|_| self.facts.lending.drops_store);
        for (local, value) in locals {
            if !value.loans.is_empty()
                && (drop_runs_code(&local.ty, &types)
                    || (local.ty.reaches_exclusive(&types)
                        && types.reaches(&local.ty, |part| part.runs_on_drop(&types))))
            {
                dropped.extend(&value.loans);
            }
        }
        self.dropped = Some(dropped);
    }

    /// Whether a call of `Rc.share` or its kin may read through the borrow of its owner: the body
    /// lends no way to write and hands no value to code that the analysis does not see
    /// (`body_lends_mutable`), and drops no value that holds loans and may run code when it is
    /// dropped (`drops_stop_reads`).
    fn may_read_through(&mut self) -> bool {
        let allowed = !self.drops_stop_reads && !self.body_lends_mutable();
        self.read_through_used |= allowed;
        allowed
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
        let tail_bound = self.tail_bound;
        let mut future = BTreeMap::new();
        count_uses(tail, &mut future);
        for (_, value) in bindings {
            count_uses(value, &mut future);
        }
        let mut ids = BTreeSet::new();
        for (local, value) in bindings {
            let mut current = live.clone();
            current.extend(future.keys().copied());
            // A31: the drops of the temporaries of the bound value are reported at the binding.
            let mark = self.hidden_log.len();
            let site = self.drop_site.replace(local.span);
            self.bound_next = true;
            let value_result = self.eval(value, Use::Consume, &current);
            self.drop_site = site;
            let mut value_result = value_result?;
            self.absorb_hidden(mark, &mut value_result, &local.ty);
            self.drop_stores(&value_result, &local.ty, local.span, None, Some(value));
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
            self.drop_keeps_self(local.id, &local.ty, local.span);
            let mut keep = live.clone();
            keep.extend(future.keys().copied());
            self.retain_live_loans(&keep);
        }
        self.bound_next = tail_bound;
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
        let read_through = self.reads_through(callee) && self.may_read_through();
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
            // A31: an argument that the callee may drop, and the temporaries of the argument,
            // are reported at the call.
            let site = self.drop_site.replace(expression.span);
            let value = self.eval(argument, Use::Consume, &during);
            self.drop_site = site;
            let value = value?;
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
                && !self.is_builtin_wrapper_parameter(argument)
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
                // A28: `Rc.share` and its kin hold what the borrowed owner holds, when that is
                // the body's own (`read_through`).
                None if index == 0 && read_through => {
                    // A borrowed temporary's value already has only what the temporary holds.
                    let loans = if matches!(argument.kind, E::BorrowOperand(_)) {
                        value.loans.clone()
                    } else {
                        self.read_through(&value.loans)
                    };
                    current.hold(loans);
                }
                // A28: the callee's body never puts this argument in its result. The argument
                // stays lent until the call returns (it is on `held`).
                None if retention.is_some_and(|boundary| index < boundary)
                    && known.is_some_and(|id| self.facts.temporary(id, index)) => {}
                // A31: without the read-through, the result of `Rc.share` and its kin holds the
                // borrow of the owner in place of what the block holds (`Loan::share`).
                None if index == 0 && self.reads_through(callee) => {
                    let shares = self.share_loans(&value.loans);
                    current.hold(shares);
                }
                None => current.absorb(&value),
            }
            self.held.push(value);
            if boundary == Some(index + 1) {
                let id = known.unwrap();
                self.store_arguments(id, arguments, start, expression.span)?;
                // A31: what the callee's body stores where its arguments reach, before a closed
                // result drops the arguments' loans.
                if self.tracks_stores()
                    && self
                        .facts
                        .stores
                        .get(id)
                        .is_some_and(|facts| !facts.is_empty())
                {
                    let mut held: Vec<Arg<'_>> = Vec::with_capacity(index + 1);
                    for (at, argument) in arguments[..=index].iter().enumerate() {
                        let value = self.held[start + 1 + at].clone();
                        held.push(self.argument(argument, value, &during));
                    }
                    self.apply_facts(id, &held, expression.span, 0);
                }
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
        if self.tracks_stores() {
            self.call_stores(
                callee,
                arguments,
                known,
                boundary,
                start,
                expression.span,
                &during,
            );
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
                result.hold([loan]);
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
        // A lambda of a body that reads nothing through reads nothing through either
        // (`lending_bodies`).
        let lends_mutable = self.body_lends_mutable().then_some(true);
        self.copy_variables.extend(check_body(
            self.module,
            locals.into_iter(),
            body,
            self.infer,
            self.facts,
            matches!(expression.ty, Type::Once(_)),
            false,
            Contract {
                lends_mutable,
                ..Contract::default()
            },
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
            result.absorb(&value);
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
                        let (local, stored) = self.state.locals.get_mut(&place.root).unwrap();
                        let ty = local.ty.clone();
                        let old = if place.fields.is_empty() {
                            Some(std::mem::replace(stored, value.clone()))
                        } else {
                            join(stored, value.clone());
                            stored.regions = None;
                            stored.closed_result = [false; 2];
                            None
                        };
                        // A31: the old value is dropped, and a value that may run code when it
                        // is dropped now holds what was stored (A28's `dropped`), which its drop
                        // may store where it reaches.
                        match old {
                            Some(old) => self.drop_stores(&old, &ty, expression.span, None, None),
                            None => {
                                let owner = self.state.locals[&place.root].1.clone();
                                self.drop_stores(&owner, &ty, expression.span, None, None);
                            }
                        }
                        self.drop_keeps_self(place.root, &ty, expression.span);
                        if self.dropped.is_some() && !value.loans.is_empty() && self.drop_runs(&ty) {
                            self.dropped
                                .as_mut()
                                .expect("checked above")
                                .extend(&value.loans);
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
                let tail_bound = self.tail_bound;
                self.eval(condition, Use::Consume, &during)?;
                let before = self.state.clone();
                let before_reachable = self.reachable;
                self.bound_next = tail_bound;
                let then_value = self.eval(then_branch, usage, live)?;
                let then_state = self.state.clone();
                let then_reachable = self.reachable;
                self.state = before;
                self.reachable = before_reachable;
                self.bound_next = tail_bound;
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
                    result.absorb(&value);
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
                let mut values = Vec::with_capacity(arguments.len());
                for argument in arguments {
                    values.push(self.eval(argument, Use::Consume, &during)?);
                }
                // A31: they run the methods of the dyn values that they reach, and the method of
                // each element type (`Eq.eq`, `Hash.hash` or `Display.display`: the function
                // values after the operands), with references to the elements of the operands.
                let types = self.module.types();
                if self.tracks_stores() {
                    let operands: Vec<Arg<'_>> = arguments
                        .iter()
                        .zip(values)
                        .map(|(argument, value)| Arg::of(Some(argument), value))
                        .collect();
                    if arguments
                        .iter()
                        .any(|argument| types.reaches(&argument.ty, |ty| matches!(ty, Type::Dyn(_))))
                    {
                        self.apply_opaque(None, &operands, expression.span, false, None);
                    }
                    let compared = operands
                        .iter()
                        .take_while(|operand| {
                            !matches!(
                                operand.expression.map(|e| &e.ty),
                                Some(Type::Function(..))
                            )
                        })
                        .count();
                    for method in &operands[compared..] {
                        let Some(Type::Function(parameters, _)) =
                            method.expression.map(|e| &e.ty)
                        else {
                            continue;
                        };
                        // Each parameter may be an element of any operand.
                        let mut element = Value::default();
                        for operand in &operands[..compared] {
                            element.absorb(&operand.value);
                        }
                        let elements = vec![Arg::of(None, element); parameters.len()];
                        self.apply_higher(method, &elements, expression.span, 0);
                    }
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
                // A31: the callback runs with what the other arguments reach (the chunks of an
                // exclusive slice, the elements of the input).
                if self.tracks_stores() && callback < arguments.len() {
                    let mut others = Value::default();
                    for (index, value) in self.held[base..].iter().enumerate() {
                        if index != callback {
                            others.absorb(value);
                        }
                    }
                    let function = &arguments[callback];
                    let arity = match &function.kind {
                        E::Closure(id, captures) => self.module.functions[*id]
                            .parameters
                            .len()
                            .saturating_sub(captures.len()),
                        _ => match &function.ty {
                            Type::Function(parameters, _) => parameters.len(),
                            _ => 1,
                        },
                    };
                    let callee = Arg::of(Some(function), self.held[base + callback].clone());
                    let sources = vec![Arg::of(None, others); arity];
                    self.apply_higher(&callee, &sources, expression.span, 0);
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
                    && !self.is_builtin_wrapper_parameter(left)
                {
                    return Err(self.owned_function_holds_borrow(left, &value, &expression.ty));
                }
                self.held.push(value.clone());
                // A28: `x |> f a` completes `f a x`, and `x |> f` into a unary `f` completes
                // `f x`, so the facts of `f` select its arguments' loans as in `eval_call`.
                let completed = (*operator == BinaryOp::Pipe)
                    .then(|| self.piped_callee(expression, right))
                    .flatten();
                let mut piped = None;
                let callee = if owned_function {
                    Value::default()
                } else if let Some(id) = completed {
                    let (callee, values) = self.eval_piped_arguments(id, right, &during)?;
                    piped = Some(values);
                    callee
                } else {
                    self.eval(right, usage, &during)?
                };
                self.held.pop();
                if *operator == BinaryOp::Pipe && !owned_function && self.tracks_stores() {
                    self.drop_stores(&value, &left.ty, expression.span, Some(left), Some(left));
                    self.pipe_stores(left, right, &value, &callee, piped, expression.span, &during);
                }
                if *operator == BinaryOp::Pipe && expression.ty.carries_loans(&self.module.types())
                {
                    result = value;
                    // `x |> Owned.call (ref f)` is `Owned.call (ref f) x`, whose result does not
                    // borrow `f` (see `eval_call`).
                    let owned_call = matches!(&right.kind, E::Call(inner, arguments)
                        if arguments.len() == 1
                            && self.builtin_called(inner) == Some(crate::check::Builtin::OwnedCall));
                    if let Some(id) = completed {
                        let piped = piped_application(right).1.len();
                        // A closed result holds no loan, as at a direct call: `joint_facts` revokes
                        // the closed result of a function whose check returns a loan that a store
                        // into a local cell put there (A31).
                        if self.facts.closed[id] {
                            result.clear_loans();
                        } else if self.facts.temporary(id, piped) {
                            result.loans = callee.loans;
                            result.through = callee.through;
                        } else {
                            result.absorb(&callee);
                        }
                    } else if self.reads_through(right) && self.may_read_through() {
                        // `ref shared |> Arc.share` is `Arc.share (ref shared)`.
                        if !matches!(left.kind, E::BorrowOperand(_)) {
                            result.loans = self.read_through(&result.loans);
                            result.through.clear();
                        }
                        result.absorb(&callee);
                    } else if self.reads_through(right) {
                        result.loans = self.share_loans(&result.loans);
                        result.through.clear();
                        result.absorb(&callee);
                    } else if !owned_call {
                        result.absorb(&callee);
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
                        result.absorb(&value);
                    }
                    self.held.push(value);
                }
                // A31: a host function that takes callbacks may run them with its arguments.
                if let E::HostCall(import, _) = &expression.kind
                    && import.callbacks
                    && self.tracks_stores()
                {
                    let arguments: Vec<Arg<'_>> = elements
                        .iter()
                        .zip(&self.held[start..])
                        .map(|(element, value)| Arg::of(Some(element), value.clone()))
                        .collect();
                    self.apply_opaque(None, &arguments, expression.span, false, None);
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
                // A31: a hole's method is a call of the instance (or of a dyn value's method).
                if self.tracks_stores() {
                    for (hole, value) in interpolation.holes.iter().zip(start..) {
                        if let Some(method) = &hole.method {
                            let operand =
                                self.argument(&hole.operand, self.held[value].clone(), &during);
                            let method = Arg::of(Some(method), Value::default());
                            self.apply_higher(&method, &[operand], expression.span, 0);
                        }
                    }
                }
                self.held.truncate(start);
            }
            E::NewArray(length, initializer) | E::NewList(length, initializer) => {
                self.eval(length, Use::Consume, &during)?;
                let value = self.eval(initializer, Use::Consume, &during)?;
                // A31: the initializer runs with each index.
                if self.tracks_stores() {
                    let function = Arg::of(Some(initializer), value.clone());
                    self.apply_higher(
                        &function,
                        &[Arg::of(None, Value::default())],
                        expression.span,
                        0,
                    );
                }
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
                value.absorb(other_value);
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
                            facts.contract(&module, id),
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

    /// A31, step 4: a program in which some function stores a value that may hold loans with a
    /// seed (`Array.write` of a function here, in a function that no other one calls, in a partial
    /// application, or in the instance of `Array.set` that a pipeline calls) keeps the facts of the
    /// functions that store nothing, as the same program with the store of an `i64` does. A28's
    /// eighth review dropped every fact of such a program (`untracked_loan_store`), until A31
    /// tracked the stores of the seeds.
    #[test]
    fn a_tracked_store_anywhere_keeps_the_facts() {
        let functions = "def a28_pass :: ref i64 -> own (unit -> i64) -> (unit -> i64)\nfn a28_pass _key f = f\n";
        for (store, seeded) in [
            (
                "def a28_store :: own (unit -> i64) -> i64\nfn a28_store g =\n    let mut arr = [\\() -> 0]\n    Array.write 0 g (ref mut arr)\n    arr[0] ()\n",
                true,
            ),
            (
                "def a28_store :: own (unit -> i64) -> i64\nfn a28_store g =\n    let mut arr = [\\() -> 0]\n    let w = Array.write 0 g\n    w (ref mut arr)\n    arr[0] ()\n",
                true,
            ),
            (
                "def a28_store :: own (unit -> i64) -> i64\nfn a28_store g =\n    let mut arr = [\\() -> 0]\n    ref mut arr |> Array.set 0 g\n    arr[0] ()\n",
                true,
            ),
            (
                "def a28_store :: own (unit -> i64) -> i64\nfn a28_store g =\n    let mut arr = [0]\n    Array.write 0 (g ()) (ref mut arr)\n    arr[0]\n",
                false,
            ),
        ] {
            let source = format!(
                "{functions}{store}let one = 1\nlet a = a28_pass (ref one) (\\() -> 42)\na () + a28_store (\\() -> 1)"
            );
            let module = crate::analyze(&source).unwrap();
            let pass = module
                .functions
                .iter()
                .position(|function| function.name == "a28_pass")
                .unwrap();
            for infer in [false, true] {
                let facts = call_facts(&module, infer);
                assert_eq!(facts.seeded, seeded && !infer, "{source} {infer}");
                assert_eq!(
                    facts.retained[pass].as_deref(),
                    Some(&[false, true][..]),
                    "{source} {infer}"
                );
                assert_eq!(
                    facts.contract(&module, pass).lends_mutable,
                    Some(false),
                    "{source} {infer}"
                );
            }
        }
    }

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

    /// A28 review: a body that lends an exclusive borrow to `Array.write` keeps `text`, which the
    /// write stores in its array, in its result. A28 gave such a body no facts; since A31 the
    /// checks of the instances apply the store facts and keep the facts, and the inference, which
    /// applies none, keeps A28's rule for a body that may store. A body that only reads its array
    /// keeps its facts in both.
    #[test]
    fn a_body_that_lends_an_exclusive_borrow_keeps_what_the_write_stores() {
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
            let lending = |name: &str| {
                let id = module
                    .functions
                    .iter()
                    .position(|function| function.name == name)
                    .unwrap_or_else(|| panic!("{name}"));
                facts.lending.facts[id]
            };
            assert_eq!(lending("a28_stash"), infer, "{infer}");
            assert_eq!(lending("a28_piped"), infer, "{infer}");
            assert!(!lending("a28_read"), "{infer}");
        }
    }

    /// A28 review (5th round): a body that passes its `Mutex` to a lock builtin keeps what the
    /// callback stores there in its result, whatever the callback's form. The lock runs a function
    /// value, code whose body the analysis does not see (`opaque_calls`, which A31 keeps), so the
    /// body has no facts and reads nothing through in both modules (A31's store facts put what the
    /// callback stores in `m` as well). An `Atomic<i64>` holds no borrow, so a body that only counts
    /// with one keeps its facts and its read-through.
    #[test]
    fn a_body_that_lends_a_mutable_cell_keeps_what_the_callback_stores() {
        let module = crate::analyze(
            "def a28_put :: ref string -> ref mut [(unit -> i64)] -> unit\nfn a28_put text slot = Array.write 0 (\\() -> String.length text) slot\n\
             def a28_locked :: ref string -> own (unit -> i64) -> (unit -> i64)\nfn a28_locked text g =\n    let m = Mutex.create [g]\n    Mutex.with_lock (ref m) (a28_put text)\n    let arr = Mutex.into_inner m\n    arr[0]\n\
             def a28_counted :: ref string -> own (unit -> i64) -> (unit -> i64)\nfn a28_counted _text g =\n    let c = Atomic.create 0i64\n    let _n = Atomic.fetch_add (ref c) 1i64\n    g\n\
             let text = \"abc\"\n\
             let a = a28_locked (ref text) (\\() -> 1)\nlet b = a28_counted (ref text) (\\() -> 2)\n\
             a () + b ()",
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
            assert_eq!(retained("a28_locked"), Some(vec![true, true]), "{infer}");
            assert_eq!(retained("a28_counted"), Some(vec![false, true]), "{infer}");
            let id = |name: &str| {
                module
                    .functions
                    .iter()
                    .position(|function| function.name == name)
                    .unwrap_or_else(|| panic!("{name}"))
            };
            assert!(facts.lending.facts[id("a28_locked")], "{infer}");
            assert!(facts.lending.reads[id("a28_locked")], "{infer}");
            assert!(!facts.lending.facts[id("a28_counted")], "{infer}");
            assert!(!facts.lending.reads[id("a28_counted")], "{infer}");
        }
    }

    /// A28 review (6th round): a body that hands a borrowing value to code that the analysis does not
    /// see (a function-valued parameter, a builtin that runs its callback, a named function that
    /// does either) has no facts and reads nothing through. A body that calls a closure with `()`
    /// has no facts either, since the closure may store what it captured, but it still reads
    /// through. A body that only passes a function value on keeps its facts.
    #[test]
    fn a_body_that_calls_unseen_code_has_no_facts() {
        let module = crate::analyze(
            "def a28_calls :: own ((unit -> i64) -> unit) -> ref string -> own (unit -> i64) -> (unit -> i64)\nfn a28_calls w text r =\n    w (\\() -> String.length text)\n    r\n\
             def a28_thunk :: own (unit -> unit) -> own (unit -> i64) -> (unit -> i64)\nfn a28_thunk t r =\n    t ()\n    r\n\
             def a28_iter :: own ((unit -> i64) -> unit) -> ref string -> own (unit -> i64) -> (unit -> i64)\nfn a28_iter w text r =\n    let arr = [\\() -> String.length text]\n    Array.iter w (ref arr)\n    r\n\
             def a28_through :: own ((unit -> i64) -> unit) -> ref string -> own (unit -> i64) -> (unit -> i64)\nfn a28_through w text r = a28_calls w text r\n\
             def a28_plain :: ref string -> own (unit -> i64) -> (unit -> i64)\nfn a28_plain _text g = g\n\
             let text = \"abc\"\n\
             let a = a28_calls (\\_ -> ()) (ref text) (\\() -> 1)\nlet b = a28_thunk (\\() -> ()) (\\() -> 2)\n\
             let c = a28_iter (\\_ -> ()) (ref text) (\\() -> 3)\nlet d = a28_through (\\_ -> ()) (ref text) (\\() -> 4)\n\
             let e = a28_plain (ref text) (\\() -> 5)\n\
             a () + b () + c () + d () + e ()",
        )
        .unwrap();
        let id = |name: &str| {
            module
                .functions
                .iter()
                .position(|function| function.name == name)
                .unwrap_or_else(|| panic!("{name}"))
        };
        for infer in [false, true] {
            let facts = call_facts(&module, infer);
            let retained = |name: &str| facts.retained[id(name)].as_deref().map(<[bool]>::to_vec);
            assert_eq!(
                retained("a28_calls"),
                Some(vec![true, true, true]),
                "{infer}"
            );
            assert_eq!(retained("a28_thunk"), Some(vec![true, true]), "{infer}");
            assert_eq!(
                retained("a28_iter"),
                Some(vec![true, true, true]),
                "{infer}"
            );
            assert_eq!(
                retained("a28_through"),
                Some(vec![true, true, true]),
                "{infer}"
            );
            assert_eq!(retained("a28_plain"), Some(vec![false, true]), "{infer}");
            for name in ["a28_calls", "a28_iter", "a28_through"] {
                assert!(facts.lending.reads[id(name)], "{name} {infer}");
            }
            assert!(facts.lending.facts[id("a28_thunk")], "{infer}");
            assert!(!facts.lending.reads[id("a28_thunk")], "{infer}");
            assert!(!facts.lending.facts[id("a28_plain")], "{infer}");
        }
    }

    /// A28, seventh review: dropping a value runs its `Drop.drop`, which may store what the value
    /// holds where the result reaches it. When a `Drop.drop` of the module makes a store that the
    /// check does not track, the parameters whose loans a value of a type that may run code when it
    /// is dropped holds are retained: a moved-in parameter that is never used, a local made from a
    /// borrowed parameter, and a value handed to a callee that drops it. A borrowed one is not
    /// dropped, and a parameter whose loans no such value holds stays temporary. Without such a
    /// `Drop.drop` (the second module), the same functions keep the facts from before.
    #[test]
    fn a_dropped_value_keeps_the_loans_that_it_holds() {
        let functions = "def a28_unused :: own Stash -> own (unit -> i64) -> (unit -> i64)\nfn a28_unused _s r = r\n\
             def a28_local :: own ((unit -> i64) -> unit) -> ref string -> own (unit -> i64) -> (unit -> i64)\nfn a28_local w text r =\n    let _held = Stash { w: w, f: \\() -> String.length text }\n    r\n\
             def a28_consume :: own Stash -> unit\nfn a28_consume _s = ()\n\
             def a28_consumes :: own Stash -> own (unit -> i64) -> (unit -> i64)\nfn a28_consumes s r =\n    a28_consume s\n    r\n\
             def a28_key :: ref string -> own Stash -> own (unit -> i64) -> (unit -> i64)\nfn a28_key _key s r =\n    let _held = s\n    r\n\
             def a28_lent :: ref string -> ref Stash -> own (unit -> i64) -> (unit -> i64)\nfn a28_lent _key _s r = r\n\
             let text = \"abc\"\n\
             let s = a28_unused (Stash { w: \\_ -> (), f: \\() -> 1 }) (\\() -> 1)\n\
             let l = a28_local (\\_ -> ()) (ref text) (\\() -> 2)\n\
             let c = a28_consumes (Stash { w: \\_ -> (), f: \\() -> 3 }) (\\() -> 3)\n\
             let k = a28_key (ref text) (Stash { w: \\_ -> (), f: \\() -> 4 }) (\\() -> 4)\n\
             let held = Stash { w: \\_ -> (), f: \\() -> 5 }\n\
             let b = a28_lent (ref text) (ref held) (\\() -> 5)\n\
             s () + l () + c () + k () + b ()";
        let stash = "record Stash { w: (unit -> i64) -> unit, f: unit -> i64 }\n";
        // The first `Drop.drop` calls a function value that it holds (code that the analysis does
        // not see), the second does nothing.
        for (drop, storing) in [
            (
                "instance Drop<Stash> {\n    fn drop s = { let w = s.w; let g = s.f; w g }\n}\n",
                true,
            ),
            ("instance Drop<Stash> {\n    fn drop _s = ()\n}\n", false),
        ] {
            let module = crate::analyze(&format!("{stash}{drop}{functions}")).unwrap();
            let id = |name: &str| {
                module
                    .functions
                    .iter()
                    .position(|function| function.name == name)
                    .unwrap_or_else(|| panic!("{name}"))
            };
            for infer in [false, true] {
                let facts = call_facts(&module, infer);
                assert_eq!(facts.lending.drops_store, storing, "{infer}");
                let retained =
                    |name: &str| facts.retained[id(name)].as_deref().map(<[bool]>::to_vec);
                assert_eq!(
                    retained("a28_unused"),
                    Some(vec![storing, true]),
                    "{storing} {infer}"
                );
                assert_eq!(
                    retained("a28_local"),
                    Some(vec![storing, storing, true]),
                    "{storing} {infer}"
                );
                assert_eq!(
                    retained("a28_consumes"),
                    Some(vec![storing, true]),
                    "{storing} {infer}"
                );
                assert_eq!(
                    retained("a28_key"),
                    Some(vec![false, storing, true]),
                    "{storing} {infer}"
                );
                assert_eq!(
                    retained("a28_lent"),
                    Some(vec![false, false, true]),
                    "{storing} {infer}"
                );
            }
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
        let lending = lending_bodies(&module, false);
        let full = parameter_retention(&module, &closed, &lending, false);
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
            parameter_retention_within(&module, &closed, &lending, false, Some(0))
                .iter()
                .all(Option::is_none)
        );
        let mut partial = false;
        for budget in 1..16 {
            let facts = parameter_retention_within(&module, &closed, &lending, false, Some(budget));
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

    /// A31: the store facts of the bodies. `Array.write`'s generated function stores its value
    /// into its view; `put` and the instance of `Array.set` inherit that; `copy_first` stores what
    /// it reads through `source` (`Part::Contents`); `rotate` writes back its own contents (no
    /// fact); `apply` calls its function-valued parameter with its array (a higher-order fact); a
    /// body that only reads stores nothing. The abstract bodies of the inference have no facts.
    /// A31, review round 1: the flags that widen what a call of a function value that the check
    /// does not see may store come from the function values that the module makes.
    #[test]
    fn the_sink_flags_follow_the_function_values_of_the_module() {
        let flags = |source: &str| {
            let module = crate::analyze(source).unwrap_or_else(|error| {
                panic!("{source}\n{}: {}", error.code, error.message);
            });
            let facts = call_facts(&module, false);
            (
                facts.sinks,
                facts.owned_sinks,
                facts.reference_sinks,
                facts.owned_types.len(),
            )
        };
        // A lambda that stores its argument where a reference that it captured reaches.
        assert_eq!(
            flags(
                "let m = Mutex.create [\\() -> 0]\nlet mr = ref m\n\
                 let w = \\g -> Mutex.with_lock mr (\\slot -> Array.write 0 g slot)\n\
                 w (\\() -> 1)\nMutex.with_lock (ref m) (\\slot -> slot[0] ())"
            ),
            (true, false, false, 0)
        );
        // A partial application of a builtin that runs a callback with the lock that it binds.
        assert_eq!(
            flags(
                "let m = Mutex.create [\\() -> 0]\nlet lock = Mutex.with_lock (ref m)\n\
                 lock (\\slot -> Array.write 0 (\\() -> 1) slot)\nMutex.with_lock (ref m) (\\slot -> slot[0] ())"
            ),
            (true, false, false, 0)
        );
        // A lambda that owns the cell that it writes: its type is the one that a call must not
        // lose sight of.
        assert_eq!(
            flags(
                "def a31_mk :: own Arc<Mutex<[unit -> i64]>> -> ((unit -> i64) -> unit)\n\
                 fn a31_mk a = \\g -> Mutex.with_lock (Arc.get (ref a)) (\\slot -> Array.write 0 g slot)\n\
                 let w = a31_mk (Arc.new (Mutex.create [\\() -> 0]))\nw (\\() -> 1)\n0"
            ),
            // The lambda, and the callback that it gives the lock, which holds `g`.
            (true, true, false, 2)
        );
        // A lambda that only reads the cell that it owns stores nothing.
        assert_eq!(
            flags(
                "def a31_reader :: own Arc<Mutex<[unit -> i64]>> -> (unit -> i64)\n\
                 fn a31_reader a = \\() -> Mutex.with_lock (Arc.get (ref a)) (\\slot -> slot[0] ())\n\
                 let mut arr = [\\() -> 1]\nArray.set 0 (\\() -> 2) (ref mut arr)\n\
                 let r = a31_reader (Arc.new (Mutex.create [\\() -> 0]))\nr () + arr[0] ()"
            ),
            (false, false, false, 0)
        );
        // A function that keeps the reference that it gets, in a function that it stores.
        assert_eq!(
            flags(
                "def a31_keep :: ref string -> ref mut [(unit -> i64)..] -> unit\n\
                 fn a31_keep s slot = Array.write 0 (\\() -> String.length s) slot\n\
                 let text = \"abc\"\nlet mut arr = [\\() -> 1]\na31_keep (ref text) (ref mut arr)\narr[0] ()"
            ),
            (false, false, true, 0)
        );
    }

    #[test]
    fn store_facts_follow_what_the_bodies_store() {
        let module = crate::analyze(
            "def a31_put :: own (unit -> i64) -> ref mut [(unit -> i64)..] -> unit\nfn a31_put f target = Array.write 0 f target\n\
             def a31_copy_first :: ref [(unit -> i64)] -> ref mut [(unit -> i64)..] -> unit\nfn a31_copy_first source target = Array.write 0 source[0] target\n\
             def a31_rotate :: ref mut [(unit -> i64)..] -> unit\nfn a31_rotate arr =\n    let second = arr[1]\n    Array.write 0 second arr\n\
             def a31_apply :: own (ref mut [(unit -> i64)..] -> i64) -> ref mut [(unit -> i64)..] -> i64\nfn a31_apply g arr = g arr\n\
             def a31_read :: ref [(unit -> i64)] -> i64\nfn a31_read arr = arr[0] ()\n\
             let mut a = [\\() -> 1, \\() -> 2]\n\
             let b = [\\() -> 3]\n\
             a31_put (\\() -> 4) (ref mut a)\n\
             a31_copy_first (ref b) (ref mut a)\n\
             a31_rotate (ref mut a)\n\
             Array.set 1 (\\() -> 5) (ref mut a)\n\
             a31_apply (\\x -> x[0] ()) (ref mut a) + a31_read (ref b)",
        )
        .unwrap();
        let id = |name: &str| {
            module
                .functions
                .iter()
                .position(|function| function.name == name)
                .unwrap_or_else(|| panic!("{name}"))
        };
        let facts = call_facts(&module, false);
        let stores = |name: &str| facts.stores[id(name)].clone();
        let only = |entries: &[(usize, Part, usize)]| StoreFacts {
            stores: entries.iter().copied().collect(),
            ..StoreFacts::default()
        };
        assert_eq!(stores("a31_put"), only(&[(0, Part::Value, 1)]));
        assert_eq!(stores("a31_copy_first"), only(&[(0, Part::Contents, 1)]));
        assert_eq!(stores("a31_rotate"), StoreFacts::default());
        assert_eq!(stores("a31_read"), StoreFacts::default());
        assert_eq!(
            stores("a31_apply"),
            StoreFacts {
                calls: BTreeSet::from([HigherOrderCall {
                    callee: 0,
                    // `arr` is a reference itself, to an array of function values: a store
                    // may reach through it (`target`), and it holds no function value of a type
                    // that owns the cell that it writes (no such function value here).
                    arguments: vec![Source {
                        parameters: BTreeSet::from([(1, Part::Value)]),
                        lent: true,
                        hides: false,
                        target: true,
                        ..Source::default()
                    }],
                }]),
                ..StoreFacts::default()
            }
        );
        // No function value of this program stores into what its environment reaches.
        assert!(facts.seeded);
        assert_eq!(
            (facts.sinks, facts.owned_sinks, facts.reference_sinks),
            (false, false, false)
        );
        // The instance of `Array.set` that the program calls, and `Array.write`'s generated
        // function, store their value into their view.
        let set: Vec<usize> = module
            .functions
            .iter()
            .enumerate()
            .filter(|(_, function)| {
                function.module.ends_with("Array")
                    && function.name.starts_with("set")
                    && function.parameters.len() == 3
                    && matches!(&function.parameters[1].ty, Type::Function(..))
            })
            .map(|(id, _)| id)
            .collect();
        assert!(
            !set.is_empty(),
            "the instance of Array.set: {:?}",
            module
                .functions
                .iter()
                .filter(|function| function.name.contains("set"))
                .map(|function| function.qualified_name())
                .collect::<Vec<_>>()
        );
        for set in set {
            assert_eq!(facts.stores[set], only(&[(1, Part::Value, 2)]));
        }
        let write = module
            .functions
            .iter()
            .position(|function| {
                function.module == "$builtin"
                    && matches!(&function.body.kind, E::Call(callee, _)
                        if matches!(&callee.kind, E::Function(crate::check::FunctionRef::Builtin(instance))
                            if instance.builtin == crate::check::Builtin::ArrayWrite)
                        && matches!(&function.parameters[1].ty, Type::Function(..)))
            })
            .expect("the generated function of Array.write");
        assert_eq!(facts.stores[write], only(&[(1, Part::Value, 2)]));
        let abstract_facts = call_facts(&module, true);
        assert!(abstract_facts.stores.iter().all(StoreFacts::is_empty));
    }
}
