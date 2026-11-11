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
/// the type arguments. `Purity` means that the types differ only in the purity of a function type,
/// which picks no member when a value is injected (D5). `Currying` means that they differ only in
/// how a pure function type is curried (`pure (A -> (B -> C))` and `pure (A -> B -> C)`), which
/// the text of a type does not show, so a diagnostic could not name one of the two members.
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
/// order. Function types compare as `functions` says; their parameter conventions are not compared,
/// as unification ignores them.
///
/// A decided answer (`Less` or `Greater`) holds for every substitution of the type variables: it
/// is found at a place that no variable reaches, and `Equal` is only answered for types without
/// variables. A generic declaration therefore keeps its members in the canonical order, and
/// pairwise distinct, in every instance.
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
                Order::Less | Order::Greater if parameters.pure != others.pure => {
                    Order::of(parameters.pure.cmp(&others.pure))
                }
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
    left.chain
        .iter()
        .zip(&right.chain)
        .map(|(left, right)| order(left, right, names))
        .find(|order| *order != Order::Equal)
        .unwrap_or(Order::Equal)
        .then(|| match left.chain.len().cmp(&right.chain.len()) {
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
        let union = self.inference.resolve(expected);
        let value = self.union_value(expression, &union)?;
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
            self.union_value(expression, &union)?
        };
        self.coerce_argument(value, &target)
    }

    /// The value of `expression` where the anonymous union `union` (resolved) is expected (D4
    /// rules 2 and 3): a member that the expression's form picks is its expected type, otherwise
    /// the expression is checked by itself and `conform_union` decides.
    fn union_value(&mut self, expression: &Expr, union: &Type) -> Result<TypedExpr, Diagnostic> {
        let Type::Union(_, members) = union else {
            unreachable!("an anonymous union is a union type")
        };
        if let Some(case) = self.preselect(expression, union, members)? {
            let value = self.expression(expression, Some(&members[case]))?;
            return Ok(wrap(value, union, case));
        }
        let value = self.expression(expression, None)?;
        self.conform_union(value, union)
    }

    /// The member that the form of `expression` picks (D4 rule 2): an unsuffixed number literal
    /// takes the only integer member, else the bigint member, else the only floating-point member
    /// (a float literal only the last); a lambda the only function member; `Dyn.of v` the only dyn
    /// member; an array literal the only array or fixed-length array member, else the only one of
    /// them that takes a literal of its length; a list literal the only list member; a tuple the
    /// only tuple member of its length. `None` when nothing or more than one member fits the
    /// form, except that a number literal that fits two members is an error. An array literal
    /// that several members take is checked by itself, and `array_literal_member` decides.
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
    /// (resolved) is expected (D4 rule 3). An anonymous union, a reference to one, an erroneous
    /// value and a value of an undetermined type that is no number literal are not wrapped: the
    /// caller unifies them with the union or reborrows the reference. Any other value is wrapped
    /// as the only member that its type may be (`may_member`).
    #[inline(never)]
    pub(super) fn conform_union(
        &mut self,
        mut value: TypedExpr,
        union: &Type,
    ) -> Result<TypedExpr, Diagnostic> {
        let Type::Union(_, members) = union else {
            unreachable!("an anonymous union is a union type")
        };
        let found = self.inference.resolve(&value.ty);
        match &found {
            Type::Error => return Ok(value),
            Type::Union(id, _) if self.names.is_anonymous_union(*id) => return Ok(value),
            Type::Reference(inner, _) if self.anonymous_union_head(inner) => return Ok(value),
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
        if let Some(case) = self.array_literal_member(&value, &found, union, members)? {
            return self.fixed_array_value(value, &found, union, case);
        }
        // A member is never a reference: a borrowed value is read as the value it borrows, which
        // the union then owns (a move out of the borrow, as in `Some s`, D10).
        let value = if matches!(found, Type::Reference(..)) {
            Self::autoderef(value)
        } else {
            value
        };
        let found = self.inference.resolve(&value.ty);
        let candidates: Vec<usize> = members
            .iter()
            .enumerate()
            .filter(|(_, member)| self.may_member(&found, member))
            .map(|(case, _)| case)
            .collect();
        match candidates.as_slice() {
            [case] => {
                let span = value.span;
                let value =
                    self.finish_expression(value.kind, value.ty, Some(&members[*case]), span)?;
                Ok(wrap(value, union, *case))
            }
            [] => {
                let expected = union.display(&self.types);
                let found = self.literal_defaults(&found).display(&self.types);
                Err(Diagnostic::of(
                    DiagnosticKind::AnonymousUnionNotMember,
                    format!(
                        "expected {expected}, found {found}; {found} is not a member of the anonymous union"
                    ),
                    value.span,
                )
                .expected_found(expected, found)
                .with_help(members_help(members, &self.types)))
            }
            [first, second, ..] => Err(Diagnostic::of(
                DiagnosticKind::AnonymousUnionAmbiguousMember,
                format!(
                    "this {} could be {} or {} in {}",
                    if self.inference.is_numeric_literal(&found) {
                        "number".to_owned()
                    } else {
                        format!(
                            "value of type {}",
                            self.literal_defaults(&found).display(&self.types)
                        )
                    },
                    member_display(&members[*first], &self.types),
                    member_display(&members[*second], &self.types),
                    union.display(&self.types)
                ),
                value.span,
            )
            .with_help("annotate the value, for example with a typed let binding")),
        }
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

    /// The fixed-length array member that the array literal `value` is, when `value` was checked
    /// by itself (as the dynamic array `found`) because several members take an array literal.
    /// The literal is as much a `[T; N]` as a `[T]` (A16), so its length and its element type
    /// decide: when a fixed-length member and any other member fit, no member is a default and
    /// the literal is `anonymous_union_ambiguous_member`. `None` when no fixed-length member fits
    /// or `value` is no array literal: the value is then the dynamic array that it was checked as.
    fn array_literal_member(
        &self,
        value: &TypedExpr,
        found: &Type,
        union: &Type,
        members: &[Type],
    ) -> Result<Option<usize>, Diagnostic> {
        let (TypedExprKind::Array(elements), Type::Array(element)) = (&value.kind, found) else {
            return Ok(None);
        };
        let fitting: Vec<(usize, bool)> = members
            .iter()
            .enumerate()
            .filter_map(|(case, member)| match member {
                Type::Array(_) => self.may_member(found, member).then_some((case, false)),
                Type::FixedArray(expected, _) => (self.takes_array_literal(member, elements.len())
                    && self.may_member(element, expected))
                .then_some((case, true)),
                _ => None,
            })
            .collect();
        match fitting.as_slice() {
            [(case, true)] => Ok(Some(*case)),
            [(first, _), (second, _), ..] if fitting.iter().any(|(_, fixed)| *fixed) => {
                Err(Diagnostic::of(
                    DiagnosticKind::AnonymousUnionAmbiguousMember,
                    format!(
                        "this array literal could be {} or {} in {}",
                        member_display(&members[*first], &self.types),
                        member_display(&members[*second], &self.types),
                        union.display(&self.types)
                    ),
                    value.span,
                )
                .with_help("annotate the value, for example with a typed let binding"))
            }
            _ => Ok(None),
        }
    }

    /// The array literal `value`, checked as the dynamic array `found`, as member `case` of
    /// `union`, a fixed-length array: the same elements in the same order, as
    /// `Checker::fixed_array_literal` builds them.
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
            unreachable!("array_literal_member picked a member for an array literal")
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

    /// `coerce_argument` for a parameter of type `U` or `ref U`: a value that is not already the
    /// union, a reference to it, or of an undetermined type goes through `conform_union` (a
    /// piped value, which was checked before the parameter was known). An undetermined type that
    /// is no number literal becomes the union, which the reference then borrows.
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
            Type::Union(id, _) if self.names.is_anonymous_union(*id) => Ok(value),
            Type::Reference(inner, _) if self.anonymous_union_head(inner) => Ok(value),
            Type::Infer(_) if !self.inference.is_numeric_literal(&found) => {
                self.same(&found, &union, value.span)?;
                value.ty = union;
                Ok(value)
            }
            _ => self.conform_union(value, &union),
        }
    }

    /// `apply_piped` for a parameter of type `U` or `ref U`: the piped value's type is unified
    /// before the call's arguments only when it is the union, a reference to it (the usual
    /// hint), or undetermined and no number literal (the union itself). Any other value is
    /// injected by `coerce_argument` once the call is complete (`None` asks for the usual hint).
    #[inline(never)]
    pub(super) fn piped_into_union(
        &mut self,
        piped: &Type,
        parameter: &Type,
        span: Span,
    ) -> Option<Result<(), Diagnostic>> {
        let found = self.inference.resolve(piped);
        match &found {
            Type::Union(id, _) if self.names.is_anonymous_union(*id) => None,
            Type::Reference(inner, _) if self.anonymous_union_head(inner) => None,
            Type::Infer(_) if !self.inference.is_numeric_literal(&found) => {
                let union = match self.inference.resolve(parameter) {
                    Type::Reference(inner, _) => *inner,
                    ty => ty,
                };
                Some(self.same(&found, &union, span))
            }
            _ => Some(Ok(())),
        }
    }

    /// Whether a value of type `found` may be the member `member` (D5): the types agree except
    /// where an inference variable stands, and the open type of a number literal agrees only with
    /// number types. Function types agree on their parameters and result, not on conventions or
    /// purity, which `finish_expression` adapts; a result that is still open may be a function of
    /// more parameters, so `string -> ?r` may be `string -> i64 -> i64`. Class constraints are
    /// not consulted. The answer never misses a member that unification would accept.
    pub(super) fn may_member(&self, found: &Type, member: &Type) -> bool {
        let found = self.inference.head(found);
        let member = self.inference.head(member);
        if found == member {
            return true;
        }
        let all = |left: &[Type], right: &[Type]| {
            left.len() == right.len()
                && left
                    .iter()
                    .zip(right)
                    .all(|(left, right)| self.may_member(left, right))
        };
        match (found, member) {
            (Type::Error, _) | (_, Type::Error) => true,
            (Type::Infer(_), other) | (other, Type::Infer(_)) => {
                let variable = if matches!(found, Type::Infer(_)) {
                    found
                } else {
                    member
                };
                !self.inference.is_numeric_literal(variable)
                    || matches!(
                        other,
                        Type::Integer(..) | Type::Binary(_) | Type::Decimal(_) | Type::Infer(_)
                    )
                    || self.is_bigint(other)
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
            | (Type::Once(element), Type::Once(other)) => self.may_member(element, other),
            (Type::FixedArray(element, length), Type::FixedArray(other, other_length)) => {
                self.may_member(element, other) && self.may_member(length, other_length)
            }
            (Type::Tuple(elements), Type::Tuple(others)) => all(elements, others),
            (Type::Function(parameters, result), Type::Function(others, other_result)) => {
                let common = parameters.len().min(others.len());
                // As `unify` reads two function types once `finish_expression` has adapted the
                // value's purity: the parameters that both have, then what is left of each. Only
                // two pure functions have their parameter counts compared.
                parameters.is_empty() == others.is_empty()
                    && (parameters.len() == others.len() || !(parameters.pure && others.pure))
                    && parameters
                        .iter()
                        .zip(others.iter())
                        .all(|(left, right)| self.may_member(left, right))
                    && match (parameters.len() > common, others.len() > common) {
                        (false, false) => self.may_member(result, other_result),
                        // An open result may be the function of the parameters that are left.
                        (false, true) => self.may_member(
                            result,
                            &Type::function(others[common..].to_vec(), (**other_result).clone()),
                        ),
                        (true, _) => self.may_member(
                            &Type::function(parameters[common..].to_vec(), (**result).clone()),
                            other_result,
                        ),
                    }
            }
            (Type::Reference(target, mutable), Type::Reference(other, other_mutable)) => {
                mutable == other_mutable && self.may_member(target, other)
            }
            (Type::Shared(value, kind), Type::Shared(other, other_kind)) => {
                kind == other_kind && self.may_member(value, other)
            }
            _ => false,
        }
    }
}

// The built-in instances of anonymous unions (A26). Their methods are ordinary typed IR that the
// specializer builds (D6): they test the tag as a case pattern does, and pass a borrow of the
// payload of member k to the method of member k. Code generation needs nothing new.

fn typed(kind: TypedExprKind, ty: Type, span: Span) -> TypedExpr {
    TypedExpr { kind, ty, span }
}

/// The anonymous union behind `reference`, a parameter of type `ref U`: a place, never a copy.
fn behind(reference: &TypedExpr) -> TypedExpr {
    let Type::Reference(union, _) = &reference.ty else {
        unreachable!("the methods of the built-in instances take their receivers by reference")
    };
    typed(
        TypedExprKind::Dereference(Box::new(reference.clone())),
        (**union).clone(),
        reference.span,
    )
}

/// The `i32` tag of the anonymous union behind `reference`: the canonical position of its member.
pub(super) fn tag_behind(reference: &TypedExpr) -> TypedExpr {
    typed(
        TypedExprKind::UnionTag(Box::new(behind(reference))),
        Type::Integer(32, true),
        reference.span,
    )
}

/// A read-only borrow of the payload of member `case`, of type `member`, of the anonymous union
/// behind `reference`. Only the arm of `by_member` for `case` may evaluate it.
fn payload_behind(reference: &TypedExpr, member: &Type, case: usize) -> TypedExpr {
    let span = reference.span;
    let payload = typed(
        TypedExprKind::UnionPayload {
            value: Box::new(behind(reference)),
            case_id: case,
        },
        member.clone(),
        span,
    );
    typed(
        TypedExprKind::Borrow(Box::new(payload), false),
        Type::Reference(Box::new(member.clone()), false),
        span,
    )
}

/// `function` of member `case` applied to the payloads of that member behind each of `references`.
pub(super) fn member_call(
    function: TypedExpr,
    references: &[TypedExpr],
    member: &Type,
    case: usize,
) -> TypedExpr {
    let Type::Function(_, result) = &function.ty else {
        unreachable!("a method of a member is a function")
    };
    let (result, span) = ((**result).clone(), function.span);
    let arguments = references
        .iter()
        .map(|reference| payload_behind(reference, member, case))
        .collect();
    typed(
        TypedExprKind::Call(Box::new(function), arguments),
        result,
        span,
    )
}

/// `if tag == 0 then arm 0 elif tag == 1 then arm 1 ... else arm n-1` for the tag of the
/// anonymous union behind `reference`, whose members are `members`: the arm of the member that the
/// value holds. The last member needs no test, since a tag is always the position of a member.
pub(super) fn by_member(
    reference: &TypedExpr,
    members: &[Type],
    mut arm: impl FnMut(usize, &Type) -> TypedExpr,
) -> TypedExpr {
    let span = reference.span;
    let mut chain: Option<TypedExpr> = None;
    for (case, member) in members.iter().enumerate().rev() {
        let body = arm(case, member);
        chain = Some(match chain {
            None => body,
            Some(rest) => {
                let tag = Type::Integer(32, true);
                let test = typed(
                    TypedExprKind::Binary(
                        BinaryOp::Equal,
                        Box::new(tag_behind(reference)),
                        Box::new(typed(TypedExprKind::Int(case as u128), tag, span)),
                    ),
                    Type::Bool,
                    span,
                );
                let ty = body.ty.clone();
                typed(
                    TypedExprKind::If {
                        condition: Box::new(test),
                        then_branch: Box::new(body),
                        else_branch: Box::new(rest),
                    },
                    ty,
                    span,
                )
            }
        });
    }
    chain.expect("an anonymous union has at least two members")
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
