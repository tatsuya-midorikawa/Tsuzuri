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

/// The `ret` lines that a function with a union result wrote, as `emit_tail` reports them.
#[derive(Default)]
pub(super) struct Exits {
    eligible: Option<bool>,
    /// The index of each `ret` line that may move into the slot.
    returns: Vec<usize>,
    /// An exit forwards a call that may recurse, so this function keeps all of its `ret`s.
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
            self.return_exits.returns.push(self.lines.len() - 1);
        } else {
            self.return_exits.keeps_ret = true;
        }
    }

    /// Whether returning `expression` through the slot is safe for tail calls. LLVM only forms a
    /// sibling call or a loop out of `ret (call f)`, so a call that may be part of a recursion cycle
    /// (a mutual or continuation-passing tail recursion over a union) keeps its plain `ret`. A
    /// callee of unknown identity counts as part of a cycle, and so does every exit of a function
    /// that is itself part of one. Shapes that cannot forward a call result may always use the slot.
    fn may_leave_through_slot(&self, expression: &TypedExpr) -> bool {
        use TypedExprKind::*;
        let callee = match &expression.kind {
            Call(callee, _) | Binary(BinaryOp::Pipe, _, callee) => callee,
            Construct { .. }
            | Local(_)
            | Field(..)
            | Index(..)
            | UnionPayload { .. }
            | Dereference(_)
            | Static(_) => return true,
            _ => return false,
        };
        let cyclic = |id: usize| {
            self.globals
                .recursive_functions
                .get(id)
                .copied()
                .unwrap_or(true)
        };
        if cyclic(self.function_id) {
            return false;
        }
        match &callee.kind {
            Function(FunctionRef::Builtin(_)) => true,
            _ => call_specialization::target(callee, &self.known_closures, self.module)
                .is_some_and(|target| !cyclic(target.function)),
        }
    }

    /// Called in `emit` after `facts::apply`, so that the facts see the plain `ret`s. Slotting only
    /// some of the exits would leave several `ret`s, which is what the slot removes, so a function
    /// that has to keep one `ret` keeps them all.
    pub(super) fn apply_return_slot(&mut self) {
        let exits = std::mem::take(&mut self.return_exits);
        if exits.keeps_ret || exits.returns.len() < 2 {
            return;
        }
        let ty = self.ty(&self.function.signature.result);
        rewrite(&mut self.lines, &mut self.allocas, &exits.returns, &ty);
    }
}

/// Moves the `ret` at each index of `exits` into `%tz.return.slot` and adds the shared `tz.return`
/// block. A line that is not the expected `ret` leaves everything as it was.
fn rewrite(lines: &mut Vec<String>, allocas: &mut Vec<String>, exits: &[usize], ty: &str) -> bool {
    let prefix = format!("  ret {ty} ");
    if exits.is_empty()
        || exits.iter().any(|&index| {
            !lines
                .get(index)
                .is_some_and(|line| line.starts_with(&prefix))
        })
    {
        return false;
    }
    allocas.push(format!("%tz.return.slot = alloca {ty}, align 16"));
    let mut rewritten = Vec::with_capacity(lines.len() + exits.len() + 3);
    let mut next = exits.iter().copied().peekable();
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
        assert!(rewrite(&mut body, &mut allocas, &[2, 4], TY));
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
        assert!(rewrite(&mut body, &mut allocas, &[0, 2], TY));
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
    fn leaves_the_function_alone_when_an_index_is_not_a_ret_of_the_result_type() {
        let plain = lines(&[
            &format!("  ret {TY} %v1"),
            "b1:",
            &format!("  ret {TY} %v2"),
        ]);
        for exits in [&[][..], &[1, 2], &[0, 3]] {
            let mut body = plain.clone();
            let mut allocas = Vec::new();
            assert!(!rewrite(&mut body, &mut allocas, exits, TY));
            assert_eq!(body, plain);
            assert!(allocas.is_empty());
        }
    }
}
