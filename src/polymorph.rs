use super::*;

const MAX_SPECIALIZATIONS: usize = 1024;
const MAX_CONSTRAINTS: usize = 128;

#[derive(Clone, Debug)]
pub(super) struct Constraint {
    pub class: usize,
    pub ty: Type,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub(super) struct MemberConstraint {
    pub receiver: Type,
    pub name: String,
    pub module: String,
    pub signature: Option<Type>,
    pub span: Span,
}

impl MemberConstraint {
    fn resolved(&self, inference: &Inference) -> Self {
        Self {
            receiver: inference.resolve(&self.receiver),
            signature: self.signature.as_ref().map(|ty| inference.resolve(ty)),
            ..self.clone()
        }
    }

    fn substituted(&self, substitutions: &BTreeMap<String, Type>, span: Span) -> Self {
        Self {
            receiver: substitute(&self.receiver, substitutions),
            signature: self
                .signature
                .as_ref()
                .map(|ty| substitute(ty, substitutions)),
            span,
            ..self.clone()
        }
    }

    fn key(&self) -> (String, String, Type, Option<Type>) {
        (
            self.module.clone(),
            self.name.clone(),
            self.receiver.clone(),
            self.signature.clone(),
        )
    }

    fn determined(&self) -> bool {
        let mut inferred = false;
        for ty in std::iter::once(&self.receiver).chain(self.signature.iter()) {
            map_type(ty, &mut |ty| {
                inferred |= matches!(ty, Type::Infer(_));
                ty.clone()
            });
        }
        !inferred
    }
}

pub(super) struct Scheme {
    pub signature: Signature,
    pub variables: Vec<String>,
    pub constraints: Vec<Constraint>,
    pub members: Vec<MemberConstraint>,
}

impl Scheme {
    pub fn poisoned() -> Self {
        Self {
            signature: Signature {
                parameters: Vec::new(),
                result: Type::Error,
            },
            variables: Vec::new(),
            constraints: Vec::new(),
            members: Vec::new(),
        }
    }

    pub fn is_poisoned(&self) -> bool {
        self.signature.result == Type::Error
    }
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
        Type::Partial(partial) => Type::Partial(Box::new(Partial {
            constructor: partial.constructor.clone(),
            trailing: partial.trailing.iter().map(|ty| map_type(ty, f)).collect(),
        })),
        Type::Application(head, arguments) => higher_kinds::apply(
            map_type(head, f),
            arguments.iter().map(|ty| map_type(ty, f)).collect(),
        ),
        Type::Array(element) => Type::Array(Box::new(map_type(element, f))),
        Type::List(element) => Type::List(Box::new(map_type(element, f))),
        Type::Vec(element) => Type::Vec(Box::new(map_type(element, f))),
        Type::Tuple(elements) => Type::Tuple(elements.iter().map(|ty| map_type(ty, f)).collect()),
        Type::Task(result) => Type::Task(Box::new(map_type(result, f))),
        Type::Reference(value, mutable) => Type::Reference(Box::new(map_type(value, f)), *mutable),
        Type::Function(parameters, result) => Type::function(
            parameters.iter().map(|ty| map_type(ty, f)).collect(),
            map_type(result, f),
        ),
        Type::Record(id, args) if !args.is_empty() => {
            Type::Record(*id, args.iter().map(|ty| map_type(ty, f)).collect())
        }
        Type::Union(id, args) if !args.is_empty() => {
            Type::Union(*id, args.iter().map(|ty| map_type(ty, f)).collect())
        }
        _ => f(ty),
    }
}

pub(super) fn substitute(ty: &Type, substitutions: &BTreeMap<String, Type>) -> Type {
    map_type(ty, &mut |ty| match ty {
        Type::Variable(name) => substitutions
            .get(name)
            .cloned()
            .unwrap_or_else(|| ty.clone()),
        _ => ty.clone(),
    })
}

pub(super) fn bounded_type(ty: &Type, span: Span) -> Result<(), Diagnostic> {
    if ty.contains_error() {
        return Ok(());
    }
    fn visit(ty: &Type, depth: usize, count: &mut usize) -> bool {
        *count += 1;
        if depth > MAX_NESTING || *count > 4096 {
            return false;
        }
        match ty {
            Type::Partial(partial) => partial
                .trailing
                .iter()
                .all(|ty| visit(ty, depth + 1, count)),
            Type::Application(head, arguments) => {
                visit(head, depth + 1, count)
                    && arguments.iter().all(|ty| visit(ty, depth + 1, count))
            }
            Type::Array(ty)
            | Type::List(ty)
            | Type::Vec(ty)
            | Type::Task(ty)
            | Type::Reference(ty, _) => visit(ty, depth + 1, count),
            Type::Function(parameters, result) => {
                parameters.iter().all(|ty| visit(ty, depth + 1, count))
                    && visit(result, depth + 1, count)
            }
            Type::Tuple(elements) => elements.iter().all(|ty| visit(ty, depth + 1, count)),
            Type::Record(_, arguments) | Type::Union(_, arguments) => {
                arguments.iter().all(|ty| visit(ty, depth + 1, count))
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
    if ty.contains_error() {
        return Ok(());
    }
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

#[derive(Clone, Default)]
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
            Type::Partial(partial) => Type::Partial(Box::new(Partial {
                constructor: partial.constructor.clone(),
                trailing: partial.trailing.iter().map(|ty| self.resolve(ty)).collect(),
            })),
            Type::Application(head, arguments) => higher_kinds::apply(
                self.resolve(head),
                arguments.iter().map(|ty| self.resolve(ty)).collect(),
            ),
            Type::Array(element) => Type::Array(Box::new(self.resolve(element))),
            Type::List(element) => Type::List(Box::new(self.resolve(element))),
            Type::Vec(element) => Type::Vec(Box::new(self.resolve(element))),
            Type::Tuple(elements) => {
                Type::Tuple(elements.iter().map(|ty| self.resolve(ty)).collect())
            }
            Type::Task(result) => Type::Task(Box::new(self.resolve(result))),
            Type::Reference(value, mutable) => {
                Type::Reference(Box::new(self.resolve(value)), *mutable)
            }
            Type::Function(parameters, result) => Type::function(
                parameters.iter().map(|ty| self.resolve(ty)).collect(),
                self.resolve(result),
            ),
            Type::Record(id, args) => {
                Type::Record(*id, args.iter().map(|ty| self.resolve(ty)).collect())
            }
            Type::Union(id, args) => {
                Type::Union(*id, args.iter().map(|ty| self.resolve(ty)).collect())
            }
            _ => ty.clone(),
        }
    }

    pub fn unify(
        &mut self,
        actual: &Type,
        expected: &Type,
        types: &TypeContext<'_>,
        span: Span,
    ) -> Result<(), Diagnostic> {
        let actual = self.resolve(actual);
        let expected = self.resolve(expected);
        if actual == expected || actual.contains_error() || expected.contains_error() {
            return Ok(());
        }
        match (&actual, &expected) {
            (Type::Partial(left), Type::Partial(right))
                if left.constructor == right.constructor
                    && left.trailing.len() == right.trailing.len() =>
            {
                for (left, right) in left.trailing.iter().zip(&right.trailing) {
                    self.unify(left, right, types, span)?;
                }
                return Ok(());
            }
            (Type::Application(left, left_args), Type::Application(right, right_args))
                if left_args.len() == right_args.len() =>
            {
                self.unify(left, right, types, span)?;
                for (left, right) in left_args.iter().zip(right_args) {
                    self.unify(left, right, types, span)?;
                }
                return Ok(());
            }
            (Type::Application(head, arguments), concrete)
            | (concrete, Type::Application(head, arguments))
                if !matches!(concrete, Type::Infer(_)) =>
            {
                if let Some((constructor, complete)) = higher_kinds::decompose(concrete)
                    && complete.len() >= arguments.len()
                {
                    let partial = Type::Partial(Box::new(Partial {
                        constructor,
                        trailing: complete[arguments.len()..].into(),
                    }));
                    self.unify(head, &partial, types, span)?;
                    for (argument, actual) in arguments.iter().zip(&complete) {
                        self.unify(argument, actual, types, span)?;
                    }
                    return Ok(());
                }
                return Err(Diagnostic::new(
                    "E1015",
                    "type constructor application has an incompatible kind",
                    span,
                ));
            }
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
            | (Type::Vec(a), Type::Vec(b))
            | (Type::Task(a), Type::Task(b)) => {
                return self.unify(a, b, types, span);
            }
            (Type::Reference(a, n), Type::Reference(b, m)) if n == m => {
                return self.unify(a, b, types, span);
            }
            (Type::Tuple(a), Type::Tuple(b)) if a.len() == b.len() => {
                for (a, b) in a.iter().zip(b) {
                    self.unify(a, b, types, span)?;
                }
                return Ok(());
            }
            (Type::Record(a_id, a), Type::Record(b_id, b))
            | (Type::Union(a_id, a), Type::Union(b_id, b))
                if a_id == b_id && a.len() == b.len() =>
            {
                let inferred = |ty: &Type| {
                    let mut inferred = false;
                    map_type(ty, &mut |ty| {
                        inferred |= matches!(ty, Type::Infer(_));
                        ty.clone()
                    });
                    inferred
                };
                for (a, b) in a.iter().zip(b) {
                    self.unify(a, b, types, span).map_err(|mut error| {
                        // Report the whole type when both sides were known before unification;
                        // partially inferred arguments would otherwise show intermediate bindings.
                        if error.code == "E1003" && !inferred(&actual) && !inferred(&expected) {
                            error.message = format!(
                                "expected {}, found {}",
                                expected.display(types),
                                actual.display(types)
                            );
                        }
                        error
                    })?;
                }
                return Ok(());
            }
            (Type::Function(a, x), Type::Function(b, y)) if a.is_empty() == b.is_empty() => {
                for (a, b) in a.iter().zip(b) {
                    self.unify(a, b, types, span)?;
                }
                let common = a.len().min(b.len());
                let tail = |parameters: &[Type], result: &Type| {
                    if parameters.len() == common {
                        result.clone()
                    } else {
                        Type::function(parameters[common..].to_vec(), result.clone())
                    }
                };
                return self.unify(&tail(a, x), &tail(b, y), types, span);
            }
            _ => {}
        }
        Err(Diagnostic::new(
            "E1003",
            format!(
                "expected {}, found {}",
                expected.display(types),
                actual.display(types)
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
    Builtin(Builtin),
}

struct Method {
    name: String,
    signature: Result<Signature, Diagnostic>,
    operation: Option<Operation>,
    default: Option<usize>,
}

impl Method {
    fn signature(&self, span: Span) -> Result<&Signature, Diagnostic> {
        self.signature.as_ref().map_err(|error| {
            let mut error = error.clone();
            error.span = span;
            error
        })
    }
}

struct Class {
    name: String,
    variable: String,
    arity: usize,
    superclasses: Vec<Constraint>,
    methods: Vec<Method>,
    builtin: bool,
}

pub(super) struct Classes {
    declarations: Vec<Class>,
    names: BTreeMap<String, usize>,
    instances: Vec<InstanceTemplate>,
}

struct InstanceTemplate {
    class: usize,
    head: Type,
    constraints: Vec<Constraint>,
    methods: Vec<(usize, Vec<Type>)>,
    span: Span,
    derived: bool,
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

/// Built-in class names; they share the type namespace with record types.
pub(super) const BUILTIN_CLASSES: [&str; 27] = [
    "SimdVector",
    "SimdNumeric",
    "SimdMask",
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
    "UnsignedInteger",
    "Float",
    "Numeric",
    "Copy",
    "Capture",
    "Send",
    "Display",
    "Parse",
    "Hash",
    "Default",
    "Elementary",
    "Drop",
    "Format",
];

impl Classes {
    pub fn collect(
        modules: &[ModuleInput<'_>],
        names: &Names,
        types: &TypeContext<'_>,
        diagnostics: &mut Diagnostics,
    ) -> Result<Self, Diagnostic> {
        use BinaryOp::*;
        let mut classes = Self {
            declarations: Vec::new(),
            names: BTreeMap::new(),
            instances: Vec::new(),
        };
        for name in BUILTIN_CLASSES {
            classes
                .names
                .insert(name.into(), classes.declarations.len());
            classes.declarations.push(Class {
                name: name.into(),
                variable: "a".into(),
                arity: 0,
                superclasses: Vec::new(),
                methods: Vec::new(),
                builtin: true,
            });
        }
        classes.declarations[classes.names["Ord"]]
            .superclasses
            .push(Constraint {
                class: classes.names["Eq"],
                ty: Type::Variable("a".into()),
                span: Span::default(),
            });
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
            let value = Type::Variable("a".into());
            let comparison = matches!(
                operator,
                Equal | NotEqual | Less | LessEqual | Greater | GreaterEqual
            );
            let result = if comparison {
                Type::Bool
            } else {
                value.clone()
            };
            let parameter = if comparison {
                Type::Reference(Box::new(value), false)
            } else {
                value
            };
            classes.declarations[class].methods.push(Method {
                name: name.into(),
                signature: Ok(Signature {
                    parameters: vec![parameter.clone(), parameter],
                    result,
                }),
                operation: Some(Operation::Binary(operator)),
                default: None,
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
                signature: Ok(Signature {
                    parameters: vec![a.clone()],
                    result: a,
                }),
                operation: Some(Operation::Unary(operator)),
                default: None,
            });
        }
        let a = Type::Variable("a".into());
        let option = names
            .std_type("Option", "Option", vec![a.clone()].into(), Span::default())
            .and_then(|ty| {
                if let Type::Union(id, args) = &ty {
                    let union = &types.unions[*id];
                    let cases = types.union_payloads(*id, args);
                    if union.cases.len() == 2
                        && union.cases.iter().zip(cases).all(|((name, _), payload)| {
                            (name == "None" && payload.is_none())
                                || (name == "Some" && payload == Some(a.clone()))
                        })
                    {
                        return Ok(ty);
                    }
                }
                Err(Diagnostic::new(
                    "E1004",
                    "Parse needs the standard union Option<'a> = None | Some of 'a",
                    Span::default(),
                ))
            });
        // Custom std inputs may omit Option. Only using Parse then reports
        // its unavailable signature; unrelated programs still type-check.
        for (class, name, builtin, signature) in [
            (
                "Display",
                "display",
                Builtin::Display,
                Ok(Signature {
                    parameters: vec![Type::Reference(Box::new(a), false)],
                    result: Type::String,
                }),
            ),
            (
                "Parse",
                "parse",
                Builtin::Parse,
                option.map(|result| Signature {
                    parameters: vec![Type::Reference(Box::new(Type::String), false)],
                    result,
                }),
            ),
        ] {
            let class = classes.names[class];
            classes.declarations[class].methods.push(Method {
                name: name.into(),
                signature,
                operation: Some(Operation::Builtin(builtin)),
                default: None,
            });
        }
        classes.declarations[classes.names["Default"]]
            .methods
            .push(Method {
                name: "default".into(),
                signature: Ok(Signature {
                    parameters: Vec::new(),
                    result: Type::Variable("a".into()),
                }),
                operation: Some(Operation::Builtin(Builtin::Default)),
                default: None,
            });
        classes.declarations[classes.names["Hash"]]
            .methods
            .push(Method {
                name: "hash".into(),
                signature: Ok(Signature {
                    parameters: vec![Type::Reference(Box::new(Type::Variable("a".into())), false)],
                    result: Type::Integer(64, false),
                }),
                operation: Some(Operation::Builtin(Builtin::Hash)),
                default: None,
            });
        // `Format.format value spec` shows a value under the text of a validated
        // `{value:spec}` hole; only user instances implement it.
        classes.declarations[classes.names["Format"]]
            .methods
            .push(Method {
                name: "format".into(),
                signature: Ok(Signature {
                    parameters: vec![
                        Type::Reference(Box::new(Type::Variable("a".into())), false),
                        Type::Reference(Box::new(Type::String), false),
                    ],
                    result: Type::String,
                }),
                operation: None,
                default: None,
            });
        classes.declarations[classes.names["Drop"]]
            .methods
            .push(Method {
                name: "drop".into(),
                signature: Ok(Signature {
                    parameters: vec![Type::Reference(Box::new(Type::Variable("a".into())), true)],
                    result: Type::Unit,
                }),
                operation: None,
                default: None,
            });
        for &ModuleInput {
            name: module,
            program,
            ..
        } in modules
        {
            for declaration in &program.classes {
                if diagnostics.is_full() {
                    break;
                }
                let collected = (|| {
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
                    let arity = higher_kinds::arity(&declaration.kind, declaration.variable.span)?;
                    let kinds = BTreeMap::from([(declaration.variable.text.clone(), arity)]);
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
                                .map(|ty| resolve_type_with_kinds(ty, module, names, &kinds))
                                .collect::<Result<_, _>>()?,
                            result: resolve_type_with_kinds(&method.result, module, names, &kinds)?,
                        };
                        validate_size(&signature.as_type(), types, method.name.span)?;
                        bounded_type(&signature.as_type(), method.name.span)?;
                        let signature_variables = variables(&signature.as_type());
                        if !signature_variables.contains(&declaration.variable.text)
                            || (arity == 0
                                && signature_variables != [declaration.variable.text.clone()])
                        {
                            return Err(Diagnostic::new(
                                "E1016",
                                "each class method must mention exactly the declared class type variable",
                                method.name.span,
                            ));
                        }
                        methods.push(Method {
                            name: method.name.text.clone(),
                            signature: Ok(signature),
                            operation: None,
                            default: None,
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
                        arity,
                        superclasses: Vec::new(),
                        methods,
                        builtin: false,
                    });
                    Ok(())
                })();
                if let Err(error) = collected {
                    diagnostics.push(error);
                }
            }
            if diagnostics.is_full() {
                break;
            }
        }
        diagnostics.check()?;
        for input in modules {
            for declaration in &input.program.classes {
                let id = classes.names[&format!("{}.{}", input.name, declaration.name.text)];
                let constraints =
                    classes.resolve_constraints(&declaration.superclasses, input.name, names)?;
                if constraints.iter().any(|constraint| matches!(&constraint.ty, Type::Variable(name) if name == &declaration.variable.text) && classes.declarations[constraint.class].arity != classes.declarations[id].arity) {
                    return Err(Diagnostic::new("E1015", "superclass parameter kind does not match the class parameter", declaration.variable.span));
                }
                if constraints.iter().any(|constraint| {
                    variables(&constraint.ty)
                        .iter()
                        .any(|variable| variable != &declaration.variable.text)
                }) {
                    return Err(Diagnostic::new(
                        "E1027",
                        "superclasses may mention only the class type variable",
                        declaration.name.span,
                    ));
                }
                classes.declarations[id].superclasses = constraints;
            }
        }
        classes.validate_superclasses()?;
        Ok(classes)
    }

    pub(super) fn constraint_kinds(
        &self,
        expressions: &[ConstraintExpr],
        module: &str,
        names: &Names,
    ) -> Result<BTreeMap<String, usize>, Diagnostic> {
        let mut kinds = BTreeMap::new();
        for expression in expressions {
            if let (ConstraintName::Class(name), TypeExprKind::Variable(variable)) =
                (&expression.name, &expression.ty.kind)
            {
                let class = self.resolve(names, module, name)?;
                let arity = self.declarations[class].arity;
                if kinds
                    .insert(variable.clone(), arity)
                    .is_some_and(|previous| previous != arity)
                {
                    return Err(Diagnostic::new(
                        "E1015",
                        "type variable is constrained at incompatible kinds",
                        expression.ty.span,
                    ));
                }
            }
        }
        Ok(kinds)
    }

    pub(super) fn constraint_type(
        &self,
        expression: &ConstraintExpr,
        module: &str,
        names: &Names,
        kinds: &BTreeMap<String, usize>,
    ) -> Result<Type, Diagnostic> {
        if let ConstraintName::Class(name) = &expression.name {
            let class = self.resolve(names, module, name)?;
            if self.declarations[class].arity > 0 {
                return higher_kinds::resolve_constructor(
                    &expression.ty,
                    self.declarations[class].arity,
                    module,
                    names,
                );
            }
        }
        resolve_type_with_kinds(&expression.ty, module, names, kinds)
    }

    fn resolve_constraints(
        &self,
        expressions: &[ConstraintExpr],
        module: &str,
        names: &Names,
    ) -> Result<Vec<Constraint>, Diagnostic> {
        if expressions.len() > MAX_CONSTRAINTS {
            return Err(Diagnostic::new(
                "E1017",
                "too many class constraints",
                expressions[MAX_CONSTRAINTS].ty.span,
            ));
        }
        expressions
            .iter()
            .map(|expression| {
                let ConstraintName::Class(name) = &expression.name else {
                    return Err(Diagnostic::new(
                        "E1027",
                        "class contexts require type class constraints",
                        expression.ty.span,
                    ));
                };
                let class = self.resolve(names, module, name).map_err(|mut error| {
                    error.code = "E1027";
                    error
                })?;
                let ty = higher_kinds::resolve_constructor(
                    &expression.ty,
                    self.declarations[class].arity,
                    module,
                    names,
                )?;
                bounded_type(&ty, expression.ty.span)?;
                Ok(Constraint {
                    class,
                    ty,
                    span: name.span,
                })
            })
            .collect()
    }

    fn validate_superclasses(&self) -> Result<(), Diagnostic> {
        let mut states = vec![0; self.declarations.len()];
        for root in 0..states.len() {
            let mut pending = vec![(root, false, 0, Span::default())];
            while let Some((class, finished, depth, span)) = pending.pop() {
                if finished {
                    states[class] = 2;
                    continue;
                }
                if states[class] == 2 {
                    continue;
                }
                if states[class] == 1 {
                    return Err(Diagnostic::new(
                        "E1027",
                        "superclass constraints form a cycle",
                        span,
                    ));
                }
                if depth >= 64 {
                    return Err(Diagnostic::new(
                        "E1017",
                        "superclass resolution exceeds depth 64",
                        span,
                    ));
                }
                states[class] = 1;
                pending.push((class, true, depth, span));
                pending.extend(
                    self.declarations[class]
                        .superclasses
                        .iter()
                        .rev()
                        .map(|constraint| (constraint.class, false, depth + 1, constraint.span)),
                );
            }
        }
        Ok(())
    }

    fn superclasses(&self, constraint: &Constraint) -> Result<Vec<Constraint>, Diagnostic> {
        let mut pending = vec![(constraint.clone(), 0)];
        let mut result = Vec::new();
        let mut seen = BTreeSet::new();
        while let Some((current, depth)) = pending.pop() {
            if depth >= 64 || result.len() >= MAX_CONSTRAINTS {
                return Err(Diagnostic::new(
                    "E1017",
                    "superclass constraint expansion exceeds the compiler limit",
                    constraint.span,
                ));
            }
            let class = &self.declarations[current.class];
            let substitutions = BTreeMap::from([(class.variable.clone(), current.ty)]);
            for superclass in &class.superclasses {
                let ty = substitute(&superclass.ty, &substitutions);
                bounded_type(&ty, constraint.span)?;
                if seen.insert((superclass.class, ty.clone())) {
                    let next = Constraint {
                        class: superclass.class,
                        ty,
                        span: constraint.span,
                    };
                    result.push(next.clone());
                    pending.push((next, depth + 1));
                }
            }
        }
        Ok(result)
    }

    /// Finds a class through the tiered name lookup of GUIDE D-07. A class
    /// that `collect` has not registered yet is unknown.
    fn find(
        &self,
        names: &Names,
        module: &str,
        name: &str,
        span: Span,
    ) -> Result<Option<usize>, Diagnostic> {
        Ok(names
            .class(module, name, span)?
            .and_then(|key| self.names.get(key).copied()))
    }

    pub fn resolve(&self, names: &Names, module: &str, name: &Ident) -> Result<usize, Diagnostic> {
        self.find(names, module, &name.text, name.span)?
            .ok_or_else(|| {
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
        if names.type_aliases.is_empty() {
            return self.expanded_inline_constraints(expression, module, names);
        }
        let expanded = expand_type_aliases(expression, module, names)?;
        self.expanded_inline_constraints(&expanded, module, names)
    }

    fn expanded_inline_constraints(
        &self,
        expression: &TypeExpr,
        module: &str,
        names: &Names,
    ) -> Result<Vec<Constraint>, Diagnostic> {
        let mut constraints = Vec::new();
        match &expression.kind {
            TypeExprKind::Apply(head, arguments)
                if crate::numeric::primitive(&head.text).is_none() =>
            {
                if head.text != "Vec"
                    && !head.text.starts_with('\'')
                    && let TypeHead::Class = names.type_head(module, head)?
                {
                    // `resolve_type` reports a class applied to zero or several types.
                    let [ty] = &**arguments else {
                        return Ok(constraints);
                    };
                    constraints.push(Constraint {
                        class: self.resolve(names, module, head)?,
                        ty: resolve_type(ty, module, names)?,
                        span: head.span,
                    });
                }
                for ty in arguments {
                    constraints.extend(self.expanded_inline_constraints(ty, module, names)?);
                }
            }
            TypeExprKind::Array(ty)
            | TypeExprKind::List(ty)
            | TypeExprKind::Task(ty)
            | TypeExprKind::Reference(ty, _)
            | TypeExprKind::Regions(ty, _) => {
                constraints.extend(self.expanded_inline_constraints(ty, module, names)?)
            }
            TypeExprKind::Function(parameters, result) => {
                for ty in parameters.iter().chain(std::iter::once(result.as_ref())) {
                    constraints.extend(self.expanded_inline_constraints(ty, module, names)?);
                }
            }
            TypeExprKind::Tuple(elements) => {
                for ty in elements {
                    constraints.extend(self.expanded_inline_constraints(ty, module, names)?);
                }
            }
            _ => {}
        }
        Ok(constraints)
    }

    pub fn instances(
        &mut self,
        modules: &[ModuleInput<'_>],
        names: &Names,
        types: &TypeContext<'_>,
        functions: &mut Vec<(String, FunctionDecl)>,
        diagnostics: &mut Diagnostics,
    ) -> Result<(), Diagnostic> {
        let derived = deriving::instances(modules, names, types)?;
        for input in modules {
            for declaration in &input.program.classes {
                let id = self.names[&format!("{}.{}", input.name, declaration.name.text)];
                let class = &mut self.declarations[id];
                for definition in &declaration.defaults {
                    let method = class
                        .methods
                        .iter_mut()
                        .find(|method| method.name == definition.name.text)
                        .ok_or_else(|| {
                            Diagnostic::new(
                                "E1016",
                                "a default body must name a declared class method",
                                definition.name.span,
                            )
                        })?;
                    if method.default.is_some() {
                        return Err(duplicate(&definition.name));
                    }
                    let mut constraints = declaration.superclasses.clone();
                    constraints.push(ConstraintExpr {
                        name: ConstraintName::Class(Ident {
                            text: class.name.clone(),
                            span: definition.name.span,
                            provenance: Provenance::Generated,
                        }),
                        ty: TypeExpr {
                            kind: TypeExprKind::Variable(class.variable.clone()),
                            span: definition.name.span,
                        },
                    });
                    let function = functions.len();
                    functions.push((
                        input.name.to_owned(),
                        instance_function(
                            method.signature(definition.name.span)?,
                            &BTreeMap::new(),
                            definition,
                            format!("$instance.default.{function}.{}", method.name),
                            constraints,
                            types,
                        )?,
                    ));
                    method.default = Some(function);
                }
            }
        }
        let mut overlap_pairs = 0;
        for &ModuleInput {
            name: module,
            program,
            ..
        } in modules
        {
            for instance in program.instances.iter().chain(
                derived
                    .iter()
                    .filter(|(owner, _)| owner == module)
                    .map(|(_, instance)| instance),
            ) {
                if diagnostics.is_full() {
                    break;
                }
                let collected = (|| {
                    let id = self.resolve(names, module, &instance.class)?;
                    let class = &self.declarations[id];
                    let ty = higher_kinds::resolve_constructor(
                        &instance.ty,
                        class.arity,
                        module,
                        names,
                    )?;
                    if class.arity > 0 && matches!(ty, Type::Variable(_)) {
                        return Err(Diagnostic::new(
                            "E1015",
                            "higher-kinded instances require a named constructor head",
                            instance.ty.span,
                        ));
                    }
                    bounded_type(&ty, instance.ty.span)?;
                    let type_parameters = variables(&ty);
                    let context = self.resolve_constraints(&instance.constraints, module, names)?;
                    if context.iter().any(|constraint| {
                        variables(&constraint.ty)
                            .iter()
                            .any(|variable| !type_parameters.contains(variable))
                    }) {
                        return Err(Diagnostic::new(
                            "E1027",
                            "instance context mentions a type variable absent from its head",
                            instance.class.span,
                        ));
                    }
                    if class.builtin
                        && (class.methods.is_empty()
                            || matches!(ty, Type::Variable(_))
                            || self.intrinsic(id, &ty, types)
                            || self.structural(id, &ty))
                    {
                        return Err(Diagnostic::new(
                            "E1016",
                            "built-in instances and marker classes cannot be overridden",
                            instance.class.span,
                        ));
                    }
                    if class.builtin && class.name == "Drop" {
                        validate_drop_instance(
                            &ty,
                            !instance.constraints.is_empty(),
                            types,
                            instance.class.span,
                        )?;
                    }
                    if class.builtin && class.name == "Format" {
                        validate_format_instance(&ty, types, instance.class.span)?;
                    }
                    for previous in self
                        .instances
                        .iter()
                        .filter(|previous| previous.class == id)
                    {
                        overlap_pairs += 1;
                        if overlap_pairs > 1024 {
                            return Err(Diagnostic::new(
                                "E1017",
                                "instance overlap checking exceeds 1024 pairs",
                                instance.class.span,
                            ));
                        }
                        let mut inference = Inference::default();
                        let mut fresh = |head: &Type| {
                            let substitutions = variables(head)
                                .into_iter()
                                .map(|name| (name, inference.fresh()))
                                .collect();
                            substitute(head, &substitutions)
                        };
                        let left = fresh(&previous.head);
                        let right = fresh(&ty);
                        if inference
                            .unify(&left, &right, types, instance.class.span)
                            .is_ok()
                        {
                            return Err(Diagnostic::new(
                                "E1016",
                                format!(
                                    "overlapping instance for {}<{}>",
                                    class.name,
                                    ty.display(types)
                                ),
                                instance.class.span,
                            ));
                        }
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
                    let substitutions = BTreeMap::from([(class.variable.clone(), ty.clone())]);
                    let mut implementations = Vec::new();
                    for method in &class.methods {
                        let signature = method.signature(instance.class.span)?;
                        let Some(definition) = methods.get(&method.name) else {
                            if let Some(function) = method.default {
                                let arguments = variables(&signature.as_type())
                                    .into_iter()
                                    .map(|variable| {
                                        if variable == class.variable {
                                            ty.clone()
                                        } else {
                                            Type::Variable(variable)
                                        }
                                    })
                                    .collect();
                                implementations.push((function, arguments));
                                continue;
                            }
                            return Err(Diagnostic::new(
                                "E1016",
                                format!("missing method '{}.{}'", class.name, method.name),
                                instance.class.span,
                            ));
                        };
                        let function_id = functions.len();
                        let renamed: BTreeMap<_, _> = variables(&signature.as_type())
                            .into_iter()
                            .filter(|variable| {
                                variable != &class.variable && type_parameters.contains(variable)
                            })
                            .map(|variable| {
                                let unique =
                                    Type::Variable(format!("$method.{function_id}.{variable}"));
                                (variable, unique)
                            })
                            .collect();
                        let signature = Signature {
                            parameters: signature
                                .parameters
                                .iter()
                                .map(|ty| substitute(ty, &renamed))
                                .collect(),
                            result: substitute(&signature.result, &renamed),
                        };
                        functions.push((
                            (*module).into(),
                            instance_function(
                                &signature,
                                &substitutions,
                                definition,
                                format!("$instance.{function_id}.{}", method.name),
                                instance.constraints.clone(),
                                types,
                            )?,
                        ));
                        implementations.push((
                            function_id,
                            variables(&substitute(&signature.as_type(), &substitutions))
                                .into_iter()
                                .map(Type::Variable)
                                .collect(),
                        ));
                    }
                    self.instances.push(InstanceTemplate {
                        class: id,
                        head: ty,
                        constraints: context,
                        methods: implementations,
                        span: instance.class.span,
                        derived: instance.class.provenance == Provenance::Generated,
                    });
                    Ok(())
                })();
                if let Err(error) = collected {
                    diagnostics.push(error);
                }
            }
            if diagnostics.is_full() {
                break;
            }
        }
        diagnostics.check()
    }

    /// Heads of the `Drop` instances: the records and unions with a user drop (B07).
    pub(super) fn drop_heads(&self) -> impl Iterator<Item = &Type> {
        let drop = self.names["Drop"];
        self.instances
            .iter()
            .filter(move |instance| instance.class == drop)
            .map(|instance| &instance.head)
    }

    /// Checks that every instance context entails the class's superclasses. It runs after the
    /// `Drop` marks are set, so `Copy`-like superclasses see the final type properties.
    pub fn check_superclasses(
        &self,
        types: &TypeContext<'_>,
        diagnostics: &mut Diagnostics,
    ) -> Result<(), Diagnostic> {
        for instance in &self.instances {
            let constraint = Constraint {
                class: instance.class,
                ty: instance.head.clone(),
                span: instance.span,
            };
            for superclass in self.superclasses(&constraint)? {
                let checked = self.normalize(&superclass, types).and_then(|required| {
                    for obligation in required {
                        if !self.entails(&instance.constraints, &obligation, types)? {
                            return Err(Diagnostic::new("E1027", "instance context does not entail its superclass; add the required constraint or instance", instance.span));
                        }
                    }
                    Ok(())
                });
                if let Err(mut error) = checked {
                    if error.code != "E1017" {
                        error.code = if instance.derived { "E1025" } else { "E1027" };
                    }
                    diagnostics.push(error);
                }
            }
        }
        diagnostics.check()
    }

    fn matching_instance(
        &self,
        class: usize,
        ty: &Type,
        types: &TypeContext<'_>,
    ) -> Option<(&InstanceTemplate, BTreeMap<String, Type>)> {
        // The fresh inference below knows nothing of the caller's variables.
        let mut open = false;
        map_type(ty, &mut |ty| {
            open |= matches!(ty, Type::Infer(_));
            ty.clone()
        });
        if open {
            return None;
        }
        self.instances
            .iter()
            .filter(|instance| instance.class == class)
            .find_map(|instance| {
                let mut inference = Inference::default();
                let substitutions: BTreeMap<_, _> = variables(&instance.head)
                    .into_iter()
                    .map(|name| (name, inference.fresh()))
                    .collect();
                inference
                    .unify(
                        &substitute(&instance.head, &substitutions),
                        ty,
                        types,
                        instance.span,
                    )
                    .ok()?;
                let substitutions = substitutions
                    .into_iter()
                    .map(|(name, ty)| (name, inference.resolve(&ty)))
                    .collect();
                Some((instance, substitutions))
            })
    }

    pub(super) fn derived_error(&self, function: usize, mut error: Diagnostic) -> Diagnostic {
        if matches!(error.code, "E1005" | "E1027") {
            if let Some(instance) = self.instances.iter().find(|instance| {
                instance.derived
                    && instance
                        .methods
                        .iter()
                        .any(|(method, _)| *method == function)
            }) {
                error.code = "E1025";
                error.span = instance.span;
            }
        }
        error
    }

    fn resolved_method(
        &self,
        class: usize,
        method: usize,
        ty: &Type,
        types: &TypeContext<'_>,
    ) -> Option<(usize, Vec<Type>)> {
        let (instance, substitutions) = self.matching_instance(class, ty, types)?;
        let (function, arguments) = &instance.methods[method];
        Some((
            *function,
            arguments
                .iter()
                .map(|ty| substitute(ty, &substitutions))
                .collect(),
        ))
    }

    fn normalize(
        &self,
        constraint: &Constraint,
        types: &TypeContext<'_>,
    ) -> Result<Vec<Constraint>, Diagnostic> {
        let mut pending = vec![(constraint.clone(), 0, false)];
        let mut residual = Vec::new();
        let mut seen = BTreeSet::new();
        let mut active = BTreeMap::new();
        let mut completed = BTreeSet::new();
        let mut work = 0;
        while let Some((current, depth, finished)) = pending.pop() {
            if current.ty.contains_error() {
                continue;
            }
            let key = (current.class, current.ty.clone());
            if finished {
                active.remove(&key);
                completed.insert(key);
                continue;
            }
            if completed.contains(&key) {
                continue;
            }
            if let Some(&(start, _)) = active.get(&key) {
                let cycle: Vec<_> = active
                    .iter()
                    .filter(|(_, (depth, _))| *depth >= start)
                    .collect();
                let structural = cycle.iter().any(|(_, (_, derived))| *derived);
                if structural && self.declarations[current.class].name == "Default" {
                    return Err(Diagnostic::new(
                        "E1025",
                        "derived Default follows a recursive first case; implement a terminating Default manually",
                        current.span,
                    ));
                }
                if structural && cycle.iter().all(|((class, _), _)| *class == current.class) {
                    continue;
                }
                return Err(Diagnostic::new(
                    "E1017",
                    "instance constraint resolution forms a cycle",
                    current.span,
                ));
            }
            let derived = self
                .matching_instance(current.class, &current.ty, types)
                .is_some_and(|(instance, _)| instance.derived);
            active.insert(key, (depth, derived));
            work += 1;
            if depth >= 64 || work > MAX_CONSTRAINTS {
                return Err(Diagnostic::new(
                    "E1017",
                    "instance constraint resolution exceeds depth 64 or 128 obligations",
                    current.span,
                ));
            }
            bounded_type(&current.ty, current.span)?;
            pending.push((current.clone(), depth, true));
            let class = &self.declarations[current.class];
            let substitutions = BTreeMap::from([(class.variable.clone(), current.ty.clone())]);
            for superclass in &class.superclasses {
                pending.push((
                    Constraint {
                        class: superclass.class,
                        ty: substitute(&superclass.ty, &substitutions),
                        span: current.span,
                    },
                    depth + 1,
                    false,
                ));
            }
            if self.structural(current.class, &current.ty) {
                let elements = match &current.ty {
                    Type::Array(element) | Type::List(element) => {
                        std::slice::from_ref(element.as_ref())
                    }
                    Type::Tuple(elements) => elements.as_slice(),
                    _ => unreachable!(),
                };
                pending.extend(elements.iter().map(|ty| {
                    (
                        Constraint {
                            class: current.class,
                            ty: ty.clone(),
                            span: current.span,
                        },
                        depth + 1,
                        false,
                    )
                }));
                continue;
            }
            if self.intrinsic(current.class, &current.ty, types) {
                continue;
            }
            if let Some((instance, substitutions)) =
                self.matching_instance(current.class, &current.ty, types)
            {
                pending.extend(instance.constraints.iter().map(|obligation| {
                    (
                        Constraint {
                            class: obligation.class,
                            ty: substitute(&obligation.ty, &substitutions),
                            span: current.span,
                        },
                        depth + 1,
                        false,
                    )
                }));
            } else if !variables(&current.ty).is_empty() {
                if seen.insert((current.class, current.ty.clone())) {
                    residual.push(current);
                }
            } else {
                self.validate_instance(&current, types)?;
            }
        }
        Ok(residual)
    }

    fn entails(
        &self,
        context: &[Constraint],
        required: &Constraint,
        types: &TypeContext<'_>,
    ) -> Result<bool, Diagnostic> {
        for constraint in context {
            for available in self.normalize(constraint, types)? {
                if available.class == required.class && available.ty == required.ty {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }

    pub fn method(
        &self,
        expression: &Expr,
        module: &str,
        names: &Names,
        is_local: impl Fn(&str) -> bool,
    ) -> Result<Option<(usize, usize)>, Diagnostic> {
        let ExprKind::Field(value, method) = &expression.kind else {
            return Ok(None);
        };
        let Some((mut class_name, root)) = expression_path(value) else {
            return Ok(None);
        };
        if is_local(&root.text) {
            return Ok(None);
        }
        if root.provenance == Provenance::Generated && class_name.starts_with("$class.") {
            class_name = class_name["$class.".len()..].to_owned();
        }
        let Some(class) = self.find(names, module, &class_name, value.span)? else {
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
        if self.declarations[class].builtin && self.declarations[class].name == "Drop" {
            return Err(Diagnostic::new(
                "E1016",
                "'Drop.drop' runs automatically when a value is dropped; let the value go out of scope or pass it to a function that consumes it",
                method.span,
            ));
        }
        Ok(Some((class, index)))
    }

    pub(super) fn recursion_targets(
        &self,
        expression: &TypedExpr,
        types: &TypeContext<'_>,
        caller: usize,
    ) -> Vec<usize> {
        let (class, method, ty) = match &expression.kind {
            TypedExprKind::Method(class, method, ty) => (*class, *method, ty),
            TypedExprKind::Function(FunctionRef::Builtin(instance))
                if instance.builtin == Builtin::ToString =>
            {
                (self.names["Display"], 0, &instance.types[0])
            }
            TypedExprKind::Binary(operator, left, _)
                if !matches!(operator, BinaryOp::And | BinaryOp::Or | BinaryOp::Pipe) =>
            {
                let (class, method) = self.operation(Operation::Binary(*operator));
                (class, method, &left.ty)
            }
            TypedExprKind::Unary(operator, operand) if *operator != UnaryOp::Not => {
                let (class, method) = self.operation(Operation::Unary(*operator));
                (class, method, &operand.ty)
            }
            _ => return Vec::new(),
        };
        if let Some((function, _)) = self.resolved_method(class, method, ty, types) {
            return vec![function];
        }
        let mut targets: BTreeSet<_> = self.declarations[class].methods[method]
            .default
            .into_iter()
            .collect();
        let descending = self.instances.iter().any(|instance| {
            if instance.class != class
                || instance.head == *ty
                || !instance
                    .methods
                    .iter()
                    .any(|(function, _)| *function == caller)
            {
                return false;
            }
            let mut contains = false;
            map_type(&instance.head, &mut |part| {
                contains |= part == ty;
                part.clone()
            });
            contains
        });
        if !variables(ty).is_empty() && !descending {
            targets.extend(
                self.instances
                    .iter()
                    .filter(|instance| instance.class == class)
                    .map(|instance| instance.methods[method].0),
            );
        }
        targets.into_iter().collect()
    }

    fn structural(&self, class: usize, ty: &Type) -> bool {
        if self.declarations[class].name == "Default" {
            return matches!(ty, Type::Tuple(_));
        }
        matches!(
            self.declarations[class].name.as_str(),
            "Eq" | "Ord" | "Hash" | "Display"
        ) && matches!(ty, Type::Array(_) | Type::List(_) | Type::Tuple(_))
    }

    fn intrinsic(&self, class: usize, ty: &Type, types: &TypeContext<'_>) -> bool {
        if ty.contains_error() {
            return true;
        }
        if !self.declarations[class].builtin {
            return false;
        }
        if let Type::Simd(vector) = ty {
            use crate::simd::SimdKind;
            return match self.declarations[class].name.as_str() {
                "SimdVector" | "Copy" | "Capture" | "Send" => true,
                "SimdNumeric" | "Add" | "Sub" | "Mul" => vector.kind != SimdKind::Mask,
                "SimdMask" => vector.kind == SimdKind::Mask,
                "Div" => vector.kind == SimdKind::Float,
                "Bits" => vector.kind != SimdKind::Float,
                "Neg" => matches!(vector.kind, SimdKind::Float | SimdKind::Signed),
                _ => false,
            };
        }
        match self.declarations[class].name.as_str() {
            "Add" => ty.is_numeric() || ty.is_string(),
            "Sub" | "Mul" | "Div" | "Numeric" => ty.is_numeric(),
            "Ord" => ty.is_numeric() || ty.is_string() || matches!(ty, Type::Char | Type::Utf8Char),
            "Rem" | "Bits" | "Integer" => ty.is_integer(),
            "SignedInteger" => matches!(ty, Type::Integer(_, true)),
            "UnsignedInteger" => matches!(ty, Type::Integer(_, false)),
            "Neg" => ty.is_float() || matches!(ty, Type::Integer(_, true)),
            "Float" => ty.is_float(),
            "Elementary" => matches!(ty, Type::Binary(32 | 64)),
            "Eq" => ty.is_scalar() || ty.is_string() || *ty == Type::Unit,
            "Copy" => ty.is_copy(types),
            "Capture" => ty.can_capture(types),
            "Send" => ty.can_send(types),
            "Display" => {
                ty.is_numeric()
                    || ty.is_string()
                    || matches!(ty, Type::Bool | Type::Unit | Type::Char | Type::Utf8Char)
            }
            "Parse" => ty.is_numeric() || matches!(ty, Type::Bool | Type::Char | Type::Utf8Char),
            "Hash" => ty.is_scalar() || ty.is_string() || matches!(ty, Type::Unit),
            "Default" => {
                ty.is_scalar()
                    || ty.is_string()
                    || matches!(ty, Type::Unit | Type::Array(_) | Type::List(_))
            }
            _ => false,
        }
    }

    fn validate(&self, constraint: &Constraint, types: &TypeContext<'_>) -> Result<(), Diagnostic> {
        self.normalize(constraint, types).map(|_| ())
    }

    fn validate_instance(
        &self,
        constraint: &Constraint,
        types: &TypeContext<'_>,
    ) -> Result<(), Diagnostic> {
        if constraint.ty.contains_error() {
            return Ok(());
        }
        bounded_type(&constraint.ty, constraint.span)?;
        if !variables(&constraint.ty).is_empty() {
            return Ok(());
        }
        require_concrete(&constraint.ty, constraint.span)?;
        if self.intrinsic(constraint.class, &constraint.ty, types)
            || self
                .matching_instance(constraint.class, &constraint.ty, types)
                .is_some()
        {
            Ok(())
        } else {
            if self.declarations[constraint.class].name == "Capture" {
                return Err(Diagnostic::new(
                    "E1005",
                    format!(
                        "cannot capture {} in a reusable function; fully apply exclusive borrows and keep single-use tasks in task blocks",
                        constraint.ty.display(types)
                    ),
                    constraint.span,
                ));
            }
            if self.declarations[constraint.class].name == "Send" {
                return Err(Diagnostic::new(
                    "E1013",
                    format!(
                        "tasks require owned values; {} contains a reference",
                        constraint.ty.display(types)
                    ),
                    constraint.span,
                ));
            }
            Err(Diagnostic::new(
                "E1005",
                format!(
                    "no instance for {}<{}>; define an instance or use a supported type",
                    self.declarations[constraint.class].name,
                    constraint.ty.display(types)
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
            Operation::Builtin(Builtin::Display) => "Display",
            Operation::Builtin(Builtin::Parse) => "Parse",
            Operation::Builtin(Builtin::Default) => "Default",
            Operation::Builtin(Builtin::Hash) => "Hash",
            Operation::Builtin(_) => unreachable!("class operations have registered builtins"),
        };
        let class = self.names[name];
        let method = self.declarations[class].methods.iter().position(|method| matches!((method.operation, operation),
            (Some(Operation::Binary(a)), Operation::Binary(b)) if a == b)
            || matches!((method.operation, operation), (Some(Operation::Unary(a)), Operation::Unary(b)) if a == b)
            || matches!((method.operation, operation), (Some(Operation::Builtin(a)), Operation::Builtin(b)) if a == b)).unwrap();
        (class, method)
    }
}

/// A `Drop` instance covers every instantiation of a record or union declared in user code, so
/// whether a type runs a user drop never depends on its type arguments (B07 D1).
fn validate_drop_instance(
    head: &Type,
    constrained: bool,
    types: &TypeContext<'_>,
    span: Span,
) -> Result<(), Diagnostic> {
    let (origin, arguments) = match head {
        Type::Record(id, arguments) => (types.records[*id].origin, arguments.as_ref()),
        Type::Union(id, arguments) => (types.unions[*id].origin, arguments.as_ref()),
        _ => (ModuleOrigin::Std, [].as_slice()),
    };
    if origin != ModuleOrigin::User {
        return Err(Diagnostic::new(
            "E1016",
            "only records and unions declared in this program can implement Drop",
            span,
        ));
    }
    let variables: BTreeSet<_> = arguments
        .iter()
        .filter_map(|argument| match argument {
            Type::Variable(name) => Some(name),
            _ => None,
        })
        .collect();
    if variables.len() != arguments.len() {
        return Err(Diagnostic::new(
            "E1016",
            "a Drop instance must cover every instantiation; write every type parameter as a distinct type variable",
            span,
        ));
    }
    if constrained {
        return Err(Diagnostic::new(
            "E1016",
            "Drop instances cannot have constraints; drop must work for every instantiation",
            span,
        ));
    }
    Ok(())
}

/// Interpolation holes hand only records and unions to `Format` (D07 D12), so an instance for any
/// other type, or for a std record or union, could never be reached from a hole and would only
/// look as if it worked. Any instantiation of a user type may have its own instance.
fn validate_format_instance(
    head: &Type,
    types: &TypeContext<'_>,
    span: Span,
) -> Result<(), Diagnostic> {
    let origin = match head {
        Type::Record(id, _) => Some(types.records[*id].origin),
        Type::Union(id, _) => Some(types.unions[*id].origin),
        _ => None,
    };
    if origin == Some(ModuleOrigin::User) {
        Ok(())
    } else {
        Err(Diagnostic::new(
            "E1016",
            "only records and unions declared in this program can implement Format",
            span,
        ))
    }
}

fn instance_function(
    signature: &Signature,
    substitutions: &BTreeMap<String, Type>,
    definition: &Definition,
    name: String,
    constraints: Vec<ConstraintExpr>,
    types: &TypeContext<'_>,
) -> Result<FunctionDecl, Diagnostic> {
    let mut definition = definition.clone();
    while let ExprKind::Lambda(parameters, body) = definition.body.kind {
        definition.parameters.extend(parameters);
        definition.body = *body;
    }
    if definition.parameters.len() > signature.parameters.len()
        || (definition.parameters.is_empty() && !signature.parameters.is_empty())
    {
        return Err(Diagnostic::new(
            "E1006",
            format!(
                "method '{}' expects {} parameters",
                definition.name.text,
                signature.parameters.len()
            ),
            definition.name.span,
        ));
    }
    let parameters = definition
        .parameters
        .iter()
        .zip(&signature.parameters)
        .map(|((name, mutable), ty)| Parameter {
            name: name.clone(),
            mutable: *mutable,
            ty: type_expression(&substitute(ty, substitutions), types, name.span),
        })
        .collect();
    Ok(FunctionDecl {
        doc: None,
        regions: Vec::new(),
        recursion: definition.recursion,
        name: Ident {
            text: name,
            span: definition.name.span,
            provenance: Provenance::Generated,
        },
        visibility: Visibility::Public,
        exported: false,
        parameters,
        result: type_expression(
            &substitute(
                &signature
                    .as_type()
                    .after_arguments(definition.parameters.len()),
                substitutions,
            ),
            types,
            definition.name.span,
        ),
        constraints,
        body: definition.body,
    })
}

pub(super) fn type_expression(ty: &Type, types: &TypeContext<'_>, span: Span) -> TypeExpr {
    let kind = match ty {
        Type::Variable(name) => TypeExprKind::Variable(name.clone()),
        Type::Array(ty) => TypeExprKind::Array(Box::new(type_expression(ty, types, span))),
        Type::List(ty) => TypeExprKind::List(Box::new(type_expression(ty, types, span))),
        Type::Vec(ty) => TypeExprKind::Apply(
            Box::new(Ident {
                text: "Vec".into(),
                span,
                provenance: Provenance::Generated,
            }),
            vec![type_expression(ty, types, span)].into(),
        ),
        Type::Tuple(elements) => TypeExprKind::Tuple(
            elements
                .iter()
                .map(|ty| type_expression(ty, types, span))
                .collect(),
        ),
        Type::Task(ty) => TypeExprKind::Task(Box::new(type_expression(ty, types, span))),
        Type::Reference(ty, mutable) => {
            TypeExprKind::Reference(Box::new(type_expression(ty, types, span)), *mutable)
        }
        Type::Function(parameters, result) => TypeExprKind::Function(
            parameters
                .iter()
                .map(|ty| type_expression(ty, types, span))
                .collect(),
            Box::new(type_expression(result, types, span)),
        ),
        Type::Record(id, arguments) | Type::Union(id, arguments) if !arguments.is_empty() => {
            let name = match ty {
                Type::Record(..) => &types.records[*id].name,
                _ => &types.unions[*id].name,
            };
            TypeExprKind::Apply(
                Box::new(Ident {
                    text: name.clone(),
                    span,
                    provenance: Provenance::Generated,
                }),
                arguments
                    .iter()
                    .map(|ty| type_expression(ty, types, span))
                    .collect(),
            )
        }
        Type::Application(head, arguments) => TypeExprKind::Apply(
            Box::new(Ident {
                text: head.display(types),
                span,
                provenance: Provenance::Generated,
            }),
            arguments
                .iter()
                .map(|ty| type_expression(ty, types, span))
                .collect(),
        ),
        Type::Partial(partial) => {
            let name = partial.constructor.name(types);
            if partial.trailing.is_empty() {
                TypeExprKind::Named(name)
            } else {
                TypeExprKind::Apply(
                    Box::new(Ident {
                        text: name,
                        span,
                        provenance: Provenance::Generated,
                    }),
                    partial
                        .trailing
                        .iter()
                        .map(|ty| type_expression(ty, types, span))
                        .collect(),
                )
            }
        }
        Type::Infer(_) => unreachable!("instance types are concrete"),
        _ => TypeExprKind::Named(ty.display(types)),
    };
    TypeExpr { kind, span }
}

/// A pending `UnsignedOf` or `WidenOf` result of a builtin use: `output` is
/// solved once `input` is a concrete integer type.
pub(super) struct Family {
    kind: FamilyKind,
    input: Type,
    output: Type,
    span: Span,
}

enum FamilyKind {
    Unsigned,
    Widen,
    SimdLane(Option<u16>),
    SimdMask,
}

impl Checker<'_> {
    /// Instantiates a builtin's scheme with fresh inference variables, which
    /// become the types of its `BuiltinInstance`.
    pub(super) fn builtin(
        &mut self,
        builtin: Builtin,
        span: Span,
    ) -> Result<(TypedExprKind, Type), Diagnostic> {
        if matches!(builtin, Builtin::IOReadLine | Builtin::IOWrite)
            && !(self.module == "IO" && self.names.origin(self.module) == ModuleOrigin::Std)
        {
            return Err(Diagnostic::new(
                "E1022",
                "IO primitives are private to the standard IO module; compose IO actions instead",
                span,
            ));
        }
        if matches!(
            builtin,
            Builtin::OsRead
                | Builtin::OsArgs
                | Builtin::OsWrite
                | Builtin::OsRandom
                | Builtin::OsClock
                | Builtin::OsSleep
                | Builtin::OsOpen
                | Builtin::OsHandle
                | Builtin::OsClose
                | Builtin::OsSpawn
        ) && !(matches!(
            self.module,
            "File" | "Dir" | "Env" | "Time" | "Random" | "Process" | "Os"
        ) && self.names.origin(self.module) == ModuleOrigin::Std)
        {
            return Err(Diagnostic::new(
                "E1022",
                "operating-system primitives are private to the standard File, Dir, Env, Time, Random, Process, and Os modules; use those APIs instead",
                span,
            ));
        }
        if builtin == Builtin::DebugPrintString
            && !(self.module == "Debug" && self.names.origin(self.module) == ModuleOrigin::Std)
        {
            return Err(Diagnostic::new(
                "E1022",
                "Debug.__print_string is private to the standard Debug module; use Debug.print or Debug.trace",
                span,
            ));
        }
        let scheme = builtin.scheme();
        let types: Vec<Type> = scheme
            .variables
            .iter()
            .map(|_| self.inference.fresh())
            .collect();
        let bindings: BTreeMap<&str, Type> = scheme
            .variables
            .iter()
            .copied()
            .zip(types.iter().cloned())
            .collect();
        let parameters: Vec<Type> = scheme
            .parameters
            .iter()
            .map(|ty| self.builtin_type(ty, &bindings, span))
            .collect::<Result<_, _>>()?;
        let result = self.builtin_type(&scheme.result, &bindings, span)?;
        if builtin == Builtin::SeqNext {
            let Type::Record(id, arguments) = &parameters[0] else {
                return Err(Diagnostic::new(
                    "E1005",
                    "Seq.next requires the standard Seq record",
                    span,
                ));
            };
            let Type::Tuple(results) = &result else {
                unreachable!("Seq.next scheme is a tuple");
            };
            let callback = Type::function(vec![Type::Unit], result.clone());
            let step = self
                .names
                .std_type("Option", "Option", vec![callback].into(), span)?;
            if self.types.record_fields(*id, arguments) != vec![results[1].clone(), step] {
                return Err(Diagnostic::new(
                    "E1005",
                    "Seq.next requires the standard head and step representation",
                    span,
                ));
            }
            let Type::Union(option, option_arguments) = &results[1] else {
                return Err(Diagnostic::new(
                    "E1005",
                    "Seq.next requires the standard Option union",
                    span,
                ));
            };
            let cases = &self.types.unions[*option].cases;
            if cases.len() != 2
                || cases[0].0 != "None"
                || cases[0].1.is_some()
                || cases[1].0 != "Some"
                || cases[1].1.is_none()
                || self.types.union_payload(*option, option_arguments, 1) != Some(types[0].clone())
            {
                return Err(Diagnostic::new(
                    "E1005",
                    "Seq.next requires Option.None and Option.Some in standard order",
                    span,
                ));
            }
        }
        if matches!(builtin, Builtin::OwnedFunction | Builtin::OwnedCall) {
            let owned = match &parameters[0] {
                Type::Reference(owned, false) if builtin == Builtin::OwnedCall => owned.as_ref(),
                _ => &result,
            };
            let run = Type::function(vec![types[0].clone()], types[1].clone());
            if !matches!(owned, Type::Record(id, arguments) if self.types.record_fields(*id, arguments) == [run])
            {
                return Err(Diagnostic::new(
                    "E1005",
                    format!(
                        "{} requires the standard Owned.Function record",
                        builtin.name()
                    ),
                    span,
                ));
            }
        }
        for constraint in &scheme.constraints {
            let ty = self.builtin_type(&constraint.ty, &bindings, span)?;
            self.require(constraint.class, ty, span)?;
        }
        Ok((
            TypedExprKind::Function(FunctionRef::Builtin(BuiltinInstance { builtin, types })),
            Type::function(parameters, result),
        ))
    }

    fn builtin_type(
        &mut self,
        ty: &BuiltinType,
        bindings: &BTreeMap<&str, Type>,
        span: Span,
    ) -> Result<Type, Diagnostic> {
        let mut element = |ty: &BuiltinType| self.builtin_type(ty, bindings, span).map(Box::new);
        Ok(match ty {
            BuiltinType::Var(name) => bindings[name].clone(),
            BuiltinType::Concrete(ty) => ty.clone(),
            BuiltinType::Array(ty) => Type::Array(element(ty)?),
            BuiltinType::List(ty) => Type::List(element(ty)?),
            BuiltinType::Vec(ty) => Type::Vec(element(ty)?),
            BuiltinType::Task(ty) => Type::Task(element(ty)?),
            BuiltinType::Reference(ty, mutable) => Type::Reference(element(ty)?, *mutable),
            BuiltinType::Std { module, name, args } => {
                let args = args
                    .iter()
                    .map(|ty| self.builtin_type(ty, bindings, span))
                    .collect::<Result<_, _>>()?;
                self.names.std_type(module, name, args, span)?
            }
            BuiltinType::Tuple(elements) => Type::Tuple(
                elements
                    .iter()
                    .map(|ty| self.builtin_type(ty, bindings, span))
                    .collect::<Result<_, _>>()?,
            ),
            BuiltinType::Function(parameters, result) => Type::function(
                parameters
                    .iter()
                    .map(|ty| self.builtin_type(ty, bindings, span))
                    .collect::<Result<_, _>>()?,
                self.builtin_type(result, bindings, span)?,
            ),
            BuiltinType::UnsignedOf(input)
            | BuiltinType::WidenOf(input)
            | BuiltinType::SimdLane(input, _)
            | BuiltinType::SimdMask(input) => {
                let input = self.builtin_type(input, bindings, span)?;
                let output = self.inference.fresh();
                self.families.push(Family {
                    kind: match ty {
                        BuiltinType::WidenOf(_) => FamilyKind::Widen,
                        BuiltinType::SimdLane(_, lanes) => FamilyKind::SimdLane(*lanes),
                        BuiltinType::SimdMask(_) => FamilyKind::SimdMask,
                        _ => FamilyKind::Unsigned,
                    },
                    input,
                    output: output.clone(),
                    span,
                });
                output
            }
        })
    }

    /// Solves the type families whose inputs are now concrete integers. At
    /// the end of a function (`last`), every remaining family is an error:
    /// phase 1 resolves them only at concrete call sites.
    pub(super) fn solve_families(&mut self, last: bool) -> Result<(), Diagnostic> {
        for family in std::mem::take(&mut self.families) {
            if matches!(family.kind, FamilyKind::SimdLane(_) | FamilyKind::SimdMask) {
                let input = self.inference.resolve(&family.input);
                let Type::Simd(vector) = input else {
                    if is_unknown(&input) && !last {
                        self.families.push(family);
                        continue;
                    }
                    return Err(Diagnostic::new(
                        if is_unknown(&input) { "E1015" } else { "E1005" },
                        "this SIMD operation needs a concrete vector type; add a vector type annotation",
                        family.span,
                    ));
                };
                let output = match family.kind {
                    FamilyKind::SimdLane(lanes) => {
                        if lanes.is_some_and(|lanes| lanes != vector.lanes()) {
                            return Err(Diagnostic::new(
                                "E1005",
                                "of_lanes count does not match the vector type",
                                family.span,
                            ));
                        }
                        vector.element()
                    }
                    _ => Type::Simd(vector.mask()),
                };
                self.same(&family.output, &output, family.span)?;
                continue;
            }
            let output = match self.inference.resolve(&family.input) {
                Type::Integer(bits, signed)
                    if matches!(family.kind, FamilyKind::Widen) && bits < 128 =>
                {
                    Type::Integer(bits * 2, signed)
                }
                Type::Integer(128, _) if matches!(family.kind, FamilyKind::Widen) => {
                    return Err(Diagnostic::new(
                        "E1005",
                        "128-bit integers have no wider integer type",
                        family.span,
                    ));
                }
                Type::Integer(bits, _) => Type::Integer(bits, false),
                Type::Infer(_) if !last => {
                    self.families.push(family);
                    continue;
                }
                input => {
                    let mut generic = false;
                    map_type(&input, &mut |ty| {
                        generic |= is_unknown(ty);
                        ty.clone()
                    });
                    if !generic {
                        // A concrete non-integer is an ordinary `Integer` error.
                        let constraint = Constraint {
                            class: self.classes.names["Integer"],
                            ty: input,
                            span: family.span,
                        };
                        self.classes.validate(&constraint, &self.types)?;
                    }
                    return Err(Diagnostic::new(
                        "E1015",
                        "this builtin needs a concrete integer argument type at its call site; generic code cannot use it",
                        family.span,
                    ));
                }
            };
            self.same(&family.output, &output, family.span)?;
        }
        Ok(())
    }

    pub(super) fn function(&mut self, id: usize) -> (TypedExprKind, Type) {
        let scheme = &self.signatures[id];
        if scheme.is_poisoned() {
            self.poisoned = true;
            return (TypedExprKind::Error, Type::Error);
        }
        if scheme.variables.is_empty() {
            if self.names.constants.contains(&id) {
                return (
                    TypedExprKind::Call(
                        Box::new(TypedExpr {
                            kind: TypedExprKind::Function(FunctionRef::User(id)),
                            ty: scheme.signature.as_type(),
                            span: Span::default(),
                        }),
                        Vec::new(),
                    ),
                    scheme.signature.result.clone(),
                );
            }
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

    /// `Display.display` as a method value whose type waits for inference.
    pub(super) fn display_method(
        &mut self,
        span: Span,
    ) -> Result<(TypedExprKind, Type), Diagnostic> {
        let class = self.classes.names["Display"];
        self.method(class, 0, span)
    }

    /// Whether an instance of `Format` covers `ty`. A type that still has
    /// unresolved parts cannot be matched yet, so it has none.
    pub(super) fn has_format_instance(&self, ty: &Type) -> bool {
        let class = self.classes.names["Format"];
        self.classes
            .matching_instance(class, ty, &self.types)
            .is_some()
    }

    /// `Format.format` as a method value whose type waits for inference.
    pub(super) fn format_method(
        &mut self,
        span: Span,
    ) -> Result<(TypedExprKind, Type), Diagnostic> {
        let class = self.classes.names["Format"];
        self.method(class, 0, span)
    }

    pub(super) fn method(
        &mut self,
        class: usize,
        method: usize,
        span: Span,
    ) -> Result<(TypedExprKind, Type), Diagnostic> {
        let declaration = &self.classes.declarations[class];
        let ty = self.inference.fresh();
        let signature = declaration.methods[method].signature(span)?.as_type();
        let mut substitutions = BTreeMap::from([(declaration.variable.clone(), ty.clone())]);
        for variable in variables(&signature) {
            substitutions
                .entry(variable)
                .or_insert_with(|| self.inference.fresh());
        }
        self.constraints.push(Constraint {
            class,
            ty: ty.clone(),
            span,
        });
        Ok((
            TypedExprKind::Method(class, method, ty),
            substitute(&signature, &substitutions),
        ))
    }

    pub(super) fn type_function(
        &mut self,
        variable: &Ident,
        name: &Ident,
    ) -> Result<(TypedExprKind, Type), Diagnostic> {
        if !self.type_parameters.contains(&variable.text) {
            return Err(Diagnostic::new(
                "E1015",
                format!(
                    "type variable '{}' is absent from this function's signature",
                    variable.text
                ),
                variable.span,
            ));
        }
        let receiver = Type::Variable(variable.text.clone());
        if !self.members.iter().any(|member| {
            member.receiver == receiver && member.name == name.text && member.signature.is_none()
        }) {
            return Err(Diagnostic::new(
                "E1016",
                format!(
                    "'{}.{} requires an explicit @'{} : #{} constraint",
                    variable.text, name.text, variable.text, name.text
                ),
                name.span,
            ));
        }
        let ty = self.inference.fresh();
        self.members.push(MemberConstraint {
            receiver: receiver.clone(),
            name: name.text.clone(),
            module: self.module.to_owned(),
            signature: Some(ty.clone()),
            span: name.span,
        });
        Ok((
            TypedExprKind::TypeFunction {
                receiver,
                name: name.text.clone(),
                module: self.module.to_owned(),
            },
            ty,
        ))
    }

    pub(super) fn require(&mut self, name: &str, ty: Type, span: Span) -> Result<(), Diagnostic> {
        let ty = self.inference.resolve(&ty);
        if ty.contains_error() {
            return Ok(());
        }
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
            self.classes.validate(&constraint, &self.types)?;
        }
        self.constraints.push(constraint);
        Ok(())
    }

    pub(super) fn annotation(&mut self, expression: &TypeExpr) -> Result<Type, Diagnostic> {
        regions::reject_local(expression)?;
        self.constraints.extend(self.classes.inline_constraints(
            expression,
            self.module,
            self.names,
        )?);
        let ty = resolve_type_with_kinds(expression, self.module, self.names, &self.kinds)?;
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
            Type::Error => Ok((vec![Type::Error; count], Type::Error)),
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
        self.solve_families(true)?;
        self.check_format_specs()?;
        for (ty, span) in &self.undecided_borrows {
            if matches!(self.inference.resolve(ty), Type::Reference(..)) {
                return Err(Diagnostic::new(
                    "E1015",
                    "'ref' must know whether its operand is a reference when it is checked; this operand is inferred later to be one, so annotate its type, or write '&x' to borrow the reference itself or '&*x' to reborrow",
                    *span,
                ));
            }
        }
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
        for (class, ty, span) in captures(body, self.classes, &self.types, |id| {
            self.signatures[id].signature.parameters.len()
        })? {
            self.require(class, ty, span)?;
        }
        let mut constraints = std::mem::take(&mut self.constraints);
        for constraint in &mut constraints {
            constraint.ty = self.inference.resolve(&constraint.ty);
            if let Err(error) = self.classes.validate(constraint, &self.types) {
                let error = if self.use_bindings.contains(&constraint.span)
                    && self.classes.declarations[constraint.class].name == "Drop"
                {
                    self.use_error(error, &constraint.ty, constraint.span)
                } else {
                    error
                };
                self.constraints = constraints;
                return Err(error);
            }
        }
        self.constraints = constraints;
        for member in &mut self.members {
            *member = member.resolved(&self.inference);
        }
        Ok(())
    }
}

pub(super) fn solve_members(
    functions: &mut [CheckedFunction],
    checkers: &mut [Checker<'_>],
) -> Result<(), Diagnostic> {
    if checkers.iter().all(|checker| checker.members.is_empty()) {
        return Ok(());
    }
    for checker in checkers.iter_mut() {
        let mut seen = BTreeSet::new();
        checker.members = std::mem::take(&mut checker.members)
            .into_iter()
            .map(|member| member.resolved(&checker.inference))
            .filter(|member| seen.insert(member.key()))
            .collect();
        if checker.members.len() > MAX_CONSTRAINTS {
            return Err(Diagnostic::new(
                "E1017",
                "too many distinct function constraints in one function",
                checker.members[MAX_CONSTRAINTS].span,
            ));
        }
    }
    let mut dependencies = Vec::new();
    for function in functions {
        let mut calls = Vec::new();
        walk(&mut function.body, &mut |expression| {
            match &expression.kind {
                TypedExprKind::GenericFunction(id, types) => {
                    calls.push((*id, types.clone(), expression.span));
                }
                TypedExprKind::Function(FunctionRef::User(id)) => {
                    calls.push((*id, Vec::new(), expression.span));
                }
                _ => {}
            }
            Ok(())
        })?;
        dependencies.push(calls);
    }
    let mut solved = vec![BTreeSet::new(); checkers.len()];
    for defaults in [false, true] {
        if defaults {
            for checker in checkers.iter_mut() {
                checker.inference.apply_defaults();
            }
        }
        loop {
            let mut changed = false;
            for (caller, checker) in checkers.iter_mut().enumerate() {
                for index in 0..checker.members.len() {
                    if solved[caller].contains(&index) {
                        continue;
                    }
                    let member = checker.members[index].resolved(&checker.inference);
                    if is_unknown(&member.receiver) {
                        continue;
                    }
                    let callee = checker.names.member_function(
                        &member.module,
                        &member.receiver,
                        &member.name,
                        &checker.types,
                        member.span,
                    )?;
                    if let Some(signature) = &member.signature {
                        let (kind, actual) = checker.function(callee);
                        checker.same(&actual, signature, member.span)?;
                        let types = match kind {
                            TypedExprKind::GenericFunction(_, types) => types,
                            _ => Vec::new(),
                        };
                        dependencies[caller].push((callee, types, member.span));
                    }
                    solved[caller].insert(index);
                    changed = true;
                }
            }
            for (caller, calls) in dependencies.iter().enumerate() {
                let mut inherited = Vec::new();
                for (callee, types, span) in calls {
                    let checker = &checkers[*callee];
                    let substitutions = checker
                        .type_parameters
                        .iter()
                        .cloned()
                        .zip(types.clone())
                        .collect();
                    for member in &checker.members {
                        let member = member.resolved(&checker.inference);
                        if member.determined() {
                            inherited.push(member.substituted(&substitutions, *span));
                        }
                    }
                }
                let checker = &mut checkers[caller];
                let mut seen: BTreeSet<_> = checker
                    .members
                    .iter()
                    .map(|member| member.resolved(&checker.inference).key())
                    .collect();
                for member in inherited {
                    let member = member.resolved(&checker.inference);
                    if seen.insert(member.key()) {
                        for ty in std::iter::once(&member.receiver).chain(member.signature.iter()) {
                            bounded_type(ty, member.span)?;
                        }
                        if checker.members.len() >= MAX_CONSTRAINTS {
                            return Err(Diagnostic::new(
                                "E1017",
                                "too many distinct function constraints; polymorphic recursion must not grow types",
                                member.span,
                            ));
                        }
                        checker.members.push(member);
                        changed = true;
                    }
                }
            }
            if !changed {
                break;
            }
        }
    }
    Ok(())
}

fn captures(
    body: &mut TypedExpr,
    classes: &Classes,
    types: &TypeContext<'_>,
    parameters: impl Fn(usize) -> usize,
) -> Result<Vec<(&'static str, Type, Span)>, Diagnostic> {
    let arity = |callee: &TypedExpr| match &callee.kind {
        TypedExprKind::Function(FunctionRef::User(id)) | TypedExprKind::GenericFunction(id, _) => {
            parameters(*id)
        }
        TypedExprKind::Function(FunctionRef::Builtin(instance)) => {
            instance.builtin.scheme().parameters.len()
        }
        TypedExprKind::Lambda { parameters, .. } => parameters.len(),
        TypedExprKind::Method(class, method, ty) => classes
            .resolved_method(*class, *method, ty, types)
            .map(|(function, _)| parameters(function))
            .unwrap_or_else(|| match &callee.ty {
                Type::Function(parameters, _) => parameters.len(),
                _ => unreachable!(),
            }),
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
            TypedExprKind::Lambda {
                captures,
                body,
                owned,
                ..
            } => {
                let task = matches!(expression.ty, Type::Task(_));
                let class = if task || *owned { "Send" } else { "Capture" };
                result.extend(
                    captures
                        .iter()
                        .map(|capture| (class, capture.ty.clone(), expression.span)),
                );
                if task {
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
    for child in expression.children_mut() {
        walk(child, f)?;
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
            TypedExprKind::GenericFunction(_, types)
            | TypedExprKind::Function(FunctionRef::Builtin(BuiltinInstance { types, .. })) => {
                for ty in types {
                    f(ty, expression.span)?;
                }
            }
            TypedExprKind::CaseConstructor { args, .. } => {
                for ty in args.iter_mut() {
                    f(ty, expression.span)?;
                }
            }
            TypedExprKind::Method(_, _, ty) | TypedExprKind::TypeFunction { receiver: ty, .. } => {
                f(ty, expression.span)?
            }
            TypedExprKind::Block { bindings, .. } => {
                for (local, _) in bindings {
                    f(&mut local.ty, local.span)?;
                }
            }
            TypedExprKind::ForRange { local, .. } => {
                f(&mut local.ty, local.span)?;
            }
            TypedExprKind::ForEach { owner, local, .. } => {
                f(&mut owner.ty, owner.span)?;
                f(&mut local.ty, local.span)?;
            }
            TypedExprKind::Match { local, arms, .. } => {
                f(&mut local.ty, local.span)?;
                for arm in arms {
                    for alternative in &mut arm.alternatives {
                        for step in &mut alternative.steps {
                            if let PatternStep::Bind(local, _) = step {
                                f(&mut local.ty, local.span)?;
                            }
                        }
                        for (local, _) in &mut alternative.bindings {
                            f(&mut local.ty, local.span)?;
                        }
                    }
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
    names: &Names,
    copy_constraints: Vec<BTreeSet<String>>,
    recursive_functions: &BTreeSet<usize>,
) -> Result<CheckedModule, Diagnostic> {
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
            let normalized = classes.normalize(
                &constraint,
                &TypeContext {
                    records: &module.records,
                    unions: &module.unions,
                },
            )?;
            for constraint in normalized {
                if !seen.insert((constraint.class, constraint.ty.clone())) {
                    continue;
                }
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
                let normalized = classes.normalize(
                    &constraint,
                    &TypeContext {
                        records: &module.records,
                        unions: &module.unions,
                    },
                )?;
                for constraint in normalized {
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
        }
        if !changed {
            break;
        }
    }
    for function in &module.functions {
        for constraint in &function.constraints {
            classes.validate(
                constraint,
                &TypeContext {
                    records: &module.records,
                    unions: &module.unions,
                },
            )?;
        }
    }
    for (class_id, class) in classes.declarations.iter().enumerate() {
        for method in &class.methods {
            if let Some(function) = method.default {
                let context = [Constraint {
                    class: class_id,
                    ty: Type::Variable(class.variable.clone()),
                    span: module.functions[function].span,
                }];
                for required in &module.functions[function].constraints {
                    if !classes.entails(&context, required, &module.types())? {
                        return Err(Diagnostic::new(
                            "E1027",
                            "default method requires an undeclared constraint; add it as a superclass",
                            required.span,
                        ));
                    }
                }
            }
        }
    }
    for instance in &classes.instances {
        for (function, arguments) in &instance.methods {
            let function = &module.functions[*function];
            let substitutions = function
                .type_parameters
                .iter()
                .cloned()
                .zip(arguments.iter().cloned())
                .collect();
            for required in &function.constraints {
                let required = Constraint {
                    class: required.class,
                    ty: substitute(&required.ty, &substitutions),
                    span: instance.span,
                };
                for required in classes.normalize(&required, &module.types())? {
                    if !classes.entails(&instance.constraints, &required, &module.types())? {
                        return Err(Diagnostic::new(
                            "E1027",
                            "instance method requires an undeclared constraint; add it to the instance context",
                            instance.span,
                        ));
                    }
                }
            }
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
        names,
        types: module.types(),
        keys: BTreeMap::new(),
        requests: Vec::new(),
        functions: Vec::new(),
        intrinsics: BTreeMap::new(),
        to_strings: BTreeMap::new(),
        current: FunctionOrigin::source(ModuleOrigin::User),
        base_count: module
            .functions
            .iter()
            .filter(|function| {
                function.type_parameters.is_empty() && function.origin.module == ModuleOrigin::User
            })
            .count(),
    };
    for (id, function) in module.functions.iter().enumerate() {
        if function.type_parameters.is_empty() && function.origin.module == ModuleOrigin::User {
            specializer.request(id, Vec::new(), function.span)?;
        }
    }
    let entry = module.entry.map(|id| specializer.keys[&(id, Vec::new())]);
    let tests = module
        .tests
        .iter()
        .cloned()
        .map(|mut test| {
            test.function = specializer.keys[&(test.function, Vec::new())];
            test
        })
        .collect();
    let mut next = 0;
    let mut user_drops = BTreeMap::new();
    let drops = classes.drop_heads().next().is_some();
    let mut scanned = 0;
    let mut seen = BTreeSet::new();
    loop {
        while next < specializer.requests.len() {
            let (id, types) = specializer.requests[next].clone();
            let function = specializer.instantiate(id, &types, next)?;
            specializer.functions.push(function);
            next += 1;
        }
        if !drops {
            break;
        }
        // Drop glue calls the user drop of every Drop type that a specialized function holds;
        // the drops can hold more Drop types, so this runs to a fixed point (B07 D6).
        let types = specializer.types;
        let mut found = BTreeSet::new();
        for function in &mut specializer.functions[scanned..] {
            for parameter in &function.parameters {
                drop_components(&parameter.ty, &types, &mut seen, &mut found);
            }
            drop_components(&function.signature.as_type(), &types, &mut seen, &mut found);
            expression_types(&mut function.body, &mut |ty, _| {
                drop_components(ty, &types, &mut seen, &mut found);
                Ok(())
            })?;
        }
        scanned = specializer.functions.len();
        if found.is_empty() {
            break;
        }
        let class = classes.names["Drop"];
        for ty in found {
            let (function, arguments) = classes
                .resolved_method(class, 0, &ty, &types)
                .expect("a Drop instance covers every instantiation of its type");
            let span = specializer.templates[function].span;
            user_drops.insert(ty, specializer.request(function, arguments, span)?);
        }
    }
    let requires_rec: Vec<_> = specializer
        .requests
        .iter()
        .map(|(template, _)| {
            *template < module.functions.len() && !recursive_functions.contains(template)
        })
        .collect();
    recursion::check_specialized(&specializer.functions, &requires_rec)?;
    let functions = specializer.functions;
    Ok(CheckedModule {
        records: module.records,
        unions: module.unions,
        functions,
        entry,
        tests,
        warnings: module.warnings,
        user_drops,
    })
}

/// Adds the Drop types that a value of `ty` owns, itself included; borrows and function
/// environments own nothing that their type shows.
fn drop_components(
    ty: &Type,
    types: &TypeContext<'_>,
    seen: &mut BTreeSet<Type>,
    found: &mut BTreeSet<Type>,
) {
    let mut pending = vec![ty.clone()];
    while let Some(ty) = pending.pop() {
        if !seen.insert(ty.clone()) {
            continue;
        }
        if ty.has_user_drop(types) {
            found.insert(ty.clone());
        }
        match &ty {
            Type::Record(id, arguments) => pending.extend(types.record_fields(*id, arguments)),
            Type::Union(id, arguments) => {
                pending.extend(types.union_payloads(*id, arguments).into_iter().flatten())
            }
            Type::Tuple(elements) => pending.extend(elements.iter().cloned()),
            Type::Array(element)
            | Type::List(element)
            | Type::Vec(element)
            | Type::Task(element) => pending.push((**element).clone()),
            _ => {}
        }
    }
}

struct Specializer<'a> {
    templates: Vec<CheckedFunction>,
    classes: &'a Classes,
    names: &'a Names,
    types: TypeContext<'a>,
    keys: BTreeMap<(usize, Vec<Type>), usize>,
    requests: Vec<(usize, Vec<Type>)>,
    functions: Vec<CheckedFunction>,
    intrinsics: BTreeMap<(usize, usize, Type), usize>,
    to_strings: BTreeMap<Type, usize>,
    /// The origin that helpers generated for the function being instantiated inherit.
    current: FunctionOrigin,
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
            function.origin = function.origin.generated(instance);
        }
        for constraint in &function.constraints {
            self.classes.validate(
                &Constraint {
                    class: constraint.class,
                    ty: substitute(&constraint.ty, &substitutions),
                    span: constraint.span,
                },
                &self.types,
            )?;
        }
        for member in &function.members {
            let member = member.substituted(&substitutions, member.span);
            self.member_target(&member)?;
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
        validate_size(&function.signature.as_type(), &self.types, function.span)?;
        expression_types(&mut function.body, &mut |ty, span| {
            *ty = substitute(ty, &substitutions);
            require_concrete(ty, span)?;
            validate_size(ty, &self.types, span)?;
            Ok(())
        })?;
        walk(&mut function.body, &mut |expression| {
            if let TypedExprKind::TypeFunction {
                receiver,
                name,
                module,
            } = &expression.kind
            {
                let member = MemberConstraint {
                    receiver: receiver.clone(),
                    name: name.clone(),
                    module: module.clone(),
                    signature: Some(expression.ty.clone()),
                    span: expression.span,
                };
                let (id, types) = self.member_target(&member)?;
                expression.kind = TypedExprKind::GenericFunction(id, types);
            }
            Ok(())
        })?;
        for (class, ty, span) in captures(&mut function.body, self.classes, &self.types, |id| {
            self.templates[id].parameters.len()
        })? {
            self.classes.validate(
                &Constraint {
                    class: self.classes.names[class],
                    ty,
                    span,
                },
                &self.types,
            )?;
        }
        self.current = function.origin.generated(instance);
        walk(&mut function.body, &mut |expression| self.lower(expression))?;
        function.type_parameters.clear();
        function.constraints.clear();
        function.members.clear();
        Ok(function)
    }

    fn member_target(&self, member: &MemberConstraint) -> Result<(usize, Vec<Type>), Diagnostic> {
        let id = self.names.member_function(
            &member.module,
            &member.receiver,
            &member.name,
            &self.types,
            member.span,
        )?;
        let Some(expected) = &member.signature else {
            return Ok((id, Vec::new()));
        };
        let target = &self.templates[id];
        let mut inference = Inference::default();
        let types: Vec<_> = target
            .type_parameters
            .iter()
            .map(|_| inference.fresh())
            .collect();
        let substitutions = target
            .type_parameters
            .iter()
            .cloned()
            .zip(types.clone())
            .collect();
        inference.unify(
            &substitute(&target.signature.as_type(), &substitutions),
            expected,
            &self.types,
            member.span,
        )?;
        let types: Vec<_> = types.iter().map(|ty| inference.resolve(ty)).collect();
        for ty in &types {
            require_concrete(ty, member.span)?;
        }
        Ok((id, types))
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
                    if let Some((function, arguments)) =
                        self.classes
                            .resolved_method(*class, *method, ty, &self.types)
                    {
                        let arguments = if self.classes.declarations[*class].arity > 0 {
                            let target = &self.templates[function];
                            let mut inference = Inference::default();
                            let variables: Vec<_> = target
                                .type_parameters
                                .iter()
                                .map(|_| inference.fresh())
                                .collect();
                            let substitutions = target
                                .type_parameters
                                .iter()
                                .cloned()
                                .zip(variables.iter().cloned())
                                .collect();
                            inference.unify(
                                &substitute(&target.signature.as_type(), &substitutions),
                                &expression.ty,
                                &self.types,
                                expression.span,
                            )?;
                            variables.iter().map(|ty| inference.resolve(ty)).collect()
                        } else {
                            arguments
                        };
                        Some(Function(FunctionRef::User(self.request(
                            function,
                            arguments,
                            expression.span,
                        )?)))
                    } else {
                        // Intrinsic method values get ordinary monomorphic wrappers as well.
                        let id = self.intrinsic_function(*class, *method, ty, expression.span)?;
                        Some(Function(FunctionRef::User(id)))
                    }
                }
                Function(FunctionRef::Builtin(instance))
                    if instance.builtin == Builtin::DisplayQuoted
                        && !matches!(
                            instance.types[0],
                            Type::String | Type::Utf8String | Type::Char | Type::Utf8Char
                        ) =>
                {
                    let class = self.classes.names["Display"];
                    let function = if let Some((function, arguments)) = self
                        .classes
                        .resolved_method(class, 0, &instance.types[0], &self.types)
                    {
                        self.request(function, arguments, expression.span)?
                    } else {
                        self.intrinsic_function(class, 0, &instance.types[0], expression.span)?
                    };
                    Some(Function(FunctionRef::User(function)))
                }
                Function(FunctionRef::Builtin(instance))
                    if instance.builtin == Builtin::ToString
                        && !self.classes.intrinsic(
                            self.classes.names["Display"],
                            &instance.types[0],
                            &self.types,
                        ) =>
                {
                    Some(Function(FunctionRef::User(
                        self.string_function(&instance.types[0], expression.span)?,
                    )))
                }
                GenericInteger(value, negative) => Some(
                    Checker::integer_literal(
                        *value,
                        None,
                        Some(&expression.ty),
                        *negative,
                        expression.span,
                        &self.types,
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
                    if self.classes.intrinsic(class, &left.ty, &self.types) {
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
                    if self.classes.intrinsic(class, &operand.ty, &self.types) {
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
        let declaration = &self.classes.declarations[class];
        let signature = substitute(
            &declaration.methods[method].signature(span)?.as_type(),
            &BTreeMap::from([(declaration.variable.clone(), ty.clone())]),
        );
        let id = if self.classes.structural(class, ty) {
            self.intrinsic_function(class, method, ty, span)?
        } else {
            let (function, types) = self
                .classes
                .resolved_method(class, method, ty, &self.types)
                .ok_or_else(|| Diagnostic::new("E1005", "no instance for this operator", span))?;
            self.request(function, types, span)?
        };
        let arguments = if matches!(self.classes.declarations[class].name.as_str(), "Eq" | "Ord") {
            arguments
                .into_iter()
                .map(|argument| TypedExpr {
                    ty: Type::Reference(Box::new(argument.ty.clone()), false),
                    span: argument.span,
                    kind: TypedExprKind::BorrowOperand(Box::new(argument)),
                })
                .collect()
        } else {
            arguments
        };
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
        let template = method.signature(span)?;
        let substitutions = BTreeMap::from([(declaration.variable.clone(), ty.clone())]);
        let signature = Signature {
            parameters: template
                .parameters
                .iter()
                .map(|ty| substitute(ty, &substitutions))
                .collect(),
            result: substitute(&template.result, &substitutions),
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
                provenance: Provenance::Generated,
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
            Operation::Builtin(Builtin::Display) if self.classes.structural(class, ty) => {
                let elements = match ty {
                    Type::Array(element) | Type::List(element) => {
                        std::slice::from_ref(element.as_ref())
                    }
                    Type::Tuple(elements) => elements.as_slice(),
                    _ => unreachable!(),
                };
                let mut values = vec![*argument(0)];
                values.extend(elements.iter().map(|element| TypedExpr {
                    kind: TypedExprKind::Function(FunctionRef::Builtin(BuiltinInstance {
                        builtin: Builtin::DisplayQuoted,
                        types: vec![element.clone()],
                    })),
                    ty: Type::function(
                        vec![Type::Reference(Box::new(element.clone()), false)],
                        Type::String,
                    ),
                    span,
                }));
                TypedExprKind::StructuralDisplay(values)
            }
            Operation::Builtin(Builtin::Hash) if self.classes.structural(class, ty) => {
                let elements = match ty {
                    Type::Array(element) | Type::List(element) => {
                        std::slice::from_ref(element.as_ref())
                    }
                    Type::Tuple(elements) => elements.as_slice(),
                    _ => unreachable!(),
                };
                let mut values = vec![*argument(0)];
                values.extend(elements.iter().map(|element| TypedExpr {
                    kind: TypedExprKind::Method(class, 0, element.clone()),
                    ty: Type::function(
                        vec![Type::Reference(Box::new(element.clone()), false)],
                        Type::Integer(64, false),
                    ),
                    span,
                }));
                TypedExprKind::StructuralHash(values)
            }
            Operation::Builtin(Builtin::Default) if matches!(ty, Type::Tuple(_)) => {
                let Type::Tuple(elements) = ty else {
                    unreachable!()
                };
                TypedExprKind::Tuple(
                    elements
                        .iter()
                        .map(|element| TypedExpr {
                            ty: element.clone(),
                            span,
                            kind: TypedExprKind::Call(
                                Box::new(TypedExpr {
                                    kind: TypedExprKind::Method(class, 0, element.clone()),
                                    ty: Type::function(Vec::new(), element.clone()),
                                    span,
                                }),
                                Vec::new(),
                            ),
                        })
                        .collect(),
                )
            }
            Operation::Binary(operator) if self.classes.structural(class, ty) => {
                let elements = match ty {
                    Type::Array(element) | Type::List(element) => {
                        std::slice::from_ref(element.as_ref())
                    }
                    Type::Tuple(elements) => elements.as_slice(),
                    _ => unreachable!(),
                };
                let mut values = vec![*argument(0), *argument(1)];
                for element in elements {
                    let borrowed = Type::Reference(Box::new(element.clone()), false);
                    for operation in std::iter::once(BinaryOp::Equal)
                        .chain((class == self.classes.names["Ord"]).then_some(operator))
                    {
                        let (class, method) = self.classes.operation(Operation::Binary(operation));
                        values.push(TypedExpr {
                            kind: TypedExprKind::Method(class, method, element.clone()),
                            ty: Type::function(
                                vec![borrowed.clone(), borrowed.clone()],
                                Type::Bool,
                            ),
                            span,
                        });
                    }
                }
                TypedExprKind::StructuralCompare(operator, values)
            }
            Operation::Binary(operator) => {
                let operand = |index| {
                    let value = argument(index);
                    if matches!(binary_class(operator), "Eq" | "Ord") {
                        Box::new(TypedExpr {
                            kind: TypedExprKind::Dereference(value),
                            ty: ty.clone(),
                            span,
                        })
                    } else {
                        value
                    }
                };
                TypedExprKind::Binary(operator, operand(0), operand(1))
            }
            Operation::Unary(operator) => TypedExprKind::Unary(operator, argument(0)),
            Operation::Builtin(builtin) => TypedExprKind::Call(
                Box::new(TypedExpr {
                    kind: TypedExprKind::Function(FunctionRef::Builtin(BuiltinInstance {
                        builtin,
                        types: vec![ty.clone()],
                    })),
                    ty: signature.as_type(),
                    span,
                }),
                parameters.iter().map(|local| *argument(local.id)).collect(),
            ),
        };
        let id = self.templates.len();
        let body = TypedExpr {
            kind,
            ty: signature.result.clone(),
            span,
        };
        self.templates.push(CheckedFunction {
            module: "$intrinsic".into(),
            region_sources: None,
            origin: self.current,
            name: format!("{}.{}.{id}", declaration.name, method.name),
            visibility: Visibility::Public,
            exported: false,
            parameters,
            signature,
            body,
            span,
            type_parameters: Vec::new(),
            constraints: Vec::new(),
            members: Vec::new(),
            capture_count: 0,
            is_task: false,
            owned_captures: false,
        });
        self.intrinsics.insert(key, id);
        self.request(id, Vec::new(), span)
    }

    fn string_function(&mut self, ty: &Type, span: Span) -> Result<usize, Diagnostic> {
        if let Some(id) = self.to_strings.get(ty) {
            return self.request(*id, Vec::new(), span);
        }
        let parameter = Local {
            id: 0,
            ty: ty.clone(),
            name: "value".into(),
            mutable: false,
            span,
            provenance: Provenance::Generated,
        };
        let borrowed = Type::Reference(Box::new(ty.clone()), false);
        let body = TypedExpr {
            kind: TypedExprKind::Call(
                Box::new(TypedExpr {
                    kind: TypedExprKind::Method(self.classes.names["Display"], 0, ty.clone()),
                    ty: Type::function(vec![borrowed.clone()], Type::String),
                    span,
                }),
                vec![TypedExpr {
                    kind: TypedExprKind::Borrow(
                        Box::new(TypedExpr {
                            kind: TypedExprKind::Local(0),
                            ty: ty.clone(),
                            span,
                        }),
                        false,
                    ),
                    ty: borrowed,
                    span,
                }],
            ),
            ty: Type::String,
            span,
        };
        let id = self.templates.len();
        self.templates.push(CheckedFunction {
            module: "$builtin".into(),
            region_sources: None,
            origin: self.current,
            name: format!("to_string.{id}"),
            visibility: Visibility::Private,
            exported: false,
            parameters: vec![parameter],
            signature: Signature {
                parameters: vec![ty.clone()],
                result: Type::String,
            },
            body,
            span,
            type_parameters: Vec::new(),
            constraints: Vec::new(),
            members: Vec::new(),
            capture_count: 0,
            is_task: false,
            owned_captures: false,
        });
        self.to_strings.insert(ty.clone(), id);
        self.request(id, Vec::new(), span)
    }
}
