//! Lowering of local `let rec` groups (B09).
//!
//! `closures::lower` turns each lambda into a function that takes its captures before its own
//! parameters. A `let rec` group is lifted to one such function per member, and the names of the
//! group become direct calls of the lifted functions, so that a call of a member inside a body
//! passes the captures on instead of copying a closure, and a tail call of the function itself is
//! a loop like the tail call of a `def rec`.
//!
//! The captures of the group are the union of its members' captures, in local order, so every
//! member takes the same ones. A capture that copies without allocating and without a drop
//! (a number, a `ref`, ...) is passed by value. Any other capture (a string, an array, a record
//! that holds one, a function value, ...) is borrowed: the closure holds a shared reference to the
//! local, and the lifted function takes `ref T` and reads the local through it. This is what
//! `let r = ref x` and `deref r` would have written, so a recursive call passes the reference
//! on, and nothing is copied or moved.
//!
//! A name of an earlier `let rec` function, or of an enclosing one, that a function mentions is
//! not captured as a function value either: its captures are added to the captures of the
//! function and its uses become direct calls too.

use super::*;

/// One capture of a lifted `let rec` function.
#[derive(Clone)]
pub(super) struct Captured {
    /// The local of the function that binds the group.
    local: Local,
    /// Whether the lifted function takes a reference to the local rather than its value.
    borrowed: bool,
}

impl Captured {
    fn new(local: Local, types: &TypeContext<'_>) -> Self {
        let borrowed = !(local.ty.is_copy(types) && !local.ty.needs_drop(types));
        Self { local, borrowed }
    }

    /// The parameter that the lifted function declares for the capture.
    fn parameter(&self) -> Local {
        let mut local = self.local.clone();
        if self.borrowed {
            local.ty = Type::Reference(Box::new(local.ty), false);
        }
        local
    }

    /// What a closure's environment takes where the group is bound: the value, or a borrow of the
    /// local.
    fn at_binding(&self, span: Span) -> TypedExpr {
        let value = TypedExpr {
            kind: TypedExprKind::Local(self.local.id),
            ty: self.local.ty.clone(),
            span,
        };
        if !self.borrowed {
            return value;
        }
        TypedExpr {
            ty: Type::Reference(Box::new(self.local.ty.clone()), false),
            kind: TypedExprKind::Borrow(Box::new(value), false),
            span,
        }
    }

    /// What a lifted function passes on to another one: its own parameter.
    fn in_body(&self, span: Span) -> TypedExpr {
        let parameter = self.parameter();
        TypedExpr {
            kind: TypedExprKind::Local(parameter.id),
            ty: parameter.ty,
            span,
        }
    }
}

/// The lifted function of a `let rec` function and what it captures.
pub(super) struct RecursiveFunction {
    function: usize,
    captures: Vec<Captured>,
}

/// The number of bindings of the group that starts at `bindings[0]`, or 0 when it is no `let rec`.
fn group_len(bindings: &[(Local, TypedExpr)]) -> Result<usize, Diagnostic> {
    let TypedExprKind::Lambda { recursive, .. } = &bindings[0].1.kind else {
        return Ok(0);
    };
    if recursive.is_empty() {
        return Ok(0);
    }
    let members = bindings.get(..recursive.len()).filter(|members| {
        members.iter().zip(recursive).all(|((local, value), member)| {
            local.id == member.id
                && matches!(&value.kind, TypedExprKind::Lambda { recursive: other, .. }
                    if other.iter().map(|local| local.id).eq(recursive.iter().map(|local| local.id)))
        })
    });
    match members {
        Some(members) => Ok(members.len()),
        None => Err(Diagnostic::new(
            "E1015",
            "internal compiler error: the members of a 'let rec' group are not bound together",
            bindings[0].1.span,
        )),
    }
}

/// Whether `expression` is a lambda of a `let rec` group.
fn is_recursive(expression: &TypedExpr) -> bool {
    matches!(&expression.kind, TypedExprKind::Lambda { recursive, .. } if !recursive.is_empty())
}

/// The locals of the enclosing scopes that a lambda of the group `recursive` can capture: not the
/// functions of the group, whose names are in scope without being captured.
pub(super) fn leave_out(outer: &mut BTreeMap<usize, Local>, recursive: &[Local]) {
    for local in recursive {
        outer.remove(&local.id);
    }
}

/// Whether lowering `expression` is up to this module: a block that binds a `let rec` group, or
/// a lambda of one that no block binds, which the checker never makes.
pub(super) fn involves(expression: &TypedExpr) -> bool {
    match &expression.kind {
        TypedExprKind::Block { bindings, .. } => {
            bindings.iter().any(|(_, value)| is_recursive(value))
        }
        _ => is_recursive(expression),
    }
}

/// Lowers an expression for which `involves` holds.
pub(super) fn lower(
    expression: &mut TypedExpr,
    functions: &mut Vec<CheckedFunction>,
    types: &TypeContext<'_>,
    generated: &mut Generated,
    origin: FunctionOrigin,
) -> Result<(), Diagnostic> {
    let span = expression.span;
    match &mut expression.kind {
        TypedExprKind::Block { bindings, result } => {
            lower_block(bindings, result, functions, types, generated, origin)
        }
        _ => Err(Diagnostic::new(
            "E1015",
            "internal compiler error: a 'let rec' function is not bound by a block",
            span,
        )),
    }
}

/// Lowers the bindings and the result of a block that binds `let rec` groups. The groups are
/// lifted where they are bound; everything else lowers as it always did.
pub(super) fn lower_block(
    bindings: &mut [(Local, TypedExpr)],
    result: &mut TypedExpr,
    functions: &mut Vec<CheckedFunction>,
    types: &TypeContext<'_>,
    generated: &mut Generated,
    origin: FunctionOrigin,
) -> Result<(), Diagnostic> {
    let mut index = 0;
    while index < bindings.len() {
        let count = group_len(&bindings[index..])?;
        if count == 0 {
            lower_expression(&mut bindings[index].1, functions, types, generated, origin)?;
            index += 1;
        } else {
            lower_group(
                &mut bindings[index..index + count],
                functions,
                types,
                generated,
                origin,
            )?;
            index += count;
        }
    }
    lower_expression(result, functions, types, generated, origin)
}

fn lower_group(
    members: &mut [(Local, TypedExpr)],
    functions: &mut Vec<CheckedFunction>,
    types: &TypeContext<'_>,
    generated: &mut Generated,
    origin: FunctionOrigin,
) -> Result<(), Diagnostic> {
    let group: BTreeSet<usize> = members.iter().map(|(local, _)| local.id).collect();
    // The captures of the group: each member's own, and the captures of every earlier `let rec`
    // function that a member mentions, since the member now calls it directly.
    let mut shared: BTreeMap<usize, Local> = BTreeMap::new();
    for (_, value) in members.iter() {
        let TypedExprKind::Lambda { captures, .. } = &value.kind else {
            continue;
        };
        for capture in captures {
            match generated.recursive.get(&capture.id) {
                Some(function) => {
                    for captured in &function.captures {
                        shared
                            .entry(captured.local.id)
                            .or_insert_with(|| captured.local.clone());
                    }
                }
                None if !group.contains(&capture.id) => {
                    shared.entry(capture.id).or_insert_with(|| capture.clone());
                }
                None => {}
            }
        }
    }
    let captured: Vec<Captured> = shared
        .into_values()
        .map(|local| Captured::new(local, types))
        .collect();
    // The ids are reserved before any body is lowered: a function nested in a body calls the
    // lifted functions of the group, which have not been built yet.
    let first = functions.len();
    for (position, (_, value)) in members.iter().enumerate() {
        let TypedExprKind::Lambda {
            parameters, body, ..
        } = &value.kind
        else {
            continue;
        };
        let mut all_parameters: Vec<Local> = captured.iter().map(Captured::parameter).collect();
        all_parameters.extend(parameters.iter().cloned());
        let id = first + position;
        functions.push(CheckedFunction {
            region_sources: None,
            callback_contracts: Vec::new(),
            region_slots: None,
            module: "$lambda".into(),
            origin,
            name: id.to_string(),
            visibility: Visibility::Public,
            exported: false,
            signature: Signature {
                parameters: all_parameters
                    .iter()
                    .map(|local| local.ty.clone())
                    .collect(),
                result: body.ty.clone(),
            },
            parameters: all_parameters,
            body: TypedExpr::error(value.span),
            span: value.span,
            type_parameters: Vec::new(),
            constraints: Vec::new(),
            members: Vec::new(),
            capture_count: captured.len(),
            is_task: false,
            owned_captures: false,
        });
        generated.recursive.insert(
            members[position].0.id,
            RecursiveFunction {
                function: id,
                captures: captured.clone(),
            },
        );
    }
    let borrowed: BTreeMap<usize, Type> = captured
        .iter()
        .filter(|capture| capture.borrowed)
        .map(|capture| (capture.local.id, capture.local.ty.clone()))
        .collect();
    for position in 0..members.len() {
        let value = &mut members[position].1;
        let span = value.span;
        let TypedExprKind::Lambda { body, .. } = &mut value.kind else {
            continue;
        };
        let mut body = std::mem::replace(&mut **body, TypedExpr::error(span));
        lower_expression(&mut body, functions, types, generated, origin)?;
        for capture in captured.iter().filter(|capture| capture.borrowed) {
            if capture.local.ty.is_copy(types) {
                continue;
            }
            if let Some(span) = body.consuming_use(capture.local.id, types) {
                return Err(Diagnostic::new(
                    "E1012",
                    format!(
                        "cannot move '{}' out of a 'let rec' function, which borrows it; borrow it with 'ref' instead",
                        capture.local.name
                    ),
                    span,
                ));
            }
        }
        rewrite(&mut body, &generated.recursive, &borrowed, functions);
        functions[first + position].body = body;
    }
    // The group's names now stand for closures of the lifted functions.
    for (position, (_, value)) in members.iter_mut().enumerate() {
        let span = value.span;
        value.kind = TypedExprKind::Closure(
            first + position,
            captured
                .iter()
                .map(|capture| capture.at_binding(span))
                .collect(),
        );
    }
    Ok(())
}

/// The function that `callee` names when it is a `let rec` function.
fn target<'a>(
    callee: &TypedExpr,
    recursive: &'a BTreeMap<usize, RecursiveFunction>,
) -> Option<&'a RecursiveFunction> {
    match callee.kind {
        TypedExprKind::Local(id) => recursive.get(&id),
        _ => None,
    }
}

/// The call of the lifted function of `function` with its captures and then `arguments`.
fn direct_call(
    function: &RecursiveFunction,
    arguments: Vec<TypedExpr>,
    span: Span,
    functions: &[CheckedFunction],
) -> TypedExprKind {
    let callee = TypedExpr {
        kind: TypedExprKind::Function(FunctionRef::User(function.function)),
        ty: functions[function.function].signature.as_type(),
        span,
    };
    let mut all: Vec<TypedExpr> = function
        .captures
        .iter()
        .map(|capture| capture.in_body(span))
        .collect();
    all.extend(arguments);
    TypedExprKind::Call(Box::new(callee), all)
}

/// Rewrites a lifted body: the names of `let rec` functions become direct calls, or closures
/// where they are used as values, and the locals that the function borrows are read through
/// their references. An explicit stack walks the body, which can be deep.
fn rewrite(
    body: &mut TypedExpr,
    recursive: &BTreeMap<usize, RecursiveFunction>,
    borrowed: &BTreeMap<usize, Type>,
    functions: &[CheckedFunction],
) {
    let mut pending = vec![body];
    while let Some(expression) = pending.pop() {
        let span = expression.span;
        let replacement = match &mut expression.kind {
            TypedExprKind::Binary(BinaryOp::Pipe, argument, callee) => target(callee, recursive)
                .map(|function| {
                    let argument = std::mem::replace(&mut **argument, TypedExpr::error(span));
                    direct_call(function, vec![argument], span, functions)
                }),
            TypedExprKind::Call(callee, arguments) => target(callee, recursive).map(|function| {
                let arguments = std::mem::take(arguments);
                direct_call(function, arguments, span, functions)
            }),
            TypedExprKind::Local(id) => match recursive.get(id) {
                Some(function) => Some(TypedExprKind::Closure(
                    function.function,
                    function
                        .captures
                        .iter()
                        .map(|capture| capture.in_body(span))
                        .collect(),
                )),
                None => borrowed
                    .get(id)
                    .filter(|ty| **ty == expression.ty)
                    .map(|ty| {
                        TypedExprKind::Dereference(Box::new(TypedExpr {
                            kind: TypedExprKind::Local(*id),
                            ty: Type::Reference(Box::new(ty.clone()), false),
                            span,
                        }))
                    }),
            },
            _ => None,
        };
        if let Some(kind) = replacement {
            expression.kind = kind;
        }
        pending.extend(expression.children_mut());
    }
}
