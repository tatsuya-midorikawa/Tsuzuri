//! Bounded backwards liveness on concrete IR, shared by emission and the Copy inventory.
use std::collections::{BTreeMap, BTreeSet};

use crate::check::{
    CheckedFunction, CheckedModule, FunctionRef, PatternStep, Type, TypeContext, TypedExpr,
    TypedExprKind, TypedMatchArm,
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
            b3_roots: BTreeMap::new(),
            call_borrowed_roots: BTreeMap::new(),
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
        if !analysis.classify_borrows(function) {
            return Self::default();
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

fn expr_addr(expression: &TypedExpr) -> usize {
    std::ptr::from_ref(expression).addr()
}

fn has_open_type(ty: &Type, types: &TypeContext<'_>) -> bool {
    let mut seen = BTreeSet::new();
    fn walk<'a>(ty: &'a Type, types: &TypeContext<'a>, seen: &mut BTreeSet<usize>) -> bool {
        match ty {
            Type::Error
            | Type::Variable(_)
            | Type::Infer(_)
            | Type::Partial(_)
            | Type::Application(..) => true,
            Type::Array(inner)
            | Type::ArrayView(inner)
            | Type::List(inner)
            | Type::Vec(inner)
            | Type::Shared(inner, _)
            | Type::Once(inner)
            | Type::FnPtr(inner)
            | Type::Reference(inner, _) => walk(inner, types, seen),
            Type::FixedArray(element, length) => {
                walk(element, types, seen) || walk(length, types, seen)
            }
            Type::Tuple(elements) => elements.iter().any(|t| walk(t, types, seen)),
            Type::Function(parameters, result) => {
                parameters.iter().any(|t| walk(t, types, seen)) || walk(result, types, seen)
            }
            Type::Record(id, arguments) => {
                arguments.iter().any(|t| walk(t, types, seen))
                    || (arguments.is_empty()
                        && seen.insert(*id)
                        && types.records[*id]
                            .fields
                            .iter()
                            .any(|(_, ft)| walk(ft, types, seen)))
            }
            Type::Union(id, arguments) => {
                arguments.iter().any(|t| walk(t, types, seen))
                    || (arguments.is_empty()
                        && seen.insert(usize::MAX - *id)
                        && types.unions[*id]
                            .cases
                            .iter()
                            .filter_map(|(_, payload)| payload.as_ref())
                            .any(|pt| walk(pt, types, seen)))
            }
            _ => false,
        }
    }
    walk(ty, types, &mut seen)
}

/// PM10 B1: whether a call result of concrete `ty` may hold a loan.
fn result_holds_loan(ty: &Type, types: &TypeContext<'_>) -> bool {
    has_open_type(ty, types) || crate::ownership::type_holds_loan(ty, types)
}

/// PM10 B1: whether a value of concrete `ty` reaches any place where a borrow could be stored:
/// (i) `ref mut` to a loan-holding type (`without_views` turns `[T..]` into `[T]`),
/// (ii) a function value, `dyn` value, or `Owned.Function`, or
/// (iii) a mutable cell whose contents may hold a loan (`Type::is_mutable_cell`).
fn can_store_loan(ty: &Type, types: &TypeContext<'_>) -> bool {
    if has_open_type(ty, types) {
        return true;
    }
    let plain = crate::ownership::without_views(ty);
    types.reaches(&plain, |part| {
        matches!(part, Type::Function(..) | Type::Dyn(_))
            || part.is_owned_function(types)
            || part.is_mutable_cell(types)
            || matches!(
                part,
                Type::Reference(target, true) if crate::ownership::type_holds_loan(target, types)
            )
    })
}

fn is_direct_non_extern_callee(callee: &TypedExpr, module: &CheckedModule) -> bool {
    match &callee.kind {
        TypedExprKind::Function(FunctionRef::Builtin(_)) | TypedExprKind::Method(..) => true,
        TypedExprKind::Function(FunctionRef::User(id)) | TypedExprKind::GenericFunction(id, _) => {
            !matches!(module.functions[*id].body.kind, TypedExprKind::HostCall(..))
        }
        _ => false,
    }
}

fn is_immut_param_callee(callee: &TypedExpr, immut_params: &BTreeSet<usize>) -> bool {
    let mut expr = callee;
    if let TypedExprKind::BorrowOperand(inner) = &expr.kind {
        expr = inner;
    }
    match &expr.kind {
        TypedExprKind::Local(id) => immut_params.contains(id),
        TypedExprKind::Dereference(inner) => {
            matches!(&inner.kind, TypedExprKind::Local(id) if immut_params.contains(id))
        }
        _ => false,
    }
}

struct LoopContext {
    exit: Live,
    head: Live,
}

struct Analysis<'a> {
    module: &'a CheckedModule,
    excluded: Live,
    b3_roots: BTreeMap<usize, Live>,
    call_borrowed_roots: BTreeMap<usize, Live>,
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

    fn mark_live(&self, live: &mut Live, id: usize) {
        live.insert(id);
        if let Some(roots) = self.b3_roots.get(&id) {
            live.extend(roots);
        }
    }

    /// Unwraps a chain of `Borrow` / `BorrowOperand` / `Slice` nodes (including the intermediate
    /// `Dereference` inserted by `Checker::reborrow_operand` when coercing `ref mut data` or
    /// `ref mut out[0..]` to `ref mut [T..]`) to its underlying place `root`, returning the root
    /// local and the addresses of every borrow/slice node in the chain.
    fn borrow_chain(&self, mut expression: &TypedExpr) -> Option<(usize, Vec<usize>)> {
        let mut chain = Vec::new();
        loop {
            match &expression.kind {
                TypedExprKind::BorrowOperand(value) if self.lent_union_match(value) => break,
                TypedExprKind::Borrow(value, _)
                | TypedExprKind::BorrowOperand(value)
                | TypedExprKind::Slice { value, .. } => {
                    chain.push(expr_addr(expression));
                    expression = value;
                }
                TypedExprKind::Dereference(inner)
                    if !chain.is_empty()
                        && matches!(
                            inner.kind,
                            TypedExprKind::Borrow(..)
                                | TypedExprKind::BorrowOperand(_)
                                | TypedExprKind::Slice { .. }
                        ) =>
                {
                    expression = inner;
                }
                _ => break,
            }
        }
        if chain.is_empty() {
            return None;
        }
        root(expression).map(|id| (id, chain))
    }

    fn count_local(&mut self, expression: &TypedExpr, target: usize, depth: usize) -> usize {
        let mut count = 0;
        let mut pending = vec![(expression, depth)];
        while let Some((expr, d)) = pending.pop() {
            if !self.spend(d) {
                return usize::MAX;
            }
            if matches!(expr.kind, TypedExprKind::Local(id) if id == target) {
                count += 1;
            }
            let _ = expr.try_for_each_child(&mut |child| {
                pending.push((child, d + 1));
                Ok(())
            });
        }
        count
    }

    /// PM10 Phase 2: classifies every borrow in `function.body` under rules B1–B4 (within budget B5).
    fn classify_borrows(&mut self, function: &CheckedFunction) -> bool {
        let types = self.module.types();
        let immut_params: BTreeSet<usize> = if function.capture_count == 0 {
            function.parameters[function.capture_count..]
                .iter()
                .filter(|p| !p.mutable)
                .map(|p| p.id)
                .collect()
        } else {
            BTreeSet::new()
        };
        let own_loan_params: BTreeSet<usize> = function
            .parameters
            .iter()
            .filter(|p| result_holds_loan(&p.ty, &types))
            .map(|p| p.id)
            .collect();

        let mut assigned = BTreeSet::new();
        let mut total_uses: BTreeMap<usize, usize> = BTreeMap::new();
        let mut all_borrows: Vec<(&TypedExpr, usize)> = Vec::new();
        let mut calls: Vec<(&TypedExpr, usize)> = Vec::new();
        let mut b3_bindings: Vec<(usize, &Type, &TypedExpr, usize)> = Vec::new();

        let mut pending = vec![(&function.body, 0usize)];
        while let Some((expression, depth)) = pending.pop() {
            if !self.spend(depth) {
                return false;
            }
            use TypedExprKind::*;
            match &expression.kind {
                Local(id) => {
                    *total_uses.entry(*id).or_default() += 1;
                }
                Assign(place, _) => {
                    if let Some(id) = root(place) {
                        assigned.insert(id);
                    }
                }
                Borrow(..) | BorrowOperand(_) | Slice { .. } => {
                    if let Some((id, _)) = self.borrow_chain(expression) {
                        all_borrows.push((expression, id));
                    }
                }
                Call(..) => {
                    calls.push((expression, depth));
                }
                Block { bindings, .. } => {
                    for (local, init) in bindings {
                        if matches!(init.kind, Call(..)) {
                            b3_bindings.push((local.id, &local.ty, init, depth + 1));
                        }
                    }
                }
                ForEach {
                    owner,
                    local,
                    source,
                    ..
                } => {
                    self.excluded.extend([owner.id, local.id]);
                    if let Some(id) = root(source) {
                        self.excluded.insert(id);
                    }
                }
                ForRange { local, .. } => {
                    self.excluded.insert(local.id);
                }
                Match { local, arms, .. } => {
                    self.excluded.insert(local.id);
                    for arm in arms {
                        for alternative in &arm.alternatives {
                            self.excluded
                                .extend(alternative.bindings.iter().map(|(b, _)| b.id));
                            for step in &alternative.steps {
                                if let PatternStep::Bind(local, _) = step {
                                    self.excluded.insert(local.id);
                                }
                            }
                        }
                    }
                }
                Lambda { captures, .. } => {
                    self.excluded.extend(captures.iter().map(|local| local.id));
                }
                _ => {}
            }
            let _ = expression.try_for_each_child(&mut |child| {
                pending.push((child, depth + 1));
                Ok(())
            });
        }

        // B1: direct borrow arguments of calls whose result holds no loan and whose other arguments
        // and callee cannot store a loan.
        let mut approved_borrows = BTreeSet::new();
        let mut b1_sole_uses: BTreeMap<usize, usize> = BTreeMap::new();
        let mut call_b1_args: BTreeMap<usize, Vec<usize>> = BTreeMap::new();

        for &(call_expr, depth) in &calls {
            let TypedExprKind::Call(callee, arguments) = &call_expr.kind else {
                continue;
            };
            if result_holds_loan(&call_expr.ty, &types) {
                continue;
            }
            let direct_ok = is_direct_non_extern_callee(callee, self.module);
            let param_ok = !direct_ok && is_immut_param_callee(callee, &immut_params);
            if !direct_ok && !param_ok {
                continue;
            }
            let can_store: Vec<bool> = arguments
                .iter()
                .map(|arg| can_store_loan(&arg.ty, &types))
                .collect();
            let store_count = can_store.iter().filter(|&&s| s).count();
            for (idx, arg) in arguments.iter().enumerate() {
                let Some((r_id, chain)) = self.borrow_chain(arg) else {
                    continue;
                };
                if param_ok && own_loan_params.contains(&r_id) {
                    continue;
                }
                let others_can_store = store_count > usize::from(can_store[idx]);
                if others_can_store {
                    continue;
                }
                approved_borrows.extend(chain);
                call_b1_args
                    .entry(expr_addr(call_expr))
                    .or_default()
                    .push(r_id);
                let occ = self.count_local(arg, r_id, depth + 1);
                if self.exhausted {
                    return false;
                }
                if occ == 1 {
                    *b1_sole_uses.entry(r_id).or_default() += 1;
                }
            }
        }

        // B3: `let L = call(...)` where `call` returns a loan-carrying value, has no other
        // loan-storing argument/callee, `L` is never reassigned and has no user `Drop`, and every
        // occurrence of `L` in the function is a sole-root borrow in a B1 (or outer B3) call.
        // Walk in reverse order so inner/later window bindings validate before outer/earlier ones.
        for &(local_id, local_ty, init_expr, depth) in b3_bindings.iter().rev() {
            if assigned.contains(&local_id) || has_open_type(local_ty, &types) {
                continue;
            }
            let plain_local = crate::ownership::without_views(local_ty);
            if types.reaches(&plain_local, |part| part.has_user_drop(&types)) {
                continue;
            }
            let uses = total_uses.get(&local_id).copied().unwrap_or(0);
            let approved = b1_sole_uses.get(&local_id).copied().unwrap_or(0);
            if uses != approved {
                continue;
            }
            let TypedExprKind::Call(callee, arguments) = &init_expr.kind else {
                continue;
            };
            if !result_holds_loan(&init_expr.ty, &types) {
                continue;
            }
            let direct_ok = is_direct_non_extern_callee(callee, self.module);
            let param_ok = !direct_ok && is_immut_param_callee(callee, &immut_params);
            if !direct_ok && !param_ok {
                continue;
            }
            // If `L` is used in any `param_ok` B1 call, ensure none of the roots borrowed here
            // come from `own_loan_params`.
            let can_store: Vec<bool> = arguments
                .iter()
                .map(|arg| can_store_loan(&arg.ty, &types))
                .collect();
            let store_count = can_store.iter().filter(|&&s| s).count();
            let mut candidate_args: Vec<(&TypedExpr, usize, Vec<usize>)> = Vec::new();
            let mut all_borrow_args_ok = true;
            for (idx, arg) in arguments.iter().enumerate() {
                if let Some((r_id, chain)) = self.borrow_chain(arg) {
                    if own_loan_params.contains(&r_id) || store_count > usize::from(can_store[idx])
                    {
                        all_borrow_args_ok = false;
                        break;
                    }
                    candidate_args.push((arg, r_id, chain));
                } else if can_store[idx] {
                    all_borrow_args_ok = false;
                    break;
                }
            }
            if !all_borrow_args_ok || candidate_args.is_empty() {
                continue;
            }
            let mut roots_set = Live::new();
            for (arg, r_id, chain) in &candidate_args {
                approved_borrows.extend(chain.iter().copied());
                roots_set.insert(*r_id);
                let occ = self.count_local(arg, *r_id, depth + 1);
                if self.exhausted {
                    return false;
                }
                if occ == 1 {
                    *b1_sole_uses.entry(*r_id).or_default() += 1;
                }
            }
            call_b1_args
                .entry(expr_addr(init_expr))
                .or_default()
                .extend(roots_set.iter().copied());
            self.b3_roots.insert(local_id, roots_set);
        }

        // Transitively close `b3_roots` (for chained windows `L2` borrowing `L1` borrowing `data`).
        let b3_keys: Vec<usize> = self.b3_roots.keys().copied().collect();
        loop {
            let mut changed = false;
            for &k in &b3_keys {
                let current: Vec<usize> = self.b3_roots[&k].iter().copied().collect();
                let mut extra = Live::new();
                for r in current {
                    if let Some(sub) = self.b3_roots.get(&r) {
                        extra.extend(sub);
                    }
                }
                let entry = self.b3_roots.get_mut(&k).unwrap();
                let before = entry.len();
                entry.extend(extra);
                if entry.len() != before {
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }

        // B4: any borrow expression not approved by B1 or B3 excludes its root (and transitive B3
        // backing roots) across the whole function.
        for (borrow_expr, r_id) in all_borrows {
            if !approved_borrows.contains(&expr_addr(borrow_expr)) {
                self.excluded.insert(r_id);
                if let Some(backing) = self.b3_roots.get(&r_id) {
                    self.excluded.extend(backing);
                }
            }
        }

        // B2: record the roots (and transitive B3 backing roots) live at each call execution point.
        for (call_addr, raw_roots) in call_b1_args {
            let mut expanded = Live::new();
            for r_id in raw_roots {
                expanded.insert(r_id);
                if let Some(backing) = self.b3_roots.get(&r_id) {
                    expanded.extend(backing);
                }
            }
            self.call_borrowed_roots.insert(call_addr, expanded);
        }
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

    /// Whether `operand`, lent to a call, is a `match`, an `if` or a block whose value is an
    /// anonymous union: one of its branches is the value, such as a widening (review round 3).
    fn lent_union_match(&self, operand: &TypedExpr) -> bool {
        matches!(
            operand.kind,
            TypedExprKind::Match { .. } | TypedExprKind::If { .. } | TypedExprKind::Block { .. }
        ) && matches!(&operand.ty, Type::Union(id, _) if self.module.unions[*id].is_anonymous())
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
                    self.mark_live(live, id);
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
                self.mark_live(live, *id);
            }
            Unary(_, value)
            | Cast(value)
            | SaturatingIndex(value)
            | FromEnd { offset: value, .. }
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
            | ReferenceOperand(_)
            | Index(..) => self.opaque(expression, live, depth),
            Binary(BinaryOp::And | BinaryOp::Or, a, b) => {
                let mut conditional = live.clone();
                self.visit(b, &mut conditional, depth + 1);
                live.extend(conditional);
                self.visit(a, live, depth + 1);
            }
            Binary(BinaryOp::Pipe, ..) => self.opaque(expression, live, depth),
            // A25: the temporary that a read parameter borrows is evaluated as a value before
            // the call (`operand_borrow`). When it is a `match` that makes an anonymous union,
            // as a widening is (`Checker::retag`), or an `if` or a block whose branch is one,
            // it is read as any other value, so an owned payload moves into the wider value.
            // Every other lent operand stays opaque, which keeps the reads of programs without
            // anonymous unions as they were.
            BorrowOperand(value) if self.lent_union_match(value) => {
                self.visit(value, live, depth + 1)
            }
            Binary(_, a, b) => {
                self.visit(b, live, depth + 1);
                self.visit(a, live, depth + 1);
            }
            Call(callee, arguments) => {
                if let Some(roots) = self.call_borrowed_roots.get(&expr_addr(expression)) {
                    live.extend(roots);
                }
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
                    self.mark_live(live, id);
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
                    let movable = binding.ty.is_copy(&types) && binding.ty.needs_drop(&types);
                    // Only the take of a Copy value that needs a drop reads a binding's decision.
                    // A retagging gives the binding of a member that names a type variable the span
                    // of its array members' bindings, and an instance may make that member a type
                    // that is not both (`Maybe<i64>`, `Maybe<string>`): its binding must not take
                    // their move (A25, Copilot review round 2). Other bindings keep their vote.
                    if !movable && binding.retags_payload() {
                        continue;
                    }
                    let moves = owned && disjoint && !arm.borrowed.contains(&binding.id) && movable;
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
