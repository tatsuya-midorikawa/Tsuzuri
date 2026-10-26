//! Bounded backwards liveness on concrete IR, shared by emission and the Copy inventory.
use std::collections::{BTreeMap, BTreeSet};

use crate::check::{
    CheckedFunction, CheckedModule, PatternStep, TypedExpr, TypedExprKind, TypedMatchArm,
};
use crate::diagnostic::Span;
use crate::syntax::BinaryOp;

type SpanKey = (Option<usize>, usize, usize);
type ReadKey = (SpanKey, usize);
type Live = BTreeSet<usize>;
const BUDGET: usize = 100_000;

#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct LastUses {
    reads: BTreeSet<ReadKey>,
    bindings: BTreeSet<SpanKey>,
}

fn key(span: Span) -> SpanKey {
    (span.source, span.start, span.end)
}

impl LastUses {
    pub(crate) fn analyze(function: &CheckedFunction, module: &CheckedModule) -> Self {
        Self::with_budget(function, module, BUDGET)
    }

    fn with_budget(function: &CheckedFunction, module: &CheckedModule, budget: usize) -> Self {
        let mut analysis = Analysis {
            module,
            excluded: BTreeSet::new(),
            loops: Vec::new(),
            recording: true,
            reads: BTreeMap::new(),
            bindings: BTreeMap::new(),
            budget,
            exhausted: false,
        };
        if function.owned_captures {
            analysis.excluded.extend(
                function.parameters[..function.capture_count]
                    .iter()
                    .map(|local| local.id),
            );
        }
        let mut pending = vec![(&function.body, 0)];
        while let Some((expression, depth)) = pending.pop() {
            if !analysis.spend(depth) {
                return Self::default();
            }
            use TypedExprKind::*;
            match &expression.kind {
                Borrow(value, _) | BorrowOperand(value) | Slice { value, .. } => {
                    if let Some(id) = root(value) {
                        analysis.excluded.insert(id);
                    }
                }
                ForEach {
                    owner,
                    local,
                    source,
                    ..
                } => {
                    analysis.excluded.extend([owner.id, local.id]);
                    if let Some(id) = root(source) {
                        analysis.excluded.insert(id);
                    }
                }
                ForRange { local, .. } => {
                    analysis.excluded.insert(local.id);
                }
                Match { local, arms, .. } => {
                    analysis.excluded.insert(local.id);
                    for arm in arms {
                        for alternative in &arm.alternatives {
                            analysis
                                .excluded
                                .extend(alternative.bindings.iter().map(|(b, _)| b.id));
                            for step in &alternative.steps {
                                if let PatternStep::Bind(local, _) = step {
                                    analysis.excluded.insert(local.id);
                                }
                            }
                        }
                    }
                }
                Lambda { captures, .. } => {
                    analysis
                        .excluded
                        .extend(captures.iter().map(|local| local.id));
                }
                _ => {}
            }
            let _ = expression.try_for_each_child(&mut |child| {
                pending.push((child, depth + 1));
                Ok(())
            });
        }
        analysis.visit(&function.body, &mut Live::new(), 0);
        if analysis.exhausted {
            return Self::default();
        }
        Self {
            reads: analysis
                .reads
                .into_iter()
                .filter_map(|(k, last)| last.then_some(k))
                .collect(),
            bindings: analysis
                .bindings
                .into_iter()
                .filter_map(|(k, last)| last.then_some(k))
                .collect(),
        }
    }

    pub(crate) fn moves_read(&self, expression: &TypedExpr) -> bool {
        matches!(expression.kind, TypedExprKind::Local(id) if self.moves_read_at(expression.span, id))
    }

    pub(crate) fn moves_read_at(&self, span: Span, local: usize) -> bool {
        self.reads.contains(&(key(span), local))
    }

    pub(crate) fn moves_binding(&self, span: Span) -> bool {
        self.bindings.contains(&key(span))
    }
}

fn root(mut expression: &TypedExpr) -> Option<usize> {
    loop {
        use TypedExprKind::*;
        match &expression.kind {
            Local(id) => return Some(*id),
            Field(value, _) | Index(value, _) | ListTail(value, _) | UnionPayload { value, .. } => {
                expression = value
            }
            _ => return None,
        }
    }
}

struct LoopContext {
    exit: Live,
    head: Live,
}

struct Analysis<'a> {
    module: &'a CheckedModule,
    excluded: Live,
    loops: Vec<LoopContext>,
    recording: bool,
    reads: BTreeMap<ReadKey, bool>,
    bindings: BTreeMap<SpanKey, bool>,
    budget: usize,
    exhausted: bool,
}

impl Analysis<'_> {
    fn spend(&mut self, depth: usize) -> bool {
        if depth > crate::syntax::MAX_NESTING || self.budget == 0 {
            self.exhausted = true;
            return false;
        }
        self.budget -= 1;
        true
    }

    fn record(&mut self, expression: &TypedExpr, id: usize, last: bool) {
        let types = self.module.types();
        if self.recording && expression.ty.is_copy(&types) && expression.ty.needs_drop(&types) {
            let last = last && !self.excluded.contains(&id);
            self.reads
                .entry((key(expression.span), id))
                .and_modify(|old| *old &= last)
                .or_insert(last);
        }
    }

    fn opaque(&mut self, expression: &TypedExpr, live: &mut Live, depth: usize) {
        let mut pending = vec![(expression, depth)];
        while let Some((expression, depth)) = pending.pop() {
            if !self.spend(depth) {
                return;
            }
            match expression.kind {
                TypedExprKind::Local(id) => {
                    self.record(expression, id, false);
                    live.insert(id);
                }
                TypedExprKind::Break | TypedExprKind::Continue => {
                    if let Some(context) = self.loops.last() {
                        live.extend(&context.exit);
                        live.extend(&context.head);
                    }
                }
                _ => {}
            }
            // An opaque match must veto colliding binding spans too.
            if self.recording
                && let TypedExprKind::Match { arms, .. } = &expression.kind
            {
                for arm in arms {
                    for alternative in &arm.alternatives {
                        for (binding, _) in &alternative.bindings {
                            self.bindings.insert(key(binding.span), false);
                        }
                    }
                }
            }
            let _ = expression.try_for_each_child(&mut |child| {
                pending.push((child, depth + 1));
                Ok(())
            });
        }
    }

    fn visit(&mut self, expression: &TypedExpr, live: &mut Live, depth: usize) {
        if !self.spend(depth) {
            return;
        }
        use TypedExprKind::*;
        match &expression.kind {
            Int(_)
            | Float(_)
            | String(_)
            | Bool(_)
            | Unit
            | Function(_)
            | Static(_)
            | Error
            | Construct { payload: None, .. } => {}
            Local(id) => {
                self.record(expression, *id, !live.contains(id));
                live.insert(*id);
            }
            Unary(_, value)
            | Cast(value)
            | Checked(value)
            | Construct {
                payload: Some(value),
                ..
            } => self.visit(value, live, depth + 1),
            // These read storage rather than consuming its root. Never mark the root as a move.
            Field(..)
            | Length(_)
            | StringLength(_)
            | UnionTag(_)
            | UnionPayload { .. }
            | ListTail(..)
            | Dereference(_)
            | Index(..) => self.opaque(expression, live, depth),
            Binary(BinaryOp::And | BinaryOp::Or, a, b) => {
                let mut conditional = live.clone();
                self.visit(b, &mut conditional, depth + 1);
                live.extend(conditional);
                self.visit(a, live, depth + 1);
            }
            Binary(BinaryOp::Pipe, ..) => self.opaque(expression, live, depth),
            Binary(_, a, b) => {
                self.visit(b, live, depth + 1);
                self.visit(a, live, depth + 1);
            }
            Call(callee, arguments) => {
                for argument in arguments.iter().rev() {
                    self.visit(argument, live, depth + 1);
                }
                self.visit(callee, live, depth + 1);
            }
            Record(fields) => {
                for (_, value) in fields.iter().rev() {
                    self.visit(value, live, depth + 1);
                }
            }
            Tuple(values) | Array(values) | List(values) | Closure(_, values) => {
                for value in values.iter().rev() {
                    self.visit(value, live, depth + 1);
                }
            }
            Assign(place, value) => {
                if let Local(id) = place.kind {
                    live.remove(&id);
                } else {
                    self.opaque(place, live, depth + 1);
                }
                self.visit(value, live, depth + 1);
            }
            Block { bindings, result } => {
                self.visit(result, live, depth + 1);
                for (local, value) in bindings.iter().rev() {
                    live.remove(&local.id);
                    self.visit(value, live, depth + 1);
                }
            }
            If {
                condition,
                then_branch,
                else_branch,
            } => {
                let mut other = live.clone();
                self.visit(then_branch, live, depth + 1);
                self.visit(else_branch, &mut other, depth + 1);
                live.extend(other);
                self.visit(condition, live, depth + 1);
            }
            While { condition, body } => {
                self.loop_expression(Some(condition), body, &[], live, depth)
            }
            ForRange {
                local,
                start,
                step,
                finish,
                body,
                ..
            } => {
                self.loop_expression(None, body, &[local.id], live, depth);
                for value in [finish, step, start] {
                    self.visit(value, live, depth + 1);
                }
            }
            ForEach {
                owner,
                local,
                source,
                body,
            } => {
                self.loop_expression(None, body, &[owner.id, local.id], live, depth);
                self.opaque(source, live, depth + 1);
            }
            Break | Continue => {
                if let Some(context) = self.loops.last() {
                    *live = if matches!(expression.kind, Break) {
                        &context.exit
                    } else {
                        &context.head
                    }
                    .clone();
                }
            }
            Match { local, value, arms } => {
                let exit = live.clone();
                let outer = std::mem::replace(&mut self.recording, false);
                let mut fall = Live::new();
                loop {
                    let next = self.match_round(arms, &exit, &fall, depth);
                    if next.is_subset(&fall) || self.exhausted {
                        break;
                    }
                    fall.extend(next);
                }
                self.recording = outer;
                *live = self.match_round(arms, &exit, &fall, depth);
                live.remove(&local.id);
                let owned = !value.is_place()
                    || matches!(value.kind, Local(id) if !live.contains(&id) && !self.excluded.contains(&id));
                if self.recording {
                    self.record_bindings(local.id, arms, owned);
                }
                // Matching reads the place but does not take the whole value.
                if let Local(id) = value.kind {
                    live.insert(id);
                } else {
                    self.visit(value, live, depth + 1);
                }
            }
            _ => self.opaque(expression, live, depth),
        }
    }

    fn loop_expression(
        &mut self,
        condition: Option<&TypedExpr>,
        body: &TypedExpr,
        killed: &[usize],
        live: &mut Live,
        depth: usize,
    ) {
        let exit = live.clone();
        let outer = std::mem::replace(&mut self.recording, false);
        let mut head = Live::new();
        loop {
            let next = self.loop_round(condition, body, killed, &exit, &head, depth);
            if next.is_subset(&head) || self.exhausted {
                break;
            }
            head.extend(next);
        }
        self.recording = outer;
        self.loop_round(condition, body, killed, &exit, &head, depth);
        *live = head;
    }

    fn loop_round(
        &mut self,
        condition: Option<&TypedExpr>,
        body: &TypedExpr,
        killed: &[usize],
        exit: &Live,
        head: &Live,
        depth: usize,
    ) -> Live {
        self.loops.push(LoopContext {
            exit: exit.clone(),
            head: head.clone(),
        });
        let mut live = head.clone();
        self.visit(body, &mut live, depth + 1);
        for id in killed {
            live.remove(id);
        }
        live.extend(exit);
        if let Some(condition) = condition {
            self.visit(condition, &mut live, depth + 1);
        }
        self.loops.pop();
        live
    }

    fn match_round(
        &mut self,
        arms: &[TypedMatchArm],
        exit: &Live,
        fall: &Live,
        depth: usize,
    ) -> Live {
        let mut entries = Live::new();
        for arm in arms {
            let mut body = exit.clone();
            self.visit(&arm.body, &mut body, depth + 1);
            if let Some(guard) = &arm.guard {
                body.extend(fall);
                // Bindings still alias the subject here. An explicit consuming read of its
                // root must not move storage needed by the pending binding materialization.
                self.opaque(guard, &mut body, depth + 1);
            }
            for alternative in &arm.alternatives {
                let mut entry = body.clone();
                for (binding, projection) in alternative.bindings.iter().rev() {
                    entry.remove(&binding.id);
                    self.opaque(projection, &mut entry, depth + 1);
                }
                for step in alternative.steps.iter().rev() {
                    match step {
                        PatternStep::Bind(local, value) => {
                            entry.remove(&local.id);
                            self.opaque(value, &mut entry, depth + 1);
                        }
                        PatternStep::Test(value) => {
                            entry.extend(fall);
                            self.opaque(value, &mut entry, depth + 1);
                        }
                    }
                }
                entries.extend(entry);
            }
        }
        entries
    }

    fn record_bindings(&mut self, subject: usize, arms: &[TypedMatchArm], owned: bool) {
        let types = self.module.types();
        for arm in arms {
            let disjoint = arm.alternatives.iter().all(|alternative| {
                let paths: Option<Vec<_>> = alternative
                    .bindings
                    .iter()
                    .map(|(_, value)| projection(value, subject, self.module))
                    .collect();
                let Some(paths) = paths else { return false };
                // Nested union-field bindings stay copied until the emitter supports partial-union moves.
                paths
                    .iter()
                    .all(|path| !path.windows(2).any(|steps| steps[0].0))
                    && paths.iter().enumerate().all(|(i, path)| {
                        paths[..i]
                            .iter()
                            .all(|other| !path.starts_with(other) && !other.starts_with(path))
                    })
            });
            for alternative in &arm.alternatives {
                for (binding, _) in &alternative.bindings {
                    let moves = owned
                        && disjoint
                        && !arm.borrowed.contains(&binding.id)
                        && binding.ty.is_copy(&types)
                        && binding.ty.needs_drop(&types);
                    self.bindings
                        .entry(key(binding.span))
                        .and_modify(|old| *old &= moves)
                        .or_insert(moves);
                }
            }
        }
    }
}

fn projection(
    expression: &TypedExpr,
    subject: usize,
    module: &CheckedModule,
) -> Option<Vec<(bool, usize)>> {
    match &expression.kind {
        TypedExprKind::Local(id) if *id == subject => Some(Vec::new()),
        TypedExprKind::Field(value, index) => {
            if value.ty.has_user_drop(&module.types()) {
                return None;
            }
            let mut path = projection(value, subject, module)?;
            path.push((false, *index));
            Some(path)
        }
        TypedExprKind::UnionPayload { value, case_id } => {
            if value.ty.has_user_drop(&module.types()) {
                return None;
            }
            let mut path = projection(value, subject, module)?;
            path.push((true, *case_id));
            Some(path)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn analysis_is_deterministic_and_budget_failure_is_empty() {
        let module = crate::analyze(
            "def f :: [i64] -> [i64] = \\xs -> if xs.length > 1 then Array.__set 0 9 xs else xs\n0",
        )
        .unwrap();
        let function = module.functions.iter().find(|f| f.name == "f").unwrap();
        let first = LastUses::analyze(function, &module);
        assert!(!first.reads.is_empty());
        assert_eq!(first, LastUses::analyze(function, &module));
        assert_eq!(
            LastUses::with_budget(function, &module, 1),
            LastUses::default()
        );
    }

    #[test]
    fn copies_every_read_that_shares_a_position_with_a_copy() {
        let mut module =
            crate::analyze("def f :: [i64] -> ([i64] * [i64]) = \\xs -> (xs, xs)\n0").unwrap();
        let function = module.functions.iter_mut().find(|f| f.name == "f").unwrap();
        let TypedExprKind::Tuple(values) = &mut function.body.kind else {
            panic!("tuple")
        };
        values[1].span = values[0].span;
        let function = module.functions.iter().find(|f| f.name == "f").unwrap();
        assert!(LastUses::analyze(function, &module).reads.is_empty());
    }

    #[test]
    fn excessive_expression_depth_has_no_moves() {
        let mut module = crate::analyze("def f :: [i64] -> [i64] = \\xs -> xs\n0").unwrap();
        let function = module.functions.iter_mut().find(|f| f.name == "f").unwrap();
        for _ in 0..=crate::syntax::MAX_NESTING {
            function.body = TypedExpr {
                kind: TypedExprKind::Cast(Box::new(function.body.clone())),
                ty: function.body.ty.clone(),
                span: function.body.span,
            };
        }
        let function = module.functions.iter().find(|f| f.name == "f").unwrap();
        assert_eq!(LastUses::analyze(function, &module), LastUses::default());
    }
}
