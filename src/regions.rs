use super::*;

fn failure(span: Span, message: impl Into<String>) -> Diagnostic {
    Diagnostic::new("E1013", message, span)
}

fn children(expression: &TypeExpr) -> Vec<&TypeExpr> {
    match &expression.kind {
        TypeExprKind::Apply(_, arguments) => arguments.iter().collect(),
        TypeExprKind::Regions(inner, _)
        | TypeExprKind::Reference(inner, _)
        | TypeExprKind::Array(inner)
        | TypeExprKind::ArrayView(inner)
        | TypeExprKind::List(inner)
        | TypeExprKind::Task(inner) => vec![inner],
        TypeExprKind::FixedArray(element, _) => vec![element],
        TypeExprKind::Tuple(elements) => elements.iter().collect(),
        TypeExprKind::Function(parameters, result) => parameters
            .iter()
            .chain(std::iter::once(result.as_ref()))
            .collect(),
        _ => Vec::new(),
    }
}

fn has_regions(expression: &TypeExpr) -> bool {
    let mut pending = vec![expression];
    while let Some(expression) = pending.pop() {
        if matches!(
            expression.kind,
            TypeExprKind::Regions(..) | TypeExprKind::Quantified(..)
        ) {
            return true;
        }
        pending.extend(children(expression));
    }
    false
}

/// `children` stops at quantified function types, whose regions are their own.
fn has_quantifier(expression: &TypeExpr) -> bool {
    let mut pending = vec![expression];
    while let Some(expression) = pending.pop() {
        if matches!(expression.kind, TypeExprKind::Quantified(..)) {
            return true;
        }
        pending.extend(children(expression));
    }
    false
}

fn misplaced_quantifier(span: Span) -> Diagnostic {
    failure(
        span,
        "region-quantified function types are only supported as whole parameter types of a named function",
    )
}

pub(super) fn reject_local(expression: &TypeExpr) -> Result<(), Diagnostic> {
    if has_regions(expression) {
        Err(failure(
            expression.span,
            "named regions belong in function signatures or record declarations; let local borrows be inferred",
        ))
    } else {
        Ok(())
    }
}

fn labels(
    expression: &TypeExpr,
    declared: &BTreeSet<String>,
    module: &str,
    names: &Names,
    types: TypeContext<'_>,
    field: bool,
) -> Result<BTreeSet<String>, Diagnostic> {
    let mut labels = BTreeSet::new();
    let mut pending = vec![expression];
    while let Some(expression) = pending.pop() {
        if matches!(expression.kind, TypeExprKind::Function(..)) && has_regions(expression) {
            return Err(failure(
                expression.span,
                "named regions inside higher-order function types are not supported; use a named function with all parameters bound",
            ));
        }
        if let TypeExprKind::Regions(inner, regions) = &expression.kind {
            // `slot_labels` takes a whole record with several regions; nested ones would mix regions.
            let count = record_regions(inner, module, names, types);
            if count >= 2 {
                return Err(if regions.len() == count {
                    mixing(expression.span, field)
                } else {
                    region_count_mismatch(expression.span, count)
                });
            }
            if regions.len() != 1 {
                return Err(failure(
                    expression.span,
                    "each borrowed value has one named region; split values with independent regions",
                ));
            }
            let ty = resolve_type(inner, module, names)?;
            if !ty.carries_loans(&types) || matches!(ty, Type::Function(..) | Type::Task(_)) {
                return Err(failure(
                    expression.span,
                    "a named region requires a reference or an aggregate carrying borrows",
                ));
            }
            for region in regions {
                if !declared.contains(&region.text) {
                    return Err(undeclared(region));
                }
                labels.insert(region.text.clone());
            }
        }
        pending.extend(children(expression));
    }
    if labels.len() > 1 {
        return Err(mixing(expression.span, field));
    }
    Ok(labels)
}

fn undeclared(region: &Ident) -> Diagnostic {
    failure(
        region.span,
        format!(
            "undeclared region '{}'; add it after the declaration name",
            region.text
        ),
    )
}

fn mixing(span: Span, field: bool) -> Diagnostic {
    failure(
        span,
        if field {
            "one field cannot mix distinct named regions; give each field one region or use a record type with several regions"
        } else {
            "one parameter or result cannot mix distinct named regions; use separate parameters"
        },
    )
}

fn region_count_mismatch(span: Span, count: usize) -> Diagnostic {
    failure(
        span,
        format!(
            "the record declares {count} regions; write exactly {count} region names in declaration order"
        ),
    )
}

/// The region count of the record that `expression` names, or 0 when it names no record.
fn record_regions(
    expression: &TypeExpr,
    module: &str,
    names: &Names,
    types: TypeContext<'_>,
) -> usize {
    match resolve_type(expression, module, names) {
        Ok(Type::Record(id, _)) => types.records[id].region_count,
        _ => 0,
    }
}

/// A13 Phase 2: `ref {r} T {s}` (or `ref mut`) names the region `s` of the borrows inside the
/// data behind the reference apart from the reference's own region `r`. Returns `r` and `T {s}`.
/// With `s` equal to `r` the type has one region, as in A09.
fn target_form(expression: &TypeExpr) -> Option<(&Ident, &TypeExpr)> {
    let TypeExprKind::Regions(reference, regions) = &expression.kind else {
        return None;
    };
    let [region] = &**regions else {
        return None;
    };
    let TypeExprKind::Reference(target, _) = &reference.kind else {
        return None;
    };
    match &target.kind {
        TypeExprKind::Regions(_, inner) if matches!(&**inner, [name] if name.text != region.text) => {
            Some((region, target))
        }
        _ => None,
    }
}

/// The region names of each region slot of a value of the type: one slot per region of a
/// record with several regions applied directly, otherwise one slot. With `targets`, a
/// reference with a named target region has a second slot for the target's borrows (A13).
fn slot_labels(
    expression: &TypeExpr,
    declared: &BTreeSet<String>,
    module: &str,
    names: &Names,
    types: TypeContext<'_>,
    field: bool,
    targets: bool,
) -> Result<Vec<BTreeSet<String>>, Diagnostic> {
    if let TypeExprKind::Regions(inner, regions) = &expression.kind
        && regions.len() >= 2
        && regions.len() == record_regions(inner, module, names, types)
    {
        if has_regions(inner) {
            return Err(mixing(expression.span, field));
        }
        return regions
            .iter()
            .map(|region| {
                if declared.contains(&region.text) {
                    Ok(BTreeSet::from([region.text.clone()]))
                } else {
                    Err(undeclared(region))
                }
            })
            .collect();
    }
    if targets && let Some((region, target)) = target_form(expression) {
        return target_labels(region, target, declared, module, names, types, field);
    }
    Ok(vec![labels(
        expression, declared, module, names, types, field,
    )?])
}

/// The slots of `ref {region} T {s}`: the reference's region, then the region of the target's borrows.
#[inline(never)]
fn target_labels(
    region: &Ident,
    target: &TypeExpr,
    declared: &BTreeSet<String>,
    module: &str,
    names: &Names,
    types: TypeContext<'_>,
    field: bool,
) -> Result<Vec<BTreeSet<String>>, Diagnostic> {
    if !declared.contains(&region.text) {
        return Err(undeclared(region));
    }
    let inner = labels(target, declared, module, names, types, field)?;
    if resolve_type(target, module, names)?.reaches_exclusive(&types) {
        return Err(failure(
            target.span,
            "the data behind a reference with its own target region cannot hold exclusive borrows; use one region, as in 'ref mut {r} T'",
        ));
    }
    Ok(vec![BTreeSet::from([region.text.clone()]), inner])
}

/// The region layout of a record declaration (A12, A13).
pub(super) struct RecordRegions {
    pub count: usize,
    /// With several regions, the region mask of each slot of each field.
    pub fields: Vec<Vec<RegionMask>>,
    /// With several regions, the region of the target's borrows of each field written
    /// `ref {r} T {s}`, and 0 for the other fields.
    pub targets: Vec<RegionMask>,
}

/// The region count of a record and, with several regions, the region mask of each slot of
/// each field (A12). Undeclared names count as every region; `validate_modules` reports them.
/// A field written `ref {r} T {s}` holds loans of `r` only; `s` belongs to its target (A13).
pub(super) fn field_regions(record: &RecordDecl) -> Result<RecordRegions, Diagnostic> {
    let count = record.regions.len();
    if count > MAX_RECORD_REGIONS {
        return Err(Diagnostic::new(
            "E1017",
            format!(
                "too many regions in record '{}'; declare at most {MAX_RECORD_REGIONS} and share one region among borrows with the same lifetime",
                record.name.text
            ),
            record.regions[MAX_RECORD_REGIONS].span,
        ));
    }
    if count < 2 {
        return Ok(RecordRegions {
            count,
            fields: Vec::new(),
            targets: Vec::new(),
        });
    }
    let all = ((1u32 << count) - 1) as RegionMask;
    let bit = |name: &str| {
        record
            .regions
            .iter()
            .position(|region| region.text == name)
            .map_or(all, |index| 1 << index)
    };
    let mut targets = vec![0; record.fields.len()];
    let fields = record
        .fields
        .iter()
        .zip(&mut targets)
        .map(|(field, target)| {
            if let TypeExprKind::Regions(_, names) = &field.ty.kind
                && names.len() >= 2
            {
                return names.iter().map(|name| bit(&name.text)).collect();
            }
            if let Some((region, inner)) = target_form(&field.ty) {
                if let TypeExprKind::Regions(_, names) = &inner.kind {
                    *target = names.iter().fold(0, |mask, name| mask | bit(&name.text));
                }
                return vec![bit(&region.text)];
            }
            let mut mask = 0;
            let mut pending = vec![&field.ty];
            while let Some(expression) = pending.pop() {
                if let TypeExprKind::Regions(_, names) = &expression.kind {
                    mask = names.iter().fold(mask, |mask, name| mask | bit(&name.text));
                }
                pending.extend(children(expression));
            }
            vec![if mask == 0 { all } else { mask }]
        })
        .collect();
    Ok(RecordRegions {
        count,
        fields,
        targets,
    })
}

fn check_used(regions: &[Ident], used: &BTreeSet<String>) -> Result<(), Diagnostic> {
    if let Some(region) = regions.iter().find(|region| !used.contains(&region.text)) {
        return Err(failure(
            region.span,
            format!(
                "unused region '{}'; remove it or use it in a borrowed type",
                region.text
            ),
        ));
    }
    Ok(())
}

/// A13: an exclusive borrow field and a field of a record with exclusive regions name their regions.
fn exclusive_field_regions(
    field: &Parameter,
    ty: &Type,
    types: TypeContext<'_>,
) -> Result<(), Diagnostic> {
    if matches!(field.ty.kind, TypeExprKind::Regions(..)) {
        return Ok(());
    }
    let name = &field.name.text;
    match ty {
        Type::Reference(_, true) => Err(failure(
            field.ty.span,
            format!(
                "exclusive borrow field '{name}' needs a named region; declare one after the record name and write 'ref mut {{r}} T'"
            ),
        )),
        Type::Record(id, _) if types.records[*id].exclusive_regions != 0 => {
            const NAMES: [&str; MAX_RECORD_REGIONS] = [
                "r", "s", "t", "u", "v", "w", "x", "y", "z", "a", "b", "c", "d", "e", "f", "g",
            ];
            let inner = &types.records[*id];
            let record = inner.name.rsplit('.').next().unwrap_or(&inner.name);
            Err(failure(
                field.ty.span,
                format!(
                    "field '{name}' stores an exclusive borrow inside record '{record}'; apply its regions, for example '{record} {{{}}}'",
                    NAMES[..inner.region_count.max(1)].join(" ")
                ),
            ))
        }
        _ => Ok(()),
    }
}

/// A13: one field position uses each exclusive region. In a record with one region, a field that
/// stores a borrow without naming the region still belongs to it (A09).
fn exclusive_positions(
    record: &RecordDecl,
    checked: &CheckedRecord,
    types: TypeContext<'_>,
) -> Result<(), Diagnostic> {
    if checked.exclusive_regions == 0 {
        return Ok(());
    }
    let mut seen: RegionMask = 0;
    for (field, (_, ty)) in record.fields.iter().zip(&checked.fields) {
        let mut regions = Vec::new();
        let mut pending = vec![&field.ty];
        while let Some(expression) = pending.pop() {
            if let TypeExprKind::Regions(_, names) = &expression.kind {
                regions.extend(names.iter().filter_map(|name| {
                    record
                        .regions
                        .iter()
                        .position(|region| region.text == name.text)
                }));
            }
            pending.extend(children(expression));
        }
        if regions.is_empty() && record.regions.len() == 1 && ty.contains_stored_reference(&types) {
            regions.push(0);
        }
        for region in regions {
            let bit: RegionMask = 1 << region;
            if checked.exclusive_regions & bit == 0 {
                continue;
            }
            if seen & bit != 0 {
                return Err(failure(
                    field.ty.span,
                    format!(
                        "region '{}' belongs to an exclusive borrow, so only one field position may use it; give field '{}' its own region",
                        record.regions[region].text, field.name.text
                    ),
                ));
            }
            seen |= bit;
        }
    }
    Ok(())
}

pub(super) fn validate_modules(
    modules: &[ModuleInput<'_>],
    names: &Names,
    types: TypeContext<'_>,
) -> Result<(), Diagnostic> {
    // Records follow the declaration order of `modules`, as `check_modules_collect` built them.
    let mut checked = types.records.iter();
    for module in modules {
        for record in &module.program.records {
            let checked = checked.next().expect("one checked record per declaration");
            let declared = record
                .regions
                .iter()
                .map(|region| region.text.clone())
                .collect();
            let mut used = BTreeSet::new();
            for (field, (_, ty)) in record.fields.iter().zip(&checked.fields) {
                exclusive_field_regions(field, ty, types)?;
                if has_quantifier(&field.ty) {
                    return Err(misplaced_quantifier(field.ty.span));
                }
                let found: BTreeSet<_> =
                    slot_labels(&field.ty, &declared, module.name, names, types, true, true)?
                        .into_iter()
                        .flatten()
                        .collect();
                if record.regions.len() >= 2
                    && found.is_empty()
                    && resolve_type(&field.ty, module.name, names)?
                        .contains_stored_reference(&types)
                {
                    return Err(failure(
                        field.ty.span,
                        format!(
                            "field '{}' stores a borrow without a region; name one of the record's regions in its type, for example 'ref {{r}} T'",
                            field.name.text
                        ),
                    ));
                }
                used.extend(found);
            }
            check_used(&record.regions, &used)?;
            exclusive_positions(record, checked, types)?;
        }
        for alias in &module.program.type_aliases {
            reject_local(&alias.target)?;
        }
        for constant in &module.program.constants {
            reject_local(&constant.ty)?;
        }
        for union in &module.program.unions {
            for case in &union.cases {
                if let Some(payload) = &case.payload {
                    reject_local(payload)?;
                }
            }
        }
        for class in &module.program.classes {
            for method in &class.methods {
                for parameter in &method.parameters {
                    reject_local(parameter)?;
                }
                reject_local(&method.result)?;
            }
        }
    }
    Ok(())
}

/// The region contracts of a named function: its named result's sources and its parameters with
/// region-quantified function types (A12), and the regions of its parameters' slots (A13).
pub(super) struct Contracts {
    pub result: Option<RegionSources>,
    pub callbacks: Vec<CallbackContract>,
    pub slots: Option<RegionSlots>,
}

pub(super) fn contract(
    function: &FunctionDecl,
    module: &str,
    names: &Names,
    types: TypeContext<'_>,
) -> Result<Contracts, Diagnostic> {
    let declared: BTreeSet<_> = function
        .regions
        .iter()
        .map(|region| region.text.clone())
        .collect();
    let mut used = BTreeSet::new();
    let mut inputs = Vec::new();
    let mut callbacks = Vec::new();
    for (index, parameter) in function.parameters.iter().enumerate() {
        if let TypeExprKind::Quantified(regions, inner) = &parameter.ty.kind {
            if parameter.mutable {
                return Err(failure(
                    parameter.name.span,
                    "a parameter with a region-quantified function type cannot be mutable",
                ));
            }
            callbacks.push(callback_contract(
                index, regions, inner, &declared, module, names, types,
            )?);
            inputs.push(vec![BTreeSet::new()]);
            continue;
        }
        if has_quantifier(&parameter.ty) {
            return Err(misplaced_quantifier(parameter.ty.span));
        }
        let input = slot_labels(&parameter.ty, &declared, module, names, types, false, true)?;
        used.extend(input.iter().flatten().cloned());
        inputs.push(input);
    }
    if has_quantifier(&function.result) {
        return Err(misplaced_quantifier(function.result.span));
    }
    let result = slot_labels(
        &function.result,
        &declared,
        module,
        names,
        types,
        false,
        true,
    )?;
    used.extend(result.iter().flatten().cloned());
    check_used(&function.regions, &used)?;
    let slots = (!function.regions.is_empty())
        .then(|| region_slots(function, module, names, types, &inputs))
        .transpose()?;
    Ok(Contracts {
        result: result_sources(&function.result, &inputs, &result)?,
        callbacks,
        slots,
    })
}

/// The declared region of each slot of each parameter, and which parameters are references with
/// a named target region (A13 Phase 2).
#[inline(never)]
fn region_slots(
    function: &FunctionDecl,
    module: &str,
    names: &Names,
    types: TypeContext<'_>,
    inputs: &[Vec<BTreeSet<String>>],
) -> Result<RegionSlots, Diagnostic> {
    let region = |labels: &BTreeSet<String>| {
        labels.first().and_then(|name| {
            function
                .regions
                .iter()
                .position(|region| region.text == *name)
        })
    };
    let parameters: Vec<Vec<Option<usize>>> = inputs
        .iter()
        .map(|slots| slots.iter().map(region).collect())
        .collect();
    let targets: Vec<bool> = function
        .parameters
        .iter()
        .map(|parameter| target_form(&parameter.ty).is_some())
        .collect();
    let mut slots = RegionSlots {
        parameters,
        targets,
        result_target: target_form(&function.result).is_some(),
        writes: false,
    };
    // A call may store any input with a written target's region in that target.
    for (index, parameter) in function.parameters.iter().enumerate() {
        if matches!(parameter.ty.kind, TypeExprKind::Quantified(..)) {
            continue;
        }
        let ty = resolve_type(&parameter.ty, module, names)?;
        for (_, contents) in slots.written_targets(index, &ty, types) {
            slots.writes |= slots.parameters.iter().enumerate().any(|(input, regions)| {
                regions.iter().enumerate().any(|(slot, region)| {
                    region.is_some_and(|region| contents.contains(&region))
                        && !(input == index && slots.targets[index] && slot == 1)
                })
            });
        }
    }
    Ok(slots)
}

/// The input slots that each slot of a wholly annotated borrowed result may borrow from.
fn result_sources(
    expression: &TypeExpr,
    inputs: &[Vec<BTreeSet<String>>],
    result: &[BTreeSet<String>],
) -> Result<Option<RegionSources>, Diagnostic> {
    if result.iter().all(BTreeSet::is_empty) {
        return Ok(None);
    }
    if !matches!(expression.kind, TypeExprKind::Regions(..)) {
        return Err(failure(
            expression.span,
            "annotate the whole borrowed result with one region, for example 'View {r}' or 'ref {r} T'",
        ));
    }
    let mut sources = Vec::new();
    for slot in result {
        let region = slot
            .first()
            .expect("each slot of a wholly annotated result names one region");
        let pairs: BTreeSet<_> = inputs
            .iter()
            .enumerate()
            .flat_map(|(index, input)| {
                input
                    .iter()
                    .enumerate()
                    .filter(|(_, labels)| labels.contains(region))
                    .map(move |(slot, _)| (index, slot))
            })
            .collect();
        if pairs.is_empty() {
            return Err(failure(
                expression.span,
                format!(
                    "result region '{region}' has no matching input; bind all function parameters and name an input region"
                ),
            ));
        }
        sources.push(pairs);
    }
    Ok(Some(sources))
}

/// The contract of the parameter `parameter` with the type `{regions} inner` (A12 Phase 2).
fn callback_contract(
    parameter: usize,
    regions: &[Ident],
    inner: &TypeExpr,
    outer: &BTreeSet<String>,
    module: &str,
    names: &Names,
    types: TypeContext<'_>,
) -> Result<CallbackContract, Diagnostic> {
    let TypeExprKind::Function(parameters, result) = &inner.kind else {
        return Err(failure(
            inner.span,
            "a region quantifier applies to a function type, as in '{r} ref {r} T -> ref {r} T'",
        ));
    };
    if let Some(region) = regions.iter().find(|region| outer.contains(&region.text)) {
        return Err(failure(
            region.span,
            format!(
                "region '{}' is already declared by the function; give the quantified region another name",
                region.text
            ),
        ));
    }
    let mut pending = vec![inner];
    while let Some(expression) = pending.pop() {
        if let TypeExprKind::Regions(_, labels) = &expression.kind
            && let Some(region) = labels.iter().find(|region| outer.contains(&region.text))
        {
            return Err(failure(
                region.span,
                format!(
                    "region '{}' belongs to the function, not to the quantified function type; quantify a region of its own",
                    region.text
                ),
            ));
        }
        pending.extend(children(expression));
    }
    let declared: BTreeSet<_> = regions.iter().map(|region| region.text.clone()).collect();
    let mut used = BTreeSet::new();
    let mut inputs = Vec::new();
    for input in parameters.iter().chain([result.as_ref()]) {
        if has_quantifier(input) {
            return Err(misplaced_quantifier(input.span));
        }
    }
    for input in parameters {
        let labels = slot_labels(input, &declared, module, names, types, false, false)?;
        used.extend(labels.iter().flatten().cloned());
        inputs.push(labels);
    }
    let output = slot_labels(result, &declared, module, names, types, false, false)?;
    used.extend(output.iter().flatten().cloned());
    check_used(regions, &used)?;
    let sources = match result_sources(result, &inputs, &output)? {
        Some(sources) => sources,
        None if resolve_type(result, module, names)?.carries_loans(&types) => {
            return Err(failure(
                result.span,
                "annotate the whole borrowed result with one region, for example 'View {r}' or 'ref {r} T'",
            ));
        }
        None => Vec::new(),
    };
    Ok(CallbackContract {
        parameter,
        arity: parameters.len(),
        sources,
    })
}

/// A function whose parameters have region-quantified types runs only in direct calls with
/// every parameter, so each callback argument is checked against its contract. So does a
/// function that may store an input through an exclusive reference (A13 Phase 2): its callers
/// keep the stored loans, which a function value would lose.
pub(super) fn validate_contract_calls(module: &CheckedModule) -> Result<(), Diagnostic> {
    let writes = |id: usize| {
        module.functions[id]
            .region_slots
            .as_ref()
            .is_some_and(|slots| slots.writes)
    };
    let contracted = |id: usize| !module.functions[id].callback_contracts.is_empty() || writes(id);
    if !(0..module.functions.len()).any(contracted) {
        return Ok(());
    }
    let target = |expression: &TypedExpr| match expression.kind {
        TypedExprKind::Function(FunctionRef::User(id)) | TypedExprKind::GenericFunction(id, _)
            if contracted(id) =>
        {
            Some(id)
        }
        _ => None,
    };
    let direct_only = |id: usize, span: Span| {
        let name = &module.functions[id].name;
        failure(
            span,
            if module.functions[id].callback_contracts.is_empty() {
                format!(
                    "'{name}' may store an input through an exclusive reference, so call it directly with all of its arguments"
                )
            } else {
                format!(
                    "'{name}' takes a function with a region-quantified type, so call it directly with all of its arguments"
                )
            },
        )
    };
    for function in &module.functions {
        let mut pending = vec![&function.body];
        while let Some(expression) = pending.pop() {
            if let TypedExprKind::Call(callee, arguments) = &expression.kind
                && let Some(id) = target(callee)
            {
                if arguments.len() < module.functions[id].parameters.len() {
                    return Err(direct_only(id, expression.span));
                }
                pending.extend(arguments);
                continue;
            }
            if let Some(id) = target(expression) {
                return Err(direct_only(id, expression.span));
            }
            pending.extend(expression.children());
        }
    }
    Ok(())
}
