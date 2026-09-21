use super::*;

impl Checker<'_> {
    pub(super) fn lambda(
        &mut self,
        names: &[(Ident, bool)],
        body: &Expr,
        expected: Option<&Type>,
        span: Span,
        task: bool,
    ) -> Result<TypedExpr, Diagnostic> {
        let (parameter_types, result) = if task {
            let result = self.inference.fresh();
            if let Some(expected) = expected {
                self.same(&Type::Task(Box::new(result.clone())), expected, span)?;
            }
            (Vec::new(), self.inference.resolve(&result))
        } else {
            match expected {
                Some(ty) => self.call_signature(ty, names.len(), span)?,
                None => (
                    names.iter().map(|_| self.inference.fresh()).collect(),
                    self.inference.fresh(),
                ),
            }
        };
        let outer: BTreeMap<_, _> = self
            .scopes
            .iter()
            .flat_map(|scope| scope.values())
            .map(|local| (local.id, local.clone()))
            .collect();
        self.scopes.push(BTreeMap::new());
        let mut seen = BTreeSet::new();
        let mut parameters = Vec::new();
        for ((name, mutable), ty) in names.iter().zip(parameter_types) {
            if name.text != "_" && !seen.insert(name.text.clone()) {
                return Err(duplicate(name));
            }
            parameters.push(self.bind(name, ty, *mutable));
        }
        let body = self.expression(body, Some(&result))?;
        self.scopes.pop();
        let mut used = BTreeSet::new();
        free_locals(&body, &mut used);
        let mut captures = Vec::new();
        for id in used {
            if let Some(local) = outer.get(&id) {
                let mut capture = local.clone();
                capture.ty = self.inference.resolve(&capture.ty);
                capture.mutable = false;
                self.require(
                    if task { "Send" } else { "Capture" },
                    capture.ty.clone(),
                    span,
                )?;
                captures.push(capture);
            }
        }
        let ty = if task {
            self.require("Send", body.ty.clone(), body.span)?;
            Type::Task(Box::new(body.ty.clone()))
        } else {
            Type::function(
                parameters
                    .iter()
                    .map(|parameter| parameter.ty.clone())
                    .collect(),
                body.ty.clone(),
            )
        };
        validate_size(&ty, self.record_sizes, span)?;
        if let Some(expected) = expected {
            self.same(&ty, expected, span)?;
        }
        Ok(TypedExpr {
            ty: self.inference.resolve(&ty),
            kind: TypedExprKind::Lambda {
                parameters,
                captures,
                body: Box::new(body),
            },
            span,
        })
    }
}

fn free_locals(expression: &TypedExpr, used: &mut BTreeSet<usize>) {
    use TypedExprKind::*;
    match &expression.kind {
        Local(id) => {
            used.insert(*id);
        }
        Lambda { captures, .. } => {
            used.extend(captures.iter().map(|local| local.id));
        }
        _ => {
            for child in children(expression) {
                free_locals(child, used);
            }
        }
    }
}

fn children(expression: &TypedExpr) -> Vec<&TypedExpr> {
    use TypedExprKind::*;
    match &expression.kind {
        Unary(_, value)
        | Borrow(value, _)
        | Dereference(value)
        | Cast(value)
        | Field(value, _)
        | Length(value)
        | StringLength(value)
        | TaskRun(value)
        | TaskParallel(value) => vec![value],
        Binary(_, a, b) | Assign(a, b) | Index(a, b) | NewArray(a, b) | NewList(a, b) => vec![a, b],
        Call(callee, arguments) => std::iter::once(callee.as_ref()).chain(arguments).collect(),
        If {
            condition,
            then_branch,
            else_branch,
        } => vec![condition, then_branch, else_branch],
        Block { bindings, result } => bindings
            .iter()
            .map(|(_, value)| value)
            .chain(std::iter::once(result.as_ref()))
            .collect(),
        Record(fields) => fields.iter().map(|(_, value)| value).collect(),
        Array(values) | List(values) | Closure(_, values) => values.iter().collect(),
        _ => Vec::new(),
    }
}

pub(super) fn lower(mut module: CheckedModule) -> Result<CheckedModule, Diagnostic> {
    let mut builtins = BTreeMap::new();
    let original_count = module.functions.len();
    for id in 0..original_count {
        let mut body = module.functions[id].body.clone();
        lower_expression(&mut body, &mut module.functions, &mut builtins)?;
        module.functions[id].body = body;
    }
    let mut bridges = Vec::new();
    for (id, function) in module.functions.iter_mut().enumerate() {
        if function.exported && matches!(function.signature.result, Type::Function(..)) {
            let Type::Function(types, result) = function.signature.as_type() else {
                unreachable!()
            };
            let parameters: Vec<_> = types
                .iter()
                .enumerate()
                .map(|(id, ty)| Local {
                    id,
                    ty: ty.clone(),
                    name: format!("arg{id}"),
                    mutable: false,
                    span: function.span,
                })
                .collect();
            let callee = TypedExpr {
                kind: TypedExprKind::Function(FunctionRef::User(id)),
                ty: function.signature.as_type(),
                span: function.span,
            };
            let arguments = parameters.iter().map(local_value).collect();
            bridges.push(CheckedFunction {
                module: "$export".into(),
                name: function.name.clone(),
                exported: true,
                parameters,
                signature: Signature {
                    parameters: types,
                    result: (*result).clone(),
                },
                body: TypedExpr {
                    kind: TypedExprKind::Call(Box::new(callee), arguments),
                    ty: *result,
                    span: function.span,
                },
                span: function.span,
                type_parameters: Vec::new(),
                constraints: Vec::new(),
                capture_count: 0,
                is_task: false,
            });
            function.exported = false;
        }
    }
    module.functions.extend(bridges);
    Ok(module)
}

fn local_value(local: &Local) -> TypedExpr {
    TypedExpr {
        kind: TypedExprKind::Local(local.id),
        ty: local.ty.clone(),
        span: local.span,
    }
}

fn lower_expression(
    expression: &mut TypedExpr,
    functions: &mut Vec<CheckedFunction>,
    builtins: &mut BTreeMap<(Builtin, Type), usize>,
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
        | TaskParallel(value) => lower_expression(value, functions, builtins)?,
        Binary(_, a, b) | Assign(a, b) | Index(a, b) | NewArray(a, b) | NewList(a, b) => {
            lower_expression(a, functions, builtins)?;
            lower_expression(b, functions, builtins)?;
        }
        Call(callee, arguments) => {
            lower_expression(callee, functions, builtins)?;
            for argument in arguments {
                lower_expression(argument, functions, builtins)?;
            }
        }
        If {
            condition,
            then_branch,
            else_branch,
        } => {
            lower_expression(condition, functions, builtins)?;
            lower_expression(then_branch, functions, builtins)?;
            lower_expression(else_branch, functions, builtins)?;
        }
        Block { bindings, result } => {
            for (_, value) in bindings {
                lower_expression(value, functions, builtins)?;
            }
            lower_expression(result, functions, builtins)?;
        }
        Record(fields) => {
            for (_, value) in fields {
                lower_expression(value, functions, builtins)?;
            }
        }
        Array(values) | List(values) => {
            for value in values {
                lower_expression(value, functions, builtins)?;
            }
        }
        Lambda {
            parameters,
            captures,
            body,
        } => {
            lower_expression(body, functions, builtins)?;
            let id = functions.len();
            let mut all_parameters = captures.clone();
            all_parameters.extend(parameters.iter().cloned());
            functions.push(CheckedFunction {
                module: if matches!(expression.ty, Type::Task(_)) {
                    "$task"
                } else {
                    "$lambda"
                }
                .into(),
                name: id.to_string(),
                exported: false,
                signature: Signature {
                    parameters: all_parameters
                        .iter()
                        .map(|local| local.ty.clone())
                        .collect(),
                    result: body.ty.clone(),
                },
                parameters: all_parameters,
                body: (**body).clone(),
                span: expression.span,
                type_parameters: Vec::new(),
                constraints: Vec::new(),
                capture_count: captures.len(),
                is_task: matches!(expression.ty, Type::Task(_)),
            });
            expression.kind = Closure(id, captures.iter().map(local_value).collect());
        }
        Function(FunctionRef::Builtin(builtin)) => {
            let builtin = *builtin;
            let key = (builtin, expression.ty.clone());
            let id = if let Some(id) = builtins.get(&key) {
                *id
            } else {
                let Type::Function(types, _) = &expression.ty else {
                    unreachable!()
                };
                let signature = Signature {
                    parameters: vec![types[0].clone()],
                    result: expression.ty.after_arguments(1),
                };
                let parameter = super::Local {
                    id: 0,
                    ty: signature.parameters[0].clone(),
                    name: "value".into(),
                    mutable: false,
                    span: expression.span,
                };
                let callee = TypedExpr {
                    kind: Function(FunctionRef::Builtin(builtin)),
                    ty: signature.as_type(),
                    span: expression.span,
                };
                let kind = match builtin {
                    Builtin::TaskRun => TaskRun(Box::new(local_value(&parameter))),
                    Builtin::TaskParallel => {
                        let Type::Task(result) = &signature.result else {
                            unreachable!()
                        };
                        Lambda {
                            parameters: Vec::new(),
                            captures: vec![parameter.clone()],
                            body: Box::new(TypedExpr {
                                kind: TaskParallel(Box::new(local_value(&parameter))),
                                ty: (**result).clone(),
                                span: expression.span,
                            }),
                        }
                    }
                    _ => Call(Box::new(callee), vec![local_value(&parameter)]),
                };
                let mut body = TypedExpr {
                    kind,
                    ty: signature.result.clone(),
                    span: expression.span,
                };
                if builtin == Builtin::TaskParallel {
                    lower_expression(&mut body, functions, builtins)?;
                }
                let id = functions.len();
                functions.push(CheckedFunction {
                    module: "$builtin".into(),
                    name: if matches!(builtin, Builtin::TaskRun | Builtin::TaskParallel) {
                        format!("{}.{id}", builtin.name())
                    } else {
                        builtin.name().into()
                    },
                    exported: false,
                    parameters: vec![parameter],
                    signature,
                    body,
                    span: expression.span,
                    type_parameters: Vec::new(),
                    constraints: Vec::new(),
                    capture_count: 0,
                    is_task: false,
                });
                builtins.insert(key, id);
                id
            };
            expression.kind = Function(FunctionRef::User(id));
        }
        _ => {}
    }
    Ok(())
}
