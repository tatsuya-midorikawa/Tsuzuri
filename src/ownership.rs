use std::collections::{BTreeMap, BTreeSet};

use crate::check::{CheckedModule, Local, Type, TypedExpr, TypedExprKind as E};
use crate::diagnostic::{Diagnostic, Span};
use crate::syntax::BinaryOp;

#[path = "ownership_control.rs"]
mod control;

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
    check_functions(module, false).map(|_| ())
}

pub(crate) fn infer_copy(module: &CheckedModule) -> Result<Vec<BTreeSet<String>>, Diagnostic> {
    check_functions(module, true)
}

fn check_functions(
    module: &CheckedModule,
    infer: bool,
) -> Result<Vec<BTreeSet<String>>, Diagnostic> {
    let closed = closed_returns(module);
    let mut constraints = Vec::new();
    for function in &module.functions {
        constraints.push(check_body(
            module,
            &function.parameters,
            &function.body,
            infer,
            &closed,
            function.is_task,
        )?);
    }
    Ok(constraints)
}

fn check_body(
    module: &CheckedModule,
    parameters: &[Local],
    body: &TypedExpr,
    infer: bool,
    closed: &[bool],
    task: bool,
) -> Result<BTreeSet<String>, Diagnostic> {
    let mut checker = Checker {
        module,
        state: State::default(),
        loans: Vec::new(),
        held: Vec::new(),
        external: BTreeSet::new(),
        infer,
        copy_variables: BTreeSet::new(),
        closed,
    };
    for parameter in parameters {
        let mut value = Value::default();
        if !task && parameter.ty.carries_loans(&module.records) {
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
    let result = checker.eval(body, Use::Consume, &BTreeSet::new())?;
    for id in result.loans {
        if task || !checker.external.contains(&checker.loans[id].place.root) {
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
    }
    Ok(checker.copy_variables)
}

// Unknown results retain their input loans. Only proven closed snapshots can end a curried stage's loans.
fn closed_returns(module: &CheckedModule) -> Vec<bool> {
    fn owned(ty: &Type, module: &CheckedModule) -> bool {
        match ty {
            Type::Variable(_) | Type::Infer(_) | Type::Reference(..) | Type::Function(..) => false,
            Type::Array(element) | Type::List(element) => owned(element, module),
            Type::Tuple(elements) => elements.iter().all(|ty| owned(ty, module)),
            Type::Record(id) => module.records[*id]
                .fields
                .iter()
                .all(|(_, ty)| owned(ty, module)),
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
            | E::Method(..)
            | E::TaskRun(_)
            | E::TaskParallel(_) => true,
            E::Local(id) => locals.get(id).copied().unwrap_or(false),
            E::Lambda { captures, .. } => captures
                .iter()
                .all(|local| owned(&local.ty, module) || locals.get(&local.id) == Some(&true)),
            E::Closure(_, captures)
            | E::Array(captures)
            | E::List(captures)
            | E::Tuple(captures) => captures
                .iter()
                .all(|value| closed(value, module, known, locals)),
            E::NewArray(_, initializer)
            | E::NewList(_, initializer)
            | E::NewLiteral(initializer) => closed(initializer, module, known, locals),
            E::Record(fields) => fields
                .iter()
                .all(|(_, value)| closed(value, module, known, locals)),
            E::If {
                then_branch,
                else_branch,
                ..
            } => {
                closed(then_branch, module, known, locals)
                    && closed(else_branch, module, known, locals)
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
                    E::Function(crate::check::FunctionRef::User(id))
                    | E::GenericFunction(id, _) => {
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
            E::Field(value, _) | E::Index(value, _) => closed(value, module, known, locals),
            _ => false,
        }
    }
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

struct Checker<'a> {
    module: &'a CheckedModule,
    state: State,
    loans: Vec<Loan>,
    held: Vec<Value>,
    external: BTreeSet<usize>,
    infer: bool,
    copy_variables: BTreeSet<String>,
    closed: &'a [bool],
}

impl Checker<'_> {
    fn is_copy(&self, ty: &Type) -> bool {
        match ty {
            Type::Variable(name) => self.copy_variables.contains(name),
            Type::Array(element) | Type::List(element) => self.is_copy(element),
            Type::Tuple(elements) => elements.iter().all(|ty| self.is_copy(ty)),
            _ => ty.is_copy(&self.module.records),
        }
    }

    fn require_copy(&mut self, ty: &Type) -> bool {
        if !self.infer {
            return false;
        }
        match ty {
            Type::Variable(name) => {
                self.copy_variables.insert(name.clone());
                true
            }
            Type::Array(element) | Type::List(element) => self.require_copy(element),
            Type::Tuple(elements) => {
                let mut changed = false;
                for ty in elements {
                    changed |= self.require_copy(ty);
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
                    "mutable access requires 'let mut' or '&mut'; record fields, array elements, and list elements are immutable",
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
            E::Local(id) if self.state.aliases.contains_key(id) => {
                Ok(self.state.aliases[id].clone())
            }
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
            E::ListTail(value, _) => {
                let mut places = self.place(value, live)?;
                for (place, _) in &mut places {
                    place.fields.push(usize::MAX);
                }
                Ok(places)
            }
            E::Index(value, index) if matches!(value.ty, Type::Array(_) | Type::List(_)) => {
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
            E::Field(value, _) | E::ListTail(value, _) => Self::is_place(value),
            E::Index(value, _) => {
                matches!(value.ty, Type::Array(_) | Type::List(_)) && Self::is_place(value)
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

    fn read_places(
        &mut self,
        expression: &TypedExpr,
        usage: Use,
        places: Vec<(Place, BTreeSet<usize>)>,
    ) -> Result<Value, Diagnostic> {
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
        let mut value = Value::default();
        for (place, via) in places {
            self.revive_generic_moves(&place);
            if moving
                && (!via.is_empty()
                    || place.fields.contains(&usize::MAX)
                    || self
                        .active()
                        .iter()
                        .any(|id| self.loans[*id].place.overlaps(&place)))
            {
                self.require_copy(&expression.ty);
            }
            moving = usage == Use::Consume && !self.is_copy(&expression.ty);
            if moving && place.fields.contains(&usize::MAX) {
                return Err(error(
                    "E1012",
                    "cannot move a non-Copy value out of an array or list element; borrow it instead",
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
            if expression.ty.carries_loans(&self.module.records) {
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
            E::Block { .. } | E::Call(..) | E::Lambda { .. } => {
                self.eval_composed(expression, live)
            }
            _ => self.eval_value(expression, usage, live),
        }
    }

    fn eval_composed(
        &mut self,
        expression: &TypedExpr,
        live: &BTreeSet<usize>,
    ) -> Result<Value, Diagnostic> {
        let mut during = live.clone();
        uses(expression, &mut during);
        let mut result = Value::default();
        match &expression.kind {
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
                    self.state.moved.retain(|place| place.root != local.id);
                    self.state
                        .generic_moves
                        .retain(|place, _| place.root != local.id);
                    ids.insert(local.id);
                    let mut keep = live.clone();
                    keep.extend(future.keys().copied());
                    for (id, (_, value)) in &mut self.state.locals {
                        if !keep.contains(id) {
                            value.loans.clear();
                        }
                    }
                }
                result = self.eval(tail, Use::Consume, live)?;
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
            E::Call(callee, arguments) => {
                let start = self.held.len();
                let value = self.eval(callee, Use::Consume, &during)?;
                let known = match callee.kind {
                    E::Function(crate::check::FunctionRef::User(id))
                    | E::GenericFunction(id, _) => Some(id),
                    _ => None,
                };
                let boundary = known
                    .map(|id| self.module.functions[id].parameters.len())
                    .filter(|count| *count <= arguments.len());
                let mut current = value.clone();
                self.held.push(value);
                for (index, argument) in arguments.iter().enumerate() {
                    let value = self.eval(argument, Use::Consume, &during)?;
                    current.loans.extend(&value.loans);
                    self.held.push(value);
                    if boundary == Some(index + 1) {
                        let id = known.unwrap();
                        if self.closed[id]
                            || !callee
                                .ty
                                .after_arguments(index + 1)
                                .carries_loans(&self.module.records)
                        {
                            current = Value::default();
                        }
                        self.held.truncate(start);
                        self.held.push(current.clone());
                    }
                }
                if expression.ty.carries_loans(&self.module.records) {
                    result = current;
                }
                self.held.truncate(start);
            }
            E::Lambda {
                parameters,
                captures,
                body,
            } => {
                let mut locals = captures.clone();
                locals.extend(parameters.iter().cloned());
                self.copy_variables.extend(check_body(
                    self.module,
                    &locals,
                    body,
                    self.infer,
                    self.closed,
                    matches!(expression.ty, Type::Task(_)),
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
            }
            _ => unreachable!("composed expression kinds are checked by eval"),
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
                    self.state
                        .generic_moves
                        .retain(|moved, _| !moved.overlaps(&place));
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
            E::Closure(_, captures) => {
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
            E::TaskRun(value) | E::TaskParallel(value) => {
                self.eval(value, Use::Consume, &during)?;
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
                let callee = self.eval(right, usage, &during)?;
                self.held.pop();
                if *operator == BinaryOp::Pipe && expression.ty.carries_loans(&self.module.records)
                {
                    result = value;
                    result.loans.extend(callee.loans);
                }
            }
            E::Record(fields) => {
                let start = self.held.len();
                for (_, field) in fields {
                    let value = self.eval(field, Use::Consume, &during)?;
                    result.loans.extend(&value.loans);
                    self.held.push(value);
                }
                self.held.truncate(start);
            }
            E::Array(elements) | E::List(elements) | E::Tuple(elements) => {
                let start = self.held.len();
                for element in elements {
                    let value = self.eval(element, Use::Consume, &during)?;
                    result.loans.extend(&value.loans);
                    self.held.push(value);
                }
                self.held.truncate(start);
            }
            E::NewArray(length, initializer) | E::NewList(length, initializer) => {
                self.eval(length, Use::Consume, &during)?;
                let value = self.eval(initializer, Use::Consume, &during)?;
                if expression.ty.carries_loans(&self.module.records) {
                    result = value;
                }
            }
            E::NewLiteral(literal) => {
                result = self.eval(literal, Use::Consume, &during)?;
            }
            E::Length(value) | E::StringLength(value) => {
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
                let container = self.eval(value, Use::Read, &during)?;
                self.held.push(container.clone());
                self.eval(index, Use::Consume, &during)?;
                self.held.pop();
                if expression.ty.carries_loans(&self.module.records) {
                    result = container;
                }
            }
            E::Field(value, _) => {
                let value = self.eval(value, Use::Consume, &during)?;
                if expression.ty.carries_loans(&self.module.records) {
                    result = value;
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
            | E::Function(_)
            | E::GenericFunction(..)
            | E::Method(..)
            | E::GenericInteger(..)
            | E::GenericFloat(_) => {}
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
            }
        }
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
