//! Read-only static data for constants of plain data (D11 Phase 2).
//!
//! A constant whose value is plain data (numbers, characters, tuples, records, fixed-length
//! arrays, unions whose payloads share one type, and, as the whole constant, a heap array of
//! those) is kept once in the binary. The uses that read it in place or borrow it do not rebuild
//! the value: `fold` rewrites them to the one `Static` object, behind a shared borrow that never
//! drops. Every other use of the constant keeps its value semantics and gets its own copy.

use super::*;

/// The most bytes of static data that one program may keep, summed over its constants. A program
/// above it is an error, not a silent fallback to copies.
pub(super) const MAX_STATIC_BYTES: usize = 4 << 20;

/// Whether the constant of type `ty` is kept as static data where it is read in place or borrowed:
/// its value is plain data with nothing to drop. A heap array counts only as the whole constant.
pub(super) fn eligible(ty: &Type, types: &TypeContext<'_>) -> bool {
    let stored = match ty {
        Type::Array(element) => element,
        _ => ty,
    };
    !stored.needs_drop(types) && types.stored_all(stored, |part| plain(part, types))
}

fn plain(ty: &Type, types: &TypeContext<'_>) -> bool {
    match ty {
        Type::Integer(..)
        | Type::Binary(_)
        | Type::Decimal(_)
        | Type::Bool
        | Type::Unit
        | Type::Char
        | Type::Utf8Char
        | Type::Tuple(_) => true,
        Type::FixedArray(_, length) => matches!(**length, Type::Length(_)),
        Type::Record(id, _) => {
            let record = &types.records[*id];
            !record.user_drop && !record.opaque() && !ty.is_noncopy_record(types)
        }
        // A union is a bare tag, or a tag and one payload type that every case shares.
        Type::Union(id, arguments) => {
            let mut payloads = types.union_payloads(*id, arguments).into_iter().flatten();
            let first = payloads.next();
            !types.unions[*id].user_drop
                && !types.recursive(ty)
                && payloads.all(|payload| Some(&payload) == first.as_ref())
        }
        Type::String
        | Type::Utf8String
        | Type::Array(_)
        | Type::List(_)
        | Type::Vec(_)
        | Type::Simd(_)
        | Type::Task(_)
        | Type::Handle(_)
        | Type::Function(..)
        | Type::Reference(..)
        | Type::Dyn(_)
        | Type::Shared(..)
        | Type::ArrayView(_)
        | Type::Length(_)
        | Type::Error
        | Type::Variable(_)
        | Type::Infer(_)
        | Type::Partial(_)
        | Type::Application(..) => false,
    }
}

/// The size and the alignment of a plain type in its LLVM layout, on the targets whose `i128` is
/// 16-byte aligned; narrower alignments make the real size no larger.
fn layout(ty: &Type, types: &TypeContext<'_>) -> (usize, usize) {
    let aggregate = |fields: Vec<(usize, usize)>| {
        let (size, align) = fields
            .into_iter()
            .fold((0usize, 1usize), |(size, align), field| {
                (size.next_multiple_of(field.1) + field.0, align.max(field.1))
            });
        (size.next_multiple_of(align), align)
    };
    match ty {
        Type::Integer(bits, _) | Type::Binary(bits) | Type::Decimal(bits) => {
            let bytes = usize::from(*bits).div_ceil(8);
            (bytes, bytes)
        }
        Type::Bool | Type::Unit => (1, 1),
        Type::Char => (2, 2),
        Type::Utf8Char => (4, 4),
        Type::FixedArray(element, _) => {
            let (size, align) = layout(element, types);
            let length = usize::try_from(ty.fixed_length().unwrap_or(0)).unwrap_or(usize::MAX);
            (size.saturating_mul(length), align)
        }
        Type::Tuple(elements) => aggregate(elements.iter().map(|ty| layout(ty, types)).collect()),
        Type::Record(id, arguments) => aggregate(
            types
                .record_fields(*id, arguments)
                .iter()
                .map(|ty| layout(ty, types))
                .collect(),
        ),
        Type::Union(id, arguments) => match types
            .union_payloads(*id, arguments)
            .into_iter()
            .flatten()
            .next()
        {
            Some(payload) => aggregate(vec![(4, 4), layout(&payload, types)]),
            None => (4, 4),
        },
        // A plain constant holds none of these.
        Type::String
        | Type::Utf8String
        | Type::Array(_)
        | Type::List(_)
        | Type::Vec(_)
        | Type::Simd(_)
        | Type::Task(_)
        | Type::Handle(_)
        | Type::Function(..)
        | Type::Reference(..)
        | Type::Dyn(_)
        | Type::Shared(..)
        | Type::ArrayView(_)
        | Type::Length(_)
        | Type::Error
        | Type::Variable(_)
        | Type::Infer(_)
        | Type::Partial(_)
        | Type::Application(..) => (8, 8),
    }
}

/// The bytes of the static data of a constant's value.
fn bytes(value: &TypedExpr, types: &TypeContext<'_>) -> usize {
    match (&value.ty, &value.kind) {
        (Type::Array(element), TypedExprKind::Array(items)) => {
            layout(element, types).0.saturating_mul(items.len())
        }
        (ty, _) => layout(ty, types).0,
    }
}

/// The constant that `expression` names, when it is one of the `eligible` constants.
fn plain_constant(expression: &TypedExpr, eligible: &BTreeSet<usize>) -> Option<usize> {
    constant_id(expression).filter(|id| eligible.contains(id))
}

/// Rewrites the uses of an eligible constant that read it in place or borrow it, in `body` and
/// below, and notes the constants in `used`:
///
/// * `ref C` (and `&C`, and a constant passed to a `ref` parameter) borrows `Static(C)`;
/// * `C[i]`, `C.length`, `C.field`, and `ref C[a..b]` read `*(ref Static(C))` in place.
///
/// Any other use of `C`, such as `let t = C` or passing it by value, is left for `inline`, which
/// copies the value there.
pub(super) fn mark_uses(
    body: &mut TypedExpr,
    eligible: &BTreeSet<usize>,
    used: &mut BTreeSet<usize>,
) {
    use TypedExprKind as K;
    let mut pending = vec![body];
    while let Some(expression) = pending.pop() {
        let borrowed = match &expression.kind {
            K::Borrow(operand, false) | K::BorrowOperand(operand) => {
                plain_constant(operand, eligible)
            }
            _ => None,
        };
        if let Some(id) = borrowed {
            used.insert(id);
            let kind = std::mem::replace(&mut expression.kind, K::Error);
            let (K::Borrow(mut operand, _) | K::BorrowOperand(mut operand)) = kind else {
                unreachable!("only a borrow names a borrowed constant");
            };
            operand.kind = K::Static(id);
            expression.kind = K::Borrow(operand, false);
            continue;
        }
        let shared = !expression.ty.is_view();
        let base = match &mut expression.kind {
            K::Index(base, _) | K::Length(base) | K::Field(base, _) => Some(base),
            K::ForEach { source, .. } => Some(source),
            K::Slice { value, .. } if shared => Some(value),
            _ => None,
        };
        if let Some(base) = base
            && let Some(id) = plain_constant(base, eligible)
        {
            used.insert(id);
            let (ty, span) = (base.ty.clone(), base.span);
            let data = TypedExpr {
                kind: K::Static(id),
                ty: ty.clone(),
                span,
            };
            let reference = TypedExpr {
                kind: K::Borrow(Box::new(data), false),
                ty: Type::Reference(Box::new(ty), false),
                span,
            };
            base.kind = K::Dereference(Box::new(reference));
        }
        expression
            .try_for_each_child_mut(&mut |child| {
                pending.push(child);
                Ok(())
            })
            .expect("rewriting children cannot fail");
    }
}

/// The static data of the constants in `used`, with their evaluated values. `Err` names the first
/// constant whose data brings the total over `MAX_STATIC_BYTES`.
pub(super) fn collect(
    used: &BTreeSet<usize>,
    values: &BTreeMap<usize, Value>,
    types: &TypeContext<'_>,
) -> Result<BTreeMap<usize, StaticData>, usize> {
    let mut statics = BTreeMap::new();
    let mut total = 0usize;
    for &id in used {
        let value = values[&id].expr.clone();
        total = total.saturating_add(bytes(&value, types));
        if total > MAX_STATIC_BYTES {
            return Err(id);
        }
        statics.insert(id, StaticData { value });
    }
    Ok(statics)
}

/// The error for a constant whose static data is over the limit.
pub(super) fn too_big(span: Span) -> Diagnostic {
    failure(
        DiagnosticKind::ConstStaticDataTooLarge,
        span,
        format!(
            "the constants that the program reads in place or borrows hold more than {MAX_STATIC_BYTES} bytes of static data; make them smaller or read this one through a 'let' binding of its value"
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn array(count: usize) -> TypedExpr {
        let span = Span::default();
        let element = Type::Integer(128, false);
        TypedExpr {
            kind: TypedExprKind::Array(
                (0..count)
                    .map(|_| TypedExpr {
                        kind: TypedExprKind::Int(1),
                        ty: element.clone(),
                        span,
                    })
                    .collect(),
            ),
            ty: Type::Array(Box::new(element)),
            span,
        }
    }

    #[test]
    fn the_total_of_static_data_is_bounded() {
        let types = TypeContext {
            records: &[],
            unions: &[],
        };
        let values = |counts: &[usize]| -> BTreeMap<usize, Value> {
            counts
                .iter()
                .enumerate()
                .map(|(id, count)| (id, Value::measured(array(*count))))
                .collect()
        };
        // Sixteen-byte elements: 262144 of them are exactly 4 MiB, in one constant or in two.
        for counts in [[262_144, 0], [100_000, 162_144]] {
            let used: BTreeSet<usize> = [0, 1].into();
            let statics = collect(&used, &values(&counts), &types).unwrap();
            assert_eq!(statics.len(), 2);
        }
        let used: BTreeSet<usize> = [0, 1].into();
        assert_eq!(
            collect(&used, &values(&[262_145, 0]), &types).err(),
            Some(0)
        );
        assert_eq!(
            collect(&used, &values(&[262_144, 1]), &types).err(),
            Some(1)
        );
        // A constant that no use reads in place is not counted.
        let used: BTreeSet<usize> = [1].into();
        assert!(collect(&used, &values(&[300_000, 10]), &types).is_ok());
    }

    #[test]
    fn plain_types_are_decided_by_their_parts() {
        let types = TypeContext {
            records: &[],
            unions: &[],
        };
        let element = |ty: Type| Type::Array(Box::new(ty));
        assert!(eligible(&Type::I64, &types));
        assert!(eligible(&Type::Unit, &types));
        assert!(eligible(&element(Type::I64), &types));
        assert!(eligible(
            &Type::Tuple(vec![Type::I64, Type::Bool, Type::Binary(16)]),
            &types
        ));
        assert!(eligible(
            &element(Type::Tuple(vec![Type::Char, Type::Decimal(128)])),
            &types
        ));
        // Heap storage, strings, and nested arrays are not plain data.
        assert!(!eligible(&Type::String, &types));
        assert!(!eligible(&element(Type::String), &types));
        assert!(!eligible(&element(element(Type::I64)), &types));
        assert!(!eligible(
            &Type::Tuple(vec![Type::I64, Type::Utf8String]),
            &types
        ));
        assert!(!eligible(
            &Type::Reference(Box::new(Type::I64), false),
            &types
        ));
    }
}
