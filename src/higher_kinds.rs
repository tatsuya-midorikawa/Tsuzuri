use super::*;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Constructor {
    Record(usize, usize),
    Union(usize, usize),
    Array,
    List,
    Vec,
    Task,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Partial {
    pub constructor: Constructor,
    pub trailing: Box<[Type]>,
}

impl Constructor {
    pub fn arity(&self) -> usize {
        match self {
            Self::Record(_, arity) | Self::Union(_, arity) => *arity,
            _ => 1,
        }
    }

    pub(super) fn name(&self, types: &TypeContext<'_>) -> String {
        match self {
            Self::Record(id, _) => types.records[*id].name.clone(),
            Self::Union(id, _) => types.unions[*id].name.clone(),
            Self::Array => "Array".into(),
            Self::List => "List".into(),
            Self::Vec => "Vec".into(),
            Self::Task => "Task".into(),
        }
    }
}

impl Partial {
    pub fn display(&self, types: &TypeContext<'_>) -> String {
        let name = self.constructor.name(types);
        if self.trailing.is_empty() {
            name
        } else {
            format!(
                "{name}<{}>",
                self.trailing
                    .iter()
                    .map(|ty| ty.display(types))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        }
    }
}

pub(super) fn apply(head: Type, arguments: Box<[Type]>) -> Type {
    if let Type::Partial(partial) = &head
        && arguments.len() + partial.trailing.len() == partial.constructor.arity()
    {
        let mut complete = arguments.into_vec();
        complete.extend(partial.trailing.iter().cloned());
        return match partial.constructor {
            Constructor::Record(id, _) => Type::Record(id, complete.into()),
            Constructor::Union(id, _) => Type::Union(id, complete.into()),
            Constructor::Array => Type::Array(Box::new(complete.remove(0))),
            Constructor::List => Type::List(Box::new(complete.remove(0))),
            Constructor::Vec => Type::Vec(Box::new(complete.remove(0))),
            Constructor::Task => Type::Task(Box::new(complete.remove(0))),
        };
    }
    Type::Application(Box::new(head), arguments)
}

pub(super) fn decompose(ty: &Type) -> Option<(Constructor, Box<[Type]>)> {
    match ty {
        Type::Record(id, arguments) => {
            Some((Constructor::Record(*id, arguments.len()), arguments.clone()))
        }
        Type::Union(id, arguments) => {
            Some((Constructor::Union(*id, arguments.len()), arguments.clone()))
        }
        Type::Array(element) => Some((Constructor::Array, vec![(**element).clone()].into())),
        Type::List(element) => Some((Constructor::List, vec![(**element).clone()].into())),
        Type::Vec(element) => Some((Constructor::Vec, vec![(**element).clone()].into())),
        Type::Task(element) => Some((Constructor::Task, vec![(**element).clone()].into())),
        _ => None,
    }
}

pub(super) fn arity(kind: &Kind, span: Span) -> Result<usize, Diagnostic> {
    let mut current = kind;
    let mut count = 0;
    while let Kind::Arrow(argument, result) = current {
        if **argument != Kind::Type {
            return Err(Diagnostic::new(
                "E1015",
                "phase-1 constructors accept value types, not higher-order kind arguments",
                span,
            ));
        }
        count += 1;
        current = result;
    }
    Ok(count)
}

pub(super) fn resolve_constructor(
    expression: &TypeExpr,
    expected: usize,
    module: &str,
    names: &Names,
) -> Result<Type, Diagnostic> {
    if expected == 0 {
        return resolve_type(expression, module, names);
    }
    let empty = [];
    let (name, arguments): (&str, &[TypeExpr]) = match &expression.kind {
        TypeExprKind::Named(name) => (name, &empty),
        TypeExprKind::Apply(head, arguments) if !head.text.starts_with('\'') => {
            (&head.text, arguments)
        }
        TypeExprKind::Variable(name) => return Ok(Type::Variable(name.clone())),
        _ => {
            return Err(Diagnostic::new(
                "E1015",
                "expected a named type constructor with the declared kind",
                expression.span,
            ));
        }
    };
    if crate::numeric::primitive(name).is_some() {
        return Err(Diagnostic::new(
            "E1015",
            "a scalar value type is not a type constructor",
            expression.span,
        ));
    }
    let constructor = match name {
        "Array" => Constructor::Array,
        "List" => Constructor::List,
        "Vec" => Constructor::Vec,
        "Task" => Constructor::Task,
        _ => match names.named_type(module, name, expression.span)? {
            NamedType::Record(info) => Constructor::Record(info.id, names.record_arities[info.id]),
            NamedType::Union(info) => Constructor::Union(info.id, names.union_arities[info.id]),
            NamedType::Alias(_) => {
                return Err(Diagnostic::new(
                    "E1015",
                    "higher-kinded type aliases are not supported; use the underlying constructor",
                    expression.span,
                ));
            }
        },
    };
    if constructor.arity().checked_sub(arguments.len()) != Some(expected) {
        return Err(Diagnostic::new(
            "E1015",
            "type constructor kind does not match the class parameter",
            expression.span,
        ));
    }
    let trailing = arguments
        .iter()
        .map(|argument| resolve_type(argument, module, names))
        .collect::<Result<_, _>>()?;
    Ok(Type::Partial(Box::new(Partial {
        constructor,
        trailing,
    })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn application_inference_and_substitution_saturate_constructors() {
        let types = TypeContext {
            records: &[],
            unions: &[],
        };
        let mut inference = Inference::default();
        let head = inference.fresh();
        let applied = Type::Application(Box::new(head), vec![Type::I64].into());
        let array = Type::Array(Box::new(Type::I64));
        inference
            .unify(&applied, &array, &types, Span::default())
            .unwrap();
        assert_eq!(inference.resolve(&applied), array);
        let partial = Type::Partial(Box::new(Partial {
            constructor: Constructor::Union(0, 2),
            trailing: vec![Type::String].into(),
        }));
        let applied =
            Type::Application(Box::new(Type::Variable("f".into())), vec![Type::I64].into());
        assert_eq!(
            polymorph::substitute(&applied, &BTreeMap::from([("f".into(), partial)])),
            Type::Union(0, vec![Type::I64, Type::String].into())
        );
    }
}
