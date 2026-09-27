use super::*;

#[derive(Clone, Copy, Default)]
pub(super) struct WarningOptions {
    pub shadowing: bool,
}

pub(super) fn unused_locals(function: &CheckedFunction) -> Vec<Diagnostic> {
    if function.origin.module != ModuleOrigin::User {
        return Vec::new();
    }
    let mut locals: BTreeMap<_, _> = function
        .parameters
        .iter()
        .map(|local| (local.id, local))
        .collect();
    let mut used = BTreeSet::new();
    collect_locals(&function.body, &mut locals, &mut used);
    locals
        .into_values()
        .filter(|local| {
            local.provenance == Provenance::User
                && !local.name.starts_with('_')
                && !used.contains(&local.id)
        })
        .map(|local| {
            Diagnostic::warning(
                "W1001",
                format!(
                    "unused local '{}'; prefix it with '_' to silence this warning",
                    local.name
                ),
                local.span,
            )
        })
        .collect()
}

fn collect_locals<'a>(
    expression: &'a TypedExpr,
    locals: &mut BTreeMap<usize, &'a Local>,
    used: &mut BTreeSet<usize>,
) {
    use TypedExprKind::*;
    match &expression.kind {
        Local(id) => {
            used.insert(*id);
        }
        Lambda {
            parameters,
            captures,
            ..
        } => {
            locals.extend(parameters.iter().map(|local| (local.id, local)));
            used.extend(captures.iter().map(|local| local.id));
        }
        Block { bindings, .. } => {
            locals.extend(bindings.iter().map(|(local, _)| (local.id, local)))
        }
        ForRange { local, .. } | ForEach { local, .. } => {
            locals.insert(local.id, local);
        }
        Match { arms, .. } => {
            for arm in arms {
                for alternative in &arm.alternatives {
                    locals.extend(
                        alternative
                            .bindings
                            .iter()
                            .map(|(local, _)| (local.id, local)),
                    );
                    for step in &alternative.steps {
                        if let PatternStep::Bind(local, _) = step {
                            locals.insert(local.id, local);
                        }
                    }
                }
            }
        }
        Error
        | Int(_)
        | Float(_)
        | String(_)
        | Bool(_)
        | Unit
        | Break
        | Continue
        | Function(_)
        | GenericFunction(..)
        | Method(..)
        | TypeFunction { .. }
        | GenericInteger(..)
        | GenericFloat(_)
        | Unary(..)
        | Binary(..)
        | Call(..)
        | HostCall(..)
        | Closure(..)
        | TaskRun(_)
        | TaskParallel(_)
        | TaskParallelResults(_)
        | If { .. }
        | While { .. }
        | Record(_)
        | RecordUpdate { .. }
        | Array(_)
        | List(_)
        | Tuple(_)
        | ListTail(..)
        | NewArray(..)
        | NewList(..)
        | NewLiteral(_)
        | Field(..)
        | Index(..)
        | Slice { .. }
        | Length(_)
        | StringLength(_)
        | Borrow(..)
        | BorrowOperand(_)
        | Dereference(_)
        | Assign(..)
        | Cast(_)
        | CaseConstructor { .. }
        | Construct { .. }
        | UnionTag(_)
        | UnionPayload { .. }
        | Parallel(..)
        | StructuralCompare(..)
        | StructuralHash(_)
        | StructuralDisplay(_) => {}
    }
    for child in expression.children() {
        collect_locals(child, locals, used);
    }
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Dependency {
    Function(usize),
    Type(String),
}

pub(super) fn unused_private(
    functions: &[CheckedFunction],
    declarations: &[(String, FunctionDecl)],
    modules: &[ModuleInput<'_>],
    names: &Names,
    types: TypeContext<'_>,
    calls: &[Vec<usize>],
    entry: Option<usize>,
) -> Vec<Diagnostic> {
    let mut edges = BTreeMap::new();
    let mut roots = BTreeSet::new();
    let mut candidates = BTreeMap::new();
    for (id, function) in functions.iter().enumerate() {
        let node = Dependency::Function(id);
        let mut references: BTreeSet<_> = calls[id]
            .iter()
            .copied()
            .map(Dependency::Function)
            .collect();
        let mut scan = TypeReferences {
            names,
            types,
            module: &function.module,
            references: &mut references,
        };
        for parameter in &function.signature.parameters {
            scan.checked_type(parameter);
        }
        scan.checked_type(&function.signature.result);
        scan.checked_expression(&function.body);
        if let Some((_, declaration)) = declarations.get(id) {
            for parameter in &declaration.parameters {
                scan.ty(&parameter.ty);
            }
            scan.ty(&declaration.result);
            for constraint in &declaration.constraints {
                scan.ty(&constraint.ty);
            }
            scan.expression(&declaration.body);
        }
        edges.insert(node.clone(), references);
        if function.origin.module == ModuleOrigin::User {
            if function.visibility == Visibility::Private
                && function.origin.provenance == Provenance::User
                && !function.exported
                && function.origin.test.is_none()
                && Some(id) != entry
            {
                candidates.insert(
                    node,
                    Diagnostic::warning(
                        "W1002",
                        format!(
                            "unused private function '{}'; remove it or make it public",
                            function.qualified_name()
                        ),
                        function.span,
                    ),
                );
            } else {
                roots.insert(node);
            }
        }
    }
    for input in modules
        .iter()
        .filter(|input| input.origin == ModuleOrigin::User)
    {
        let program = input.program;
        let type_declarations = program
            .records
            .iter()
            .map(|record| {
                (
                    &record.name,
                    record.visibility,
                    record
                        .fields
                        .iter()
                        .map(|field| &field.ty)
                        .collect::<Vec<_>>(),
                )
            })
            .chain(program.unions.iter().map(|union| {
                (
                    &union.name,
                    union.visibility,
                    union
                        .cases
                        .iter()
                        .filter_map(|case| case.payload.as_ref())
                        .collect(),
                )
            }))
            .chain(
                program
                    .type_aliases
                    .iter()
                    .map(|alias| (&alias.name, alias.visibility, vec![&alias.target])),
            );
        for (name, visibility, fields) in type_declarations {
            let qualified = format!("{}.{}", input.name, name.text);
            let node = Dependency::Type(qualified.clone());
            let mut references = BTreeSet::new();
            let mut scan = TypeReferences {
                names,
                types,
                module: input.name,
                references: &mut references,
            };
            for field in fields {
                scan.ty(field);
            }
            edges.insert(node.clone(), references);
            if visibility == Visibility::Private && name.provenance == Provenance::User {
                candidates.insert(
                    node,
                    Diagnostic::warning(
                        "W1002",
                        format!("unused private type '{qualified}'; remove it or make it public"),
                        name.span,
                    ),
                );
            } else {
                roots.insert(node);
            }
        }
        let mut references = BTreeSet::new();
        let mut scan = TypeReferences {
            names,
            types,
            module: input.name,
            references: &mut references,
        };
        for class in &program.classes {
            for method in &class.methods {
                for parameter in &method.parameters {
                    scan.ty(parameter);
                }
                scan.ty(&method.result);
                for constraint in &method.constraints {
                    scan.ty(&constraint.ty);
                }
            }
        }
        for instance in &program.instances {
            scan.ty(&instance.ty);
        }
        roots.extend(references);
        if let (Some(id), Some(expression)) = (entry, &program.entry) {
            let references = edges.entry(Dependency::Function(id)).or_default();
            TypeReferences {
                names,
                types,
                module: input.name,
                references,
            }
            .expression(expression);
        }
    }
    let mut reached = BTreeSet::new();
    let mut pending: Vec<_> = roots.into_iter().collect();
    while let Some(node) = pending.pop() {
        if reached.insert(node.clone()) {
            if let Some(references) = edges.get(&node) {
                pending.extend(references.iter().cloned());
            }
        }
    }
    candidates
        .into_iter()
        .filter_map(|(node, diagnostic)| (!reached.contains(&node)).then_some(diagnostic))
        .collect()
}

struct TypeReferences<'a> {
    names: &'a Names,
    types: TypeContext<'a>,
    module: &'a str,
    references: &'a mut BTreeSet<Dependency>,
}

impl TypeReferences<'_> {
    fn named(&mut self, name: &str, span: Span) {
        if let Ok(ty) = self.names.named_type(self.module, name, span) {
            self.references
                .insert(Dependency::Type(ty.info().name.clone()));
        }
    }

    fn ty(&mut self, ty: &TypeExpr) {
        match &ty.kind {
            TypeExprKind::Named(name) => self.named(name, ty.span),
            TypeExprKind::Apply(name, arguments) => {
                self.named(&name.text, name.span);
                for argument in arguments {
                    self.ty(argument);
                }
            }
            TypeExprKind::Array(inner)
            | TypeExprKind::List(inner)
            | TypeExprKind::Task(inner)
            | TypeExprKind::Reference(inner, _)
            | TypeExprKind::Regions(inner, _) => self.ty(inner),
            TypeExprKind::Tuple(elements) => {
                for element in elements {
                    self.ty(element);
                }
            }
            TypeExprKind::Function(parameters, result) => {
                for parameter in parameters {
                    self.ty(parameter);
                }
                self.ty(result);
            }
            TypeExprKind::Variable(_) => {}
        }
    }

    fn checked_type(&mut self, ty: &Type) {
        match ty {
            Type::Record(id, arguments) | Type::Union(id, arguments) => {
                let name = if matches!(ty, Type::Record(..)) {
                    &self.types.records[*id].name
                } else {
                    &self.types.unions[*id].name
                };
                self.references.insert(Dependency::Type(name.clone()));
                for argument in arguments {
                    self.checked_type(argument);
                }
            }
            Type::Array(inner)
            | Type::List(inner)
            | Type::Vec(inner)
            | Type::Task(inner)
            | Type::Reference(inner, _) => self.checked_type(inner),
            Type::Tuple(elements) => {
                for element in elements {
                    self.checked_type(element);
                }
            }
            Type::Function(parameters, result) => {
                for parameter in parameters {
                    self.checked_type(parameter);
                }
                self.checked_type(result);
            }
            _ => {}
        }
    }

    fn checked_expression(&mut self, expression: &TypedExpr) {
        self.checked_type(&expression.ty);
        for child in expression.children() {
            self.checked_expression(child);
        }
    }

    fn expression(&mut self, expression: &Expr) {
        use ExprKind::*;
        match &expression.kind {
            Block { bindings, result } => {
                for binding in bindings {
                    if let Some(ty) = &binding.annotation {
                        self.ty(ty);
                    }
                    self.expression(&binding.value);
                }
                self.expression(result);
            }
            NewArray(ty, length, initialize) | NewList(ty, length, initialize) => {
                self.ty(ty);
                self.expression(length);
                self.expression(initialize);
            }
            Cast(value, ty) => {
                self.ty(ty);
                self.expression(value);
            }
            For {
                pattern,
                source,
                body,
            } => {
                self.pattern(pattern);
                self.expression(source);
                self.expression(body);
            }
            Match { value, arms, .. } => {
                self.expression(value);
                for arm in arms {
                    self.pattern(&arm.pattern);
                    if let Some(guard) = &arm.guard {
                        self.expression(guard);
                    }
                    self.expression(&arm.body);
                }
            }
            Record { name, fields } => {
                self.named(&name.text, name.span);
                for (_, value) in fields {
                    self.expression(value);
                }
            }
            RecordUpdate { base, fields } => {
                self.expression(base);
                for (_, value) in fields {
                    self.expression(value);
                }
            }
            Computation(_, body) => self.computation(body),
            Unary(_, value)
            | Lambda(_, value)
            | Task(value)
            | TaskRun(value)
            | ComputationBoundary(value)
            | NewLiteral(value)
            | Field(value, _)
            | Borrow(value, ..)
            | Dereference(value, _) => self.expression(value),
            Binary(_, left, right)
            | Index(left, right)
            | Assign(left, right)
            | While {
                condition: left,
                body: right,
            } => {
                self.expression(left);
                self.expression(right);
            }
            Call(callee, arguments) => {
                self.expression(callee);
                for argument in arguments {
                    self.expression(argument);
                }
            }
            If {
                condition,
                then_branch,
                else_branch,
            } => {
                self.expression(condition);
                self.expression(then_branch);
                self.expression(else_branch);
            }
            Range {
                start,
                step,
                finish,
                ..
            } => {
                self.expression(start);
                if let Some(step) = step {
                    self.expression(step);
                }
                self.expression(finish);
            }
            Slice { value, start, end } => {
                self.expression(value);
                for bound in start.iter().chain(end.iter()) {
                    self.expression(bound);
                }
            }
            Array(values) | List(values) | Tuple(values) => {
                for value in values {
                    self.expression(value);
                }
            }
            Integer(..) | Float(..) | String(_) | Char(_) | Utf8Char(_) | Bool(_) | Unit
            | Break | Continue | Name(_) | QualifiedFunction(_) | TypeFunction(..) => {}
        }
    }

    fn pattern(&mut self, pattern: &Pattern) {
        use PatternKind::*;
        match &pattern.kind {
            Annotated(pattern, ty) => {
                self.ty(ty);
                self.pattern(pattern);
            }
            Record(name, fields) => {
                if let Some(name) = name {
                    self.named(&name.text, name.span);
                }
                for (_, field) in fields {
                    self.pattern(field);
                }
            }
            Apply(_, patterns) | Tuple(patterns) | Array(patterns) | List(patterns) => {
                for pattern in patterns {
                    self.pattern(pattern);
                }
            }
            Cons(left, right) | Or(left, right) | And(left, right) => {
                self.pattern(left);
                self.pattern(right);
            }
            As(pattern, _) => self.pattern(pattern),
            Argument(value) | Literal(value) => self.expression(value),
            Wildcard | Binding(_) => {}
        }
    }

    fn computation(&mut self, block: &ComputationBlock) {
        use ComputationStatementKind::*;
        for statement in &block.statements {
            match &statement.kind {
                LetAnd(bindings) => {
                    for binding in bindings {
                        if let Some(ty) = &binding.annotation {
                            self.ty(ty);
                        }
                        self.expression(&binding.value);
                    }
                }
                Match(value, arms) => {
                    self.expression(value);
                    for arm in arms {
                        self.pattern(&arm.pattern);
                        if let Some(guard) = &arm.guard {
                            self.expression(guard);
                        }
                        self.computation(&arm.body);
                    }
                }
                Let(binding, _) => {
                    if let Some(ty) = &binding.annotation {
                        self.ty(ty);
                    }
                    self.expression(&binding.value);
                }
                Do(value) | Operation(_, value) | Expression(value) => self.expression(value),
                If(condition, yes, no) => {
                    self.expression(condition);
                    self.computation(yes);
                    if let Some(no) = no {
                        self.computation(no);
                    }
                }
                For(pattern, source, body) => {
                    self.pattern(pattern);
                    self.expression(source);
                    self.computation(body);
                }
                While(condition, body) => {
                    self.expression(condition);
                    self.computation(body);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shadowing_is_opt_in_and_lexically_scoped() {
        let program = crate::parser::parse("let value = 1\nlet value = value + 1\nlet _hidden = 1\nlet _hidden = 2\n{ let value = 3; value }").unwrap();
        let modules = [ModuleInput {
            name: "Main",
            program: &program,
            origin: ModuleOrigin::User,
        }];
        for enabled in [false, true] {
            let module = check_modules_collect(
                &modules,
                &mut Diagnostics::new(0),
                WarningOptions { shadowing: enabled },
                None,
            )
            .unwrap();
            let warnings: Vec<_> = module
                .warnings
                .iter()
                .filter(|diagnostic| diagnostic.code == "W1004")
                .collect();
            assert_eq!(
                warnings.len(),
                usize::from(enabled),
                "{:?}",
                module.warnings
            );
        }
    }
}
