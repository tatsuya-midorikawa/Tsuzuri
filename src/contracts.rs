//! Contracts (A21): the `@requires` and `@ensures` lines that start a function body. The checker
//! weaves them into the typed body as calls of the built-in functions `$requires` and `$ensures`
//! around the original body, using only `Block`, `Call` and `Local`, so every pass handles them
//! with its usual rules. `ContractMode::Off` strips them again right before specialization, so
//! that the emitted IR is exactly the IR of the function without the lines.

use super::*;

/// How a command uses the contract lines.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ContractMode {
    /// Type-check and purity-check the lines only; emit nothing for them.
    Off,
    /// Check `@requires` on entry and `@ensures` on exit, trapping when one is false.
    Check,
    /// `Check`, and `tsuzuri test` generates property tests from the contracts.
    Test,
    /// Verify the contracts with an SMT solver first, then continue as `Check`.
    Prove,
}

impl ContractMode {
    pub const ALL: [ContractMode; 4] = [
        ContractMode::Off,
        ContractMode::Check,
        ContractMode::Test,
        ContractMode::Prove,
    ];

    pub fn name(self) -> &'static str {
        match self {
            ContractMode::Off => "off",
            ContractMode::Check => "check",
            ContractMode::Test => "test",
            ContractMode::Prove => "prove",
        }
    }

    pub fn from_name(name: &str) -> Option<ContractMode> {
        ContractMode::ALL
            .into_iter()
            .find(|mode| mode.name() == name)
    }

    /// Whether the woven checks stay in the emitted code.
    pub fn emits_checks(self) -> bool {
        self != ContractMode::Off
    }
}

/// How a function's woven body is laid out: the counts of its contract lines and of the
/// `@old` values that it binds on entry.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ContractShape {
    pub requires: u8,
    pub olds: u8,
    pub ensures: u8,
}

impl ContractShape {
    pub fn is_empty(self) -> bool {
        self == ContractShape::default()
    }
}

/// Replaces a woven body by the original body (`ContractMode::Off`). Idempotent.
pub(crate) fn strip(function: &mut CheckedFunction) {
    let shape = function.contracts;
    if shape.is_empty() {
        return;
    }
    let body = std::mem::replace(&mut function.body, TypedExpr::error(function.span));
    let TypedExprKind::Block {
        mut bindings,
        result,
    } = body.kind
    else {
        unreachable!("a woven contract body is a block")
    };
    function.body = if shape.ensures == 0 {
        *result
    } else {
        bindings
            .swap_remove(usize::from(shape.requires) + usize::from(shape.olds))
            .1
    };
    function.contracts = ContractShape::default();
}

/// The condition of each contract line of a woven body, with the span of its line, and the
/// operand of each `@old`, which the woven body evaluates on entry, with its own span.
pub(crate) fn conditions(function: &CheckedFunction) -> Vec<(&TypedExpr, Span)> {
    let shape = function.contracts;
    if shape.is_empty() {
        return Vec::new();
    }
    let TypedExprKind::Block { bindings, .. } = &function.body.kind else {
        return Vec::new();
    };
    let requires = usize::from(shape.requires);
    let ensures_from = requires + usize::from(shape.olds) + usize::from(shape.ensures > 0);
    fn condition(value: &TypedExpr) -> Option<(&TypedExpr, Span)> {
        match &value.kind {
            TypedExprKind::Call(_, arguments) => arguments.first().map(|c| (c, value.span)),
            _ => None,
        }
    }
    let mut found: Vec<(&TypedExpr, Span)> = bindings[..requires.min(bindings.len())]
        .iter()
        .filter_map(|(_, value)| condition(value))
        .collect();
    let olds = requires..(requires + usize::from(shape.olds)).min(bindings.len());
    found.extend(bindings[olds].iter().map(|(_, value)| (value, value.span)));
    found.extend(
        bindings
            .iter()
            .skip(ensures_from)
            .filter_map(|(_, value)| condition(value)),
    );
    found
}

/// The `$old` calls of an expression, outermost only, in source order: each call's span and its
/// operand.
fn old_operands(expression: &Expr, found: &mut Vec<(Span, Expr)>) {
    let mut calls: Vec<(Span, Expr)> = Vec::new();
    expression.visit(&mut |expression| {
        if let ExprKind::Call(callee, arguments) = &expression.kind
            && matches!(&callee.kind, ExprKind::Name(name) if name.text == "$old")
            && arguments.len() == 1
        {
            calls.push((expression.span, arguments[0].clone()));
        }
    });
    calls.sort_by_key(|(span, _)| (span.start, std::cmp::Reverse(span.end)));
    let mut outer: Vec<(Span, Expr)> = Vec::new();
    for (span, operand) in calls {
        if !outer
            .iter()
            .any(|(around, _)| around.start <= span.start && span.end <= around.end)
        {
            outer.push((span, operand));
        }
    }
    found.extend(outer);
}

fn mentions(expression: &Expr, name: &str) -> bool {
    let mut found = false;
    expression.visit(&mut |expression| {
        found |= matches!(&expression.kind, ExprKind::Name(ident) if ident.text == name);
    });
    found
}

impl Checker<'_> {
    /// The typed body of `function` with its contract lines woven in (A21):
    /// `Block { [$requires(c)]*, [$old.k = e]*, [result = body, $ensures(c)*]?, result }`.
    pub(super) fn contracted_body(
        &mut self,
        function: &FunctionDecl,
        expected: &Type,
    ) -> Result<(TypedExpr, ContractShape), Diagnostic> {
        let mut bindings = Vec::new();
        let mut shape = ContractShape::default();
        for clause in &function.contracts {
            if matches!(clause.condition.kind, ExprKind::Bool(true)) {
                self.contract_warnings.push(Diagnostic::of(
                    DiagnosticKind::ContractAlwaysTrue,
                    "this contract is always true; remove it or state a real condition",
                    clause.condition.span,
                ));
            }
            if let Some(binder) = &clause.binder
                && binder.text != "_"
                && !mentions(&clause.condition, &binder.text)
            {
                self.contract_warnings.push(Diagnostic::of(
                    DiagnosticKind::ContractResultUnused,
                    format!(
                        "the result name '{}' is not used; write '@ensures \\_ -> ...' to say that the line is about the arguments",
                        binder.text
                    ),
                    binder.span,
                ));
            }
        }
        for clause in function
            .contracts
            .iter()
            .filter(|clause| clause.kind == ContractKind::Requires)
        {
            let condition = self.contract_condition(&clause.condition)?;
            bindings.push(self.contract_call(Builtin::ContractRequires, condition, clause.span));
            shape.requires += 1;
        }
        // Phase 6: each `@old (e)` of the `@ensures` lines, evaluated on entry.
        let mut olds = Vec::new();
        for clause in &function.contracts {
            if clause.kind == ContractKind::Ensures {
                old_operands(&clause.condition, &mut olds);
            }
        }
        let mut old_locals = Vec::new();
        if !olds.is_empty() {
            self.scopes.push(BTreeMap::new());
            for (index, (span, operand)) in olds.iter().enumerate() {
                let mut nested = Vec::new();
                old_operands(operand, &mut nested);
                if let Some((inner, _)) = nested.first() {
                    return Err(Diagnostic::of(
                        DiagnosticKind::ContractOldNested,
                        "'@old' cannot contain another '@old'; take the old value once",
                        *inner,
                    ));
                }
                let binder = function.contracts.iter().find_map(|clause| {
                    (clause.kind == ContractKind::Ensures
                        && clause.span.start <= span.start
                        && span.end <= clause.span.end)
                        .then_some(clause.binder.as_ref())
                        .flatten()
                });
                if let Some(binder) = binder
                    && binder.text != "_"
                    && mentions(operand, &binder.text)
                {
                    return Err(Diagnostic::of(
                        DiagnosticKind::ContractOldOperand,
                        format!(
                            "'@old' reads the value on entry, before the result '{}' exists; read the result outside '@old'",
                            binder.text
                        ),
                        operand.span,
                    ));
                }
                let value = self.expression(operand, None)?;
                let ty = self.inference.resolve(&value.ty);
                if !ty.is_copy(&self.types) || ty.needs_drop(&self.types) || ty.contains_reference()
                {
                    return Err(Diagnostic::of(
                        DiagnosticKind::ContractOldNotCopy,
                        format!(
                            "'@old' keeps a copy of the value on entry, but '{}' is not a plain Copy value; take a number, a bool, or a field of one",
                            ty.display(&self.types)
                        ),
                        operand.span,
                    ));
                }
                let local = self.bind(
                    &Ident {
                        text: format!("$old.{index}"),
                        span: *span,
                        provenance: Provenance::Generated,
                    },
                    value.ty.clone(),
                    false,
                );
                old_locals.push((*span, local.clone()));
                bindings.push((local, value));
                shape.olds += 1;
            }
        }
        let result = (|| {
            let body = self.expression(&function.body, Some(expected))?;
            if !function
                .contracts
                .iter()
                .any(|clause| clause.kind == ContractKind::Ensures)
            {
                return Ok((body, None));
            }
            let resolved = self.inference.resolve(expected);
            if resolved.contains_reference() {
                let clause = function
                    .contracts
                    .iter()
                    .find(|clause| clause.kind == ContractKind::Ensures)
                    .unwrap();
                return Err(Diagnostic::of(
                    DiagnosticKind::ContractEnsuresReferenceResult,
                    format!(
                        "an '@ensures' needs the result as a value, but '{}' holds a reference; remove the line or return an owned value",
                        resolved.display(&self.types)
                    ),
                    clause.span,
                ));
            }
            let result = self.bind(
                &Ident {
                    text: "$contract.result".into(),
                    span: function.name.span,
                    provenance: Provenance::Generated,
                },
                body.ty.clone(),
                false,
            );
            let mut ensures = Vec::new();
            for clause in function
                .contracts
                .iter()
                .filter(|clause| clause.kind == ContractKind::Ensures)
            {
                self.scopes.push(BTreeMap::new());
                if let Some(binder) = &clause.binder
                    && binder.text != "_"
                {
                    self.scopes
                        .last_mut()
                        .unwrap()
                        .insert(binder.text.clone(), result.clone());
                }
                let outer = std::mem::replace(&mut self.old_locals, old_locals.clone());
                let checked = self.contract_condition(&clause.condition);
                self.old_locals = outer;
                self.scopes.pop();
                ensures.push(self.contract_call(Builtin::ContractEnsures, checked?, clause.span));
            }
            Ok((body, Some((result, ensures))))
        })();
        if !olds.is_empty() {
            self.scopes.pop();
        }
        let (body, ensures) = result?;
        let Some((result, ensures)) = ensures else {
            if bindings.is_empty() {
                return Ok((body, shape));
            }
            let span = body.span;
            let ty = body.ty.clone();
            return Ok((
                TypedExpr {
                    kind: TypedExprKind::Block {
                        bindings,
                        result: Box::new(body),
                    },
                    ty,
                    span,
                },
                shape,
            ));
        };
        let span = body.span;
        let ty = result.ty.clone();
        bindings.push((result.clone(), body));
        shape.ensures = u8::try_from(ensures.len()).unwrap_or(u8::MAX);
        bindings.extend(ensures);
        Ok((
            TypedExpr {
                kind: TypedExprKind::Block {
                    bindings,
                    result: Box::new(TypedExpr {
                        kind: TypedExprKind::Local(result.id),
                        ty: ty.clone(),
                        span,
                    }),
                },
                ty,
                span,
            },
            shape,
        ))
    }

    /// A contract line's condition, which must be `bool`.
    fn contract_condition(&mut self, condition: &Expr) -> Result<TypedExpr, Diagnostic> {
        let typed = self.expression(condition, None)?;
        if self.same(&typed.ty, &Type::Bool, condition.span).is_err() {
            let found = self.inference.resolve(&typed.ty).display(&self.types);
            return Err(Diagnostic::of(
                DiagnosticKind::ContractNotBool,
                format!(
                    "a contract condition must be 'bool', but this is '{found}'; write a comparison such as 'x > 0'"
                ),
                condition.span,
            )
            .with_help("compare the value, for example 'x > 0'"));
        }
        Ok(typed)
    }

    /// `_ = $requires condition` (or `$ensures`), at the contract line.
    fn contract_call(
        &mut self,
        builtin: Builtin,
        condition: TypedExpr,
        span: Span,
    ) -> (Local, TypedExpr) {
        let callee = TypedExpr {
            kind: TypedExprKind::Function(FunctionRef::Builtin(BuiltinInstance {
                builtin,
                types: Vec::new(),
            })),
            ty: Type::function(vec![Type::Bool], Type::Unit),
            span,
        };
        let call = TypedExpr {
            kind: TypedExprKind::Call(Box::new(callee), vec![condition]),
            ty: Type::Unit,
            span,
        };
        let local = self.bind(
            &Ident {
                text: "_".into(),
                span,
                provenance: Provenance::Generated,
            },
            Type::Unit,
            false,
        );
        (local, call)
    }
}

/// `E1033` (`contract_impure`) for each function whose contract conditions are not pure: they
/// may not do any of A19's effects (`alloc`, `trap`, `copy` and `diverge` are allowed). The first
/// source in source order, per function.
pub(super) fn validate_purity(module: &CheckedModule) -> Vec<Diagnostic> {
    let roots: Vec<(usize, Vec<(&TypedExpr, Span)>)> = module
        .functions
        .iter()
        .enumerate()
        .filter(|(_, function)| !function.contracts.is_empty())
        .map(|(id, function)| (id, conditions(function)))
        .collect();
    if roots.is_empty() {
        return Vec::new();
    }
    match effects::impurities(module, &roots) {
        Ok(found) => {
            let mut seen = BTreeSet::new();
            found
                .into_iter()
                .filter(|(function, _)| {
                    seen.insert(
                        module.functions[*function]
                            .qualified_name()
                            .split(".$mono.")
                            .next()
                            .unwrap()
                            .to_owned(),
                    )
                })
                .map(|(_, diagnostic)| diagnostic)
                .collect()
        }
        Err(error) => vec![error],
    }
}

/// `E1033` (`contract_writes`) for each function whose contract lines may change data that the
/// function received or returns: an assignment to it, a mutable borrow of it, or a call that
/// receives it as a mutable reference. Such a line would make `check` and `off` compute
/// differently. A condition may read mutable arguments and change data of its own.
pub(super) fn validate_writes(module: &CheckedModule) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut seen = BTreeSet::new();
    for function in &module.functions {
        if function.contracts.is_empty() {
            continue;
        }
        // The storage of the call: the parameters and the locals that the woven body binds
        // around the conditions (the result and the `@old` values).
        let mut outside: BTreeSet<usize> = function.parameters.iter().map(|p| p.id).collect();
        if let TypedExprKind::Block { bindings, .. } = &function.body.kind {
            outside.extend(bindings.iter().map(|(local, _)| local.id));
        }
        for (condition, _) in conditions(function) {
            let Some(span) = write_outside(condition, &outside) else {
                continue;
            };
            let name = function.qualified_name();
            if seen.insert(name.split(".$mono.").next().unwrap().to_owned()) {
                diagnostics.push(
                    Diagnostic::of(
                        DiagnosticKind::ContractWrites,
                        "a contract line cannot change the data that the function receives or returns; 'off' would not run it",
                        span,
                    )
                    .with_help("read the value, or copy it into a local of the condition first"),
                );
            }
            break;
        }
    }
    diagnostics
}

/// The local that a place is rooted at: through fields, indices, slices, borrows and
/// dereferences.
fn place_root(mut expression: &TypedExpr) -> Option<usize> {
    loop {
        match &expression.kind {
            TypedExprKind::Local(id) => return Some(*id),
            TypedExprKind::Field(inner, _)
            | TypedExprKind::Index(inner, _)
            | TypedExprKind::Borrow(inner, _)
            | TypedExprKind::BorrowOperand(inner)
            | TypedExprKind::Dereference(inner)
            | TypedExprKind::Slice { value: inner, .. } => expression = inner,
            _ => return None,
        }
    }
}

/// The first place in `condition` that may write to one of the `outside` locals.
fn write_outside(condition: &TypedExpr, outside: &BTreeSet<usize>) -> Option<Span> {
    let rooted = |place: &TypedExpr| place_root(place).is_some_and(|id| outside.contains(&id));
    let mut pending = vec![condition];
    while let Some(expression) = pending.pop() {
        match &expression.kind {
            TypedExprKind::Assign(place, _) if rooted(place) => return Some(expression.span),
            TypedExprKind::Borrow(place, true) if rooted(place) => return Some(expression.span),
            TypedExprKind::Call(callee, arguments) => {
                if let Type::Function(parameters, _) = &callee.ty {
                    for (index, argument) in arguments.iter().enumerate() {
                        let mutable =
                            matches!(parameters.types.get(index), Some(Type::Reference(_, true)))
                                || parameters.conventions.get(index)
                                    == Some(&ParameterConvention::Mut);
                        if mutable && rooted(argument) {
                            return Some(argument.span);
                        }
                    }
                }
            }
            _ => {}
        }
        pending.extend(expression.children());
    }
    None
}

/// Whether a parameter of this written type has a generator for the property tests: the named
/// numbers, `bool`, `f64`, `char`, `string`, arrays and tuples of two to four of such, and the
/// non-generic records and unions of `program` that derive both `Arbitrary` and `Display`
/// (appendix E.2 of A21). `Maybe` has a generator but no `Display`, which the test's
/// counterexample needs, so it has none here.
fn generated(ty: &TypeExpr, program: &Program) -> bool {
    match &ty.kind {
        TypeExprKind::Named(name) => {
            matches!(
                name.as_str(),
                "i8" | "i16"
                    | "i32"
                    | "i64"
                    | "i8u"
                    | "i16u"
                    | "i32u"
                    | "i64u"
                    | "bool"
                    | "f64"
                    | "char"
                    | "string"
            ) || {
                let derives = |derives: &[(DeriveClass, Span)]| {
                    derives
                        .iter()
                        .any(|(class, _)| *class == DeriveClass::Arbitrary)
                        && derives
                            .iter()
                            .any(|(class, _)| *class == DeriveClass::Display)
                };
                program.records.iter().any(|record| {
                    record.name.text == *name
                        && record.parameters.is_empty()
                        && derives(&record.derives)
                }) || program.unions.iter().any(|union| {
                    union.name.text == *name
                        && union.parameters.is_empty()
                        && derives(&union.derives)
                })
            }
        }
        TypeExprKind::Array(element) => generated(element, program),
        TypeExprKind::Tuple(elements) => {
            (2..=4).contains(&elements.len())
                && elements.iter().all(|element| generated(element, program))
        }
        TypeExprKind::Own(inner) => generated(inner, program),
        _ => false,
    }
}

fn mentions_variable(ty: &TypeExpr) -> bool {
    let mut found = false;
    let mut pending = vec![ty];
    while let Some(ty) = pending.pop() {
        match &ty.kind {
            TypeExprKind::Variable(_) => found = true,
            TypeExprKind::Named(_) | TypeExprKind::Length(_) | TypeExprKind::Dyn(_) => {}
            TypeExprKind::Apply(_, arguments) => pending.extend(arguments.iter()),
            TypeExprKind::Regions(inner, _)
            | TypeExprKind::Quantified(_, inner)
            | TypeExprKind::Array(inner)
            | TypeExprKind::ArrayView(inner)
            | TypeExprKind::List(inner)
            | TypeExprKind::Task(inner)
            | TypeExprKind::Reference(inner, _)
            | TypeExprKind::Own(inner)
            | TypeExprKind::Mut(inner)
            | TypeExprKind::Pure(inner) => pending.push(inner),
            TypeExprKind::FixedArray(element, length) => {
                pending.push(element);
                pending.push(length);
            }
            TypeExprKind::Tuple(elements) => pending.extend(elements.iter()),
            TypeExprKind::Function(parameters, result) => {
                pending.extend(parameters.iter());
                pending.push(result);
            }
        }
    }
    found
}

/// Why a function with contract lines gets no property test, or `None` when it gets one.
fn skipped(function: &FunctionDecl, program: &Program) -> Option<String> {
    if !function.constraints.is_empty()
        || mentions_variable(&function.result)
        || function
            .parameters
            .iter()
            .any(|parameter| mentions_variable(&parameter.ty))
    {
        return Some("it is generic".into());
    }
    if function.parameters.is_empty() {
        return Some("it has no parameters".into());
    }
    if let Some(parameter) = function
        .parameters
        .iter()
        .find(|parameter| parameter.mutable || !generated(&parameter.ty, program))
    {
        return Some(format!(
            "parameter '{}' has type '{}', which has no generator",
            parameter.name.text,
            crate::docgen::type_text(&parameter.ty)
        ));
    }
    let names: BTreeSet<&str> = function
        .contracts
        .iter()
        .filter_map(|clause| clause.binder.as_ref())
        .map(|binder| binder.text.as_str())
        .filter(|name| *name != "_")
        .collect();
    (names.len() > 1).then(|| "its '@ensures' lines name the result differently".into())
}

/// A21 Phase 3 (`--contracts test`): for each function of `program` with contract lines, a
/// private twin `$contract_raw_<name>` without them and a test `contract <name>` that checks its
/// `@ensures` lines on generated arguments that satisfy its `@requires` lines. Returns `W1009`
/// for each function that gets no test, and why.
pub(crate) fn generate_tests(program: &mut Program, module: &str) -> Vec<Diagnostic> {
    let mut warnings = Vec::new();
    let mut twins = Vec::new();
    let mut tests = Vec::new();
    for function in &program.functions {
        if function.contracts.is_empty() {
            continue;
        }
        if let Some(reason) = skipped(function, program) {
            warnings.push(Diagnostic::of(
                DiagnosticKind::ContractTestSkipped,
                format!(
                    "no property test for '{module}.{}': {reason}",
                    function.name.text
                ),
                function.name.span,
            ));
            continue;
        }
        match property_test(function, module) {
            Ok((twin, test)) => {
                twins.push(twin);
                tests.push(test);
            }
            Err(error) => warnings.push(error),
        }
    }
    program.functions.extend(twins);
    program.tests.extend(tests);
    warnings
}

fn property_test(
    function: &FunctionDecl,
    module: &str,
) -> Result<(FunctionDecl, TestDecl), Diagnostic> {
    let build = deriving::Build {
        module,
        span: function.name.span,
        nodes: std::cell::Cell::new(0),
    };
    let raw = format!("$contract_raw_{}", function.name.text);
    let mut twin = function.clone();
    twin.name = build.ident(&raw);
    twin.contracts = Vec::new();
    twin.visibility = Visibility::Private;
    twin.exported = false;
    twin.recursion = None;
    twin.doc = None;
    let names: Vec<&str> = function
        .parameters
        .iter()
        .map(|parameter| parameter.name.text.as_str())
        .collect();
    let all = |kind: ContractKind| -> Result<Option<Expr>, Diagnostic> {
        let mut joined: Option<Expr> = None;
        for clause in function
            .contracts
            .iter()
            .filter(|clause| clause.kind == kind)
        {
            let condition = clause.condition.clone();
            joined = Some(match joined {
                None => condition,
                Some(left) => build.binary(BinaryOp::And, left, condition)?,
            });
        }
        Ok(joined)
    };
    let result = function
        .contracts
        .iter()
        .filter_map(|clause| clause.binder.as_ref())
        .map(|binder| binder.text.clone())
        .find(|name| name != "_")
        .unwrap_or_else(|| "_".into());
    let arguments = names
        .iter()
        .map(|name| build.name(name))
        .collect::<Result<Vec<_>, _>>()?;
    let call = build.make(ExprKind::Call(Box::new(build.name(&raw)?), arguments))?;
    // Each `@old (e)` is `e` on entry: bound before the call, which may take the arguments, and
    // read by name in the `@ensures` lines.
    let mut olds = Vec::new();
    for clause in &function.contracts {
        if clause.kind == ContractKind::Ensures {
            old_operands(&clause.condition, &mut olds);
        }
    }
    let mut bindings: Vec<_> = olds
        .iter()
        .enumerate()
        .map(|(index, (_, operand))| build.bind(format!("$old_{index}"), operand.clone()))
        .collect();
    let ensures = all(ContractKind::Ensures)?.map(|mut ensures| {
        ensures.visit_mut(&mut |expression| {
            let Some(index) = olds
                .iter()
                .position(|(span, _)| *span == expression.span)
                .filter(|_| matches!(&expression.kind, ExprKind::Call(callee, _) if matches!(&callee.kind, ExprKind::Name(name) if name.text == "$old")))
            else {
                return true;
            };
            expression.kind = ExprKind::Name(build.ident(format!("$old_{index}")));
            false
        });
        ensures
    });
    let verdict = match ensures {
        Some(ensures) => build.construct("Maybe.Maybe", "Some", ensures)?,
        None => build.construct("Maybe.Maybe", "Some", build.boolean(true)?)?,
    };
    bindings.push(build.bind(result, call));
    let mut property = build.block(bindings, verdict)?;
    if let Some(requires) = all(ContractKind::Requires)? {
        property = build.conditional(requires, property, build.case("Maybe.Maybe", "None")?)?;
    }
    // The generator of the arguments, paired from the right, and the pattern that names them.
    let mut generator = build.call("Arbitrary", "arbitrary", Vec::new())?;
    let binding = |name: &str| Pattern {
        kind: PatternKind::Binding(build.ident(name)),
        span: build.span,
        depth: 1,
    };
    let mut pattern = binding(names[names.len() - 1]);
    for name in names[..names.len() - 1].iter().rev() {
        generator = build.apply(
            "Gen.pair",
            vec![build.call("Arbitrary", "arbitrary", Vec::new())?, generator],
        )?;
        let depth = pattern.depth + 1;
        pattern = Pattern {
            kind: PatternKind::Tuple(vec![binding(name), pattern]),
            span: build.span,
            depth,
        };
    }
    let checker = if names.len() == 1 {
        build.lambda(&[names[0]], property)?
    } else {
        build.lambda(
            &["$args"],
            build.matched("$args", vec![build.arm(pattern, property)])?,
        )?
    };
    let body = build.apply("Gen.for_all_where", vec![generator, checker])?;
    let test = TestDecl {
        name: format!("contract {}", function.name.text),
        name_span: function.name.span,
        body,
        span: function.name.span,
    };
    Ok((twin, test))
}
