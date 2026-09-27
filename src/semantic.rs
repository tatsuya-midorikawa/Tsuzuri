use super::*;

#[derive(Clone, Debug)]
pub struct SemanticEntry {
    pub span: Span,
    pub detail: String,
    pub target: Option<Span>,
    pub priority: u8,
}

#[derive(Clone, Debug)]
pub struct DocumentSymbol {
    pub name: String,
    pub kind: u8,
    pub span: Span,
    pub selection: Span,
}

#[derive(Clone, Debug, Default)]
pub struct SemanticIndex {
    pub entries: Vec<SemanticEntry>,
    pub symbols: Vec<DocumentSymbol>,
    pub documentation: Vec<(Span, String)>,
}

impl SemanticIndex {
    pub fn doc_for(&self, target: Span) -> Option<&str> {
        self.documentation
            .iter()
            .find(|(span, _)| *span == target)
            .map(|(_, text)| text.as_str())
    }

    fn document(&mut self, name: &Ident, doc: Option<&Documentation>) {
        if let Some(doc) = doc {
            self.documentation.push((name.span, doc.text.clone()));
        }
    }

    pub fn at(&self, source: usize, offset: usize) -> Option<&SemanticEntry> {
        self.entries
            .iter()
            .filter(|entry| {
                entry.span.source == Some(source)
                    && entry.span.start <= offset
                    && offset < entry.span.end
            })
            .min_by_key(|entry| (entry.span.end - entry.span.start, entry.priority))
    }

    fn symbol(&mut self, name: &Ident, kind: u8, end: Span, detail: String) {
        self.symbols.push(DocumentSymbol {
            name: name.text.clone(),
            kind,
            span: name.span.through(end),
            selection: name.span,
        });
        self.entries.push(SemanticEntry {
            span: name.span,
            detail,
            target: Some(name.span),
            priority: 2,
        });
    }
}

pub(super) fn collect(
    modules: &[ModuleInput<'_>],
    functions: &[CheckedFunction],
    names: &Names,
    types: TypeContext<'_>,
) -> SemanticIndex {
    let mut index = SemanticIndex::default();
    for module in modules {
        let program = module.program;
        for external in &program.externs {
            index.document(&external.name, external.doc.as_ref());
            index.symbol(
                &external.name,
                12,
                external.result.span,
                format!("extern def {}.{}", module.name, external.name.text),
            );
        }
        for record in &program.records {
            index.document(&record.name, record.doc.as_ref());
            index.symbol(
                &record.name,
                23,
                record
                    .fields
                    .last()
                    .map_or(record.name.span, |field| field.ty.span),
                format!("record {}.{}", module.name, record.name.text),
            );
            for field in &record.fields {
                type_entry(&mut index, &field.ty, module.name, names, &types);
            }
        }
        for union in &program.unions {
            index.document(&union.name, union.doc.as_ref());
            index.symbol(
                &union.name,
                10,
                union.cases.last().map_or(union.name.span, |case| {
                    case.payload.as_ref().map_or(case.name.span, |ty| ty.span)
                }),
                format!("union {}.{}", module.name, union.name.text),
            );
            for case in &union.cases {
                if let Some(ty) = &case.payload {
                    type_entry(&mut index, ty, module.name, names, &types);
                }
            }
        }
        for alias in &program.type_aliases {
            index.document(&alias.name, alias.doc.as_ref());
            index.symbol(
                &alias.name,
                26,
                alias.target.span,
                format!("type {}.{}", module.name, alias.name.text),
            );
            type_entry(&mut index, &alias.target, module.name, names, &types);
        }
        for constant in &program.constants {
            index.document(&constant.name, constant.doc.as_ref());
            let detail = resolve_type(&constant.ty, module.name, names)
                .map(|ty| {
                    format!(
                        "const {}.{}: {}",
                        module.name,
                        constant.name.text,
                        ty.display(&types)
                    )
                })
                .unwrap_or_else(|_| constant.name.text.clone());
            index.symbol(&constant.name, 14, constant.value.span, detail);
            type_entry(&mut index, &constant.ty, module.name, names, &types);
        }
        for declaration in &program.functions {
            index.document(&declaration.name, declaration.doc.as_ref());
            if let Some(function) = functions.iter().find(|function| {
                function.module == module.name && function.name == declaration.name.text
            }) {
                index.symbol(
                    &declaration.name,
                    12,
                    declaration.body.span,
                    format!(
                        "def {} :: {}",
                        function.qualified_name(),
                        function.signature.as_type().display(&types)
                    ),
                );
            }
            for parameter in &declaration.parameters {
                type_entry(&mut index, &parameter.ty, module.name, names, &types);
            }
            type_entry(&mut index, &declaration.result, module.name, names, &types);
        }
        for class in &program.classes {
            index.document(&class.name, class.doc.as_ref());
            index.symbol(
                &class.name,
                5,
                class
                    .methods
                    .last()
                    .map_or(class.name.span, |method| method.result.span),
                format!("class {}.{}", module.name, class.name.text),
            );
            for method in &class.methods {
                index.document(&method.name, method.doc.as_ref());
                index.symbol(
                    &method.name,
                    6,
                    method.result.span,
                    format!(
                        "def {}.{}.{}",
                        module.name, class.name.text, method.name.text
                    ),
                );
            }
        }
        for instance in &program.instances {
            index.symbol(
                &instance.class,
                5,
                instance
                    .methods
                    .last()
                    .map_or(instance.ty.span, |method| method.body.span),
                format!("instance {}", instance.class.text),
            );
        }
        for test in &program.tests {
            index.symbol(
                &Ident {
                    text: test.name.clone(),
                    span: test.name_span,
                    provenance: Provenance::User,
                },
                12,
                test.span,
                format!("test {:?}", test.name),
            );
        }
    }
    for function in functions
        .iter()
        .filter(|function| function.origin.module == ModuleOrigin::User)
    {
        let mut locals: BTreeMap<usize, &Local> = function
            .parameters
            .iter()
            .map(|local| (local.id, local))
            .collect();
        let mut expressions = Vec::new();
        let mut pending = vec![&function.body];
        while let Some(expression) = pending.pop() {
            match &expression.kind {
                TypedExprKind::Block { bindings, .. } => {
                    locals.extend(bindings.iter().map(|(local, _)| (local.id, local)))
                }
                TypedExprKind::Lambda { parameters, .. } => {
                    locals.extend(parameters.iter().map(|local| (local.id, local)))
                }
                TypedExprKind::ForRange { local, .. } | TypedExprKind::ForEach { local, .. } => {
                    locals.insert(local.id, local);
                }
                TypedExprKind::Match { arms, .. } => {
                    for arm in arms {
                        for alternative in &arm.alternatives {
                            locals.extend(
                                alternative
                                    .bindings
                                    .iter()
                                    .map(|(local, _)| (local.id, local)),
                            );
                        }
                    }
                }
                _ => {}
            }
            expressions.push(expression);
            pending.extend(expression.children());
        }
        for local in locals
            .values()
            .filter(|local| local.provenance == Provenance::User)
        {
            index.entries.push(SemanticEntry {
                span: local.span,
                detail: format!("{}: {}", local.name, local.ty.display(&types)),
                target: Some(local.span),
                priority: 0,
            });
        }
        for expression in expressions {
            let target = match &expression.kind {
                TypedExprKind::Local(id) => locals.get(id).map(|local| local.span),
                TypedExprKind::Function(FunctionRef::User(id))
                | TypedExprKind::GenericFunction(id, _) => Some(functions[*id].span),
                TypedExprKind::Record(_) => type_target(&expression.ty, &types),
                _ => None,
            };
            let detail = if let TypedExprKind::Local(id) = expression.kind {
                locals
                    .get(&id)
                    .map(|local| format!("{}: {}", local.name, expression.ty.display(&types)))
                    .unwrap_or_else(|| expression.ty.display(&types))
            } else {
                expression.ty.display(&types)
            };
            index.entries.push(SemanticEntry {
                span: expression.span,
                detail,
                target,
                priority: 1,
            });
        }
    }
    index.entries.sort_by_key(|entry| {
        (
            entry.span.source,
            entry.span.start,
            entry.span.end,
            entry.priority,
        )
    });
    index
        .symbols
        .sort_by_key(|symbol| (symbol.selection.source, symbol.selection.start));
    index
}

fn type_target(ty: &Type, types: &TypeContext<'_>) -> Option<Span> {
    match ty {
        Type::Record(id, _) => Some(types.records[*id].span),
        Type::Union(id, _) => Some(types.unions[*id].span),
        _ => None,
    }
}

fn type_entry(
    index: &mut SemanticIndex,
    expression: &TypeExpr,
    module: &str,
    names: &Names,
    types: &TypeContext<'_>,
) {
    if let Ok(ty) = resolve_type(expression, module, names) {
        index.entries.push(SemanticEntry {
            span: expression.span,
            detail: ty.display(types),
            target: type_target(&ty, types),
            priority: 1,
        });
    }
    match &expression.kind {
        TypeExprKind::Apply(_, arguments) => {
            for argument in arguments {
                type_entry(index, argument, module, names, types);
            }
        }
        TypeExprKind::Regions(inner, _)
        | TypeExprKind::Reference(inner, _)
        | TypeExprKind::Array(inner)
        | TypeExprKind::List(inner)
        | TypeExprKind::Task(inner) => type_entry(index, inner, module, names, types),
        TypeExprKind::Tuple(elements) => {
            for element in elements {
                type_entry(index, element, module, names, types);
            }
        }
        TypeExprKind::Function(parameters, result) => {
            for parameter in parameters {
                type_entry(index, parameter, module, names, types);
            }
            type_entry(index, result, module, names, types);
        }
        _ => {}
    }
}
