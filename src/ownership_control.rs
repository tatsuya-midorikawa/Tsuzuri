use super::*;
use crate::check::PatternStep;

type LoanSummary = BTreeMap<usize, BTreeSet<(Place, bool)>>;

impl Checker<'_> {
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
        let ids: BTreeSet<_> = entry.locals.keys().copied().collect();
        let live: BTreeSet<_> = live.intersection(&ids).copied().collect();
        for _ in 0..=crate::syntax::MAX_NESTING {
            let before = self.loop_summary();
            if let Some(condition) = condition {
                self.eval(condition, Use::Consume, &live)?;
            }
            let exit = self.state.clone();
            self.eval(body, Use::Consume, &live)?;
            self.state.moved.retain(|place| ids.contains(&place.root));
            self.state
                .generic_moves
                .retain(|place, _| ids.contains(&place.root));
            self.merge(&entry);
            if self.loop_summary() == before {
                self.state = exit;
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
        let mut exits: Option<State> = None;
        let mut result = Value::default();
        for arm in arms {
            let mut pattern_pending = pending.clone();
            let mut failed = pending.clone();
            for alternative in &arm.alternatives {
                self.state = pattern_pending.clone();
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
                            self.finish_control_scope(&temporaries, &Value::default(), span)?;
                            self.merge(&pattern_pending);
                            pattern_pending = self.state.clone();
                            self.state = success;
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
                self.finish_control_scope(&temporaries, &Value::default(), span)?;
                self.merge(&failed);
                failed = self.state.clone();
                self.state = success;
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
                    let value = self.eval(projection, Use::Consume, during)?;
                    self.state.locals.insert(local.id, (local.clone(), value));
                    self.state.moved.retain(|place| place.root != local.id);
                    self.state
                        .generic_moves
                        .retain(|place, _| place.root != local.id);
                }
                let value = self.eval(&arm.body, Use::Consume, live)?;
                ids.extend(temporaries);
                self.finish_control_scope(&ids, &value, span)?;
                result.loans.extend(value.loans);
                if let Some(other) = &exits {
                    self.merge(other);
                }
                exits = Some(self.state.clone());
            }
            self.state = pattern_pending;
            self.merge(&failed);
            pending = self.state.clone();
        }
        self.state = exits.expect("the parser requires match arms");
        if temporary {
            self.finish_control_scope(&BTreeSet::from([subject.id]), &result, span)?;
        } else {
            self.state.locals.remove(&subject.id);
            self.state.aliases.remove(&subject.id);
        }
        Ok(result)
    }
}
