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
    functions: &[CheckedFunction],
    names: &Names,
    classes: &Classes,
    types: &TypeContext<'_>,
) -> Result<ResolvedApi, Diagnostic> {
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
    let functions: BTreeMap<_, _> = functions
        .iter()
        .map(|function| (function.qualified_name(), function))
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
            let Some(function) = functions.get(&format!("{name}.{}", declaration.name.text)) else {
                return Err(Diagnostic::new(
                    "E1016",
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
                    "{:?}:{:?}:{:?}:{:?}",
                    function.region_sources,
                    function.region_slots,
                    function.callback_contracts,
                    specialization_obligation,
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
                return Err(Diagnostic::new(
                    "E1016",
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
                return Err(Diagnostic::new(
                    "E1016",
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
                    return Err(Diagnostic::new(
                        "E1016",
                        "effective API class method is unavailable",
                        method.name.span,
                    ));
                };
                result
                    .signatures
                    .insert(key(method.name.span), signature(resolved, types));
            }
        }
        for declaration in &program.instances {
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
