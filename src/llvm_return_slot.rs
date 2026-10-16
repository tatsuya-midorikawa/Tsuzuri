use super::*;

/// The largest union result that gets a return slot. A function that returns a bigger union keeps its `ret`s.
const MAX_SLOT_BYTES: usize = 1024;

/// Whether a function that returns `ty` may return through a slot: a union stored as a first-class
/// aggregate (not a bare tag) of a size that an entry-block `alloca` can hold cheaply. A layout that
/// is a scalar must not match here, because a scalar result never becomes an aggregate `phi`.
pub(super) fn uses_return_slot(ty: &Type, module: &CheckedModule) -> bool {
    let Type::Union(id, arguments) = ty else {
        return false;
    };
    !module.types().recursive(ty)
        && !matches!(union_layout(*id, arguments, module), UnionLayout::Enum)
        && storage_layout(ty, module).0 <= MAX_SLOT_BYTES
}

/// The union exits that a function wrote with `emit_tail`, as they were reported to `note_union_return`.
#[derive(Default)]
pub(super) struct Exits {
    eligible: Option<bool>,
    /// How many `ret` lines may move into the slot.
    count: usize,
    /// An exit may hold the result of a call, so this function keeps all of its `ret`s.
    keeps_ret: bool,
}

impl FunctionEmitter<'_, '_> {
    /// Called right after `emit_tail` wrote the `ret` of `expression`.
    pub(super) fn note_union_return(&mut self, expression: &TypedExpr) {
        let result = &self.function.signature.result;
        let eligible = *self
            .return_exits
            .eligible
            .get_or_insert_with(|| uses_return_slot(result, self.module));
        if !eligible {
            return;
        }
        if self.may_leave_through_slot(expression) {
            self.return_exits.count += 1;
        } else {
            self.return_exits.keeps_ret = true;
        }
    }

    /// Whether returning `expression` through the slot keeps LLVM's tail calls. LLVM turns `ret (call f)`
    /// into a sibling call or a loop, which is what lets a mutual recursion, a recursion through a
    /// local, or a continuation-passing recursion over a union run in a constant stack. A slot hides
    /// the call, and no analysis of the call graph proves that a callee cannot lead back: the recursion
    /// may run through a closure that a caller passes as an argument. So an exit that may hold the result
    /// of a call (a call, a pipeline, a local that is not a parameter, or any other shape) keeps the `ret`s of
    /// its function. An exit that builds a value, or reads one from storage or from a parameter, has no call to hide.
    fn may_leave_through_slot(&self, expression: &TypedExpr) -> bool {
        use TypedExprKind::*;
        match &expression.kind {
            Construct { .. }
            | Field(..)
            | Index(..)
            | UnionPayload { .. }
            | Dereference(_)
            | Static(_) => true,
            Local(id) => self
                .function
                .parameters
                .iter()
                .any(|parameter| parameter.id == *id && !parameter.mutable),
            _ => false,
        }
    }

    /// Called in `emit` after `facts::apply`, so that the facts see the plain `ret`s. Slotting only
    /// some of the exits would leave several `ret`s, which is what the slot removes, so a function
    /// that has to keep one `ret` keeps them all.
    pub(super) fn apply_return_slot(&mut self) {
        let exits = std::mem::take(&mut self.return_exits);
        if exits.keeps_ret || exits.count < 2 {
            return;
        }
        let ty = self.ty(&self.function.signature.result);
        rewrite(&mut self.lines, &mut self.allocas, exits.count, &ty);
    }
}

/// Moves each `ret` of `lines` into `%tz.return.slot` and adds the shared `tz.return` block. The
/// exits are found in the final lines, because `facts::apply` puts scope declarations in front of the
/// body. Everything stays as it was unless the function has exactly `exits` `ret` lines, all of them of
/// type `ty`.
fn rewrite(lines: &mut Vec<String>, allocas: &mut Vec<String>, exits: usize, ty: &str) -> bool {
    let prefix = format!("  ret {ty} ");
    let returns: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter(|(_, line)| line.starts_with("  ret "))
        .map(|(index, _)| index)
        .collect();
    if exits == 0
        || returns.len() != exits
        || returns
            .iter()
            .any(|&index| !lines[index].starts_with(&prefix))
    {
        return false;
    }
    allocas.push(format!("%tz.return.slot = alloca {ty}, align 16"));
    let mut rewritten = Vec::with_capacity(lines.len() + exits + 3);
    let mut next = returns.iter().copied().peekable();
    for (index, line) in std::mem::take(lines).into_iter().enumerate() {
        if next.next_if_eq(&index).is_none() {
            rewritten.push(line);
            continue;
        }
        let operand = &line[prefix.len()..];
        // `instruction` appends the debug location, which both new instructions keep.
        let (value, location) = operand
            .rsplit_once(", !dbg !")
            .map_or((operand, String::new()), |(value, scope)| {
                (value, format!(", !dbg !{scope}"))
            });
        rewritten.push(format!(
            "  store {ty} {value}, ptr %tz.return.slot{location}"
        ));
        rewritten.push(format!("  br label %tz.return{location}"));
    }
    rewritten.push("tz.return:".to_owned());
    rewritten.push(format!(
        "  %tz.return.value = load {ty}, ptr %tz.return.slot"
    ));
    rewritten.push(format!("  ret {ty} %tz.return.value"));
    *lines = rewritten;
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    const TY: &str = "%\"tz.union.Maybe.Maybe[i64]\"";

    fn lines(text: &[&str]) -> Vec<String> {
        text.iter().map(|line| (*line).to_owned()).collect()
    }

    #[test]
    fn moves_every_exit_through_one_slot() {
        let mut body = lines(&[
            "  br i1 %v0, label %b0, label %b1",
            "b0:",
            "  ret %\"tz.union.Maybe.Maybe[i64]\" %v1",
            "b1:",
            "  ret %\"tz.union.Maybe.Maybe[i64]\" zeroinitializer",
        ]);
        let mut allocas = vec!["%v9 = alloca i64, align 16".to_owned()];
        assert!(rewrite(&mut body, &mut allocas, 2, TY));
        assert_eq!(
            allocas,
            [
                "%v9 = alloca i64, align 16".to_owned(),
                format!("%tz.return.slot = alloca {TY}, align 16"),
            ]
        );
        assert_eq!(
            body,
            lines(&[
                "  br i1 %v0, label %b0, label %b1",
                "b0:",
                &format!("  store {TY} %v1, ptr %tz.return.slot"),
                "  br label %tz.return",
                "b1:",
                &format!("  store {TY} zeroinitializer, ptr %tz.return.slot"),
                "  br label %tz.return",
                "tz.return:",
                &format!("  %tz.return.value = load {TY}, ptr %tz.return.slot"),
                &format!("  ret {TY} %tz.return.value"),
            ])
        );
    }

    #[test]
    fn keeps_the_debug_location_of_each_exit() {
        let mut body = lines(&[
            &format!("  ret {TY} %v3, !dbg !7"),
            "b1:",
            &format!("  ret {TY} %v4, !dbg !8"),
        ]);
        let mut allocas = Vec::new();
        assert!(rewrite(&mut body, &mut allocas, 2, TY));
        assert_eq!(
            body[..2],
            [
                format!("  store {TY} %v3, ptr %tz.return.slot, !dbg !7"),
                "  br label %tz.return, !dbg !7".to_owned(),
            ]
        );
        assert_eq!(
            body[3..5],
            [
                format!("  store {TY} %v4, ptr %tz.return.slot, !dbg !8"),
                "  br label %tz.return, !dbg !8".to_owned(),
            ]
        );
        assert_eq!(body.len(), 8);
    }

    #[test]
    fn finds_the_exits_behind_lines_that_facts_put_in_front() {
        let scope = "  call void @llvm.experimental.noalias.scope.decl(metadata !3)";
        let mut body = lines(&[
            scope,
            scope,
            "  br i1 %v0, label %b0, label %b1",
            "b0:",
            &format!("  ret {TY} %v1"),
            "b1:",
            &format!("  ret {TY} zeroinitializer"),
        ]);
        let mut allocas = Vec::new();
        assert!(rewrite(&mut body, &mut allocas, 2, TY));
        assert_eq!(body[..2], [scope.to_owned(), scope.to_owned()]);
        assert_eq!(body[4], format!("  store {TY} %v1, ptr %tz.return.slot"));
        assert_eq!(
            body[7],
            format!("  store {TY} zeroinitializer, ptr %tz.return.slot")
        );
        assert_eq!(body.len(), 12);
    }

    #[test]
    fn leaves_the_function_alone_unless_it_has_exactly_the_reported_rets_of_the_result_type() {
        let plain = lines(&[
            &format!("  ret {TY} %v1"),
            "b1:",
            &format!("  ret {TY} %v2"),
        ]);
        let other = lines(&[
            &format!("  ret {TY} %v1"),
            "b1:",
            "  ret %\"tz.union.Result.Result[i64,i64]\" %v2",
        ]);
        // No exit was reported, one more or one fewer than the lines hold, or a `ret` of another type.
        for (body, exits) in [(&plain, 0), (&plain, 1), (&plain, 3), (&other, 2)] {
            let mut copy = body.clone();
            let mut allocas = Vec::new();
            assert!(!rewrite(&mut copy, &mut allocas, exits, TY));
            assert_eq!(&copy, body);
            assert!(allocas.is_empty());
        }
    }
}
