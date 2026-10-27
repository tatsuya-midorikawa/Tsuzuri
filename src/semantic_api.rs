use super::*;

type Key = (Option<usize>, usize, usize);

pub(crate) fn key(span: Span) -> Key {
    (span.source, span.start, span.end)
}

#[derive(Clone, Debug)]
pub(crate) struct ApiSignature {
    pub parameters: Vec<TypeExpr>,
    pub result: TypeExpr,
}

/// Resolved API facts captured before generic specialization, only when explicitly requested.
#[derive(Clone, Debug, Default)]
pub struct ResolvedApi {
    pub(crate) signatures: BTreeMap<Key, ApiSignature>,
    pub(crate) requirements: BTreeMap<Key, Vec<ConstraintExpr>>,
    pub(crate) annotations: BTreeMap<Key, TypeExpr>,
    pub(crate) public_names: BTreeMap<String, String>,
    pub(crate) classes: BTreeMap<Key, String>,
    pub(crate) contracts: BTreeMap<Key, String>,
}

fn expression(ty: &Type, types: &TypeContext<'_>) -> TypeExpr {
    polymorph::type_expression(ty, types, Span::default())
}

fn signature(signature: &Signature, types: &TypeContext<'_>) -> ApiSignature {
    ApiSignature {
        parameters: signature
            .parameters
            .iter()
            .map(|ty| expression(ty, types))
            .collect(),
        result: expression(&signature.result, types),
    }
}

fn purity_obligations(module: &CheckedModule) -> Result<Vec<BTreeSet<String>>, Diagnostic> {
    let functions = &module.functions;
    let mut count = 0;
    let parameters: Vec<BTreeMap<_, _>> = functions
        .iter()
        .map(|function| {
            function
                .type_parameters
                .iter()
                .map(|name| {
                    let id = count;
                    count += 1;
                    (name.as_str(), id)
                })
                .collect()
        })
        .collect();
    let mut dependents = vec![Vec::new(); count];
    let mut required = vec![false; count];
    let mut pending = Vec::new();
    for (caller, function) in functions.iter().enumerate() {
        let mut seeds = BTreeSet::new();
        if function.origin.pure {
            seeds.extend(function.type_parameters.iter().cloned());
        }
        let mut callback_values = Vec::new();
        let mut aliases = BTreeMap::new();
        let mut expressions = vec![&function.body];
        while let Some(expression) = expressions.pop() {
            if let TypedExprKind::Block { bindings, .. } = &expression.kind {
                aliases.extend(
                    bindings
                        .iter()
                        .filter(|(local, _)| !local.mutable)
                        .map(|(local, value)| (local.id, value)),
                );
            }
            if expression.ty.function_purity() == Some(true) {
                seeds.extend(polymorph::variables(&expression.ty));
                match &expression.kind {
                    TypedExprKind::Lambda { captures, .. } => {
                        seeds.extend(
                            captures
                                .iter()
                                .flat_map(|capture| polymorph::variables(&capture.ty)),
                        );
                        callback_values.push(expression);
                    }
                    TypedExprKind::GenericFunction(_, arguments) => {
                        seeds.extend(arguments.iter().flat_map(polymorph::variables))
                    }
                    TypedExprKind::Method(_, _, receiver)
                    | TypedExprKind::TypeFunction { receiver, .. } => {
                        seeds.extend(polymorph::variables(receiver));
                    }
                    _ => {}
                }
            }
            if let TypedExprKind::GenericFunction(callee, arguments) = &expression.kind {
                for (parameter, argument) in
                    functions[*callee].type_parameters.iter().zip(arguments)
                {
                    let callee = parameters[*callee][parameter.as_str()];
                    for variable in polymorph::variables(argument) {
                        if let Some(&caller) = parameters[caller].get(variable.as_str()) {
                            dependents[callee].push(caller);
                        }
                    }
                }
            }
            expressions.extend(expression.children());
        }
        if !function.origin.pure {
            seeds.extend(pure_rules::callback_variables(
                module,
                callback_values,
                aliases,
            )?);
        }
        for variable in seeds {
            if let Some(&id) = parameters[caller].get(variable.as_str()) {
                required[id] = true;
                pending.push(id);
            }
        }
    }
    // Each parameter enters the worklist once, including through mutually recursive helpers.
    while let Some(callee) = pending.pop() {
        for &caller in &dependents[callee] {
            if !required[caller] {
                required[caller] = true;
                pending.push(caller);
            }
        }
    }
    Ok(functions
        .iter()
        .enumerate()
        .map(|(id, function)| {
            function
                .type_parameters
                .iter()
                .filter(|name| required[parameters[id][name.as_str()]])
                .cloned()
                .collect()
        })
        .collect())
}

fn obligation_mask<'a>(
    types: impl IntoIterator<Item = &'a Type>,
    required: &BTreeSet<String>,
) -> Vec<bool> {
    // Signature occurrence order keeps the proof independent of type-parameter spelling.
    let mut mask = Vec::new();
    for ty in types {
        polymorph::map_type(ty, &mut |ty| {
            if let Type::Variable(name) = ty {
                mask.push(required.contains(name));
            }
            ty.clone()
        });
    }
    mask
}

impl ResolvedApi {
    fn constraints(
        &mut self,
        expressions: &[ConstraintExpr],
        module: &str,
        names: &Names,
        classes: &Classes,
        types: &TypeContext<'_>,
    ) -> Result<(), Diagnostic> {
        let kinds = classes.constraint_kinds(expressions, module, names)?;
        for constraint in expressions {
            let ty = classes.constraint_type(constraint, module, names, &kinds)?;
            self.annotations
                .insert(key(constraint.ty.span), expression(&ty, types));
            match &constraint.name {
                ConstraintName::Class(name) => {
                    self.classes.insert(
                        key(name.span),
                        key_path(classes.class_name(name, module, names)?),
                    );
                }
                ConstraintName::Function(_, Some(annotation)) => {
                    let ty = resolve_type_with_kinds(annotation, module, names, &kinds)?;
                    self.annotations
                        .insert(key(annotation.span), expression(&ty, types));
                }
                ConstraintName::Function(_, None) => {}
            }
        }
        Ok(())
    }
}

pub(in crate::check) fn collect(
    modules: &[ModuleInput<'_>],
    checked: &CheckedModule,
    names: &Names,
    classes: &Classes,
) -> Result<ResolvedApi, Diagnostic> {
    let functions = &checked.functions;
    let types = &checked.types();
    let mut result = ResolvedApi::default();
    for name in names
        .records
        .keys()
        .chain(names.unions.keys())
        .chain(names.handles.keys())
        .chain(&names.classes)
    {
        let shown = name.rsplit_once('.').map_or_else(
            || name.clone(),
            |(module, declared)| type_name(&names.module_display(module), module, declared),
        );
        result.public_names.insert(key_path(name), shown);
    }
    let obligations = purity_obligations(checked)?;
    let indexed_functions = functions;
    let mut instance_functions: BTreeMap<_, BTreeMap<_, _>> = BTreeMap::new();
    for (span, head, name, id, arguments) in classes.instance_functions() {
        instance_functions
            .entry(key(span))
            .or_default()
            .insert(name, (id, arguments, head));
    }
    let functions: BTreeMap<_, _> = functions
        .iter()
        .enumerate()
        .map(|(id, function)| (function.qualified_name(), (id, function)))
        .collect();
    let records: BTreeMap<_, _> = types
        .records
        .iter()
        .map(|record| (record.name.as_str(), record))
        .collect();
    let unions: BTreeMap<_, _> = types
        .unions
        .iter()
        .map(|union| (union.name.as_str(), union))
        .collect();
    let standard_root = modules
        .iter()
        .all(|module| module.origin == ModuleOrigin::Std);
    for module in modules
        .iter()
        .filter(|module| standard_root || module.origin == ModuleOrigin::User)
    {
        let program = module.program;
        let name = module.name;
        for declaration in &program.functions {
            let Some((id, function)) = functions.get(&format!("{name}.{}", declaration.name.text))
            else {
                return Err(Diagnostic::of(
                    DiagnosticKind::InternalEffectiveApiUnavailable,
                    "effective API signature is unavailable",
                    declaration.name.span,
                ));
            };
            result.signatures.insert(
                key(declaration.name.span),
                signature(&function.signature, types),
            );
            let specialization_obligation = function.origin.pure
                && (!function.type_parameters.is_empty() || !function.constraints.is_empty());
            // Generic purity adds a specialization proof, even when the resolved ABI is unchanged.
            result.contracts.insert(
                key(declaration.name.span),
                format!(
                    "{:?}:{:?}:{:?}:{:?}:{:?}",
                    function.region_sources,
                    function.region_slots,
                    function.callback_contracts,
                    specialization_obligation,
                    obligation_mask(
                        function
                            .signature
                            .parameters
                            .iter()
                            .chain([&function.signature.result]),
                        &obligations[*id]
                    ),
                ),
            );
            result.constraints(&declaration.constraints, name, names, classes, types)?;
            let mut constraints = classes.constraint_expressions(&function.constraints, types)?;
            // Typed uses refine bare declarations; concrete receivers have already been solved.
            let typed_members: BTreeSet<_> = function
                .members
                .iter()
                .filter(|member| member.signature.is_some())
                .map(|member| (&member.receiver, member.name.as_str()))
                .collect();
            for member in &function.members {
                if !polymorph::is_unknown(&member.receiver)
                    || (member.signature.is_none()
                        && typed_members.contains(&(&member.receiver, member.name.as_str())))
                {
                    continue;
                }
                constraints.push(ConstraintExpr {
                    name: ConstraintName::Function(
                        Ident {
                            text: member.name.clone(),
                            span: Span::default(),
                            provenance: Provenance::Generated,
                        },
                        member
                            .signature
                            .as_ref()
                            .map(|ty| Box::new(expression(ty, types))),
                    ),
                    ty: expression(&member.receiver, types),
                });
            }
            result
                .requirements
                .insert(key(declaration.name.span), constraints);
        }
        for declaration in &program.externs {
            let parameters = declaration
                .parameters
                .iter()
                .map(|ty| {
                    let ty = match &ty.kind {
                        TypeExprKind::Own(inner) => inner.as_ref(),
                        _ => ty,
                    };
                    resolve_type(ty, name, names)
                })
                .collect::<Result<_, _>>()?;
            let resolved = Signature {
                parameters,
                result: resolve_type(&declaration.result, name, names)?,
            };
            result
                .signatures
                .insert(key(declaration.name.span), signature(&resolved, types));
        }
        for declaration in &program.records {
            let Some(record) = records.get(format!("{name}.{}", declaration.name.text).as_str())
            else {
                return Err(Diagnostic::of(
                    DiagnosticKind::InternalEffectiveApiUnavailable,
                    "effective API record is unavailable",
                    declaration.name.span,
                ));
            };
            for (position, (field, (_, ty))) in
                declaration.fields.iter().zip(&record.fields).enumerate()
            {
                result
                    .annotations
                    .insert(key(field.ty.span), expression(ty, types));
                result.contracts.insert(
                    key(field.name.span),
                    format!(
                        "{:?}:{:?}",
                        record.field_regions.get(position),
                        record.field_targets.get(position),
                    ),
                );
            }
        }
        for declaration in &program.unions {
            let Some(union) = unions.get(format!("{name}.{}", declaration.name.text).as_str())
            else {
                return Err(Diagnostic::of(
                    DiagnosticKind::InternalEffectiveApiUnavailable,
                    "effective API union is unavailable",
                    declaration.name.span,
                ));
            };
            for (case, (_, ty)) in declaration.cases.iter().zip(&union.cases) {
                if let (Some(source), Some(ty)) = (&case.payload, ty) {
                    result
                        .annotations
                        .insert(key(source.span), expression(ty, types));
                }
            }
        }
        for declaration in program
            .type_aliases
            .iter()
            .filter(|declaration| declaration.visibility != Visibility::Private)
        {
            let ty = resolve_type(&declaration.target, name, names)?;
            result
                .annotations
                .insert(key(declaration.target.span), expression(&ty, types));
            let constraints = classes.inline_constraints(&declaration.target, name, names)?;
            result.requirements.insert(
                key(declaration.name.span),
                classes.constraint_expressions(&constraints, types)?,
            );
        }
        for declaration in &program.constants {
            let ty = resolve_type(&declaration.ty, name, names)?;
            result
                .annotations
                .insert(key(declaration.ty.span), expression(&ty, types));
        }
        for declaration in &program.classes {
            result.constraints(&declaration.superclasses, name, names, classes, types)?;
            for method in &declaration.methods {
                let Some(resolved) = classes.method_signature(
                    &format!("{name}.{}", declaration.name.text),
                    &method.name.text,
                ) else {
                    return Err(Diagnostic::of(
                        DiagnosticKind::InternalEffectiveApiUnavailable,
                        "effective API class method is unavailable",
                        method.name.span,
                    ));
                };
                result
                    .signatures
                    .insert(key(method.name.span), signature(resolved, types));
                if let Some(id) = classes.default_function(
                    &format!("{name}.{}", declaration.name.text),
                    &method.name.text,
                ) {
                    let function = &indexed_functions[id];
                    let receiver = Type::Variable(declaration.variable.text.clone());
                    let mask = obligation_mask(
                        [&receiver].into_iter().chain(
                            function
                                .signature
                                .parameters
                                .iter()
                                .chain([&function.signature.result]),
                        ),
                        &obligations[id],
                    );
                    if mask.contains(&true) {
                        result
                            .contracts
                            .insert(key(method.name.span), format!("{mask:?}"));
                    }
                }
            }
        }
        for declaration in &program.instances {
            if let Some(methods) = instance_functions.get(&key(declaration.class.span)) {
                let mut masks = BTreeMap::new();
                for (name, (id, arguments, head)) in methods {
                    let function = &indexed_functions[*id];
                    let substitutions: BTreeMap<_, _> = function
                        .type_parameters
                        .iter()
                        .cloned()
                        .zip(arguments.iter().cloned())
                        .collect();
                    let required = obligations[*id]
                        .iter()
                        .flat_map(|name| {
                            substitutions
                                .get(name)
                                .map_or_else(|| vec![name.clone()], polymorph::variables)
                        })
                        .collect();
                    let signature =
                        polymorph::substitute(&function.signature.as_type(), &substitutions);
                    let mask = obligation_mask([*head, &signature], &required);
                    if mask.contains(&true) {
                        masks.insert(*name, mask);
                    }
                }
                if !masks.is_empty() {
                    result
                        .contracts
                        .insert(key(declaration.class.span), format!("{masks:?}"));
                }
            }
            let head = ConstraintExpr {
                name: ConstraintName::Class(declaration.class.clone()),
                ty: declaration.ty.clone(),
            };
            result.constraints(std::slice::from_ref(&head), name, names, classes, types)?;
            result.constraints(&declaration.constraints, name, names, classes, types)?;
        }
    }
    Ok(result)
}
