//! Proofs that an array index is in range, computed on the typed IR before LLVM emission (F12).

use std::collections::{BTreeMap, BTreeSet};

use crate::check::{
    CheckedFunction, CheckedModule, FunctionRef, Local, ModuleOrigin, PatternStep, Type, TypedExpr,
    TypedExprKind,
};
use crate::interval::Interval;
use crate::syntax::BinaryOp;

#[path = "ranges_intervals.rs"]
mod intervals;

/// A function with more typed nodes than this gets no facts, so every check stays.
pub(crate) const NODE_BUDGET: usize = 65_536;

/// `-1` as a 64-bit step literal.
const MINUS_ONE: u128 = u128::MAX >> 64;
/// Integer literals below this are non-negative `i64` values.
const SIGNED_LIMIT: u128 = 1 << 63;

#[derive(Clone, Debug, Default)]
pub(crate) struct RangeFacts {
    /// Loop local -> array local; the local lies in `[0, len(array) - 1]` (R1, R2).
    below_length: BTreeMap<usize, usize>,
    /// Loop local -> inclusive constant interval (R3).
    constant: BTreeMap<usize, (u64, u64)>,
    /// Array local -> statically known length (R4).
    lengths: BTreeMap<usize, u64>,
    /// Locals declared once and never assigned or mutably borrowed.
    stable: BTreeSet<usize>,
    /// The stable locals that are not declared `mut`: what is known of their values (PR02) holds
    /// wherever they are read.
    fixed: BTreeSet<usize>,
    /// `(index local, array local)` pairs from enclosing `if` conditions (R5), innermost last.
    guards: Vec<(usize, usize)>,
    /// The values of immutable integer locals: loop counters, `let` values and what the enclosing
    /// `if` conditions prove (PR02). A local without an entry can have any value of its type.
    values: BTreeMap<usize, Interval>,
    /// What `values` held before each narrowing, innermost last, to put back when its scope ends.
    undo: Vec<(usize, Option<Interval>)>,
    /// How many expression nodes the interval proofs of this function may still visit.
    steps: usize,
}

/// What `enter_condition` pushed, for `leave_condition` to remove again.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Entered {
    guards: usize,
    narrowed: usize,
}

/// The facts of `function`. The integer intervals (PR02) are tracked only when `intervals` is set,
/// which is when the emitter writes or checks overflow flags, so that the default mode does none of
/// that work.
pub(crate) fn analyze(
    module: &CheckedModule,
    function: &CheckedFunction,
    intervals: bool,
) -> RangeFacts {
    analyze_within(module, function, NODE_BUDGET, intervals)
}

fn analyze_within(
    module: &CheckedModule,
    function: &CheckedFunction,
    budget: usize,
    intervals: bool,
) -> RangeFacts {
    let mut scan = Scan::default();
    for parameter in &function.parameters {
        scan.declare_local(parameter);
    }
    let mut work = vec![&function.body];
    let mut nodes = 0;
    while let Some(expression) = work.pop() {
        nodes += 1;
        if nodes > budget {
            return RangeFacts::default();
        }
        scan.visit(module, expression);
        work.extend(expression.children());
    }
    scan.finish(intervals)
}

impl RangeFacts {
    /// Whether `array[index]` is provably within the array's length.
    pub(crate) fn index_in_bounds(&self, array: &TypedExpr, index: &TypedExpr) -> bool {
        if !matches!(array.ty, Type::Array(_)) {
            return false;
        }
        let Some(array) = root(array).filter(|id| self.stable.contains(id)) else {
            return false;
        };
        match &index.kind {
            TypedExprKind::Local(local) if index.ty == Type::I64 => {
                self.below_length.get(local) == Some(&array)
                    || self.guards.contains(&(*local, array))
                    || matches!(
                        (self.constant.get(local), self.lengths.get(&array)),
                        (Some((_, high)), Some(length)) if high < length
                    )
            }
            TypedExprKind::Int(value) => self
                .lengths
                .get(&array)
                .is_some_and(|length| *value < u128::from(*length)),
            _ => false,
        }
    }

    /// Records what `condition` proves inside the `then` branch and returns what it pushed.
    pub(crate) fn enter_condition(
        &mut self,
        module: &CheckedModule,
        condition: &TypedExpr,
    ) -> Entered {
        let mut non_negative = BTreeSet::new();
        let mut below = Vec::new();
        let mut terms = vec![condition];
        while let Some(term) = terms.pop() {
            let TypedExprKind::Binary(operator, left, right) = &term.kind else {
                continue;
            };
            if *operator == BinaryOp::And {
                terms.push(right);
                terms.push(left);
                continue;
            }
            non_negative.extend(lower_term(*operator, left, right));
            below.extend(upper_term(module, *operator, left, right));
        }
        let before = self.guards.len();
        for (index, array) in below {
            if non_negative.contains(&index)
                && self.stable.contains(&index)
                && self.stable.contains(&array)
            {
                self.guards.push((index, array));
            }
        }
        Entered {
            guards: self.guards.len() - before,
            narrowed: self.refine(module, condition, 0),
        }
    }

    /// Records what the `else` branch of `if condition` knows: `not (a || b)` is `not a && not b`.
    /// Only the effect summary calls it (A20); the emitter does not, so no emitted check changes.
    pub(crate) fn enter_else_condition(
        &mut self,
        module: &CheckedModule,
        condition: &TypedExpr,
    ) -> Entered {
        let mut non_negative = BTreeSet::new();
        let mut below = Vec::new();
        let mut terms = vec![condition];
        while let Some(term) = terms.pop() {
            let TypedExprKind::Binary(operator, left, right) = &term.kind else {
                continue;
            };
            if *operator == BinaryOp::Or {
                terms.push(right);
                terms.push(left);
                continue;
            }
            let Some(negated) = negate(*operator) else {
                continue;
            };
            non_negative.extend(lower_term(negated, left, right));
            below.extend(upper_term(module, negated, left, right));
        }
        let before = self.guards.len();
        for (index, array) in below {
            if non_negative.contains(&index)
                && self.stable.contains(&index)
                && self.stable.contains(&array)
            {
                self.guards.push((index, array));
            }
        }
        Entered {
            guards: self.guards.len() - before,
            narrowed: 0,
        }
    }

    pub(crate) fn leave_condition(&mut self, entered: Entered) {
        self.guards.truncate(self.guards.len() - entered.guards);
        self.restore(self.undo.len() - entered.narrowed);
    }
}

/// The comparison that holds when `operator` does not.
fn negate(operator: BinaryOp) -> Option<BinaryOp> {
    match operator {
        BinaryOp::Less => Some(BinaryOp::GreaterEqual),
        BinaryOp::LessEqual => Some(BinaryOp::Greater),
        BinaryOp::Greater => Some(BinaryOp::LessEqual),
        BinaryOp::GreaterEqual => Some(BinaryOp::Less),
        _ => None,
    }
}

enum Loop {
    BelowLength(usize),
    Constant(u64, u64),
}

#[derive(Default)]
struct Scan {
    declared: BTreeSet<usize>,
    /// The locals declared `mut`.
    mutable: BTreeSet<usize>,
    /// Ids declared more than once cannot carry facts.
    duplicated: BTreeSet<usize>,
    unstable: BTreeSet<usize>,
    loops: Vec<(usize, Loop)>,
    literals: Vec<(usize, u64)>,
}

impl Scan {
    fn declare(&mut self, id: usize) {
        if !self.declared.insert(id) {
            self.duplicated.insert(id);
        }
    }

    fn declare_local(&mut self, local: &Local) {
        self.declare(local.id);
        if local.mutable {
            self.mutable.insert(local.id);
        }
    }

    /// Marks the local that an assignment or mutable borrow can replace.
    fn mutate(&mut self, target: &TypedExpr) {
        let mut place = target;
        loop {
            match &place.kind {
                TypedExprKind::Local(id) => {
                    self.unstable.insert(*id);
                    return;
                }
                TypedExprKind::Field(inner, _)
                | TypedExprKind::Index(inner, _)
                | TypedExprKind::Dereference(inner)
                | TypedExprKind::ListTail(inner, _)
                | TypedExprKind::Borrow(inner, _)
                | TypedExprKind::BorrowOperand(inner)
                | TypedExprKind::UnionPayload { value: inner, .. } => place = inner.as_ref(),
                _ => return,
            }
        }
    }

    fn visit(&mut self, module: &CheckedModule, expression: &TypedExpr) {
        match &expression.kind {
            TypedExprKind::Assign(target, _) | TypedExprKind::Borrow(target, true) => {
                self.mutate(target);
            }
            TypedExprKind::BorrowOperand(target)
                if matches!(expression.ty, Type::Reference(_, true)) =>
            {
                self.mutate(target);
            }
            TypedExprKind::Block { bindings, .. } => {
                for (local, value) in bindings {
                    self.declare_local(local);
                    if let Some(length) = literal_length(value) {
                        self.literals.push((local.id, length));
                    }
                }
            }
            TypedExprKind::ForRange {
                local,
                start,
                step,
                finish,
                descending,
                ..
            } => {
                self.declare_local(local);
                if let Some(kind) = loop_kind(module, local, start, step, finish, *descending) {
                    self.loops.push((local.id, kind));
                }
            }
            TypedExprKind::ForEach { owner, local, .. } => {
                self.declare_local(owner);
                self.declare_local(local);
            }
            TypedExprKind::Match { local, arms, .. } => {
                self.declare_local(local);
                for alternative in arms.iter().flat_map(|arm| &arm.alternatives) {
                    for (binding, _) in &alternative.bindings {
                        self.declare_local(binding);
                    }
                    for step in &alternative.steps {
                        if let PatternStep::Bind(binding, _) = step {
                            self.declare_local(binding);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn finish(self, intervals: bool) -> RangeFacts {
        let stable: BTreeSet<usize> = self
            .declared
            .into_iter()
            .filter(|id| !self.unstable.contains(id) && !self.duplicated.contains(id))
            .collect();
        let fixed = stable.difference(&self.mutable).copied().collect();
        let mut facts = RangeFacts {
            stable,
            fixed,
            steps: if intervals { intervals::STEP_BUDGET } else { 0 },
            ..RangeFacts::default()
        };
        for (local, kind) in self.loops {
            if !facts.stable.contains(&local) {
                continue;
            }
            match kind {
                Loop::BelowLength(array) if facts.stable.contains(&array) => {
                    facts.below_length.insert(local, array);
                }
                Loop::Constant(low, high) => {
                    facts.constant.insert(local, (low, high));
                }
                Loop::BelowLength(_) => {}
            }
        }
        for (array, length) in self.literals {
            if facts.stable.contains(&array) {
                facts.lengths.insert(array, length);
            }
        }
        facts
    }
}

/// The local that an array expression names, looking through shared borrows and dereferences.
fn root(array: &TypedExpr) -> Option<usize> {
    let mut place = array;
    loop {
        match &place.kind {
            TypedExprKind::Borrow(inner, false)
            | TypedExprKind::BorrowOperand(inner)
            | TypedExprKind::Dereference(inner) => place = inner.as_ref(),
            TypedExprKind::Local(id) => {
                let array =
                    matches!(place.ty, Type::Array(_)) || place.ty.slice_element().is_some();
                return array.then_some(*id);
            }
            _ => return None,
        }
    }
}

fn is_array_length(module: &CheckedModule, id: usize) -> bool {
    let function = &module.functions[id];
    function.origin.module == ModuleOrigin::Std
        && function.module == "Array"
        && function.parameters.len() == 1
        && function.name.split(".$mono.").next() == Some("length")
}

/// `array.length` or std `Array.length array`, returning the array's local.
fn length_root(module: &CheckedModule, expression: &TypedExpr) -> Option<usize> {
    match &expression.kind {
        TypedExprKind::Length(array) => root(array),
        TypedExprKind::Call(callee, arguments) if arguments.len() == 1 => {
            let TypedExprKind::Function(FunctionRef::User(id)) = callee.kind else {
                return None;
            };
            if is_array_length(module, id) {
                root(&arguments[0])
            } else {
                None
            }
        }
        _ => None,
    }
}

/// `len(array) - 1`, returning the array's local.
fn last_index_root(module: &CheckedModule, expression: &TypedExpr) -> Option<usize> {
    let TypedExprKind::Binary(BinaryOp::Subtract, length, one) = &expression.kind else {
        return None;
    };
    if !matches!(one.kind, TypedExprKind::Int(1)) {
        return None;
    }
    length_root(module, length)
}

fn literal_length(value: &TypedExpr) -> Option<u64> {
    if !matches!(value.ty, Type::Array(_)) {
        return None;
    }
    match &value.kind {
        TypedExprKind::Array(elements) => u64::try_from(elements.len()).ok(),
        TypedExprKind::NewLiteral(literal) => match &literal.kind {
            TypedExprKind::Array(elements) => u64::try_from(elements.len()).ok(),
            _ => None,
        },
        TypedExprKind::NewArray(length, _) => match length.kind {
            TypedExprKind::Int(count) if count < SIGNED_LIMIT => u64::try_from(count).ok(),
            _ => None,
        },
        _ => None,
    }
}

fn loop_kind(
    module: &CheckedModule,
    local: &Local,
    start: &TypedExpr,
    step: &TypedExpr,
    finish: &TypedExpr,
    descending: bool,
) -> Option<Loop> {
    use TypedExprKind::Int;
    if local.ty != Type::I64 {
        return None;
    }
    // `downto` counts down by its step `1`; a range counts down by the step `-1`.
    let up = !descending && matches!(step.kind, Int(1));
    let down = matches!(step.kind, Int(MINUS_ONE)) || descending && matches!(step.kind, Int(1));
    if up
        && matches!(start.kind, Int(0))
        && let Some(array) = last_index_root(module, finish)
    {
        return Some(Loop::BelowLength(array));
    }
    if down
        && matches!(finish.kind, Int(0))
        && let Some(array) = last_index_root(module, start)
    {
        return Some(Loop::BelowLength(array));
    }
    if let (Int(first), Int(last)) = (&start.kind, &finish.kind)
        && *first < SIGNED_LIMIT
        && *last < SIGNED_LIMIT
        && (up || down)
    {
        return Some(Loop::Constant(
            (*first).min(*last) as u64,
            (*first).max(*last) as u64,
        ));
    }
    None
}

fn index_local(expression: &TypedExpr) -> Option<usize> {
    match expression.kind {
        TypedExprKind::Local(id) if expression.ty == Type::I64 => Some(id),
        _ => None,
    }
}

/// `i >= 0` or `0 <= i`, returning the local.
fn lower_term(operator: BinaryOp, left: &TypedExpr, right: &TypedExpr) -> Option<usize> {
    let (local, zero) = match operator {
        BinaryOp::GreaterEqual => (left, right),
        BinaryOp::LessEqual => (right, left),
        _ => return None,
    };
    if matches!(zero.kind, TypedExprKind::Int(0)) {
        index_local(local)
    } else {
        None
    }
}

/// `i < len(v)`, `len(v) > i` or `i <= len(v) - 1`, returning the local and the array's local.
fn upper_term(
    module: &CheckedModule,
    operator: BinaryOp,
    left: &TypedExpr,
    right: &TypedExpr,
) -> Option<(usize, usize)> {
    match operator {
        BinaryOp::Less => Some((index_local(left)?, length_root(module, right)?)),
        BinaryOp::Greater => Some((index_local(right)?, length_root(module, left)?)),
        BinaryOp::LessEqual => Some((index_local(left)?, last_index_root(module, right)?)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn function<'a>(module: &'a CheckedModule, name: &str) -> &'a CheckedFunction {
        module
            .functions
            .iter()
            .find(|function| function.module == "Main" && function.name == name)
            .unwrap_or_else(|| panic!("no function {name}"))
    }

    /// Verdicts for every array index of `expression`, applying `if` facts the way the emitter does.
    fn walk(
        module: &CheckedModule,
        facts: &mut RangeFacts,
        expression: &TypedExpr,
        verdicts: &mut Vec<bool>,
    ) {
        if let TypedExprKind::Index(array, index) = &expression.kind {
            verdicts.push(facts.index_in_bounds(array, index));
        }
        if let TypedExprKind::If {
            condition,
            then_branch,
            else_branch,
        } = &expression.kind
        {
            walk(module, facts, condition, verdicts);
            let pushed = facts.enter_condition(module, condition);
            walk(module, facts, then_branch, verdicts);
            facts.leave_condition(pushed);
            walk(module, facts, else_branch, verdicts);
            return;
        }
        for child in expression.children() {
            walk(module, facts, child, verdicts);
        }
    }

    fn verdicts_with(
        module: &CheckedModule,
        function: &CheckedFunction,
        facts: RangeFacts,
    ) -> Vec<bool> {
        let mut facts = facts;
        let mut verdicts = Vec::new();
        walk(module, &mut facts, &function.body, &mut verdicts);
        assert!(facts.guards.is_empty(), "every condition is left again");
        verdicts
    }

    fn verdicts(source: &str, name: &str) -> Vec<bool> {
        let module = crate::analyze(source).unwrap_or_else(|error| {
            panic!("{source}\n{}: {}", error.code, error.message);
        });
        let function = function(&module, name);
        verdicts_with(&module, function, analyze(&module, function, true))
    }

    /// `walk`, and the facts of the `else` branch too, as the effect summary applies them (A20).
    fn walk_else(
        module: &CheckedModule,
        facts: &mut RangeFacts,
        expression: &TypedExpr,
        verdicts: &mut Vec<bool>,
    ) {
        if let TypedExprKind::Index(array, index) = &expression.kind {
            verdicts.push(facts.index_in_bounds(array, index));
        }
        if let TypedExprKind::If {
            condition,
            then_branch,
            else_branch,
        } = &expression.kind
        {
            walk_else(module, facts, condition, verdicts);
            let pushed = facts.enter_condition(module, condition);
            walk_else(module, facts, then_branch, verdicts);
            facts.leave_condition(pushed);
            let pushed = facts.enter_else_condition(module, condition);
            walk_else(module, facts, else_branch, verdicts);
            facts.leave_condition(pushed);
            return;
        }
        for child in expression.children() {
            walk_else(module, facts, child, verdicts);
        }
    }

    fn else_verdicts(source: &str, name: &str) -> Vec<bool> {
        let module = crate::analyze(source).unwrap_or_else(|error| {
            panic!("{source}\n{}: {}", error.code, error.message);
        });
        let function = function(&module, name);
        let mut facts = analyze(&module, function, false);
        let mut verdicts = Vec::new();
        walk_else(&module, &mut facts, &function.body, &mut verdicts);
        assert!(facts.guards.is_empty(), "every condition is left again");
        verdicts
    }

    #[test]
    fn else_branch_of_or_proves_the_index() {
        let source = "def guard :: ref [i64] -> i64 -> i64\n\
            fn guard values i = if i < 0 || i >= values.length then 0 else values[i]\n\
            def flipped :: ref [i64] -> i64 -> i64\n\
            fn flipped values i = if values.length <= i || 0 > i then 0 else values[i]";
        assert_eq!(else_verdicts(source, "guard"), [true]);
        assert_eq!(else_verdicts(source, "flipped"), [true]);
        // The emitter does not use the `else` facts: its own walk keeps the check.
        assert_eq!(verdicts(source, "guard"), [false]);
    }

    #[test]
    fn else_branch_keeps_weak_conditions_unproven() {
        let source = "def weak :: ref [i64] -> i64 -> i64\n\
            fn weak values i = if i < 0 || i > values.length then 0 else values[i]\n\
            def half :: ref [i64] -> i64 -> i64\n\
            fn half values i = if i >= values.length then 0 else values[i]\n\
            def both :: ref [i64] -> i64 -> i64\n\
            fn both values i = if i < 0 && i >= values.length then 0 else values[i]\n\
            def then_side :: ref [i64] -> i64 -> i64\n\
            fn then_side values i = if i < 0 || i >= values.length then values[i] else 0";
        for name in ["weak", "half", "both", "then_side"] {
            assert_eq!(else_verdicts(source, name), [false], "{name}");
        }
    }

    #[test]
    fn forward_and_backward_length_loops_prove_their_indices() {
        let source = "def forward :: ref [i64] -> i64\n\
            fn forward values =\n    let mut total = 0\n    for i in 0 .. values.length - 1 do\n        total = total + values[i]\n    total\n\
            def backward :: ref [i64] -> i64\n\
            fn backward values =\n    let mut total = 0\n    for i in values.length - 1 .. -1 .. 0 do\n        total = total + values[i]\n    total\n\
            def standard :: ref [i64] -> i64\n\
            fn standard values =\n    let mut total = 0\n    for i in 0 .. Array.length values - 1 do\n        total = total + values[i]\n    total\n\
            def owned :: i64 -> i64\n\
            fn owned count =\n    let values = new [i64](count, \\i -> i)\n    let mut total = 0\n    for i in 0 .. values.length - 1 do\n        total = total + values[i]\n    total\n\
            def counted :: ref [i64] -> i64\n\
            fn counted values =\n    let mut total = 0\n    for i = 0 to values.length - 1 do\n        total = total + values[i]\n    total\n\
            def counted_down :: ref [i64] -> i64\n\
            fn counted_down values =\n    let mut total = 0\n    for i = values.length - 1 downto 0 do\n        total = total + values[i]\n    total";
        for name in [
            "forward",
            "backward",
            "standard",
            "owned",
            "counted",
            "counted_down",
        ] {
            assert_eq!(verdicts(source, name), [true], "{name}");
        }
    }

    #[test]
    fn constant_loops_and_literal_lengths_prove_constant_indices() {
        let source = "def literal :: i64 -> i64\n\
            fn literal seed =\n    let values = [10, 20, 30]\n    values[0] + values[2] + seed\n\
            def past :: i64 -> i64\n\
            fn past seed =\n    let values = [10, 20, 30]\n    values[3] + seed\n\
            def negative :: i64 -> i64\n\
            fn negative seed =\n    let values = [10, 20, 30]\n    values[-1] + seed\n\
            def created :: i64 -> i64\n\
            fn created seed =\n    let values = new [i64](3, \\i -> i)\n    values[2] + seed\n\
            def literal_new :: i64 -> i64\n\
            fn literal_new seed =\n    let values = new [10, 20]\n    values[1] + values[2] + seed\n\
            def up :: i64 -> i64\n\
            fn up seed =\n    let values = [10, 20, 30]\n    let mut total = seed\n    for i in 0 .. 2 do\n        total = total + values[i]\n    total\n\
            def down :: i64 -> i64\n\
            fn down seed =\n    let values = [10, 20, 30]\n    let mut total = seed\n    for i in 2 .. -1 .. 0 do\n        total = total + values[i]\n    total\n\
            def beyond :: i64 -> i64\n\
            fn beyond seed =\n    let values = [10, 20, 30]\n    let mut total = seed\n    for i in 0 .. 3 do\n        total = total + values[i]\n    total\n\
            def counted_down :: i64 -> i64\n\
            fn counted_down seed =\n    let values = [10, 20, 30]\n    let mut total = seed\n    for i = 2 downto 0 do\n        total = total + values[i]\n    total\n\
            def counted_beyond :: i64 -> i64\n\
            fn counted_beyond seed =\n    let values = [10, 20, 30]\n    let mut total = seed\n    for i = 3 downto 0 do\n        total = total + values[i]\n    total";
        assert_eq!(verdicts(source, "literal"), [true, true]);
        assert_eq!(verdicts(source, "past"), [false]);
        assert_eq!(verdicts(source, "negative"), [false]);
        assert_eq!(verdicts(source, "created"), [true]);
        assert_eq!(verdicts(source, "literal_new"), [true, false]);
        assert_eq!(verdicts(source, "up"), [true]);
        assert_eq!(verdicts(source, "down"), [true]);
        assert_eq!(verdicts(source, "beyond"), [false]);
        assert_eq!(verdicts(source, "counted_down"), [true]);
        assert_eq!(verdicts(source, "counted_beyond"), [false]);
    }

    #[test]
    fn assignments_and_mutable_borrows_make_arrays_unstable() {
        let source = "def replaced :: i64 -> i64\n\
            fn replaced count =\n    let mut values = new [i64](count, \\i -> i)\n    let mut total = 0\n    for i in 0 .. values.length - 1 do\n        values = [0]\n        total = total + values[i]\n    total\n\
            def touch :: ref mut [i64] -> unit\n\
            fn touch values = ()\n\
            def borrowed :: i64 -> i64\n\
            fn borrowed seed =\n    let mut values = [10, 20, 30]\n    touch (ref mut values)\n    values[2] + seed\n\
            def untouched :: i64 -> i64\n\
            fn untouched seed =\n    let mut values = [10, 20, 30]\n    values[2] + seed";
        assert_eq!(verdicts(source, "replaced"), [false]);
        assert_eq!(verdicts(source, "borrowed"), [false]);
        assert_eq!(verdicts(source, "untouched"), [true]);
    }

    #[test]
    fn unmatched_shapes_keep_their_checks() {
        let source = "def inclusive :: i64 -> i64\n\
            fn inclusive count =\n    let values = new [i64](count, \\i -> i)\n    let mut total = 0\n    for i in 0 .. values.length do\n        total = total + values[i]\n    total\n\
            def other :: i64 -> i64\n\
            fn other count =\n    let a = new [i64](count, \\i -> i)\n    let b = new [i64](count, \\i -> i)\n    let mut total = 0\n    for i in 0 .. a.length - 1 do\n        total = total + b[i]\n    total\n\
            def counted :: i64 -> i64\n\
            fn counted count =\n    let values = new [i64](count, \\i -> i)\n    let mut total = 0\n    let mut i = 0\n    while i < values.length do\n        total = total + values[i]\n        i = i + 1\n    total\n\
            def folded :: i64 -> i64\n\
            fn folded count =\n    let values = new [i64](count, \\i -> i)\n    let mut total = 0\n    for i in 0 .. values.length - 1 + 0 do\n        total = total + values[i]\n    total\n\
            def linked :: i64 -> i64\n\
            fn linked count =\n    let values = new [|i64|](count, \\i -> i)\n    let mut total = 0\n    for i in 0 .. values.length - 1 do\n        total = total + values[i]\n    total\n\
            def nested :: i64 -> i64\n\
            fn nested count =\n    let rows = new [[i64]](count, \\i -> new [i64](count, \\j -> j))\n    let mut total = 0\n    for i in 0 .. rows.length - 1 do\n        total = total + rows[i][0]\n    total";
        for name in ["inclusive", "other", "counted", "folded", "linked"] {
            assert_eq!(verdicts(source, name), [false], "{name}");
        }
        // The outer index (`[0]`) comes first in traversal order and indexes a temporary.
        assert_eq!(verdicts(source, "nested"), [false, true]);
    }

    #[test]
    fn lambda_bodies_get_no_facts() {
        let source = "def with_closure :: ref [i64] -> i64\n\
            fn with_closure values =\n    let mut total = 0\n    for i in 0 .. values.length - 1 do\n        let read = \\unused -> values[i]\n        total = total + read 0\n    total";
        let module = crate::analyze(source).unwrap();
        let mut checked = 0;
        for function in module
            .functions
            .iter()
            .filter(|function| matches!(function.module.as_str(), "Main" | "$lambda"))
        {
            let verdicts = verdicts_with(&module, function, analyze(&module, function, true));
            assert!(!verdicts.contains(&true), "{}: {verdicts:?}", function.name);
            checked += verdicts.len();
        }
        assert_eq!(checked, 1, "the closure body indexes once");
    }

    #[test]
    fn dominating_conjunction_proves_the_then_branch_only() {
        let source = "def guarded :: i64 -> i64\n\
            fn guarded index =\n    let values = [10, 20, 30]\n    let first = if index >= 0 && index < values.length then values[index] else 0\n    let second = if index >= 0 then values[index] else 0\n    let third = if index < values.length && 0 <= index then values[index] else values[index]\n    first + second + third + values[index]\n\
            def forms :: i64 -> i64\n\
            fn forms index =\n    let values = [10, 20, 30]\n    let a = if values.length > index && index >= 0 then values[index] else 0\n    let b = if 0 <= index && index <= values.length - 1 then values[index] else 0\n    a + b\n\
            def upper_only :: i64 -> i64\n\
            fn upper_only index =\n    let values = [10, 20, 30]\n    if index < values.length then values[index] else 0\n\
            def disjunction :: i64 -> i64\n\
            fn disjunction index =\n    let values = [10, 20, 30]\n    if index >= 0 || index < values.length then values[index] else 0\n\
            def reassigned :: i64 -> i64\n\
            fn reassigned seed =\n    let values = [10, 20, 30]\n    let mut index = seed\n    index = index + 1\n    if index >= 0 && index < values.length then values[index] else 0\n\
            def rec walk :: i64 -> i64 -> i64\n\
            fn rec walk i total =\n    let values = [10, 20, 30]\n    if i >= 0 && i < values.length then walk (i + 1) (total + values[i]) else total";
        assert_eq!(
            verdicts(source, "guarded"),
            [true, false, true, false, false]
        );
        assert_eq!(verdicts(source, "forms"), [true, true]);
        assert_eq!(verdicts(source, "upper_only"), [false]);
        assert_eq!(verdicts(source, "disjunction"), [false]);
        assert_eq!(verdicts(source, "reassigned"), [false]);
        // Parameters change at a self tail call, but the guard only covers the same iteration.
        assert_eq!(verdicts(source, "walk"), [true]);
    }

    #[test]
    fn node_budget_falls_back_to_no_facts() {
        let source = "def forward :: ref [i64] -> i64\n\
            fn forward values =\n    let mut total = 0\n    for i in 0 .. values.length - 1 do\n        total = total + values[i]\n    total";
        let module = crate::analyze(source).unwrap();
        let function = function(&module, "forward");
        assert_eq!(
            verdicts_with(
                &module,
                function,
                analyze_within(&module, function, NODE_BUDGET, true)
            ),
            [true]
        );
        assert_eq!(
            verdicts_with(
                &module,
                function,
                analyze_within(&module, function, 8, true)
            ),
            [false]
        );
    }

    /// The flags that the engine proves for every integer `+ - *` of an expression, in the order the
    /// emitter reaches them, applying `bind`, `enter_counter` and `enter_condition` where it does.
    fn flag_walk(
        module: &CheckedModule,
        facts: &mut RangeFacts,
        expression: &TypedExpr,
        flags: &mut Vec<String>,
    ) {
        match &expression.kind {
            TypedExprKind::Block { bindings, result } => {
                for (local, value) in bindings {
                    flag_walk(module, facts, value, flags);
                    facts.bind(module, local, value);
                }
                flag_walk(module, facts, result, flags);
            }
            TypedExprKind::ForRange {
                local,
                start,
                step,
                finish,
                body,
                descending,
            } => {
                for part in [start, step, finish] {
                    flag_walk(module, facts, part, flags);
                }
                facts.enter_counter(module, local, start, step, finish, *descending);
                flag_walk(module, facts, body, flags);
            }
            TypedExprKind::If {
                condition,
                then_branch,
                else_branch,
            } => {
                flag_walk(module, facts, condition, flags);
                let entered = facts.enter_condition(module, condition);
                flag_walk(module, facts, then_branch, flags);
                facts.leave_condition(entered);
                flag_walk(module, facts, else_branch, flags);
            }
            TypedExprKind::Binary(operator, left, right) => {
                flag_walk(module, facts, left, flags);
                flag_walk(module, facts, right, flags);
                let symbol = match operator {
                    BinaryOp::Add => "+",
                    BinaryOp::Subtract => "-",
                    BinaryOp::Multiply => "*",
                    _ => return,
                };
                if matches!(left.ty, Type::Integer(bits, _) if bits <= 64) {
                    let proven = facts.integer_flags(module, *operator, left, right);
                    flags.push(format!("{symbol} {}", proven.text()).trim().to_owned());
                }
            }
            _ => {
                for child in expression.children() {
                    flag_walk(module, facts, child, flags);
                }
            }
        }
    }

    fn flags_with(
        module: &CheckedModule,
        function: &CheckedFunction,
        facts: RangeFacts,
    ) -> Vec<String> {
        let mut facts = facts;
        let mut flags = Vec::new();
        flag_walk(module, &mut facts, &function.body, &mut flags);
        assert!(facts.undo.is_empty(), "every narrowing is undone again");
        flags
    }

    fn flags(source: &str, name: &str) -> Vec<String> {
        let module = crate::analyze(source).unwrap_or_else(|error| {
            panic!("{source}\n{}: {}", error.code, error.message);
        });
        let function = function(&module, name);
        flags_with(&module, function, analyze(&module, function, true))
    }

    /// One function per row: its source after `fn`, and the flags of its `+ - *` in order.
    fn expect_flags(rows: &[(&str, &[&str])]) {
        for (index, (definition, expected)) in rows.iter().enumerate() {
            let source = format!("def f :: {definition}");
            assert_eq!(flags(&source, "f"), *expected, "row {index}: {definition}");
        }
    }

    #[test]
    fn types_casts_masks_and_remainders_prove_flags_without_any_flow() {
        expect_flags(&[
            // A narrow value widened keeps its range: 255 * 3 + 1 fits an i32 either way.
            (
                "i8u -> i32\nfn f x = (x as i32) * 3 + 1",
                &["* nuw nsw", "+ nuw nsw"],
            ),
            // As an unsigned number an i8 is not below 128, so only the signed reading proves it.
            ("i8 -> i32\nfn f x = (x as i32) * 3", &["* nsw"]),
            ("i32 -> i32 -> i32\nfn f a b = a + b", &["+"]),
            ("i8 -> i8\nfn f x = x + 1", &["+"]),
            ("i64 -> i64\nfn f x = (x & 255) + 1", &["+ nuw nsw"]),
            ("i64 -> i64\nfn f x = (x & 255) * (x & 255)", &["* nuw nsw"]),
            ("i64 -> i64\nfn f x = x % 10 + 1", &["+ nsw"]),
            ("i64u -> i64u\nfn f x = x % 10 + 48", &["+ nuw nsw"]),
            ("i64u -> i64u\nfn f x = (x >>> 56) + 1", &["+ nuw nsw"]),
            ("i64 -> i64\nfn f x = (x <<< 3) + 1", &["+"]),
            ("i64 -> i64\nfn f x = (x / 4) + 1", &["+ nsw"]),
            // A mask with the sign bit set, a variable shift and an `or` prove nothing.
            ("i64 -> i64\nfn f x = (x & -2) + 1", &["+"]),
            ("i64 -> i64 -> i64\nfn f x n = (x >>> n) + 1", &["+"]),
            ("i64 -> i64 -> i64\nfn f x y = (x | y) + 1", &["+"]),
            // A cast that wraps can be any value of the narrower type.
            ("i64 -> i8\nfn f x = (x as i8) + 1", &["+"]),
            ("i64 -> i8\nfn f x = ((x & 127) as i8) + 1", &["+ nuw"]),
            ("i64 -> i64\nfn f x = (-(x & 255)) + 255", &["+ nsw"]),
            ("i64 -> i64\nfn f x = (~(x & 255)) + 256", &["+ nsw"]),
            (
                "bool -> i64\nfn f c = (if c then 3 else 9) + 1",
                &["+ nuw nsw"],
            ),
            // `length` is not negative and below 2^63.
            (
                "ref [i64] -> i64\nfn f values = values.length + 1",
                &["+ nuw"],
            ),
            (
                "ref [i64] -> i64\nfn f values = values.length - 1",
                &["- nsw"],
            ),
            (
                "ref [i64] -> i64\nfn f values = Array.length values - 1",
                &["- nsw"],
            ),
            ("ref string -> i64\nfn f text = text.length * 2", &["* nuw"]),
        ]);
    }

    #[test]
    fn let_values_and_loop_counters_carry_their_ranges() {
        expect_flags(&[
            (
                "i64 -> i64\nfn f x =\n    let y = x & 15\n    y * 4 + 1",
                &["* nuw nsw", "+ nuw nsw"],
            ),
            // A mutable local can change, so it proves nothing.
            (
                "i64 -> i64\nfn f x =\n    let mut y = x & 15\n    y * 4",
                &["*"],
            ),
            (
                "i64 -> i64\nfn f x =\n    let y = x & 15\n    let z = y + 3\n    z * z",
                &["+ nuw nsw", "* nuw nsw"],
            ),
            (
                "i64 -> i64\nfn f n =\n    let mut total = 0\n    for i in 0 .. 99 do total = total + i * 2\n    total",
                &["* nuw nsw", "+"],
            ),
            (
                "i64 -> i64\nfn f n =\n    let mut total = 0\n    for i in 100 .. -1 .. 1 do total = total + (i - 1)\n    total",
                &["- nuw nsw", "+"],
            ),
            // The counter of a loop to an unknown bound is not negative, and can be the maximum.
            (
                "i64 -> i64\nfn f n =\n    let mut total = 0\n    for i in 0 .. n do total = total + (i + 1)\n    total",
                &["+ nuw", "+"],
            ),
            // Down to an unknown bound the counter stays below its start.
            (
                "i64 -> i64\nfn f n =\n    let mut total = 0\n    for i in 9 .. -1 .. n do total = total + (i + 1)\n    total",
                &["+ nsw", "+"],
            ),
            // A step of unknown sign keeps the counter between the bounds, both of them.
            (
                "i64 -> i64 -> i64\nfn f n s =\n    let mut total = 0\n    for i in 0 .. s .. 9 do total = total + (i + 1)\n    total",
                &["+ nuw nsw", "+"],
            ),
            // A counter of an unsigned type counts up whatever the step.
            (
                "i8u -> i64\nfn f n =\n    let mut total = 0\n    for i in 0i8u .. n do total = total + ((i as i64) + 1)\n    total",
                &["+ nuw nsw", "+"],
            ),
            // `downto` counts down, an unsigned counter too: it stays between the end and the start.
            (
                "i64 -> i64\nfn f n =\n    let mut total = 0\n    for i = 100 downto 1 do total = total + (i - 1)\n    total",
                &["- nuw nsw", "+"],
            ),
            (
                "i64u -> i64u\nfn f n =\n    let mut total = 0i64u\n    for i = n downto 1i64u do total = total + (i - 1i64u)\n    total",
                &["- nuw", "+"],
            ),
            (
                "ref [i64] -> i64\nfn f values =\n    let mut total = 0\n    for i in 0 .. values.length - 1 do total = total + (i + 1)\n    total",
                &["- nsw", "+ nuw nsw", "+"],
            ),
        ]);
    }

    #[test]
    fn conditions_narrow_their_locals_in_the_then_branch_only() {
        expect_flags(&[
            (
                "i64 -> i64\nfn f x = if x < 100 && x >= 0 then x + 1 else 0",
                &["+ nuw nsw"],
            ),
            (
                "i64 -> i64\nfn f x = if x > 0 then x - 1 else 0",
                &["- nuw nsw"],
            ),
            (
                "i64u -> i64u\nfn f x = if x != 0 then x - 1 else 0",
                &["- nuw"],
            ),
            (
                "i64u -> i64u\nfn f x = if x > 0 then x - 1 else 0",
                &["- nuw"],
            ),
            (
                "i64 -> i64\nfn f x = if 0 < x && x <= 9 then x * x else 0",
                &["* nuw nsw"],
            ),
            (
                "i64 -> i64 -> i64\nfn f x y = if x == y && y == 7 then x * 100 else 0",
                &["* nuw nsw"],
            ),
            // The else branch, a disjunction, and a condition on a mutable local prove nothing.
            ("i64 -> i64\nfn f x = if x < 10 then 0 else x + 1", &["+"]),
            ("i64 -> i64\nfn f x = (if x < 10 then 0 else x) + 1", &["+"]),
            (
                "i64 -> i64\nfn f x = if x < 0 || x > 10 then x + 1 else 0",
                &["+"],
            ),
            (
                "i64 -> i64\nfn f x =\n    let mut y = x\n    if y < 10 then y + 1 else 0",
                &["+"],
            ),
            // A bound that is itself an unknown number still bounds the local by its type.
            (
                "i64 -> i64 -> i64\nfn f x n = if x < n then x + 1 else 0",
                &["+ nsw"],
            ),
            (
                "i64 -> i64 -> i64\nfn f x n = if n > x then x + 1 else 0",
                &["+ nsw"],
            ),
            (
                "i8 -> i8\nfn f x = if x < 127 then x + 1 else 0",
                &["+ nsw"],
            ),
            ("i8 -> i8\nfn f x = if x <= 127 then x + 1 else 0", &["+"]),
            // The values of the then branch are those of the condition's operand, wherever it is used.
            (
                "i64 -> i64\nfn f x =\n    let y = if x >= 0 && x < 50 then x else 0\n    y * 2",
                &["* nuw nsw"],
            ),
        ]);
    }

    #[test]
    fn nesting_beyond_the_depth_limit_proves_nothing_and_flags_stay_sound() {
        // `((x & 1) + 1) + 1 ...`: after k additions the value is in [k, k + 1], so an i8 add is
        // provably free of signed overflow up to k = 126.
        let additions = 100;
        let sum = (0..additions).fold("(x & 1)".to_owned(), |sum, _| format!("({sum} + 1)"));
        let source = format!("def f :: i8 -> i8\nfn f x = {sum}");
        let proven = flags(&source, "f");
        assert_eq!(proven.len(), additions);
        let free = |flags: &str| flags == "+ nuw nsw";
        assert!(proven[..20].iter().all(|flags| free(flags)), "{proven:?}");
        assert!(proven[60..].iter().all(|flags| flags == "+"), "{proven:?}");
        // Nothing is flagged that can overflow: the k-th addition (counting from 1) produces at
        // most k + 1, which exceeds 127 only after k = 126.
        assert!(proven.iter().all(|flags| flags == "+" || free(flags)));
    }

    #[test]
    fn a_function_beyond_the_node_budget_or_without_intervals_gets_no_flags() {
        let source = "def f :: i64 -> i64\nfn f x = (x & 255) + 1";
        let module = crate::analyze(source).unwrap();
        let function = function(&module, "f");
        assert_eq!(
            flags_with(&module, function, analyze(&module, function, true)),
            ["+ nuw nsw"]
        );
        assert_eq!(
            flags_with(
                &module,
                function,
                analyze_within(&module, function, 2, true)
            ),
            ["+"]
        );
        // The default mode of the emitter asks for no intervals at all.
        assert_eq!(
            flags_with(&module, function, analyze(&module, function, false)),
            ["+"]
        );
    }

    #[test]
    fn intervals_follow_the_typed_expression() {
        let source = "def f :: i64 -> i64\nfn f x =\n    let y = (x & 15) + 1\n    y * 3";
        let module = crate::analyze(source).unwrap();
        let function = function(&module, "f");
        let mut facts = analyze(&module, function, true);
        let TypedExprKind::Block { bindings, result } = &function.body.kind else {
            panic!("{:?}", function.body.kind);
        };
        let (local, value) = &bindings[0];
        assert_eq!(
            facts.interval(&module, value),
            Some(Interval::new(1, 16)),
            "{value:?}"
        );
        facts.bind(&module, local, value);
        assert_eq!(facts.interval(&module, result), Some(Interval::new(3, 48)));
        assert_eq!(
            facts.interval(&module, &function.body),
            Some(Interval::new(3, 48))
        );
    }
}
