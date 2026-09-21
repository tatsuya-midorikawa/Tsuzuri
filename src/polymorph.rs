use super::*;

const MAX_SPECIALIZATIONS: usize = 1024;
const MAX_CONSTRAINTS: usize = 128;

#[derive(Clone, Debug)]
pub(super) struct Constraint {
    pub class: usize,
    pub ty: Type,
    pub span: Span,
}

pub(super) struct Scheme {
    pub signature: Signature,
    pub variables: Vec<String>,
    pub constraints: Vec<Constraint>,
}

pub(super) fn variables(ty: &Type) -> Vec<String> {
    let mut variables = BTreeSet::new();
    map_type(ty, &mut |ty| {
        if let Type::Variable(name) = ty {
            variables.insert(name.clone());
        }
        ty.clone()
    });
    variables.into_iter().collect()
}

pub(super) fn is_unknown(ty: &Type) -> bool {
    matches!(ty, Type::Variable(_) | Type::Infer(_))
}

fn map_type(ty: &Type, f: &mut impl FnMut(&Type) -> Type) -> Type {
    match ty {
        Type::Array(element) => Type::Array(Box::new(map_type(element, f))),
        Type::List(element) => Type::List(Box::new(map_type(element, f))),
        Type::Task(result) => Type::Task(Box::new(map_type(result, f))),
        Type::Reference(value, mutable) => Type::Reference(Box::new(map_type(value, f)), *mutable),
        Type::Function(parameters, result) => Type::function(
            parameters.iter().map(|ty| map_type(ty, f)).collect(),
            map_type(result, f),
        ),
        _ => f(ty),
    }
}

fn substitute(ty: &Type, substitutions: &BTreeMap<String, Type>) -> Type {
    map_type(ty, &mut |ty| match ty {
        Type::Variable(name) => substitutions
            .get(name)
            .cloned()
            .unwrap_or_else(|| ty.clone()),
        _ => ty.clone(),
    })
}

pub(super) fn bounded_type(ty: &Type, span: Span) -> Result<(), Diagnostic> {
    fn visit(ty: &Type, depth: usize, count: &mut usize) -> bool {
        *count += 1;
        if depth > MAX_NESTING || *count > 4096 {
            return false;
        }
        match ty {
            Type::Array(ty) | Type::List(ty) | Type::Task(ty) | Type::Reference(ty, _) => {
                visit(ty, depth + 1, count)
            }
            Type::Function(parameters, result) => {
                parameters.iter().all(|ty| visit(ty, depth + 1, count))
                    && visit(result, depth + 1, count)
            }
            _ => true,
        }
    }
    if visit(ty, 0, &mut 0) {
        Ok(())
    } else {
        Err(Diagnostic::new(
            "E1017",
            "polymorphic type expansion exceeds the compiler limit; simplify the type or recursion",
            span,
        ))
    }
}

pub(super) fn require_concrete(ty: &Type, span: Span) -> Result<(), Diagnostic> {
    let mut unknown = false;
    map_type(ty, &mut |ty| {
        unknown |= is_unknown(ty);
        ty.clone()
    });
    if unknown {
        Err(Diagnostic::new(
            "E1015",
            "a concrete type is required here; add a type annotation",
            span,
        ))
    } else {
        bounded_type(ty, span)
    }
}

#[derive(Default)]
pub(super) struct Inference {
    solutions: Vec<Option<Type>>,
    defaults: BTreeMap<usize, Type>,
}

impl Inference {
    pub(super) fn fresh(&mut self) -> Type {
        let id = self.solutions.len();
        self.solutions.push(None);
        Type::Infer(id)
    }

    pub fn resolve(&self, ty: &Type) -> Type {
        let mut ty = ty;
        while let Type::Infer(id) = ty {
            match &self.solutions[*id] {
                Some(solution) => ty = solution,
                None => break,
            }
        }
        match ty {
            Type::Array(element) => Type::Array(Box::new(self.resolve(element))),
            Type::List(element) => Type::List(Box::new(self.resolve(element))),
            Type::Task(result) => Type::Task(Box::new(self.resolve(result))),
            Type::Reference(value, mutable) => {
                Type::Reference(Box::new(self.resolve(value)), *mutable)
            }
            Type::Function(parameters, result) => Type::function(
                parameters.iter().map(|ty| self.resolve(ty)).collect(),
                self.resolve(result),
            ),
            _ => ty.clone(),
        }
    }

    pub fn unify(
        &mut self,
        actual: &Type,
        expected: &Type,
        records: &[CheckedRecord],
        span: Span,
    ) -> Result<(), Diagnostic> {
        let actual = self.resolve(actual);
        let expected = self.resolve(expected);
        if actual == expected {
            return Ok(());
        }
        match (&actual, &expected) {
            (Type::Infer(id), ty) | (ty, Type::Infer(id)) => {
                let mut occurs = false;
                map_type(ty, &mut |ty| {
                    occurs |= ty == &Type::Infer(*id);
                    ty.clone()
                });
                if occurs {
                    return Err(Diagnostic::new("E1015", "infinite inferred type", span));
                }
                bounded_type(ty, span)?;
                self.solutions[*id] = Some(ty.clone());
                return Ok(());
            }
            (Type::Array(a), Type::Array(b))
            | (Type::List(a), Type::List(b))
            | (Type::Task(a), Type::Task(b)) => {
                return self.unify(a, b, records, span);
            }
            (Type::Reference(a, n), Type::Reference(b, m)) if n == m => {
                return self.unify(a, b, records, span);
            }
            (Type::Function(a, x), Type::Function(b, y)) if a.is_empty() == b.is_empty() => {
                for (a, b) in a.iter().zip(b) {
                    self.unify(a, b, records, span)?;
                }
                let common = a.len().min(b.len());
                let tail = |parameters: &[Type], result: &Type| {
                    if parameters.len() == common {
                        result.clone()
                    } else {
                        Type::function(parameters[common..].to_vec(), result.clone())
                    }
                };
                return self.unify(&tail(a, x), &tail(b, y), records, span);
            }
            _ => {}
        }
        Err(Diagnostic::new(
            "E1003",
            format!(
                "expected {}, found {}",
                expected.display(records),
                actual.display(records)
            ),
            span,
        ))
    }

    pub fn default_numeric(&mut self, ty: &Type, default: Type) {
        if let Type::Infer(id) = ty {
            self.defaults.entry(*id).or_insert(default);
        }
    }

    fn apply_defaults(&mut self) {
        for (id, default) in self.defaults.clone() {
            if let Type::Infer(root) = self.resolve(&Type::Infer(id)) {
                self.solutions[root] = Some(default);
            }
        }
    }
}

#[derive(Clone, Copy)]
enum Operation {
    Binary(BinaryOp),
    Unary(UnaryOp),
}

struct Method {
    name: String,
    signature: Signature,
    operation: Option<Operation>,
}

struct Class {
    name: String,
    variable: String,
    methods: Vec<Method>,
    builtin: bool,
}

pub(super) struct Classes {
    declarations: Vec<Class>,
    names: BTreeMap<String, usize>,
    implementations: BTreeMap<(usize, Type), Vec<usize>>,
}

pub(super) fn binary_class(operator: BinaryOp) -> &'static str {
    use BinaryOp::*;
    match operator {
        Add => "Add",
        Subtract => "Sub",
        Multiply => "Mul",
        Divide => "Div",
        Remainder => "Rem",
        Equal | NotEqual => "Eq",
        Less | LessEqual | Greater | GreaterEqual => "Ord",
        BitAnd | BitOr | BitXor | ShiftLeft | ShiftRight | ShiftRightUnsigned => "Bits",
        And | Or | Pipe => unreachable!("non-overloadable operators"),
    }
}

impl Classes {
    pub fn collect(
        modules: &[(&str, &Program)],
        names: &Names,
        sizes: &[usize],
    ) -> Result<Self, Diagnostic> {
        use BinaryOp::*;
        let mut classes = Self {
            declarations: Vec::new(),
            names: BTreeMap::new(),
            implementations: BTreeMap::new(),
        };
        for name in [
            "Add",
            "Sub",
            "Mul",
            "Div",
            "Rem",
            "Eq",
            "Ord",
            "Bits",
            "Neg",
            "Integer",
            "SignedInteger",
            "Float",
            "Numeric",
            "Copy",
            "Capture",
            "Send",
        ] {
            classes
                .names
                .insert(name.into(), classes.declarations.len());
            classes.declarations.push(Class {
                name: name.into(),
                variable: "a".into(),
                methods: Vec::new(),
                builtin: true,
            });
        }
        for (name, operator) in [
            ("add", Add),
            ("sub", Subtract),
            ("mul", Multiply),
            ("div", Divide),
            ("rem", Remainder),
            ("eq", Equal),
            ("ne", NotEqual),
            ("lt", Less),
            ("le", LessEqual),
            ("gt", Greater),
            ("ge", GreaterEqual),
            ("bit_and", BitAnd),
            ("bit_or", BitOr),
            ("bit_xor", BitXor),
            ("shl", ShiftLeft),
            ("shr", ShiftRight),
            ("ushr", ShiftRightUnsigned),
        ] {
            let class = classes.names[binary_class(operator)];
            let a = Type::Variable("a".into());
            let result = if matches!(
                operator,
                Equal | NotEqual | Less | LessEqual | Greater | GreaterEqual
            ) {
                Type::Bool
            } else {
                a.clone()
            };
            classes.declarations[class].methods.push(Method {
                name: name.into(),
                signature: Signature {
                    parameters: vec![a.clone(), a],
                    result,
                },
                operation: Some(Operation::Binary(operator)),
            });
        }
        for (class, name, operator) in [
            ("Neg", "neg", UnaryOp::Negate),
            ("Bits", "bit_not", UnaryOp::BitNot),
        ] {
            let class = classes.names[class];
            let a = Type::Variable("a".into());
            classes.declarations[class].methods.push(Method {
                name: name.into(),
                signature: Signature {
                    parameters: vec![a.clone()],
                    result: a,
                },
                operation: Some(Operation::Unary(operator)),
            });
        }
        for (module, program) in modules {
            for declaration in &program.classes {
                let qualified = format!("{module}.{}", declaration.name.text);
                if declaration.name.text == "_"
                    || declaration.name.text == "Task"
                    || classes.names.contains_key(&declaration.name.text)
                    || classes
                        .names
                        .insert(qualified.clone(), classes.declarations.len())
                        .is_some()
                {
                    return Err(duplicate(&declaration.name));
                }
                let mut methods = Vec::new();
                let mut seen = BTreeSet::new();
                for method in &declaration.methods {
                    if method.name.text == "_" || !seen.insert(&method.name.text) {
                        return Err(duplicate(&method.name));
                    }
                    if !method.constraints.is_empty() {
                        return Err(Diagnostic::new(
                            "E1016",
                            "class methods use the class parameter; method-specific constraints are not supported",
                            method.name.span,
                        ));
                    }
                    for ty in method
                        .parameters
                        .iter()
                        .chain(std::iter::once(&method.result))
                    {
                        if !classes.inline_constraints(ty, module, names)?.is_empty() {
                            return Err(Diagnostic::new(
                                "E1016",
                                "class methods cannot add constraints to the class parameter",
                                method.name.span,
                            ));
                        }
                    }
                    let signature = Signature {
                        parameters: method
                            .parameters
                            .iter()
                            .map(|ty| resolve_type(ty, module, names))
                            .collect::<Result<_, _>>()?,
                        result: resolve_type(&method.result, module, names)?,
                    };
                    validate_size(&signature.as_type(), sizes, method.name.span)?;
                    bounded_type(&signature.as_type(), method.name.span)?;
                    if variables(&signature.as_type()) != [declaration.variable.text.clone()] {
                        return Err(Diagnostic::new(
                            "E1016",
                            "each class method must mention exactly the declared class type variable",
                            method.name.span,
                        ));
                    }
                    methods.push(Method {
                        name: method.name.text.clone(),
                        signature,
                        operation: None,
                    });
                }
                if methods.is_empty() {
                    return Err(Diagnostic::new(
                        "E1016",
                        "a class needs at least one method",
                        declaration.name.span,
                    ));
                }
                classes.declarations.push(Class {
                    name: qualified,
                    variable: declaration.variable.text.clone(),
                    methods,
                    builtin: false,
                });
            }
        }
        Ok(classes)
    }

    fn find(&self, module: &str, name: &str) -> Option<usize> {
        self.names
            .get(&format!("{module}.{name}"))
            .or_else(|| self.names.get(name))
            .copied()
    }

    pub fn resolve(&self, module: &str, name: &Ident) -> Result<usize, Diagnostic> {
        self.find(module, &name.text).ok_or_else(|| {
            Diagnostic::new(
                "E1016",
                format!("unknown type class '{}'", name.text),
                name.span,
            )
        })
    }

    pub(super) fn inline_constraints(
        &self,
        expression: &TypeExpr,
        module: &str,
        names: &Names,
    ) -> Result<Vec<Constraint>, Diagnostic> {
        let mut constraints = Vec::new();
        match &expression.kind {
            TypeExprKind::Constrained(class, ty) => {
                constraints.push(Constraint {
                    class: self.resolve(module, class)?,
                    ty: resolve_type(ty, module, names)?,
                    span: class.span,
                });
                constraints.extend(self.inline_constraints(ty, module, names)?);
            }
            TypeExprKind::Array(ty)
            | TypeExprKind::List(ty)
            | TypeExprKind::Task(ty)
            | TypeExprKind::Reference(ty, _) => {
                constraints.extend(self.inline_constraints(ty, module, names)?)
            }
            TypeExprKind::Function(parameters, result) => {
                for ty in parameters.iter().chain(std::iter::once(result.as_ref())) {
                    constraints.extend(self.inline_constraints(ty, module, names)?);
                }
            }
            _ => {}
        }
        Ok(constraints)
    }

    pub fn instances(
        &mut self,
        modules: &[(&str, &Program)],
        names: &Names,
        records: &[CheckedRecord],
        functions: &mut Vec<(String, FunctionDecl)>,
    ) -> Result<(), Diagnostic> {
        for (module, program) in modules {
            for instance in &program.instances {
                let id = self.resolve(module, &instance.class)?;
                let class = &self.declarations[id];
                let ty = resolve_type(&instance.ty, module, names)?;
                require_concrete(&ty, instance.ty.span)?;
                if class.builtin && (class.methods.is_empty() || self.intrinsic(id, &ty, records)) {
                    return Err(Diagnostic::new(
                        "E1016",
                        "built-in instances and marker classes cannot be overridden",
                        instance.class.span,
                    ));
                }
                let key = (id, ty.clone());
                if self.implementations.contains_key(&key) {
                    return Err(Diagnostic::new(
                        "E1016",
                        format!(
                            "overlapping instance for {} {}",
                            class.name,
                            ty.display(records)
                        ),
                        instance.class.span,
                    ));
                }
                let mut methods = BTreeMap::new();
                for definition in &instance.methods {
                    if !class
                        .methods
                        .iter()
                        .any(|method| method.name == definition.name.text)
                    {
                        return Err(Diagnostic::new(
                            "E1016",
                            format!(
                                "unknown method '{}' for {}",
                                definition.name.text, class.name
                            ),
                            definition.name.span,
                        ));
                    }
                    if methods
                        .insert(definition.name.text.clone(), definition)
                        .is_some()
                    {
                        return Err(duplicate(&definition.name));
                    }
                }
                let substitutions = BTreeMap::from([(class.variable.clone(), ty)]);
                let mut implementations = Vec::new();
                for method in &class.methods {
                    let definition = methods.get(&method.name).ok_or_else(|| {
                        Diagnostic::new(
                            "E1016",
                            format!("missing method '{}.{}'", class.name, method.name),
                            instance.class.span,
                        )
                    })?;
                    let mut definition = (**definition).clone();
                    while let ExprKind::Lambda(parameters, body) = definition.body.kind {
                        definition.parameters.extend(parameters);
                        definition.body = *body;
                    }
                    if definition.parameters.len() > method.signature.parameters.len()
                        || (definition.parameters.is_empty()
                            && !method.signature.parameters.is_empty())
                    {
                        return Err(Diagnostic::new(
                            "E1006",
                            format!(
                                "method '{}' expects {} parameters",
                                method.name,
                                method.signature.parameters.len()
                            ),
                            definition.name.span,
                        ));
                    }
                    let function_id = functions.len();
                    let parameters = definition
                        .parameters
                        .iter()
                        .zip(&method.signature.parameters)
                        .map(|((name, mutable), ty)| Parameter {
                            name: name.clone(),
                            mutable: *mutable,
                            ty: type_expression(
                                &substitute(ty, &substitutions),
                                records,
                                name.span,
                            ),
                        })
                        .collect();
                    functions.push((
                        (*module).into(),
                        FunctionDecl {
                            name: Ident {
                                text: format!("$instance.{function_id}.{}", method.name),
                                span: definition.name.span,
                            },
                            exported: false,
                            parameters,
                            result: type_expression(
                                &substitute(
                                    &method
                                        .signature
                                        .as_type()
                                        .after_arguments(definition.parameters.len()),
                                    &substitutions,
                                ),
                                records,
                                definition.name.span,
                            ),
                            constraints: Vec::new(),
                            body: definition.body.clone(),
                        },
                    ));
                    implementations.push(function_id);
                }
                self.implementations.insert(key, implementations);
            }
        }
        Ok(())
    }

    pub fn method(
        &self,
        expression: &Expr,
        module: &str,
        is_local: impl Fn(&str) -> bool,
    ) -> Result<Option<(usize, usize)>, Diagnostic> {
        let ExprKind::Field(value, method) = &expression.kind else {
            return Ok(None);
        };
        let class_name = match &value.kind {
            ExprKind::Name(name) if !is_local(&name.text) => name.text.clone(),
            ExprKind::Field(root, class) => match &root.kind {
                ExprKind::Name(name) if !is_local(&name.text) => {
                    format!("{}.{}", name.text, class.text)
                }
                _ => return Ok(None),
            },
            _ => return Ok(None),
        };
        let Some(class) = self.find(module, &class_name) else {
            return Ok(None);
        };
        let index = self.declarations[class]
            .methods
            .iter()
            .position(|candidate| candidate.name == method.text)
            .ok_or_else(|| {
                Diagnostic::new(
                    "E1002",
                    format!("type class '{class_name}' has no method '{}'", method.text),
                    method.span,
                )
            })?;
        Ok(Some((class, index)))
    }

    fn intrinsic(&self, class: usize, ty: &Type, records: &[CheckedRecord]) -> bool {
        if !self.declarations[class].builtin {
            return false;
        }
        match self.declarations[class].name.as_str() {
            "Add" => ty.is_numeric() || *ty == Type::String,
            "Sub" | "Mul" | "Div" | "Ord" | "Numeric" => ty.is_numeric(),
            "Rem" | "Bits" | "Integer" => ty.is_integer(),
            "SignedInteger" => matches!(ty, Type::Integer(_, true)),
            "Neg" => ty.is_float() || matches!(ty, Type::Integer(_, true)),
            "Float" => ty.is_float(),
            "Eq" => ty.is_scalar() || matches!(ty, Type::String | Type::Unit),
            "Copy" => ty.is_copy(records),
            "Capture" => ty.can_capture(records),
            "Send" => ty.can_send(records),
            _ => false,
        }
    }

    fn validate(
        &self,
        constraint: &Constraint,
        records: &[CheckedRecord],
    ) -> Result<(), Diagnostic> {
        bounded_type(&constraint.ty, constraint.span)?;
        if !variables(&constraint.ty).is_empty() {
            return Ok(());
        }
        require_concrete(&constraint.ty, constraint.span)?;
        if self.intrinsic(constraint.class, &constraint.ty, records)
            || self
                .implementations
                .contains_key(&(constraint.class, constraint.ty.clone()))
        {
            Ok(())
        } else {
            if self.declarations[constraint.class].name == "Capture" {
                return Err(Diagnostic::new(
                    "E1005",
                    format!(
                        "cannot capture {} in a reusable function; fully apply exclusive borrows and keep single-use tasks in task blocks",
                        constraint.ty.display(records)
                    ),
                    constraint.span,
                ));
            }
            if self.declarations[constraint.class].name == "Send" {
                return Err(Diagnostic::new(
                    "E1013",
                    format!(
                        "tasks require owned values; {} contains a reference",
                        constraint.ty.display(records)
                    ),
                    constraint.span,
                ));
            }
            Err(Diagnostic::new(
                "E1005",
                format!(
                    "no instance for {} {}; define an instance or use a supported type",
                    self.declarations[constraint.class].name,
                    constraint.ty.display(records)
                ),
                constraint.span,
            ))
        }
    }

    fn operation(&self, operation: Operation) -> (usize, usize) {
        let name = match operation {
            Operation::Binary(operator) => binary_class(operator),
            Operation::Unary(UnaryOp::Negate) => "Neg",
            Operation::Unary(UnaryOp::BitNot) => "Bits",
            Operation::Unary(UnaryOp::Not) => unreachable!(),
        };
        let class = self.names[name];
        let method = self.declarations[class].methods.iter().position(|method| matches!((method.operation, operation),
            (Some(Operation::Binary(a)), Operation::Binary(b)) if a == b)
            || matches!((method.operation, operation), (Some(Operation::Unary(a)), Operation::Unary(b)) if a == b)).unwrap();
        (class, method)
    }
}

fn type_expression(ty: &Type, records: &[CheckedRecord], span: Span) -> TypeExpr {
    let kind = match ty {
        Type::Variable(name) => TypeExprKind::Variable(name.clone()),
        Type::Array(ty) => TypeExprKind::Array(Box::new(type_expression(ty, records, span))),
        Type::List(ty) => TypeExprKind::List(Box::new(type_expression(ty, records, span))),
        Type::Task(ty) => TypeExprKind::Task(Box::new(type_expression(ty, records, span))),
        Type::Reference(ty, mutable) => {
            TypeExprKind::Reference(Box::new(type_expression(ty, records, span)), *mutable)
        }
        Type::Function(parameters, result) => TypeExprKind::Function(
            parameters
                .iter()
                .map(|ty| type_expression(ty, records, span))
                .collect(),
            Box::new(type_expression(result, records, span)),
        ),
        Type::Infer(_) => unreachable!("instance types are concrete"),
        _ => TypeExprKind::Named(ty.display(records)),
    };
    TypeExpr { kind, span }
}

impl Checker<'_> {
    pub(super) fn builtin(&mut self, builtin: Builtin) -> (TypedExprKind, Type) {
        let ty = builtin.signature().as_type();
        let substitutions = variables(&ty)
            .into_iter()
            .map(|name| (name, self.inference.fresh()))
            .collect();
        (
            TypedExprKind::Function(FunctionRef::Builtin(builtin)),
            substitute(&ty, &substitutions),
        )
    }

    pub(super) fn function(&mut self, id: usize) -> (TypedExprKind, Type) {
        let scheme = &self.signatures[id];
        if scheme.variables.is_empty() {
            return (
                TypedExprKind::Function(FunctionRef::User(id)),
                scheme.signature.as_type(),
            );
        }
        let types: Vec<_> = scheme
            .variables
            .iter()
            .map(|_| self.inference.fresh())
            .collect();
        let substitutions = scheme
            .variables
            .iter()
            .cloned()
            .zip(types.clone())
            .collect();
        (
            TypedExprKind::GenericFunction(id, types),
            substitute(&scheme.signature.as_type(), &substitutions),
        )
    }

    pub(super) fn method(
        &mut self,
        class: usize,
        method: usize,
        span: Span,
    ) -> (TypedExprKind, Type) {
        let declaration = &self.classes.declarations[class];
        let ty = self.inference.fresh();
        let substitutions = BTreeMap::from([(declaration.variable.clone(), ty.clone())]);
        self.constraints.push(Constraint {
            class,
            ty: ty.clone(),
            span,
        });
        (
            TypedExprKind::Method(class, method, ty),
            substitute(
                &declaration.methods[method].signature.as_type(),
                &substitutions,
            ),
        )
    }

    pub(super) fn require(&mut self, name: &str, ty: Type, span: Span) -> Result<(), Diagnostic> {
        let ty = self.inference.resolve(&ty);
        let constraint = Constraint {
            class: self.classes.names[name],
            ty,
            span,
        };
        let mut undetermined = false;
        map_type(&constraint.ty, &mut |ty| {
            undetermined |= matches!(ty, Type::Infer(_));
            ty.clone()
        });
        if !undetermined {
            self.classes.validate(&constraint, self.records)?;
        }
        self.constraints.push(constraint);
        Ok(())
    }

    pub(super) fn annotation(&mut self, expression: &TypeExpr) -> Result<Type, Diagnostic> {
        self.constraints.extend(self.classes.inline_constraints(
            expression,
            self.module,
            self.names,
        )?);
        let ty = resolve_type(expression, self.module, self.names)?;
        if variables(&ty)
            .iter()
            .any(|name| !self.type_parameters.contains(name))
        {
            return Err(Diagnostic::new(
                "E1015",
                "type annotation mentions a variable absent from this function's signature",
                expression.span,
            ));
        }
        Ok(ty)
    }

    pub(super) fn call_signature(
        &mut self,
        ty: &Type,
        count: usize,
        span: Span,
    ) -> Result<(Vec<Type>, Type), Diagnostic> {
        match self.inference.resolve(ty) {
            Type::Function(parameters, result)
                if parameters.len() >= count && (count != 0 || parameters.is_empty()) =>
            {
                let remaining = if parameters.len() == count {
                    *result
                } else {
                    Type::function(parameters[count..].to_vec(), *result)
                };
                Ok((parameters[..count].to_vec(), remaining))
            }
            Type::Function(parameters, result)
                if count > parameters.len() && matches!(*result, Type::Infer(_)) =>
            {
                let mut parameters = parameters;
                let extra: Vec<_> = (parameters.len()..count)
                    .map(|_| self.inference.fresh())
                    .collect();
                let output = self.inference.fresh();
                self.same(
                    &result,
                    &Type::function(extra.clone(), output.clone()),
                    span,
                )?;
                parameters.extend(extra);
                Ok((parameters, output))
            }
            Type::Function(parameters, _) => Err(Diagnostic::new(
                "E1006",
                format!(
                    "cannot apply {count} arguments to a function accepting {} arguments",
                    parameters.len()
                ),
                span,
            )),
            Type::Infer(id) => {
                let parameters: Vec<_> = (0..count).map(|_| self.inference.fresh()).collect();
                let result = self.inference.fresh();
                self.same(
                    &Type::Infer(id),
                    &Type::function(parameters.clone(), result.clone()),
                    span,
                )?;
                Ok((parameters, result))
            }
            _ => Err(Diagnostic::new(
                "E1005",
                "only a function value can be called",
                span,
            )),
        }
    }

    pub(super) fn finish(&mut self, body: &mut TypedExpr) -> Result<(), Diagnostic> {
        self.inference.apply_defaults();
        expression_types(body, &mut |ty, span| {
            let resolved = self.inference.resolve(ty);
            let mut ambiguous = false;
            map_type(&resolved, &mut |ty| {
                ambiguous |= matches!(ty, Type::Infer(_));
                ty.clone()
            });
            if ambiguous {
                return Err(Diagnostic::new(
                    "E1015",
                    "ambiguous polymorphic use; add a concrete type annotation or supply determining arguments",
                    span,
                ));
            }
            bounded_type(&resolved, span)?;
            *ty = resolved;
            Ok(())
        })?;
        for (class, ty, span) in captures(body, self.classes, |id| {
            self.signatures[id].signature.parameters.len()
        })? {
            self.require(class, ty, span)?;
        }
        for constraint in &mut self.constraints {
            constraint.ty = self.inference.resolve(&constraint.ty);
            self.classes.validate(constraint, self.records)?;
        }
        Ok(())
    }
}

fn captures(
    body: &mut TypedExpr,
    classes: &Classes,
    parameters: impl Fn(usize) -> usize,
) -> Result<Vec<(&'static str, Type, Span)>, Diagnostic> {
    let arity = |callee: &TypedExpr| match &callee.kind {
        TypedExprKind::Function(FunctionRef::User(id)) | TypedExprKind::GenericFunction(id, _) => {
            parameters(*id)
        }
        TypedExprKind::Function(FunctionRef::Builtin(_)) => 1,
        TypedExprKind::Lambda { parameters, .. } => parameters.len(),
        TypedExprKind::Method(class, method, ty)
            if classes.implementations.contains_key(&(*class, ty.clone())) =>
        {
            parameters(classes.implementations[&(*class, ty.clone())][*method])
        }
        _ => match &callee.ty {
            Type::Function(parameters, _) => parameters.len(),
            _ => unreachable!(),
        },
    };
    let mut immediate = Vec::new();
    walk(body, &mut |expression| {
        if let TypedExprKind::Binary(BinaryOp::Pipe, _, right) = &expression.kind {
            if let TypedExprKind::Call(callee, arguments) = &right.kind {
                if arguments.len() + 1 >= arity(callee) {
                    immediate.push(right.span);
                }
            }
        }
        Ok(())
    })?;
    let mut result = Vec::new();
    walk(body, &mut |expression| {
        match &expression.kind {
            TypedExprKind::Call(callee, arguments)
                if arguments.len() < arity(callee) && !immediate.contains(&expression.span) =>
            {
                result.extend(
                    arguments
                        .iter()
                        .map(|argument| ("Capture", argument.ty.clone(), argument.span)),
                )
            }
            TypedExprKind::Lambda { captures, body, .. } => {
                let class = if matches!(expression.ty, Type::Task(_)) {
                    "Send"
                } else {
                    "Capture"
                };
                result.extend(
                    captures
                        .iter()
                        .map(|capture| (class, capture.ty.clone(), expression.span)),
                );
                if class == "Send" {
                    result.push(("Send", body.ty.clone(), body.span));
                }
            }
            _ => {}
        }
        Ok(())
    })?;
    Ok(result)
}

fn walk(
    expression: &mut TypedExpr,
    f: &mut impl FnMut(&mut TypedExpr) -> Result<(), Diagnostic>,
) -> Result<(), Diagnostic> {
    use TypedExprKind::*;
    match &mut expression.kind {
        Unary(_, value)
        | Borrow(value, _)
        | Dereference(value)
        | Cast(value)
        | Field(value, _)
        | Length(value)
        | StringLength(value)
        | TaskRun(value)
        | TaskParallel(value) => walk(value, f)?,
        Binary(_, left, right)
        | Assign(left, right)
        | Index(left, right)
        | NewArray(left, right)
        | NewList(left, right) => {
            walk(left, f)?;
            walk(right, f)?;
        }
        Call(callee, arguments) => {
            walk(callee, f)?;
            for argument in arguments {
                walk(argument, f)?;
            }
        }
        Lambda { body, .. } => walk(body, f)?,
        Closure(_, captures) => {
            for capture in captures {
                walk(capture, f)?;
            }
        }
        If {
            condition,
            then_branch,
            else_branch,
        } => {
            walk(condition, f)?;
            walk(then_branch, f)?;
            walk(else_branch, f)?;
        }
        Block { bindings, result } => {
            for (_, value) in bindings {
                walk(value, f)?;
            }
            walk(result, f)?;
        }
        Record(fields) => {
            for (_, value) in fields {
                walk(value, f)?;
            }
        }
        Array(values) | List(values) => {
            for value in values {
                walk(value, f)?;
            }
        }
        _ => {}
    }
    f(expression)
}

fn expression_types(
    expression: &mut TypedExpr,
    f: &mut impl FnMut(&mut Type, Span) -> Result<(), Diagnostic>,
) -> Result<(), Diagnostic> {
    walk(expression, &mut |expression| {
        f(&mut expression.ty, expression.span)?;
        match &mut expression.kind {
            TypedExprKind::GenericFunction(_, types) => {
                for ty in types {
                    f(ty, expression.span)?;
                }
            }
            TypedExprKind::Method(_, _, ty) => f(ty, expression.span)?,
            TypedExprKind::Block { bindings, .. } => {
                for (local, _) in bindings {
                    f(&mut local.ty, local.span)?;
                }
            }
            TypedExprKind::Lambda {
                parameters,
                captures,
                ..
            } => {
                for local in parameters.iter_mut().chain(captures) {
                    f(&mut local.ty, local.span)?;
                }
            }
            _ => {}
        }
        Ok(())
    })
}

pub(super) fn specialize(
    mut module: CheckedModule,
    classes: &Classes,
    sizes: &[usize],
) -> Result<CheckedModule, Diagnostic> {
    let copy_constraints = crate::ownership::infer_copy(&module)?;
    for (function, copy_variables) in module.functions.iter_mut().zip(copy_constraints) {
        for name in copy_variables {
            function.constraints.push(Constraint {
                class: classes.names["Copy"],
                ty: Type::Variable(name),
                span: function.body.span,
            });
        }
        let mut seen = BTreeSet::new();
        let mut constraints = Vec::new();
        for constraint in std::mem::take(&mut function.constraints) {
            classes.validate(&constraint, &module.records)?;
            if !variables(&constraint.ty).is_empty()
                && seen.insert((constraint.class, constraint.ty.clone()))
            {
                if constraints.len() == MAX_CONSTRAINTS {
                    return Err(Diagnostic::new(
                        "E1017",
                        "too many distinct polymorphic constraints in one function",
                        constraint.span,
                    ));
                }
                constraints.push(constraint);
            }
        }
        function.constraints = constraints;
    }
    let mut dependencies = Vec::new();
    for function in &mut module.functions {
        let mut calls = Vec::new();
        walk(&mut function.body, &mut |expression| {
            match &expression.kind {
                TypedExprKind::GenericFunction(id, types) => {
                    calls.push((*id, types.clone(), expression.span))
                }
                TypedExprKind::Function(FunctionRef::User(id)) => {
                    calls.push((*id, Vec::new(), expression.span))
                }
                _ => {}
            }
            Ok(())
        })?;
        dependencies.push(calls);
    }
    // Constraints travel through recursive call graphs to a fixed point, independent of declaration order.
    loop {
        let mut changed = false;
        for (id, calls) in dependencies.iter().enumerate() {
            let mut inherited = Vec::new();
            for (callee, types, span) in calls {
                let callee = &module.functions[*callee];
                let substitutions = callee
                    .type_parameters
                    .iter()
                    .cloned()
                    .zip(types.clone())
                    .collect();
                for constraint in &callee.constraints {
                    inherited.push(Constraint {
                        class: constraint.class,
                        ty: substitute(&constraint.ty, &substitutions),
                        span: *span,
                    });
                }
            }
            let function = &mut module.functions[id];
            for constraint in inherited {
                classes.validate(&constraint, &module.records)?;
                if variables(&constraint.ty).is_empty() {
                    continue;
                }
                if !function.constraints.iter().any(|existing| {
                    existing.class == constraint.class && existing.ty == constraint.ty
                }) {
                    if function.constraints.len() >= MAX_CONSTRAINTS {
                        return Err(Diagnostic::new(
                            "E1017",
                            "constraint expansion exceeds the compiler limit; polymorphic recursion must not grow types",
                            constraint.span,
                        ));
                    }
                    function.constraints.push(constraint);
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }
    for function in &module.functions {
        for constraint in &function.constraints {
            classes.validate(constraint, &module.records)?;
        }
    }
    if let Some(entry) = module.entry {
        if !module.functions[entry].type_parameters.is_empty() {
            return Err(Diagnostic::new(
                "E1015",
                "the application entry point cannot be polymorphic",
                module.functions[entry].span,
            ));
        }
    }
    let mut specializer = Specializer {
        templates: module.functions.clone(),
        classes,
        records: &module.records,
        sizes,
        keys: BTreeMap::new(),
        requests: Vec::new(),
        functions: Vec::new(),
        intrinsics: BTreeMap::new(),
        base_count: module
            .functions
            .iter()
            .filter(|function| function.type_parameters.is_empty())
            .count(),
    };
    for (id, function) in module.functions.iter().enumerate() {
        if function.type_parameters.is_empty() {
            specializer.request(id, Vec::new(), function.span)?;
        }
    }
    let entry = module.entry.map(|id| specializer.keys[&(id, Vec::new())]);
    let mut next = 0;
    while next < specializer.requests.len() {
        let (id, types) = specializer.requests[next].clone();
        let function = specializer.instantiate(id, &types, next)?;
        specializer.functions.push(function);
        next += 1;
    }
    let functions = specializer.functions;
    Ok(CheckedModule {
        records: module.records,
        functions,
        entry,
    })
}

struct Specializer<'a> {
    templates: Vec<CheckedFunction>,
    classes: &'a Classes,
    records: &'a [CheckedRecord],
    sizes: &'a [usize],
    keys: BTreeMap<(usize, Vec<Type>), usize>,
    requests: Vec<(usize, Vec<Type>)>,
    functions: Vec<CheckedFunction>,
    intrinsics: BTreeMap<(usize, usize, Type), usize>,
    base_count: usize,
}

impl Specializer<'_> {
    fn request(&mut self, id: usize, types: Vec<Type>, span: Span) -> Result<usize, Diagnostic> {
        for ty in &types {
            require_concrete(ty, span)?;
        }
        let key = (id, types);
        if let Some(id) = self.keys.get(&key) {
            return Ok(*id);
        }
        if self.requests.len() >= self.base_count + MAX_SPECIALIZATIONS {
            return Err(Diagnostic::new(
                "E1017",
                format!(
                    "more than {MAX_SPECIALIZATIONS} specializations; remove type-growing polymorphic recursion"
                ),
                span,
            ));
        }
        let next = self.requests.len();
        self.requests.push(key.clone());
        self.keys.insert(key, next);
        Ok(next)
    }

    fn instantiate(
        &mut self,
        id: usize,
        types: &[Type],
        instance: usize,
    ) -> Result<CheckedFunction, Diagnostic> {
        let mut function = self.templates[id].clone();
        let substitutions: BTreeMap<_, _> = function
            .type_parameters
            .iter()
            .cloned()
            .zip(types.iter().cloned())
            .collect();
        if !types.is_empty() {
            function.name = format!("{}.$mono.{instance}", function.name);
        }
        for constraint in &function.constraints {
            self.classes.validate(
                &Constraint {
                    class: constraint.class,
                    ty: substitute(&constraint.ty, &substitutions),
                    span: constraint.span,
                },
                self.records,
            )?;
        }
        for parameter in &mut function.parameters {
            parameter.ty = substitute(&parameter.ty, &substitutions);
        }
        function.signature.parameters = function
            .signature
            .parameters
            .iter()
            .map(|ty| substitute(ty, &substitutions))
            .collect();
        function.signature.result = substitute(&function.signature.result, &substitutions);
        validate_size(&function.signature.as_type(), self.sizes, function.span)?;
        function
            .signature
            .validate_borrows(self.records, function.span)?;
        expression_types(&mut function.body, &mut |ty, span| {
            *ty = substitute(ty, &substitutions);
            require_concrete(ty, span)?;
            validate_size(ty, self.sizes, span)?;
            Ok(())
        })?;
        for (class, ty, span) in captures(&mut function.body, self.classes, |id| {
            self.templates[id].parameters.len()
        })? {
            self.classes.validate(
                &Constraint {
                    class: self.classes.names[class],
                    ty,
                    span,
                },
                self.records,
            )?;
        }
        walk(&mut function.body, &mut |expression| self.lower(expression))?;
        function.type_parameters.clear();
        function.constraints.clear();
        Ok(function)
    }

    fn lower(&mut self, expression: &mut TypedExpr) -> Result<(), Diagnostic> {
        use TypedExprKind::*;
        let replacement =
            match &expression.kind {
                Function(FunctionRef::User(id)) => Some(Function(FunctionRef::User(
                    self.request(*id, Vec::new(), expression.span)?,
                ))),
                GenericFunction(id, types) => Some(Function(FunctionRef::User(self.request(
                    *id,
                    types.clone(),
                    expression.span,
                )?))),
                Method(class, method, ty) => {
                    if let Some(implementations) =
                        self.classes.implementations.get(&(*class, ty.clone()))
                    {
                        Some(Function(FunctionRef::User(self.request(
                            implementations[*method],
                            Vec::new(),
                            expression.span,
                        )?)))
                    } else {
                        // Intrinsic method values get ordinary monomorphic wrappers as well.
                        let id = self.intrinsic_function(*class, *method, ty, expression.span)?;
                        Some(Function(FunctionRef::User(id)))
                    }
                }
                GenericInteger(value, negative) => Some(
                    Checker::integer_literal(
                        *value,
                        None,
                        Some(&expression.ty),
                        *negative,
                        expression.span,
                        self.records,
                    )?
                    .0,
                ),
                GenericFloat(value) => Some(Float(crate::numeric::float_literal(
                    value,
                    &expression.ty,
                    expression.span,
                )?)),
                Binary(operator, left, right)
                    if !matches!(operator, BinaryOp::And | BinaryOp::Or | BinaryOp::Pipe) =>
                {
                    let (class, method) = self.classes.operation(Operation::Binary(*operator));
                    if self.classes.intrinsic(class, &left.ty, self.records) {
                        None
                    } else {
                        Some(self.operator_call(
                            class,
                            method,
                            &left.ty,
                            vec![(**left).clone(), (**right).clone()],
                            expression.span,
                        )?)
                    }
                }
                Unary(operator, operand) if *operator != UnaryOp::Not => {
                    let (class, method) = self.classes.operation(Operation::Unary(*operator));
                    if self.classes.intrinsic(class, &operand.ty, self.records) {
                        None
                    } else {
                        Some(self.operator_call(
                            class,
                            method,
                            &operand.ty,
                            vec![(**operand).clone()],
                            expression.span,
                        )?)
                    }
                }
                _ => None,
            };
        if let Some(kind) = replacement {
            expression.kind = kind;
        }
        Ok(())
    }

    fn operator_call(
        &mut self,
        class: usize,
        method: usize,
        ty: &Type,
        arguments: Vec<TypedExpr>,
        span: Span,
    ) -> Result<TypedExprKind, Diagnostic> {
        let id = self.classes.implementations[&(class, ty.clone())][method];
        let signature = self.templates[id].signature.as_type();
        let id = self.request(id, Vec::new(), span)?;
        Ok(TypedExprKind::Call(
            Box::new(TypedExpr {
                kind: TypedExprKind::Function(FunctionRef::User(id)),
                ty: signature,
                span,
            }),
            arguments,
        ))
    }

    fn intrinsic_function(
        &mut self,
        class: usize,
        method: usize,
        ty: &Type,
        span: Span,
    ) -> Result<usize, Diagnostic> {
        let key = (class, method, ty.clone());
        if let Some(id) = self.intrinsics.get(&key) {
            return self.request(*id, Vec::new(), span);
        }
        let declaration = &self.classes.declarations[class];
        let method = &declaration.methods[method];
        let substitutions = BTreeMap::from([(declaration.variable.clone(), ty.clone())]);
        let signature = Signature {
            parameters: method
                .signature
                .parameters
                .iter()
                .map(|ty| substitute(ty, &substitutions))
                .collect(),
            result: substitute(&method.signature.result, &substitutions),
        };
        let parameters: Vec<_> = signature
            .parameters
            .iter()
            .enumerate()
            .map(|(id, ty)| Local {
                id,
                ty: ty.clone(),
                name: format!("arg{id}"),
                mutable: false,
                span,
            })
            .collect();
        let argument = |id: usize| {
            Box::new(TypedExpr {
                kind: TypedExprKind::Local(id),
                ty: parameters[id].ty.clone(),
                span,
            })
        };
        let kind = match method.operation.expect("intrinsic methods have operations") {
            Operation::Binary(operator) => {
                TypedExprKind::Binary(operator, argument(0), argument(1))
            }
            Operation::Unary(operator) => TypedExprKind::Unary(operator, argument(0)),
        };
        let id = self.templates.len();
        let body = TypedExpr {
            kind,
            ty: signature.result.clone(),
            span,
        };
        self.templates.push(CheckedFunction {
            module: "$intrinsic".into(),
            name: format!("{}.{}.{id}", declaration.name, method.name),
            exported: false,
            parameters,
            signature,
            body,
            span,
            type_parameters: Vec::new(),
            constraints: Vec::new(),
            capture_count: 0,
            is_task: false,
        });
        self.intrinsics.insert(key, id);
        self.request(id, Vec::new(), span)
    }
}
