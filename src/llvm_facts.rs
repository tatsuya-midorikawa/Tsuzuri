//! Conservative facts for internal functions, not the public ABI or closure adapters (PR01).
use super::*;

const BUDGET: usize = 200_000;

#[derive(Default)]
pub(super) struct Metadata {
    tbaa: BTreeMap<String, usize>,
    nonnull: Option<usize>,
}

pub(super) struct Facts {
    pub parameters: Vec<String>,
    pub result: &'static str,
    pub memory: String,
}

fn scalar(ty: &Type) -> bool {
    matches!(
        ty,
        Type::Integer(..)
            | Type::Binary(_)
            | Type::Decimal(_)
            | Type::Bool
            | Type::Unit
            | Type::Char
            | Type::Utf8Char
    )
}

fn hidden(ty: &Type, module: &CheckedModule) -> bool {
    matches!(
        ty,
        Type::Shared(..) | Type::Handle(_) | Type::Function(..) | Type::Task(_) | Type::Dyn(_)
    ) || matches!(ty, Type::Record(id, _) if module.records[*id].opaque())
}

fn parameter_facts(function: &CheckedFunction, module: &CheckedModule) -> Vec<String> {
    let types = module.types();
    let mut captures = None;
    function
        .parameters
        .iter()
        .map(|parameter| {
            let ty = &parameter.ty;
            if scalar(ty) {
                return " noundef".into();
            }
            let Type::Reference(target, mutable) = ty else {
                return String::new();
            };
            if ty.slice_element().is_some() {
                return String::new();
            }
            let (size, align) = layout_bound(target, module, true);
            if size == 0 {
                return String::new();
            }
            // Stored references and hidden shared state are not unique just because their slot is.
            let isolated =
                !target.carries_loans(&types) && !types.reaches(target, |ty| hidden(ty, module));
            let frozen = target.is_frozen(&types);
            let mut attributes = String::new();
            if isolated && (*mutable || frozen) {
                attributes.push_str(" noalias");
            }
            if !*captures.get_or_insert_with(|| {
                function.signature.result.carries_loans(&types)
                    || function.parameters.iter().any(|parameter| {
                        !scalar(&parameter.ty)
                            && types.reaches(&parameter.ty, |ty| {
                                hidden(ty, module)
                                    || matches!(ty, Type::Reference(target, true) if target.carries_loans(&types))
                            })
                    })
            }) {
                attributes.push_str(" nocapture");
            }
            attributes.push_str(" noundef nonnull");
            if !mutable && frozen {
                attributes.push_str(" readonly");
            }
            let _ = write!(attributes, " align {align} dereferenceable({size})");
            attributes
        })
        .collect()
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Origin {
    Stack,
    Argument(usize),
    Buffer(usize),
    Allocation,
    Descriptor(usize),
}

struct Root {
    storage: String,
    exclusive: bool,
}

fn instruction(line: &str) -> (Option<&str>, &str) {
    let line = line.trim();
    let line = if line.contains('!') {
        line.split(", !").next().unwrap()
    } else {
        line
    };
    let Some((value, rhs)) = line.split_once('=') else {
        return (None, line);
    };
    if let Some(value) = value.strip_suffix(' ')
        && let Some(rhs) = rhs.strip_prefix(' ')
    {
        return (Some(value), rhs);
    }
    match line.split_once(" = ") {
        Some((value, rhs)) => (Some(value), rhs),
        None => (None, line),
    }
}

fn operand(text: &str) -> &str {
    text.split([',', ' ', ')', ']']).next().unwrap_or("")
}

/// Only the emitted simple load/store grammar; atomics, aggregates with inline types, and
/// unfamiliar instructions fail closed instead of acquiring a guessed access type.
fn access(rhs: &str) -> Option<(&str, &str, bool)> {
    if let Some(load) = rhs.strip_prefix("load ") {
        let (ty, address) = load.split_once(", ptr ")?;
        return (!ty.contains(' ')).then(|| (ty, operand(address), false));
    }
    let store = rhs.strip_prefix("store ")?;
    let (value, address) = store.split_once(", ptr ")?;
    let (ty, _) = value.split_once(' ')?;
    Some((ty, operand(address), true))
}

fn memory_free_call(rhs: &str) -> bool {
    if !rhs.contains("call ") {
        return false;
    }
    let Some((_, name)) = rhs.split_once('@') else {
        return false;
    };
    let name = name.split('(').next().unwrap_or("");
    // Do not classify constrained FP, memory, pointer, target-specific or experimental intrinsics.
    [
        "llvm.sqrt.",
        "llvm.fma.",
        "llvm.fabs.",
        "llvm.copysign.",
        "llvm.floor.",
        "llvm.ceil.",
        "llvm.trunc.",
        "llvm.roundeven.",
        "llvm.ctpop.",
        "llvm.bswap.",
        "llvm.bitreverse.",
        "llvm.sadd.with.overflow.",
        "llvm.uadd.with.overflow.",
        "llvm.ssub.with.overflow.",
        "llvm.usub.with.overflow.",
        "llvm.smul.with.overflow.",
        "llvm.umul.with.overflow.",
        "llvm.fptosi.sat.",
        "llvm.fptoui.sat.",
    ]
    .iter()
    .any(|prefix| name.starts_with(prefix))
}

fn nonmemory_instruction(rhs: &str) -> bool {
    if rhs.ends_with(':') || rhs == "]" {
        return true;
    }
    let opcode = rhs.split_whitespace().next().unwrap_or("");
    matches!(
        opcode,
        "ret"
            | "br"
            | "switch"
            | "unreachable"
            | "phi"
            | "select"
            | "add"
            | "sub"
            | "mul"
            | "udiv"
            | "sdiv"
            | "urem"
            | "srem"
            | "shl"
            | "lshr"
            | "ashr"
            | "and"
            | "or"
            | "xor"
            | "fneg"
            | "fadd"
            | "fsub"
            | "fmul"
            | "fdiv"
            | "frem"
            | "icmp"
            | "fcmp"
            | "trunc"
            | "zext"
            | "sext"
            | "fptrunc"
            | "fpext"
            | "fptoui"
            | "fptosi"
            | "uitofp"
            | "sitofp"
            | "ptrtoint"
            | "inttoptr"
            | "bitcast"
            | "getelementptr"
            | "alloca"
            | "extractvalue"
            | "insertvalue"
            | "extractelement"
            | "insertelement"
            | "shufflevector"
    ) || (opcode
        .strip_prefix('i')
        .is_some_and(|bits| bits.parse::<u16>().is_ok())
        && rhs.contains(", label %"))
}

fn metadata(globals: &mut Globals, content: impl AsRef<str>) -> usize {
    let id = globals.next_metadata;
    globals.next_metadata += 1;
    globals
        .definitions
        .push(format!("!{id} = {}", content.as_ref()));
    id
}

fn tbaa(globals: &mut Globals, storage: &str) -> usize {
    if let Some(id) = globals.facts.tbaa.get(storage) {
        return *id;
    }
    let byte = if let Some(id) = globals.facts.tbaa.get("byte") {
        *id
    } else {
        let root = metadata(globals, "!{!\"Tsuzuri scalar storage\"}");
        let byte = metadata(
            globals,
            format!("!{{!\"omnipotent byte\", !{root}, i64 0}}"),
        );
        globals.facts.tbaa.insert("byte".into(), byte);
        byte
    };
    let base = metadata(globals, format!("!{{!\"{storage}\", !{byte}, i64 0}}"));
    let tag = metadata(globals, format!("!{{!{base}, !{base}, i64 0}}"));
    globals.facts.tbaa.insert(storage.into(), tag);
    tag
}

fn tbaa_scalar(storage: &str) -> bool {
    // Byte accesses remain untagged and alias every typed access. Signedness and decimal/binary
    // encodings sharing an LLVM integer representation intentionally share their node.
    matches!(storage, "i16" | "i32" | "i64" | "i128" | "float" | "double")
}

fn scopes(
    roots: &BTreeMap<usize, Root>,
    globals: &mut Globals,
) -> BTreeMap<usize, (usize, Option<usize>)> {
    if roots.len() < 2 || !roots.values().any(|root| root.exclusive) {
        return BTreeMap::new();
    }
    let domain = globals.next_metadata;
    let domain = metadata(
        globals,
        format!("distinct !{{!{domain}, !\"Tsuzuri loans\"}}"),
    );
    let mut nodes = BTreeMap::new();
    for index in roots.keys() {
        let node = globals.next_metadata;
        let node = metadata(
            globals,
            format!("distinct !{{!{node}, !{domain}, !\"argument {index}\"}}"),
        );
        nodes.insert(*index, node);
    }
    roots
        .iter()
        .map(|(index, root)| {
            let own = metadata(globals, format!("!{{!{}}}", nodes[index]));
            let others = roots
                .iter()
                .filter(|(other, other_root)| {
                    *other != index && (root.exclusive || other_root.exclusive)
                })
                .map(|(other, _)| format!("!{}", nodes[other]))
                .collect::<Vec<_>>();
            let other = (!others.is_empty())
                .then(|| metadata(globals, format!("!{{{}}}", others.join(", "))));
            (*index, (own, other))
        })
        .collect()
}

/// A small, closed SSA grammar over the emitter's own instructions. In particular a loaded
/// pointer is NOT based on the pointer it was loaded through. Only a private single-store spill
/// forwards an already-proven value; arbitrary loads, phi/select and pointer casts do not.
fn provenance(
    function: &CheckedFunction,
    module: &CheckedModule,
    allocas: &[String],
    instructions: &[(Option<&str>, &str)],
) -> (BTreeMap<String, Origin>, BTreeMap<usize, Root>) {
    let mut values = BTreeMap::new();
    let mut roots = BTreeMap::new();
    for (index, parameter) in function.parameters.iter().enumerate() {
        let ty = &parameter.ty;
        let Type::Reference(target, exclusive) = ty else {
            continue;
        };
        let (storage, origin) = if let Some(element) = ty.slice_element() {
            if !scalar(element) {
                continue;
            }
            (llvm_type(element, module), Origin::Descriptor(index))
        } else if scalar(target) {
            (llvm_type(target, module), Origin::Argument(index))
        } else {
            continue;
        };
        roots.insert(
            index,
            Root {
                storage,
                exclusive: *exclusive,
            },
        );
        values.insert(format!("%p{index}"), origin);
    }
    let mut slots = BTreeSet::new();
    for line in allocas {
        let (Some(value), rhs) = instruction(line) else {
            continue;
        };
        if rhs.starts_with("alloca ") {
            slots.insert(value.to_owned());
            values.insert(value.to_owned(), Origin::Stack);
        }
    }
    let mut stored = BTreeMap::<String, Option<String>>::new();
    let mut exposed = BTreeSet::new();
    for &(_, rhs) in instructions {
        let memory = access(rhs);
        if let Some((_, address, true)) = memory
            && slots.contains(address)
        {
            let source = rhs
                .strip_prefix("store ")
                .and_then(|text| text.split_once(' '))
                .map(|(_, value)| operand(value).to_owned());
            stored
                .entry(address.to_owned())
                .and_modify(|value| *value = None)
                .or_insert(source);
        }
        // A slot used anywhere except as the direct address of a load/store can be aliased.
        for token in
            rhs.split(|c: char| c.is_whitespace() || matches!(c, ',' | '(' | ')' | '[' | ']'))
        {
            if token.starts_with('%')
                && slots.contains(token)
                && !memory.is_some_and(|(_, address, _)| address == token)
            {
                exposed.insert(token.to_owned());
            }
        }
    }
    stored
        .retain(|slot, source| slots.contains(slot) && !exposed.contains(slot) && source.is_some());
    let candidates: Vec<_> = instructions
        .iter()
        .copied()
        .filter(|(value, rhs)| {
            value.is_some()
                && (rhs.starts_with("call ptr @tz.alloc(")
                    || rhs.starts_with("getelementptr inbounds ")
                    || rhs.starts_with("extractvalue %tz.array ")
                    || rhs.starts_with("insertvalue %tz.array ")
                    || access(rhs)
                        .is_some_and(|(_, address, store)| !store && stored.contains_key(address)))
        })
        .collect();
    for _ in 0..8 {
        let previous = values.len();
        for &(value, rhs) in &candidates {
            let value = value.unwrap();
            if values.contains_key(value) {
                continue;
            }
            let origin = if rhs.starts_with("call ptr @tz.alloc(") {
                Some(Origin::Allocation)
            } else if let Some(gep) = rhs.strip_prefix("getelementptr inbounds ") {
                gep.split_once(", ptr ")
                    .and_then(|(_, pointer)| values.get(operand(pointer)).copied())
                    .filter(|origin| !matches!(origin, Origin::Descriptor(_)))
            } else if let Some(extract) = rhs.strip_prefix("extractvalue %tz.array ") {
                extract.split_once(", ").and_then(|(descriptor, index)| {
                    if index != "0" {
                        return None;
                    }
                    match values.get(descriptor) {
                        Some(Origin::Descriptor(root)) => Some(Origin::Buffer(*root)),
                        _ => None,
                    }
                })
            } else if let Some(insert) = rhs.strip_prefix("insertvalue %tz.array ") {
                insert.split_once(", ").and_then(|(descriptor, field)| {
                    if let Some(data) = field.strip_prefix("ptr ")
                        && let Some((data, "0")) = data.split_once(", ")
                        && let Some(Origin::Buffer(root)) = values.get(data)
                    {
                        return Some(Origin::Descriptor(*root));
                    }
                    if field.starts_with("i64 ") && field.ends_with(", 1") {
                        return values
                            .get(descriptor)
                            .copied()
                            .filter(|origin| matches!(origin, Origin::Descriptor(_)));
                    }
                    None
                })
            } else if let Some((_, address, false)) = access(rhs) {
                stored
                    .get(address)
                    .and_then(|source| source.as_ref())
                    .and_then(|source| values.get(source).copied())
            } else {
                None
            };
            if let Some(origin) = origin {
                values.insert(value.to_owned(), origin);
            }
        }
        if values.len() == previous {
            break;
        }
    }
    (values, roots)
}

fn memory_summarizable(instructions: &[(Option<&str>, &str)]) -> bool {
    instructions.iter().all(|&(_, rhs)| {
        // LLVM may discard an unused call described as memory-free, including at WASM -O0.
        // Language traps and termination are observable. Do not summarize branching bodies
        // without a separate totality proof, and never classify llvm.trap as a pure intrinsic.
        if rhs.starts_with("br ") || rhs.starts_with("switch ") || rhs == "unreachable" {
            return false;
        }
        if rhs.contains("call ") && !memory_free_call(rhs) {
            return false;
        }
        if [
            "atomicrmw ",
            "cmpxchg ",
            "fence ",
            "load atomic ",
            "load volatile ",
            "store atomic ",
            "store volatile ",
        ]
        .iter()
        .any(|word| rhs.starts_with(word))
        {
            return false;
        }
        access(rhs).is_some()
            || (!rhs.starts_with("load ")
                && !rhs.starts_with("store ")
                && (rhs.contains("call ") || nonmemory_instruction(rhs)))
    })
}

fn memory_effects(
    instructions: &[(Option<&str>, &str)],
    values: &BTreeMap<String, Origin>,
) -> String {
    let (mut read, mut write) = (false, false);
    for &(_, rhs) in instructions {
        if let Some((_, address, storing)) = access(rhs) {
            match values.get(address) {
                Some(Origin::Stack) => {}
                Some(Origin::Argument(_)) => {
                    read |= !storing;
                    write |= storing;
                }
                _ => return String::new(),
            }
        }
    }
    match (read, write) {
        (false, false) => " memory(none)",
        (true, false) => " memory(argmem: read)",
        (false, true) => " memory(argmem: write)",
        (true, true) => " memory(argmem: readwrite)",
    }
    .into()
}

fn defined_result(function: &CheckedFunction, instructions: &[(Option<&str>, &str)]) -> bool {
    scalar(&function.signature.result)
        && function.parameters.iter().all(|parameter| {
            scalar(&parameter.ty)
                || matches!(&parameter.ty, Type::Reference(target, _) if scalar(target))
        })
        && instructions.iter().all(|&(_, rhs)| {
            (access(rhs).is_some() || nonmemory_instruction(rhs) || memory_free_call(rhs))
                && (!rhs.contains("call ") || memory_free_call(rhs))
                && !rhs
                    .split(|c: char| !c.is_alphanumeric())
                    .any(|word| matches!(word, "poison" | "undef" | "nsw" | "nuw" | "exact"))
                && ![
                    "sdiv ",
                    "udiv ",
                    "srem ",
                    "urem ",
                    "shl ",
                    "lshr ",
                    "ashr ",
                    "fptosi ",
                    "fptoui ",
                    "extractelement ",
                    "shufflevector ",
                ]
                .iter()
                .any(|opcode| rhs.starts_with(opcode))
        })
}

/// These two lifted builtins access only their scalar slice (plus disjoint trap state). Their
/// call-site scopes follow the accesses when LLVM inlines the wrapper and the builtin. Never
/// extend this to element Drop, callbacks or an arbitrary function with a similar signature.
fn scalar_slice_call(
    rhs: &str,
    module: &CheckedModule,
    values: &BTreeMap<String, Origin>,
) -> Option<usize> {
    if !rhs.contains('@') {
        return None;
    }
    let (_, call) = rhs.split_once(" @tz.fn.$builtin.")?;
    let (name, arguments) = call.split_once('(')?;
    let id = name.rsplit_once('.')?.1.parse::<usize>().ok()?;
    let function = module.functions.get(id)?;
    if function.module != "$builtin" || function.name != name {
        return None;
    }
    let TypedExprKind::Call(callee, _) = &function.body.kind else {
        return None;
    };
    let TypedExprKind::Function(FunctionRef::Builtin(instance)) = &callee.kind else {
        return None;
    };
    if !matches!(instance.builtin, Builtin::ArrayWrite | Builtin::ArraySwapIn)
        || !function.signature.parameters[0]
            .slice_element()
            .is_some_and(scalar)
    {
        return None;
    }
    let descriptor = operand(arguments.strip_prefix("%tz.array ")?);
    match values.get(descriptor) {
        Some(Origin::Descriptor(root)) => Some(*root),
        _ => None,
    }
}

pub(super) fn apply(
    module: &CheckedModule,
    function: &CheckedFunction,
    allocas: &[String],
    lines: &mut Vec<String>,
    single_entry: bool,
    globals: &mut Globals,
    intrinsics: &mut BTreeSet<String>,
) -> Facts {
    let mut facts = Facts {
        parameters: vec![String::new(); function.parameters.len()],
        result: "",
        memory: String::new(),
    };
    if function.capture_count != 0 || function.is_task {
        return facts;
    }
    facts.parameters = parameter_facts(function, module);
    if !single_entry {
        // Updated self-tail phis never use the entry arguments' pointer provenance.
        return facts;
    }
    // A larger or unfamiliar body simply has fewer optimization facts, not different semantics.
    if lines
        .iter()
        .try_fold(BUDGET, |remaining, line| {
            remaining.checked_sub(line.len().saturating_add(16))
        })
        .is_none()
        || function.parameters.len() > 64
        || lines.iter().any(|line| line.contains('\n'))
    {
        return facts;
    }
    let instructions: Vec<_> = lines.iter().map(|line| instruction(line)).collect();
    if defined_result(function, &instructions) {
        facts.result = "noundef ";
    }
    let summarize_memory = memory_summarizable(&instructions);
    let has_roots = function.parameters.iter().any(|parameter| {
        matches!(&parameter.ty, Type::Reference(target, _) if scalar(target)
            || parameter.ty.slice_element().is_some_and(scalar))
    });
    let metadata_needed = has_roots
        || instructions
            .iter()
            .any(|&(_, rhs)| rhs.starts_with("call ptr @tz.alloc("));
    // Without a loan root or allocation there is no access metadata to emit. A body
    // already excluded from memory summaries needs no pointer/spill analysis either.
    if !metadata_needed && !summarize_memory {
        return facts;
    }
    let (values, roots) = provenance(function, module, allocas, &instructions);
    if summarize_memory {
        facts.memory = memory_effects(&instructions, &values);
    }
    if !metadata_needed {
        return facts;
    }
    drop(instructions);
    let callbacks = !roots.is_empty()
        && function.parameters.iter().any(|parameter| {
            !scalar(&parameter.ty)
                && module
                    .types()
                    .reaches(&parameter.ty, |ty| hidden(ty, module))
        });
    let scope = if callbacks {
        BTreeMap::new()
    } else {
        scopes(&roots, globals)
    };
    for line in lines.iter_mut() {
        let (value, rhs) = instruction(line);
        let mut suffix = String::new();
        let call_root = (!scope.is_empty())
            .then(|| scalar_slice_call(rhs, module, &values))
            .flatten();
        let allocation = rhs.starts_with("call ptr @tz.alloc(");
        if let Some(root) = call_root
            && let Some((own, other)) = scope.get(&root)
        {
            let _ = write!(suffix, ", !alias.scope !{own}");
            if let Some(other) = other {
                let _ = write!(suffix, ", !noalias !{other}");
            }
        }
        if let Some((ty, address, store)) = access(rhs)
            && let Some(origin) = values.get(address)
        {
            if let Origin::Argument(root) | Origin::Buffer(root) = origin
                && !callbacks
            {
                if let Some((own, other)) = scope.get(root) {
                    let _ = write!(suffix, ", !alias.scope !{own}");
                    if let Some(other) = other {
                        let _ = write!(suffix, ", !noalias !{other}");
                    }
                }
                if roots[root].storage == ty && tbaa_scalar(ty) {
                    let tag = tbaa(globals, ty);
                    let _ = write!(suffix, ", !tbaa !{tag}");
                }
            }
            if !store
                && ty == "ptr"
                && let Some(value) = value
                && matches!(
                    values.get(value),
                    Some(Origin::Allocation | Origin::Argument(_))
                )
            {
                let id = match globals.facts.nonnull {
                    Some(id) => id,
                    None => {
                        let id = metadata(globals, "!{}");
                        globals.facts.nonnull = Some(id);
                        id
                    }
                };
                let _ = write!(suffix, ", !nonnull !{id}");
            }
        }
        if allocation {
            let at = line.find("call ptr @tz.alloc(").unwrap() + "call ".len();
            line.insert_str(at, "nonnull ");
        }
        line.push_str(&suffix);
    }
    if !scope.is_empty() {
        // The declaration models dynamic scope state, not ordinary argument memory.
        facts.memory.clear();
        intrinsics.insert("declare void @llvm.experimental.noalias.scope.decl(metadata)".into());
        let declarations = scope.values().map(|(own, _)| {
            format!("  call void @llvm.experimental.noalias.scope.decl(metadata !{own})")
        });
        lines.splice(0..0, declarations);
    }
    facts
}

#[cfg(test)]
mod tests {
    use super::*;

    fn annotate(source: &str, allocas: &[&str], instructions: &[&str]) -> Vec<String> {
        let module = crate::analyze(source).unwrap();
        let function = module
            .functions
            .iter()
            .find(|function| function.name == "f")
            .unwrap();
        let mut lines = instructions.iter().map(|line| (*line).to_owned()).collect();
        apply(
            &module,
            function,
            &allocas
                .iter()
                .map(|line| (*line).to_owned())
                .collect::<Vec<_>>(),
            &mut lines,
            true,
            &mut Globals::default(),
            &mut BTreeSet::new(),
        );
        lines
    }

    #[test]
    fn allocator_reload_nonnull_requires_a_private_single_store_slot() {
        let source = "def f :: unit -> unit = \\unused -> ()";
        let initial = [
            "%allocation = call ptr @tz.alloc(i64 8)",
            "store ptr %allocation, ptr %slot",
            "%loaded = load ptr, ptr %slot",
        ];
        let lines = annotate(source, &["%slot = alloca ptr, align 16"], &initial);
        assert!(lines[0].contains("call nonnull ptr"));
        assert!(lines[2].contains("!nonnull"));
        for instructions in [
            vec![
                initial[0],
                initial[1],
                "store ptr null, ptr %slot",
                initial[2],
            ],
            vec![
                initial[0],
                initial[1],
                "call void @unknown(ptr %slot)",
                initial[2],
            ],
            vec![
                initial[0],
                initial[1],
                "%alias = getelementptr inbounds ptr, ptr %slot, i64 0",
                initial[2],
            ],
        ] {
            assert!(
                !annotate(source, &["%slot = alloca ptr, align 16"], &instructions)
                    .iter()
                    .any(|line| line.contains("!nonnull"))
            );
        }
    }

    #[test]
    fn arbitrary_loaded_pointers_and_changed_reference_spills_have_no_provenance() {
        let source = "def f :: ref i64 -> i64 = \\value -> deref value";
        for instructions in [
            vec![
                "store ptr %p0, ptr %slot",
                "store ptr null, ptr %slot",
                "%loaded = load ptr, ptr %slot",
                "%number = load i64, ptr %loaded",
            ],
            vec![
                "%address = ptrtoint ptr %p0 to i64",
                "%pointer = inttoptr i64 %address to ptr",
                "%number = load i64, ptr %pointer",
            ],
            vec![
                "%pointer = load ptr, ptr %p0",
                "%number = load i64, ptr %pointer",
            ],
        ] {
            let lines = annotate(source, &["%slot = alloca ptr, align 16"], &instructions);
            assert!(
                !lines
                    .iter()
                    .any(|line| line.contains("!tbaa") || line.contains("!nonnull"))
            );
        }
    }

    #[test]
    fn analysis_budget_exhaustion_omits_optional_facts() {
        let source = "def f :: ref i64 -> i64 = \\value -> deref value";
        let instructions = vec!["%number = load i64, ptr %p0"; BUDGET / 16 + 1];
        let lines = annotate(source, &[], &instructions);
        assert!(!lines.iter().any(|line| line.contains("!tbaa")));
    }

    #[test]
    fn skipped_and_linear_bodies_keep_exact_ir_and_facts() {
        let module = crate::analyze("def f :: i64 -> i64 = \\value -> value").unwrap();
        let function = module
            .functions
            .iter()
            .find(|function| function.name == "f")
            .unwrap();
        for (single_entry, body, result, memory) in [
            (
                true,
                vec!["  %sum = add i64 %p0, 1, !dbg !0", "  ret i64 %sum"],
                "noundef ",
                " memory(none)",
            ),
            (
                true,
                vec![
                    "  br i1 true, label %yes, label %no",
                    "yes:",
                    "  ret i64 %p0",
                    "no:",
                    "  ret i64 0",
                ],
                "noundef ",
                "",
            ),
            (
                false,
                vec!["  %sum = add i64 %p0, 1", "  ret i64 %sum"],
                "",
                "",
            ),
        ] {
            let original: Vec<_> = body.into_iter().map(str::to_owned).collect();
            let mut lines = original.clone();
            let mut globals = Globals::default();
            let mut intrinsics = BTreeSet::new();
            let facts = apply(
                &module,
                function,
                &[],
                &mut lines,
                single_entry,
                &mut globals,
                &mut intrinsics,
            );
            assert_eq!(lines, original);
            assert_eq!(facts.parameters, [" noundef"]);
            assert_eq!(facts.result, result);
            assert_eq!(facts.memory, memory);
            assert!(globals.definitions.is_empty());
            assert!(intrinsics.is_empty());
        }
    }
}
