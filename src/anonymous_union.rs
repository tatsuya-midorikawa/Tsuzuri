//! Anonymous unions `(A | B)` (A24): the generic declarations behind them, the canonical order of
//! their members, and how they read in diagnostics.
//!
//! An anonymous union of n members is `Type::Union(id, members)`, where `id` is the synthesized
//! declaration `$anonymous.Union$n<$t0, ..>` whose case k holds `$t{k}` (D2), and the members are in
//! the canonical order of `order` (D3). Every other part of the compiler sees an instance of a generic
//! union, so ownership, layout, drop, specialization and code generation need nothing new.

use super::*;

/// The pseudo-module of the synthesized declarations. `$` cannot start or appear in a written name,
/// so no user name collides with them and no source can name them.
pub(super) const MODULE: &str = "$anonymous";

/// The most members that an anonymous union has after flattening (D9).
pub(super) const MAX_MEMBERS: usize = 32;

/// `Union$n`, the declaration of the anonymous unions of `members` members.
pub(super) fn declaration_name(members: usize) -> String {
    format!("Union${members}")
}

/// Whether a qualified declaration name belongs to `MODULE`.
pub(super) fn is_anonymous_name(qualified: &str) -> bool {
    qualified
        .strip_prefix(MODULE)
        .is_some_and(|rest| rest.starts_with('.'))
}

/// The declarations `Union$2` to `Union$32` of `MODULE`. The checker adds them after the user unions
/// when some module writes an anonymous union or a type test; otherwise nothing changes.
pub(super) fn declarations() -> Vec<UnionDecl> {
    let generated = |text: String| Ident {
        text,
        span: Span::default(),
        provenance: Provenance::Generated,
    };
    (2..=MAX_MEMBERS)
        .map(|members| {
            let name = declaration_name(members);
            let cases = (0..members)
                .map(|case| UnionCaseDecl {
                    name: generated(format!("{name}${case}")),
                    payload: Some(TypeExpr {
                        kind: TypeExprKind::Variable(format!("$t{case}")),
                        span: Span::default(),
                    }),
                    json: None,
                })
                .collect();
            UnionDecl {
                doc: None,
                visibility: Visibility::Public,
                name: generated(name),
                parameters: (0..members)
                    .map(|parameter| generated(format!("$t{parameter}")))
                    .collect(),
                cases,
                derives: Vec::new(),
            }
        })
        .collect()
}

/// How two member types compare in the canonical order. `Unknown` means that a type variable, an
/// inference variable or an erroneous type decides the comparison, so the order could change with
/// the type arguments.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Order {
    Less,
    Equal,
    Greater,
    Unknown,
}

impl Order {
    fn of(ordering: std::cmp::Ordering) -> Self {
        match ordering {
            std::cmp::Ordering::Less => Order::Less,
            std::cmp::Ordering::Equal => Order::Equal,
            std::cmp::Ordering::Greater => Order::Greater,
        }
    }

    /// This comparison, or `next` when it is `Equal`.
    fn then(self, next: impl FnOnce() -> Order) -> Order {
        match self {
            Order::Equal => next(),
            decided => decided,
        }
    }
}

/// The rank of a type's head, in the declaration order of `Type`. `Once`, `ArrayView`, `Length`,
/// `Reference` and a borrowing `Dyn` are never members; their ranks only keep the order total.
fn rank(ty: &Type) -> Option<u8> {
    Some(match ty {
        Type::Error
        | Type::Variable(_)
        | Type::Infer(_)
        | Type::Partial(_)
        | Type::Application(..) => return None,
        Type::Integer(..) => 0,
        Type::Binary(_) => 1,
        Type::Decimal(_) => 2,
        Type::Simd(_) => 3,
        Type::Bool => 4,
        Type::Unit => 5,
        Type::Char => 6,
        Type::Utf8Char => 7,
        Type::String => 8,
        Type::Utf8String => 9,
        Type::Record(..) => 10,
        Type::Union(..) => 11,
        Type::Array(_) => 12,
        Type::ArrayView(_) => 13,
        Type::FixedArray(..) => 14,
        Type::Length(_) => 15,
        Type::List(_) => 16,
        Type::Vec(_) => 17,
        Type::Tuple(_) => 18,
        Type::Once(_) => 19,
        Type::Handle(_) => 20,
        Type::Function(..) => 21,
        Type::Reference(..) => 22,
        Type::Dyn(_) => 23,
        Type::Shared(..) => 24,
    })
}

/// The canonical order of two member types (D3): the head's rank, then the head's own values, with
/// records and unions by their qualified names, not their ids, so that adding a module changes no
/// order. A function type compares by purity, its parameters with impure currying flattened, and its
/// result; its parameter conventions are not compared, as unification ignores them.
pub(super) fn order(left: &Type, right: &Type, names: &dyn DeclarationNames) -> Order {
    let (Some(left_rank), Some(right_rank)) = (rank(left), rank(right)) else {
        return Order::Unknown;
    };
    if left_rank != right_rank {
        return Order::of(left_rank.cmp(&right_rank));
    }
    match (left, right) {
        (Type::Integer(bits, signed), Type::Integer(other_bits, other_signed)) => {
            Order::of((bits, signed).cmp(&(other_bits, other_signed)))
        }
        (Type::Binary(bits), Type::Binary(other)) | (Type::Decimal(bits), Type::Decimal(other)) => {
            Order::of(bits.cmp(other))
        }
        (Type::Simd(ty), Type::Simd(other)) => Order::of(ty.cmp(other)),
        (Type::Record(id, arguments), Type::Record(other, other_arguments)) => {
            Order::of(names.record_name(*id).cmp(names.record_name(*other)))
                .then(|| sequence(arguments.iter(), other_arguments.iter(), names))
        }
        (Type::Union(id, arguments), Type::Union(other, other_arguments)) => {
            Order::of(names.union_name(*id).cmp(names.union_name(*other)))
                .then(|| sequence(arguments.iter(), other_arguments.iter(), names))
        }
        (Type::Array(element), Type::Array(other))
        | (Type::ArrayView(element), Type::ArrayView(other))
        | (Type::List(element), Type::List(other))
        | (Type::Vec(element), Type::Vec(other))
        | (Type::Once(element), Type::Once(other)) => order(element, other, names),
        (Type::FixedArray(element, length), Type::FixedArray(other, other_length)) => {
            order(element, other, names).then(|| order(length, other_length, names))
        }
        (Type::Length(length), Type::Length(other)) => Order::of(length.cmp(other)),
        (Type::Tuple(elements), Type::Tuple(others)) => {
            sequence(elements.iter(), others.iter(), names)
        }
        (Type::Handle(name), Type::Handle(other)) => Order::of(name.cmp(other)),
        (Type::Function(parameters, result), Type::Function(others, other_result)) => {
            let (pure, chain, result) = function_key(parameters, result);
            let (other_pure, other_chain, other_result) = function_key(others, other_result);
            Order::of(pure.cmp(&other_pure))
                .then(|| sequence(chain.into_iter(), other_chain.into_iter(), names))
                .then(|| order(result, other_result, names))
        }
        (Type::Reference(target, mutable), Type::Reference(other, other_mutable)) => {
            Order::of(mutable.cmp(other_mutable)).then(|| order(target, other, names))
        }
        (Type::Dyn(dyn_type), Type::Dyn(other)) => Order::of(dyn_type.cmp(other)),
        (Type::Shared(value, kind), Type::Shared(other, other_kind)) => {
            Order::of(kind.cmp(other_kind)).then(|| order(value, other, names))
        }
        // Bool, Unit, Char, Utf8Char, String and Utf8String: the rank decides.
        _ => Order::Equal,
    }
}

/// The purity, the parameters and the result of a function type, with the parameters of an impure
/// curried result appended, as `Type::function_with_conventions` builds and `unify` compares them.
fn function_key<'t>(
    parameters: &'t FunctionParameters,
    result: &'t Type,
) -> (bool, Vec<&'t Type>, &'t Type) {
    let mut chain: Vec<&Type> = parameters.iter().collect();
    let mut result = result;
    if !parameters.pure && !parameters.is_empty() {
        while let Type::Function(more, tail) = result
            && !more.pure
            && !more.is_empty()
        {
            chain.extend(more.iter());
            result = tail;
        }
    }
    (parameters.pure, chain, result)
}

/// Two lists of types by length, then element by element.
fn sequence<'t>(
    left: impl ExactSizeIterator<Item = &'t Type>,
    right: impl ExactSizeIterator<Item = &'t Type>,
    names: &dyn DeclarationNames,
) -> Order {
    Order::of(left.len().cmp(&right.len())).then(|| {
        left.zip(right)
            .map(|(left, right)| order(left, right, names))
            .find(|order| *order != Order::Equal)
            .unwrap_or(Order::Equal)
    })
}

/// The resolved members of `(T1 | ... | Tn)` as one type: flattened, checked and in canonical order
/// (the normalization steps 2 to 6 of the ticket). `span` is the whole union type as written.
#[inline(never)]
pub(super) fn canonicalize(
    written: Vec<Type>,
    names: &Names,
    span: Span,
) -> Result<Type, Diagnostic> {
    let mut members = Vec::with_capacity(written.len());
    for member in written {
        match member {
            Type::Union(id, inner) if names.is_anonymous_union(id) => {
                members.extend(inner.into_vec())
            }
            member => members.push(member),
        }
    }
    if members.iter().any(Type::contains_error) {
        return Ok(Type::Error);
    }
    // Before the checks and the quadratic comparisons, so a huge union costs nothing more.
    if members.len() > MAX_MEMBERS {
        return Err(Diagnostic::of(
            DiagnosticKind::AnonymousUnionMemberLimit,
            format!(
                "an anonymous union has at most {MAX_MEMBERS} members, but this one has {}; declare a union with named cases instead",
                members.len()
            ),
            span,
        ));
    }
    for member in &members {
        if member.contains_reference() {
            return Err(Diagnostic::of(
                DiagnosticKind::AnonymousUnionBorrowedMember,
                format!(
                    "an anonymous union holds owned values; {} is borrowed",
                    member.display_with(names)
                ),
                span,
            ));
        }
        if matches!(
            member,
            Type::Variable(_) | Type::Infer(_) | Type::Application(..) | Type::Partial(_)
        ) {
            return Err(Diagnostic::of(
                DiagnosticKind::AnonymousUnionTypeVariable,
                format!(
                    "a type variable cannot be a member of an anonymous union: {}; members must differ without knowing type arguments",
                    display(&members, names)
                ),
                span,
            )
            .with_help("use a union with named cases, such as union OrInt<'a> = Value of 'a | Int of i64"));
        }
    }
    for (index, member) in members.iter().enumerate() {
        for other in &members[index + 1..] {
            match order(member, other, names) {
                Order::Unknown => {
                    return Err(Diagnostic::of(
                        DiagnosticKind::AnonymousUnionIndistinctMembers,
                        format!(
                            "{} and {} may be the same type for some type arguments, so they cannot both be members of an anonymous union",
                            member.display_with(names),
                            other.display_with(names)
                        ),
                        span,
                    ));
                }
                Order::Equal => {
                    return Err(Diagnostic::of(
                        DiagnosticKind::AnonymousUnionDuplicateMember,
                        format!(
                            "{} appears more than once in the anonymous union {}",
                            member.display_with(names),
                            display(&members, names)
                        ),
                        span,
                    )
                    .with_help("a type alias names the same type as its definition"));
                }
                Order::Less | Order::Greater => {}
            }
        }
    }
    // Every pair is decided and distinct, so this is a strict total order on the members.
    members.sort_by(|left, right| match order(left, right, names) {
        Order::Less => std::cmp::Ordering::Less,
        Order::Greater => std::cmp::Ordering::Greater,
        Order::Equal | Order::Unknown => std::cmp::Ordering::Equal,
    });
    let id = names.anonymous_unions[members.len() - 2];
    Ok(Type::Union(id, members.into_boxed_slice()))
}

/// `(m1 | m2 | ...)` in the order given; an impure function member is parenthesized, so the text
/// parses back as the same type.
pub(super) fn display(members: &[Type], names: &dyn DeclarationNames) -> String {
    let members: Vec<String> = members
        .iter()
        .map(|member| {
            let text = member.display_with(names);
            if matches!(member, Type::Function(parameters, _) if !parameters.pure) {
                format!("({text})")
            } else {
                text
            }
        })
        .collect();
    format!("({})", members.join(" | "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn declarations_are_generic_unions_of_2_to_32_members() {
        let declarations = declarations();
        assert_eq!(declarations.len(), 31);
        for (index, union) in declarations.iter().enumerate() {
            let members = index + 2;
            assert_eq!(union.name.text, format!("Union${members}"));
            assert_eq!(union.name.provenance, Provenance::Generated);
            assert_eq!(union.visibility, Visibility::Public);
            assert!(union.derives.is_empty() && union.doc.is_none());
            let parameters: Vec<_> = union.parameters.iter().map(|p| p.text.as_str()).collect();
            let expected: Vec<_> = (0..members).map(|k| format!("$t{k}")).collect();
            assert_eq!(parameters, expected);
            assert_eq!(union.cases.len(), members);
            for (case, declaration) in union.cases.iter().enumerate() {
                assert_eq!(declaration.name.text, format!("Union${members}${case}"));
                assert!(matches!(
                    &declaration.payload,
                    Some(TypeExpr { kind: TypeExprKind::Variable(name), .. }) if *name == format!("$t{case}")
                ));
            }
        }
        // The cases of all declarations together (D9).
        let cases: usize = declarations.iter().map(|union| union.cases.len()).sum();
        assert_eq!(cases, 527);
    }

    #[test]
    fn names_belong_to_the_pseudo_module() {
        assert!(is_anonymous_name("$anonymous.Union$2"));
        assert!(!is_anonymous_name("$anonymousX.Union$2"));
        assert!(!is_anonymous_name("Main.Union"));
        // `$` is followed by a lowercase letter or a digit, never one of the escapes of
        // `mangled_type` (`$A`, `$L`, `$R`, `$C`, `$P`).
        for union in declarations() {
            let qualified = format!("{MODULE}.{}", union.name.text);
            for text in std::iter::once(qualified)
                .chain(union.parameters.iter().map(|p| p.text.clone()))
                .chain(union.cases.iter().map(|case| case.name.text.clone()))
            {
                for (at, _) in text.match_indices('$') {
                    let next = text[at + 1..].chars().next().unwrap();
                    assert!(next.is_ascii_lowercase() || next.is_ascii_digit(), "{text}");
                }
            }
        }
    }
}
