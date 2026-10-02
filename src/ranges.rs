//! Proofs that an array index is in range, computed on the typed IR before LLVM emission (F12).

use std::collections::{BTreeMap, BTreeSet};

use crate::check::{
    CheckedFunction, CheckedModule, FunctionRef, Local, ModuleOrigin, PatternStep, Type, TypedExpr,
    TypedExprKind,
};
use crate::syntax::BinaryOp;

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
    /// `(index local, array local)` pairs from enclosing `if` conditions (R5), innermost last.
    guards: Vec<(usize, usize)>,
}

pub(crate) fn analyze(module: &CheckedModule, function: &CheckedFunction) -> RangeFacts {
    analyze_within(module, function, NODE_BUDGET)
}

fn analyze_within(module: &CheckedModule, function: &CheckedFunction, budget: usize) -> RangeFacts {
    let mut scan = Scan::default();
    for parameter in &function.parameters {
        scan.declare(parameter.id);
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
    scan.finish()
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

    /// Records what `condition` proves inside the `then` branch and returns how many facts it pushed.
    pub(crate) fn enter_condition(
        &mut self,
        module: &CheckedModule,
        condition: &TypedExpr,
    ) -> usize {
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
        self.guards.len() - before
    }

    pub(crate) fn leave_condition(&mut self, pushed: usize) {
        self.guards.truncate(self.guards.len() - pushed);
    }
}

enum Loop {
    BelowLength(usize),
    Constant(u64, u64),
}

#[derive(Default)]
struct Scan {
    declared: BTreeSet<usize>,
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
                    self.declare(local.id);
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
                ..
            } => {
                self.declare(local.id);
                if let Some(kind) = loop_kind(module, local, start, step, finish) {
                    self.loops.push((local.id, kind));
                }
            }
            TypedExprKind::ForEach { owner, local, .. } => {
                self.declare(owner.id);
                self.declare(local.id);
            }
            TypedExprKind::Match { local, arms, .. } => {
                self.declare(local.id);
                for alternative in arms.iter().flat_map(|arm| &arm.alternatives) {
                    for (binding, _) in &alternative.bindings {
                        self.declare(binding.id);
                    }
                    for step in &alternative.steps {
                        if let PatternStep::Bind(binding, _) = step {
                            self.declare(binding.id);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn finish(self) -> RangeFacts {
        let stable: BTreeSet<usize> = self
            .declared
            .into_iter()
            .filter(|id| !self.unstable.contains(id) && !self.duplicated.contains(id))
            .collect();
        let mut facts = RangeFacts {
            stable,
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
                    matches!(place.ty, Type::Array(_)) || place.ty.shared_array_element().is_some();
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
) -> Option<Loop> {
    use TypedExprKind::Int;
    if local.ty != Type::I64 {
        return None;
    }
    if matches!((&start.kind, &step.kind), (Int(0), Int(1)))
        && let Some(array) = last_index_root(module, finish)
    {
        return Some(Loop::BelowLength(array));
    }
    if matches!((&step.kind, &finish.kind), (Int(MINUS_ONE), Int(0)))
        && let Some(array) = last_index_root(module, start)
    {
        return Some(Loop::BelowLength(array));
    }
    if let (Int(first), Int(last)) = (&start.kind, &finish.kind)
        && *first < SIGNED_LIMIT
        && *last < SIGNED_LIMIT
        && matches!(step.kind, Int(1) | Int(MINUS_ONE))
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
        verdicts_with(&module, function, analyze(&module, function))
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
            fn owned count =\n    let values = new [i64](count, \\i -> i)\n    let mut total = 0\n    for i in 0 .. values.length - 1 do\n        total = total + values[i]\n    total";
        for name in ["forward", "backward", "standard", "owned"] {
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
            fn beyond seed =\n    let values = [10, 20, 30]\n    let mut total = seed\n    for i in 0 .. 3 do\n        total = total + values[i]\n    total";
        assert_eq!(verdicts(source, "literal"), [true, true]);
        assert_eq!(verdicts(source, "past"), [false]);
        assert_eq!(verdicts(source, "negative"), [false]);
        assert_eq!(verdicts(source, "created"), [true]);
        assert_eq!(verdicts(source, "literal_new"), [true, false]);
        assert_eq!(verdicts(source, "up"), [true]);
        assert_eq!(verdicts(source, "down"), [true]);
        assert_eq!(verdicts(source, "beyond"), [false]);
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
            let verdicts = verdicts_with(&module, function, analyze(&module, function));
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
                analyze_within(&module, function, NODE_BUDGET)
            ),
            [true]
        );
        assert_eq!(
            verdicts_with(&module, function, analyze_within(&module, function, 8)),
            [false]
        );
    }
}
