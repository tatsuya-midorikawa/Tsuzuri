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

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum SymbolKind {
    Module,
    Record,
    Union,
    Case,
    Field,
    Alias,
    Class,
    Method,
    Function,
    Extern,
    Const,
    Parameter,
    Local,
}

impl SymbolKind {
    pub fn describe(self) -> &'static str {
        match self {
            Self::Module => "module",
            Self::Record => "record",
            Self::Union => "union",
            Self::Case => "union case",
            Self::Field => "field",
            Self::Alias => "type alias",
            Self::Class => "class",
            Self::Method => "method",
            Self::Function => "function",
            Self::Extern => "extern function",
            Self::Const => "const",
            Self::Parameter => "parameter",
            Self::Local => "local",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Declaration,
    Write,
    Read,
}

#[derive(Clone, Debug)]
pub struct Definition {
    pub name: String,
    pub kind: SymbolKind,
    /// The name identifier only.
    pub span: Span,
    pub module: String,
    /// The record of a field or the union of a case.
    pub container: Option<usize>,
    pub detail: String,
    pub public: bool,
    pub exported: bool,
    /// `name: type` for each parameter of a function or extern.
    pub parameters: Vec<String>,
    pub result: String,
}

#[derive(Clone, Copy, Debug)]
pub struct Reference {
    pub span: Span,
    pub definition: usize,
    pub role: Role,
}

#[derive(Clone, Copy, Debug)]
pub struct LocalScope {
    pub definition: usize,
    pub visible: Span,
}

#[derive(Clone, Debug, Default)]
pub struct SemanticIndex {
    pub entries: Vec<SemanticEntry>,
    pub symbols: Vec<DocumentSymbol>,
    pub documentation: Vec<(Span, String)>,
    pub definitions: Vec<Definition>,
    /// Sorted by source and start.
    pub references: Vec<Reference>,
    pub scopes: Vec<LocalScope>,
    /// Record-typed expressions and the definition of their record.
    pub receivers: Vec<(Span, usize)>,
}

impl SemanticIndex {
    /// The definition or reference at `offset`, or else one that ends there.
    pub fn occurrence_at(&self, source: usize, offset: usize) -> Option<Reference> {
        let mut touching = None;
        for occurrence in self.all_occurrences() {
            if occurrence.span.source != Some(source) {
                continue;
            }
            if occurrence.span.start <= offset && offset < occurrence.span.end {
                return Some(occurrence);
            }
            if occurrence.span.end == offset && touching.is_none() {
                touching = Some(occurrence);
            }
        }
        touching
    }

    /// Every spelling of `definition`, ordered by source and start.
    pub fn occurrences(&self, definition: usize) -> Vec<Reference> {
        let mut found: Vec<_> = self
            .all_occurrences()
            .filter(|occurrence| occurrence.definition == definition)
            .collect();
        found.sort_by_key(|occurrence| (occurrence.span.source, occurrence.span.start));
        found.dedup_by_key(|occurrence| (occurrence.span.source, occurrence.span.start));
        found
    }

    /// Definitions as declarations, then references.
    pub fn all_occurrences(&self) -> impl Iterator<Item = Reference> + '_ {
        self.definitions
            .iter()
            .enumerate()
            .map(|(definition, item)| Reference {
                span: item.span,
                definition,
                role: Role::Declaration,
            })
            .chain(self.references.iter().copied())
    }

    /// Keeps only references whose source text spells their definition's name.
    pub(crate) fn retain_spelled(&mut self, texts: &[&str]) {
        let definitions = &self.definitions;
        self.references.retain(|reference| {
            reference
                .span
                .source
                .and_then(|source| texts.get(source))
                .and_then(|text| text.get(reference.span.start..reference.span.end))
                == Some(definitions[reference.definition].name.as_str())
        });
    }

    fn define(&mut self, definition: Definition) -> usize {
        self.definitions.push(definition);
        self.definitions.len() - 1
    }

    fn reference(&mut self, span: Span, definition: usize, role: Role) {
        self.references.push(Reference {
            span,
            definition,
            role,
        });
    }

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

/// Checker ids linked to the definitions of their declarations.
struct Links {
    records: Vec<Option<usize>>,
    unions: Vec<Option<usize>>,
    /// Extern types by qualified name.
    handles: BTreeMap<String, usize>,
    /// Functions, externs, and constants by qualified name.
    callables: BTreeMap<String, usize>,
}

impl Links {
    fn type_reference(
        &self,
        index: &mut SemanticIndex,
        text: &str,
        span: Span,
        ty: &Type,
        types: &TypeContext<'_>,
    ) {
        let (definition, name) = match ty {
            Type::Record(id, _) => (self.records[*id], types.records[*id].name.as_str()),
            Type::Union(id, _) => (self.unions[*id], types.unions[*id].name.as_str()),
            Type::Handle(name) => (self.handles.get(name.as_ref()).copied(), name.as_ref()),
            _ => return,
        };
        let last = text.rsplit(['.', ':']).next().unwrap_or(text);
        // An alias spells another name; aliases have no references yet.
        if let Some(definition) = definition
            && name.rsplit('.').next() == Some(last)
        {
            let start = span.end.saturating_sub(last.len());
            index.reference(Span { start, ..span }, definition, Role::Read);
        }
    }
}

fn plain(
    name: &Ident,
    kind: SymbolKind,
    module: &str,
    detail: String,
    visibility: Visibility,
) -> Definition {
    Definition {
        name: name.text.clone(),
        kind,
        span: name.span,
        module: module.to_owned(),
        container: None,
        detail,
        public: visibility == Visibility::Public,
        exported: false,
        parameters: Vec::new(),
        result: String::new(),
    }
}

/// A type declared in the module `key` whose full name is `shown`, as source
/// code writes it: a type named after its module has the module's name.
fn type_name(shown: &str, key: &str, declared: &str) -> String {
    if module_stem(key) == declared {
        shown.to_owned()
    } else {
        format!("{shown}.{declared}")
    }
}

/// Defines every source declaration; fields and cases follow their container.
fn define_declarations(
    index: &mut SemanticIndex,
    modules: &[ModuleInput<'_>],
    functions: &[CheckedFunction],
    names: &Names,
    types: &TypeContext<'_>,
) -> Links {
    let record_ids: BTreeMap<&str, usize> = types
        .records
        .iter()
        .enumerate()
        .map(|(id, record)| (record.name.as_str(), id))
        .collect();
    let union_ids: BTreeMap<&str, usize> = types
        .unions
        .iter()
        .enumerate()
        .map(|(id, union)| (union.name.as_str(), id))
        .collect();
    let checked: BTreeMap<String, &CheckedFunction> = functions
        .iter()
        .map(|function| (function.qualified_name(), function))
        .collect();
    let mut links = Links {
        records: vec![None; types.records.len()],
        unions: vec![None; types.unions.len()],
        handles: BTreeMap::new(),
        callables: BTreeMap::new(),
    };
    for module in modules {
        let program = module.program;
        let name = module.name;
        let shown = names.module_display(name);
        let typed = |declared: &str| type_name(&shown, name, declared);
        for record in &program.records {
            if record.name.provenance == Provenance::Generated {
                continue;
            }
            let qualified = format!("{name}.{}", record.name.text);
            let id = record_ids.get(qualified.as_str()).copied();
            let container = index.define(plain(
                &record.name,
                SymbolKind::Record,
                name,
                format!("record {}", typed(&record.name.text)),
                record.visibility,
            ));
            if let Some(id) = id {
                links.records[id] = Some(container);
            }
            for (position, field) in record.fields.iter().enumerate() {
                let detail = id
                    .and_then(|id| types.records[id].fields.get(position))
                    .map_or_else(
                        || field.name.text.clone(),
                        |(field, ty)| format!("{field}: {}", ty.display(types)),
                    );
                index.define(Definition {
                    container: Some(container),
                    ..plain(
                        &field.name,
                        SymbolKind::Field,
                        name,
                        detail,
                        record.visibility,
                    )
                });
            }
        }
        for union in &program.unions {
            if union.name.provenance == Provenance::Generated {
                continue;
            }
            let qualified = format!("{name}.{}", union.name.text);
            let id = union_ids.get(qualified.as_str()).copied();
            let container = index.define(plain(
                &union.name,
                SymbolKind::Union,
                name,
                format!("union {}", typed(&union.name.text)),
                union.visibility,
            ));
            if let Some(id) = id {
                links.unions[id] = Some(container);
            }
            for (position, case) in union.cases.iter().enumerate() {
                let payload = id
                    .and_then(|id| types.unions[id].cases.get(position))
                    .and_then(|(_, payload)| payload.as_ref());
                let case_name = format!("{}.{}", typed(&union.name.text), case.name.text);
                let detail = match payload {
                    Some(ty) => format!("{case_name} of {}", ty.display(types)),
                    None => case_name,
                };
                index.define(Definition {
                    container: Some(container),
                    ..plain(&case.name, SymbolKind::Case, name, detail, union.visibility)
                });
            }
        }
        for alias in &program.type_aliases {
            index.define(plain(
                &alias.name,
                SymbolKind::Alias,
                name,
                format!("type {}", typed(&alias.name.text)),
                alias.visibility,
            ));
        }
        for handle in &program.extern_types {
            let definition = index.define(plain(
                &handle.name,
                SymbolKind::Record,
                name,
                format!("extern type {}", typed(&handle.name.text)),
                handle.visibility,
            ));
            links
                .handles
                .insert(format!("{name}.{}", handle.name.text), definition);
        }
        for constant in &program.constants {
            let detail = resolve_type(&constant.ty, name, names)
                .map(|ty| {
                    format!(
                        "const {shown}.{}: {}",
                        constant.name.text,
                        ty.display(types)
                    )
                })
                .unwrap_or_else(|_| constant.name.text.clone());
            let definition = index.define(plain(
                &constant.name,
                SymbolKind::Const,
                name,
                detail,
                constant.visibility,
            ));
            links
                .callables
                .insert(format!("{name}.{}", constant.name.text), definition);
        }
        let callables = program
            .functions
            .iter()
            .map(|declaration| {
                let labels = declaration
                    .parameters
                    .iter()
                    .map(|parameter| &parameter.name);
                (
                    &declaration.name,
                    labels.collect::<Vec<_>>(),
                    SymbolKind::Function,
                    declaration.visibility,
                    declaration.exported,
                )
            })
            .chain(program.externs.iter().map(|external| {
                (
                    &external.name,
                    Vec::new(),
                    SymbolKind::Extern,
                    external.visibility,
                    external.exported,
                )
            }));
        for (declared, labels, kind, visibility, exported) in callables {
            let qualified = format!("{name}.{}", declared.text);
            let Some(function) = checked.get(&qualified) else {
                continue;
            };
            if declared.provenance == Provenance::Generated {
                continue;
            }
            let parameters = function
                .signature
                .parameters
                .iter()
                .enumerate()
                .map(|(position, ty)| {
                    let label = labels
                        .get(position)
                        .filter(|label| {
                            label.provenance == Provenance::User && !label.text.starts_with('$')
                        })
                        .map_or_else(
                            || format!("arg{}", position + 1),
                            |label| label.text.clone(),
                        );
                    format!("{label}: {}", ty.display(types))
                })
                .collect();
            let detail = if kind == SymbolKind::Extern {
                format!("extern def {shown}.{}", declared.text)
            } else {
                format!(
                    "def {shown}.{} :: {}",
                    declared.text,
                    function.signature.as_type().display(types)
                )
            };
            let definition = index.define(Definition {
                exported,
                parameters,
                result: function.signature.result.display(types),
                ..plain(declared, kind, name, detail, visibility)
            });
            links.callables.insert(qualified, definition);
        }
        for class in &program.classes {
            let container = index.define(plain(
                &class.name,
                SymbolKind::Class,
                name,
                format!("class {shown}.{}", class.name.text),
                Visibility::Public,
            ));
            for method in &class.methods {
                index.define(Definition {
                    container: Some(container),
                    ..plain(
                        &method.name,
                        SymbolKind::Method,
                        name,
                        format!("def {shown}.{}.{}", class.name.text, method.name.text),
                        method.visibility,
                    )
                });
            }
        }
    }
    links
}

pub(super) fn collect(
    modules: &[ModuleInput<'_>],
    functions: &[CheckedFunction],
    name_uses: &[Vec<(Span, NameTarget)>],
    names: &Names,
    types: TypeContext<'_>,
) -> SemanticIndex {
    let mut index = SemanticIndex::default();
    let links = define_declarations(&mut index, modules, functions, names, &types);
    let links = &links;
    for module in modules {
        let program = module.program;
        let shown = names.module_display(module.name);
        let typed = |declared: &str| type_name(&shown, module.name, declared);
        for external in &program.externs {
            index.document(&external.name, external.doc.as_ref());
            index.symbol(
                &external.name,
                12,
                external.result.span,
                format!("extern def {shown}.{}", external.name.text),
            );
            for ty in external.parameters.iter().chain([&external.result]) {
                type_entry(&mut index, ty, module.name, names, &types, links, true);
            }
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
                format!("record {}", typed(&record.name.text)),
            );
            for field in &record.fields {
                type_entry(
                    &mut index,
                    &field.ty,
                    module.name,
                    names,
                    &types,
                    links,
                    true,
                );
            }
        }
        for union in &program.unions {
            if union.name.provenance == Provenance::Generated {
                continue;
            }
            index.document(&union.name, union.doc.as_ref());
            index.symbol(
                &union.name,
                10,
                union.cases.last().map_or(union.name.span, |case| {
                    case.payload.as_ref().map_or(case.name.span, |ty| ty.span)
                }),
                format!("union {}", typed(&union.name.text)),
            );
            for case in &union.cases {
                if let Some(ty) = &case.payload {
                    type_entry(&mut index, ty, module.name, names, &types, links, true);
                }
            }
        }
        for handle in &program.extern_types {
            index.document(&handle.name, handle.doc.as_ref());
            index.symbol(
                &handle.name,
                23,
                handle.name.span,
                format!("extern type {}", typed(&handle.name.text)),
            );
        }
        for alias in &program.type_aliases {
            index.document(&alias.name, alias.doc.as_ref());
            index.symbol(
                &alias.name,
                26,
                alias.target.span,
                format!("type {}", typed(&alias.name.text)),
            );
            type_entry(
                &mut index,
                &alias.target,
                module.name,
                names,
                &types,
                links,
                true,
            );
        }
        for constant in &program.constants {
            index.document(&constant.name, constant.doc.as_ref());
            let detail = resolve_type(&constant.ty, module.name, names)
                .map(|ty| {
                    format!(
                        "const {shown}.{}: {}",
                        constant.name.text,
                        ty.display(&types)
                    )
                })
                .unwrap_or_else(|_| constant.name.text.clone());
            index.symbol(&constant.name, 14, constant.value.span, detail);
            type_entry(
                &mut index,
                &constant.ty,
                module.name,
                names,
                &types,
                links,
                true,
            );
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
                        "def {shown}.{} :: {}",
                        function.name,
                        function.signature.as_type().display(&types)
                    ),
                );
            }
            for parameter in &declaration.parameters {
                type_entry(
                    &mut index,
                    &parameter.ty,
                    module.name,
                    names,
                    &types,
                    links,
                    true,
                );
            }
            type_entry(
                &mut index,
                &declaration.result,
                module.name,
                names,
                &types,
                links,
                true,
            );
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
                format!("class {shown}.{}", class.name.text),
            );
            for method in &class.methods {
                index.document(&method.name, method.doc.as_ref());
                for ty in method.parameters.iter().chain([&method.result]) {
                    type_entry(&mut index, ty, module.name, names, &types, links, true);
                }
                index.symbol(
                    &method.name,
                    6,
                    method.result.span,
                    format!("def {shown}.{}.{}", class.name.text, method.name.text),
                );
            }
        }
        for instance in &program.instances {
            type_entry(
                &mut index,
                &instance.ty,
                module.name,
                names,
                &types,
                links,
                true,
            );
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
    for module in modules
        .iter()
        .filter(|module| module.origin == ModuleOrigin::User)
    {
        let program = module.program;
        let bodies = program
            .functions
            .iter()
            .map(|function| &function.body)
            .chain(program.constants.iter().map(|constant| &constant.value))
            .chain(program.tests.iter().map(|test| &test.body))
            .chain(program.entry.as_ref());
        for body in bodies {
            body.visit(&mut |expression| {
                let annotations: Vec<&TypeExpr> = match &expression.kind {
                    ExprKind::Block { bindings, .. } => bindings
                        .iter()
                        .filter_map(|binding| binding.annotation.as_ref())
                        .collect(),
                    ExprKind::Cast(_, ty) => vec![ty],
                    ExprKind::NewArray(ty, ..) | ExprKind::NewList(ty, ..) => vec![&**ty],
                    _ => Vec::new(),
                };
                for ty in annotations {
                    type_entry(&mut index, ty, module.name, names, &types, links, false);
                }
            });
        }
    }
    for (position, function) in functions
        .iter()
        .enumerate()
        .filter(|(_, function)| function.origin.module == ModuleOrigin::User)
    {
        let uses = name_uses.get(position).map_or(&[][..], Vec::as_slice);
        index_function(&mut index, function, uses, functions, links, &types);
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
    let key = |reference: &Reference| {
        (
            reference.span.source,
            reference.span.start,
            reference.span.end,
        )
    };
    index.references.sort_by_key(key);
    index.references.dedup_by_key(|reference| key(reference));
    index
}

/// Hover entries, local definitions and scopes, and references of one body.
fn index_function(
    index: &mut SemanticIndex,
    function: &CheckedFunction,
    uses: &[(Span, NameTarget)],
    functions: &[CheckedFunction],
    links: &Links,
    types: &TypeContext<'_>,
) {
    let mut locals: BTreeMap<usize, &Local> = function
        .parameters
        .iter()
        .map(|local| (local.id, local))
        .collect();
    let mut parameters: BTreeSet<usize> = locals.keys().copied().collect();
    let body_end = function.body.span.end;
    let mut scopes: Vec<(usize, usize, usize)> = function
        .parameters
        .iter()
        .map(|local| (local.id, local.span.end, body_end))
        .collect();
    let mut writes = BTreeSet::new();
    let mut expressions = Vec::new();
    let mut pending = vec![&function.body];
    while let Some(expression) = pending.pop() {
        let end = expression.span.end;
        match &expression.kind {
            TypedExprKind::Block { bindings, .. } => {
                for (local, value) in bindings {
                    locals.insert(local.id, local);
                    scopes.push((local.id, value.span.end, end));
                }
            }
            TypedExprKind::Lambda {
                parameters: list, ..
            } => {
                for local in list {
                    locals.insert(local.id, local);
                    parameters.insert(local.id);
                    scopes.push((local.id, local.span.end, end));
                }
            }
            TypedExprKind::ForRange { local, .. } | TypedExprKind::ForEach { local, .. } => {
                locals.insert(local.id, local);
                scopes.push((local.id, local.span.end, end));
            }
            TypedExprKind::Match { arms, .. } => {
                for arm in arms {
                    for alternative in &arm.alternatives {
                        for (local, _) in &alternative.bindings {
                            locals.insert(local.id, local);
                            scopes.push((local.id, local.span.end, arm.body.span.end));
                        }
                    }
                }
            }
            TypedExprKind::Assign(target, _) if matches!(target.kind, TypedExprKind::Local(_)) => {
                writes.insert((target.span.start, target.span.end));
            }
            _ => {}
        }
        expressions.push(expression);
        pending.extend(expression.children());
    }
    let mut definitions = BTreeMap::new();
    for local in locals
        .values()
        .filter(|local| local.provenance == Provenance::User)
    {
        let detail = format!("{}: {}", local.name, local.ty.display(types));
        index.entries.push(SemanticEntry {
            span: local.span,
            detail: detail.clone(),
            target: Some(local.span),
            priority: 0,
        });
        let kind = if parameters.contains(&local.id) {
            SymbolKind::Parameter
        } else {
            SymbolKind::Local
        };
        let definition = index.define(Definition {
            name: local.name.clone(),
            kind,
            span: local.span,
            module: function.module.clone(),
            container: None,
            detail,
            public: false,
            exported: false,
            parameters: Vec::new(),
            result: String::new(),
        });
        definitions.insert(local.id, definition);
    }
    let mut scoped = BTreeSet::new();
    for (id, start, end) in scopes {
        if let Some(&definition) = definitions.get(&id)
            && start <= end
            && scoped.insert(id)
        {
            let source = index.definitions[definition].span.source;
            index.scopes.push(LocalScope {
                definition,
                visible: Span { start, end, source },
            });
        }
    }
    for expression in expressions {
        let target = match &expression.kind {
            TypedExprKind::Local(id) => locals.get(id).map(|local| local.span),
            TypedExprKind::Function(FunctionRef::User(id))
            | TypedExprKind::GenericFunction(id, _) => Some(functions[*id].span),
            TypedExprKind::Record(_) => type_target(&expression.ty, types),
            _ => None,
        };
        let detail = if let TypedExprKind::Local(id) = expression.kind {
            locals
                .get(&id)
                .map(|local| format!("{}: {}", local.name, expression.ty.display(types)))
                .unwrap_or_else(|| expression.ty.display(types))
        } else {
            expression.ty.display(types)
        };
        index.entries.push(SemanticEntry {
            span: expression.span,
            detail,
            target,
            priority: 1,
        });
        let callee = match &expression.kind {
            TypedExprKind::Function(FunctionRef::User(id))
            | TypedExprKind::GenericFunction(id, _) => Some(*id),
            // A constant is a call whose callee has no span.
            TypedExprKind::Call(callee, arguments)
                if arguments.is_empty() && callee.span.source.is_none() =>
            {
                match callee.kind {
                    TypedExprKind::Function(FunctionRef::User(id)) => Some(id),
                    _ => None,
                }
            }
            _ => None,
        };
        if let TypedExprKind::Local(id) = &expression.kind
            && let Some(&definition) = definitions.get(id)
        {
            let span = (expression.span.start, expression.span.end);
            let role = if writes.contains(&span) {
                Role::Write
            } else {
                Role::Read
            };
            index.reference(expression.span, definition, role);
        }
        if let Some(id) = callee
            && expression.span.source.is_some()
            && let Some(&definition) = links.callables.get(&functions[id].qualified_name())
        {
            // A qualified spelling ends with the function name.
            let start = expression.span.end.saturating_sub(functions[id].name.len());
            if start >= expression.span.start {
                index.reference(
                    Span {
                        start,
                        ..expression.span
                    },
                    definition,
                    Role::Read,
                );
            }
        }
        let mut ty = &expression.ty;
        while let Type::Reference(inner, _) = ty {
            ty = inner;
        }
        if let Type::Record(id, _) = ty
            && let Some(definition) = links.records[*id]
        {
            index.receivers.push((expression.span, definition));
        }
    }
    for &(span, target) in uses {
        let (definition, role) = match target {
            NameTarget::Record(id) => (links.records[id], Role::Read),
            NameTarget::Union(id) => (links.unions[id], Role::Read),
            NameTarget::Field(id, field) => (
                links.records[id].map(|record| record + 1 + field),
                Role::Read,
            ),
            NameTarget::Case(id, case) => {
                (links.unions[id].map(|union| union + 1 + case), Role::Read)
            }
            NameTarget::Local(id) => (definitions.get(&id).copied(), Role::Declaration),
        };
        if let Some(definition) = definition {
            index.reference(span, definition, role);
        }
    }
}

fn type_target(ty: &Type, types: &TypeContext<'_>) -> Option<Span> {
    match ty {
        Type::Record(id, _) => Some(types.records[*id].span),
        Type::Union(id, _) => Some(types.unions[*id].span),
        _ => None,
    }
}

/// Adds the hover entry (when `hover`) and the record or union reference of
/// a type expression and its parts.
fn type_entry(
    index: &mut SemanticIndex,
    expression: &TypeExpr,
    module: &str,
    names: &Names,
    types: &TypeContext<'_>,
    links: &Links,
    hover: bool,
) {
    if let Ok(ty) = resolve_type(expression, module, names) {
        if hover {
            index.entries.push(SemanticEntry {
                span: expression.span,
                detail: ty.display(types),
                target: type_target(&ty, types),
                priority: 1,
            });
        }
        let head = match &expression.kind {
            TypeExprKind::Named(text) => Some((text.as_str(), expression.span)),
            TypeExprKind::Apply(head, _) => Some((head.text.as_str(), head.span)),
            _ => None,
        };
        if let Some((text, span)) = head {
            links.type_reference(index, text, span, &ty, types);
        }
    }
    let mut recurse =
        |inner: &TypeExpr| type_entry(index, inner, module, names, types, links, hover);
    match &expression.kind {
        TypeExprKind::Apply(_, arguments) => arguments.iter().for_each(&mut recurse),
        TypeExprKind::Regions(inner, _)
        | TypeExprKind::Quantified(_, inner)
        | TypeExprKind::Reference(inner, _)
        | TypeExprKind::Array(inner)
        | TypeExprKind::ArrayView(inner)
        | TypeExprKind::List(inner)
        | TypeExprKind::Task(inner) => recurse(inner),
        TypeExprKind::FixedArray(element, _) => recurse(element),
        TypeExprKind::Tuple(elements) => elements.iter().for_each(&mut recurse),
        TypeExprKind::Function(parameters, result) => {
            parameters.iter().for_each(&mut recurse);
            recurse(result);
        }
        _ => {}
    }
}
