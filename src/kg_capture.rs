//! G26 (D-60): the declaration view of the knowledge graph. `check_modules_collect` calls
//! `collect_abstract` after the ownership inference and before specialization, while the module
//! still holds the abstract body of every generic function. It only reads the checker's state:
//! the nodes (declarations), the edges that the abstract bodies and the semantic index show, the
//! tests, and the recursion and loops of each function. `crate::kg` joins this with the facts of
//! the final, specialized module.

use super::polymorph::Classes;
use super::*;
use crate::docgen;
use crate::kg::algo;
use crate::kg::ids;

/// The nodes that one traversal of a function body may visit, and those of every body of one
/// view (G26 rule 8). A body over the limit keeps what its traversal found and is marked.
pub(crate) const FUNCTION_NODES: usize = 1 << 20;
pub(crate) const VIEW_NODES: usize = 1 << 23;

/// How deep `tested_by` follows calls from a test, and how many functions one test lists.
pub(crate) const TESTED_BY_DEPTH: usize = 8;
pub(crate) const TESTED_BY_LIMIT: usize = 200;

#[derive(Default)]
pub(crate) struct Capture {
    /// A copy of the semantic index that the checker collected for the capture.
    pub(crate) semantic: Option<semantic::SemanticIndex>,
    /// Capture std modules too (the std KG, G27 Phase 4); their origin is `std`.
    pub(crate) include_std: bool,
    /// The modules of the graph, in input order.
    pub(crate) modules: Vec<CapturedModule>,
    /// The ID of every module by key, std modules included (an std module's ID is its key).
    pub(crate) module_ids: BTreeMap<String, String>,
    pub(crate) nodes: Vec<CapturedNode>,
    pub(crate) edges: Vec<CapturedEdge>,
    pub(crate) tests: Vec<CapturedTest>,
    /// (function module, base name) of an abstract function -> the ID of its node.
    pub(crate) function_ids: BTreeMap<(String, String), String>,
    /// Facts of the declaration view, by node ID.
    pub(crate) recursion: BTreeMap<String, &'static str>,
    pub(crate) loops: BTreeMap<String, Vec<(&'static str, Span)>>,
    pub(crate) events: BTreeMap<String, Vec<CapturedEvent>>,
    /// The nodes whose declaration-view traversal stopped at the node limit.
    pub(crate) truncated: BTreeSet<String>,
}

pub(crate) struct CapturedModule {
    pub(crate) key: String,
    pub(crate) id: String,
    pub(crate) origin: String,
    /// The index of the module's source in the analysis inputs.
    pub(crate) source: usize,
}

#[derive(Clone, Debug)]
pub(crate) struct CapturedNode {
    pub(crate) id: String,
    pub(crate) kind: &'static str,
    pub(crate) module: String,
    pub(crate) name: String,
    pub(crate) signature: String,
    pub(crate) params: Vec<CapturedParam>,
    pub(crate) result: Option<String>,
    pub(crate) public: bool,
    pub(crate) export: bool,
    pub(crate) pure: bool,
    pub(crate) doc: Option<String>,
    pub(crate) deprecated: Option<String>,
    pub(crate) origin: String,
    pub(crate) span: Span,
    /// The declaration from the start of its name to the end of its body (appendix C1).
    pub(crate) extent: Span,
    /// The node that `contains` this one.
    pub(crate) parent: Option<usize>,
    /// The abstract function of a function-like node.
    pub(crate) function: Option<usize>,
}

#[derive(Clone, Debug)]
pub(crate) struct CapturedParam {
    pub(crate) name: String,
    pub(crate) ty: String,
    pub(crate) conv: &'static str,
}

#[derive(Clone, Debug)]
pub(crate) struct CapturedEdge {
    pub(crate) rel: &'static str,
    pub(crate) from: String,
    pub(crate) to: String,
    pub(crate) args: Option<Vec<&'static str>>,
    pub(crate) span: Span,
}

pub(crate) struct CapturedTest {
    pub(crate) id: String,
    pub(crate) kind: &'static str,
    pub(crate) module: String,
    pub(crate) name: String,
    pub(crate) index: usize,
    pub(crate) span: Span,
}

#[derive(Clone, Debug)]
pub(crate) struct CapturedEvent {
    pub(crate) var: String,
    pub(crate) event: &'static str,
    pub(crate) to: Option<String>,
    pub(crate) span: Span,
}

/// What the checker holds at the hook, before `polymorph::specialize` consumes the module.
pub(super) struct Abstract<'a> {
    pub(super) modules: &'a [ModuleInput<'a>],
    pub(super) module: &'a CheckedModule,
    pub(super) names: &'a Names,
    pub(super) classes: &'a Classes,
    pub(super) references: &'a [Vec<usize>],
}

/// The convention of a parameter of the resolved type `ty` (appendix B4).
pub(crate) fn conv(ty: &Type, types: &TypeContext<'_>) -> &'static str {
    match ty {
        Type::Reference(_, false) => "ref",
        Type::Reference(_, true) => "ref_mut",
        _ if ty.is_copy(types) => "copy",
        _ => "own",
    }
}

/// The convention of an argument expression (appendix D2).
pub(crate) fn argument_conv(argument: &TypedExpr, types: &TypeContext<'_>) -> &'static str {
    match &argument.kind {
        TypedExprKind::Borrow(_, false) | TypedExprKind::BorrowOperand(_) => "ref",
        TypedExprKind::Borrow(_, true) => "ref_mut",
        _ if argument.ty.is_copy(types) => "copy",
        _ => "own",
    }
}

/// The written type of a parameter without its ownership mode: `own T` and `mut T` are passing
/// conventions, which `conv` shows.
fn parameter_type_text(ty: &TypeExpr) -> String {
    match &ty.kind {
        TypeExprKind::Own(inner) | TypeExprKind::Mut(inner) => docgen::type_text(inner),
        _ => docgen::type_text(ty),
    }
}

fn joined(start: Span, end: usize) -> Span {
    Span {
        start: start.start,
        end: end.max(start.end),
        source: start.source,
    }
}

fn position(span: Span) -> (usize, usize) {
    (span.source.unwrap_or(usize::MAX), span.start)
}

/// The node text of a std function: its module key and name without the specialization suffix.
fn std_function_id(function: &CheckedFunction) -> Option<String> {
    let base = function
        .name
        .split(".$mono.")
        .next()
        .unwrap_or(&function.name);
    (!base.starts_with('$') && !function.module.starts_with('$'))
        .then(|| ids::item_id(&function.module, base))
}

struct Builder<'a, 'b> {
    input: &'b Abstract<'a>,
    capture: &'b mut Capture,
    types: TypeContext<'a>,
}

impl Builder<'_, '_> {
    fn module_id(&self, key: &str) -> String {
        self.capture
            .module_ids
            .get(key)
            .cloned()
            .unwrap_or_else(|| key.replace('.', "::"))
    }

    /// The ID of a class: a built-in class by its name, a user class by its declaration.
    fn class_id(&self, class: usize) -> String {
        let name = self.input.classes.declared_name(class);
        if self.input.classes.class_is_builtin(class) {
            return name.to_owned();
        }
        match name.rsplit_once('.') {
            Some((key, declared)) => ids::item_id(&self.module_id(key), declared),
            None => name.to_owned(),
        }
    }

    fn push(&mut self, node: CapturedNode) -> usize {
        self.capture.nodes.push(node);
        self.capture.nodes.len() - 1
    }

    #[allow(clippy::too_many_arguments)]
    fn node(
        &self,
        id: String,
        kind: &'static str,
        module: &CapturedModule,
        name: String,
        signature: String,
        span: Span,
        extent: Span,
        parent: usize,
    ) -> CapturedNode {
        CapturedNode {
            id,
            kind,
            module: module.id.clone(),
            name,
            signature,
            params: Vec::new(),
            result: None,
            public: true,
            export: false,
            pure: false,
            doc: None,
            deprecated: None,
            origin: module.origin.clone(),
            span,
            extent,
            parent: Some(parent),
            function: None,
        }
    }
}

fn documented(node: &mut CapturedNode, doc: Option<&Documentation>) {
    if let Some(doc) = doc {
        node.doc = Some(doc.text.clone());
        node.deprecated = doc.deprecation().map(str::to_owned);
    }
}

pub(super) fn collect_abstract(capture: &mut Capture, input: &Abstract<'_>) {
    let types = input.module.types();
    let mut builder = Builder {
        input,
        capture,
        types,
    };
    for module in input.modules {
        let id = if module.origin == ModuleOrigin::Std {
            module.name.to_owned()
        } else {
            input.names.module_display(module.name)
        };
        builder
            .capture
            .module_ids
            .insert(module.name.to_owned(), id);
    }
    let mut owners = Vec::new();
    for (source, module) in input.modules.iter().enumerate() {
        if module.origin == ModuleOrigin::Std && !builder.capture.include_std {
            continue;
        }
        let origin = match (module.origin, module.package) {
            (ModuleOrigin::Std, _) => "std".to_owned(),
            (ModuleOrigin::User, None) => "project".to_owned(),
            (ModuleOrigin::User, Some(package)) => format!("dependency:{}", package.name),
        };
        let captured = CapturedModule {
            key: module.name.to_owned(),
            id: builder.module_id(module.name),
            origin,
            source,
        };
        declare_module(&mut builder, module, &captured, &mut owners);
        builder.capture.modules.push(captured);
    }
    // Equal IDs (tests and benches with one name) get suffixes in source order; a child follows
    // the final ID of its parent.
    let raw: Vec<_> = builder
        .capture
        .nodes
        .iter()
        .map(|node| (node.id.clone(), position(node.span)))
        .collect();
    let unique = ids::disambiguate(&raw);
    for (index, mut id) in unique.into_iter().enumerate() {
        if let Some(parent) = builder.capture.nodes[index].parent {
            let (old, new) = (&raw[parent].0, &builder.capture.nodes[parent].id);
            if old != new
                && let Some(rest) = id.strip_prefix(old.as_str())
            {
                id = format!("{new}{rest}");
            }
        }
        builder.capture.nodes[index].id = id;
    }
    let functions = &input.module.functions;
    for node in &builder.capture.nodes {
        if let Some(function) = node.function {
            let function = &functions[function];
            builder.capture.function_ids.insert(
                (function.module.clone(), function.name.clone()),
                node.id.clone(),
            );
        }
    }
    for (index, owner) in owners {
        let id = builder.capture.nodes[index].id.clone();
        if let Some(test) = builder.capture.tests.get_mut(owner) {
            test.id = id;
        }
    }
    contains_edges(&mut builder);
    let mut budget = VIEW_NODES;
    function_edges(&mut builder, &mut budget);
    declared_edges(&mut builder);
    uses_type_edges(&mut builder);
    tested_by_edges(&mut builder);
    recursion(&mut builder);
    ownership_events(&mut builder);
}

/// The ownership events of the function-like nodes (G26 phase 5), from their abstract bodies:
/// a generic function is checked once, and a type variable that it needs as Copy is copied.
fn ownership_events(builder: &mut Builder<'_, '_>) {
    let owners: BTreeMap<usize, String> = builder
        .capture
        .nodes
        .iter()
        .filter(|node| matches!(node.kind, "def" | "extern" | "test" | "bench" | "method"))
        .filter_map(|node| Some((node.function?, node.id.clone())))
        .collect();
    let functions: Vec<usize> = owners.keys().copied().collect();
    let events = crate::ownership::events_abstract(builder.input.module, &functions);
    let mut captured = BTreeMap::new();
    for (function, events) in events {
        let rows = events
            .into_iter()
            .map(|event| CapturedEvent {
                var: event.var,
                event: event.kind.name(),
                to: event.to.and_then(|target| match target {
                    crate::ownership::EventTarget::User(id) => callee_id(builder, id),
                    crate::ownership::EventTarget::Builtin(builtin) => {
                        Some(builtin.name().to_owned())
                    }
                    crate::ownership::EventTarget::Method(class, method) => Some(ids::item_id(
                        &builder.class_id(class),
                        builder.input.classes.method_name(class, method),
                    )),
                }),
                span: event.span,
            })
            .collect();
        captured.insert(owners[&function].clone(), rows);
    }
    builder.capture.events = captured;
}

/// The nodes of one module (appendix B3), in the order of the program's fields. `owners` gets
/// (node, test row) for each test and bench, whose ID may change when it is disambiguated.
fn declare_module(
    builder: &mut Builder<'_, '_>,
    input: &ModuleInput<'_>,
    module: &CapturedModule,
    owners: &mut Vec<(usize, usize)>,
) {
    let Program {
        source_kind: _,
        namespace: _,
        usings: _,
        type_aliases,
        constants,
        externs,
        extern_types,
        records,
        unions,
        functions,
        classes,
        instances,
        active_patterns,
        tests,
        benches,
        entry: _,
        dyn_types: _,
        cpu_attributes: _,
        effect_attributes: _,
        trusted: _,
        noncanonical: _,
        aliases: _,
        declaration_starts: _,
    } = input.program;
    let key = input.name;
    let names = builder.input.names;
    let checked = &builder.input.module.functions;
    let types = builder.types;
    let start = Span {
        start: 0,
        end: 0,
        source: Some(module.source),
    };
    let root = builder.push(CapturedNode {
        id: module.id.clone(),
        kind: "module",
        module: module.id.clone(),
        name: module
            .id
            .rsplit("::")
            .next()
            .unwrap_or(&module.id)
            .to_owned(),
        signature: format!("module {}", module.id),
        params: Vec::new(),
        result: None,
        public: true,
        export: false,
        pure: false,
        doc: None,
        deprecated: None,
        origin: module.origin.clone(),
        span: start,
        extent: Span {
            end: usize::MAX,
            ..start
        },
        parent: None,
        function: None,
    });
    let item = |name: &str| ids::item_id(&module.id, name);
    for declaration in functions {
        let pattern = active_patterns
            .iter()
            .find(|pattern| pattern.function == declaration.name.text);
        if declaration.name.text.starts_with('$') && pattern.is_none() {
            continue;
        }
        let prefix = format!(
            "{}{}def{}",
            if declaration.exported { "export " } else { "" },
            if declaration.constant {
                "const "
            } else if declaration.pure {
                "pure "
            } else {
                ""
            },
            if declaration.recursion.is_some() {
                " rec"
            } else {
                ""
            }
        );
        let name = match pattern {
            Some(pattern) => {
                let mut cases: Vec<_> = pattern
                    .cases
                    .iter()
                    .map(|case| case.text.as_str())
                    .collect();
                if pattern.partial {
                    cases.push("_");
                }
                format!("(|{}|)", cases.join("|"))
            }
            None => declaration.name.text.clone(),
        };
        let implicit_result = pattern
            .filter(|pattern| pattern.cases.len() > 1)
            .map(|_| TypeExpr {
                kind: TypeExprKind::Variable("T".into()),
                span: declaration.result.span,
            });
        let result = implicit_result.as_ref().unwrap_or(&declaration.result);
        let signature = docgen::signature(
            &prefix,
            &name,
            &declaration.regions,
            &declaration.constraints,
            declaration.parameters.iter().map(|parameter| &parameter.ty),
            result,
        );
        let function = names.local_function(key, &declaration.name.text);
        let resolved = function.map(|id| &checked[id].signature.parameters);
        let params = declaration
            .parameters
            .iter()
            .enumerate()
            .map(|(index, parameter)| CapturedParam {
                name: if parameter.name.text.starts_with('$')
                    || parameter.name.provenance == Provenance::Generated
                {
                    format!("arg{}", index + 1)
                } else {
                    parameter.name.text.clone()
                },
                ty: parameter_type_text(&parameter.ty),
                conv: resolved
                    .and_then(|parameters| parameters.get(index))
                    .map_or("own", |ty| conv(ty, &types)),
            })
            .collect();
        let mut node = builder.node(
            item(&name),
            "def",
            module,
            name,
            signature,
            declaration.name.span,
            joined(declaration.name.span, declaration.body.span.end),
            root,
        );
        node.params = params;
        node.result = Some(docgen::type_text(result));
        node.public = declaration.visibility == Visibility::Public;
        node.export = declaration.exported;
        node.pure = declaration.pure;
        node.function = function;
        documented(&mut node, declaration.doc.as_ref());
        builder.push(node);
    }
    for constant in constants {
        let mut node = builder.node(
            item(&constant.name.text),
            "const",
            module,
            constant.name.text.clone(),
            format!(
                "const {}: {}",
                constant.name.text,
                docgen::type_text(&constant.ty)
            ),
            constant.name.span,
            joined(constant.name.span, constant.value.span.end),
            root,
        );
        node.result = Some(docgen::type_text(&constant.ty));
        node.public = constant.visibility == Visibility::Public;
        node.function = names.local_function(key, &constant.name.text);
        documented(&mut node, constant.doc.as_ref());
        builder.push(node);
    }
    for external in externs {
        let function = names.local_function(key, &external.name.text);
        let resolved = function.map(|id| &checked[id].signature.parameters);
        let mut node = builder.node(
            item(&external.name.text),
            "extern",
            module,
            external.name.text.clone(),
            docgen::signature(
                "extern def",
                &external.name.text,
                &external.regions,
                &external.constraints,
                external.parameters.iter(),
                &external.result,
            ),
            external.name.span,
            joined(external.name.span, external.result.span.end),
            root,
        );
        node.params = external
            .parameters
            .iter()
            .enumerate()
            .map(|(index, ty)| CapturedParam {
                name: format!("arg{}", index + 1),
                ty: parameter_type_text(ty),
                conv: resolved
                    .and_then(|parameters| parameters.get(index))
                    .map_or("own", |ty| conv(ty, &types)),
            })
            .collect();
        node.result = Some(docgen::type_text(&external.result));
        node.public = external.visibility == Visibility::Public;
        node.export = external.exported;
        node.pure = external.pure;
        node.function = function;
        documented(&mut node, external.doc.as_ref());
        builder.push(node);
    }
    for handle in extern_types {
        let mut node = builder.node(
            item(&handle.name.text),
            "handle",
            module,
            handle.name.text.clone(),
            format!("extern type {}", handle.name.text),
            handle.name.span,
            handle.name.span,
            root,
        );
        node.public = handle.visibility == Visibility::Public;
        documented(&mut node, handle.doc.as_ref());
        builder.push(node);
    }
    for record in records {
        if record.name.provenance == Provenance::Generated {
            continue;
        }
        let end = record
            .fields
            .last()
            .map_or(record.name.span.end, |field| field.ty.span.end);
        let id = item(&record.name.text);
        let mut node = builder.node(
            id.clone(),
            "record",
            module,
            record.name.text.clone(),
            format!(
                "record {}{}{}",
                record.name.text,
                docgen::parameters_text(&record.parameters),
                docgen::region_text(&record.regions)
            ),
            record.name.span,
            joined(record.name.span, end),
            root,
        );
        let public = record.visibility == Visibility::Public;
        node.public = public;
        documented(&mut node, record.doc.as_ref());
        let parent = builder.push(node);
        for field in &record.fields {
            let mut node = builder.node(
                ids::item_id(&id, &field.name.text),
                "field",
                module,
                field.name.text.clone(),
                format!("{}: {}", field.name.text, docgen::type_text(&field.ty)),
                field.name.span,
                joined(field.name.span, field.ty.span.end),
                parent,
            );
            node.result = Some(docgen::type_text(&field.ty));
            node.public = public;
            builder.push(node);
        }
    }
    for union in unions {
        if union.name.provenance == Provenance::Generated {
            continue;
        }
        let end = union.cases.last().map_or(union.name.span.end, |case| {
            case.payload
                .as_ref()
                .map_or(case.name.span.end, |payload| payload.span.end)
        });
        let id = item(&union.name.text);
        let mut node = builder.node(
            id.clone(),
            "union",
            module,
            union.name.text.clone(),
            format!(
                "union {}{}",
                union.name.text,
                docgen::parameters_text(&union.parameters)
            ),
            union.name.span,
            joined(union.name.span, end),
            root,
        );
        let public = union.visibility == Visibility::Public;
        node.public = public;
        documented(&mut node, union.doc.as_ref());
        let parent = builder.push(node);
        for case in &union.cases {
            let payload = case.payload.as_ref().map(docgen::type_text);
            let mut node = builder.node(
                ids::item_id(&id, &case.name.text),
                "case",
                module,
                case.name.text.clone(),
                match &payload {
                    Some(payload) => format!("{} of {payload}", case.name.text),
                    None => case.name.text.clone(),
                },
                case.name.span,
                joined(
                    case.name.span,
                    case.payload
                        .as_ref()
                        .map_or(case.name.span.end, |payload| payload.span.end),
                ),
                parent,
            );
            node.result = payload;
            node.public = public;
            builder.push(node);
        }
    }
    for alias in type_aliases {
        let mut node = builder.node(
            item(&alias.name.text),
            "alias",
            module,
            alias.name.text.clone(),
            format!(
                "type {}{} = {}",
                alias.name.text,
                docgen::parameters_text(&alias.parameters),
                docgen::type_text(&alias.target)
            ),
            alias.name.span,
            joined(alias.name.span, alias.target.span.end),
            root,
        );
        node.result = Some(docgen::type_text(&alias.target));
        node.public = alias.visibility == Visibility::Public;
        documented(&mut node, alias.doc.as_ref());
        builder.push(node);
    }
    for class in classes {
        let end = class
            .methods
            .last()
            .map_or(class.name.span.end, |method| method.result.span.end);
        let id = item(&class.name.text);
        let mut node = builder.node(
            id.clone(),
            "class",
            module,
            class.name.text.clone(),
            format!(
                "class {}{}<'{}{}>",
                docgen::constraints_text(&class.superclasses),
                class.name.text,
                class.variable.text,
                if class.kind == crate::syntax::Kind::Type {
                    String::new()
                } else {
                    format!(": {}", class.kind.display())
                }
            ),
            class.name.span,
            joined(class.name.span, end),
            root,
        );
        documented(&mut node, class.doc.as_ref());
        let parent = builder.push(node);
        let class_id = builder
            .input
            .classes
            .class_id(&format!("{key}.{}", class.name.text));
        for (index, method) in class.methods.iter().enumerate() {
            let resolved =
                class_id.and_then(|class| builder.input.classes.method_signature_at(class, index));
            let mut node = builder.node(
                ids::item_id(&id, &method.name.text),
                "method",
                module,
                method.name.text.clone(),
                docgen::signature(
                    if method.pure { "pure def" } else { "def" },
                    &method.name.text,
                    &method.regions,
                    &method.constraints,
                    method.parameters.iter(),
                    &method.result,
                ),
                method.name.span,
                joined(method.name.span, method.result.span.end),
                parent,
            );
            node.params = method
                .parameters
                .iter()
                .enumerate()
                .map(|(index, ty)| CapturedParam {
                    name: format!("arg{}", index + 1),
                    ty: parameter_type_text(ty),
                    conv: resolved
                        .and_then(|signature| signature.parameters.get(index))
                        .map_or("own", |ty| conv(ty, &types)),
                })
                .collect();
            node.result = Some(docgen::type_text(&method.result));
            node.public = method.visibility == Visibility::Public;
            node.pure = method.pure;
            documented(&mut node, method.doc.as_ref());
            builder.push(node);
        }
    }
    let views = builder.input.classes.instance_views();
    for instance in instances {
        let Some(view) = views
            .iter()
            .find(|view| !view.derived && view.span == instance.class.span)
        else {
            continue;
        };
        let class_id = builder.class_id(view.class);
        let head = ids::type_id_text(view.head, &types, &builder.capture.module_ids);
        let id = ids::instance_id(&class_id, &head);
        let end = instance
            .methods
            .last()
            .map_or(instance.ty.span.end, |method| method.body.span.end);
        let node = builder.node(
            id.clone(),
            "instance",
            module,
            format!(
                "{}<{}>",
                instance.class.text,
                docgen::type_text(&instance.ty)
            ),
            format!(
                "instance {}{}<{}>",
                docgen::constraints_text(&instance.constraints),
                instance.class.text,
                docgen::type_text(&instance.ty)
            ),
            instance.class.span,
            joined(instance.class.span, end),
            root,
        );
        let parent = builder.push(node);
        for &(function, method) in &view.methods {
            let checked = &builder.input.module.functions[function];
            if !checked.name.starts_with("$instance.") {
                // A default method of the class: the instance does not write it.
                continue;
            }
            let Some(definition) = instance
                .methods
                .iter()
                .find(|definition| definition.name.text == method)
            else {
                continue;
            };
            let signature = checked.signature.as_type().display(&types);
            let mut node = builder.node(
                ids::item_id(&id, method),
                "method",
                module,
                method.to_owned(),
                format!("def {method} :: {signature}"),
                definition.name.span,
                joined(definition.name.span, definition.body.span.end),
                parent,
            );
            node.params = checked
                .parameters
                .iter()
                .zip(&checked.signature.parameters)
                .map(|(local, ty)| CapturedParam {
                    name: local.name.clone(),
                    ty: ty.display(&types),
                    conv: conv(ty, &types),
                })
                .collect();
            node.result = Some(checked.signature.result.display(&types));
            node.function = Some(function);
            builder.push(node);
        }
    }
    let module_tests = &builder.input.module.tests;
    for test in tests {
        let Some(checked) = module_tests
            .iter()
            .position(|checked| checked.span == test.name_span)
        else {
            continue;
        };
        let mut node = builder.node(
            ids::test_id(&module.id, &test.name),
            "test",
            module,
            test.name.clone(),
            format!("test \"{}\"", test.name),
            test.name_span,
            test.span,
            root,
        );
        node.public = false;
        node.function = Some(module_tests[checked].function);
        let index = builder.push(node);
        owners.push((index, builder.capture.tests.len()));
        builder.capture.tests.push(CapturedTest {
            id: String::new(),
            kind: "test",
            module: module.id.clone(),
            name: test.name.clone(),
            index: module_tests[checked].index,
            span: test.name_span,
        });
    }
    let module_benches = &builder.input.module.benches;
    for bench in benches {
        let Some(checked) = module_benches
            .iter()
            .position(|checked| checked.span == bench.name_span)
        else {
            continue;
        };
        let mut node = builder.node(
            ids::bench_id(&module.id, &bench.name),
            "bench",
            module,
            bench.name.clone(),
            format!("bench \"{}\"", bench.name),
            bench.name_span,
            bench.span,
            root,
        );
        node.public = false;
        node.function = Some(module_benches[checked].function);
        let index = builder.push(node);
        owners.push((index, builder.capture.tests.len()));
        builder.capture.tests.push(CapturedTest {
            id: String::new(),
            kind: "bench",
            module: module.id.clone(),
            name: bench.name.clone(),
            index: module_benches[checked].index,
            span: bench.name_span,
        });
    }
    // Top-level code is the function `$entry`; with `def main` the entry is `main` itself, a node
    // already.
    if input.program.entry.is_some()
        && let Some(entry) = builder.input.module.entry
        && checked[entry].module == key
    {
        let function = &checked[entry];
        let mut node = builder.node(
            item("$entry"),
            "def",
            module,
            "$entry".into(),
            format!("entry :: {}", function.signature.result.display(&types)),
            function.span,
            function.span,
            root,
        );
        node.result = Some(function.signature.result.display(&types));
        node.public = false;
        node.function = Some(entry);
        builder.push(node);
    }
}

/// `contains`: every node but a module has the node that declares it as its parent.
fn contains_edges(builder: &mut Builder<'_, '_>) {
    let nodes = &builder.capture.nodes;
    let edges: Vec<_> = nodes
        .iter()
        .filter_map(|node| {
            let parent = &nodes[node.parent?];
            Some(CapturedEdge {
                rel: "contains",
                from: parent.id.clone(),
                to: node.id.clone(),
                args: None,
                span: node.span,
            })
        })
        .collect();
    builder.capture.edges.extend(edges);
}

/// The node of the abstract function `id`, or the node text of an std function.
fn callee_id(builder: &Builder<'_, '_>, id: usize) -> Option<String> {
    let function = &builder.input.module.functions[id];
    if let Some(node) = builder
        .capture
        .function_ids
        .get(&(function.module.clone(), function.name.clone()))
    {
        return Some(node.clone());
    }
    (function.origin.module == ModuleOrigin::Std)
        .then(|| std_function_id(function))
        .flatten()
}

/// The target of a callee or a function value (appendix D1).
fn target_of(builder: &Builder<'_, '_>, expression: &TypedExpr, called: bool) -> Option<String> {
    match &expression.kind {
        TypedExprKind::Function(FunctionRef::User(id)) | TypedExprKind::GenericFunction(id, _) => {
            callee_id(builder, *id)
        }
        TypedExprKind::Function(FunctionRef::Builtin(instance)) if called => {
            Some(instance.builtin.name().to_owned())
        }
        TypedExprKind::Method(class, method, _) => Some(ids::item_id(
            &builder.class_id(*class),
            builder.input.classes.method_name(*class, *method),
        )),
        _ => None,
    }
}

/// `calls` and the loops of every function-like node, from the abstract bodies (appendix D1, D2,
/// E1): a call names its callee; a function used as a value is a call without arguments.
fn function_edges(builder: &mut Builder<'_, '_>, budget: &mut usize) {
    let owners: Vec<(String, usize)> = builder
        .capture
        .nodes
        .iter()
        .filter_map(|node| Some((node.id.clone(), node.function?)))
        .collect();
    let types = builder.types;
    let mut edges = Vec::new();
    for (id, function) in owners {
        let body = &builder.input.module.functions[function].body;
        let mut loops = Vec::new();
        let mut stack = vec![body];
        let mut visited = 0;
        while let Some(expression) = stack.pop() {
            if visited == FUNCTION_NODES || *budget == 0 {
                builder.capture.truncated.insert(id.clone());
                break;
            }
            visited += 1;
            *budget -= 1;
            let mut children = expression.children();
            match &expression.kind {
                TypedExprKind::Call(callee, arguments) => {
                    if let Some(to) = target_of(builder, callee, true) {
                        edges.push(CapturedEdge {
                            rel: "calls",
                            from: id.clone(),
                            to,
                            args: Some(
                                arguments
                                    .iter()
                                    .map(|argument| argument_conv(argument, &types))
                                    .collect(),
                            ),
                            span: callee.span,
                        });
                        children.remove(0);
                    }
                }
                TypedExprKind::Binary(crate::syntax::BinaryOp::Pipe, argument, callee) => {
                    if let Some(to) = target_of(builder, callee, true) {
                        edges.push(CapturedEdge {
                            rel: "calls",
                            from: id.clone(),
                            to,
                            args: Some(vec![argument_conv(argument, &types)]),
                            span: callee.span,
                        });
                        children.pop();
                    }
                }
                // The builtins that check into nodes of their own (G30): `Task.scope`,
                // `Parallel.*` and the task runners are calls too.
                TypedExprKind::Parallel(operation, arguments) => edges.push(CapturedEdge {
                    rel: "calls",
                    from: id.clone(),
                    to: operation.name().to_owned(),
                    args: Some(
                        arguments
                            .iter()
                            .map(|argument| argument_conv(argument, &types))
                            .collect(),
                    ),
                    span: expression.span,
                }),
                TypedExprKind::TaskRun(argument)
                | TypedExprKind::TaskParallel(argument)
                | TypedExprKind::TaskParallelResults(argument) => {
                    let builtin = match &expression.kind {
                        TypedExprKind::TaskRun(_) => crate::check::Builtin::TaskRun,
                        TypedExprKind::TaskParallel(_) => crate::check::Builtin::TaskParallel,
                        _ => crate::check::Builtin::TaskParallelResults,
                    };
                    edges.push(CapturedEdge {
                        rel: "calls",
                        from: id.clone(),
                        to: builtin.name().to_owned(),
                        args: Some(vec![argument_conv(argument, &types)]),
                        span: expression.span,
                    });
                }
                TypedExprKind::Function(FunctionRef::User(_))
                | TypedExprKind::GenericFunction(..)
                | TypedExprKind::Method(..) => {
                    if let Some(to) = target_of(builder, expression, false) {
                        edges.push(CapturedEdge {
                            rel: "calls",
                            from: id.clone(),
                            to,
                            args: Some(Vec::new()),
                            span: expression.span,
                        });
                    }
                }
                TypedExprKind::While { .. } => loops.push(("while", expression.span)),
                TypedExprKind::ForRange { .. } => loops.push(("for_range", expression.span)),
                TypedExprKind::ForEach { .. } => loops.push(("for_each", expression.span)),
                _ => {}
            }
            stack.extend(children.into_iter().rev());
        }
        loops.sort_by_key(|(kind, span)| (position(*span), span.end, *kind));
        loops.dedup();
        builder.capture.loops.insert(id, loops);
    }
    builder.capture.edges.extend(edges);
}

/// `implements`, `instance_of` and `imports` (appendix D5).
fn declared_edges(builder: &mut Builder<'_, '_>) {
    let types = builder.types;
    let views = builder.input.classes.instance_views();
    let mut edges = Vec::new();
    let nodes: BTreeMap<(Option<usize>, usize), &CapturedNode> = builder
        .capture
        .nodes
        .iter()
        .filter(|node| node.kind == "instance")
        .map(|node| ((node.span.source, node.span.start), node))
        .collect();
    for view in &views {
        if view.derived {
            continue;
        }
        let Some(node) = nodes.get(&(view.span.source, view.span.start)) else {
            continue;
        };
        let class = builder.class_id(view.class);
        edges.push(CapturedEdge {
            rel: "instance_of",
            from: node.id.clone(),
            to: class.clone(),
            args: None,
            span: view.span,
        });
        edges.push(CapturedEdge {
            rel: "implements",
            from: ids::type_id_text(view.head, &types, &builder.capture.module_ids),
            to: class,
            args: None,
            span: view.span,
        });
    }
    for module in &builder.capture.modules {
        let program = builder.input.modules[module.source].program;
        for using in &program.usings {
            edges.push(CapturedEdge {
                rel: "imports",
                from: module.id.clone(),
                to: using.path.text.clone(),
                args: None,
                span: using.span,
            });
        }
        let derives = program
            .records
            .iter()
            .filter(|record| record.name.provenance != Provenance::Generated)
            .map(|record| (&record.name, &record.derives))
            .chain(
                program
                    .unions
                    .iter()
                    .filter(|union| union.name.provenance != Provenance::Generated)
                    .map(|union| (&union.name, &union.derives)),
            );
        for (name, derives) in derives {
            for (class, span) in derives {
                edges.push(CapturedEdge {
                    rel: "implements",
                    from: ids::item_id(&module.id, &name.text),
                    to: class.name().to_owned(),
                    args: None,
                    span: *span,
                });
            }
        }
    }
    builder.capture.edges.extend(edges);
}

/// `uses_type` from the semantic index (appendix D4): each read of a record, union, class or extern
/// type, from the innermost declaration around it.
fn uses_type_edges(builder: &mut Builder<'_, '_>) {
    let Some(index) = builder.capture.semantic.as_ref() else {
        return;
    };
    let sources: BTreeSet<usize> = builder
        .capture
        .modules
        .iter()
        .map(|module| module.source)
        .collect();
    // The extents of each source, for the innermost one around a use.
    let mut extents: BTreeMap<usize, Vec<(usize, usize, &str)>> = BTreeMap::new();
    for node in &builder.capture.nodes {
        if node.kind == "module" {
            continue;
        }
        if let Some(source) = node.extent.source {
            extents.entry(source).or_default().push((
                node.extent.start,
                node.extent.end,
                node.id.as_str(),
            ));
        }
    }
    let mut edges = Vec::new();
    for reference in &index.references {
        if reference.role != semantic::Role::Read {
            continue;
        }
        let Some(source) = reference
            .span
            .source
            .filter(|source| sources.contains(source))
        else {
            continue;
        };
        let definition = &index.definitions[reference.definition];
        if !matches!(
            definition.kind,
            semantic::SymbolKind::Record
                | semantic::SymbolKind::Union
                | semantic::SymbolKind::Class
        ) {
            continue;
        }
        let to = ids::item_id(
            &builder
                .capture
                .module_ids
                .get(&definition.module)
                .cloned()
                .unwrap_or_else(|| definition.module.replace('.', "::")),
            &definition.name,
        );
        let from = extents.get(&source).and_then(|extents| {
            extents
                .iter()
                .filter(|(start, end, _)| {
                    *start <= reference.span.start && reference.span.end <= *end
                })
                .min_by_key(|(start, end, id)| (end - start, *id))
                .map(|(_, _, id)| (*id).to_owned())
        });
        let Some(from) = from.filter(|from| *from != to) else {
            continue;
        };
        edges.push(CapturedEdge {
            rel: "uses_type",
            from,
            to,
            args: None,
            span: reference.span,
        });
    }
    builder.capture.edges.extend(edges);
}

/// The reference graph of the abstract functions (`recursion::check`), as a `Digraph`.
fn reference_graph(references: &[Vec<usize>]) -> algo::Digraph {
    algo::Digraph::from_lists(references)
}

/// `tested_by` (appendix D3): the user functions that a test reaches by at most 8 references.
fn tested_by_edges(builder: &mut Builder<'_, '_>) {
    let graph = reference_graph(builder.input.references);
    let callable: BTreeMap<usize, &CapturedNode> = builder
        .capture
        .nodes
        .iter()
        .filter(|node| matches!(node.kind, "def" | "method" | "extern") && node.name != "$entry")
        .filter_map(|node| Some((node.function?, node)))
        .collect();
    let mut edges = Vec::new();
    for test in builder
        .capture
        .nodes
        .iter()
        .filter(|node| node.kind == "test")
    {
        let Some(function) = test.function else {
            continue;
        };
        let visits = algo::bfs(&graph, &[function], TESTED_BY_DEPTH, usize::MAX);
        let mut listed = 0;
        for visit in visits.iter().filter(|visit| visit.depth > 0) {
            if listed == TESTED_BY_LIMIT {
                break;
            }
            if let Some(node) = callable.get(&visit.node) {
                listed += 1;
                edges.push(CapturedEdge {
                    rel: "tested_by",
                    from: node.id.clone(),
                    to: test.id.clone(),
                    args: None,
                    span: test.span,
                });
            }
        }
    }
    builder.capture.edges.extend(edges);
}

/// The calls of `function` to itself in its body, and whether each is in tail position.
fn self_calls(body: &TypedExpr, function: usize) -> Vec<bool> {
    let is_self = |callee: &TypedExpr| {
        matches!(&callee.kind,
            TypedExprKind::Function(FunctionRef::User(id)) | TypedExprKind::GenericFunction(id, _)
                if *id == function)
    };
    let mut calls = Vec::new();
    // (expression, in tail position)
    let mut stack = vec![(body, true)];
    let mut visited = 0;
    while let Some((expression, tail)) = stack.pop() {
        visited += 1;
        if visited > FUNCTION_NODES {
            calls.push(false);
            break;
        }
        match &expression.kind {
            TypedExprKind::Call(callee, arguments) if is_self(callee) => {
                calls.push(tail);
                stack.extend(arguments.iter().map(|argument| (argument, false)));
                continue;
            }
            TypedExprKind::Binary(crate::syntax::BinaryOp::Pipe, argument, callee)
                if is_self(callee) =>
            {
                calls.push(tail);
                stack.push((argument, false));
                continue;
            }
            TypedExprKind::If {
                condition,
                then_branch,
                else_branch,
            } => {
                stack.push((condition, false));
                stack.push((then_branch, tail));
                stack.push((else_branch, tail));
                continue;
            }
            TypedExprKind::Block { bindings, result } => {
                stack.extend(bindings.iter().map(|(_, value)| (value, false)));
                stack.push((result, tail));
                continue;
            }
            TypedExprKind::Match { value, arms, .. } => {
                stack.push((value, false));
                for arm in arms {
                    for alternative in &arm.alternatives {
                        stack.extend(
                            alternative
                                .steps
                                .iter()
                                .map(|step| (step.expression(), false)),
                        );
                        stack.extend(alternative.bindings.iter().map(|(_, value)| (value, false)));
                    }
                    stack.extend(arm.guard.iter().map(|guard| (guard, false)));
                    stack.push((&arm.body, tail));
                }
                continue;
            }
            _ => {}
        }
        if is_self(expression) {
            // A reference to itself that is not a direct call.
            calls.push(false);
        }
        stack.extend(
            expression
                .children()
                .into_iter()
                .map(|child| (child, false)),
        );
    }
    calls
}

/// `recursion` (algorithm A4): the strongly connected components of the reference graph.
fn recursion(builder: &mut Builder<'_, '_>) {
    let graph = reference_graph(builder.input.references);
    let component = algo::strongly_connected(&graph);
    let mut sizes = BTreeMap::new();
    for &of in &component {
        *sizes.entry(of).or_insert(0usize) += 1;
    }
    let types = builder.types;
    let functions = &builder.input.module.functions;
    let mut classes = BTreeMap::new();
    for node in &builder.capture.nodes {
        let Some(function) = node.function else {
            continue;
        };
        if !matches!(node.kind, "def" | "method" | "extern" | "test" | "bench") {
            continue;
        }
        let class = if function >= component.len() {
            "none"
        } else if sizes[&component[function]] > 1 {
            "mutual"
        } else if graph.has_edge(function, function) {
            let calls = self_calls(&functions[function].body, function);
            let borrowing = functions[function]
                .signature
                .parameters
                .iter()
                .any(|ty| ty.carries_loans(&types));
            if !calls.is_empty() && calls.iter().all(|tail| *tail) && !borrowing {
                "self_tail"
            } else {
                "self"
            }
        } else {
            "none"
        };
        classes.insert(node.id.clone(), class);
    }
    builder.capture.recursion = classes;
}
