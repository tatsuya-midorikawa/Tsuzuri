//! What a `const def` may take, return and contain (D11).
//!
//! Every `Type` and every `TypedExprKind` is named in the matches below, without a wildcard, so a
//! new variant has to be judged here and in the evaluator (`const_machine.rs`).

use super::*;

/// Whether the compile-time evaluator builds values of `ty`: plain data held in a literal tree.
/// A record or union with a user `Drop` instance is out, because the evaluator never runs drops.
pub(super) fn value_type(ty: &Type, types: &TypeContext<'_>) -> bool {
    types.stored_all(ty, |ty| value_part(ty, types))
}

fn value_part(ty: &Type, types: &TypeContext<'_>) -> bool {
    match ty {
        Type::Integer(..)
        | Type::Binary(_)
        | Type::Decimal(_)
        | Type::Bool
        | Type::Unit
        | Type::Char
        | Type::Utf8Char
        | Type::String
        | Type::Utf8String
        | Type::Array(_)
        | Type::Tuple(_) => true,
        Type::FixedArray(_, length) => matches!(**length, Type::Length(_)),
        Type::Record(id, _) => !types.records[*id].user_drop && !types.records[*id].opaque(),
        Type::Union(id, _) => !types.unions[*id].user_drop,
        // A vector is built by instructions, not written as a literal.
        Type::Simd(_)
        // Collections of other shapes, handles, and values with behavior have no literal form.
        | Type::List(_)
        | Type::Vec(_)
        | Type::Task(_)
        | Type::Handle(_)
        | Type::Function(..)
        | Type::Reference(..)
        | Type::Dyn(_)
        | Type::Shared(..)
        // Never the type of a value, or not settled in a concrete signature.
        | Type::ArrayView(_)
        | Type::Length(_)
        | Type::Error
        | Type::Variable(_)
        | Type::Infer(_)
        | Type::Partial(_)
        | Type::Application(..) => false,
    }
}

/// A `const def` has a concrete signature of plain values.
pub(in super::super) fn check_signature(
    function: &FunctionDecl,
    signature: &Signature,
    variables: &[String],
    types: &TypeContext<'_>,
) -> Result<(), Diagnostic> {
    if !variables.is_empty() || !function.constraints.is_empty() {
        return Err(failure(
            DiagnosticKind::ConstSignatureInvalid,
            function.name.span,
            "a const def must have a concrete signature; remove type variables and constraints",
        ));
    }
    if let Some(ty) = signature
        .parameters
        .iter()
        .chain([&signature.result])
        .find(|ty| !value_type(ty, types))
    {
        return Err(failure(
            DiagnosticKind::ConstSignatureInvalid,
            function.name.span,
            format!(
                "const def parameters and results must be plain values; '{}' is not one, so remove functions, references, tasks, lists, and std collections",
                ty.display(types)
            ),
        ));
    }
    Ok(())
}

/// The part of an expression that decides how its children are judged.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Position {
    /// A value that the evaluator computes.
    Value,
    /// The base of a field, element, length, or tag read: a borrow of a place is read through.
    Place,
}

fn rejected(span: Span) -> Diagnostic {
    failure(
        DiagnosticKind::ConstOperationNotAllowed,
        span,
        "this operation is not allowed in a const def; move it to a runtime def",
    )
}

fn not_const_def(span: Span, name: &str) -> Diagnostic {
    failure(
        DiagnosticKind::ConstCalleeNotConst,
        span,
        format!("'{name}' is not a const def; mark it 'const def' or compute the value at runtime"),
    )
}

fn partial(span: Span) -> Diagnostic {
    failure(
        DiagnosticKind::ConstPartialApplication,
        span,
        "a const def must be fully applied in const evaluation; pass all arguments",
    )
}

fn operator_on_aggregate(span: Span) -> Diagnostic {
    failure(
        DiagnosticKind::ConstAggregateOperator,
        span,
        "an operator on a record, tuple, array, or union is not allowed in a const def; apply it to their fields",
    )
}

/// Checks the body of the `const def` `id` before anything is evaluated, so that a function that
/// no constant calls is judged too. The first rejected expression in source order is reported.
pub(super) fn validate_function(
    id: usize,
    functions: &[CheckedFunction],
    constants: &BTreeSet<usize>,
    const_functions: &BTreeSet<usize>,
    types: &TypeContext<'_>,
) -> Result<(), Diagnostic> {
    use TypedExprKind as K;
    let mut cache: BTreeMap<&Type, bool> = BTreeMap::new();
    let mut pending = vec![(&functions[id].body, Position::Value)];
    while let Some((expression, position)) = pending.pop() {
        let span = expression.span;
        let reading = position == Position::Place;
        match &expression.kind {
            K::Int(_)
            | K::Float(_)
            | K::GenericInteger(..)
            | K::GenericFloat(_)
            | K::String(_)
            | K::Bool(_)
            | K::Unit
            | K::Local(_) => {}
            K::Break | K::Continue => {}
            K::NewLiteral(value)
            | K::Construct {
                payload: Some(value),
                ..
            } => pending.push((value, Position::Value)),
            K::Unary(_, value) => {
                if !matches!(
                    value.ty,
                    Type::Integer(..) | Type::Binary(_) | Type::Decimal(_) | Type::Bool
                ) {
                    return Err(operator_on_aggregate(span));
                }
                pending.push((value, Position::Value));
            }
            K::SaturatingIndex(value) => pending.push((value, Position::Value)),
            K::Cast(value) => {
                let numeric = |ty: &Type| matches!(ty, Type::Integer(..) | Type::Binary(_));
                if value.ty != expression.ty && !(numeric(&value.ty) && numeric(&expression.ty)) {
                    return Err(failure(
                        DiagnosticKind::ConstUnsupportedOperation,
                        span,
                        "this const cast is unsupported; keep the conversion in runtime code",
                    ));
                }
                pending.push((value, Position::Value));
            }
            K::Construct { payload: None, .. } => {}
            K::Field(value, _)
            | K::Length(value)
            | K::StringLength(value)
            | K::UnionTag(value)
            | K::UnionPayload { value, .. } => pending.push((value, Position::Place)),
            K::Borrow(value, false) | K::BorrowOperand(value) | K::Dereference(value) => {
                if !reading {
                    return Err(failure(
                        DiagnosticKind::ConstBorrowNotAllowed,
                        span,
                        "a borrow is not allowed in a const def; read the value directly",
                    ));
                }
                pending.push((value, Position::Place));
            }
            K::Index(value, index) => {
                pending.push((index, Position::Value));
                pending.push((value, Position::Place));
            }
            K::Binary(BinaryOp::Pipe, ..) => {
                return Err(failure(
                    DiagnosticKind::ConstPipeNotAllowed,
                    span,
                    "the pipe operator is not allowed in a const def; write the call",
                ));
            }
            K::Binary(_, left, right) => {
                match left.ty {
                    Type::Decimal(_) => {
                        return Err(failure(
                            DiagnosticKind::ConstUnsupportedOperation,
                            span,
                            "this const arithmetic is unsupported; keep decimal arithmetic and overloaded operations in runtime code",
                        ));
                    }
                    Type::Integer(..)
                    | Type::Binary(_)
                    | Type::Bool
                    | Type::Unit
                    | Type::Char
                    | Type::Utf8Char
                    | Type::String
                    | Type::Utf8String => {}
                    _ => return Err(operator_on_aggregate(span)),
                }
                pending.push((right, Position::Value));
                pending.push((left, Position::Value));
            }
            K::Block { .. }
            | K::Match { .. }
            | K::If { .. }
            | K::While { .. }
            | K::Record(_)
            | K::RecordUpdate { .. }
            | K::Tuple(_)
            | K::Array(_) => {
                for child in expression.children().into_iter().rev() {
                    pending.push((child, Position::Value));
                }
            }
            K::ForRange {
                start,
                step,
                finish,
                body,
                ..
            } => {
                for child in [body, finish, step, start] {
                    pending.push((child, Position::Value));
                }
            }
            K::ForEach { source, body, .. } => {
                if !matches!(source.ty, Type::Array(_) | Type::String | Type::Utf8String) {
                    return Err(failure(
                        DiagnosticKind::ConstForLoopForm,
                        source.span,
                        "a for loop in a const def enumerates an array, a string, or a utf8string, or counts a range; move other loops to a runtime def",
                    ));
                }
                pending.push((body, Position::Value));
                pending.push((source, Position::Value));
            }
            K::Assign(target, value) => {
                if !matches!(target.kind, K::Local(_)) {
                    return Err(failure(
                        DiagnosticKind::ConstAssignmentTarget,
                        target.span,
                        "a const def assigns only to a 'let mut' variable, not to a field or an element",
                    ));
                }
                pending.push((value, Position::Value));
            }
            K::NewArray(length, initializer) => {
                match &initializer.kind {
                    K::Lambda {
                        parameters,
                        owned: false,
                        body,
                        ..
                    } if parameters.len() == 1 => pending.push((body, Position::Value)),
                    K::Function(FunctionRef::User(callee))
                        if const_functions.contains(callee)
                            && functions[*callee].parameters.len() == 1 => {}
                    K::Function(FunctionRef::User(callee)) if !constants.contains(callee) => {
                        return Err(if const_functions.contains(callee) {
                            partial(initializer.span)
                        } else {
                            not_const_def(initializer.span, &functions[*callee].name)
                        });
                    }
                    _ => {
                        return Err(failure(
                            DiagnosticKind::ConstNewCollectionForm,
                            initializer.span,
                            "new [T](n, f) in a const def takes a lambda or a const def of one parameter",
                        ));
                    }
                }
                pending.push((length, Position::Value));
            }
            K::Call(callee, arguments) => match &callee.kind {
                K::Function(FunctionRef::User(callee_id)) if constants.contains(callee_id) => {}
                K::Function(FunctionRef::User(callee_id))
                    if const_functions.contains(callee_id) =>
                {
                    if arguments.len() != functions[*callee_id].parameters.len() {
                        return Err(partial(span));
                    }
                    for argument in arguments.iter().rev() {
                        pending.push((argument, Position::Value));
                    }
                }
                K::Function(FunctionRef::User(callee_id)) | K::GenericFunction(callee_id, _) => {
                    return Err(not_const_def(span, &functions[*callee_id].name));
                }
                K::Function(FunctionRef::Builtin(instance)) => {
                    return Err(not_const_def(span, instance.builtin.name()));
                }
                _ => {
                    return Err(failure(
                        DiagnosticKind::ConstCalleeNotConst,
                        span,
                        "only a const def can be called in a const def; mark the callee 'const def' or compute the value at runtime",
                    ));
                }
            },
            K::Function(_) => return Err(partial(span)),
            K::Lambda { .. } | K::Closure(..) => {
                return Err(failure(
                    DiagnosticKind::ConstFunctionValue,
                    span,
                    "a function value is not allowed in a const def; only new [T](n, f) takes one",
                ));
            }
            K::Checked(_) => {
                return Err(failure(
                    DiagnosticKind::ConstCheckedOperation,
                    span,
                    "an '@checked' operation raises an exception that has no compile-time form; use wrapping arithmetic in a const def",
                ));
            }
            K::StructuralCompare(..) => {
                return Err(failure(
                    DiagnosticKind::ConstAggregateOperator,
                    span,
                    "comparing records, tuples, and arrays as a whole is not allowed in a const def; compare their fields",
                ));
            }
            K::Error
            | K::GenericFunction(..)
            | K::Method(..)
            | K::TypeFunction { .. }
            | K::HostCall(..)
            | K::TaskRun(_)
            | K::TaskParallel(_)
            | K::TaskParallelResults(_)
            | K::Parallel(..)
            | K::StructuralHash(_)
            | K::StructuralDisplay(_)
            | K::Interpolated(_)
            | K::List(_)
            | K::ListTail(..)
            | K::NewList(..)
            | K::Slice { .. }
            | K::Borrow(_, true)
            | K::CaseConstructor { .. }
            | K::Try(_)
            | K::RaisedException
            | K::Reraise
            | K::DynDispatch { .. }
            | K::Static(_) => return Err(rejected(span)),
        }
        // What every accepted expression yields is plain data. A borrow of a place is read
        // through, so its reference type is not a value.
        let read_through = matches!(
            expression.kind,
            K::Borrow(..) | K::BorrowOperand(_) | K::Dereference(_)
        );
        if !read_through
            && !*cache
                .entry(&expression.ty)
                .or_insert_with(|| value_type(&expression.ty, types))
        {
            return Err(failure(
                DiagnosticKind::ConstUnbuildableType,
                span,
                format!(
                    "a value of type '{}' cannot be built in a const def; use plain values",
                    expression.ty.display(types)
                ),
            ));
        }
    }
    Ok(())
}
