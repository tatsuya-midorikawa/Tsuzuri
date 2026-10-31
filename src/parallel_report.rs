//! `tsuzuri check --report parallel` (F15 Phase 3): the calls of `Array` that `Parallel` can replace without
//! changing the result (suggestions, never applied), and whether the callbacks of the existing `Parallel` calls are
//! pure (`parallel_safe`). It only reads the checked module after specialization and closure lowering, and the
//! purity questions go to A19's query boundary (`pure_rules::reachable_effect`, D-53).
//!
//! The report is deterministic: the entries are kept in ordered sets and sorted by path and position, and nothing
//! of the machine (absolute paths, times, environment values) goes into it.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use crate::check::effects::builtin_facts;
use crate::check::pure_rules::{self, Effect, LimitExceeded};
use crate::check::{
    Builtin, CheckedModule, FunctionRef, ModuleOrigin, Type, TypeContext, TypedExpr, TypedExprKind,
};
use crate::diagnostic::{Span, json_string, location};
use crate::syntax::Provenance;

/// The most entries that a report holds; the ones after them in the order of the report are counted in `truncated`.
pub const MAX_ENTRIES: usize = 1000;
/// An input shorter than this many elements is one chunk of `Parallel`, so it is not suggested.
pub const SMALL_INPUT: u64 = 4096;
/// A function with more expressions than this is skipped (the report does not fail).
const MAX_NODES_PER_FUNCTION: usize = 100_000;

/// What an entry is: a suggestion (sorted first at the same position) or an existing `Parallel` call.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum EntryKind {
    Suggestion,
    Site,
}

/// One line of the report.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub kind: EntryKind,
    /// The call as written: `Array.map`, `Parallel.fold`.
    pub call: &'static str,
    /// The `Parallel` function that a suggestion puts in place of `call`.
    pub replacement: Option<&'static str>,
    /// The function of `CheckedModule::functions` that holds the call (the first specialization that does).
    pub function: usize,
    /// The name of the call for a suggestion (what the edit replaces); the whole call expression for a site.
    pub span: Span,
    pub parallel_safe: bool,
    /// Why a site is not `parallel_safe`.
    pub reason: Option<String>,
    /// Why a suggestion keeps the result.
    pub why: Option<&'static str>,
}

/// The entries in the order of the report, and how many were left out after `MAX_ENTRIES`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Report {
    pub entries: Vec<Entry>,
    pub truncated: usize,
}

/// The source files that a report reads: the text for the suggestion condition and the path for the output.
pub trait Sources {
    fn text(&self, span: Span) -> &str;
    fn path(&self, span: Span) -> String;
}

/// The `Array` functions that have a `Parallel` replacement, with the replacement and why it keeps the result.
const REPLACEMENTS: &[(&str, &str, &str)] = &[
    ("Array.map", "Parallel.map", "the callback is pure"),
    ("Array.mapr", "Parallel.map_ref", "the callback is pure"),
    (
        "Array.fold_monoid",
        "Parallel.fold",
        "the result is the same by the laws of Monoid",
    ),
    (
        "Array.sum",
        "Parallel.sum",
        "integer sums are the same in any order",
    ),
];

/// The `Parallel` functions of std (not builtins) whose calls are sites.
const STD_SITES: &[&str] = &["Parallel.sum", "Parallel.fold"];

/// Whether a callback is pure, and why not.
enum Purity {
    Pure,
    Impure(String),
}

/// A candidate entry of one specialization; the copies of the same position are merged in `build`.
struct Found {
    entry: Option<Entry>,
}

/// The report of a module that checked: every user function (lambdas and generated helpers included) is scanned for
/// the calls of `REPLACEMENTS` and of the `Parallel` functions.
pub fn build(module: &CheckedModule, sources: &dyn Sources) -> Report {
    let mut entries = entries(module, sources);
    let truncated = entries.len().saturating_sub(MAX_ENTRIES);
    entries.truncate(MAX_ENTRIES);
    Report { entries, truncated }
}

/// Every entry of the report, in its order, before the `MAX_ENTRIES` cut; the knowledge graph's facts (G26) take
/// them by function.
pub(crate) fn entries(module: &CheckedModule, sources: &dyn Sources) -> Vec<Entry> {
    // (path, start, end, kind, call): one entry per position however many specializations hold it. A suggestion
    // that one specialization disqualifies is not made; a site is `parallel_safe` only when every copy is.
    let mut found: BTreeMap<(String, usize, usize, EntryKind, &'static str), Found> =
        BTreeMap::new();
    for (index, function) in module.functions.iter().enumerate() {
        if function.origin.module != ModuleOrigin::User {
            continue;
        }
        let mut pending = vec![&function.body];
        let mut nodes = Vec::new();
        while let Some(expression) = pending.pop() {
            nodes.push(expression);
            if nodes.len() > MAX_NODES_PER_FUNCTION {
                break;
            }
            pending.extend(expression.children());
        }
        if nodes.len() > MAX_NODES_PER_FUNCTION {
            continue;
        }
        for expression in nodes {
            let Some(candidate) = inspect(module, sources, index, expression) else {
                continue;
            };
            let (kind, call, span, entry) = candidate;
            let key = (sources.path(span), span.start, span.end, kind, call);
            match found.get_mut(&key) {
                None => {
                    found.insert(key, Found { entry });
                }
                Some(previous) => merge(previous, entry),
            }
        }
    }
    found
        .into_values()
        .filter_map(|found| found.entry)
        .collect()
}

/// Merges another specialization's view of the same position into `previous`.
fn merge(previous: &mut Found, entry: Option<Entry>) {
    let Some(kept) = &mut previous.entry else {
        return;
    };
    match entry {
        None => previous.entry = None,
        Some(entry) => {
            if kept.parallel_safe && !entry.parallel_safe {
                kept.parallel_safe = false;
                kept.reason = entry.reason;
            }
        }
    }
}

type Candidate = (EntryKind, &'static str, Span, Option<Entry>);

/// The entry that `expression` gives, if it is a call that the report is about; `None` in the last place when it is
/// such a call but a suggestion's conditions do not hold.
fn inspect(
    module: &CheckedModule,
    sources: &dyn Sources,
    function: usize,
    expression: &TypedExpr,
) -> Option<Candidate> {
    match &expression.kind {
        TypedExprKind::Parallel(builtin, arguments) => {
            let callback = match builtin {
                Builtin::ParallelInit | Builtin::ParallelReduce | Builtin::ParallelForEachChunk => {
                    arguments.get(1)?
                }
                Builtin::ParallelMap | Builtin::ParallelMapRef => arguments.first()?,
                _ => return None,
            };
            let call = builtin.name();
            let chunk = *builtin == Builtin::ParallelForEachChunk;
            let others = arguments
                .iter()
                .filter(|argument| !std::ptr::eq(*argument, callback));
            let purity = purity(module, callback, others, &expression.ty, chunk);
            Some((
                EntryKind::Site,
                call,
                expression.span,
                Some(site(call, function, expression.span, purity)),
            ))
        }
        TypedExprKind::Call(callee, arguments) => {
            let TypedExprKind::Function(FunctionRef::User(id)) = callee.kind else {
                return None;
            };
            let called = &module.functions[id];
            if called.origin.module != ModuleOrigin::Std
                || arguments.len() != called.signature.parameters.len()
            {
                return None;
            }
            let qualified = called.qualified_name();
            let name = qualified.split(".$mono.").next().unwrap_or(&qualified);
            if let Some(call) = STD_SITES.iter().copied().find(|call| *call == name) {
                let purity = match reducer(module, id) {
                    Some(callback) => {
                        match purity(module, callback, arguments.iter(), &expression.ty, false) {
                            // The rest of the std body runs too: `Parallel.fold` evaluates `Monoid.empty ()`
                            // before `Parallel.reduce`, outside the reducer.
                            Purity::Pure => effect_purity(pure_rules::reachable_effect(module, id)),
                            impure => impure,
                        }
                    }
                    None => Purity::Impure(unknown_call()),
                };
                return Some((
                    EntryKind::Site,
                    call,
                    expression.span,
                    Some(site(call, function, expression.span, purity)),
                ));
            }
            // The name as written: in edition 2027 `Array.map` calls `Array.mapr` with a read callback.
            let (call, replacement, why, span) =
                REPLACEMENTS.iter().find_map(|&(call, replacement, why)| {
                    let resolved = call == name || (call == "Array.map" && name == "Array.mapr");
                    resolved
                        .then(|| name_span(sources, expression, callee, call))
                        .flatten()
                        .map(|span| (call, replacement, why, span))
                })?;
            let entry =
                suggestion(module, expression, arguments, id, call).map(|parallel_safe| Entry {
                    kind: EntryKind::Suggestion,
                    call,
                    replacement: Some(replacement),
                    function,
                    span,
                    parallel_safe,
                    reason: None,
                    why: Some(why),
                });
            Some((EntryKind::Suggestion, call, span, entry))
        }
        _ => None,
    }
}

fn site(call: &'static str, function: usize, span: Span, purity: Purity) -> Entry {
    let (parallel_safe, reason) = match purity {
        Purity::Pure => (true, None),
        Purity::Impure(reason) => (false, Some(reason)),
    };
    Entry {
        kind: EntryKind::Site,
        call,
        replacement: None,
        function,
        span,
        parallel_safe,
        reason,
        why: None,
    }
}

/// The span of the name `call` at the start of the call expression (or its callee), when the source has it there as a
/// whole name. An alias, a pipeline or parentheses put something else there, and nothing is suggested.
fn name_span(
    sources: &dyn Sources,
    expression: &TypedExpr,
    callee: &TypedExpr,
    call: &str,
) -> Option<Span> {
    [expression.span, callee.span].into_iter().find_map(|span| {
        let text = sources.text(span);
        let end = span.start + call.len();
        let whole = text
            .as_bytes()
            .get(end)
            .is_none_or(|byte| !(byte.is_ascii_alphanumeric() || *byte == b'_'));
        (text.get(span.start..end) == Some(call) && whole).then_some(Span { end, ..span })
    })
}

/// The callback itself: without borrows and the adapters of read callbacks.
fn unwrapped<'a>(module: &CheckedModule, callback: &'a TypedExpr) -> &'a TypedExpr {
    let mut callee = callback;
    loop {
        if let TypedExprKind::Borrow(inner, _) | TypedExprKind::BorrowOperand(inner) = &callee.kind
        {
            callee = inner;
        } else if let Some(inner) = adapted(module, callee) {
            callee = inner;
        } else {
            return callee;
        }
    }
}

/// The callback that `Checker::callback_adapter` wrapped: an edition-2027 read callback made from a function that
/// takes its argument by value is `{ let f = callback; closure capturing f }`, whose body only calls `f`. The adapter
/// adds no effect, so the callback is judged instead; `None` for any other expression.
fn adapted<'a>(module: &CheckedModule, callback: &'a TypedExpr) -> Option<&'a TypedExpr> {
    let TypedExprKind::Block { bindings, result } = &callback.kind else {
        return None;
    };
    let [(local, value)] = bindings.as_slice() else {
        return None;
    };
    let TypedExprKind::Closure(id, captures) = &result.kind else {
        return None;
    };
    let [capture] = captures.as_slice() else {
        return None;
    };
    // Only the compiler names a local `$convention.N`; a block that a program writes has the same shape but may
    // compute an effect before it calls the function it binds.
    if local.provenance != Provenance::Generated
        || !local.name.starts_with("$convention.")
        || !matches!(capture.kind, TypedExprKind::Local(captured) if captured == local.id)
    {
        return None;
    }
    let adapter = &module.functions[*id];
    let (captured, parameters) = adapter.parameters.split_first()?;
    let TypedExprKind::Call(called, arguments) = &adapter.body.kind else {
        return None;
    };
    // The adapter passes its own parameters on, borrowed or read through: nothing else is evaluated.
    let forwarded = arguments.iter().all(|argument| {
        let mut argument = argument;
        while let TypedExprKind::Borrow(inner, _)
        | TypedExprKind::BorrowOperand(inner)
        | TypedExprKind::Dereference(inner)
        | TypedExprKind::Cast(inner) = &argument.kind
        {
            argument = inner;
        }
        matches!(argument.kind, TypedExprKind::Local(local)
            if parameters.iter().any(|parameter| parameter.id == local))
    });
    (adapter.capture_count == 1
        && forwarded
        && matches!(called.kind, TypedExprKind::Local(local) if local == captured.id))
    .then_some(value)
}

/// The reducer that the body of `Parallel.sum` or `Parallel.fold` passes to `Parallel.reduce`.
fn reducer(module: &CheckedModule, id: usize) -> Option<&TypedExpr> {
    let mut pending = vec![&module.functions[id].body];
    while let Some(expression) = pending.pop() {
        if let TypedExprKind::Parallel(Builtin::ParallelReduce, arguments) = &expression.kind {
            return arguments.get(1);
        }
        pending.extend(expression.children());
    }
    None
}

/// Whether a suggestion's conditions hold for the call `expression` of the std function `id` (`call`), and its
/// `parallel_safe`; `None` when it is not suggested.
fn suggestion(
    module: &CheckedModule,
    expression: &TypedExpr,
    arguments: &[TypedExpr],
    id: usize,
    call: &str,
) -> Option<bool> {
    let types = module.types();
    let input = arguments.last()?;
    let mut input_ty = &input.ty;
    if let Type::Reference(inner, _) = input_ty {
        input_ty = inner;
    }
    let Type::Array(element) = input_ty else {
        return None;
    };
    let result = match call {
        "Array.map" | "Array.mapr" => match &expression.ty {
            Type::Array(result) => result.as_ref(),
            _ => return None,
        },
        _ => &expression.ty,
    };
    // `Parallel` rejects function values that it cannot prove own their environments (E1013).
    if holds_function(element, &types) || holds_function(result, &types) {
        return None;
    }
    let typed = match call {
        "Array.map" => {
            element.is_copy(&types) && element.can_send(&types) && result.can_send(&types)
        }
        "Array.mapr" => element.can_send(&types) && result.can_send(&types),
        "Array.fold_monoid" => result.can_send(&types),
        "Array.sum" => matches!(element.as_ref(), Type::Integer(..)),
        _ => false,
    };
    if !typed || short(input) {
        return None;
    }
    let purity = match call {
        "Array.map" | "Array.mapr" => {
            let callback = arguments.first()?;
            if let TypedExprKind::Closure(_, captures) = &unwrapped(module, callback).kind
                && captures.iter().any(|capture| {
                    types.reaches(&capture.ty, |ty| matches!(ty, Type::Reference(..)))
                        || capture.ty.holds_cell(&types)
                })
            {
                return None;
            }
            purity(
                module,
                callback,
                std::iter::once(input),
                &expression.ty,
                false,
            )
        }
        "Array.fold_monoid" => effect_purity(pure_rules::reachable_effect(module, id)),
        _ => Purity::Pure,
    };
    // A suggestion is only made where the replacement keeps the result: its callback must be pure.
    matches!(purity, Purity::Pure).then_some(true)
}

/// Whether the length of `input` is known to be below `SMALL_INPUT`: an array literal, or `new [T](n, ...)` with a
/// literal `n`, borrowed in place. Locals are not followed.
fn short(input: &TypedExpr) -> bool {
    let mut value = input;
    while let TypedExprKind::Borrow(inner, _) | TypedExprKind::BorrowOperand(inner) = &value.kind {
        value = inner;
    }
    if let TypedExprKind::NewLiteral(inner) = &value.kind {
        value = inner;
    }
    match &value.kind {
        TypedExprKind::Array(elements) => (elements.len() as u64) < SMALL_INPUT,
        TypedExprKind::NewArray(length, _) => {
            let mut length = length.as_ref();
            while let TypedExprKind::Cast(inner) = &length.kind {
                length = inner;
            }
            matches!(length.kind, TypedExprKind::Int(n) | TypedExprKind::GenericInteger(n, _) if n < SMALL_INPUT as u128)
        }
        _ => false,
    }
}

fn holds_function(ty: &Type, types: &TypeContext<'_>) -> bool {
    types.reaches(ty, |ty| matches!(ty, Type::Function(..)))
}

/// Whether `callback` is pure (§6 of F15): a function value that comes from outside (an argument, a capture or the
/// result that holds one, or a local) is not, and a known function or closure is pure when A19's rules find no impure
/// source that it reaches. `chunk` lends the last parameter of a `for_each_chunk` body, its exclusive chunk.
fn purity<'a>(
    module: &CheckedModule,
    callback: &TypedExpr,
    others: impl Iterator<Item = &'a TypedExpr>,
    result: &Type,
    chunk: bool,
) -> Purity {
    let types = module.types();
    let mut outside = others
        .map(|argument| &argument.ty)
        .chain(std::iter::once(result));
    if outside.any(|ty| holds_function(ty, &types)) {
        return Purity::Impure(unknown_call());
    }
    match &unwrapped(module, callback).kind {
        TypedExprKind::Closure(id, captures) => {
            if captures
                .iter()
                .any(|capture| holds_function(&capture.ty, &types))
            {
                return Purity::Impure(unknown_call());
            }
            staged_purity(module, *id, chunk)
        }
        TypedExprKind::Function(FunctionRef::User(id)) => staged_purity(module, *id, chunk),
        TypedExprKind::Function(FunctionRef::Builtin(instance)) => {
            if pure_rules::builtin_impure(instance.builtin) {
                let dynamic = builtin_facts(instance.builtin).dynamic;
                Purity::Impure(builtin_reason(instance.builtin, dynamic))
            } else {
                Purity::Pure
            }
        }
        _ => Purity::Impure(unknown_call()),
    }
}

/// The purity of the function `id` as a callback. A function that returns a function value is applied in stages, and
/// the later stages run code that the first one only builds as a closure (`\left -> \right -> Debug.trace right`):
/// A19's reachability does not follow returned closures, so such a root is not proven pure.
fn staged_purity(module: &CheckedModule, id: usize, chunk: bool) -> Purity {
    if holds_function(&module.functions[id].signature.result, &module.types()) {
        return Purity::Impure(reason(&Effect::Unresolved));
    }
    effect_purity(reachable(module, id, chunk))
}

fn reachable(
    module: &CheckedModule,
    id: usize,
    chunk: bool,
) -> Result<Option<Effect>, LimitExceeded> {
    if chunk {
        pure_rules::reachable_effect_of_chunk_body(module, id)
    } else {
        pure_rules::reachable_effect(module, id)
    }
}

fn effect_purity(result: Result<Option<Effect>, LimitExceeded>) -> Purity {
    match result {
        Ok(None) => Purity::Pure,
        Ok(Some(effect)) => Purity::Impure(reason(&effect)),
        Err(LimitExceeded) => {
            Purity::Impure("the analysis limit was reached (effect: analysis_limit)".into())
        }
    }
}

fn unknown_call() -> String {
    "the callback is or captures a function value that is not known (effect: unknown_call)".into()
}

/// The closed set of reasons of a site that is not `parallel_safe` (appendix L7 of F15, with A19's `Io`).
fn reason(effect: &Effect) -> String {
    match effect {
        Effect::Host(name) => {
            format!("the callback calls the extern function '{name}' (effect: extern)")
        }
        Effect::Builtin(builtin, _) => builtin_reason(*builtin, builtin_facts(*builtin).dynamic),
        Effect::Concurrency => {
            "the callback starts a task or a parallel operation (effect: task)".into()
        }
        Effect::Dyn => "the callback uses dyn dispatch (effect: dyn)".into(),
        Effect::UserDrop(ty) => format!(
            "the callback holds a value whose type '{ty}' has a user Drop (effect: user_drop)"
        ),
        Effect::Io => "the callback may execute an I/O action (effect: io)".into(),
        Effect::Unresolved => "the callback cannot be proven pure (effect: unresolved)".into(),
    }
}

/// The reason of an impure builtin: its name and the D-53 name of its effect (A20's classification of builtins),
/// or `dyn` for dynamic dispatch.
fn builtin_reason(builtin: Builtin, dynamic: bool) -> String {
    let effect = match builtin_facts(builtin).effect {
        _ if dynamic => "dyn",
        Some(effect) => effect.name(),
        None => "unresolved",
    };
    format!(
        "the callback reaches '{}' (effect: {effect})",
        builtin.name()
    )
}

fn counts(report: &Report) -> (usize, usize) {
    let suggestions = report
        .entries
        .iter()
        .filter(|entry| entry.kind == EntryKind::Suggestion)
        .count();
    (suggestions, report.entries.len() - suggestions)
}

/// The report as text: a summary line, then one line per entry (a suggestion has its fix on the next line).
pub fn render_text(report: &Report, sources: &dyn Sources) -> String {
    let (suggestions, sites) = counts(report);
    let mut output = format!("parallel report: {suggestions} suggestions, {sites} sites\n");
    for entry in &report.entries {
        let (line, column) = location(sources.text(entry.span), entry.span.start);
        let path = sources.path(entry.span);
        match entry.kind {
            EntryKind::Suggestion => {
                let replacement = entry.replacement.unwrap_or_default();
                let _ = writeln!(
                    output,
                    "{path}:{line}:{column}: suggestion: {} can be {replacement}: {}",
                    entry.call,
                    entry.why.unwrap_or_default()
                );
                let _ = writeln!(
                    output,
                    "  fix (maybe): replace '{}' with '{replacement}'",
                    entry.call
                );
            }
            EntryKind::Site if entry.parallel_safe => {
                let _ = writeln!(
                    output,
                    "{path}:{line}:{column}: site: {} callback is pure (parallel_safe: true)",
                    entry.call
                );
            }
            EntryKind::Site => {
                let _ = writeln!(
                    output,
                    "{path}:{line}:{column}: site: {} callback is not pure (parallel_safe: false): {}",
                    entry.call,
                    entry.reason.as_deref().unwrap_or_default()
                );
            }
        }
    }
    output
}

/// The report as JSON Lines (`tz-report-parallel/1`): the summary, then one object per entry, with keys in a fixed
/// order and only the ones that have a value.
pub fn render_json(report: &Report, sources: &dyn Sources) -> String {
    let (suggestions, sites) = counts(report);
    let mut output = format!(
        "{{\"schema\":\"tz-report-parallel/1\",\"suggestions\":{suggestions},\"sites\":{sites},\"truncated\":{}}}\n",
        report.truncated
    );
    for entry in &report.entries {
        let text = sources.text(entry.span);
        let path = json_string(&sources.path(entry.span));
        let (line, column) = location(text, entry.span.start);
        let (end_line, end_column) = location(text, entry.span.end);
        let span = format!(
            "{{\"start\":{},\"end\":{},\"line\":{line},\"column\":{column},\"end_line\":{end_line},\"end_column\":{end_column}}}",
            entry.span.start, entry.span.end
        );
        let kind = match entry.kind {
            EntryKind::Suggestion => "suggestion",
            EntryKind::Site => "site",
        };
        let _ = write!(
            output,
            "{{\"kind\":\"{kind}\",\"call\":{}",
            json_string(entry.call)
        );
        if let Some(replacement) = entry.replacement {
            let _ = write!(output, ",\"replacement\":{}", json_string(replacement));
        }
        let _ = write!(output, ",\"parallel_safe\":{}", entry.parallel_safe);
        if let Some(reason) = &entry.reason {
            let _ = write!(output, ",\"reason\":{}", json_string(reason));
        }
        let _ = write!(output, ",\"path\":{path},\"span\":{span}");
        if let Some(replacement) = entry.replacement {
            let title = format!("replace '{}' with '{replacement}'", entry.call);
            let _ = write!(
                output,
                ",\"fixes\":[{{\"title\":{},\"applicability\":\"maybe\",\"edits\":[{{\"path\":{path},\"span\":{span},\"text\":{}}}]}}]",
                json_string(&title),
                json_string(replacement)
            );
        }
        output.push_str("}\n");
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    struct One(&'static str);

    impl Sources for One {
        fn text(&self, _span: Span) -> &str {
            self.0
        }
        fn path(&self, _span: Span) -> String {
            "./Main.tz".into()
        }
    }

    fn report(source: &'static str) -> Report {
        let module = crate::analyze(source).unwrap_or_else(|error| panic!("{}", error.message));
        build(&module, &One(source))
    }

    /// The byte range of `text` in `source`, found from the source so that the expectations do not depend on its
    /// line endings.
    fn range(source: &str, text: &str, length: usize) -> (usize, usize) {
        let start = source.find(text).unwrap_or_else(|| panic!("{text}"));
        (start, start + length)
    }

    #[test]
    fn builds_the_basic_report() {
        let source = include_str!("../tests/fixtures/parallel_report/basic/Main.tz");
        let at = |text: &str, length: usize| range(source, text, length);
        let ((map, map_end), (sum, sum_end)) = (at("Array.map square", 9), at("Array.sum", 9));
        let doubled = "Parallel.map (\\value -> value * 2) values";
        let shown = "Parallel.map (\\value -> Debug.trace value) values";
        let ((pure, pure_end), (traced, traced_end)) =
            (at(doubled, doubled.len()), at(shown, shown.len()));
        let entries: Vec<_> = report(source)
            .entries
            .into_iter()
            .map(|entry| {
                (
                    entry.kind,
                    entry.call,
                    entry.span.start,
                    entry.span.end,
                    entry.parallel_safe,
                )
            })
            .collect();
        assert_eq!(
            entries,
            [
                (EntryKind::Suggestion, "Array.map", map, map_end, true),
                (EntryKind::Suggestion, "Array.sum", sum, sum_end, true),
                (EntryKind::Site, "Parallel.map", pure, pure_end, true),
                (EntryKind::Site, "Parallel.map", traced, traced_end, false),
            ]
        );
        // The offsets of the specification (§7.5 of F15) for the file as checked out with LF.
        if !source.contains('\r') {
            assert_eq!((map, sum, pure, traced), (134, 162, 341, 434));
        }
    }

    #[test]
    fn builds_the_rules_report() {
        let source = include_str!("../tests/fixtures/parallel_report/rules/Main.tz");
        let entries: Vec<_> = report(source)
            .entries
            .into_iter()
            .map(|entry| (entry.kind, entry.call, location(source, entry.span.start).0))
            .collect();
        assert_eq!(
            entries,
            [
                (EntryKind::Suggestion, "Array.sum", 2),
                (EntryKind::Suggestion, "Array.map", 14),
                (EntryKind::Suggestion, "Array.mapr", 17),
                (EntryKind::Suggestion, "Array.fold_monoid", 20),
                (EntryKind::Site, "Parallel.sum", 26),
                (EntryKind::Site, "Parallel.fold", 29),
                (EntryKind::Site, "Parallel.init", 32),
            ]
        );
    }

    /// `(kind, call, parallel_safe, reason)` of every entry of the report of `source`.
    fn summary(source: &'static str) -> Vec<(EntryKind, &'static str, bool, Option<String>)> {
        report(source)
            .entries
            .into_iter()
            .map(|entry| (entry.kind, entry.call, entry.parallel_safe, entry.reason))
            .collect()
    }

    const PRINT: &str = "the callback reaches 'Debug.__print_string' (effect: debug)";

    /// PR #75 review: `Parallel.fold` evaluates `Monoid.empty ()` too, outside the reducer; an instance whose `empty`
    /// prints is not `parallel_safe`, and `Array.fold_monoid` over it is not suggested.
    #[test]
    fn an_effectful_empty_makes_the_fold_impure() {
        let entries = summary(
            "record Noisy { value: i64 }\n\
             instance Monoid<Noisy> {\n\
             \x20   let empty = \\() -> { let zero = 0; Debug.print (ref zero); Noisy { value: zero } }\n\
             \x20   fn combine left right = Noisy { value: left.value + right.value }\n\
             }\n\
             def folded :: ref [Noisy] -> Noisy\n\
             fn folded values = Parallel.fold values\n\
             def sequential :: ref [Noisy] -> Noisy\n\
             fn sequential values = Array.fold_monoid values\n",
        );
        assert_eq!(
            entries,
            [(
                EntryKind::Site,
                "Parallel.fold",
                false,
                Some(PRINT.to_owned())
            )]
        );
    }

    /// PR #75 review: a block that a program writes, `({ let f = square; \index -> f (Debug.trace index) })`, has the
    /// shape of the compiler's callback adapter but evaluates an effect before it calls `f`; it is not unwrapped.
    #[test]
    fn only_compiler_adapters_are_unwrapped() {
        let entries = summary(
            "def square :: i64 -> i64\n\
             fn square value = value * value\n\
             def wrapped :: ref [i64] -> [i64]\n\
             fn wrapped values = Array.map ({ let f = square; \\index -> f (Debug.trace index) }) values\n\
             def site :: ref [i64] -> [i64]\n\
             fn site values = Parallel.map ({ let f = square; \\index -> f (Debug.trace index) }) values\n\
             def plain :: ref [i64] -> [i64]\n\
             fn plain values = Array.map square values\n",
        );
        assert_eq!(
            entries,
            [
                (EntryKind::Site, "Parallel.map", false, Some(unknown_call())),
                (EntryKind::Suggestion, "Array.map", true, None),
            ],
            "only `plain` is suggested, and the site is not pure"
        );
    }

    /// PR #75 review: a callback applied in stages runs its effect when a later argument is supplied; the closure
    /// that the first stage returns is not a call, so a root that returns a function is not proven pure.
    #[test]
    fn a_callback_that_returns_a_function_is_not_proven_pure() {
        let entries = summary(
            "def staged :: ref [i64] -> i64\n\
             fn staged values = Parallel.reduce 0 (\\left -> { let offset = left; \\right -> Debug.trace (offset + right) }) values\n",
        );
        assert_eq!(
            entries,
            [(
                EntryKind::Site,
                "Parallel.reduce",
                false,
                Some("the callback cannot be proven pure (effect: unresolved)".to_owned())
            )]
        );
    }

    #[test]
    fn every_impure_builtin_has_an_effect_name() {
        for &builtin in Builtin::ALL {
            if pure_rules::builtin_impure(builtin) && !builtin_facts(builtin).dynamic {
                assert!(
                    builtin_facts(builtin).effect.is_some(),
                    "{}",
                    builtin.name()
                );
            }
        }
    }

    #[test]
    fn reasons_name_the_effect() {
        assert_eq!(
            reason(&Effect::Builtin(
                Builtin::DebugPrintString,
                pure_rules::Category::Debug
            )),
            "the callback reaches 'Debug.__print_string' (effect: debug)"
        );
        assert_eq!(
            reason(&Effect::Builtin(Builtin::DynOf, pure_rules::Category::Dyn)),
            "the callback reaches 'Dyn.of' (effect: dyn)"
        );
        assert_eq!(
            reason(&Effect::Unresolved),
            "the callback cannot be proven pure (effect: unresolved)"
        );
    }
}
