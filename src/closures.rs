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
        validate_size(&ty, &self.types, span)?;
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
            for child in expression.children() {
                free_locals(child, used);
            }
        }
    }
}

/// Generated functions that stand for builtins and case constructors used as
/// values, one per concrete instance.
#[derive(Default)]
struct Generated {
    builtins: BTreeMap<(BuiltinInstance, Type), usize>,
    cases: BTreeMap<(usize, usize, Box<[Type]>), usize>,
}

pub(super) fn lower(mut module: CheckedModule) -> Result<CheckedModule, Diagnostic> {
    let mut generated = Generated::default();
    let original_count = module.functions.len();
    for id in 0..original_count {
        let mut body = module.functions[id].body.clone();
        let origin = module.functions[id].origin.generated(id);
        lower_expression(
            &mut body,
            &mut module.functions,
            &module.unions,
            &mut generated,
            origin,
        )?;
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
                origin: function.origin.generated(id),
                name: function.name.clone(),
                visibility: Visibility::Public,
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

/// Lowers lambdas, builtin values, and case constructor values in
/// `expression` to generated functions, which inherit `origin`.
fn lower_expression(
    expression: &mut TypedExpr,
    functions: &mut Vec<CheckedFunction>,
    unions: &[CheckedUnion],
    generated: &mut Generated,
    origin: FunctionOrigin,
) -> Result<(), Diagnostic> {
    use TypedExprKind::*;
    match &mut expression.kind {
        Lambda {
            parameters,
            captures,
            body,
        } => {
            lower_expression(body, functions, unions, generated, origin)?;
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
                origin,
                name: id.to_string(),
                visibility: Visibility::Public,
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
        Function(FunctionRef::Builtin(instance)) => {
            let key = (instance.clone(), expression.ty.clone());
            let id = if let Some(id) = generated.builtins.get(&key) {
                *id
            } else {
                // One wrapper per instance takes every scheme parameter;
                // closure wrappers provide partial application.
                let builtin = instance.builtin;
                let scheme = builtin.scheme();
                let arity = scheme.parameters.len();
                let Type::Function(types, _) = &expression.ty else {
                    unreachable!()
                };
                let signature = Signature {
                    parameters: types[..arity].to_vec(),
                    result: expression.ty.after_arguments(arity),
                };
                let parameters: Vec<_> = signature
                    .parameters
                    .iter()
                    .enumerate()
                    .map(|(id, ty)| super::Local {
                        id,
                        ty: ty.clone(),
                        name: if arity == 1 {
                            "value".into()
                        } else {
                            format!("value{id}")
                        },
                        mutable: false,
                        span: expression.span,
                    })
                    .collect();
                let callee = TypedExpr {
                    kind: Function(FunctionRef::Builtin(instance.clone())),
                    ty: signature.as_type(),
                    span: expression.span,
                };
                let kind = match builtin {
                    Builtin::TaskRun => TaskRun(Box::new(local_value(&parameters[0]))),
                    Builtin::TaskParallel => {
                        let Type::Task(result) = &signature.result else {
                            unreachable!()
                        };
                        Lambda {
                            parameters: Vec::new(),
                            captures: vec![parameters[0].clone()],
                            body: Box::new(TypedExpr {
                                kind: TaskParallel(Box::new(local_value(&parameters[0]))),
                                ty: (**result).clone(),
                                span: expression.span,
                            }),
                        }
                    }
                    _ => Call(
                        Box::new(callee),
                        parameters.iter().map(local_value).collect(),
                    ),
                };
                let mut body = TypedExpr {
                    kind,
                    ty: signature.result.clone(),
                    span: expression.span,
                };
                if builtin == Builtin::TaskParallel {
                    lower_expression(&mut body, functions, unions, generated, origin)?;
                }
                let id = functions.len();
                functions.push(CheckedFunction {
                    module: "$builtin".into(),
                    origin,
                    // Instances of a polymorphic builtin get distinct symbols.
                    name: if scheme.variables.is_empty() {
                        builtin.name().into()
                    } else {
                        format!("{}.{id}", builtin.name())
                    },
                    visibility: Visibility::Public,
                    exported: false,
                    parameters,
                    signature,
                    body,
                    span: expression.span,
                    type_parameters: Vec::new(),
                    constraints: Vec::new(),
                    capture_count: 0,
                    is_task: false,
                });
                generated.builtins.insert(key, id);
                id
            };
            expression.kind = Function(FunctionRef::User(id));
        }
        CaseConstructor {
            union_id,
            case_id,
            args,
        } => {
            let (union_id, case_id) = (*union_id, *case_id);
            let key = (union_id, case_id, args.clone());
            let id = if let Some(id) = generated.cases.get(&key) {
                *id
            } else {
                let Type::Function(parameters, result) = &expression.ty else {
                    unreachable!("a case constructor is a function value")
                };
                let parameter = super::Local {
                    id: 0,
                    ty: parameters[0].clone(),
                    name: "payload".into(),
                    mutable: false,
                    span: expression.span,
                };
                let id = functions.len();
                let union = &unions[union_id];
                // Instances of a generic union get distinct symbols.
                let instance = if args.is_empty() {
                    std::string::String::new()
                } else {
                    format!(".$mono.{id}")
                };
                functions.push(CheckedFunction {
                    module: "$case".into(),
                    origin,
                    name: format!("{}.{}{instance}", union.name, union.cases[case_id].0),
                    visibility: Visibility::Public,
                    exported: false,
                    signature: Signature {
                        parameters: vec![parameter.ty.clone()],
                        result: (**result).clone(),
                    },
                    body: TypedExpr {
                        kind: Construct {
                            union_id,
                            case_id,
                            payload: Some(Box::new(local_value(&parameter))),
                        },
                        ty: (**result).clone(),
                        span: expression.span,
                    },
                    parameters: vec![parameter],
                    span: expression.span,
                    type_parameters: Vec::new(),
                    constraints: Vec::new(),
                    capture_count: 0,
                    is_task: false,
                });
                generated.cases.insert(key, id);
                id
            };
            expression.kind = Function(FunctionRef::User(id));
        }
        _ => {
            for child in expression.children_mut() {
                lower_expression(child, functions, unions, generated, origin)?;
            }
        }
    }
    Ok(())
}
