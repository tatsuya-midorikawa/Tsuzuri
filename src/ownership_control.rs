use super::*;
use crate::check::PatternStep;

type LoanSummary = BTreeMap<usize, BTreeSet<(Place, bool)>>;

pub(super) struct LoopFlow {
    locals: BTreeSet<usize>,
    breaks: Vec<State>,
    continues: Vec<State>,
}

/// The states at which control may leave a `try` body (or, with `finally`, its
/// handler) for the handler that catches the exception.
pub(super) struct TryFlow {
    locals: BTreeSet<usize>,
    raises: Vec<State>,
}

impl Checker<'_> {
    /// Records a possible raise to the innermost `try` of this body: a checked
    /// operation or a re-raise. Without one, an uncaught exception traps.
    pub(super) fn raise_edge(&mut self, span: Span) -> Result<(), Diagnostic> {
        if !self.reachable {
            return Ok(());
        }
        let Some(flow) = self.try_flows.last() else {
            return Ok(());
        };
        if flow.raises.len() >= 4096 {
            return Err(error(
                "E1017",
                "too many raise points in one try; split the try body",
                span,
            ));
        }
        let locals = self
            .state
            .locals
            .keys()
            .copied()
            .filter(|id| !flow.locals.contains(id))
            .collect();
        let state = self.state.clone();
        self.finish_control_scope(&locals, &Value::default(), span)?;
        let edge = std::mem::replace(&mut self.state, state);
        self.try_flows.last_mut().unwrap().raises.push(edge);
        Ok(())
    }

    /// The handler starts from the merged raise states of the body; exceptions that
    /// leave the handler run `finally` before they reach the next `try`.
    #[inline(never)]
    pub(super) fn eval_try(
        &mut self,
        handled: &crate::check::TypedTry,
        during: &BTreeSet<usize>,
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let locals: BTreeSet<usize> = self.state.locals.keys().copied().collect();
        let entry = self.state.clone();
        self.try_flows.push(TryFlow {
            locals: locals.clone(),
            raises: Vec::new(),
        });
        let body = self.eval(&handled.body, Use::Consume, during);
        let raised = self.try_flows.pop().unwrap().raises;
        let body = body?;
        let after_body = self.state.clone();
        let body_reachable = self.reachable;
        self.state = entry;
        self.reachable = !raised.is_empty();
        if let Some((first, rest)) = raised.split_first() {
            self.state = first.clone();
            for edge in rest {
                self.merge(edge);
            }
        }
        let finally = handled.finally.as_ref();
        if finally.is_some() {
            self.try_flows.push(TryFlow {
                locals,
                raises: Vec::new(),
            });
        }
        let handler = self.eval(&handled.handler, Use::Consume, during);
        let escapes = if finally.is_some() {
            self.try_flows.pop().unwrap().raises
        } else {
            Vec::new()
        };
        let handler = handler?;
        let mut result = Value::default();
        if self.reachable {
            join(&mut result, handler);
        }
        self.merge_reachable(&after_body, body_reachable);
        if body_reachable {
            join(&mut result, body);
        }
        if let Some(finally) = finally {
            self.held.push(result.clone());
            if let Some((first, rest)) = escapes.split_first() {
                let normal = std::mem::replace(&mut self.state, first.clone());
                let normal_reachable = std::mem::replace(&mut self.reachable, true);
                for edge in rest {
                    self.merge(edge);
                }
                self.eval(finally, Use::Consume, during)?;
                self.raise_edge(span)?;
                self.state = normal;
                self.reachable = normal_reachable;
            }
            self.eval(finally, Use::Consume, during)?;
            self.held.pop();
        }
        Ok(result)
    }

    pub(super) fn merge_reachable(&mut self, other: &State, reachable: bool) {
        if reachable {
            if self.reachable {
                self.merge(other);
            } else {
                self.state = other.clone();
            }
        }
        self.reachable |= reachable;
    }

    pub(super) fn eval_loop_jump(&mut self, breaking: bool, span: Span) -> Result<(), Diagnostic> {
        if !self.reachable {
            return Ok(());
        }
        let flow = self
            .loop_flows
            .last()
            .expect("loop context checked before ownership");
        if flow.breaks.len() + flow.continues.len() >= 4096 {
            return Err(error(
                "E1017",
                "too many loop exit paths; split the loop body",
                span,
            ));
        }
        let locals = self
            .state
            .locals
            .keys()
            .copied()
            .filter(|id| !flow.locals.contains(id))
            .collect();
        let state = self.state.clone();
        self.finish_control_scope(&locals, &Value::default(), span)?;
        let edge = std::mem::replace(&mut self.state, state);
        let flow = self.loop_flows.last_mut().unwrap();
        if breaking {
            flow.breaks.push(edge);
        } else {
            flow.continues.push(edge);
        }
        self.reachable = false;
        Ok(())
    }

    fn alias_source(
        &mut self,
        local: &Local,
        source: &TypedExpr,
        live: &BTreeSet<usize>,
    ) -> Result<bool, Diagnostic> {
        let temporary = !Self::is_place(source);
        if temporary {
            let value = self.eval(source, Use::Consume, live)?;
            self.state.locals.insert(local.id, (local.clone(), value));
        } else {
            let places = self.place(source, live)?;
            let value = self.read_places(source, Use::Read, places.clone())?;
            self.state.locals.insert(local.id, (local.clone(), value));
            self.state.aliases.insert(local.id, places);
        }
        Ok(temporary)
    }

    /// Binds a pattern variable as a read-only view of the matched storage.
    /// The view is its own root, so a borrow of it ends with the arm like a
    /// borrow of an owned pattern variable; the view's loans keep the storage
    /// shared while the view or such a borrow is live.
    fn view(
        &mut self,
        local: &Local,
        projection: &TypedExpr,
        live: &BTreeSet<usize>,
    ) -> Result<(), Diagnostic> {
        let mut value = Value::default();
        for (place, via) in self.place(projection, live)? {
            self.access(&place, &via, Use::Read, projection.span)?;
            value.loans.insert(self.loans.len());
            self.loans.push(Loan {
                place,
                mutable: false,
                parents: via,
                view: Some(local.ty.clone()),
            });
        }
        let root = Place {
            root: local.id,
            fields: Vec::new(),
            through_drop: false,
        };
        self.state
            .aliases
            .insert(local.id, vec![(root, value.loans.clone())]);
        self.state.locals.insert(local.id, (local.clone(), value));
        self.state.moved.retain(|place| place.root != local.id);
        self.state
            .generic_moves
            .retain(|place, _| place.root != local.id);
        Ok(())
    }

    fn protect(&mut self, local: &Local, live: &BTreeSet<usize>) -> Result<Value, Diagnostic> {
        let expression = TypedExpr {
            kind: E::Local(local.id),
            ty: local.ty.clone(),
            span: local.span,
        };
        let mut protected = Value::default();
        for (place, via) in self.place(&expression, live)? {
            self.access(&place, &via, Use::Borrow, local.span)?;
            protected.loans.insert(self.loan(place, false, via));
        }
        Ok(protected)
    }

    fn finish_control_scope(
        &mut self,
        ids: &BTreeSet<usize>,
        value: &Value,
        span: Span,
    ) -> Result<(), Diagnostic> {
        if value
            .loans
            .iter()
            .any(|id| ids.contains(&self.loans[*id].place.root))
            || self.state.locals.iter().any(|(id, (_, value))| {
                !ids.contains(id)
                    && value
                        .loans
                        .iter()
                        .any(|loan| ids.contains(&self.loans[*loan].place.root))
            })
        {
            return Err(error(
                "E1013",
                "a borrowed value cannot outlive its pattern or iteration binding",
                span,
            ));
        }
        for id in ids {
            self.state.locals.remove(id);
            self.state.aliases.remove(id);
        }
        self.state.moved.retain(|place| !ids.contains(&place.root));
        self.state
            .generic_moves
            .retain(|place, _| !ids.contains(&place.root));
        Ok(())
    }

    fn loop_summary(&self) -> (BTreeSet<Place>, LoanSummary, BTreeSet<String>) {
        let loans = self
            .state
            .locals
            .iter()
            .map(|(id, (_, value))| {
                (
                    *id,
                    value
                        .loans
                        .iter()
                        .map(|id| {
                            let loan = &self.loans[*id];
                            (loan.place.clone(), loan.mutable)
                        })
                        .collect(),
                )
            })
            .collect();
        (self.state.moved.clone(), loans, self.copy_variables.clone())
    }

    fn check_loop(
        &mut self,
        condition: Option<&TypedExpr>,
        body: &TypedExpr,
        live: &BTreeSet<usize>,
    ) -> Result<(), Diagnostic> {
        let entry = self.state.clone();
        let entry_reachable = self.reachable;
        let ids: BTreeSet<_> = entry.locals.keys().copied().collect();
        let live: BTreeSet<_> = live.intersection(&ids).copied().collect();
        for _ in 0..=crate::syntax::MAX_NESTING {
            let before = self.loop_summary();
            self.reachable = entry_reachable;
            if let Some(condition) = condition {
                self.eval(condition, Use::Consume, &live)?;
            }
            let exit = self.state.clone();
            let exit_reachable = self.reachable;
            self.loop_flows.push(LoopFlow {
                locals: ids.clone(),
                breaks: Vec::new(),
                continues: Vec::new(),
            });
            self.eval(body, Use::Consume, &live)?;
            let mut flow = self.loop_flows.pop().unwrap();
            if self.reachable {
                self.state.moved.retain(|place| ids.contains(&place.root));
                self.state
                    .generic_moves
                    .retain(|place, _| ids.contains(&place.root));
                flow.continues.push(self.state.clone());
            }
            self.state = entry.clone();
            for edge in flow.continues {
                self.merge(&edge);
            }
            if self.loop_summary() == before {
                self.state = exit;
                self.reachable = exit_reachable;
                for edge in flow.breaks {
                    self.merge_reachable(&edge, true);
                }
                return Ok(());
            }
        }
        Err(error(
            "E1017",
            "loop ownership analysis exceeds the compiler limit; simplify loop-carried borrows",
            body.span,
        ))
    }

    pub(super) fn eval_control(
        &mut self,
        expression: &TypedExpr,
        live: &BTreeSet<usize>,
    ) -> Result<Value, Diagnostic> {
        let mut during = live.clone();
        uses(expression, &mut during);
        match &expression.kind {
            E::While { condition, body } => {
                self.check_loop(Some(condition), body, &during)?;
            }
            E::ForRange {
                local,
                start,
                step,
                finish,
                body,
            } => {
                self.eval(start, Use::Consume, &during)?;
                self.eval(step, Use::Consume, &during)?;
                self.eval(finish, Use::Consume, &during)?;
                self.state
                    .locals
                    .insert(local.id, (local.clone(), Value::default()));
                self.check_loop(None, body, &during)?;
                self.finish_control_scope(
                    &BTreeSet::from([local.id]),
                    &Value::default(),
                    expression.span,
                )?;
            }
            E::ForEach {
                owner,
                local,
                source,
                body,
            } => {
                let temporary = self.alias_source(owner, source, &during)?;
                let protected = self.protect(owner, &during)?;
                self.held.push(protected);
                let subject = TypedExpr {
                    kind: E::Local(owner.id),
                    ty: owner.ty.clone(),
                    span: source.span,
                };
                let mut places = self.place(&subject, &during)?;
                for (place, _) in &mut places {
                    place.fields.push(ELEMENT);
                }
                self.state.aliases.insert(local.id, places);
                let loans = self.state.locals[&owner.id].1.clone();
                self.state.locals.insert(local.id, (local.clone(), loans));
                self.check_loop(None, body, &during)?;
                self.held.pop();
                let mut ids = BTreeSet::from([local.id]);
                if temporary {
                    ids.insert(owner.id);
                }
                self.finish_control_scope(&ids, &Value::default(), expression.span)?;
                self.state.locals.remove(&owner.id);
                self.state.aliases.remove(&owner.id);
            }
            E::Match { local, value, arms } => {
                return self.eval_match(local, value, arms, live, &during, expression.span);
            }
            _ => unreachable!("control expression kinds checked by eval"),
        }
        Ok(Value::default())
    }

    fn eval_match(
        &mut self,
        subject: &Local,
        matched: &TypedExpr,
        arms: &[crate::check::TypedMatchArm],
        live: &BTreeSet<usize>,
        during: &BTreeSet<usize>,
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let temporary = self.alias_source(subject, matched, during)?;
        let mut pending = self.state.clone();
        let mut pending_reachable = self.reachable;
        let mut exits: Option<State> = None;
        let mut result = Value::default();
        for arm in arms {
            let mut pattern_pending = pending.clone();
            let mut pattern_pending_reachable = pending_reachable;
            let mut failed = pending.clone();
            let mut failed_reachable = pending_reachable;
            for alternative in &arm.alternatives {
                self.state = pattern_pending.clone();
                self.reachable = pattern_pending_reachable;
                let protected = self.protect(subject, during)?;
                self.held.push(protected);
                let mut temporaries = BTreeSet::new();
                for step in &alternative.steps {
                    match step {
                        PatternStep::Bind(local, expression) => {
                            let value = self.eval(expression, Use::Consume, during)?;
                            self.state.locals.insert(local.id, (local.clone(), value));
                            self.state.moved.retain(|place| place.root != local.id);
                            self.state
                                .generic_moves
                                .retain(|place, _| place.root != local.id);
                            temporaries.insert(local.id);
                        }
                        PatternStep::Test(condition) => {
                            self.eval(condition, Use::Consume, during)?;
                            let success = self.state.clone();
                            let success_reachable = self.reachable;
                            self.finish_control_scope(&temporaries, &Value::default(), span)?;
                            self.merge_reachable(&pattern_pending, pattern_pending_reachable);
                            pattern_pending = self.state.clone();
                            pattern_pending_reachable = self.reachable;
                            self.state = success;
                            self.reachable = success_reachable;
                        }
                    }
                }
                let mut ids = BTreeSet::new();
                for (local, projection) in &alternative.bindings {
                    self.alias_source(local, projection, during)?;
                    ids.insert(local.id);
                }
                if let Some(guard) = &arm.guard {
                    self.eval(guard, Use::Consume, during)?;
                }
                self.held.pop();
                for id in &ids {
                    self.state.locals.remove(id);
                    self.state.aliases.remove(id);
                }
                let success = self.state.clone();
                let success_reachable = self.reachable;
                self.finish_control_scope(&temporaries, &Value::default(), span)?;
                self.merge_reachable(&failed, failed_reachable);
                failed = self.state.clone();
                failed_reachable = self.reachable;
                self.state = success;
                self.reachable = success_reachable;
                // Pattern variables acquire their values only after the guard succeeds.
                if !subject.ty.carries_loans(&self.module.types()) {
                    self.state
                        .locals
                        .get_mut(&subject.id)
                        .unwrap()
                        .1
                        .loans
                        .clear();
                }
                for (local, projection) in &alternative.bindings {
                    if arm.borrowed.contains(&local.id)
                        && !self.is_copy(&local.ty)
                        && !local.ty.carries_loans(&self.module.types())
                    {
                        self.view(local, projection, during)?;
                        continue;
                    }
                    // The generated code copies through the binding's own name (A15).
                    self.note_copy(projection, local.span, read_kind(projection), false);
                    let copies = self.copies.take();
                    let value = self.eval(projection, Use::Consume, during);
                    self.copies = copies;
                    let value = value?;
                    self.state.locals.insert(local.id, (local.clone(), value));
                    self.state.moved.retain(|place| place.root != local.id);
                    self.state
                        .generic_moves
                        .retain(|place, _| place.root != local.id);
                }
                let value = self.eval(&arm.body, Use::Consume, live)?;
                ids.extend(temporaries);
                self.finish_control_scope(&ids, &value, span)?;
                if self.reachable {
                    join(&mut result, value);
                    if let Some(other) = &exits {
                        self.merge(other);
                    }
                    exits = Some(self.state.clone());
                }
            }
            self.state = pattern_pending;
            self.reachable = pattern_pending_reachable;
            self.merge_reachable(&failed, failed_reachable);
            pending = self.state.clone();
            pending_reachable = self.reachable;
        }
        self.reachable = exits.is_some();
        self.state = exits.unwrap_or(pending);
        if temporary {
            self.finish_control_scope(&BTreeSet::from([subject.id]), &result, span)?;
        } else {
            self.state.locals.remove(&subject.id);
            self.state.aliases.remove(&subject.id);
        }
        Ok(result)
    }
}
