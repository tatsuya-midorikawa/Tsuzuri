//! B10 Phase 2: the rules of a call of `Domain.run` (`E1038`). The safety of a failure domain rests on A19: the
//! closure is pure (it reaches no effect, `pure_rules::impurity_of_roots`), it captures plain data only, and its
//! result is plain data, so a trap that abandons it leaves nothing outside half updated (ticket appendix C). The
//! checks read the module after specialization and closure lowering, as `gpu::validate_calls` does.
use super::*;

/// The functions that are `Domain.run` (its instances), by id.
fn runs(module: &CheckedModule) -> BTreeSet<usize> {
    module
        .functions
        .iter()
        .enumerate()
        .filter(|(_, function)| {
            function.origin.module == ModuleOrigin::Std
                && function.module == "Domain"
                && function.name.split('.').next() == Some("run")
        })
        .map(|(id, _)| id)
        .collect()
}

/// The user functions whose bodies are code that runs: instances, not generic templates.
fn user_functions(module: &CheckedModule) -> impl Iterator<Item = &CheckedFunction> {
    module.functions.iter().filter(|function| {
        function.origin.module != ModuleOrigin::Std && function.type_parameters.is_empty()
    })
}

/// A reference to `Domain.run`: the callee of a call with one argument (or of a pipe, `argument |> Domain.run`), with
/// the call and its argument, or any other use.
enum Use<'a> {
    Call(&'a TypedExpr, &'a TypedExpr),
    Other(Span),
}

/// Every use of `Domain.run` in the user's code, in the order of the sources.
fn uses<'a>(module: &'a CheckedModule, runs: &BTreeSet<usize>) -> Vec<Use<'a>> {
    let mut found = Vec::new();
    if runs.is_empty() {
        return found;
    }
    for function in user_functions(module) {
        // The callee of a direct call is visited as part of its call, never on its own.
        let mut pending = vec![&function.body];
        while let Some(expression) = pending.pop() {
            if let TypedExprKind::Call(callee, arguments) = &expression.kind
                && let TypedExprKind::Function(FunctionRef::User(id)) = &callee.kind
                && runs.contains(id)
            {
                match arguments.as_slice() {
                    [argument] => found.push(Use::Call(expression, argument)),
                    _ => found.push(Use::Other(expression.span)),
                }
                pending.extend(arguments);
                continue;
            }
            // `argument |> Domain.run` applies the function to one argument, as a call does.
            if let TypedExprKind::Binary(BinaryOp::Pipe, argument, callee) = &expression.kind
                && let TypedExprKind::Function(FunctionRef::User(id)) = &callee.kind
                && runs.contains(id)
            {
                found.push(Use::Call(expression, argument));
                pending.push(argument);
                continue;
            }
            if let TypedExprKind::Function(FunctionRef::User(id)) = &expression.kind
                && runs.contains(id)
            {
                found.push(Use::Other(expression.span));
                continue;
            }
            pending.extend(expression.children());
        }
    }
    found.sort_by_key(|found| {
        let span = match found {
            Use::Call(call, _) => call.span,
            Use::Other(span) => *span,
        };
        (span.source, span.start, span.end)
    });
    found
}

/// The spans of the calls of `Domain.run` in the user's code, in the order of the sources. A program that has one
/// is built with the failure-domain runtime (src/runtime/trap.c).
pub(crate) fn domain_calls(module: &CheckedModule) -> Vec<Span> {
    let runs = runs(module);
    uses(module, &runs)
        .into_iter()
        .map(|found| match found {
            Use::Call(call, _) => call.span,
            Use::Other(span) => span,
        })
        .collect()
}

fn error(kind: DiagnosticKind, message: impl Into<String>, span: Span) -> Diagnostic {
    Diagnostic::of(kind, message, span)
}

/// The rules of `E1038` for each call, in their order; each call reports its first violation only.
pub(super) fn validate(module: &CheckedModule) -> Vec<Diagnostic> {
    let runs = runs(module);
    let types = module.types();
    let mut diagnostics = Vec::new();
    for found in uses(module, &runs) {
        let (call, argument) = match found {
            Use::Call(call, argument) => (call, argument),
            Use::Other(span) => {
                diagnostics.push(error(
                    DiagnosticKind::DomainNotCalledDirectly,
                    "'Domain.run' must be called directly with its closure; write 'Domain.run (\\() -> ...)'",
                    span,
                ));
                continue;
            }
        };
        // Rule 1: a lambda (with or without captures) or the name of a pure def. A pure function value passed where
        // the parameter's type is not pure comes through a cast of its type.
        let mut value = argument;
        while let TypedExprKind::Cast(inner) = &value.kind
            && matches!(inner.ty, Type::Function(..))
        {
            value = inner;
        }
        let (body, captures, checked) = match &value.kind {
            TypedExprKind::Closure(id, _) => (*id, module.functions[*id].capture_count, false),
            // A lambda without captures, lowered to its function. A named def, an instance of a generic one
            // included, must be a `pure def`.
            TypedExprKind::Function(FunctionRef::User(id))
                if module.functions[*id].module == "$lambda" =>
            {
                (*id, 0, false)
            }
            TypedExprKind::Function(FunctionRef::User(id)) if module.functions[*id].origin.pure => {
                (*id, 0, true)
            }
            _ => {
                diagnostics.push(error(
                    DiagnosticKind::DomainArgumentNotClosure,
                    "the argument of 'Domain.run' must be a lambda or the name of a pure def; write 'Domain.run (\\() -> ...)'",
                    argument.span,
                ));
                continue;
            }
        };
        // Rule 2: what it captures is plain data (a shared reference to plain data included).
        let function = &module.functions[body];
        let capture = function.parameters[..captures.min(function.parameters.len())]
            .iter()
            .find_map(|local| {
                let ty = match &local.ty {
                    Type::Reference(inner, false) => inner.as_ref(),
                    ty => ty,
                };
                pure_rules::own_violation(ty, &types).map(|part| (local, part))
            });
        if let Some((local, part)) = capture {
            diagnostics.push(error(
                DiagnosticKind::DomainCaptureNotPlain,
                format!(
                    "the closure passed to 'Domain.run' captures '{}', which holds {}; capture plain data or compute the value inside the closure",
                    local.name,
                    pure_rules::what(&part, &types)
                ),
                argument.span,
            ));
            continue;
        }
        // Rule 3: the body reaches no effect (A19); a pure def was checked as one already.
        if !checked
            && let Some(impurity) = pure_rules::impurity_of_roots(module, &[body]).remove(&body)
        {
            let chain = if impurity.chain.is_empty() {
                String::new()
            } else {
                format!(" (through {})", impurity.chain)
            };
            diagnostics.push(error(
                DiagnosticKind::DomainBodyNotPure,
                format!(
                    "the closure passed to 'Domain.run' must be pure: it {}{chain}; move that work out of the closure",
                    impurity.reason
                ),
                impurity.span,
            ));
            continue;
        }
        // Rule 4: the result is plain data.
        if let Type::Union(_, arguments) = &call.ty
            && let Some(result) = arguments.first()
            && let Some(part) = pure_rules::own_violation(result, &types)
        {
            diagnostics.push(error(
                DiagnosticKind::DomainResultNotPlain,
                format!(
                    "the result of 'Domain.run' must be plain data; '{}' holds {}",
                    result.display(&types),
                    pure_rules::what(&part, &types)
                ),
                call.span,
            ));
        }
    }
    diagnostics
}
