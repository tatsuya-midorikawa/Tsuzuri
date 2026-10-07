use std::collections::{BTreeMap, BTreeSet};

use crate::check::{
    CallbackContract, CheckedModule, Local, MAX_RECORD_REGIONS, RegionMask, RegionSlots,
    RegionSources, Type, TypedExpr, TypedExprKind as E,
};
use crate::copies::{CopyKind, CopyRead};
use crate::diagnostic::{Diagnostic, Diagnostics, MAX_UNIQUE_DIAGNOSTICS, Span};
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

struct Loan {
    place: Place,
    mutable: bool,
    parents: BTreeSet<usize>,
    /// The type of the pattern view that holds this loan. Such a view would be
    /// a copy if the type were Copy, so while inferring constraints a conflict
    /// with the loan requires Copy instead of rejecting the access.
    view: Option<Type>,
}

#[derive(Clone, Default)]
struct State {
    locals: BTreeMap<usize, (Local, Value)>,
    aliases: BTreeMap<usize, Vec<(Place, BTreeSet<usize>)>>,
    moved: BTreeSet<Place>,
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
    check_functions(module, false).err().unwrap_or_default()
}

pub(crate) fn infer_copy_all(
    module: &CheckedModule,
) -> Result<Vec<BTreeSet<String>>, Vec<Diagnostic>> {
    check_functions(module, true)
}

// ponytail: access errors only while types are broken; Error stubs lack loans, so lifetimes wait.
pub(crate) fn check_recovered(module: &CheckedModule) -> Vec<Diagnostic> {
    let closed = closed_returns(module);
    let mut recovered = Vec::new();
    for function in &module.functions {
        let _ = check_body(
            module,
            &function.parameters,
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
            &function.parameters,
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

fn check_functions(
    module: &CheckedModule,
    infer: bool,
) -> Result<Vec<BTreeSet<String>>, Vec<Diagnostic>> {
    let closed = closed_returns(module);
    let mut constraints = Vec::new();
    let mut diagnostics = Diagnostics::new(0);
    for (id, function) in module.functions.iter().enumerate() {
        if diagnostics.is_full() {
            break;
        }
        let mut recovered = Vec::new();
        let checked = check_body(
            module,
            &function.parameters,
            &function.body,
            infer,
            &closed,
            function.is_task,
            module.user_drops.values().any(|drop| *drop == id),
            Contract::of(function),
            &mut recovered,
            None,
        );
        diagnostics.extend(recovered);
        match checked {
            Ok(required) => constraints.push(required),
            Err(error) => diagnostics.push(error),
        }
    }
    if diagnostics.is_empty() {
        Ok(constraints)
    } else {
        Err(diagnostics.into_vec())
    }
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
fn check_body(
    module: &CheckedModule,
    parameters: &[Local],
    body: &TypedExpr,
    infer: bool,
    closed: &[bool],
    task: bool,
    user_drop: bool,
    contract: Contract<'_>,
    recovered: &mut Vec<Diagnostic>,
    copies: Option<&mut Vec<CopyRead>>,
) -> Result<BTreeSet<String>, Diagnostic> {
    let mut checker = Checker {
        module,
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
            .map(|callback| (parameters[callback.parameter].id, callback))
            .collect(),
        // `Drop.drop` borrows the dropped value through its only parameter.
        drop_root: user_drop.then(|| usize::MAX - parameters[0].id),
    };
    for (index, parameter) in parameters.iter().enumerate() {
        let mut value = Value::default();
        if !task && parameter.ty.carries_loans(&module.types()) {
            checker.external_loans(parameter, index, contract.slots, &mut value);
        }
        checker
            .state
            .locals
            .insert(parameter.id, (parameter.clone(), value));
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
        | Type::FixedArray(element, _) => owned(element, module),
        Type::Tuple(elements) => elements.iter().all(|ty| owned(ty, module)),
        Type::Record(id, arguments) => module
            .types()
            .record_fields(*id, arguments)
            .iter()
            .all(|ty| owned(ty, module)),
        Type::Union(id, arguments) => module
            .types()
            .union_payloads(*id, arguments)
            .iter()
            .flatten()
            .all(|ty| owned(ty, module)),
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

fn error(code: &'static str, message: impl Into<String>, span: Span) -> Diagnostic {
    Diagnostic::new(code, message, span)
}

/// Whether `callee` is `Dyn.of` building a dyn value without a region, which owns all it holds (A14).
fn owned_dyn_of(callee: &TypedExpr) -> bool {
    matches!(&callee.kind, E::Function(crate::check::FunctionRef::Builtin(instance))
        if instance.builtin == crate::check::Builtin::DynOf
            && matches!(instance.types.get(1), Some(Type::Dyn(dyn_type)) if !dyn_type.borrowed))
}

struct Checker<'a> {
    module: &'a CheckedModule,
    state: State,
    loans: Vec<Loan>,
    held: Vec<Value>,
    loop_flows: Vec<control::LoopFlow>,
    try_flows: Vec<control::TryFlow>,
    reachable: bool,
    external: BTreeSet<usize>,
    /// A13 Phase 2: the region slots of the parameters, by external root and slot.
    slots: BTreeMap<(usize, usize), ExternalSlot>,
    infer: bool,
    copy_variables: BTreeSet<String>,
    closed: &'a [bool],
    recovered: &'a mut Vec<Diagnostic>,
    /// Collects the copied consuming reads for `copies::sites`; the checks stay the same.
    copies: Option<&'a mut Vec<CopyRead>>,
    reported: BTreeSet<usize>,
    /// Parameters with region-quantified function types, by local id (A12 Phase 2).
    callbacks: BTreeMap<usize, &'a CallbackContract>,
    /// The root of the value that the checked `Drop.drop` body drops.
    drop_root: Option<usize>,
}

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

impl<'a> Checker<'a> {
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
                        &function.parameters,
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

impl Checker<'_> {
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

    fn loan(&mut self, place: Place, mutable: bool, mut parents: BTreeSet<usize>) -> usize {
        if let Some((_, value)) = self.state.locals.get(&place.root) {
            parents.extend(&value.loans);
        }
        let id = self.loans.len();
        self.loans.push(Loan {
            place,
            mutable,
            parents,
            view: None,
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
            loans.push(self.loan(place, mutable, BTreeSet::new()));
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
        parameters: &[Local],
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
            if task || !self.external.contains(&place.root) {
                return Err(error(
                    "E1013",
                    if task {
                        "task results cannot retain borrowed values"
                    } else {
                        "cannot return a reference to a local value"
                    },
                    body.span,
                ));
            }
            let Some(sources) = region_sources else {
                continue;
            };
            let owner = parameters
                .iter()
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
        if usage != Use::Write && self.state.moved.iter().any(|moved| moved.overlaps(place)) {
            let name = self
                .state
                .locals
                .get(&place.root)
                .map_or("value", |(local, _)| local.name.as_str());
            return self.report(
                place,
                error(
                    "E1012",
                    format!("use of moved or partially moved value '{name}'"),
                    span,
                ),
            );
        }
        // Replacing the value reruns this drop, and a `ref mut` callee could replace it too.
        if place.fields.is_empty() {
            if usage == Use::Write && Some(place.root) == self.drop_root {
                return Err(error(
                    "E1012",
                    "cannot replace the whole value inside Drop.drop; read or borrow its fields instead",
                    span,
                ));
            }
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
            if !mutable || !unmarked_fields(place).is_empty() {
                return self.report(place, error(
                    "E1014",
                    "mutable access requires 'let mut' or an exclusive reference ('ref mut' or '&mut'); record fields, array elements, and list elements are immutable",
                    span,
                ));
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
                return self.report(place, error(
                    "E1014",
                    "access conflicts with a live borrow; use the reference or end its last use before moving, replacing, or borrowing exclusively",
                    span,
                ));
            }
        }
        Ok(())
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
                        .insert(self.loan(place.clone(), false, via.clone()));
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
                .insert(self.loan(place.clone(), false, via.clone()));
        }
        self.held.push(guard);
        for bound in start.iter().chain(end) {
            self.eval(bound, Use::Consume, during)?;
        }
        self.held.pop();
        let mut result = Value::default();
        for (place, via) in places {
            self.access(&place, &via, Use::MutBorrow, span)?;
            result.loans.insert(self.loan(place, true, via));
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
        let local = match expression.kind {
            E::Local(id) if whole && !self.state.aliases.contains_key(&id) => Some(id),
            _ => None,
        };
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
                value.loans.insert(self.loan(place.clone(), false, via));
            }
            if moving {
                self.state.moved.insert(place.clone());
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
                .insert(local.id, (local.clone(), value_result));
            self.state.moved.retain(|place| place.root != local.id);
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
                return Err(error(
                    "E1013",
                    "borrowed value does not live long enough to leave this block",
                    tail.span,
                ));
            }
        }
        for (id, (_, value)) in &self.state.locals {
            if !ids.contains(id)
                && value.loans.iter().any(|loan| {
                    ids.contains(&self.loans[*loan].place.root)
                        && !self.reported.contains(&self.loans[*loan].place.root)
                })
            {
                return Err(error(
                    "E1013",
                    "assignment lets a reference outlive its owner",
                    expression.span,
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
        let value = self.eval(callee, Use::Consume, &during)?;
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
            if !value.loans.is_empty() && owned_dyn_of(callee) {
                return Err(error(
                    "E1013",
                    "a dyn value without a region cannot hold borrowed data; pass an owned value to Dyn.of, or name a region, as in 'dyn Shapes.Shape {r}'",
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
            result = current;
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
        let mut locals = captures.clone();
        locals.extend(parameters.iter().cloned());
        self.copy_variables.extend(check_body(
            self.module,
            &locals,
            body,
            self.infer,
            self.closed,
            matches!(expression.ty, Type::Task(_)),
            false,
            Contract::default(),
            self.recovered,
            None,
        )?);
        for capture in captures {
            let value = TypedExpr {
                kind: E::Local(capture.id),
                ty: capture.ty.clone(),
                span: expression.span,
            };
            let value = self.eval(&value, Use::Consume, &during)?;
            if matches!(expression.ty, Type::Task(_)) && !value.loans.is_empty() {
                return Err(error(
                    "E1013",
                    "task captures cannot retain borrowed values, including borrowed function environments",
                    expression.span,
                ));
            }
            result.loans.extend(value.loans);
        }
        Ok(result)
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
                    result.loans.insert(self.loan(place, false, via));
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
                    result.loans.insert(self.loan(place, *mutable, via));
                }
            }
            E::Assign(place, value) => {
                self.shared_exclusive(place, expression.span)?;
                let value = self.eval(value, Use::Consume, &during)?;
                self.held.push(value.clone());
                for (place, via) in self.exclusive_places(place, &during)? {
                    self.access(&place, &via, Use::Write, expression.span)?;
                    if value
                        .loans
                        .iter()
                        .any(|id| self.loans[*id].place.overlaps(&place))
                    {
                        return Err(error(
                            "E1013",
                            "cannot store a reference inside its own owner",
                            expression.span,
                        ));
                    }
                    self.state.moved.retain(|moved| !moved.overlaps(&place));
                    self.state
                        .generic_moves
                        .retain(|moved, _| !moved.overlaps(&place));
                    if via.is_empty() {
                        self.state.locals.get_mut(&place.root).unwrap().1 = value.clone();
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
                let input = if *operation == crate::check::Builtin::ParallelInit {
                    None
                } else {
                    Some(arguments.len() - 1)
                };
                let base = self.held.len();
                for (index, argument) in arguments.iter().enumerate() {
                    let value = self.eval(argument, Use::Consume, &during)?;
                    if Some(index) == input {
                        let element = argument
                            .ty
                            .slice_element()
                            .expect("parallel input is a slice");
                        if element.carries_loans(&self.module.types())
                            && value.loans.iter().any(|id| {
                                !self.loans[*id].parents.is_empty()
                                    || self.external.contains(&self.loans[*id].place.root)
                            })
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
                        && !value.closed_result
                            [usize::from(*operation == crate::check::Builtin::ParallelReduce)]
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
            E::Unary(_, value) | E::Cast(value) => {
                self.eval(value, Use::Consume, &during)?;
            }
            E::Int(_)
            | E::Float(_)
            | E::String(_)
            | E::Bool(_)
            | E::Unit
            | E::Error
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

    fn merge(&mut self, other: &State) {
        self.state.moved.extend(other.moved.iter().cloned());
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
            for child in expression.children() {
                count_uses(child, counts);
            }
        }
    }
}

fn uses(expression: &TypedExpr, ids: &mut BTreeSet<usize>) {
    let mut counts = BTreeMap::new();
    count_uses(expression, &mut counts);
    ids.extend(counts.into_keys());
}
