use std::collections::{BTreeMap, BTreeSet};

use crate::check::{CheckedModule, Local, Type, TypedExpr, TypedExprKind as E};
use crate::diagnostic::{Diagnostic, Span};
use crate::syntax::BinaryOp;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Place {
    root: usize,
    fields: Vec<usize>,
}

impl Place {
    fn overlaps(&self, other: &Self) -> bool {
        self.root == other.root && self.fields.iter().zip(&other.fields).all(|(a, b)| a == b)
    }
}

#[derive(Clone, Default)]
struct Value {
    loans: BTreeSet<usize>,
}

struct Loan {
    place: Place,
    mutable: bool,
    parents: BTreeSet<usize>,
}

#[derive(Clone, Default)]
struct State {
    locals: BTreeMap<usize, (Local, Value)>,
    moved: BTreeSet<Place>,
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
    for function in &module.functions {
        let mut checker = Checker {
            module,
            state: State::default(),
            loans: Vec::new(),
            held: Vec::new(),
            external: BTreeSet::new(),
        };
        for parameter in &function.parameters {
            let mut value = Value::default();
            if parameter.ty.contains_reference() {
                let root = usize::MAX - parameter.id;
                checker.external.insert(root);
                let mutable = matches!(parameter.ty, Type::Reference(_, true));
                value.loans.insert(checker.loan(
                    Place {
                        root,
                        fields: Vec::new(),
                    },
                    mutable,
                    BTreeSet::new(),
                ));
            }
            checker
                .state
                .locals
                .insert(parameter.id, (parameter.clone(), value));
        }
        let result = checker.eval(&function.body, Use::Consume, &BTreeSet::new())?;
        for id in result.loans {
            if !checker.external.contains(&checker.loans[id].place.root) {
                return Err(error(
                    "E1013",
                    "cannot return a reference to a local value",
                    function.body.span,
                ));
            }
        }
    }
    Ok(())
}

fn error(code: &'static str, message: impl Into<String>, span: Span) -> Diagnostic {
    Diagnostic::new(code, message, span)
}

struct Checker<'a> {
    module: &'a CheckedModule,
    state: State,
    loans: Vec<Loan>,
    held: Vec<Value>,
    external: BTreeSet<usize>,
}

impl Checker<'_> {
    fn loan(&mut self, place: Place, mutable: bool, parents: BTreeSet<usize>) -> usize {
        let id = self.loans.len();
        self.loans.push(Loan {
            place,
            mutable,
            parents,
        });
        id
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

    fn access(
        &self,
        place: &Place,
        via: &BTreeSet<usize>,
        usage: Use,
        span: Span,
    ) -> Result<(), Diagnostic> {
        if usage != Use::Write && self.state.moved.iter().any(|moved| moved.overlaps(place)) {
            let name = self
                .state
                .locals
                .get(&place.root)
                .map_or("value", |(local, _)| local.name.as_str());
            return Err(error(
                "E1012",
                format!("use of moved or partially moved value '{name}'"),
                span,
            ));
        }
        if matches!(usage, Use::MutBorrow | Use::Write) {
            let mutable = if via.is_empty() {
                self.state
                    .locals
                    .get(&place.root)
                    .is_some_and(|(local, _)| local.mutable)
            } else {
                via.iter().all(|id| self.loans[*id].mutable)
            };
            if !mutable || !place.fields.is_empty() {
                return Err(error(
                    "E1014",
                    "mutable access requires 'let mut' or '&mut'; record fields and array elements are immutable",
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
                return Err(error(
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
            E::Local(id) => Ok(vec![(
                Place {
                    root: *id,
                    fields: Vec::new(),
                },
                BTreeSet::new(),
            )]),
            E::Field(value, field) => {
                let mut places = self.place(value, live)?;
                for (place, _) in &mut places {
                    place.fields.push(*field);
                }
                Ok(places)
            }
            E::Index(value, index) if matches!(value.ty, Type::Array(..)) => {
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
                    // Array loans conservatively cover every element.
                    place.fields.push(usize::MAX);
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

    fn is_place(expression: &TypedExpr) -> bool {
        match &expression.kind {
            E::Local(_) | E::Dereference(_) => true,
            E::Field(value, _) | E::Index(value, _) => Self::is_place(value),
            _ => false,
        }
    }

    fn read_place(
        &mut self,
        expression: &TypedExpr,
        usage: Use,
        live: &BTreeSet<usize>,
    ) -> Result<Value, Diagnostic> {
        let copy = expression.ty.is_copy(&self.module.records);
        let moving = usage == Use::Consume && !copy;
        if moving && matches!(expression.kind, E::Index(..)) {
            return Err(error(
                "E1012",
                "cannot move a non-Copy element out of an array; borrow the element instead",
                expression.span,
            ));
        }
        let places = self.place(expression, live)?;
        let mut value = Value::default();
        for (place, via) in places {
            if moving && place.fields.contains(&usize::MAX) {
                return Err(error(
                    "E1012",
                    "cannot move a non-Copy value out of an array element; borrow it instead",
                    expression.span,
                ));
            }
            if moving && !via.is_empty() {
                return Err(error(
                    "E1012",
                    "cannot move a non-Copy value out of a reference",
                    expression.span,
                ));
            }
            self.access(
                &place,
                &via,
                if moving { Use::Consume } else { Use::Read },
                expression.span,
            )?;
            if expression.ty.contains_reference() {
                if let Some((_, stored)) = self.state.locals.get(&place.root) {
                    value.loans.extend(stored.loans.iter().copied());
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
        if Self::is_place(expression)
            && !matches!(expression.kind, E::Index(ref value, _) if value.ty == Type::String)
        {
            return self.read_place(expression, usage, live);
        }
        let usage = Use::Consume;
        let mut during = live.clone();
        uses(expression, &mut during);
        let mut result = Value::default();
        match &expression.kind {
            E::Borrow(value, mutable) => {
                for (place, via) in self.place(value, &during)? {
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
                let value = self.eval(value, Use::Consume, &during)?;
                self.held.push(value.clone());
                for (place, via) in self.place(place, &during)? {
                    self.access(&place, &via, Use::Write, expression.span)?;
                    if value
                        .loans
                        .iter()
                        .any(|id| self.loans[*id].place.root == place.root)
                    {
                        return Err(error(
                            "E1013",
                            "cannot store a reference inside its own owner",
                            expression.span,
                        ));
                    }
                    self.state.moved.retain(|moved| !moved.overlaps(&place));
                    if via.is_empty() {
                        self.state.locals.get_mut(&place.root).unwrap().1 = value.clone();
                    } else if expression.ty.contains_reference() || !value.loans.is_empty() {
                        return Err(error(
                            "E1013",
                            "assigning borrowed values through references requires explicit lifetimes",
                            expression.span,
                        ));
                    }
                }
                self.held.pop();
            }
            E::Block {
                bindings,
                result: tail,
            } => {
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
                    ids.insert(local.id);
                    let mut keep = live.clone();
                    keep.extend(future.keys().copied());
                    for (id, (_, value)) in &mut self.state.locals {
                        if !keep.contains(id) {
                            value.loans.clear();
                        }
                    }
                }
                result = self.eval(tail, usage, live)?;
                for id in &result.loans {
                    if ids.contains(&self.loans[*id].place.root) {
                        return Err(error(
                            "E1013",
                            "borrowed value does not live long enough to leave this block",
                            tail.span,
                        ));
                    }
                }
                for (id, (_, value)) in &self.state.locals {
                    if !ids.contains(id)
                        && value
                            .loans
                            .iter()
                            .any(|loan| ids.contains(&self.loans[*loan].place.root))
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
            }
            E::If {
                condition,
                then_branch,
                else_branch,
            } => {
                self.eval(condition, Use::Consume, &during)?;
                let before = self.state.clone();
                let then_value = self.eval(then_branch, usage, live)?;
                let then_state = self.state.clone();
                self.state = before;
                let else_value = self.eval(else_branch, usage, live)?;
                self.merge(&then_state);
                result.loans.extend(then_value.loans);
                result.loans.extend(else_value.loans);
            }
            E::Binary(BinaryOp::And | BinaryOp::Or, left, right) => {
                self.eval(left, Use::Consume, &during)?;
                let before = self.state.clone();
                self.eval(right, Use::Consume, &during)?;
                self.merge(&before);
            }
            E::Call(callee, arguments) => {
                self.eval(callee, Use::Consume, &during)?;
                let start = self.held.len();
                for argument in arguments {
                    let value = self.eval(argument, Use::Consume, &during)?;
                    if expression.ty.contains_reference() {
                        result.loans.extend(&value.loans);
                    }
                    self.held.push(value);
                }
                self.held.truncate(start);
            }
            E::Binary(operator, left, right) => {
                let usage = if left.ty == Type::String
                    && matches!(operator, BinaryOp::Equal | BinaryOp::NotEqual)
                {
                    Use::Read
                } else {
                    Use::Consume
                };
                let value = self.eval(left, usage, &during)?;
                self.held.push(value.clone());
                self.eval(right, usage, &during)?;
                self.held.pop();
                if *operator == BinaryOp::Pipe && expression.ty.contains_reference() {
                    result = value;
                }
            }
            E::Record(fields) => {
                let start = self.held.len();
                for (_, field) in fields {
                    let value = self.eval(field, Use::Consume, &during)?;
                    self.held.push(value);
                }
                self.held.truncate(start);
            }
            E::Array(elements) => {
                let start = self.held.len();
                for element in elements {
                    let value = self.eval(element, Use::Consume, &during)?;
                    result.loans.extend(&value.loans);
                    self.held.push(value);
                }
                self.held.truncate(start);
            }
            E::Length(value, _) | E::StringLength(value) => {
                self.eval(value, Use::Read, &during)?;
            }
            E::Index(value, index) => {
                if !expression.ty.is_copy(&self.module.records) {
                    return Err(error(
                        "E1012",
                        "cannot move a non-Copy array element; borrow it instead",
                        expression.span,
                    ));
                }
                let container = self.eval(value, Use::Read, &during)?;
                self.held.push(container.clone());
                self.eval(index, Use::Consume, &during)?;
                self.held.pop();
                if expression.ty.contains_reference() {
                    result = container;
                }
            }
            E::Field(value, _) => {
                result = self.eval(value, Use::Consume, &during)?;
            }
            E::Unary(_, value) | E::Cast(value) => {
                self.eval(value, Use::Consume, &during)?;
            }
            E::Int(_) | E::Float(_) | E::String(_) | E::Bool(_) | E::Unit | E::Function(_) => {}
            E::Local(_) | E::Dereference(_) => unreachable!("places handled above"),
        }
        Ok(result)
    }

    fn merge(&mut self, other: &State) {
        self.state.moved.extend(other.moved.iter().cloned());
        for (id, (_, value)) in &mut self.state.locals {
            if let Some((_, other_value)) = other.locals.get(id) {
                value.loans.extend(&other_value.loans);
            }
        }
    }
}

fn count_uses(expression: &TypedExpr, counts: &mut BTreeMap<usize, usize>) {
    match &expression.kind {
        E::Local(id) => {
            *counts.entry(*id).or_default() += 1;
        }
        E::Unary(_, value)
        | E::Borrow(value, _)
        | E::Dereference(value)
        | E::Cast(value)
        | E::Field(value, _)
        | E::Length(value, _)
        | E::StringLength(value) => count_uses(value, counts),
        E::Binary(_, left, right) | E::Assign(left, right) | E::Index(left, right) => {
            count_uses(left, counts);
            count_uses(right, counts);
        }
        E::Call(callee, arguments) => {
            count_uses(callee, counts);
            for argument in arguments {
                count_uses(argument, counts);
            }
        }
        E::If {
            condition,
            then_branch,
            else_branch,
        } => {
            count_uses(condition, counts);
            count_uses(then_branch, counts);
            count_uses(else_branch, counts);
        }
        E::Block { bindings, result } => {
            for (_, value) in bindings {
                count_uses(value, counts);
            }
            count_uses(result, counts);
        }
        E::Record(fields) => {
            for (_, value) in fields {
                count_uses(value, counts);
            }
        }
        E::Array(values) => {
            for value in values {
                count_uses(value, counts);
            }
        }
        _ => {}
    }
}

fn uses(expression: &TypedExpr, ids: &mut BTreeSet<usize>) {
    let mut counts = BTreeMap::new();
    count_uses(expression, &mut counts);
    ids.extend(counts.into_keys());
}
