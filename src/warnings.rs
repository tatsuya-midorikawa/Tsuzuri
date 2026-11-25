use super::*;

#[derive(Clone, Copy, Default)]
pub(super) struct WarningOptions {
    pub shadowing: bool,
}

/// The unqualified names of the program's functions (`area` for `Shapes.area`), once per analysis
/// for `unused_locals`.
pub(super) fn function_names(names: &Names) -> BTreeSet<&str> {
    names
        .functions
        .keys()
        .map(|name| {
            name.rsplit_once('.')
                .map_or(name.as_str(), |(_, last)| last)
        })
        .collect()
}

/// W1001 at each local of `function` that nothing uses. The warning carries the `certain` fix
/// that prefixes the name with `_`, when no local of the function and no function of the program
/// (`function_names`) already has the prefixed name, so the rename cannot capture or shadow
/// another name (G24).
pub(super) fn unused_locals(
    function: &CheckedFunction,
    function_names: &BTreeSet<&str>,
) -> Vec<Diagnostic> {
    if function.origin.module != ModuleOrigin::User {
        return Vec::new();
    }
    let mut locals: BTreeMap<_, _> = function
        .parameters
        .iter()
        .map(|local| (local.id, local))
        .collect();
    let mut uses = Uses::default();
    collect_locals(&function.body, &mut locals, &mut uses);
    // A rename of one span cannot reach the other binders of an or-pattern, nor the mentions of a
    // `let rec` function inside its group.
    let partial: BTreeSet<usize> = uses
        .calls
        .values()
        .flatten()
        .chain(&uses.shared)
        .copied()
        .collect();
    let used = uses.finish();
    let unused: Vec<_> = locals
        .values()
        .filter(|local| {
            local.provenance == Provenance::User
                && !local.name.starts_with('_')
                && !used.contains(&local.id)
        })
        .collect();
    if unused.is_empty() {
        return Vec::new();
    }
    let taken: BTreeSet<&str> = locals.values().map(|local| local.name.as_str()).collect();
    unused
        .into_iter()
        .map(|local| {
            let diagnostic = Diagnostic::of(
                DiagnosticKind::UnusedLocal,
                format!(
                    "unused local '{}'; prefix it with '_' to silence this warning",
                    local.name
                ),
                local.span,
            );
            let prefixed = format!("_{}", local.name);
            // The span must be the name itself for the edit to rename it.
            if taken.contains(prefixed.as_str())
                || function_names.contains(prefixed.as_str())
                || partial.contains(&local.id)
                || local.span.end - local.span.start != local.name.len()
            {
                return diagnostic;
            }
            diagnostic.fix(
                format!("Prefix '{}' with '_'", local.name),
                Applicability::Certain,
                vec![Edit {
                    span: local.span,
                    text: prefixed,
                }],
            )
        })
        .collect()
}

/// The locals that a body mentions. A `let rec` function that only its own group's bodies mention
/// is unused unless a used function of the group calls it (B09), so those mentions are kept as
/// calls between the functions.
#[derive(Default)]
struct Uses {
    used: BTreeSet<usize>,
    /// The `let rec` functions whose bodies are being walked, each with its group.
    bodies: Vec<(usize, Vec<usize>)>,
    /// The functions of a group that the body of each function mentions.
    calls: BTreeMap<usize, BTreeSet<usize>>,
    /// The locals that several alternatives of a match arm bind, each at its own span.
    shared: BTreeSet<usize>,
}

impl Uses {
    fn mention(&mut self, id: usize) {
        match self
            .bodies
            .iter()
            .rev()
            .find(|(_, group)| group.contains(&id))
        {
            Some((owner, _)) => {
                self.calls.entry(*owner).or_default().insert(id);
            }
            None => {
                self.used.insert(id);
            }
        }
    }

    /// The used locals: what a used function calls is used too.
    fn finish(mut self) -> BTreeSet<usize> {
        loop {
            let mut changed = false;
            for (owner, callees) in &self.calls {
                if self.used.contains(owner) {
                    for callee in callees {
                        changed |= self.used.insert(*callee);
                    }
                }
            }
            if !changed {
                return self.used;
            }
        }
    }
}

/// The locals a block declares, and what its values and its result mention. The body of a `let rec`
/// function is walked as the body of that function. A function of its own, so that the frame of
/// `collect_locals`, which every level of an expression pays for, stays small.
fn collect_block<'a>(
    bindings: &'a [(Local, TypedExpr)],
    result: &'a TypedExpr,
    locals: &mut BTreeMap<usize, &'a Local>,
    uses: &mut Uses,
) {
    locals.extend(bindings.iter().map(|(local, _)| (local.id, local)));
    for (local, value) in bindings {
        match &value.kind {
            TypedExprKind::Lambda { recursive, .. } if !recursive.is_empty() => {
                let group = recursive.iter().map(|member| member.id).collect();
                uses.bodies.push((local.id, group));
                collect_locals(value, locals, uses);
                uses.bodies.pop();
            }
            _ => collect_locals(value, locals, uses),
        }
    }
    collect_locals(result, locals, uses);
}

fn collect_locals<'a>(
    expression: &'a TypedExpr,
    locals: &mut BTreeMap<usize, &'a Local>,
    uses: &mut Uses,
) {
    use TypedExprKind::*;
    match &expression.kind {
        Local(id) => uses.mention(*id),
        Lambda {
            parameters,
            captures,
            ..
        } => {
            locals.extend(parameters.iter().map(|local| (local.id, local)));
            for capture in captures {
                uses.mention(capture.id);
            }
        }
        Block { bindings, result } => {
            collect_block(bindings, result, locals, uses);
            return;
        }
        ForRange { local, .. } | ForEach { local, .. } => {
            locals.insert(local.id, local);
        }
        Match { arms, .. } => {
            for arm in arms {
                if arm.alternatives.len() > 1 {
                    uses.shared.extend(
                        arm.alternatives
                            .iter()
                            .flat_map(|alternative| &alternative.bindings)
                            .map(|(local, _)| local.id),
                    );
                }
                for alternative in &arm.alternatives {
                    locals.extend(
                        alternative
                            .bindings
                            .iter()
                            .map(|(local, _)| (local.id, local)),
                    );
                    for step in &alternative.steps {
                        if let PatternStep::Bind(local, _) = step {
                            locals.insert(local.id, local);
                        }
                    }
                }
            }
        }
        Error
        | Int(_)
        | Float(_)
        | String(_)
        | Bool(_)
        | Unit
        | Break
        | Continue
        | Function(_)
        | GenericFunction(..)
        | Method(..)
        | TypeFunction { .. }
        | GenericInteger(..)
        | GenericFloat(_)
        | Unary(..)
        | Binary(..)
        | Call(..)
        | HostCall(..)
        | Closure(..)
        | RoutineGo(_)
        | SelectWait(_)
        | If { .. }
        | While { .. }
        | Record(_)
        | RecordUpdate { .. }
        | Array(_)
        | List(_)
        | Tuple(_)
        | ListTail(..)
        | NewArray(..)
        | NewList(..)
        | NewLiteral(_)
        | Field(..)
        | Index(..)
        | Slice { .. }
        | Length(_)
        | StringLength(_)
        | Borrow(..)
        | BorrowOperand(_)
        | Dereference(_)
        | ReferenceOperand(_)
        | Assign(..)
        | Cast(_)
        | SaturatingIndex(_)
        | CaseConstructor { .. }
        | Construct { .. }
        | UnionTag(_)
        | UnionPayload { .. }
        | Checked(_)
        | Try(_)
        | RaisedException
        | Reraise
        | Parallel(..)
        | StructuralCompare(..)
        | StructuralHash(_)
        | StructuralDisplay(_)
        | Interpolated(_)
        | Static(_)
        | DynDispatch { .. } => {}
    }
    for child in expression.children() {
        collect_locals(child, locals, uses);
    }
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Dependency {
    Function(usize),
    Type(String),
}

/// What a test runs on the names of the program whose private declarations `unused_private`
/// finds (PB08).
#[cfg(test)]
type NamesProbe = Option<Box<dyn FnMut(&Names)>>;

#[cfg(test)]
thread_local! {
    static NAMES_PROBE: std::cell::RefCell<NamesProbe> = const { std::cell::RefCell::new(None) };
}

pub(super) fn unused_private(
    functions: &[CheckedFunction],
    declarations: &[(String, FunctionDecl)],
    modules: &[ModuleInput<'_>],
    names: &Names,
    types: TypeContext<'_>,
    calls: &[Vec<usize>],
    entry: Option<usize>,
) -> Vec<Diagnostic> {
    #[cfg(test)]
    NAMES_PROBE.with_borrow_mut(|probe| {
        if let Some(probe) = probe {
            probe(names);
        }
    });
    let mut edges = BTreeMap::new();
    let mut roots = BTreeSet::new();
    let mut candidates = BTreeMap::new();
    for (id, function) in functions.iter().enumerate() {
        let node = Dependency::Function(id);
        let mut references: BTreeSet<_> = calls[id]
            .iter()
            .copied()
            .map(Dependency::Function)
            .collect();
        let mut scan = TypeReferences {
            names,
            types,
            module: &function.module,
            references: &mut references,
        };
        for parameter in &function.signature.parameters {
            scan.checked_type(parameter);
        }
        scan.checked_type(&function.signature.result);
        scan.checked_expression(&function.body);
        if let Some((_, declaration)) = declarations.get(id) {
            for parameter in &declaration.parameters {
                scan.ty(&parameter.ty);
            }
            scan.ty(&declaration.result);
            for constraint in &declaration.constraints {
                scan.ty(&constraint.ty);
            }
            scan.expression(&declaration.body);
        }
        edges.insert(node.clone(), references);
        if function.origin.module == ModuleOrigin::User {
            if function.visibility == Visibility::Private
                && function.origin.provenance == Provenance::User
                && !function.exported
                && function.origin.test.is_none()
                && Some(id) != entry
            {
                candidates.insert(
                    node,
                    Diagnostic::of(
                        DiagnosticKind::UnusedPrivateFunction,
                        format!(
                            "unused private function '{}'; remove it or make it public",
                            names.spelling(&function.qualified_name())
                        ),
                        function.span,
                    ),
                );
            } else {
                roots.insert(node);
            }
        }
    }
    for input in modules
        .iter()
        .filter(|input| input.origin == ModuleOrigin::User)
    {
        let program = input.program;
        let type_declarations = program
            .records
            .iter()
            .map(|record| {
                (
                    &record.name,
                    record.visibility,
                    record
                        .fields
                        .iter()
                        .map(|field| &field.ty)
                        .collect::<Vec<_>>(),
                )
            })
            .chain(program.unions.iter().map(|union| {
                (
                    &union.name,
                    union.visibility,
                    union
                        .cases
                        .iter()
                        .filter_map(|case| case.payload.as_ref())
                        .collect(),
                )
            }))
            .chain(
                program
                    .type_aliases
                    .iter()
                    .map(|alias| (&alias.name, alias.visibility, vec![&alias.target])),
            );
        for (name, visibility, fields) in type_declarations {
            let qualified = format!("{}.{}", input.name, name.text);
            let node = Dependency::Type(qualified.clone());
            let mut references = BTreeSet::new();
            let mut scan = TypeReferences {
                names,
                types,
                module: input.name,
                references: &mut references,
            };
            for field in fields {
                scan.ty(field);
            }
            edges.insert(node.clone(), references);
            if visibility == Visibility::Private && name.provenance == Provenance::User {
                candidates.insert(
                    node,
                    Diagnostic::of(
                        DiagnosticKind::UnusedPrivateType,
                        format!(
                            "unused private type '{}'; remove it or make it public",
                            names.type_spelling(&qualified)
                        ),
                        name.span,
                    ),
                );
            } else {
                roots.insert(node);
            }
        }
        let mut references = BTreeSet::new();
        let mut scan = TypeReferences {
            names,
            types,
            module: input.name,
            references: &mut references,
        };
        for class in &program.classes {
            for method in &class.methods {
                for parameter in &method.parameters {
                    scan.ty(parameter);
                }
                scan.ty(&method.result);
                for constraint in &method.constraints {
                    scan.ty(&constraint.ty);
                }
            }
        }
        for instance in &program.instances {
            scan.ty(&instance.ty);
        }
        roots.extend(references);
        if let (Some(id), Some(expression)) = (entry, &program.entry) {
            let references = edges.entry(Dependency::Function(id)).or_default();
            TypeReferences {
                names,
                types,
                module: input.name,
                references,
            }
            .expression(expression);
        }
    }
    let mut reached = BTreeSet::new();
    let mut pending: Vec<_> = roots.into_iter().collect();
    while let Some(node) = pending.pop() {
        if reached.insert(node.clone()) {
            if let Some(references) = edges.get(&node) {
                pending.extend(references.iter().cloned());
            }
        }
    }
    candidates
        .into_iter()
        .filter_map(|(node, diagnostic)| (!reached.contains(&node)).then_some(diagnostic))
        .collect()
}

struct TypeReferences<'a> {
    names: &'a Names,
    types: TypeContext<'a>,
    module: &'a str,
    references: &'a mut BTreeSet<Dependency>,
}

impl TypeReferences<'_> {
    fn named(&mut self, name: &str, span: Span) {
        if let Some(ty) = self.names.quiet_named_type(self.module, name, span) {
            self.references
                .insert(Dependency::Type(ty.info().name.clone()));
        }
    }

    fn ty(&mut self, ty: &TypeExpr) {
        match &ty.kind {
            TypeExprKind::Named(name) => self.named(name, ty.span),
            TypeExprKind::Apply(name, arguments) => {
                self.named(&name.text, name.span);
                for argument in arguments {
                    self.ty(argument);
                }
            }
            TypeExprKind::Array(inner)
            | TypeExprKind::ArrayView(inner)
            | TypeExprKind::List(inner)
            | TypeExprKind::Own(inner)
            | TypeExprKind::Mut(inner)
            | TypeExprKind::Pure(inner)
            | TypeExprKind::Reference(inner, _)
            | TypeExprKind::Regions(inner, _)
            | TypeExprKind::Quantified(_, inner) => self.ty(inner),
            TypeExprKind::Tuple(elements) => {
                for element in elements {
                    self.ty(element);
                }
            }
            TypeExprKind::AnonymousUnion(elements) => {
                for element in elements {
                    self.ty(element);
                }
            }
            TypeExprKind::FixedArray(element, length) => {
                self.ty(element);
                // A named length may be an integer constant, which this type then uses.
                if let TypeExprKind::Named(name) = &length.kind {
                    if let Some((id, _)) =
                        self.names.length_constant(self.module, name, length.span)
                    {
                        self.references.insert(Dependency::Function(id));
                    }
                }
            }
            TypeExprKind::Function(parameters, result) => {
                for parameter in parameters.iter() {
                    self.ty(parameter);
                }
                self.ty(result);
            }
            // A dyn type names classes, which have no visibility (A14).
            TypeExprKind::Variable(_) | TypeExprKind::Length(_) | TypeExprKind::Dyn(_) => {}
        }
    }

    fn checked_type(&mut self, ty: &Type) {
        match ty {
            Type::Record(id, arguments) | Type::Union(id, arguments) => {
                let name = if matches!(ty, Type::Record(..)) {
                    &self.types.records[*id].name
                } else {
                    &self.types.unions[*id].name
                };
                self.references.insert(Dependency::Type(name.clone()));
                for argument in arguments {
                    self.checked_type(argument);
                }
            }
            Type::Array(inner)
            | Type::List(inner)
            | Type::Vec(inner)
            | Type::Once(inner)
            | Type::FixedArray(inner, _)
            | Type::Shared(inner, _)
            | Type::Reference(inner, _) => self.checked_type(inner),
            Type::Tuple(elements) => {
                for element in elements {
                    self.checked_type(element);
                }
            }
            Type::Function(parameters, result) => {
                for parameter in parameters.iter() {
                    self.checked_type(parameter);
                }
                self.checked_type(result);
            }
            _ => {}
        }
    }

    fn checked_expression(&mut self, expression: &TypedExpr) {
        self.checked_type(&expression.ty);
        for child in expression.children() {
            self.checked_expression(child);
        }
    }

    fn expression(&mut self, expression: &Expr) {
        use ExprKind::*;
        match &expression.kind {
            Block { bindings, result } => {
                for binding in bindings {
                    if let Some((pattern, _)) = binding.pattern.as_deref() {
                        self.pattern(pattern);
                    }
                    if let Some(ty) = &binding.annotation {
                        self.ty(ty);
                    }
                    self.expression(&binding.value);
                }
                self.expression(result);
            }
            NewArray(ty, length, initialize) | NewList(ty, length, initialize) => {
                self.ty(ty);
                self.expression(length);
                self.expression(initialize);
            }
            Cast(value, ty) => {
                self.ty(ty);
                self.expression(value);
            }
            For {
                pattern,
                source,
                body,
            } => {
                self.pattern(pattern);
                self.expression(source);
                self.expression(body);
            }
            Match { value, arms, .. } => {
                self.expression(value);
                for arm in arms {
                    self.pattern(&arm.pattern);
                    if let Some(guard) = &arm.guard {
                        self.expression(guard);
                    }
                    self.expression(&arm.body);
                }
            }
            Try(handled) => {
                self.expression(&handled.body);
                for arm in &handled.arms {
                    self.pattern(&arm.pattern);
                    if let Some(guard) = &arm.guard {
                        self.expression(guard);
                    }
                    self.expression(&arm.body);
                }
                if let Some(finally) = &handled.finally {
                    self.expression(finally);
                }
            }
            Select(arms) => {
                for arm in arms {
                    for expression in arm.expressions() {
                        self.expression(expression);
                    }
                }
            }
            Record { name, fields } => {
                self.named(&name.text, name.span);
                for (_, value) in fields {
                    self.expression(value);
                }
            }
            RecordUpdate { base, fields } => {
                self.expression(base);
                for (_, value) in fields {
                    self.expression(value);
                }
            }
            Computation(_, body) => self.computation(body),
            Unary(_, value)
            | Lambda(_, value)
            | Routine(value)
            | ComputationBoundary(value)
            | Checked(value)
            | NewLiteral(value)
            | Field(value, _)
            | Borrow(value, ..)
            | Dereference(value, _) => self.expression(value),
            Binary(_, left, right)
            | Index(left, right)
            | Assign(left, right)
            | While {
                condition: left,
                body: right,
            } => {
                self.expression(left);
                self.expression(right);
            }
            Call(callee, arguments) => {
                self.expression(callee);
                for argument in arguments {
                    self.expression(argument);
                }
            }
            If {
                condition,
                then_branch,
                else_branch,
            } => {
                self.expression(condition);
                self.expression(then_branch);
                self.expression(else_branch);
            }
            Range {
                start,
                step,
                finish,
                ..
            } => {
                self.expression(start);
                if let Some(step) = step {
                    self.expression(step);
                }
                self.expression(finish);
            }
            Slice {
                value, start, end, ..
            } => {
                self.expression(value);
                for bound in start.iter().chain(end.iter()) {
                    self.expression(bound);
                }
            }
            Array(values) | List(values) | Tuple(values) => {
                for value in values {
                    self.expression(value);
                }
            }
            Interpolated(interpolation) => {
                for hole in &interpolation.holes {
                    self.expression(&hole.value);
                }
            }
            Integer(..)
            | BigInt(_)
            | Float(..)
            | String(_)
            | Char(_)
            | Utf8Char(_)
            | Bool(_)
            | Unit
            | Break
            | Continue
            | Hole(_)
            | Name(_)
            | QualifiedFunction(_)
            | TypeFunction(..)
            | DynDispatch { .. } => {}
        }
    }

    fn pattern(&mut self, pattern: &Pattern) {
        use PatternKind::*;
        match &pattern.kind {
            Annotated(pattern, ty) => {
                self.ty(ty);
                self.pattern(pattern);
            }
            Record(name, fields) => {
                if let Some(name) = name {
                    self.named(&name.text, name.span);
                }
                for (_, field) in fields {
                    self.pattern(field);
                }
            }
            Apply(_, patterns) | Tuple(patterns) | Array(patterns) | List(patterns) => {
                for pattern in patterns {
                    self.pattern(pattern);
                }
            }
            Cons(left, right) | Or(left, right) | And(left, right) => {
                self.pattern(left);
                self.pattern(right);
            }
            As(pattern, _) => self.pattern(pattern),
            Argument(value) | Literal(value) => self.expression(value),
            TypeTest(ty, ..) => self.ty(ty),
            Wildcard | Binding(_) => {}
        }
    }

    fn computation(&mut self, block: &ComputationBlock) {
        use ComputationStatementKind::*;
        for statement in &block.statements {
            match &statement.kind {
                LetAnd(bindings) => {
                    for binding in bindings {
                        if let Some((pattern, _)) = binding.pattern.as_deref() {
                            self.pattern(pattern);
                        }
                        if let Some(ty) = &binding.annotation {
                            self.ty(ty);
                        }
                        self.expression(&binding.value);
                    }
                }
                Match(value, arms) => {
                    self.expression(value);
                    for arm in arms {
                        self.pattern(&arm.pattern);
                        if let Some(guard) = &arm.guard {
                            self.expression(guard);
                        }
                        self.computation(&arm.body);
                    }
                }
                Let(binding, _) => {
                    if let Some((pattern, _)) = binding.pattern.as_deref() {
                        self.pattern(pattern);
                    }
                    if let Some(ty) = &binding.annotation {
                        self.ty(ty);
                    }
                    self.expression(&binding.value);
                }
                Do(value) | Operation(_, value) | Expression(value) => self.expression(value),
                If(condition, yes, no) => {
                    self.expression(condition);
                    self.computation(yes);
                    if let Some(no) = no {
                        self.computation(no);
                    }
                }
                For(pattern, source, body) => {
                    self.pattern(pattern);
                    self.expression(source);
                    self.computation(body);
                }
                While(condition, body) => {
                    self.expression(condition);
                    self.computation(body);
                }
            }
        }
    }
}

/// The longest deprecation note that a warning shows, in characters; a longer note is cut.
const MAX_NOTE_CHARACTERS: usize = 200;

/// A public `def`, `record` or `union` whose documentation has the `@deprecated` tag.
pub(super) struct Deprecation {
    /// The index in `modules` of the module that declares it.
    pub module: usize,
    pub note: String,
}

/// The deprecated declarations of `modules` that a use can warn about, by the source and start
/// of their name, the position that the semantic index gives a declaration. A declaration of
/// std is always one. A declaration of a user module is one only in a dependency package: no
/// package depends on the root package, and W1005 never reports a use within a package.
pub(super) fn deprecations(modules: &[ModuleInput<'_>]) -> BTreeMap<(usize, usize), Deprecation> {
    let mut found = BTreeMap::new();
    for (module, input) in modules.iter().enumerate() {
        if input.origin == ModuleOrigin::User && input.package.is_none() {
            continue;
        }
        let program = input.program;
        let declared =
            program
                .functions
                .iter()
                .map(|declaration| (&declaration.name, declaration.visibility, &declaration.doc))
                .chain(program.records.iter().map(|declaration| {
                    (&declaration.name, declaration.visibility, &declaration.doc)
                }))
                .chain(program.unions.iter().map(|declaration| {
                    (&declaration.name, declaration.visibility, &declaration.doc)
                }));
        for (name, visibility, doc) in declared {
            if visibility == Visibility::Private || name.provenance == Provenance::Generated {
                continue;
            }
            let (Some(doc), Some(source)) = (doc, name.span.source) else {
                continue;
            };
            if let Some(note) = doc.deprecation() {
                found.insert(
                    (source, name.span.start),
                    Deprecation {
                        module,
                        note: note.to_owned(),
                    },
                );
            }
        }
    }
    found
}

/// A reference to a deprecated declaration that W1005 can report.
struct DeprecatedUse {
    source: usize,
    span: Span,
    /// The definition of the deprecated declaration: the union for a case.
    owner: usize,
    case: bool,
    message: String,
    /// The function that the note names as the replacement (`replacement_for`).
    replacement: Option<String>,
}

/// The function that replaces the deprecated function `owner` at a use spelled `spelled`: the
/// note is exactly `use {path} instead`, `path` names a function of the same module as
/// `spelled` does, and that function is public and has the same parameter and result types.
fn replacement_for(
    owner: &semantic::Definition,
    spelled: &str,
    note: &str,
    index: &semantic::SemanticIndex,
) -> Option<String> {
    if owner.kind != semantic::SymbolKind::Function {
        return None;
    }
    let path = note.trim().strip_prefix("use ")?.strip_suffix(" instead")?;
    if path.is_empty() || path.contains(char::is_whitespace) {
        return None;
    }
    let (module, target) = path.rsplit_once('.')?;
    if spelled.rsplit_once('.').map(|(module, _)| module) != Some(module)
        || !target.starts_with(|c: char| c.is_ascii_lowercase() || c == '_')
        || !target
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        return None;
    }
    let types = |definition: &semantic::Definition| -> Vec<String> {
        definition
            .parameters
            .iter()
            .map(|parameter| {
                parameter
                    .split_once(": ")
                    .map_or(parameter.as_str(), |(_, ty)| ty)
                    .to_owned()
            })
            .collect()
    };
    // The modifiers (`pure `, `const `) and the declared signature (`own string -> unit`): a
    // replacement must accept every call and every context that the deprecated function does,
    // which the signature shows only without type parameters and named regions.
    fn signature(definition: &semantic::Definition) -> Option<(&str, &str)> {
        if !definition.plain_signature {
            return None;
        }
        let (head, signature) = definition.detail.split_once(" :: ")?;
        let (modifiers, _) = head.split_once("def ")?;
        Some((modifiers, signature))
    }
    index
        .definitions
        .iter()
        .any(|candidate| {
            candidate.kind == semantic::SymbolKind::Function
                && candidate.module == owner.module
                && candidate.name == target
                && candidate.public
                && types(candidate) == types(owner)
                && candidate.result == owner.result
                && signature(candidate).is_some()
                && signature(candidate) == signature(owner)
        })
        .then(|| target.to_owned())
}

/// W1005 at each reference, in a user module, to a deprecated declaration of another package
/// or of std (G19). A union stands for its cases and a record for its construction, patterns
/// and type annotations, as the semantic index records them.
pub(super) fn deprecated_uses(
    modules: &[ModuleInput<'_>],
    index: &semantic::SemanticIndex,
    names: &Names,
    deprecations: &BTreeMap<(usize, usize), Deprecation>,
) -> Vec<Diagnostic> {
    let mut reported = BTreeSet::new();
    // The replacement of each deprecated function, found once: the search walks every definition.
    let mut replacements: BTreeMap<usize, Option<String>> = BTreeMap::new();
    let mut uses = Vec::new();
    for reference in &index.references {
        let Some(source) = reference.span.source else {
            continue;
        };
        let Some(user) = modules
            .get(source)
            .filter(|module| module.origin == ModuleOrigin::User)
        else {
            continue;
        };
        let definition = &index.definitions[reference.definition];
        let (owner_id, case) = match (definition.kind, definition.container) {
            (
                semantic::SymbolKind::Function
                | semantic::SymbolKind::Record
                | semantic::SymbolKind::Union,
                _,
            ) => (reference.definition, false),
            (semantic::SymbolKind::Case, Some(union)) => (union, true),
            _ => continue,
        };
        let owner = &index.definitions[owner_id];
        let Some(deprecation) = owner
            .span
            .source
            .and_then(|source| deprecations.get(&(source, owner.span.start)))
        else {
            continue;
        };
        let declaring = &modules[deprecation.module];
        if declaring.origin == ModuleOrigin::User && declaring.package == user.package {
            continue;
        }
        let key = (source, reference.span.start, reference.span.end);
        if !reported.insert(key) {
            continue;
        }
        let qualified = format!("{}.{}", owner.module, owner.name);
        let spelled = if owner.kind == semantic::SymbolKind::Function {
            names.spelling(&qualified)
        } else {
            names.type_spelling(&qualified)
        };
        let mut note = deprecation.note.chars();
        let shown: String = note.by_ref().take(MAX_NOTE_CHARACTERS).collect();
        let message = match (shown.is_empty(), note.next().is_some()) {
            (true, _) => {
                format!("'{spelled}' is deprecated; see its documentation for a replacement")
            }
            (false, false) => format!("'{spelled}' is deprecated: {shown}"),
            (false, true) => format!("'{spelled}' is deprecated: {shown}..."),
        };
        // The span must be the name itself for the edit to replace it.
        let replacement = replacements
            .entry(owner_id)
            .or_insert_with(|| replacement_for(owner, &spelled, &deprecation.note, index))
            .clone()
            .filter(|_| reference.span.end - reference.span.start == owner.name.len());
        uses.push(DeprecatedUse {
            source,
            span: reference.span,
            owner: owner_id,
            case,
            message,
            replacement,
        });
    }
    // `Union.Case` names the union and the case, which is one use: it warns at the case.
    let cases: BTreeSet<_> = uses
        .iter()
        .filter(|used| used.case)
        .map(|used| (used.source, used.owner, used.span.start))
        .collect();
    uses.into_iter()
        .filter(|used| used.case || !cases.contains(&(used.source, used.owner, used.span.end + 1)))
        .map(|used| {
            let diagnostic = Diagnostic::of(DiagnosticKind::DeprecatedUse, used.message, used.span);
            match used.replacement {
                Some(target) => diagnostic.fix(
                    format!(
                        "Replace '{}' with '{target}'",
                        index.definitions[used.owner].name
                    ),
                    Applicability::Likely,
                    vec![Edit {
                        span: used.span,
                        text: target,
                    }],
                ),
                None => diagnostic,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_usable_deprecation_makes_the_check_collect_references() {
        use crate::SourceInput;
        use crate::package::PackageId;
        use crate::syntax::Edition;
        let collections = || semantic::COLLECTIONS.with(std::cell::Cell::get);
        let tagged = "/// @deprecated\ndef old :: i64 = 1\n";
        let std = [("std/Legacy.tz", tagged)];
        let input = |path, text, origin, package| SourceInput {
            path,
            text,
            origin,
            namespace: "",
            edition: Edition::LATEST,
            package,
        };
        let user = ModuleOrigin::User;
        // No tag: the check runs no collection.
        let before = collections();
        crate::analyze("let x = 1\nx").unwrap();
        assert_eq!(collections(), before);
        // A tag in the only user package can never warn.
        crate::analyze_modules(&[("Main.tz", "Old.old"), ("Old.tz", tagged)]).unwrap();
        assert_eq!(collections(), before);
        // A tagged std declaration does, once, whether or not the index is requested.
        let module =
            crate::analyze_modules_with_std_all(&[("Main.tz", "Legacy.old")], &std).unwrap();
        assert_eq!(module.warnings.len(), 1);
        assert_eq!(collections(), before + 1);
        let inputs = [
            input("Main.tz", "Legacy.old", user, None),
            input("std/Legacy.tz", tagged, ModuleOrigin::Std, None),
        ];
        let mut index = semantic::SemanticIndex::default();
        let module = crate::analyze_inputs_semantic(&inputs, &mut index).unwrap();
        assert_eq!(module.warnings.len(), 1);
        assert!(!index.definitions.is_empty());
        assert_eq!(collections(), before + 2);
        // Without a tag, a requested index is still collected once.
        let plain = [input("Main.tz", "1", user, None)];
        let mut index = semantic::SemanticIndex::default();
        crate::analyze_inputs_semantic(&plain, &mut index).unwrap();
        assert_eq!(collections(), before + 3);
        // A tag in the root package never collects, even when a dependency exists: nothing
        // depends on the root package.
        let library = PackageId {
            name: "library".into(),
            root: "/library".into(),
        };
        let inputs = [
            input("Main.tz", "Old.old", user, None),
            input("Old.tz", tagged, user, None),
            input(
                "Library/Plain.tz",
                "def plain :: i64 = 1\n",
                user,
                Some(&library),
            ),
        ];
        let module = crate::analyze_inputs_all(&inputs).unwrap();
        assert!(module.warnings.is_empty(), "{:?}", module.warnings);
        assert_eq!(collections(), before + 3);
        // A tag in a dependency does, once.
        let inputs = [
            input("Main.tz", "Library::Old.old", user, None),
            input("Library/Old.tz", tagged, user, Some(&library)),
        ];
        let module = crate::analyze_inputs_all(&inputs).unwrap();
        assert_eq!(module.warnings.len(), 1, "{:?}", module.warnings);
        assert_eq!(collections(), before + 4);
    }

    /// PB08 O1: the private reachability of `unused_private` looks up every type name of every
    /// function, std included; a successful check builds no suggestion pool for the names that
    /// are not declared types (`i64`, `string`, type variables of std's signatures).
    #[test]
    fn a_successful_check_builds_no_type_suggestions() {
        let pools = || crate::check::find::TYPE_POOLS.with(std::cell::Cell::get);
        let before = pools();
        crate::analyze_modules(&[
            (
                "Main.tz",
                "record Point { x: i64, label: string }\ntype Meters = f64\nprivate record Kept { value: Meters }\n\
                def length :: Point -> i64 = \\point -> point.x + Geometry.area (Geometry.Circle { radius: 2 })\n\
                let kept = Kept { value: 1.5 }\nlet total: Maybe<i64> = Some 1\nlet extra = match total with\n    | Some value -> value\n    | None -> 0\nlength (Point { x: 1, label: \"a\" }) + extra",
            ),
            (
                "Geometry.tz",
                "record Circle { radius: i64 }\ndef area :: Circle -> i64 = \\circle -> circle.radius * circle.radius\n",
            ),
        ])
        .unwrap();
        assert_eq!(
            pools(),
            before,
            "a successful check built a type suggestion pool"
        );
        // A reported unknown type still has its suggestions.
        let error = crate::analyze("def f :: Pointt -> i64 = \\p -> 1\nrecord Point { x: i64 }\n0")
            .unwrap_err();
        assert_eq!(error.code, "E1004", "{error:?}");
        assert!(pools() > before);
    }

    /// PB08 O6: the names that `find::OptInNames` reads from the opt-in std sources are those that
    /// `Names` holds of the opt-in modules when a check loads every std module: each module, its
    /// public functions, constants and extern functions that are not hidden, with the word of their
    /// candidate signature, the cases of its public unions, and the types that a user module names
    /// bare (`type_choice` finds them, as `type_pool` asks, and they are declared in the module).
    #[test]
    fn opt_in_names_are_the_checker_names() {
        let every = format!(
            "// {}\n0\n",
            crate::stdlib::OPT_IN
                .iter()
                .map(|module| module.module)
                .collect::<Vec<_>>()
                .join(" ")
        );
        let read = super::find::OptInNames::read(crate::stdlib::SOURCES).entries();
        let found = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let into = found.clone();
        with_names(&[("Main.tz", every.as_str())], move |names| {
            let hidden = |name: &str| name.contains('$') || name.contains("__");
            let mut entries = Vec::new();
            for (key, origin) in &names.modules {
                if *origin != ModuleOrigin::Std || !crate::stdlib::is_opt_in(key) {
                    continue;
                }
                entries.push(format!("module {key}"));
                for (name, info) in &names.functions {
                    if info.module == *key && info.visible_from("Main") && !hidden(name) {
                        let word = if names.constants.contains(&info.id) {
                            "constant"
                        } else {
                            "function"
                        };
                        entries.push(format!("function {name} {word}"));
                    }
                }
                for (name, case) in &names.cases {
                    if case.info.module == *key && case.info.visible_from("Main") {
                        entries.push(format!("case {name}"));
                    }
                }
                let types: BTreeSet<&String> = names
                    .record_aliases
                    .keys()
                    .chain(names.union_aliases.keys())
                    .chain(names.type_alias_names.keys())
                    .chain(names.handle_aliases.keys())
                    .collect();
                for name in types {
                    if let Choice::Found(named, _) = names.type_choice("Main", name)
                        && named.info().module == *key
                    {
                        entries.push(format!("type {name} {}", named.info().name));
                    }
                }
            }
            entries.sort();
            *into.borrow_mut() = entries;
        });
        let found = found.borrow();
        assert_eq!(
            found
                .iter()
                .filter(|entry| entry.starts_with("module "))
                .count(),
            crate::stdlib::OPT_IN.len()
        );
        assert!(found.len() > 300, "{}", found.len());
        assert_eq!(read, *found);
    }

    /// Runs `probe` on the names of the program of `sources`, which must check.
    fn with_names(sources: &[(&str, &str)], probe: impl FnMut(&Names) + 'static) {
        let ran = std::rc::Rc::new(std::cell::Cell::new(false));
        let mut probe = probe;
        let flag = ran.clone();
        NAMES_PROBE.set(Some(Box::new(move |names: &Names| {
            probe(names);
            flag.set(true);
        })));
        let checked = crate::analyze_modules(sources);
        NAMES_PROBE.set(None);
        checked.unwrap_or_else(|error| panic!("{error:?}"));
        assert!(ran.get());
    }

    /// PB08 O1: `quiet_named_type` finds a type exactly where `named_type` returns `Ok`, and the
    /// same type. `expected` is the type, written `kind key-qualified-name`, or `None` for an error.
    fn quiet_lookup_agrees(sources: &[(&str, &str)], cases: &[(&str, &str, Option<&str>)]) {
        let cases: Vec<(String, String, Option<String>)> = cases
            .iter()
            .map(|(module, name, expected)| {
                (
                    (*module).to_owned(),
                    (*name).to_owned(),
                    expected.map(str::to_owned),
                )
            })
            .collect();
        with_names(sources, move |names| {
            let describe = |named: NamedType<'_>| format!("{} {}", named.kind(), named.info().name);
            for (module, name, expected) in &cases {
                let span = Span::default();
                let quiet = names.quiet_named_type(module, name, span).map(describe);
                let named = names.named_type(module, name, span);
                let message = named.as_ref().err().map(|error| error.message.clone());
                assert_eq!(
                    quiet,
                    named.ok().map(describe),
                    "{module}: {name}: {message:?}"
                );
                assert_eq!(quiet, *expected, "{module}: {name}: {message:?}");
            }
        });
    }

    #[test]
    fn quiet_type_lookup_agrees_with_named_type() {
        // Primitive and std types, records, aliases, extern types, private and ambiguous names.
        quiet_lookup_agrees(
            &[
                (
                    "Main.tz",
                    "type Meters = f64\nextern type Handle\nrecord Point { x: i64 }\n\
                    def size :: Point -> Geometry.Circle -> i64 = \\p c -> p.x + c.radius + Secret.value()\n0",
                ),
                ("Geometry.tz", "record Circle { radius: i64 }\n"),
                (
                    "Secret.tz",
                    "private record Hidden { value: i64 }\ndef value :: i64 = 1\n",
                ),
                ("Left.tz", "record Item { left: i64 }\n"),
                ("Right.tz", "record Item { right: i64 }\n"),
            ],
            &[
                ("Main", "i64", None),
                ("Main", "string", None),
                ("Main", "bigint", None),
                ("Main", "Async", None),
                ("Main", "Async.Operation", None),
                ("Main", "Display", None),
                ("Main", "Unknown", None),
                ("Main", "Meters", Some("type alias Main.Meters")),
                ("Main", "Handle", Some("extern type Main.Handle")),
                ("Main", "Point", Some("record Main.Point")),
                ("Main", "Main.Point", Some("record Main.Point")),
                ("Main", "Geometry.Circle", Some("record Geometry.Circle")),
                ("Main", "Circle", Some("record Geometry.Circle")),
                ("Main", "Secret.Hidden", None),
                ("Main", "Hidden", None),
                ("Secret", "Hidden", Some("record Secret.Hidden")),
                ("Main", "Item", None),
                ("Main", "Left.Item", Some("record Left.Item")),
                ("Main", "Maybe", Some("union Maybe.Maybe")),
                ("Main", "Maybe.Maybe", None),
                ("Main", "Result", Some("union Result.Result")),
            ],
        );
        // Namespaces: `::` paths, module-named types, a path that joins a namespace with `.`
        // (`namespace_spelling`) and one that repeats its module's name (`check_module_type`).
        let shape = "namespace Sample\n\nunion Shape =\n    | Circle of f64\n    | Rect of f64 * f64\n\nunion Maybe<'a> = None | Some of 'a\n";
        let point = "namespace Sample\n\nrecord Point { x: f64, y: f64 }\n";
        let other = "namespace Other\n\nrecord Point { x: f64, y: f64 }\n";
        let main = "namespace Sample\n\nlet p = Sample::Point { x: 1.0, y: 2.0 }\np.x\n";
        quiet_lookup_agrees(
            &[
                ("Shape", shape),
                ("Point", point),
                ("Other/Point", other),
                ("Main", main),
            ],
            &[
                ("Sample.Main", "Point", Some("record Sample.Point.Point")),
                (
                    "Sample.Main",
                    "Sample::Point",
                    Some("record Sample.Point.Point"),
                ),
                (
                    "Sample.Main",
                    "Other::Point",
                    Some("record Other.Point.Point"),
                ),
                ("Sample.Main", "Shape", Some("union Sample.Shape.Shape")),
                (
                    "Sample.Main",
                    "Sample::Shape.Maybe",
                    Some("union Sample.Shape.Maybe"),
                ),
                ("Sample.Main", "Sample::Point.Point", None),
                ("Sample.Main", "Sample::Shape.Shape", None),
                ("Sample.Main", "Sample.Point", None),
                ("Sample.Main", "Sample.Shape.Maybe", None),
                ("Sample.Main", "Missing::Point", None),
                ("Sample.Main", "i64", None),
            ],
        );
        // A bare module name that two `using` declarations make ambiguous (`check_module`).
        quiet_lookup_agrees(
            &[
                (
                    "Left/Text",
                    "namespace Left\n\nrecord Text { value: i64 }\n",
                ),
                (
                    "Right/Text",
                    "namespace Right\n\nrecord Text { value: i64 }\n",
                ),
                ("Main", "using Left\nusing Right\n\nlet x = 1\nx\n"),
            ],
            &[
                ("Main", "Text", None),
                ("Main", "Left::Text", Some("record Left.Text.Text")),
                ("Main", "Right::Text", Some("record Right.Text.Text")),
            ],
        );
    }

    #[test]
    fn deprecation_note_reads_the_first_tag_line() {
        let documented = |text: &str| {
            Documentation {
                text: text.to_owned(),
                span: Span::default(),
            }
            .deprecation()
            .map(str::to_owned)
        };
        for (text, expected) in [
            ("Old.\n@deprecated use new", Some("use new")),
            ("@deprecated", Some("")),
            ("  @deprecated  spaced  ", Some("spaced")),
            ("@deprecated\ttabbed", Some("tabbed")),
            ("@deprecated a\n@deprecated b", Some("a")),
            ("@deprecatedX", None),
            ("@deprecatedX\n@deprecated real", Some("real")),
            ("text", None),
            ("see @deprecated here", None),
            ("", None),
        ] {
            assert_eq!(documented(text).as_deref(), expected, "{text:?}");
        }
    }

    #[test]
    fn shadowing_is_opt_in_and_lexically_scoped() {
        let program = crate::parser::parse("let value = 1\nlet value = value + 1\nlet _hidden = 1\nlet _hidden = 2\n{ let value = 3; value }").unwrap();
        let modules = [ModuleInput {
            name: "Main",
            program: &program,
            origin: ModuleOrigin::User,
            namespace: "",
            entry: true,
            edition: crate::syntax::Edition::LATEST,
            package: None,
        }];
        for enabled in [false, true] {
            let module = check_modules_collect(
                &modules,
                &mut Diagnostics::new(0),
                WarningOptions { shadowing: enabled },
                None,
                None,
                crate::parallel::Parallelism::SEQUENTIAL,
                crate::check::contracts::ContractMode::Check,
                None,
            )
            .unwrap();
            let warnings: Vec<_> = module
                .warnings
                .iter()
                .filter(|diagnostic| diagnostic.code == "W1004")
                .collect();
            assert_eq!(
                warnings.len(),
                usize::from(enabled),
                "{:?}",
                module.warnings
            );
        }
    }
}
