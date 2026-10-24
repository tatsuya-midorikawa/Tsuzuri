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
        || !function
            .signature
            .parameters
            .last()?
            .slice_element()
            .is_some_and(scalar)
    {
        return None;
    }
    // The slice is the last argument, after the scalars (an index and a value or a second index).
    let descriptor = operand(arguments.rsplit_once(", ")?.1.strip_prefix("%tz.array ")?);
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

fn local_tokens(text: &str) -> impl Iterator<Item = (usize, &str)> {
    let bytes = text.as_bytes();
    let mut cursor = 0;
    std::iter::from_fn(move || {
        while cursor < bytes.len() {
            match bytes[cursor] {
                b';' => {
                    cursor = bytes.len();
                    return None;
                }
                b'"' => cursor = traps::quoted_end(bytes, cursor).unwrap_or(bytes.len()),
                b'%' => {
                    let start = cursor;
                    cursor += 1;
                    while bytes.get(cursor).is_some_and(|byte| {
                        byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'$' | b'-')
                    }) {
                        cursor += 1;
                    }
                    return Some((start, &text[start..cursor]));
                }
                _ => cursor += 1,
            }
        }
        None
    })
}

fn replace_locals(text: &str, aliases: &BTreeMap<String, String>) -> String {
    let mut output = String::new();
    let mut copied = 0;
    for (start, token) in local_tokens(text) {
        if let Some(value) = aliases.get(token) {
            output.push_str(&text[copied..start]);
            output.push_str(value);
            copied = start + token.len();
        }
    }
    output.push_str(&text[copied..]);
    output
}

fn unquoted_local(value: &str) -> bool {
    local_tokens(value)
        .next()
        .is_some_and(|(start, token)| start == 0 && token == value && token.len() > 1)
}

fn integer_ir(ty: &str) -> bool {
    ty.strip_prefix('i').is_some_and(|digits| {
        !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit())
    })
}

fn private_integer_slots<'a>(lines: &[&'a str]) -> BTreeSet<&'a str> {
    let mut slots: BTreeSet<_> = lines
        .iter()
        .filter_map(|line| {
            let (value, rhs) = instruction(line);
            let ty = rhs.strip_prefix("alloca ")?.split([',', ' ']).next()?;
            integer_ir(ty)
                .then_some(value?)
                .filter(|value| unquoted_local(value))
        })
        .collect();
    let mut escaped = BTreeSet::new();
    for line in lines {
        let (value, rhs) = instruction(line);
        for (_, token) in local_tokens(line) {
            if slots.contains(token)
                && value != Some(token)
                && !access(rhs).is_some_and(|(ty, address, _)| integer_ir(ty) && address == token)
            {
                escaped.insert(token);
            }
        }
    }
    slots.retain(|slot| !escaped.contains(slot));
    slots
}

fn reuse_in_function(lines: &[&str], eligible: &BTreeSet<&str>) -> String {
    if lines
        .iter()
        .try_fold(BUDGET, |remaining, line| remaining.checked_sub(line.len()))
        .is_none()
    {
        return lines.concat();
    }
    let private = private_integer_slots(lines);
    let mut stored = BTreeMap::new();
    let mut values = BTreeMap::new();
    let mut calls = BTreeMap::<String, String>::new();
    let mut aliases = BTreeMap::new();
    let mut removed = BTreeSet::new();
    for (position, line) in lines.iter().enumerate() {
        let text = line.trim_start();
        let label = if text.starts_with('"') {
            traps::quoted_end(text.as_bytes(), 0).is_some_and(|end| text[end..].starts_with(':'))
        } else {
            text.split_once(':').is_some_and(|(name, _)| {
                !name.is_empty()
                    && name.bytes().all(|byte| {
                        byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'$' | b'-')
                    })
            })
        };
        if label {
            calls.clear();
            stored.clear();
        }
        let (value, rhs) = instruction(line);
        // A source scalar is spilled even at -O0. Only slots whose address never escapes may
        // supply a known value across calls; borrowed, atomic and volatile slots never do.
        if let Some((ty, address, storing)) = access(rhs)
            && integer_ir(ty)
            && private.contains(address)
        {
            if storing {
                let contents = rhs
                    .strip_prefix("store ")
                    .and_then(|text| text.split_once(' '))
                    .and_then(|(_, text)| text.split_once(", ptr "))
                    .map(|(value, _)| value);
                match contents {
                    Some(value) if !matches!(value, "undef" | "poison") => {
                        stored.insert(address, replace_locals(value, &values));
                    }
                    _ => {
                        stored.remove(address);
                    }
                }
            } else if let Some(value) = value
                && let Some(contents) = stored.get(address)
            {
                values.insert(value.to_owned(), contents.clone());
            }
        }
        let Some(value) = value else {
            continue;
        };
        if !rhs.starts_with("call ")
            || !unquoted_local(value)
            || value.as_bytes().get(1).is_some_and(u8::is_ascii_digit)
        {
            continue;
        }
        let Some(call) = traps::callable(rhs, false) else {
            continue;
        };
        if !eligible.contains(call.name) || call.close + 1 != rhs.len() {
            continue;
        }
        let key = replace_locals(rhs, &values);
        match calls.get(&key) {
            Some(previous) => {
                values.insert(value.to_owned(), previous.clone());
                aliases.insert(value.to_owned(), previous.clone());
                removed.insert(position);
            }
            None => {
                calls.insert(key, value.to_owned());
            }
        }
    }
    if aliases.is_empty() {
        return lines.concat();
    }
    // A loop phi can precede the call that supplies its back edge. Rewrite the whole function
    // only after collecting aliases; replacing later lines alone leaves an undefined SSA value.
    lines
        .iter()
        .enumerate()
        .filter(|(position, _)| !removed.contains(position))
        .map(|(_, line)| replace_locals(line, &aliases))
        .collect()
}

/// Reuse only an already executed integer-scalar call in its own basic block. Source purity
/// and the independently emitted memory(none) proof are both necessary, never interchangeable.
pub(super) fn reuse_pure_calls(module: &CheckedModule, ir: String) -> String {
    let pure: BTreeSet<_> = module
        .functions
        .iter()
        .filter(|function| {
            function.origin.pure
                && function.capture_count == 0
                && !function.is_task
                && function.origin.cpu == 0
                && matches!(function.signature.result, Type::Integer(..))
                && function
                    .signature
                    .parameters
                    .iter()
                    .all(|ty| matches!(ty, Type::Integer(..)))
        })
        .map(|function| format!("@tz.fn.{}", function.qualified_name()))
        .collect();
    if pure.is_empty() {
        return ir;
    }
    let eligible: BTreeSet<_> = ir
        .lines()
        .filter(|line| line.starts_with("define internal ") && line.contains(" memory(none)"))
        .filter_map(|line| traps::callable(line, true))
        .map(|call| call.name)
        .filter(|name| pure.contains(*name))
        .collect();
    if eligible.is_empty() {
        return ir;
    }
    let mut output = String::with_capacity(ir.len());
    let mut function = Vec::new();
    for line in ir.split_inclusive('\n') {
        if line.starts_with("define ") || !function.is_empty() {
            function.push(line);
            if line.trim_end() == "}" {
                output.push_str(&reuse_in_function(&function, &eligible));
                function.clear();
            }
        } else {
            output.push_str(line);
        }
    }
    for line in function {
        output.push_str(line);
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scalar_reuse_preserves_first_calls_blocks_and_forward_phis() {
        let source = "entry:\n  br label %loop\nloop:\n  %p = phi i64 [ 0, %entry ], [ %v1, %loop ]\n  %v0 = call i64 @f(i64 %p)\n  %v1 = call i64 @f(i64 %p), !dbg !12\n  %v10 = add i64 %v1, 1\n  br label %other\nother: ; a label may carry a comment\n  %v2 = call i64 @f(i64 %p)\n  ret i64 %v2\n";
        let lines: Vec<_> = source.split_inclusive('\n').collect();
        let actual = reuse_in_function(&lines, &BTreeSet::from(["@f"]));
        assert_eq!(actual.matches("call i64 @f").count(), 2);
        assert!(actual.contains("%v0 = call i64 @f(i64 %p)\n"));
        assert!(actual.contains("[ %v0, %loop ]"));
        assert!(actual.contains("%v10 = add i64 %v0, 1"));
        assert!(actual.contains("%v2 = call i64 @f(i64 %p)"));
    }

    #[test]
    fn scalar_reuse_tracks_only_non_escaping_integer_spills() {
        for (middle, count) in [
            ("call void @unrelated()", 1),
            ("store i64 2, ptr %slot", 2),
            ("call void @mutate(ptr %slot)", 2),
            ("%\"esc=ape\" = call i64 @mutate(ptr %slot)", 2),
            (
                "%alias = getelementptr i64, ptr %slot, i64 0\nstore i64 2, ptr %alias",
                2,
            ),
            ("store volatile i64 2, ptr %slot", 2),
            ("store atomic i64 2, ptr %slot seq_cst, align 8", 2),
            ("store ptr %slot, ptr %outside", 2),
        ] {
            let source = format!(
                "entry:\n  %slot = alloca i64, align 8\n  store i64 1, ptr %slot\n  %a = load i64, ptr %slot\n  %first = call i64 @f(i64 %a)\n  {middle}\n  %b = load i64, ptr %slot\n  %second = call i64 @f(i64 %b)\n  ret i64 %second\n"
            );
            let lines: Vec<_> = source.split_inclusive('\n').collect();
            let actual = reuse_in_function(&lines, &BTreeSet::from(["@f"]));
            assert_eq!(actual.matches("call i64 @f").count(), count, "{actual}");
        }
    }

    #[test]
    fn scalar_reuse_rewrites_only_complete_unquoted_ssa_tokens() {
        let aliases = BTreeMap::from([("%v1".into(), "%v0".into())]);
        let source = "%v10 = add i64 %v1, %v11 ; %v1 stays in a comment\n";
        assert_eq!(
            replace_locals(source, &aliases),
            "%v10 = add i64 %v0, %v11 ; %v1 stays in a comment\n"
        );
        let source = "call void @debug(ptr c\"%v1\\22%v1\", i64 %v1)";
        assert_eq!(
            replace_locals(source, &aliases),
            "call void @debug(ptr c\"%v1\\22%v1\", i64 %v0)"
        );
    }

    #[test]
    fn scalar_reuse_leaves_quoted_and_numbered_local_definitions_unchanged() {
        let source = "entry:\n  %0 = call i64 @f(i64 1)\n  %1 = call i64 @f(i64 1)\n  %\"left\" = call i64 @f(i64 1)\n  %\"right\" = call i64 @f(i64 1)\n  ret i64 %1\n";
        let lines: Vec<_> = source.split_inclusive('\n').collect();
        assert_eq!(reuse_in_function(&lines, &BTreeSet::from(["@f"])), source);
        let source = "entry:\n  %\"slot\" = alloca i64, align 8\n  store i64 1, ptr %\"slot\"\n  %a = load i64, ptr %\"slot\"\n  %first = call i64 @f(i64 %a)\n  call void @mutate(ptr %\"slot\")\n  %b = load i64, ptr %\"slot\"\n  %second = call i64 @f(i64 %b)\n  ret i64 %second\n";
        let lines: Vec<_> = source.split_inclusive('\n').collect();
        assert_eq!(reuse_in_function(&lines, &BTreeSet::from(["@f"])), source);
    }

    #[test]
    fn scalar_reuse_normalizes_arguments_but_keeps_different_calls_and_bundles() {
        let source = "entry:\n  %a = call i64 @f(i64 1)\n  %b = call i64 @f(i64 1)\n  %c = call i64 @f(i64 %a)\n  %d = call i64 @f(i64 %b)\n  %e = call i64 @f(i64 2)\n  %f = call i64 @f(i64 2) [ \"deopt\"(i64 0) ]\n  %g = call i64 @other(i64 2)\n  ret i64 %d\n";
        let lines: Vec<_> = source.split_inclusive('\n').collect();
        let actual = reuse_in_function(&lines, &BTreeSet::from(["@f"]));
        assert_eq!(actual.matches("call i64 @f").count(), 4);
        assert!(actual.contains("ret i64 %c"));
        assert!(actual.contains("%f = call i64 @f(i64 2) [ \"deopt\"(i64 0) ]"));
        assert!(actual.contains("%g = call i64 @other(i64 2)"));
        assert_eq!(reuse_in_function(&lines, &BTreeSet::new()), source);
        let large = format!("; {}\n{source}", "x".repeat(BUDGET));
        let lines: Vec<_> = large.split_inclusive('\n').collect();
        assert_eq!(reuse_in_function(&lines, &BTreeSet::from(["@f"])), large);
    }

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
