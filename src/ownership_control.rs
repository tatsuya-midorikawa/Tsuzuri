use super::*;
use crate::check::PatternStep;

/// What the fixed point of a loop compares of a loan (`Checker::loop_summary`): its place, whether
/// it is mutable, and the flags `LOAN_*` of what the checks read of it besides. Its number is no
/// part of it: an iteration makes its loans again with fresh numbers, so a summary by number would
/// never settle.
type LoanKey = (Place, bool, u8);

/// A31: the flags of a `LoanKey`. The loan stands for a share of a block (`Loan::share`), belongs
/// to a pattern view (`Loan::view`), is held only as what the value read through a reference
/// parameter (`Value::through`, by the value that holds it), or is in `Checker::exact_loans`,
/// `Checker::ordered_loans` or `Checker::holding_loans`.
const LOAN_SHARE: u8 = 1;
const LOAN_VIEW: u8 = 2;
const LOAN_THROUGH: u8 = 4;
const LOAN_EXACT: u8 = 8;
const LOAN_ORDERED: u8 = 16;
const LOAN_HOLDING: u8 = 32;

/// A31: how many iterations of one loop may each borrow a place that leaves its type
/// (`Checker::leaves_type`) deeper than every such place that the iterations before them
/// borrowed, before `Checker::check_loop` stops with `E1017`. The first iteration never counts,
/// and an iteration counts only when the loop does not settle with it, so a loop that settles
/// within `DEEPER_ITERATIONS` + 1 iterations never meets this limit. Neither does a loop whose
/// places get deeper only along their types: without a union, a type nests only so deep (a chain
/// of references, each to a field of the target of the next, settles when the chain ends). In
/// the loops that keep borrowing deeper (A34 `loopplace`), a store into a function value that
/// owns a cell adds the borrows of a reference parameter's target to the parameter, its
/// dereference (`Checker::place`) makes the places of the next iteration from them one step
/// deeper (`p.cell.cell.cell`, the field `cell` of an array from the third iteration on), and the
/// loans that the locals hold double with each iteration, so that the check would not reach the
/// limit of `MAX_NESTING` + 1 iterations in practical time.
const DEEPER_ITERATIONS: usize = 9;

/// The help of the `E1017` of `DEEPER_ITERATIONS`: the change that lets the loops of A34
/// `loopplace` settle.
const LOOP_DEEPER_HELP: &str = "the loop's borrows reach a deeper place in every iteration and \
     never settle; when the loop passes a borrow of a reference parameter's target to a function \
     value that owns a cell (a lambda that captures an `Arc` by value), capture a reference to the \
     cell instead, such as `let cell = Arc.get (ref sink)`";

/// What `LoopSummary` keeps of the value of a local: its loans, and those of each of its region
/// slots (`Value::regions`).
type LocalSummary = (BTreeSet<LoanKey>, Option<Vec<BTreeSet<LoanKey>>>);

/// What the fixed point of a loop compares of the state where an iteration starts
/// (`Checker::check_loop`). The check returns the exit state of the iteration whose summary did
/// not change, which starts from what the iterations before it left: a fact of the state that an
/// iteration changes and the summary leaves out would be lost after the loop, and the body would
/// not be checked again under it. So it holds the facts of `State` that a check reads, but for the
/// three left out below, each loan by its `LoanKey` and each move by its place (the spans of the
/// moves would keep it from settling), and the facts of the body that only grow and that a later
/// write or store reads where it happens (`Checker::handed`, `Checker::exports`): the next
/// iteration's writes and stores meet those of this one. A31, Copilot's review 2:
/// `kept_elsewhere`, `contents_elsewhere`, `moved_always`, `generic_moves`, the regions and the
/// flags of the loans, and `exports` were missing (a copy of a target's contents or a borrow of it
/// stored elsewhere in a loop was lost, and a later `deref p = v` ended the borrow). The aliases
/// are not in it: only the bindings of a match, a `for` and a pattern view add them, for their own
/// locals, and the end of their scopes takes them out (`finish_control_scope`, `eval_match`,
/// `eval_control`), so an iteration leaves those of the locals before the loop as they were.
/// `Value::closed_result` is not in it either: `State::merge` keeps the first state's, so the
/// summary of a merged state never shows a change. Nor are the parents of a loan
/// (`Loan::parents`, which `active`, `retain_live_loans`, `held_keeps`, `holders` and the move of
/// a reborrowed exclusive reference read): with them a chain of reborrows such as
/// `cur = ref mut (deref cur).next` would make the key grow with each iteration and the summary
/// would never settle, and a loan of the same place and flags under other parents would conflict
/// with the exclusive borrow of its parent.
#[derive(PartialEq, Eq)]
struct LoopSummary {
    moved: BTreeSet<Place>,
    moved_always: BTreeSet<usize>,
    generic_moves: BTreeSet<Place>,
    locals: BTreeMap<usize, LocalSummary>,
    copy_variables: BTreeSet<String>,
    kept: BTreeSet<LoanKey>,
    kept_elsewhere: BTreeSet<LoanKey>,
    contents_elsewhere: BTreeSet<usize>,
    block_shares: BTreeMap<usize, BTreeSet<usize>>,
    handed: usize,
    exports: usize,
}

pub(super) struct LoopFlow<'a> {
    locals: BTreeSet<usize>,
    breaks: Vec<State<'a>>,
    continues: Vec<State<'a>>,
}

/// The states at which control may leave a `try` body (or, with `finally`, its
/// handler) for the handler that catches the exception.
pub(super) struct TryFlow<'a> {
    locals: BTreeSet<usize>,
    raises: Vec<State<'a>>,
}

/// The bindings of an alternative of a match arm whose guard and body were checked, the state
/// after its pattern steps, and whether that state was reachable (`eval_match`).
type Checked<'b, 'a> = (&'b [(Local, TypedExpr)], State<'a>, bool);

/// Whether two alternatives of an arm bind the same locals, in the same order, to the same
/// places: projections that `same_projection` finds equal.
fn same_bindings(left: &[(Local, TypedExpr)], right: &[(Local, TypedExpr)]) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .all(|((a, x), (b, y))| a.id == b.id && a.span == b.span && same_projection(x, y))
}

/// Whether two binding projections are the same place, with the same types and spans. Only the
/// kinds of a place compare equal; any other expression is never the same as another. These are
/// every kind that a pattern projects (`pattern_alternatives`): the matched local, a field of a
/// record or a part of a tuple, an element of an array or a list (its index an integer literal),
/// the tail of a list, the payload of a case and a dereference.
fn same_projection(left: &TypedExpr, right: &TypedExpr) -> bool {
    if left.ty != right.ty || left.span != right.span {
        return false;
    }
    match (&left.kind, &right.kind) {
        (E::Local(a), E::Local(b)) => a == b,
        (E::Field(a, i), E::Field(b, j)) | (E::ListTail(a, i), E::ListTail(b, j)) => {
            i == j && same_projection(a, b)
        }
        // An element of an array or a list pattern and the head of a cons pattern. Evaluating
        // the index reads nothing. A computed index is never a pattern's, so it stays unequal.
        (E::Index(a, i), E::Index(b, j)) => {
            matches!((&i.kind, &j.kind), (E::Int(m), E::Int(n)) if m == n)
                && i.ty == j.ty
                && i.span == j.span
                && same_projection(a, b)
        }
        (
            E::UnionPayload {
                value: a,
                case_id: i,
            },
            E::UnionPayload {
                value: b,
                case_id: j,
            },
        ) => i == j && same_projection(a, b),
        (E::Dereference(a), E::Dereference(b)) => same_projection(a, b),
        _ => false,
    }
}

impl<'a> Checker<'a, '_> {
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
                DiagnosticKind::TooManyRaisePoints,
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
        // A31: the locals of the scopes that the exception leaves are dropped here.
        self.check_exit_drop_order(&locals, span)?;
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

    pub(super) fn merge_reachable(&mut self, other: &State<'a>, reachable: bool) {
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
                DiagnosticKind::TooManyLoopExits,
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
        // A31: the locals of the scopes that the jump leaves are dropped here, whatever the code
        // after it puts into them.
        self.check_exit_drop_order(&locals, span)?;
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
            // A31: the temporary lives in the local until the match or loop ends, where it is
            // dropped: the binding applies the drop hook, as a `let` does.
            let mark = self.hidden_log.len();
            self.bound_next = true;
            let mut value = self.eval(source, Use::Consume, live)?;
            self.absorb_hidden(mark, &mut value, &local.ty, Some(source));
            self.drop_stores(&value, &local.ty, source.span, None, Some(source));
            self.state
                .locals
                .insert(local.id, (self.declarations[&local.id], value));
            self.drop_keeps_self(local.id, &local.ty, source.span);
        } else {
            let places = self.place(source, live)?;
            let value = self.read_places(source, Use::Read, places.clone())?;
            self.state
                .locals
                .insert(local.id, (self.declarations[&local.id], value));
            self.state.aliases.insert(local.id, places);
        }
        self.note_binding(local.id);
        Ok(temporary)
    }

    /// Binds a pattern variable as a read-only view of the matched storage.
    /// The view is its own root, so a borrow of it ends with the arm like a
    /// borrow of an owned pattern variable; the view's loans keep the storage
    /// shared while the view or such a borrow is live. A31: a borrow of it that
    /// goes where the caller's data or a hidden environment reaches holds the
    /// view's loans instead (`Checker::expand_views`).
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
                span: projection.span,
                share: false,
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
        self.state
            .locals
            .insert(local.id, (self.declarations[&local.id], value));
        self.views.insert(local.id);
        self.state.moved.retain(|place, _| place.root != local.id);
        self.state.moved_always.remove(&local.id);
        self.state
            .generic_moves
            .retain(|place, _| place.root != local.id);
        self.note_binding(local.id);
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
            protected
                .loans
                .insert(self.loan(place, false, via, local.span));
        }
        Ok(protected)
    }

    fn finish_control_scope(
        &mut self,
        ids: &BTreeSet<usize>,
        value: &Value,
        span: Span,
    ) -> Result<(), Diagnostic> {
        let escaping = value
            .loans
            .iter()
            .find(|id| ids.contains(&self.loans[**id].place.root))
            .or_else(|| {
                self.state
                    .locals
                    .iter()
                    .filter(|(id, _)| !ids.contains(id))
                    .flat_map(|(_, (_, value))| value.loans.iter())
                    .find(|loan| ids.contains(&self.loans[**loan].place.root))
            });
        if let Some(loan) = escaping {
            return Err(self
                .escape(
                    *loan,
                    span,
                    "'{name}' is bound here and ends with this arm or iteration",
                    error(DiagnosticKind::BorrowOutlivesBinding,
                        "a borrowed value cannot outlive its pattern or iteration binding",
                        span,
                    ),
                )
                .with_help(
                    "move or copy the value out of the pattern or loop instead of keeping a reference to it",
                ));
        }
        for id in ids {
            self.state.locals.remove(id);
            self.state.aliases.remove(id);
        }
        self.state
            .moved
            .retain(|place, _| !ids.contains(&place.root));
        self.state.moved_always.retain(|id| !ids.contains(id));
        self.state
            .generic_moves
            .retain(|place, _| !ids.contains(&place.root));
        Ok(())
    }

    /// The `LoanKey` of `loan`, held as what the holder read through a reference parameter when
    /// `through`.
    fn loan_key(&self, loan: usize, through: bool) -> LoanKey {
        let held = &self.loans[loan];
        let mut flags = 0;
        for (set, flag) in [
            (held.share, LOAN_SHARE),
            (held.view.is_some(), LOAN_VIEW),
            (through, LOAN_THROUGH),
            (self.exact_loans.contains(&loan), LOAN_EXACT),
            (self.ordered_loans.contains(&loan), LOAN_ORDERED),
            (self.holding_loans.contains(&loan), LOAN_HOLDING),
        ] {
            if set {
                flags |= flag;
            }
        }
        (held.place.clone(), held.mutable, flags)
    }

    fn loop_summary(&self) -> LoopSummary {
        let keys = |loans: &BTreeSet<usize>, value: Option<&Value>| -> BTreeSet<LoanKey> {
            loans
                .iter()
                .map(|loan| {
                    let through = value.is_some_and(|value| value.through.contains(loan));
                    self.loan_key(*loan, through)
                })
                .collect()
        };
        let locals = self
            .state
            .locals
            .iter()
            .map(|(id, (_, value))| {
                let regions = value
                    .regions
                    .as_ref()
                    .map(|slots| slots.iter().map(|slot| keys(slot, Some(value))).collect());
                (*id, (keys(&value.loans, Some(value)), regions))
            })
            .collect();
        LoopSummary {
            moved: self.state.moved.keys().cloned().collect(),
            moved_always: self.state.moved_always.clone(),
            generic_moves: self.state.generic_moves.keys().cloned().collect(),
            locals,
            copy_variables: self.copy_variables.clone(),
            kept: keys(&self.state.kept, None),
            kept_elsewhere: keys(&self.state.kept_elsewhere, None),
            contents_elsewhere: self.state.contents_elsewhere.clone(),
            block_shares: self.state.block_shares.clone(),
            handed: self.handed.len(),
            exports: self.exports.len(),
        }
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
        let limit = || {
            error(
                DiagnosticKind::LoopOwnershipLimit,
                "loop ownership analysis exceeds the compiler limit; simplify loop-carried borrows",
                body.span,
            )
        };
        // The deepest place that leaves its type that an iteration so far borrowed, and how many
        // iterations borrowed one deeper than every iteration before them (`DEEPER_ITERATIONS`).
        let mut deepest: Option<usize> = None;
        let mut deeper = 0;
        for iteration in 0..=crate::syntax::MAX_NESTING {
            let made = self.loans.len();
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
                self.state
                    .moved
                    .retain(|place, _| ids.contains(&place.root));
                self.state.moved_always.retain(|id| ids.contains(id));
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
            let leaving = self.loans[made..]
                .iter()
                .map(|loan| &loan.place)
                .filter(|place| deepest.is_none_or(|known| place.fields.len() > known))
                .filter(|place| self.leaves_type(place))
                .map(|place| place.fields.len())
                .max();
            if let Some(depth) = leaving {
                if iteration > 0 {
                    deeper += 1;
                    if deeper == DEEPER_ITERATIONS {
                        return Err(limit().with_help(LOOP_DEEPER_HELP));
                    }
                }
                deepest = Some(depth);
            }
        }
        Err(limit())
    }

    /// A31: whether `place` takes a step that the type it has reached does not have
    /// (`DEEPER_ITERATIONS`): a field of a value that is no record or tuple, or that its record or
    /// tuple lacks, or an element of a value that is no collection. Each step reads through `Rc`
    /// and `Arc`, as `place_type` does. What follows the payload of a union (whose case the place
    /// does not name) or a type whose structure the check does not know (a type variable, an
    /// application of one, a `dyn` value) is not known to leave its type, nor is a place whose
    /// root has no type that the check knows (static data). The slot marker of a parameter's
    /// region slot is no step (`unmarked_fields`).
    fn leaves_type(&self, place: &Place) -> bool {
        let external = self.external.contains(&place.root);
        let root = if external {
            usize::MAX - place.root
        } else {
            place.root
        };
        let Some(local) = self.declarations.get(&root) else {
            return false;
        };
        let mut current = match &local.ty {
            Type::Reference(target, _) if external => (**target).clone(),
            ty => ty.clone(),
        };
        let types = self.module.types();
        for &field in unmarked_fields(place) {
            if field == PAYLOAD {
                return false;
            }
            let mut ty = without_views(&current);
            while let Type::Shared(
                inner,
                crate::check::SharedKind::Rc | crate::check::SharedKind::Arc,
            ) = ty
            {
                ty = *inner;
            }
            current = match ty {
                Type::Record(id, arguments) if field < types.records[id].fields.len() => {
                    types.record_field(id, &arguments, field)
                }
                Type::Tuple(elements) if field < elements.len() => elements[field].clone(),
                Type::Array(inner)
                | Type::ArrayView(inner)
                | Type::List(inner)
                | Type::Vec(inner)
                | Type::FixedArray(inner, _)
                    if field == ELEMENT =>
                {
                    *inner
                }
                Type::Error
                | Type::Variable(_)
                | Type::Infer(_)
                | Type::Partial(_)
                | Type::Application(..)
                | Type::Dyn(_) => return false,
                _ => return true,
            };
        }
        false
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
                ..
            } => {
                self.eval(start, Use::Consume, &during)?;
                self.eval(step, Use::Consume, &during)?;
                self.eval(finish, Use::Consume, &during)?;
                self.state
                    .locals
                    .insert(local.id, (self.declarations[&local.id], Value::default()));
                self.note_binding(local.id);
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
                // A code unit of a string is a value of its own, not a place in the string: a borrow of
                // `local` ends with the iteration, like one of a range variable or of a pattern variable
                // (the loans of the owner below keep the string shared meanwhile). The units of a string
                // of `TSUZURI_STRINGS=compact` may be bytes, which have no 16-bit unit in memory to refer to.
                if owner.ty != Type::String {
                    let mut places = self.place(&subject, &during)?;
                    for (place, _) in &mut places {
                        place.fields.push(ELEMENT);
                    }
                    self.state.aliases.insert(local.id, places);
                }
                let loans = self.state.locals[&owner.id].1.clone();
                self.state
                    .locals
                    .insert(local.id, (self.declarations[&local.id], loans));
                self.note_binding(local.id);
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
        let mut exits: Option<State<'a>> = None;
        let mut result = Value::default();
        for arm in arms {
            let mut pattern_pending = pending.clone();
            let mut pattern_pending_reachable = pending_reachable;
            let mut failed = pending.clone();
            let mut failed_reachable = pending_reachable;
            // The last alternative of the arm whose guard and body were checked, with the state
            // after its steps.
            let mut checked: Option<Checked<'_, 'a>> = None;
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
                            self.state
                                .locals
                                .insert(local.id, (self.declarations[&local.id], value));
                            self.state.moved.retain(|place, _| place.root != local.id);
                            self.state.moved_always.remove(&local.id);
                            self.state
                                .generic_moves
                                .retain(|place, _| place.root != local.id);
                            self.note_binding(local.id);
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
                // An alternative that binds the same locals to the same places as the last checked
                // one, and reaches the state that it reached after its steps, would check the same
                // guard and body from the same state: every result (the failed and the exit states,
                // the value, the diagnostics) is one that the checked alternative gave already, up
                // to the numbers of the loans that it makes. The alternatives of a narrowing (A25)
                // and those of a pattern that binds nothing (`:? A | :? B`) are such. Checking the
                // body again for each made nested arms cost the product of their alternatives.
                if arm.alternatives.len() > 1 {
                    if let Some((bindings, state, reachable)) = &checked
                        && *reachable == self.reachable
                        && same_bindings(bindings, &alternative.bindings)
                        && state.same(&self.state)
                    {
                        self.held.pop();
                        continue;
                    }
                    checked = Some((&alternative.bindings, self.state.clone(), self.reachable));
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
                    // As `TypedMatchArm::views`, which code generation follows.
                    if arm.borrowed.contains(&local.id)
                        && (!self.is_copy(&local.ty) || local.hides_narrowed())
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
                    // A31: a variable bound to a reference that points at its loans' places (the
                    // annotated parameter of a lambda, `\(q: ref mut Pair) -> ...`, is one bound to
                    // the whole parameter).
                    self.note_pointing(local, projection, &value);
                    self.state
                        .locals
                        .insert(local.id, (self.declarations[&local.id], value));
                    self.state.moved.retain(|place, _| place.root != local.id);
                    self.state.moved_always.remove(&local.id);
                    self.state
                        .generic_moves
                        .retain(|place, _| place.root != local.id);
                    self.note_binding(local.id);
                }
                let value = self.eval(&arm.body, Use::Consume, live)?;
                ids.extend(temporaries);
                // A31: the pattern's variables and temporaries are dropped where the arm ends, in
                // the reverse order of their bindings (`| (c, s) ->` drops `s` before `c`), as
                // where a `break` leaves them. A view is no value of its own: the matched storage
                // that it reads is dropped with the subject, after the arm.
                if self.reachable {
                    let dropped: BTreeSet<usize> = ids
                        .iter()
                        .copied()
                        .filter(|id| !self.views.contains(id))
                        .collect();
                    self.check_exit_drop_order(&dropped, arm.body.span)?;
                }
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
            // A31: an arm without a guard that has an alternative that tests nothing matches every
            // value, so no arm after it runs. The arm that re-raises an exception that no arm of a
            // `try` handler matches comes after every arm of the handler (`| e -> e` catches
            // everything): it raises nothing to the next `try`, whose drops it would check.
            if arm.guard.is_none()
                && arm.alternatives.iter().any(|alternative| {
                    !alternative
                        .steps
                        .iter()
                        .any(|step| matches!(step, PatternStep::Test(_)))
                })
            {
                pending_reachable = false;
            }
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
