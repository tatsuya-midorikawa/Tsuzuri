//! Anonymous unions `(A | B)` (A24): the generic declarations behind them, the canonical order of
//! their members, and how they read in diagnostics.
//!
//! An anonymous union of n members is `Type::Union(id, members)`, where `id` is the synthesized
//! declaration `$anonymous.Union$n<$t0, ..>` whose case k holds `$t{k}` (D2), and the members are in
//! the canonical order of `order` (D3). Every other part of the compiler sees an instance of a generic
//! union, so ownership, layout, drop, specialization and code generation need nothing new.

use std::rc::Rc;

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
/// the type arguments. `Purity` means that the types differ only in the purity of a function type,
/// which picks no member when a value is injected (D5). `Currying` means that they differ only in
/// how a pure function type is curried (`pure (A -> (B -> C))` and `pure (A -> B -> C)`), which
/// the text of a type does not show, so a diagnostic could not name one of the two members.
///
/// Purity is no key of the order: two types that differ in it and, at another place, in something
/// that the order reads are `Less` or `Greater` by that place, wherever the two places are. Were
/// purity a key that such a place overrides, three members could be ordered in a circle. `Purity`
/// and `Currying` are answered only for two types that the order reads alike.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Order {
    Less,
    Equal,
    Greater,
    Unknown,
    Purity,
    Currying,
}

impl Order {
    fn of(ordering: std::cmp::Ordering) -> Self {
        match ordering {
            std::cmp::Ordering::Less => Order::Less,
            std::cmp::Ordering::Equal => Order::Equal,
            std::cmp::Ordering::Greater => Order::Greater,
        }
    }

    /// This comparison when it is `Less`, `Greater` or `Unknown`; otherwise what `next` answers,
    /// except that `Purity` or `Currying` stays when `next` finds no other difference.
    fn then(self, next: impl FnOnce() -> Order) -> Order {
        match self {
            Order::Less | Order::Greater | Order::Unknown => self,
            Order::Equal => next(),
            Order::Purity | Order::Currying => match next() {
                Order::Equal | Order::Purity | Order::Currying => self,
                decided => decided,
            },
        }
    }

    /// The comparison of two lists of equal length, element by element.
    fn all<'t>(
        pairs: impl Iterator<Item = (&'t Type, &'t Type)>,
        names: &dyn DeclarationNames,
    ) -> Order {
        let mut so_far = Order::Equal;
        for (left, right) in pairs {
            so_far = so_far.then(|| order(left, right, names));
            if matches!(so_far, Order::Less | Order::Greater | Order::Unknown) {
                break;
            }
        }
        so_far
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
/// order. Function types compare as `functions` says; their parameter conventions are not compared,
/// as unification ignores them, and neither is their purity (see `Order`).
///
/// A decided answer (`Less` or `Greater`) holds for every substitution of the type variables: it
/// is found at a place that no variable reaches, and `Equal`, `Purity` and `Currying` are only
/// answered for types without variables. A generic declaration therefore keeps its members in the
/// canonical order, and pairwise distinct, in every instance.
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
            let curried = functions(
                FunctionKey::of(parameters, result),
                FunctionKey::of(others, other_result),
                names,
            );
            match curried {
                // A pure function is also a value of the function type that is not pure, so the
                // two are members of one union only when they differ without their purity.
                Order::Equal if parameters.pure != others.pure => Order::Purity,
                // `pure (A -> (B -> C))` and `pure (A -> B -> C)` are two types that read alike.
                Order::Equal if parameters.pure && parameters.len() != others.len() => {
                    Order::Currying
                }
                decided => decided,
            }
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

/// A function type as `order` reads it: its parameters and its result.
struct FunctionKey<'t> {
    chain: Vec<&'t Type>,
    result: &'t Type,
    /// Whether filling in a type variable can add parameters to `chain`.
    grows: bool,
}

impl<'t> FunctionKey<'t> {
    /// The key of `parameters -> result`, read as a function that is not pure, whatever its
    /// purity (a pure function is also a value of that type): the parameters of a curried result
    /// that is not pure are appended, as `Type::function_with_conventions` builds and `unify`
    /// compares them (`i64 -> (i64 -> i64)` is `i64 -> i64 -> i64`). Such a chain grows when its
    /// result is still a type variable, an inference variable or the application of one, which
    /// a function type may replace.
    fn of(parameters: &'t FunctionParameters, result: &'t Type) -> Self {
        let mut chain: Vec<&Type> = parameters.iter().collect();
        let mut result = result;
        let curried = !parameters.is_empty();
        if curried {
            while let Type::Function(more, tail) = result
                && !more.pure
                && !more.is_empty()
            {
                chain.extend(more.iter());
                result = tail;
            }
        }
        Self {
            chain,
            result,
            grows: curried && rank(result).is_none(),
        }
    }
}

/// The order of two function types without their purity: the parameters that both have, one by
/// one; then the shorter chain first; then the results. A chain that can grow
/// (`FunctionKey::grows`) is not ordered against a longer one that agrees with it so far, because
/// a type argument could make the two the same type or turn their order: `(i64 -> 'r)` and
/// `(i64 -> i64 -> i64)` are `Unknown`. The number of parameters is not compared before the
/// parameters themselves, so that a chain that grows keeps its place: `(i64 -> 'r)` is before
/// `(string -> i64)` whatever `'r` is.
fn functions(left: FunctionKey<'_>, right: FunctionKey<'_>, names: &dyn DeclarationNames) -> Order {
    let common = left.chain.iter().copied().zip(right.chain.iter().copied());
    Order::all(common, names).then(|| match left.chain.len().cmp(&right.chain.len()) {
        std::cmp::Ordering::Equal => order(left.result, right.result, names),
        std::cmp::Ordering::Less if left.grows => Order::Unknown,
        std::cmp::Ordering::Greater if right.grows => Order::Unknown,
        shorter => Order::of(shorter),
    })
}

/// Two lists of types by length, then element by element.
fn sequence<'t>(
    left: impl ExactSizeIterator<Item = &'t Type>,
    right: impl ExactSizeIterator<Item = &'t Type>,
    names: &dyn DeclarationNames,
) -> Order {
    Order::of(left.len().cmp(&right.len())).then(|| Order::all(left.zip(right), names))
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
                Order::Purity => {
                    return Err(Diagnostic::of(
                        DiagnosticKind::AnonymousUnionIndistinctMembers,
                        format!(
                            "{} and {} differ only in the purity of a function type, and purity does not pick a member, so they cannot both be members of an anonymous union",
                            member.display_with(names),
                            other.display_with(names)
                        ),
                        span,
                    )
                    .with_help("a pure function is also a value of the function type that is not pure; keep one of the two members, or declare a union with named cases"));
                }
                Order::Currying => {
                    return Err(Diagnostic::of(
                        DiagnosticKind::AnonymousUnionIndistinctMembers,
                        format!(
                            "{} and {} differ only in how a pure function type is curried, which their text does not show, so they cannot both be members of an anonymous union",
                            member.display_with(names),
                            other.display_with(names)
                        ),
                        span,
                    )
                    .with_help("keep one of the two members, or declare a union with named cases"));
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
        Order::Equal | Order::Unknown | Order::Purity | Order::Currying => {
            std::cmp::Ordering::Equal
        }
    });
    let id = names.anonymous_unions[members.len() - 2];
    Ok(Type::Union(id, members.into_boxed_slice()))
}

/// `(m1 | m2 | ...)` in the order given; an impure function member is parenthesized, so the text
/// parses back as the same type.
pub(super) fn display(members: &[Type], names: &dyn DeclarationNames) -> String {
    let members: Vec<String> = members
        .iter()
        .map(|member| member_display(member, names))
        .collect();
    format!("({})", members.join(" | "))
}

/// One member as `display` writes it.
fn member_display(member: &Type, names: &dyn DeclarationNames) -> String {
    let text = member.display_with(names);
    if matches!(member, Type::Function(parameters, _) if !parameters.pure) {
        format!("({text})")
    } else {
        text
    }
}

/// `:? T`, the type test that names `member` in a witness of a non-exhaustive match (D7). `T` is
/// read by `type_product`, so an impure function member is parenthesized, as in `display`. `None`
/// when the member is not known, or holds an inference variable or an erroneous type: the message
/// then reads `:? _`, which is not a pattern, so no arm is offered.
pub(super) fn type_test_text(
    member: Option<&Type>,
    names: &dyn DeclarationNames,
) -> Option<String> {
    member
        .filter(|member| determined(member))
        .map(|member| format!(":? {}", member_display(member, names)))
}

/// Whether `ty` holds no inference variable and no erroneous type, so that a type test can name it.
fn determined(ty: &Type) -> bool {
    match ty {
        Type::Error | Type::Infer(_) => false,
        Type::Partial(partial) => partial.trailing.iter().all(determined),
        Type::Application(head, arguments) => determined(head) && arguments.iter().all(determined),
        Type::Array(ty)
        | Type::ArrayView(ty)
        | Type::List(ty)
        | Type::Vec(ty)
        | Type::Once(ty)
        | Type::Shared(ty, _)
        | Type::Reference(ty, _) => determined(ty),
        Type::FixedArray(element, length) => determined(element) && determined(length),
        Type::Function(parameters, result) => {
            parameters.iter().all(determined) && determined(result)
        }
        Type::Tuple(elements) => elements.iter().all(determined),
        Type::Record(_, args) | Type::Union(_, args) => args.iter().all(determined),
        _ => true,
    }
}

/// `the members are m1, m2, ...`, the help of the diagnostics that name a type outside a union.
pub(super) fn members_help(members: &[Type], names: &dyn DeclarationNames) -> String {
    let members: Vec<String> = members
        .iter()
        .map(|member| member_display(member, names))
        .collect();
    format!("the members are {}", members.join(", "))
}

/// The class of an unsuffixed number literal, which picks its member by itself (D4 rule 2).
#[derive(Clone, Copy, PartialEq, Eq)]
enum LiteralClass {
    Integer,
    Float,
}

/// The class and the text of `expression` when it is an unsuffixed number literal, negated or not.
/// `Checker::untyped_number` also answers for other unary operators, which this does not.
fn literal_class(expression: &Expr) -> Option<(LiteralClass, String)> {
    let (operand, sign) = match &expression.kind {
        ExprKind::Unary(UnaryOp::Negate, operand) => (operand.as_ref(), "-"),
        _ => (expression, ""),
    };
    match &operand.kind {
        ExprKind::Integer(value, None) => Some((LiteralClass::Integer, format!("{sign}{value}"))),
        ExprKind::Float(text, None) => Some((LiteralClass::Float, format!("{sign}{text}"))),
        _ => None,
    }
}

/// `Construct` of member `case` of the anonymous union `union` (`Some x` for a named union): the
/// value is used once, then tagged.
fn wrap(value: TypedExpr, union: &Type, case: usize) -> TypedExpr {
    let Type::Union(union_id, _) = union else {
        unreachable!("an anonymous union is a union type")
    };
    TypedExpr {
        span: value.span,
        ty: union.clone(),
        kind: TypedExprKind::Construct {
            union_id: *union_id,
            case_id: case,
            payload: Some(Box::new(value)),
        },
    }
}

/// The expressions that pass their expected type on to their arms, results or reports, so that
/// each arm is injected by itself and nothing is wrapped twice (D4 rule 1). A hole reports the
/// union; a dyn dispatch takes it as its result type.
fn passes_expected_down(expression: &Expr) -> bool {
    matches!(
        expression.kind,
        ExprKind::If { .. }
            | ExprKind::Match { .. }
            | ExprKind::Block { .. }
            | ExprKind::ComputationBoundary(_)
            | ExprKind::Select(_)
            | ExprKind::Checked(_)
            | ExprKind::Hole(_)
            | ExprKind::DynDispatch { .. }
    )
}

/// The injection of values into anonymous unions (D1, D4, D5). Every entry point first checks
/// `names.anonymous_unions.is_empty()`, and the work is in functions of their own that are never
/// inlined, so that a program without anonymous unions is checked exactly as before and the
/// recursive frames of `Checker::expression` and `Checker::argument` do not grow.
impl Checker<'_> {
    /// Whether `ty`, followed through its solved inference variables only, is an anonymous union.
    pub(super) fn anonymous_union_head(&self, ty: &Type) -> bool {
        matches!(self.inference.head(ty), Type::Union(id, _) if self.names.is_anonymous_union(*id))
    }

    /// Whether `expected` is an anonymous union `U` or a shared reference `ref U` (a read
    /// parameter), the two places where a value is injected into an argument.
    pub(super) fn anonymous_union_parameter(&self, expected: &Type) -> bool {
        match self.inference.head(expected) {
            Type::Reference(inner, false) => self.anonymous_union_head(inner),
            ty => self.anonymous_union_head(ty),
        }
    }

    /// Records the result positions of `operand`, the operand of a binary operator that is checked
    /// with its sibling's type as its expected type, for `injected_expression`, which does not
    /// widen there, and returns the length of `Checker::operand` to truncate it back to once the
    /// operand is checked. The operand checked first is not recorded: it is checked by itself, so
    /// an `if` there gives its `then` branch's type to its `else` branch as anywhere else (D4), and
    /// that branch is widened. A result position is the operand, or what a form of
    /// `passes_expected_down` in one passes the expected type down to as its value: the branches
    /// of an `if`, the arms of a `match` or `select`, the result of a block, `@checked` or a
    /// computation expression. Everything else (a call and its arguments, the value of a `let`, a
    /// condition, a guard, a lambda, a pipe) is checked by itself, so a conversion that it asks for
    /// stays. Positions are kept by span and depth, the height of the subtree: a generated node can
    /// share its span with a child (the parser writes `f >> g` as a block with a lambda of its
    /// span), and the depth tells them apart. `select` arms keep their spans and depths through
    /// the `match` that `Checker::select` builds of clones of them.
    #[inline(never)]
    pub(super) fn operand_results(&mut self, operand: &Expr) -> usize {
        let marked = self.operand.len();
        if self.names.anonymous_unions.is_empty() {
            return marked;
        }
        let mut pending = vec![operand];
        while let Some(expression) = pending.pop() {
            match &expression.kind {
                ExprKind::If {
                    then_branch,
                    else_branch,
                    ..
                } => pending.extend([&**then_branch, &**else_branch]),
                ExprKind::Match { arms, .. } => pending.extend(arms.iter().map(|arm| &arm.body)),
                ExprKind::Select(arms) => pending.extend(arms.iter().map(|arm| &arm.body)),
                ExprKind::Block { result, .. }
                | ExprKind::ComputationBoundary(result)
                | ExprKind::Checked(result) => pending.push(result),
                _ => {
                    // A hole and a dyn dispatch pass the type down, but to no value of theirs.
                    debug_assert!(
                        !passes_expected_down(expression)
                            || matches!(
                                expression.kind,
                                ExprKind::Hole(_) | ExprKind::DynDispatch { .. }
                            ),
                        "operand_results follows every form of passes_expected_down"
                    );
                    self.operand.push((expression.span, expression.depth));
                }
            }
        }
        marked
    }

    /// Whether `Checker::expression` injects `expression` into `expected`, which the caller has
    /// found to be some type while anonymous unions exist.
    pub(super) fn injects(&self, expression: &Expr, expected: &Type) -> bool {
        self.anonymous_union_head(expected) && !passes_expected_down(expression)
    }

    /// `Checker::expression` of a value whose expected type is an anonymous union: the value of
    /// `union_value`, unified with the expected type.
    #[inline(never)]
    pub(super) fn injected_expression(
        &mut self,
        expression: &Expr,
        expected: &Type,
    ) -> Result<TypedExpr, Diagnostic> {
        // The operand of a binary operator that takes its sibling's type is no place of D-62, and
        // neither is each branch, arm or block result that is its value: it is injected (A24) but
        // not widened, so neither `v == w` nor `v == (if c then w else u)` moves `w` (A25).
        let widen = !self.operand.contains(&(expression.span, expression.depth));
        let union = self.inference.resolve(expected);
        let value = self.union_value(expression, &union, widen)?;
        self.finish_expression(value.kind, value.ty, Some(&union), expression.span)
    }

    /// `Checker::argument` for a parameter of type `U` or `ref U`: the value of `union_value`,
    /// borrowed by `coerce_argument` when the parameter reads it (as `f (Some x)` is). It runs
    /// before every other kind of argument, so a call is checked without an expected type and
    /// never waits with an argument hint (A27).
    #[inline(never)]
    pub(super) fn union_argument(
        &mut self,
        expression: &Expr,
        expected: &Type,
    ) -> Result<TypedExpr, Diagnostic> {
        let target = self.inference.resolve(expected);
        let union = match &target {
            Type::Reference(inner, false) => (**inner).clone(),
            ty => ty.clone(),
        };
        let value = if passes_expected_down(expression) {
            if !matches!(target, Type::Reference(..)) {
                return self.expression(expression, Some(&target));
            }
            self.expression(expression, Some(&union))?
        } else {
            self.union_value(expression, &union, true)?
        };
        self.coerce_argument(value, &target)
    }

    /// The value of `expression` where the anonymous union `union` (resolved) is expected (D4
    /// rules 2 and 3): a member that the expression's form picks is its expected type, otherwise
    /// the expression is checked by itself and `conform_union` decides. `widen` is false for a
    /// result position of the operand that takes its sibling's type (`operand_results`), which is
    /// not widened from another anonymous union.
    fn union_value(
        &mut self,
        expression: &Expr,
        union: &Type,
        widen: bool,
    ) -> Result<TypedExpr, Diagnostic> {
        let Type::Union(_, members) = union else {
            unreachable!("an anonymous union is a union type")
        };
        if let Some(case) = self.preselect(expression, union, members)? {
            let value = self.expression(expression, Some(&members[case]))?;
            return Ok(wrap(value, union, case));
        }
        let value = self.expression(expression, None)?;
        self.conform_union(value, union, widen)
    }

    /// The member that the form of `expression` picks (D4 rule 2): an unsuffixed number literal
    /// takes the only integer member, else the bigint member, else the only floating-point member
    /// (a float literal only the last); a lambda the only function member; `Dyn.of v` the only dyn
    /// member; an array literal the only array or fixed-length array member, else the only one of
    /// them that takes a literal of its length; a list literal the only list member; a tuple the
    /// only tuple member of its length. `None` when nothing or more than one member fits the
    /// form, except that a number literal that fits two members is an error. An array literal
    /// that several members take is checked by itself, and `conform_union` decides.
    fn preselect(
        &mut self,
        expression: &Expr,
        union: &Type,
        members: &[Type],
    ) -> Result<Option<usize>, Diagnostic> {
        if let Some((class, text)) = literal_class(expression) {
            return self.literal_member(class, &text, union, members, expression.span);
        }
        let only = |fits: &dyn Fn(&Type) -> bool| {
            let mut fitting = members
                .iter()
                .enumerate()
                .filter(|(_, member)| fits(member));
            match (fitting.next(), fitting.next()) {
                (Some((case, _)), None) => Some(case),
                _ => None,
            }
        };
        Ok(match &expression.kind {
            ExprKind::Lambda(..) => only(&|member| matches!(member, Type::Function(..))),
            ExprKind::Call(callee, arguments)
                if arguments.len() == 1 && self.names_dyn_of(callee) =>
            {
                only(&|member| matches!(member, Type::Dyn(_)))
            }
            ExprKind::Array(values) => {
                only(&|member| matches!(member, Type::Array(_) | Type::FixedArray(..)))
                    .or_else(|| only(&|member| self.takes_array_literal(member, values.len())))
            }
            ExprKind::List(_) => only(&|member| matches!(member, Type::List(_))),
            ExprKind::NewLiteral(literal) => match literal.kind {
                ExprKind::Array(_) => only(&|member| matches!(member, Type::Array(_))),
                ExprKind::List(_) => only(&|member| matches!(member, Type::List(_))),
                _ => None,
            },
            ExprKind::Tuple(values) => only(
                &|member| matches!(member, Type::Tuple(elements) if elements.len() == values.len()),
            ),
            _ => None,
        })
    }

    /// The member of an unsuffixed number literal: the integer members first, then a bigint
    /// member, then the floating-point members (a float literal looks only at those). Two members
    /// of the first group that has any is `anonymous_union_ambiguous_literal`: no member is a
    /// default.
    fn literal_member(
        &self,
        class: LiteralClass,
        text: &str,
        union: &Type,
        members: &[Type],
        span: Span,
    ) -> Result<Option<usize>, Diagnostic> {
        let groups: [&dyn Fn(&Type) -> bool; 3] = [
            &|member| class == LiteralClass::Integer && matches!(member, Type::Integer(..)),
            &|member| class == LiteralClass::Integer && self.is_bigint(member),
            &|member| member.is_float(),
        ];
        for fits in groups {
            let fitting: Vec<usize> = members
                .iter()
                .enumerate()
                .filter(|(_, member)| fits(member))
                .map(|(case, _)| case)
                .collect();
            match fitting.as_slice() {
                [] => {}
                [case] => return Ok(Some(*case)),
                [first, second, ..] => {
                    let first = member_display(&members[*first], &self.types);
                    let second = member_display(&members[*second], &self.types);
                    return Err(Diagnostic::of(
                        DiagnosticKind::AnonymousUnionAmbiguousLiteral,
                        format!(
                            "the literal {text} could be {first} or {second} in {}",
                            union.display(&self.types)
                        ),
                        span,
                    )
                    .with_help(format!("add a suffix, for example {text}{first}")));
                }
            }
        }
        Ok(None)
    }

    /// The value `value`, checked without an expected type, where the anonymous union `union`
    /// (resolved) is expected (D4 rule 3). An erroneous value and a value of an undetermined type
    /// that is no number literal are not wrapped: the caller unifies them with the union. A
    /// value of an anonymous union with fewer members is widened (`widened`, A25) when `widen`
    /// allows it; any other anonymous union and a reference to one are left to the caller, which
    /// unifies them or reborrows the reference. Any other value is wrapped as the only member
    /// that its type may be (`may_member`); when its type may be none, as the only member that
    /// it reaches through the adapter of a borrowed parameter (`Fit::Borrowed`). An array
    /// literal inside the value picks no member silently (`literal_fits`).
    #[inline(never)]
    pub(super) fn conform_union(
        &mut self,
        mut value: TypedExpr,
        union: &Type,
        widen: bool,
    ) -> Result<TypedExpr, Diagnostic> {
        let Type::Union(_, members) = union else {
            unreachable!("an anonymous union is a union type")
        };
        let found = self.inference.resolve(&value.ty);
        match &found {
            Type::Error => return Ok(value),
            Type::Union(id, from) if self.names.is_anonymous_union(*id) => {
                return if widen && from.len() != members.len() {
                    self.widened(value, &found, union)
                } else {
                    Ok(value)
                };
            }
            Type::Reference(inner, _) if self.anonymous_union_head(inner) => {
                return if widen {
                    self.borrowed_union(value, &found, union)
                } else {
                    Ok(value)
                };
            }
            Type::Infer(_) if !self.inference.is_numeric_literal(&found) => {
                // No information to decide by, so the value is the union: `let v: (i64 | string)
                // = Parse.parse text` reads a union.
                self.same(&found, union, value.span)?;
                value.ty = union.clone();
                return Ok(value);
            }
            _ => {}
        }
        if let Some((case, literal)) = self.retyped_literal(&value, union, members)? {
            return Ok(wrap(literal, union, case));
        }
        // A member is never a reference: a borrowed value is read as the value it borrows, which
        // the union then owns (a move out of the borrow, as in `Some s`, D10).
        let value = if matches!(found, Type::Reference(..)) {
            Self::autoderef(value)
        } else {
            value
        };
        let found = self.inference.resolve(&value.ty);
        let fitting = |fits: &dyn Fn(&Type) -> bool| -> Vec<usize> {
            members
                .iter()
                .enumerate()
                .filter(|(_, member)| fits(member))
                .map(|(case, _)| case)
                .collect()
        };
        let mut candidates = fitting(&|member| self.may_member(&found, member));
        // The members that the value could also be if its array literals were checked with the
        // member as their expected type: no member is a default, so a second one is an error.
        let literal = fitting(&|member| self.literal_fits(&value, member, Fit::Purity));
        if literal.len() > candidates.len() {
            match literal.as_slice() {
                [first, second, ..] => {
                    return Err(Diagnostic::of(
                        DiagnosticKind::AnonymousUnionAmbiguousMember,
                        format!(
                            "this {} could be {} or {} in {}",
                            if matches!(value.kind, TypedExprKind::Array(_)) {
                                "array literal"
                            } else {
                                "value, which holds an array literal,"
                            },
                            member_display(&members[*first], &self.types),
                            member_display(&members[*second], &self.types),
                            union.display(&self.types)
                        ),
                        value.span,
                    )
                    .with_help("annotate the value, for example with a typed let binding"));
                }
                [case] if self.fixed_array_literal_member(&value, &members[*case]) => {
                    return self.fixed_array_value(value, &found, union, *case);
                }
                // The only member needs a fixed-length array inside the value, which was checked
                // as a dynamic array: the value is not a member as it is.
                _ => {}
            }
        }
        // A function that fits no member as it is may fit one through the adapter between a
        // parameter that is borrowed and one that is not, as it does where a function type is
        // expected.
        if candidates.is_empty() && self.read_conventions && matches!(found, Type::Function(..)) {
            candidates = fitting(&|member| self.member_fits(&found, member, Fit::Borrowed, false));
        }
        match candidates.as_slice() {
            [case] => {
                let span = value.span;
                let value =
                    self.finish_expression(value.kind, value.ty, Some(&members[*case]), span)?;
                Ok(wrap(value, union, *case))
            }
            [] => {
                let expected = union.display(&self.types);
                let open = self.holds_open_literal(&found);
                let found = self.literal_defaults(&found).display(&self.types);
                let help = members_help(members, &self.types);
                Err(Diagnostic::of(
                    DiagnosticKind::AnonymousUnionNotMember,
                    format!(
                        "expected {expected}, found {found}; {found} is not a member of the anonymous union"
                    ),
                    value.span,
                )
                .expected_found(expected, found)
                .with_help(if open {
                    format!(
                        "{help}; a number literal inside the value takes no type from the union, so add a suffix to it or annotate the value"
                    )
                } else {
                    help
                }))
            }
            [first, second, ..] => {
                let open = self.holds_open_literal(&found);
                Err(Diagnostic::of(
                    DiagnosticKind::AnonymousUnionAmbiguousMember,
                    format!(
                        "this {} could be {} or {} in {}",
                        if self.inference.is_numeric_literal(&found) {
                            "number".to_owned()
                        } else if open {
                            // The literals have no type yet, so the value has none to name.
                            "value".to_owned()
                        } else {
                            format!("value of type {}", found.display(&self.types))
                        },
                        member_display(&members[*first], &self.types),
                        member_display(&members[*second], &self.types),
                        union.display(&self.types)
                    ),
                    value.span,
                )
                .with_help(if open && !self.inference.is_numeric_literal(&found) {
                    "its number literals have no type yet: add a suffix to them, or annotate the value, for example with a typed let binding"
                } else {
                    "annotate the value, for example with a typed let binding"
                }))
            }
        }
    }

    /// `value`, of the anonymous union type `found`, as a value of the anonymous union `union`
    /// with another number of members (both resolved, A25): each member of `found` is the member
    /// of `union` that it equals, or else the only one that it is as unification reads the two
    /// (`agrees`), with which it is then unified. With such a member for each, the value is
    /// widened (`widen_union`: its tag is renumbered and its payload moves on, nothing is
    /// allocated). A member without one, and so any union of more members than `union`, is
    /// `anonymous_union_not_subset`; a member that agrees with two members of `union` is
    /// ambiguous, since no member is a default.
    ///
    /// The payload moves on as it is: nothing adapts it, as `finish_expression` adapts a value
    /// that is injected. A pure function member is therefore no member `A -> B` that is not
    /// pure, and a function reaches no member through the adapter of a borrowed parameter.
    fn widened(
        &mut self,
        mut value: TypedExpr,
        found: &Type,
        union: &Type,
    ) -> Result<TypedExpr, Diagnostic> {
        let (Type::Union(_, from), Type::Union(_, to)) = (found, union) else {
            unreachable!("anonymous unions are union types")
        };
        let mut cases = Vec::with_capacity(from.len());
        for member in from.iter() {
            let fitting: Vec<usize> = match to.iter().position(|other| other == member) {
                Some(case) => vec![case],
                None => to
                    .iter()
                    .enumerate()
                    .filter(|(_, other)| self.agrees(member, other))
                    .map(|(case, _)| case)
                    .collect(),
            };
            let ambiguous = |message: String| {
                Diagnostic::of(
                    DiagnosticKind::AnonymousUnionAmbiguousMember,
                    message,
                    value.span,
                )
                .with_help("annotate the value, for example with a typed let binding")
            };
            match fitting.as_slice() {
                [] => return Err(self.not_subset(member, found, union, value.span)),
                [case] => {
                    // Canonical members are pairwise distinct, so two of them are never the
                    // same member of `union`; if they were, the value would have no one reading.
                    if let Some(earlier) = cases.iter().position(|taken| taken == case) {
                        return Err(ambiguous(format!(
                            "the members {} and {} of {} could both be {} in {}",
                            member_display(&from[earlier], &self.types),
                            member_display(member, &self.types),
                            found.display(&self.types),
                            member_display(&to[*case], &self.types),
                            union.display(&self.types)
                        )));
                    }
                    self.same(member, &to[*case], value.span)?;
                    cases.push(*case);
                }
                [first, second, ..] => {
                    return Err(ambiguous(format!(
                        "the member {} of {} could be {} or {} in {}",
                        member_display(member, &self.types),
                        found.display(&self.types),
                        member_display(&to[*first], &self.types),
                        member_display(&to[*second], &self.types),
                        union.display(&self.types)
                    )));
                }
            }
        }
        // More members than `union` has: one of them found no member above.
        debug_assert!(from.len() < to.len());
        value.ty = self.inference.resolve(&value.ty);
        Ok(self.widen_union(value, union, &cases))
    }

    /// `E1003` for a value of the anonymous union `found` where `union` is expected and `member`
    /// of `found` is no member of `union`.
    fn not_subset(&self, member: &Type, found: &Type, union: &Type, span: Span) -> Diagnostic {
        let Type::Union(_, members) = union else {
            unreachable!("an anonymous union is a union type")
        };
        let expected = union.display(&self.types);
        let found = self.literal_defaults(found).display(&self.types);
        let member = member_display(&self.literal_defaults(member), &self.types);
        Diagnostic::of(
            DiagnosticKind::AnonymousUnionNotSubset,
            format!("expected {expected}, found {found}; {member} is not a member of {expected}"),
            span,
        )
        .with_help(format!(
            "{}; match the value with ':?' and pass on the members that {expected} has",
            members_help(members, &self.types)
        ))
        .expected_found(expected, found)
    }

    /// `value`, a reference to an anonymous union (`found`, resolved), where the anonymous union
    /// `union` is expected (A25). A reference to a union of as many members is left to the
    /// caller, which reborrows `ref U` for a read parameter and otherwise reports the mismatch.
    /// A reference to a union of another number of members is never widened, since that would
    /// move the value out of the borrow: it is a type mismatch that names the way to an owned
    /// value.
    fn borrowed_union(
        &self,
        value: TypedExpr,
        found: &Type,
        union: &Type,
    ) -> Result<TypedExpr, Diagnostic> {
        let (Type::Reference(inner, _), Type::Union(_, to)) = (found, union) else {
            unreachable!("a reference to an anonymous union, where one is expected")
        };
        if matches!(&**inner, Type::Union(_, from) if from.len() == to.len()) {
            return Ok(value);
        }
        let expected = union.display(&self.types);
        let found = found.display(&self.types);
        let mut error = Diagnostic::of(
            DiagnosticKind::TypeMismatch,
            format!("expected {expected}, found {found}"),
            value.span,
        )
        .expected_found(expected, found);
        match &value.kind {
            TypedExprKind::Local(id) if self.default_reads.contains_key(id) => {
                let (name, ty, pure) = &self.default_reads[id];
                read_only_hint(&mut error, name, ty, *pure);
            }
            _ => error = error.with_help("an anonymous union is widened from an owned value"),
        }
        Err(error)
    }

    /// Whether `ty` (resolved) holds the open type of a number literal.
    fn holds_open_literal(&self, ty: &Type) -> bool {
        let mut open = false;
        polymorph::map_type(ty, &mut |leaf| {
            open |= self.inference.is_numeric_literal(leaf);
            leaf.clone()
        });
        open
    }

    /// `ty` (resolved) as a diagnostic names it: the open type of each number literal in it is the
    /// type that the literal takes when nothing decides it (`Maybe<i32>` for `Some 4`).
    fn literal_defaults(&self, ty: &Type) -> Type {
        polymorph::map_type(ty, &mut |leaf| {
            self.inference
                .numeric_default(leaf)
                .unwrap_or_else(|| leaf.clone())
        })
    }

    /// Whether an array literal of `length` elements can be the member `member` by its form: a
    /// dynamic array, or a fixed-length array of that length (or of a length not yet known).
    fn takes_array_literal(&self, member: &Type, length: usize) -> bool {
        match member {
            Type::Array(_) => true,
            Type::FixedArray(_, expected) => match self.inference.head(expected) {
                Type::Length(expected) => *expected == length as u64,
                Type::Infer(_) => true,
                _ => false,
            },
            _ => false,
        }
    }

    /// Whether `value`, checked by itself, could be the member `member` if it were checked with
    /// that member as its expected type, as far as its array literals go: a literal is as much a
    /// `[T; N]` as a `[T]` (A16), and the expected type reaches the literals inside a literal, a
    /// tuple and, through unification, most other expressions. A literal that is the value, an
    /// element or a tuple component is compared by its length and its elements; a value of any
    /// other form that holds an array literal anywhere may have a fixed-length array wherever its
    /// type has a dynamic one. `fit` is how much the value itself is adapted (see `Fit`).
    ///
    /// The answer holds for every member that `member_fits` answers for, and for no other member unless
    /// the value holds an array literal. An inference variable stands for one type in the whole
    /// comparison, as it does in `member_fits` (`Ties`).
    fn literal_fits(&self, value: &TypedExpr, member: &Type, fit: Fit) -> bool {
        self.literal_fits_tied(value, member, fit, &Ties::default(), 0)
    }

    /// `literal_fits` inside one comparison, whose inference variables keep the types that `ties`
    /// gives them. `depth` is how many levels the comparison has descended (`tied_head`).
    fn literal_fits_tied(
        &self,
        value: &TypedExpr,
        member: &Type,
        fit: Fit,
        ties: &Ties,
        depth: usize,
    ) -> bool {
        let found = self.tied_head(&value.ty, ties, depth);
        let member = self.tied_head(member, ties, depth);
        let all = |elements: &[TypedExpr], element: &Type| {
            elements
                .iter()
                .all(|value| self.literal_fits_tied(value, element, Fit::Same, ties, depth + 1))
        };
        match (&value.kind, found.as_ref(), member.as_ref()) {
            (TypedExprKind::Array(elements), Type::Array(_), Type::FixedArray(element, _)) => {
                self.takes_array_literal(member.as_ref(), elements.len()) && all(elements, element)
            }
            (TypedExprKind::Array(elements), Type::Array(_), Type::Array(element)) => {
                all(elements, element)
            }
            (TypedExprKind::Tuple(elements), Type::Tuple(_), Type::Tuple(others))
                if elements.len() == others.len() =>
            {
                elements.iter().zip(others).all(|(value, other)| {
                    self.literal_fits_tied(value, other, Fit::Same, ties, depth + 1)
                })
            }
            (_, found, member) => {
                // The second reading starts from the ties made before the first one.
                let mark = ties.mark();
                self.member_fits_tied(found, member, fit, false, ties, depth) || {
                    ties.undo(mark);
                    self.member_fits_tied(found, member, fit, true, ties, depth)
                        && holds_array_literal(value)
                }
            }
        }
    }

    /// Whether the array literal `value`, checked by itself as a dynamic array, becomes the
    /// fixed-length array `member` with its elements as they are (`fixed_array_value`).
    fn fixed_array_literal_member(&self, value: &TypedExpr, member: &Type) -> bool {
        match (
            &value.kind,
            self.inference.head(&value.ty),
            self.inference.head(member),
        ) {
            (
                TypedExprKind::Array(elements),
                Type::Array(element),
                Type::FixedArray(expected, _),
            ) => {
                self.takes_array_literal(member, elements.len())
                    && self.member_fits(element, expected, Fit::Same, false)
            }
            _ => false,
        }
    }

    /// The array literal `value`, checked as the dynamic array `found`, as member `case` of
    /// `union`, a fixed-length array that `fixed_array_literal_member` accepts: the same elements
    /// in the same order, as `Checker::fixed_array_literal` builds them.
    fn fixed_array_value(
        &mut self,
        value: TypedExpr,
        found: &Type,
        union: &Type,
        case: usize,
    ) -> Result<TypedExpr, Diagnostic> {
        let (TypedExprKind::Array(elements), Type::Array(element), Type::Union(_, members)) =
            (value.kind, found, union)
        else {
            unreachable!("fixed_array_literal_member accepted an array literal")
        };
        let length = Type::Length(elements.len() as u64);
        let ty = Type::FixedArray(element.clone(), Box::new(length));
        let value = self.finish_expression(
            TypedExprKind::Tuple(elements),
            ty,
            Some(&members[case]),
            value.span,
        )?;
        validate_size(&value.ty, &self.types, value.span)?;
        Ok(wrap(value, union, case))
    }

    /// A number literal that was checked before the union was known (`1 |> f`), checked again
    /// as the member that `literal_member` picks, as `let v: U = 1` would check it. The literal's
    /// own open type is left to its default and serves nothing.
    fn retyped_literal(
        &mut self,
        value: &TypedExpr,
        union: &Type,
        members: &[Type],
    ) -> Result<Option<(usize, TypedExpr)>, Diagnostic> {
        if !self.inference.is_numeric_literal(&value.ty) {
            return Ok(None);
        }
        let span = value.span;
        let (class, text, negative, operand) = match &value.kind {
            TypedExprKind::GenericInteger(magnitude, negative) => (
                LiteralClass::Integer,
                format!("{}{magnitude}", if *negative { "-" } else { "" }),
                *negative,
                value,
            ),
            TypedExprKind::GenericFloat(text) => (LiteralClass::Float, text.clone(), false, value),
            TypedExprKind::Unary(UnaryOp::Negate, operand)
                if matches!(operand.kind, TypedExprKind::GenericFloat(_)) =>
            {
                let TypedExprKind::GenericFloat(text) = &operand.kind else {
                    unreachable!()
                };
                (LiteralClass::Float, format!("-{text}"), true, &**operand)
            }
            _ => return Ok(None),
        };
        let Some(case) = self.literal_member(class, &text, union, members, span)? else {
            return Ok(None);
        };
        let member = &members[case];
        let literal = match &operand.kind {
            TypedExprKind::GenericInteger(magnitude, _) => {
                let (kind, ty) = self.integer(*magnitude, None, Some(member), negative, span)?;
                TypedExpr { kind, ty, span }
            }
            TypedExprKind::GenericFloat(text) => {
                let float = TypedExpr {
                    kind: TypedExprKind::Float(crate::numeric::float_literal(text, member, span)?),
                    ty: member.clone(),
                    span: operand.span,
                };
                if negative {
                    TypedExpr {
                        kind: TypedExprKind::Unary(UnaryOp::Negate, Box::new(float)),
                        ty: member.clone(),
                        span,
                    }
                } else {
                    float
                }
            }
            _ => unreachable!("a number literal"),
        };
        Ok(Some((case, literal)))
    }

    /// `coerce_argument` for a parameter of type `U` or `ref U`: the value goes through
    /// `conform_union` (a piped value, which was checked before the parameter was known), which
    /// leaves the union itself and a reference to it as they are. An undetermined type that is
    /// no number literal becomes the union, which the reference then borrows.
    #[inline(never)]
    pub(super) fn conform_argument(
        &mut self,
        mut value: TypedExpr,
        expected: &Type,
    ) -> Result<TypedExpr, Diagnostic> {
        let union = match self.inference.resolve(expected) {
            Type::Reference(inner, _) => *inner,
            ty => ty,
        };
        let found = self.inference.resolve(&value.ty);
        match &found {
            Type::Error => Ok(value),
            Type::Infer(_) if !self.inference.is_numeric_literal(&found) => {
                self.same(&found, &union, value.span)?;
                value.ty = union;
                Ok(value)
            }
            _ => self.conform_union(value, &union, true),
        }
    }

    /// `apply_piped` for a parameter of type `U` or `ref U`: the piped value's type is unified
    /// before the call's arguments only when it is the union, a reference to it (the usual
    /// hint), or undetermined and no number literal (the union itself). Any other value is
    /// injected, and an anonymous union of another number of members widened (A25), by
    /// `coerce_argument` once the call is complete (`None` asks for the usual hint).
    #[inline(never)]
    pub(super) fn piped_into_union(
        &mut self,
        piped: &Type,
        parameter: &Type,
        span: Span,
    ) -> Option<Result<(), Diagnostic>> {
        let found = self.inference.resolve(piped);
        let union = match self.inference.resolve(parameter) {
            Type::Reference(inner, _) => *inner,
            ty => ty,
        };
        let Type::Union(_, members) = &union else {
            unreachable!("the parameter is an anonymous union or a reference to one")
        };
        // The union itself, or a reference to it, takes the usual hint (as does a union of as
        // many members, whose mismatch the hint reports); a union of another number of members
        // is widened or refused by `coerce_argument`.
        let other = |piped: &Type| match piped {
            Type::Union(id, from) if self.names.is_anonymous_union(*id) => {
                Some(from.len() != members.len())
            }
            _ => None,
        };
        match &found {
            Type::Infer(_) if !self.inference.is_numeric_literal(&found) => {
                Some(self.same(&found, &union, span))
            }
            Type::Reference(inner, _) => other(inner).is_none_or(|other| other).then_some(Ok(())),
            found => other(found).is_none_or(|other| other).then_some(Ok(())),
        }
    }

    /// Whether a value of type `found` may be the member `member` (D5): the types agree except
    /// where an inference variable stands, and the purity of the value's own function type is
    /// not compared, since `finish_expression` adapts it. See `member_fits`.
    pub(super) fn may_member(&self, found: &Type, member: &Type) -> bool {
        self.member_fits(found, member, Fit::Purity, false)
    }

    /// Whether the type `found` and the member `member` are one type except where an inference
    /// variable stands, as unification compares them: nothing is adapted.
    pub(super) fn agrees(&self, found: &Type, member: &Type) -> bool {
        self.member_fits(found, member, Fit::Same, false)
    }

    /// Whether the open type `variable` of a number literal can become `other`: the classes
    /// that the literal asks of its type (`Integer`, `SignedInteger`, `Float`) have an instance
    /// for it. An integer literal that was checked without an expected type is no floating-point
    /// number and no `BigInt`, and a floating-point literal is no integer.
    fn literal_takes(&self, variable: &Type, other: &Type) -> bool {
        if matches!(other, Type::Infer(_)) {
            return true;
        }
        let root = self.inference.head(variable);
        self.constraints
            .iter()
            .filter(|constraint| self.inference.head(&constraint.ty) == root)
            .all(|constraint| {
                let constraint = polymorph::Constraint {
                    class: constraint.class,
                    ty: other.clone(),
                    span: constraint.span,
                };
                self.classes.validate(&constraint, &self.types).is_ok()
            })
    }

    /// Whether a value of type `found` may be the member `member`: the types agree except where
    /// an inference variable stands, and the open type of a number literal agrees only with the
    /// types that the literal can take (`literal_takes`). Function types agree as `unify` reads
    /// them (their parameters, what is left of the longer chain against the other result, their
    /// purity; not their conventions), and a result that is still open may be a function of more
    /// parameters, so `string -> ?r` may be `string -> i64 -> i64`. Other class constraints are
    /// not consulted.
    ///
    /// `fit` is what `finish_expression` adapts in the value itself, never inside it (see `Fit`).
    /// With `literal`, a dynamic array of `found` may also be a fixed-length array of `member`,
    /// as an array literal is.
    ///
    /// An inference variable stands for one type in the whole comparison, as unification makes it:
    /// `(Maybe<?a> * Maybe<?a>)` may be `(Maybe<i64> * Maybe<i64>)` but not
    /// `(Maybe<i64> * Maybe<string>)`. The comparison records what each variable has met (`Ties`)
    /// and leaves the inference state as it was. It does so within the limits of `Ties`: a variable
    /// is tied only to a type that `polymorph::bounded_type` accepts, and only while the comparison
    /// is no deeper than two such types reach (`TIE_DEPTH`) and has steps left (`TIE_STEPS`); past
    /// them, its variables may be anything again.
    ///
    /// For a member without inference variables the answer is what unification answers once the
    /// value is adapted, up to class constraints and those limits: it misses no member that an
    /// annotation with that member accepts for a value checked by itself, and it adds none.
    fn member_fits(&self, found: &Type, member: &Type, fit: Fit, literal: bool) -> bool {
        self.member_fits_tied(found, member, fit, literal, &Ties::default(), 0)
    }

    /// `member_fits` inside one comparison, whose inference variables keep the types that `ties`
    /// gives them. `depth` is how many levels the comparison has descended: each level descends
    /// into one of the two types at least (`tied_head`).
    fn member_fits_tied(
        &self,
        found: &Type,
        member: &Type,
        fit: Fit,
        literal: bool,
        ties: &Ties,
        depth: usize,
    ) -> bool {
        let found = self.tied_head(found, ties, depth);
        let member = self.tied_head(member, ties, depth);
        let (found, member) = (found.as_ref(), member.as_ref());
        if found == member {
            return true;
        }
        let same = |left: &Type, right: &Type| {
            self.member_fits_tied(left, right, Fit::Same, literal, ties, depth + 1)
        };
        let all = |left: &[Type], right: &[Type]| {
            left.len() == right.len()
                && left
                    .iter()
                    .zip(right)
                    .all(|(left, right)| same(left, right))
        };
        match (found, member) {
            (Type::Error, _) | (_, Type::Error) => true,
            (Type::Infer(_), other) | (other, Type::Infer(_)) => {
                let variable = if matches!(found, Type::Infer(_)) {
                    found
                } else {
                    member
                };
                if self.inference.is_numeric_literal(variable)
                    && !self.literal_takes(variable, other)
                {
                    return false;
                }
                self.tie(variable, other, ties, depth);
                true
            }
            (Type::Application(..) | Type::Partial(_), _)
            | (_, Type::Application(..) | Type::Partial(_)) => true,
            (Type::Record(id, arguments), Type::Record(other, other_arguments))
            | (Type::Union(id, arguments), Type::Union(other, other_arguments)) => {
                id == other && all(arguments, other_arguments)
            }
            (Type::Array(element), Type::Array(other))
            | (Type::ArrayView(element), Type::ArrayView(other))
            | (Type::List(element), Type::List(other))
            | (Type::Vec(element), Type::Vec(other))
            | (Type::Once(element), Type::Once(other)) => same(element, other),
            (Type::Array(element), Type::FixedArray(other, _)) if literal => same(element, other),
            (Type::FixedArray(element, length), Type::FixedArray(other, other_length)) => {
                same(element, other) && same(length, other_length)
            }
            (Type::Tuple(elements), Type::Tuple(others)) => all(elements, others),
            (Type::Function(parameters, result), Type::Function(others, other_result)) => {
                // The value's purity once it is adapted: a pure function is cast to the type
                // that is not pure, and a closed function takes a pure type.
                let pure = if fit == Fit::Same {
                    parameters.pure
                } else {
                    others.pure
                };
                // One side borrows a parameter with `ref` that the other does not: the adapter
                // of `finish_expression` (`callback_adapter`) is between two function types of
                // as many parameters.
                let borrowed = fit == Fit::Borrowed && parameters.len() == others.len();
                let parameter = |left: &Type, right: &Type| {
                    if borrowed {
                        match (
                            self.tied_head(left, ties, depth + 1).as_ref(),
                            self.tied_head(right, ties, depth + 1).as_ref(),
                        ) {
                            (Type::Reference(inner, false), other)
                                if !matches!(other, Type::Reference(..)) =>
                            {
                                return same(inner, other);
                            }
                            (other, Type::Reference(inner, false))
                                if !matches!(other, Type::Reference(..)) =>
                            {
                                return same(other, inner);
                            }
                            _ => {}
                        }
                    }
                    same(left, right)
                };
                let common = parameters.len().min(others.len());
                // As `unify` reads two function types: the parameters that both have, then what
                // is left of each. Only two pure functions have their parameter counts compared.
                parameters.is_empty() == others.is_empty()
                    && pure == others.pure
                    && (!pure || parameters.len() == others.len())
                    && parameters
                        .iter()
                        .zip(others.iter())
                        .all(|(left, right)| parameter(left, right))
                    && match (parameters.len() > common, others.len() > common) {
                        (false, false) => same(result, other_result),
                        // An open result may be the function of the parameters that are left.
                        (false, true) => same(
                            result,
                            &Type::function(others[common..].to_vec(), (**other_result).clone()),
                        ),
                        (true, _) => same(
                            &Type::function(parameters[common..].to_vec(), (**result).clone()),
                            other_result,
                        ),
                    }
            }
            (Type::Reference(target, mutable), Type::Reference(other, other_mutable)) => {
                mutable == other_mutable && same(target, other)
            }
            (Type::Shared(value, kind), Type::Shared(other, other_kind)) => {
                kind == other_kind && same(value, other)
            }
            _ => false,
        }
    }

    /// `ty` with the solved inference variables at its top followed, then the variables that the
    /// comparison has tied to a type (`ties`), and so on until neither applies. `depth` is how many
    /// levels the comparison has descended to reach `ty`: a tie is followed only within
    /// `TIE_DEPTH` levels and while the comparison has steps left (`Ties::follow`). Past either, a
    /// tied variable is open again and may be anything, which only counts more members.
    fn tied_head<'t>(&'t self, ty: &'t Type, ties: &Ties, depth: usize) -> Head<'t> {
        let mut head = Head::Part(self.inference.head(ty));
        while depth <= TIE_DEPTH
            && let Type::Infer(id) = head.as_ref()
            && let Some(tied) = ties.follow(*id)
        {
            // The inference state does not change during a comparison, so the type that a
            // variable is tied to, a head when it was tied, is still one.
            debug_assert!(std::ptr::eq(self.inference.head(&tied), &*tied));
            head = Head::Tied(tied);
        }
        head
    }

    /// Ties the open inference variable `variable` to `other` for the rest of one comparison, as
    /// unification would solve it. Of two open variables, the open type of a number literal stays
    /// the one that is not tied, so that its classes still decide what it meets later. Where
    /// unification would not solve the variable (against the target of an exclusive slice, which
    /// it leaves open, or a type that `solvable` refuses, which it rejects), nothing is tied: the
    /// variable may still be anything, which only counts more members. Nor is anything tied past
    /// the levels and the steps within which `tied_head` follows ties (`depth`).
    fn tie(&self, variable: &Type, other: &Type, ties: &Ties, depth: usize) {
        if depth > TIE_DEPTH {
            return;
        }
        let (variable, other) = match other {
            Type::Infer(_)
                if self.inference.is_numeric_literal(variable)
                    && !self.inference.is_numeric_literal(other) =>
            {
                (other, variable)
            }
            _ => (variable, other),
        };
        let Type::Infer(id) = variable else {
            unreachable!("only an open inference variable is tied")
        };
        if matches!(other, Type::ArrayView(_)) || !self.solvable(other, *id, ties, 0, &mut 0) {
            return;
        }
        ties.tie(*id, Rc::new(other.clone()));
    }

    /// Whether unification could solve the open inference variable `root` as `ty`, with the solved
    /// and the tied variables in `ty` followed: `root` does not occur in it (`unify` rejects that
    /// as an infinite type, `E1015`) and `polymorph::bounded_type` accepts it (`E1017`): it nests at
    /// most `MAX_NESTING` levels and holds at most `polymorph::MAX_TYPE_SIZE` type constructors,
    /// counted as `type_size` counts them. `depth` is the nesting of `ty` and `count` the
    /// constructors counted so far. The walk copies nothing and stops at the limit, so that
    /// variables that tie each other back and forth, whose types double with every tie, cost no
    /// more than the limit. Each constructor visited takes a step of the comparison, and a walk
    /// that runs out of steps answers no.
    fn solvable(
        &self,
        ty: &Type,
        root: usize,
        ties: &Ties,
        depth: usize,
        count: &mut usize,
    ) -> bool {
        let ty = self.tied_head(ty, ties, 0);
        *count += 1;
        if depth > MAX_NESTING || *count > polymorph::MAX_TYPE_SIZE || !ties.step() {
            return false;
        }
        let mut inner = |ty: &Type| self.solvable(ty, root, ties, depth + 1, count);
        match ty.as_ref() {
            Type::Infer(id) => *id != root,
            Type::Partial(partial) => partial.trailing.iter().all(inner),
            Type::Application(head, arguments) => inner(head) && arguments.iter().all(inner),
            Type::Array(element)
            | Type::ArrayView(element)
            | Type::List(element)
            | Type::Vec(element)
            | Type::Once(element)
            | Type::Shared(element, _)
            | Type::Reference(element, _) => inner(element),
            Type::Function(parameters, result) => {
                parameters.iter().all(&mut inner) && inner(result)
            }
            Type::Tuple(elements) => elements.iter().all(inner),
            Type::FixedArray(element, length) => inner(element) && inner(length),
            Type::Record(_, arguments) | Type::Union(_, arguments) => arguments.iter().all(inner),
            _ => true,
        }
    }
}

/// The levels that one comparison of `Checker::member_fits_tied` descends within which it
/// follows and makes ties (`Checker::tied_head`). Each level descends into one of the two compared
/// types at least, so two types that `polymorph::bounded_type` accepts (a nesting of at most
/// `MAX_NESTING`) are compared within it. A value is a member without `E1017` only if its type,
/// once unification has made it the member, is such a type (every expression's type is bounded
/// when its function has been checked), so past the limit the comparison only counts more of the
/// members that would end in `E1017`. The limit keeps the recursion as shallow as the comparison
/// of two such types.
const TIE_DEPTH: usize = 2 * MAX_NESTING;

/// The steps through ties that one comparison takes at most (`Ties::step`). A comparison of types
/// that share no inference variables takes none; one that ties takes one step for each tied
/// variable that it follows and for each constructor of the types that it ties variables to,
/// which `polymorph::MAX_TYPE_SIZE` bounds for each tie. Variables that tie each other back and
/// forth can make the types that the comparison follows double with every tie; the steps keep such
/// a comparison from taking time exponential in the number of variables.
const TIE_STEPS: usize = 1 << 16;

/// A type that `Checker::tied_head` reached: a part of the compared types or of the inference
/// state, or the type that a variable is tied to.
enum Head<'t> {
    Part(&'t Type),
    Tied(Rc<Type>),
}

impl AsRef<Type> for Head<'_> {
    fn as_ref(&self) -> &Type {
        match self {
            Head::Part(ty) => ty,
            Head::Tied(ty) => ty,
        }
    }
}

/// The inference variables that one comparison of `Checker::member_fits_tied` has tied to a type:
/// the type that each first met. A variable that occurs again must be the same type there, as
/// unification makes it. The ties belong to the comparison of one member with one value and never
/// reach the inference state: `member_fits` and `literal_fits` start each comparison without any,
/// and a reading that fails is undone (`mark` and `undo`).
///
/// The comparison follows and makes ties for at most `TIE_STEPS` steps (`step`); once they are
/// spent, every variable may be anything again, as each variable was before the comparison kept
/// ties, which only counts more members.
#[derive(Default)]
struct Ties {
    types: std::cell::RefCell<BTreeMap<usize, Rc<Type>>>,
    /// The variables in the order that they were tied, to undo the ties after a mark.
    order: std::cell::RefCell<Vec<usize>>,
    /// The steps that the comparison has taken. `undo` gives none back.
    steps: std::cell::Cell<usize>,
}

impl Ties {
    /// The type that `id` is tied to, which takes a step; `None` when `id` is not tied or no steps
    /// are left.
    fn follow(&self, id: usize) -> Option<Rc<Type>> {
        let tied = self.types.borrow().get(&id).cloned()?;
        self.step().then_some(tied)
    }

    /// Takes one of the comparison's steps through ties, if one is left.
    fn step(&self) -> bool {
        let steps = self.steps.get();
        if steps >= TIE_STEPS {
            return false;
        }
        self.steps.set(steps + 1);
        true
    }

    fn tie(&self, id: usize, ty: Rc<Type>) {
        self.types.borrow_mut().insert(id, ty);
        self.order.borrow_mut().push(id);
    }

    fn mark(&self) -> usize {
        self.order.borrow().len()
    }

    fn undo(&self, mark: usize) {
        let mut order = self.order.borrow_mut();
        let mut types = self.types.borrow_mut();
        for id in order.drain(mark..) {
            types.remove(&id);
        }
    }
}

/// What `Checker::member_fits` lets `finish_expression` adapt in a value whose type is compared with a
/// member. Only the value itself is adapted: the types inside it are compared as they are.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Fit {
    /// Nothing, as unification compares two types.
    Same,
    /// The purity of a function: a pure function is also a value of the function type that is
    /// not pure, and a closed function that is not pure takes a pure type of its parameters.
    Purity,
    /// The purity, and a parameter that only one of the two function types borrows with `ref`:
    /// a function that reads its parameter where the member takes the value, or the reverse.
    Borrowed,
}

/// Whether `value` holds an array literal that was checked as a dynamic array, in any of its
/// subexpressions. The literal that `new` allocates is always a dynamic array; its elements count.
fn holds_array_literal(value: &TypedExpr) -> bool {
    match &value.kind {
        TypedExprKind::Array(_) => true,
        TypedExprKind::NewLiteral(literal) => {
            literal.children().into_iter().any(holds_array_literal)
        }
        _ => value.children().into_iter().any(holds_array_literal),
    }
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

    /// Declaration names for `display` without a checked program: `Main.Maybe` is union 0.
    struct Fixed;

    impl DeclarationNames for Fixed {
        fn record_name(&self, _: usize) -> &str {
            "Main.Cell"
        }

        fn union_name(&self, _: usize) -> &str {
            "Main.Maybe"
        }
    }

    /// D7: a witness names the member that it misses, `:? T`, only when the member type is known.
    /// Otherwise the message reads `:? _`, which no arm accepts, so no fix is offered. A type test
    /// needs the scrutinee type when it is checked, so a source cannot reach that branch through
    /// `check_coverage` today: this test pins the rule itself.
    #[test]
    fn unresolved_member_offers_no_fix() {
        let text = |member: Option<&Type>| type_test_text(member, &Fixed);
        assert_eq!(text(Some(&Type::I64)).as_deref(), Some(":? i64"));
        assert_eq!(
            text(Some(&Type::Union(0, vec![Type::String].into()))).as_deref(),
            Some(":? Main.Maybe<string>")
        );
        // `type_product` reads the type, so an impure function is parenthesized.
        let function = Type::function(vec![Type::I64], Type::I64);
        assert_eq!(text(Some(&function)).as_deref(), Some(":? (i64 -> i64)"));
        // A type variable of the signature can be written in the body.
        let generic = Type::Union(0, vec![Type::Variable("a".into())].into());
        assert_eq!(text(Some(&generic)).as_deref(), Some(":? Main.Maybe<'a>"));
        for unknown in [
            None,
            Some(Type::Infer(0)),
            Some(Type::Error),
            Some(Type::Union(0, vec![Type::Infer(3)].into())),
            Some(Type::Array(Box::new(Type::Error))),
            Some(Type::function(vec![Type::Infer(1)], Type::I64)),
            Some(Type::Tuple(vec![Type::I64, Type::Infer(2)])),
        ] {
            assert_eq!(text(unknown.as_ref()), None, "{unknown:?}");
        }
    }
}
