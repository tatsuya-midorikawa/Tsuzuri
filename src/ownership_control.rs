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

/// A31: the stops of `Checker::check_loop` before its limit of `MAX_NESTING` + 1 iterations. Each
/// iteration checks the condition and the body again (with the loops and the checks inside them)
/// from the state that the iterations before it left, and makes its loans again with fresh
/// numbers, one for each place that a borrow gives in that state, so the loans that it makes
/// (`self.loans.len()` after it, less before it) measure how far that state has grown, and the
/// time of an iteration grows with them.
///
/// In the loops that never settle (A34 `loopplace`), a store into a function value that owns a
/// cell adds the borrows of a reference parameter's target to the parameter, whose dereference
/// then gives their places too, so each iteration borrows places one field deeper, and the loans
/// that an iteration makes grow without end: about twice those of the one before, three times
/// through a choice between reborrows, and by 1.2 to 1.6 times when the borrow passes through two
/// to nine locals before the store, after one more loan per iteration for about as many iterations
/// as there are locals. Each doubling makes an iteration take about four times as long, and the
/// state that the loop carries makes each iteration of a loop inside it longer too, so the check
/// would not reach the limit of the iterations in practical time. Three stops end such checks with
/// `E1017` (`loop_ownership_limit`): `LOOP_FASTER_ITERATIONS` iterations in a row whose loans grow
/// faster and faster, `LOOP_GROWING_ITERATIONS` (or `LOOP_GROWING_LARGE`) iterations in a row
/// whose loans grow at least as fast as before, and `LOOP_STATE_ITERATIONS` iterations in a row
/// after which the loans that the loop's state holds grow faster and faster, for a loop whose
/// growth a part that settles hides. No stop applies to an iteration whose loans grow by less than
/// those of the one before it, or do not grow, and the first two counts start again there.
///
/// In a loop that settles, the later iterations make about as many loans as the first: in the gate
/// and generated corpora, the examples and test fixtures, the std examples and the timing programs,
/// every loop settles within 3 iterations, and an iteration makes at most 3 more loans than the
/// first. A reference that the loop assigns again (`code = sub0`, `cur = match op with ...`) widens
/// to every place that the loop may give it at the first merge, and a local that copies it
/// (`saved = code`) one iteration later, so each borrow through them makes one loan for each of
/// those places from the second iteration on: an interpreter loop of a few dozen opcodes grows by
/// hundreds of loans once, in its second iteration, and settles in its third. When the loop also
/// keeps references that settle one iteration after another (a history `last4 = last3`, ...,
/// `last0 = ref names[pc]`, a return stack `ret3 = ret2`, ..., `ret0 = code`), each of its later
/// iterations makes a few loans more until it settles, and after the jump no iteration adds more
/// than the one before it. So `LOOP_FASTER_ITERATIONS` does not count them, and
/// `LOOP_GROWING_ITERATIONS` counts one iteration for each level of a return stack whose levels add
/// the same few loans.
///
/// A chain of `L` references, each to a field of the target of the next, advanced in one loop
/// (`q0 = ref (deref q1).a`, ..., `q(L-1) = ref (deref qL).a`), borrows in iteration `k` (from 0),
/// for each link `j`, every place that link `j + 1` holds, `m` times `1 + min(k, L - 1 - j)` of
/// them when each link starts with `m` places, so the iteration makes `m (k (k - 1) / 2 + k (L -
/// k))` more loans than the first, at most `m L (L - 1) / 2` in iteration `L - 1`, and the loop
/// settles in iteration `L`. Its loans grow by less in each iteration than in the one before
/// (`m (L - k)`), and so do those of a loop that passes a reference along locals and borrows
/// through each of them (`r1 = r0`, also when it reads through each many times and the first takes
/// one of several places), so no stop applies to them, however many loans they make, and
/// they keep the verdict of `15d84aaf`, which had no stops. Their check grows with about the
/// seventh power of the chain's length: on a release build, a chain of 20 links takes about 1.5
/// seconds, one of 30 about 18, one of 33 about 33 and one of 40 about 2 minutes, as long as the
/// same check without stops takes. A chain whose links each take one of two fields of the next
/// link's target doubles its places at each link instead, and meets `LOOP_FASTER_ITERATIONS`.
///
/// No stop counts the loans themselves. A limit on them (A31 `LOOP_LOAN_GROWTH`, removed: an
/// iteration from the fifth on that made more than 500 loans beyond the iteration before it that
/// made the fewest, and more than 100 beyond the fourth) stopped first, in the 1887 loop traces of
/// the reviews of A31, only loops that settle (such chains and windows, and interpreters and state
/// machines with a history or a return stack whose reference widened once), and every `loopplace`
/// form met `LOOP_FASTER_ITERATIONS` or `LOOP_GROWING_ITERATIONS` before it. A `loopplace` loop
/// that also passes a reference along locals and reads through each of them, which those traces
/// did not have, took 10 to 90 seconds on a release build to meet `LOOP_FASTER_ITERATIONS` without
/// the limit (which had stopped it in 0.2), and meets `LOOP_STATE_ITERATIONS` sooner.
///
/// What `check_loop` keeps for the stops: the loans that the last iteration made and that the
/// state held after it, how many more of each than the iteration before, and the three counts.
/// Planned test of real loops' margin to these stops: `_features/A35-loop-growth-margin-check.md`.
#[derive(Default)]
struct LoanGrowth {
    /// The loans that the last iteration made, and how many more than the one before it (none when
    /// it made no more).
    last: Option<(usize, Option<usize>)>,
    /// How many iterations in a row made more loans than the one before, and more of them than
    /// that one added.
    faster: usize,
    /// How many iterations in a row made more loans than the one before, and at least as many more
    /// as that one added.
    growing: usize,
    /// The loans that the state held after the last iteration (`Checker::loop_state_loans`), and
    /// how many more than after the one before it (none when it held no more).
    state: Option<(usize, Option<usize>)>,
    /// How many iterations in a row left the state holding more loans than the one before, and
    /// more of them than that one added.
    state_faster: usize,
}

/// A31: how many iterations in a row that did not settle may each make more loans than the
/// iteration before it, and more of them than that one added, before `Checker::check_loop` stops
/// with `E1017`. The loans of a `loopplace` loop grow so from its second iteration on (1, 2, 4,
/// ... more) or from the iteration whose store first gets the borrow that its locals passed on, so
/// the check stops at its sixth iteration (iteration 5) or five iterations after that store. In the
/// gate and generated corpora and the std examples, a loop that settles grew faster at most once
/// in a row, while its locals widened one after another (a reference that the loop assigns again,
/// then the locals that copy it). A chain whose links each take one of two fields of the next
/// link's target (`qk = if c then ref (deref q(k+1)).a else ref (deref q(k+1)).b`) doubles its
/// places at each link, so its loans grow faster in each iteration until it settles, and this
/// stops it from 7 links on (`15d84aaf` checks 7 links in 0.8 seconds on a release build, 8 in 3.3
/// and 9 in 16). With one of four fields, 5 links grow faster in four iterations in a row and
/// settle in the fifth (57, 121, 313, 825, 1849 loans).
const LOOP_FASTER_ITERATIONS: usize = 5;

/// A31: how many iterations in a row that did not settle may each make more loans than the
/// iteration before it, and at least as many more as that one added, before
/// `Checker::check_loop` stops with `E1017`; `LOOP_GROWING_LARGE` stops a loop that makes many
/// loans sooner. A `loopplace` loop that passes the borrow through `k` locals before the store into
/// the function value (`t1 = t0`, A34) adds one loan per iteration for about `k` iterations before
/// its loans grow faster, so this stops it at its 25th iteration at the latest (`k = 140`: 13
/// seconds for the 129 iterations of the limit on a release build). An iteration costs about the
/// statements that the body evaluates times the loans that the state holds, not the loans that the
/// iteration makes, so a body of many statements that borrow little still runs those 24
/// iterations: on a release build, a hand-on through 60 locals with 2000 statements that make no
/// loans takes about 8.5 seconds, and one through 500 locals about 7 (A34 `loopplace`). A loop that
/// copies a reference along `n` locals, one per iteration (`u(n-1) = u(n-2)`, ..., `u1 = u0`, `u0
/// = ref a`), and borrows through the last of them adds one loan per iteration the same way and
/// settles in iteration `n`, so this stops such a loop from 25 locals on.
const LOOP_GROWING_ITERATIONS: usize = 24;

/// A31: the iterations in a row of `LOOP_GROWING_ITERATIONS` that stop a loop whose last iteration
/// made more loans than the second number. The loans are the only measure of the body that the
/// stops have: a body that makes many of them takes long enough per iteration that 24 iterations
/// would take seconds (a body of 1000 borrows and 40 locals that pass the borrow on took about 5
/// seconds for 32), and this stops it after a third of that. A body of many statements that make
/// few loans takes as long per iteration but runs all 24. A loop that copies a reference along 9 or
/// more locals and makes more than 200 loans per iteration stops too.
const LOOP_GROWING_LARGE: (usize, usize) = (8, 200);

/// A31: how many iterations in a row that did not settle may each leave the loop's state holding
/// more loans than the one before it, and more of them than that one added
/// (`Checker::loop_state_loans`), before `Checker::check_loop` stops with `E1017` at an iteration
/// that counts for `LOOP_GROWING_ITERATIONS`. A part of a loop that settles can hide the doubling
/// of a `loopplace` loop from the loans that its iterations make: a reference passed along `S`
/// locals and read through each of them `R` times (`s1 = s0`, `acc = acc + String.length
/// s1.name`) makes about `R` fewer loans in each iteration than in the one before until the
/// locals settle, so the loop's loans grow by less in each iteration until the doubling (1, 2, 4,
/// ... more) outgrows `R`, about `log2 R + 2` iterations in, or the locals settle, and
/// `LOOP_FASTER_ITERATIONS` stops it five iterations after that, each two to three times as long
/// as the one before (on a release build, 19 seconds for `S = 10` and `R = 32`, 87 for `R = 64`).
/// The reads multiply the loans of an iteration, not those of the state, which double from the
/// first iteration on (27, 45, 70, 110, 181, 315, 576, 1092 for `S = 10` and `R = 32`), so this
/// stops such a loop at the first iteration, from its ninth (iteration 8) on, whose loans grow at
/// least as much as those of the one before it: iteration 8 for `R` up to 64 (2503 loans for `R =
/// 32`, 4743 for `R = 64`), and one iteration later for each doubling of `R` beyond (A34
/// `loopplace` has the times). The state of a loop that settles can grow faster for as long while
/// its iterations make fewer loans each time (a chain that the loop advances while it stores a
/// function value that reads it: 8 iterations in a row for `each9_store0_live`, 11 for longer
/// chains), so this never counts an iteration whose loans grow by less than those of the one
/// before it, and it stops no loop whose loans grow by less in each iteration, such as the chains
/// and windows that `LoanGrowth` describes. In the loops of the reviews of A31 that settle, the
/// state had grown faster at most 4 iterations in a row at such an iteration (a chain whose links
/// each take one of two fields of the next link's target, of 6 links; 8 at 10 links, which
/// `LOOP_FASTER_ITERATIONS` stops first).
const LOOP_STATE_ITERATIONS: usize = 8;

/// A31: the change that lets the loops of A34 `loopplace` settle, which the help of each stop of
/// `LoanGrowth` names.
const LOOP_GROWTH_ADVICE: &str = "when the loop passes a borrow of a reference parameter's target \
     to a function value that owns a cell (a lambda that captures an `Arc` by value), let the \
     function value capture a reference to the cell instead, such as \
     `let cell = Arc.get (ref sink)`";

impl LoanGrowth {
    /// Notes the `made` loans of an iteration that did not settle, and returns the help of the
    /// `E1017` that stops the loop when they meet a limit.
    fn note(&mut self, made: usize, held: usize) -> Option<String> {
        let held_added = self
            .state
            .and_then(|(before, _)| held.checked_sub(before))
            .filter(|added| *added > 0);
        let held_earlier = self.state.and_then(|(_, added)| added);
        self.state_faster = match held_added {
            Some(added) if held_earlier.is_none_or(|earlier| added > earlier) => {
                self.state_faster + 1
            }
            _ => 0,
        };
        self.state = Some((held, held_added));
        let added = self
            .last
            .and_then(|(before, _)| made.checked_sub(before))
            .filter(|added| *added > 0);
        let earlier = self.last.and_then(|(_, added)| added);
        match added {
            Some(added) => {
                self.faster = if earlier.is_none_or(|earlier| added > earlier) {
                    self.faster + 1
                } else {
                    0
                };
                self.growing = if earlier.is_none_or(|earlier| added >= earlier) {
                    self.growing + 1
                } else {
                    0
                };
            }
            None => (self.faster, self.growing) = (0, 0),
        }
        self.last = Some((made, added));
        if self.faster >= LOOP_FASTER_ITERATIONS {
            return Some(format!(
                "the loop's borrows keep growing faster: each of the last {LOOP_FASTER_ITERATIONS} \
                 iterations made more borrows than the one before it, and more of them than that \
                 one added (the last made {made}), so the check stops before the loop settles; \
                 {LOOP_GROWTH_ADVICE}"
            ));
        }
        let (large_iterations, large) = LOOP_GROWING_LARGE;
        if self.growing >= LOOP_GROWING_ITERATIONS
            || (self.growing >= large_iterations && made > large)
        {
            return Some(format!(
                "the loop's borrows keep growing: each of the last {} iterations made more \
                 borrows than the one before it, and at least as many more as that one added (the \
                 last made {made}), so the check stops before the loop settles; a reference that \
                 the loop passes along {} or more locals, one per iteration (`u1 = u0`), and \
                 borrows through grows this way, and along {} or more when its iterations make \
                 more than {large} borrows; {LOOP_GROWTH_ADVICE}",
                self.growing,
                LOOP_GROWING_ITERATIONS + 1,
                large_iterations + 1
            ));
        }
        if self.state_faster >= LOOP_STATE_ITERATIONS && self.growing > 0 {
            return Some(format!(
                "the borrows that the loop carries keep growing faster: after each of the last \
                 {LOOP_STATE_ITERATIONS} iterations, the loop's state held more borrows than \
                 after the one before it, and more of them than that one added (now {held}), and \
                 the last iteration made {made} borrows, at least as many more than the one before \
                 it as that one added, so the check stops before the loop settles; locals that \
                 pass a reference along and are read through make each iteration add fewer \
                 borrows until they settle, which hides such growth for some iterations; \
                 {LOOP_GROWTH_ADVICE}"
            ));
        }
        None
    }
}

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

    /// A31: the loans that the state holds, for `LOOP_STATE_ITERATIONS`: those of each local and of
    /// each of its region slots, and those of `State::kept` and `State::kept_elsewhere`.
    fn loop_state_loans(&self) -> usize {
        let locals: usize = self
            .state
            .locals
            .values()
            .map(|(_, value)| {
                value.loans.len()
                    + value
                        .regions
                        .iter()
                        .flatten()
                        .map(BTreeSet::len)
                        .sum::<usize>()
            })
            .sum();
        locals + self.state.kept.len() + self.state.kept_elsewhere.len()
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
        let mut growth = LoanGrowth::default();
        for _ in 0..=crate::syntax::MAX_NESTING {
            let before = self.loop_summary();
            let made_before = self.loans.len();
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
            if let Some(help) = growth.note(self.loans.len() - made_before, self.loop_state_loans())
            {
                return Err(limit().with_help(help));
            }
        }
        Err(limit())
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
