use super::*;

#[path = "closures_rec.rs"]
mod letrec;

/// The value a lambda expression becomes.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum LambdaKind {
    /// A reusable function value. Copying it copies the captures, which need `Capture`.
    Function,
    /// A single-use `task`. Its captures and result need `Send`.
    Task,
    /// The argument of `Owned.function` (B07): one parameter and borrowed `Send` captures.
    Owned,
}

impl Checker<'_> {
    /// `recursive` holds the `let rec` group that the lambda is a member of (B09; empty for any
    /// other lambda): the group's names are in scope in the body, and the lambda does not capture
    /// them.
    pub(super) fn lambda(
        &mut self,
        names: &[(Ident, bool)],
        body: &Expr,
        expected: Option<&Type>,
        span: Span,
        kind: LambdaKind,
        recursive: &[Local],
    ) -> Result<TypedExpr, Diagnostic> {
        if kind == LambdaKind::Owned && names.len() != 1 {
            return Err(Diagnostic::new(
                "E1006",
                "Owned.function takes a lambda with one parameter; take a tuple or return another function",
                span,
            ));
        }
        let (parameter_types, result) = if kind == LambdaKind::Task {
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
        let mut outer: BTreeMap<_, _> = self
            .scopes
            .iter()
            .flat_map(|scope| scope.values())
            .map(|local| (local.id, local.clone()))
            .collect();
        letrec::leave_out(&mut outer, recursive);
        self.scopes.push(BTreeMap::new());
        let mut seen = BTreeSet::new();
        let mut parameters = Vec::new();
        let conventions = expected
            .map(|ty| self.inference.resolve(ty))
            .and_then(|ty| match ty {
                Type::Function(parameters, _) => Some(parameters.conventions),
                _ => None,
            })
            .unwrap_or_default();
        for (index, ((name, mutable), ty)) in names.iter().zip(parameter_types).enumerate() {
            if name.text != "_" && !seen.insert(name.text.clone()) {
                return Err(duplicate(name));
            }
            let local = self.bind(name, ty, *mutable);
            if matches!(
                conventions.get(index),
                Some(ParameterConvention::Read | ParameterConvention::Mut)
            ) && matches!(local.ty, Type::Reference(..))
            {
                self.read_value_parameters.insert(local.id);
            }
            parameters.push(local);
        }
        let outer_loop_depth = std::mem::take(&mut self.normal_loop_depth);
        let body = self.expression(body, Some(&result));
        self.normal_loop_depth = outer_loop_depth;
        let body = body?;
        self.scopes.pop();
        self.group_lambda(parameters, body, &outer, expected, span, kind, recursive)
    }

    pub(super) fn checked_lambda(
        &mut self,
        parameters: Vec<Local>,
        body: TypedExpr,
        outer: &BTreeMap<usize, Local>,
        expected: Option<&Type>,
        span: Span,
        kind: LambdaKind,
    ) -> Result<TypedExpr, Diagnostic> {
        self.group_lambda(parameters, body, outer, expected, span, kind, &[])
    }

    /// The lambda of the function `parameters -> body`, with the captures it takes from `outer`;
    /// `recursive` is the `let rec` group that it is a member of, if any (B09).
    #[allow(clippy::too_many_arguments)]
    fn group_lambda(
        &mut self,
        parameters: Vec<Local>,
        body: TypedExpr,
        outer: &BTreeMap<usize, Local>,
        expected: Option<&Type>,
        span: Span,
        kind: LambdaKind,
        recursive: &[Local],
    ) -> Result<TypedExpr, Diagnostic> {
        let task = kind == LambdaKind::Task;
        let mut used = Vec::new();
        free_locals(&body, &mut used);
        let mut captures = Vec::new();
        for id in used.iter().enumerate().flat_map(|(word, &bits)| {
            (0..64).filter_map(move |bit| (bits & (1u64 << bit) != 0).then_some(word * 64 + bit))
        }) {
            if let Some(local) = outer.get(&id) {
                let mut capture = local.clone();
                capture.ty = self.inference.resolve(&capture.ty);
                capture.mutable = false;
                let class = if kind == LambdaKind::Function {
                    "Capture"
                } else {
                    "Send"
                };
                self.require(class, capture.ty.clone(), span)
                    .map_err(|error| {
                        if kind != LambdaKind::Owned
                            || error.code != "E1013"
                            || capture.ty.holds_rc(&self.types)
                            || capture.ty.holds_unsync_arc(&self.types)
                        {
                            return error;
                        }
                        Diagnostic::new(
                            "E1013",
                            format!(
                                "an owned function captures only owned values; {} contains a reference",
                                capture.ty.display(&self.types)
                            ),
                            span,
                        )
                    })?;
                captures.push(capture);
            }
        }
        let ty = if task {
            self.require("Send", body.ty.clone(), body.span)?;
            Type::Task(Box::new(body.ty.clone()))
        } else {
            let pure = expected.is_some_and(|ty| ty.function_purity() == Some(true));
            self.pure_values |= pure;
            Type::function_with_conventions(
                parameters
                    .iter()
                    .map(|parameter| parameter.ty.clone())
                    .collect(),
                body.ty.clone(),
                Vec::new(),
                pure,
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
                owned: kind == LambdaKind::Owned,
                recursive: recursive.to_vec(),
            },
            span,
        })
    }
}

fn note_local(used: &mut Vec<u64>, id: usize) {
    let word = id / 64;
    if word >= used.len() {
        used.resize(word + 1, 0);
    }
    used[word] |= 1 << (id % 64);
}

fn free_locals(expression: &TypedExpr, used: &mut Vec<u64>) {
    use TypedExprKind::*;
    match &expression.kind {
        Local(id) => {
            note_local(used, *id);
        }
        Lambda { captures, .. } => {
            for local in captures {
                note_local(used, local.id);
            }
        }
        _ => {
            expression
                .try_for_each_child(&mut |child| {
                    free_locals(child, used);
                    Ok(())
                })
                .expect("collecting free locals cannot fail");
        }
    }
}

/// Generated functions that stand for builtins and case constructors used as
/// values, one per concrete instance.
#[derive(Default)]
struct Generated {
    builtins: BTreeMap<(BuiltinInstance, Type), usize>,
    cases: BTreeMap<(usize, usize, Box<[Type]>), usize>,
    /// The `let rec` functions of the function being lowered (B09).
    recursive: letrec::State,
}

pub(super) fn lower(mut module: CheckedModule) -> Result<CheckedModule, Diagnostic> {
    let mut generated = Generated::default();
    let original_count = module.functions.len();
    for id in 0..original_count {
        let span = module.functions[id].body.span;
        let mut body = std::mem::replace(&mut module.functions[id].body, TypedExpr::error(span));
        let origin = module.functions[id].origin.generated(id);
        let types = TypeContext {
            records: &module.records,
            unions: &module.unions,
        };
        generated.recursive.start(&body);
        lower_expression(
            &mut body,
            &mut module.functions,
            &types,
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
                    provenance: Provenance::Generated,
                })
                .collect();
            let callee = TypedExpr {
                kind: TypedExprKind::Function(FunctionRef::User(id)),
                ty: function.signature.as_type(),
                span: function.span,
            };
            let arguments = parameters.iter().map(local_value).collect();
            bridges.push(CheckedFunction {
                read_by_default: Vec::new(),
                module: "$export".into(),
                region_sources: None,
                callback_contracts: Vec::new(),
                region_slots: None,
                origin: function.origin.generated(id),
                name: function.name.clone(),
                visibility: Visibility::Public,
                exported: true,
                parameters,
                signature: Signature {
                    parameters: types.types,
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
                members: Vec::new(),
                capture_count: 0,
                is_task: false,
                owned_captures: false,
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
    types: &TypeContext<'_>,
    generated: &mut Generated,
    origin: FunctionOrigin,
) -> Result<(), Diagnostic> {
    use TypedExprKind::*;
    // A block that binds a `let rec` group lifts it as a group (B09). This is checked before the
    // match, and returned as it is, which keeps this frame, on the stack for every level of an
    // expression, as small as it was.
    if letrec::involves(expression) {
        return letrec::lower(expression, functions, types, generated, origin);
    }
    match &mut expression.kind {
        Lambda {
            parameters,
            captures,
            body,
            owned,
            ..
        } => {
            letrec::lower_body(body, functions, types, generated, origin)?;
            if *owned {
                // The environment outlives each call, so the body must not move captures.
                for capture in captures.iter() {
                    if capture.ty.is_copy(types) {
                        continue;
                    }
                    if let Some(span) = body.consuming_use(capture.id, types) {
                        return Err(Diagnostic::new(
                            "E1012",
                            format!(
                                "cannot move '{}' out of an owned function; borrow it with 'ref' instead",
                                capture.name
                            ),
                            span,
                        ));
                    }
                }
            }
            let id = functions.len();
            let mut all_parameters = captures.clone();
            all_parameters.extend(parameters.iter().cloned());
            functions.push(CheckedFunction {
                read_by_default: Vec::new(),
                region_sources: None,
                callback_contracts: Vec::new(),
                region_slots: None,
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
                members: Vec::new(),
                capture_count: captures.len(),
                is_task: matches!(expression.ty, Type::Task(_)),
                owned_captures: *owned,
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
                let Type::Function(parameter_types, _) = &expression.ty else {
                    unreachable!()
                };
                let signature = Signature {
                    parameters: parameter_types[..arity].to_vec(),
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
                        provenance: Provenance::Generated,
                    })
                    .collect();
                let callee = TypedExpr {
                    kind: Function(FunctionRef::Builtin(instance.clone())),
                    ty: signature.as_type(),
                    span: expression.span,
                };
                let kind = match builtin {
                    Builtin::TaskRun => TaskRun(Box::new(local_value(&parameters[0]))),
                    Builtin::TaskParallel | Builtin::TaskParallelResults => {
                        let Type::Task(result) = &signature.result else {
                            unreachable!()
                        };
                        Lambda {
                            parameters: Vec::new(),
                            captures: vec![parameters[0].clone()],
                            body: Box::new(TypedExpr {
                                kind: if builtin == Builtin::TaskParallelResults {
                                    TaskParallelResults(Box::new(local_value(&parameters[0])))
                                } else {
                                    TaskParallel(Box::new(local_value(&parameters[0])))
                                },
                                ty: (**result).clone(),
                                span: expression.span,
                            }),
                            owned: false,
                            recursive: Vec::new(),
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
                if matches!(
                    builtin,
                    Builtin::TaskParallel | Builtin::TaskParallelResults
                ) {
                    lower_expression(&mut body, functions, types, generated, origin)?;
                }
                let id = functions.len();
                let region_slots = if builtin == Builtin::VecPushMut
                    && signature.parameters[0].carries_loans(types)
                {
                    let count = match &signature.parameters[0] {
                        Type::Record(id, _) => types.records[*id].region_count.max(1),
                        _ => 1,
                    };
                    Some(RegionSlots {
                        parameters: vec![vec![Some(0); count], vec![Some(1), Some(0)]],
                        targets: vec![false, true],
                        result_target: false,
                        writes: true,
                    })
                } else {
                    None
                };
                functions.push(CheckedFunction {
                    read_by_default: Vec::new(),
                    module: "$builtin".into(),
                    region_sources: None,
                    callback_contracts: Vec::new(),
                    region_slots,
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
                    members: Vec::new(),
                    capture_count: 0,
                    is_task: false,
                    owned_captures: false,
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
                    provenance: Provenance::Generated,
                };
                let id = functions.len();
                let union = &types.unions[union_id];
                // Instances of a generic union get distinct symbols.
                let instance = if args.is_empty() {
                    std::string::String::new()
                } else {
                    format!(".$mono.{id}")
                };
                functions.push(CheckedFunction {
                    read_by_default: Vec::new(),
                    module: "$case".into(),
                    region_sources: None,
                    callback_contracts: Vec::new(),
                    region_slots: None,
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
                    members: Vec::new(),
                    capture_count: 0,
                    is_task: false,
                    owned_captures: false,
                });
                generated.cases.insert(key, id);
                id
            };
            expression.kind = Function(FunctionRef::User(id));
        }
        _ => {
            expression.try_for_each_child_mut(&mut |child| {
                lower_expression(child, functions, types, generated, origin)
            })?;
        }
    }
    Ok(())
}
