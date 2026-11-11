//! The cooperative preemption checks of TZ10 Phase 1 (D2, D4): in a native program that spawns, the
//! code that the compiler emits from Tsuzuri source reads `@tsuzuri_tz_preempt_pending` and, when
//! sysmon has asked some P to give up its tzroutine, calls `tsuzuri_tz_preempt_check`.
//!
//! The check is one line, `call void @tz.preempt.poll()`, whose body is `alwaysinline`: LLVM splits
//! the block for the cold call itself, so the pass never edits the control flow or a `phi`. Every
//! long computation is a loop or a recursion, and the check goes where they repeat:
//!
//! - in a function of a cycle of direct calls (recursion), before the first call in a block to a
//!   function of a cycle: the path that recurses checks once per call, the base case not at all;
//! - at the entry of a function whose cycle the IR does not show (`opaque_functions`: recursion that
//!   only a closure closes, and a `dyn` dispatcher), after the static `alloca`s of the entry block;
//! - before the back edge of a loop whose blocks make a *heavy* call: one that stays a call that can
//!   take long whatever LLVM inlines (the C runtime, an extern function, the C library, inline
//!   assembly, a recursive function, or a defined function that makes such a call).
//!
//! Everything else stays as it is, because a check there would cost more than the load it costs. A
//! call that LLVM may inline into a loop that is then free of calls and vectorized (a small function,
//! a lambda that a loop calls through a constant closure) would bring a check into that loop and cost
//! its vector loop (S2); a call that only allocates (`malloc`, `realloc`, `free` and the allocators
//! of `--allocator`) is bounded, and a check beside it in a loop of `Vec.push` grows the function
//! enough that LLVM stops inlining it (S3: `Array.filter` lost its inlined predicate, +32 %). An entry
//! check in every recursive function costs a recursion as small as `fib` 9 %; before its recursive
//! calls, nothing measurable (docs/benchmarks.md). What is left without a check is a loop whose calls
//! are all light: Phase 2 stops those (D2).
//!
//! Loops are found from the text of each function: its blocks, the labels of their terminators,
//! the dominators, and the natural loop of each back edge (an edge to a block that dominates its
//! source). A function that `facts` marks with a memory effect is straight-line and calls only
//! intrinsics; the pass leaves it alone.
//!
//! A check may move its tzroutine to another thread. A runtime function in IR that reads a thread's
//! local storage (`@llvm.threadlocal.address`, the caches of `--allocator small`) is made `noinline`,
//! so that no address of one thread's storage is kept in a function across a check (LLVM may reuse
//! the intrinsic's result across calls). A wait moves a tzroutine too, also the helper of a
//! `Parallel.*` in a program that does not spawn: there `with_thread_storage_noinline` adds the
//! `noinline` alone, when the code that the helpers run may wait (TZ14).

use std::collections::{BTreeMap, BTreeSet};

use super::traps::callable;
use crate::check::{CheckedModule, FunctionRef, TypedExpr, TypedExprKind};

/// The check that the pass inserts.
pub(super) const POLL_CALL: &str = "  call void @tz.preempt.poll()\n";

/// The check's body and the runtime's two symbols (src/runtime/tzroutine.c), which the runtime
/// defines hidden in the same image (a direct load, no GOT). The request is rare, so the call is
/// weighted cold. A yield is no synchronization of the program's memory (a program has no data race,
/// D-61), so the call touches only the count, which it takes as its argument, and memory that the
/// module cannot see: LLVM keeps the program's values in registers across it; it is never removed.
const POLL_DEFINITION: &str = "@tsuzuri_tz_preempt_pending = external hidden global i32\n\
declare void @tsuzuri_tz_preempt_check(ptr) nounwind memory(argmem: readwrite, inaccessiblemem: readwrite)\n\
define internal void @tz.preempt.poll() nounwind alwaysinline {\n\
entry:\n\
\x20 %pending = load atomic i32, ptr @tsuzuri_tz_preempt_pending monotonic, align 4\n\
\x20 %asked = icmp ne i32 %pending, 0\n\
\x20 br i1 %asked, label %yield, label %done, !prof !{!\"branch_weights\", i32 1, i32 1048575}\n\
yield:\n\
\x20 call void @tsuzuri_tz_preempt_check(ptr @tsuzuri_tz_preempt_pending)\n\
\x20 br label %done\n\
done:\n\
\x20 ret void\n\
}\n";

/// The C library's allocator and the allocators of `--allocator host` and of the trap boundaries:
/// bounded calls that never wait.
const LIGHT: [&str; 11] = [
    "@malloc",
    "@calloc",
    "@realloc",
    "@free",
    "@aligned_alloc",
    "@tsuzuri_host_alloc",
    "@tsuzuri_host_realloc",
    "@tsuzuri_host_free",
    "@tsuzuri_tracked_malloc",
    "@tsuzuri_tracked_realloc",
    "@tsuzuri_tracked_free",
];

/// A symbol without its `@` and the quotes of `@"name"`: the versions of a `@cpu` function are
/// `@"tz.fn.M.f.cpu.avx2"` (`llvm_cpu.rs`).
fn bare(name: &str) -> &str {
    let name = name.strip_prefix('@').unwrap_or(name);
    name.strip_prefix('"')
        .and_then(|name| name.strip_suffix('"'))
        .unwrap_or(name)
}

/// The functions whose code is Tsuzuri source: the ones that get checks.
pub(super) fn emitted_from_source(name: &str) -> bool {
    let name = bare(name);
    name.starts_with("tz.fn.") || name.starts_with("tz.specialized.")
}

/// The generated functions that the signal of Phase 2 may stop besides the source's and their callers
/// (`with_generated_section`), which reach the source through a pointer that LLVM may resolve: the
/// adapters, the entries of tzroutines, the chunks of `Parallel.*` and `Routine.scope`, and the C
/// wrappers that a G enters inside an extern call, a callback (`@tz.callback.*`) or an export
/// (`exports`, `tz_<name>`), which take the P back for their body.
fn preemptible(name: &str, exports: &BTreeSet<String>) -> bool {
    let bare = bare(name);
    emitted_from_source(name)
        || bare.starts_with("tz.apply.")
        || bare.starts_with("tz.tzroutine.start.")
        || bare.starts_with("tz.parallel.chunk.")
        || bare.starts_with("tz.callback.")
        || exports.contains(bare)
}

/// The symbols of the functions whose cycles the IR may not show: a function in a cycle of the
/// functions and lambdas that the source names (recursion, also through a closure), and a `dyn`
/// dispatcher, whose calls go through a table.
pub(super) fn opaque_functions(module: &CheckedModule) -> BTreeSet<String> {
    fn named(expression: &TypedExpr, found: &mut BTreeSet<usize>) {
        match &expression.kind {
            TypedExprKind::Function(FunctionRef::User(id))
            | TypedExprKind::GenericFunction(id, _)
            | TypedExprKind::Closure(id, _) => {
                found.insert(*id);
            }
            _ => {}
        }
        for child in expression.children() {
            named(child, found);
        }
    }
    let count = module.functions.len();
    let mut edges = Vec::with_capacity(count);
    let mut opaque = BTreeSet::new();
    for (id, function) in module.functions.iter().enumerate() {
        if matches!(function.body.kind, TypedExprKind::DynDispatch { .. }) {
            opaque.insert(id);
        }
        let mut found = BTreeSet::new();
        named(&function.body, &mut found);
        edges.push(
            found
                .into_iter()
                .filter(|&target| target < count)
                .collect::<Vec<_>>(),
        );
    }
    for component in components(&edges) {
        if component.len() > 1 || edges[component[0]].contains(&component[0]) {
            opaque.extend(component);
        }
    }
    opaque
        .into_iter()
        .map(|id| format!("@tz.fn.{}", module.functions[id].qualified_name()))
        .collect()
}

/// Tarjan's strongly connected components of a graph, iteratively; they come out callees first.
fn components(edges: &[Vec<usize>]) -> Vec<Vec<usize>> {
    let count = edges.len();
    let mut order = vec![usize::MAX; count];
    let mut low = vec![0usize; count];
    let mut on_stack = vec![false; count];
    let mut stack = Vec::new();
    let mut next = 0;
    let mut found = Vec::new();
    for root in 0..count {
        if order[root] != usize::MAX {
            continue;
        }
        let mut work = vec![(root, 0usize)];
        order[root] = next;
        low[root] = next;
        next += 1;
        stack.push(root);
        on_stack[root] = true;
        while let Some(&mut (node, ref mut position)) = work.last_mut() {
            if let Some(&target) = edges[node].get(*position) {
                *position += 1;
                if order[target] == usize::MAX {
                    order[target] = next;
                    low[target] = next;
                    next += 1;
                    stack.push(target);
                    on_stack[target] = true;
                    work.push((target, 0));
                } else if on_stack[target] {
                    low[node] = low[node].min(order[target]);
                }
                continue;
            }
            work.pop();
            if let Some(&(parent, _)) = work.last() {
                low[parent] = low[parent].min(low[node]);
            }
            if low[node] == order[node] {
                let mut component = Vec::new();
                while let Some(member) = stack.pop() {
                    on_stack[member] = false;
                    component.push(member);
                    if member == node {
                        break;
                    }
                }
                found.push(component);
            }
        }
    }
    found
}

/// What a `call` line calls.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Callee {
    /// An LLVM intrinsic, a call through a pointer (a closure, which LLVM resolves and inlines where
    /// it is a constant), or an allocator (`LIGHT`).
    Light,
    /// A function that the module defines (an opaque function of the source is heavy itself).
    Defined(String),
    /// A declared function, inline assembly or a form not recognized.
    Heavy,
}

fn callee(line: &str, defined: &BTreeSet<&str>) -> Option<Callee> {
    let body = line.trim_start();
    if body.starts_with(';') || !body.contains("call ") {
        return None;
    }
    let Some(call) = callable(line, false) else {
        return Some(Callee::Heavy);
    };
    let name = call.name;
    Some(
        if name.starts_with("@llvm.") || name.starts_with('%') || LIGHT.contains(&name) {
            Callee::Light
        } else if defined.contains(name) {
            Callee::Defined(name.to_owned())
        } else {
            Callee::Heavy
        },
    )
}

/// One `define` of the module: the lines of its header and closing brace.
struct Definition<'a> {
    name: &'a str,
    header: usize,
    close: usize,
}

fn definitions<'a>(lines: &[&'a str]) -> Vec<Definition<'a>> {
    let mut found = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        let header = lines[index].trim_end();
        if header.starts_with("define ") && header.ends_with('{') {
            let close = (index + 1..lines.len())
                .find(|&line| lines[line].trim_end() == "}")
                .unwrap_or(lines.len() - 1);
            if let Some(call) = callable(lines[index], true) {
                found.push(Definition {
                    name: call.name,
                    header: index,
                    close,
                });
            }
            index = close + 1;
        } else {
            index += 1;
        }
    }
    found
}

/// The defined functions in a cycle of direct calls, and those that make a heavy call (themselves
/// or through the defined functions that they call; a function of a cycle is heavy).
fn classify(calls: &BTreeMap<&str, Vec<Callee>>) -> (BTreeSet<String>, BTreeSet<String>) {
    let names: Vec<&str> = calls.keys().copied().collect();
    let index: BTreeMap<&str, usize> = names
        .iter()
        .enumerate()
        .map(|(id, name)| (*name, id))
        .collect();
    let edges: Vec<Vec<usize>> = names
        .iter()
        .map(|name| {
            calls[name]
                .iter()
                .filter_map(|callee| match callee {
                    Callee::Defined(target) => index.get(target.as_str()).copied(),
                    _ => None,
                })
                .collect()
        })
        .collect();
    let mut heavy = vec![false; names.len()];
    let mut cycles = BTreeSet::new();
    let mut heavies = BTreeSet::new();
    for component in components(&edges) {
        let cyclic = component.len() > 1 || edges[component[0]].contains(&component[0]);
        for &member in &component {
            heavy[member] = cyclic
                || calls[names[member]].contains(&Callee::Heavy)
                || edges[member].iter().any(|&target| heavy[target]);
            if cyclic {
                cycles.insert(names[member].to_owned());
            }
            if heavy[member] {
                heavies.insert(names[member].to_owned());
            }
        }
    }
    (cycles, heavies)
}

/// A label line (`name:` or `"name":`, a comment after it allowed), or `None`.
fn label_of(line: &str) -> Option<&str> {
    if line.starts_with([' ', '\t']) {
        return None;
    }
    let text = line.split(';').next().unwrap_or_default().trim_end();
    let name = text.strip_suffix(':')?;
    (!name.is_empty()).then_some(name)
}

/// The labels that `text` (a terminator) branches to.
fn targets(text: &str) -> Vec<&str> {
    let mut found = Vec::new();
    let mut rest = text;
    while let Some(at) = rest.find("label %") {
        let after = &rest[at + "label %".len()..];
        let end = if let Some(quoted) = after.strip_prefix('"') {
            quoted.find('"').map_or(after.len(), |end| end + 2)
        } else {
            after
                .find(|character: char| {
                    !(character.is_ascii_alphanumeric()
                        || matches!(character, '_' | '.' | '$' | '-'))
                })
                .unwrap_or(after.len())
        };
        found.push(&after[..end]);
        rest = &after[end..];
    }
    found
}

const TERMINATORS: [&str; 11] = [
    "br ",
    "switch ",
    "ret",
    "unreachable",
    "indirectbr ",
    "resume ",
    "invoke ",
    "callbr ",
    "cleanupret ",
    "catchret ",
    "catchswitch ",
];

fn terminator(line: &str) -> bool {
    let body = line.trim_start();
    let instruction = body
        .split_once(" = ")
        .filter(|(value, _)| value.starts_with('%'))
        .map_or(body, |(_, rest)| rest);
    TERMINATORS.iter().any(|word| instruction.starts_with(word))
}

/// One block of a function: its first instruction, where its terminator starts, its successors, and
/// whether it makes a heavy call.
struct Block<'a> {
    start: usize,
    terminator: usize,
    successors: Vec<&'a str>,
    heavy: bool,
}

/// What the checks of one function are placed by.
struct Rules<'r> {
    /// A call that makes a loop need a check at its back edge.
    heavy: &'r dyn Fn(&str) -> bool,
    /// A call to a function of a cycle (checked before the first such call of each block), when the
    /// function itself is in one.
    recursive: Option<&'r dyn Fn(&str) -> bool>,
    /// A check at the entry.
    entry: bool,
}

/// The lines before which a check goes in the function between `header` and `close`. `None` when
/// the function's blocks cannot be read.
fn check_sites(
    lines: &[&str],
    header: usize,
    close: usize,
    rules: &Rules<'_>,
) -> Option<BTreeSet<usize>> {
    let mut names: Vec<&str> = Vec::new();
    let mut blocks: Vec<Block<'_>> = Vec::new();
    let mut index = header + 1;
    let first = lines.get(index).and_then(|line| label_of(line))?;
    let mut current = Some(first);
    index += 1;
    let mut start = index;
    while index <= close {
        let label = if index == close {
            Some("")
        } else {
            label_of(lines[index])
        };
        if let Some(next) = label {
            let name = current?;
            let terminator = (start..index).find(|&line| terminator(lines[line]))?;
            let successors = lines[terminator..index]
                .iter()
                .flat_map(|line| targets(line))
                .collect();
            let heavy = (start..terminator).any(|line| (rules.heavy)(lines[line]));
            names.push(name);
            blocks.push(Block {
                start,
                terminator,
                successors,
                heavy,
            });
            current = (index != close).then_some(next);
            start = index + 1;
        }
        index += 1;
    }
    let id: BTreeMap<&str, usize> = names
        .iter()
        .enumerate()
        .map(|(position, name)| (*name, position))
        .collect();
    let successors: Vec<Vec<usize>> = blocks
        .iter()
        .map(|block| {
            block
                .successors
                .iter()
                .filter_map(|name| id.get(name).copied())
                .collect()
        })
        .collect();
    let count = blocks.len();
    // Reverse postorder from the entry block.
    let mut postorder = Vec::with_capacity(count);
    let mut seen = vec![false; count];
    let mut work = vec![(0usize, 0usize)];
    seen[0] = true;
    while let Some(&mut (node, ref mut position)) = work.last_mut() {
        if let Some(&target) = successors[node].get(*position) {
            *position += 1;
            if !seen[target] {
                seen[target] = true;
                work.push((target, 0));
            }
        } else {
            postorder.push(node);
            work.pop();
        }
    }
    let mut rank = vec![usize::MAX; count];
    for (position, &node) in postorder.iter().rev().enumerate() {
        rank[node] = position;
    }
    let mut predecessors = vec![Vec::new(); count];
    for (node, targets) in successors.iter().enumerate() {
        if seen[node] {
            for &target in targets {
                predecessors[target].push(node);
            }
        }
    }
    // Cooper, Harvey and Kennedy's iterative dominators.
    let mut dominator = vec![usize::MAX; count];
    dominator[0] = 0;
    let mut changed = true;
    while changed {
        changed = false;
        for &node in postorder.iter().rev().skip(1) {
            let mut new = usize::MAX;
            for &predecessor in &predecessors[node] {
                if dominator[predecessor] == usize::MAX {
                    continue;
                }
                new = if new == usize::MAX {
                    predecessor
                } else {
                    let (mut left, mut right) = (new, predecessor);
                    while left != right {
                        while rank[left] > rank[right] {
                            left = dominator[left];
                        }
                        while rank[right] > rank[left] {
                            right = dominator[right];
                        }
                    }
                    left
                };
            }
            if new != dominator[node] {
                dominator[node] = new;
                changed = true;
            }
        }
    }
    let dominates = |ancestor: usize, mut node: usize| loop {
        if node == ancestor {
            return true;
        }
        if node == 0 || dominator[node] == usize::MAX {
            return false;
        }
        node = dominator[node];
    };
    let mut sites = BTreeSet::new();
    if rules.entry {
        // A `musttail` call must stay right before its `ret`.
        let entry = blocks[0].terminator;
        let musttail = lines[entry].trim_start().starts_with("ret")
            && entry > blocks[0].start
            && lines[entry - 1].contains("musttail call ");
        sites.insert(if musttail { entry - 1 } else { entry });
    }
    if let Some(recursive) = rules.recursive {
        for (node, block) in blocks.iter().enumerate() {
            if seen[node]
                && let Some(line) =
                    (block.start..block.terminator).find(|&line| recursive(lines[line]))
            {
                sites.insert(line);
            }
        }
    }
    for latch in postorder.iter().copied() {
        for &header in &successors[latch] {
            if !dominates(header, latch) || sites.contains(&blocks[latch].terminator) {
                continue;
            }
            // The natural loop: the header and every block that reaches the latch without it.
            let mut body = BTreeSet::from([header, latch]);
            let mut work = vec![latch];
            while let Some(node) = work.pop() {
                if node == header {
                    continue;
                }
                for &predecessor in &predecessors[node] {
                    if body.insert(predecessor) {
                        work.push(predecessor);
                    }
                }
            }
            if body.iter().any(|&node| blocks[node].heavy) {
                sites.insert(blocks[latch].terminator);
            }
        }
    }
    Some(sites)
}

/// Adds the checks to the IR of a native program that spawns (TZ10 Phase 1). `opaque` names the
/// functions of `opaque_functions`.
pub(crate) fn with_preemption_checks(ir: String, opaque: &BTreeSet<String>) -> String {
    let lines: Vec<&str> = ir.split_inclusive('\n').collect();
    let functions = definitions(&lines);
    let defined: BTreeSet<&str> = functions.iter().map(|function| function.name).collect();
    let mut calls: BTreeMap<&str, Vec<Callee>> = BTreeMap::new();
    for function in &functions {
        let mut found: Vec<Callee> = lines[function.header + 1..function.close]
            .iter()
            .filter_map(|line| callee(line, &defined))
            .collect();
        if opaque.contains(function.name) {
            found.push(Callee::Heavy);
        }
        calls.insert(function.name, found);
    }
    let (cycles, heavies) = classify(&calls);
    let heavy = |line: &str| match callee(line, &defined) {
        None | Some(Callee::Light) => false,
        Some(Callee::Defined(name)) => heavies.contains(&name),
        Some(Callee::Heavy) => true,
    };
    let recursive = |line: &str| match callee(line, &defined) {
        Some(Callee::Defined(name)) => cycles.contains(&name),
        _ => false,
    };
    let mut sites = BTreeSet::new();
    let thread_bound = thread_bound(&lines, &functions);
    for function in &functions {
        if !emitted_from_source(function.name)
            || !heavies.contains(function.name)
            || lines[function.header].contains(" memory(")
        {
            continue;
        }
        let rules = Rules {
            heavy: &heavy,
            recursive: cycles
                .contains(function.name)
                .then_some(&recursive as &dyn Fn(&str) -> bool),
            entry: opaque.contains(function.name) && !cycles.contains(function.name),
        };
        if let Some(found) = check_sites(&lines, function.header, function.close, &rules) {
            sites.extend(found);
        }
    }
    // A program without check sites still needs the functions that read thread storage kept out of
    // the code that the signal of Phase 2 stops.
    if sites.is_empty() && thread_bound.is_empty() {
        return ir;
    }
    let mut output = String::with_capacity(ir.len() + sites.len() * POLL_CALL.len() + 512);
    for (index, line) in lines.iter().enumerate() {
        if sites.contains(&index) {
            output.push_str(POLL_CALL);
        }
        push_line(&mut output, line, thread_bound.contains(&index));
    }
    if !sites.is_empty() {
        if !output.ends_with('\n') {
            output.push('\n');
        }
        output.push_str(POLL_DEFINITION);
    }
    output
}

/// The headers of the runtime's functions in IR (not emitted from source) that read a thread's local
/// storage (`@llvm.threadlocal.address`: the caches of `--allocator small`, `tz_small_realloc`) and
/// are not `noinline` yet.
fn thread_bound(lines: &[&str], functions: &[Definition<'_>]) -> BTreeSet<usize> {
    functions
        .iter()
        .filter(|function| {
            !emitted_from_source(function.name)
                && !lines[function.header].contains(" noinline")
                && lines[function.header + 1..function.close]
                    .iter()
                    .any(|line| line.contains("@llvm.threadlocal.address"))
        })
        .map(|function| function.header)
        .collect()
}

/// Copies `line`, with ` noinline` before the ` {` of a header of `thread_bound`.
fn push_line(output: &mut String, line: &str, thread_bound: bool) {
    match line.rfind(" {").filter(|_| thread_bound) {
        Some(at) => {
            output.push_str(&line[..at]);
            output.push_str(" noinline");
            output.push_str(&line[at..]);
        }
        None => output.push_str(line),
    }
}

/// The runtime's entry that runs the chunks of `Parallel.*` on helper tzroutines
/// (src/runtime/tzroutine.c, `src/llvm_parallel.rs`).
const PARALLEL: &str = "@tsuzuri_tz_parallel";

/// The declared functions that a helper of `Parallel.*` may call without waiting (TZ14): the C
/// library's allocator, which `src/runtime/heap-small.ll` and the runtime's other IR call, and its
/// memory functions. None of them enters the tzroutine runtime. Every other declared function, the
/// runtime's included, counts as a wait.
const NEVER_WAITS: [&str; 10] = [
    "@malloc",
    "@calloc",
    "@realloc",
    "@free",
    "@posix_memalign",
    "@aligned_alloc",
    "@memcpy",
    "@memmove",
    "@memset",
    "@memcmp",
];

/// TZ14: whether a helper tzroutine of `Parallel.*` may wait in a native program that does not
/// spawn, that is whether some code that `@tsuzuri_tz_parallel` runs on a helper may call into
/// something that parks it. The entries are the chunks that the IR passes to `@tsuzuri_tz_parallel`;
/// from them the analysis follows the direct calls to defined functions. It answers `true`, and errs
/// that way, for a reachable call through a pointer, inline assembly, an `invoke` or `callbr`, a
/// call that it does not parse, a call of `@tsuzuri_tz_parallel` (a nested `Parallel.*`, whose
/// helper waits for its own), a call of a declared function other than the intrinsics and
/// `NEVER_WAITS`, and for a use of `@tsuzuri_tz_parallel` other than a direct call with a defined
/// function as its first operand.
fn helpers_may_wait(lines: &[&str], functions: &[Definition<'_>]) -> bool {
    helper_wait(lines, functions).is_some()
}

/// The line that makes `helpers_may_wait` answer `true`, or `None`.
fn helper_wait<'a>(lines: &[&'a str], functions: &[Definition<'_>]) -> Option<&'a str> {
    let defined: BTreeMap<&str, &Definition<'_>> = functions
        .iter()
        .map(|function| (function.name, function))
        .collect();
    let mut work = Vec::new();
    for &line in lines {
        let body = line.trim_start();
        if !line.contains(PARALLEL) || body.starts_with("declare ") || body.starts_with(';') {
            continue;
        }
        let Some(call) = callable(line, false).filter(|call| call.name == PARALLEL) else {
            return Some(line);
        };
        let chunk = line[call.open + 1..call.close]
            .split(',')
            .next()
            .and_then(|operand| operand.trim().strip_prefix("ptr "))
            .map(str::trim);
        match chunk.and_then(|chunk| defined.get(chunk)) {
            Some(function) => work.push(*function),
            None => return Some(line),
        }
    }
    let mut seen = BTreeSet::new();
    while let Some(function) = work.pop() {
        if !seen.insert(function.name) {
            continue;
        }
        for &line in &lines[function.header + 1..function.close] {
            let body = line.trim_start();
            if body.starts_with(';') {
                continue;
            }
            if body.contains("invoke ") || body.contains("callbr ") {
                return Some(line);
            }
            let Some(at) = body.find("call ") else {
                continue;
            };
            if body[at..].contains(" asm ") {
                return Some(line);
            }
            let Some(call) = callable(line, false) else {
                return Some(line);
            };
            if call.name.starts_with("@llvm.") || NEVER_WAITS.contains(&call.name) {
                continue;
            }
            if call.name == PARALLEL {
                return Some(line);
            }
            match defined.get(call.name) {
                Some(callee) => work.push(*callee),
                // A pointer (`%name`) or a declared function.
                None => return Some(line),
            }
        }
    }
    None
}

/// The starts of LLVM's top-level forms: what may follow the closing brace of a function on its
/// line in the runtime's IR (`runtime_definitions`).
const TOP_LEVEL: [&str; 12] = [
    "define ",
    "declare ",
    "@",
    "%",
    "$",
    "!",
    ";",
    "attributes ",
    "source_filename ",
    "target ",
    "module ",
    "uselistorder",
];

/// The lines of `ir` and its definitions, for `with_thread_storage_noinline`. IR joined from pieces
/// that end without a newline starts the next piece on the line of the last closing brace
/// (`}define internal void @tz.string.copy(...) nounwind {`, `}; PM08: ...`,
/// `}@tz.debug.newline = ...`). Such a line, a brace and then a top-level form (`TOP_LEVEL`), is
/// split after the brace and the blanks that follow it, so that the function closes there and the
/// next one is a definition of its own: `definitions` would take what follows into the function
/// before, so that the calls of a function after the brace looked like calls of a declared function.
/// The pieces of the runtime's IR end with a newline since TZ15 (`src/runtime/*.ll`, kept so by a
/// unit test of `llvm`), so the emitter makes no such line; the split stays as a guard for IR that is
/// joined in another way. The lines are slices of `ir` and make `ir` again.
/// (`with_preemption_checks` keeps the lines of `ir`, so that the checks of TZ10 stay where they are.)
fn runtime_definitions(ir: &str) -> (Vec<&str>, Vec<Definition<'_>>) {
    let mut lines = Vec::new();
    for line in ir.split_inclusive('\n') {
        let form = line
            .strip_prefix('}')
            .map(|rest| line.len() - rest.trim_start().len())
            .filter(|&at| TOP_LEVEL.iter().any(|form| line[at..].starts_with(form)));
        match form {
            Some(at) => {
                lines.push(&line[..at]);
                lines.push(&line[at..]);
            }
            None => lines.push(line),
        }
    }
    let functions = definitions(&lines);
    (lines, functions)
}

/// TZ14: the `noinline` of `with_preemption_checks` alone, for a native program whose tzroutines
/// are only the helpers of `Parallel.*`. It gets no check (TZ10 D1, S1), and only a wait moves a
/// helper to another thread: then LLVM may have kept the address that `@llvm.threadlocal.address`
/// returned across the wait. A helper whose code cannot wait (`helpers_may_wait`) runs each chunk to
/// its end on one thread, and the caller of a top-level `Parallel.*` is a thread's pseudo-G, which
/// never leaves its thread: then, as without such a function, the IR is returned as it is.
pub(crate) fn with_thread_storage_noinline(ir: String) -> String {
    let (lines, functions) = runtime_definitions(&ir);
    let thread_bound = thread_bound(&lines, &functions);
    if thread_bound.is_empty() || !helpers_may_wait(&lines, &functions) {
        return ir;
    }
    let mut output = String::with_capacity(ir.len() + thread_bound.len() * " noinline".len());
    for (index, line) in lines.iter().enumerate() {
        push_line(&mut output, line, thread_bound.contains(&index));
    }
    output
}

/// The section of the generated code (TZ10 Phase 2): the only code where the runtime's signal
/// handler may stop a tzroutine. Every function emitted from source goes there, after the checks
/// (whose cold call LLVM inlines into it), with the generated functions of `preemptible` and every
/// function that calls one of them directly, itself or through others: LLVM may inline a source
/// function into any of its callers, with a loop that has no calls (a `dyn` slot, the drop of an
/// environment or a `dyn` value, which run a method or a `Drop.drop`). A function that reads thread
/// storage (`@llvm.threadlocal.address`) stays in `.text`, and so do the callers that only it joins
/// to the source (it is `noinline`, `with_preemption_checks`), and the runtime's other functions in
/// IR. None of these functions holds a runtime lock: the runtime calls the generated code after it
/// lets go of its locks.
pub(super) const MACH_O_SECTION: &str = "__TEXT,__tz_text,regular,pure_instructions";
pub(super) const ELF_SECTION: &str = "tz_text";

pub(crate) fn with_generated_section(
    ir: String,
    mach_o: bool,
    exports: &BTreeSet<String>,
) -> String {
    let section = format!(
        " section \"{}\"",
        if mach_o { MACH_O_SECTION } else { ELF_SECTION }
    );
    let lines: Vec<&str> = ir.split_inclusive('\n').collect();
    let functions = definitions(&lines);
    let defined: BTreeSet<&str> = functions.iter().map(|function| function.name).collect();
    let mut callers: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    let mut thread_bound = vec![false; functions.len()];
    for (id, function) in functions.iter().enumerate() {
        for line in &lines[function.header + 1..function.close] {
            thread_bound[id] |= line.contains("@llvm.threadlocal.address");
            if let Some(Callee::Defined(name)) = callee(line, &defined) {
                callers.entry(name).or_default().push(id);
            }
        }
    }
    let mut inside = vec![false; functions.len()];
    let mut work: Vec<usize> = (0..functions.len())
        .filter(|&id| !thread_bound[id] && preemptible(functions[id].name, exports))
        .collect();
    for &id in &work {
        inside[id] = true;
    }
    while let Some(id) = work.pop() {
        for &caller in callers.get(functions[id].name).into_iter().flatten() {
            if !inside[caller] && !thread_bound[caller] {
                inside[caller] = true;
                work.push(caller);
            }
        }
    }
    let headers: BTreeSet<usize> = functions
        .iter()
        .zip(&inside)
        .filter(|(_, inside)| **inside)
        .map(|(function, _)| function.header)
        .collect();
    let mut output = String::with_capacity(ir.len() + headers.len() * section.len());
    for (index, line) in lines.iter().enumerate() {
        let header = line.trim_end();
        let at = (headers.contains(&index) && !header.contains(" section \""))
            .then(|| header.find(" !dbg ").or_else(|| header.rfind(" {")))
            .flatten();
        match at {
            Some(at) => {
                output.push_str(&line[..at]);
                output.push_str(&section);
                output.push_str(&line[at..]);
            }
            None => output.push_str(line),
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    fn checks(ir: &str, opaque: &[&str]) -> Vec<String> {
        let opaque = opaque.iter().map(|name| (*name).to_owned()).collect();
        let output = with_preemption_checks(ir.to_owned(), &opaque);
        let mut found = Vec::new();
        let mut function = String::new();
        let lines: Vec<&str> = output.lines().collect();
        for (index, line) in lines.iter().enumerate() {
            if line.starts_with("define ") {
                function = callable(line, true).unwrap().name.to_owned();
            }
            if *line == POLL_CALL.trim_end() {
                found.push(format!("{function} {}", lines[index + 1].trim()));
            }
        }
        found
    }

    #[test]
    fn light_calls_and_call_free_loops_get_no_check() {
        let ir = "define internal i64 @tz.fn.M.square(i64 %arg0) nounwind {\nentry:\n  br label %loop\nloop:\n  %v = mul i64 %arg0, %arg0\n  ret i64 %v\n}\n\
define internal ptr @tz.alloc(i64 %size) nounwind {\nentry:\n  %p = call ptr @malloc(i64 %size)\n  ret ptr %p\n}\n\
define internal i64 @tz.fn.M.sum(ptr %arg0) nounwind {\nentry:\n  %s = alloca i64\n  br label %loop\nloop:\n  br label %b0\nb0:\n  %x = call i64 @tz.fn.M.square(i64 1)\n  %y = call i64 %arg0(i64 %x)\n  %z = call ptr @tz.alloc(i64 8)\n  %c = call { i64, i1 } @llvm.sadd.with.overflow.i64(i64 %x, i64 1)\n  br i1 true, label %b0, label %b1\nb1:\n  ret i64 %x\n}\n\
declare ptr @malloc(i64)\n";
        assert!(checks(ir, &[]).is_empty());
        assert_eq!(with_preemption_checks(ir.to_owned(), &BTreeSet::new()), ir);
    }

    #[test]
    fn recursion_and_a_loop_with_a_heavy_call_are_checked() {
        let ir = "define internal i64 @tz.fn.M.fib(i64 %arg0) nounwind {\nentry:\n  %slot = alloca i64\n  br label %loop\nloop:\n  %small = icmp slt i64 %arg0, 2\n  br i1 %small, label %b0, label %b1\nb0:\n  ret i64 %arg0\nb1:\n  %a = call i64 @tz.fn.M.fib(i64 %arg0)\n  %b = call i64 @tz.fn.M.fib(i64 %a)\n  ret i64 %b\n}\n\
define internal void @tz.fn.M.spin(ptr %arg0) nounwind {\nentry:\n  br label %loop\nloop:\n  br label %b0\nb0:\n  call void @tsuzuri_tz_yield()\n  br label %b1\nb1:\n  %x = load i1, ptr %arg0\n  switch i32 0, label %b0 [\n    i32 1, label %b2\n  ]\nb2:\n  ret void\n}\n\
declare void @tsuzuri_tz_yield()\n";
        assert_eq!(
            checks(ir, &[]),
            [
                "@tz.fn.M.fib %a = call i64 @tz.fn.M.fib(i64 %arg0)",
                "@tz.fn.M.spin switch i32 0, label %b0 [",
            ]
        );
        let output = with_preemption_checks(ir.to_owned(), &BTreeSet::new());
        assert_eq!(
            output
                .matches("define internal void @tz.preempt.poll()")
                .count(),
            1
        );
        assert_eq!(
            output
                .matches("@tsuzuri_tz_preempt_pending = external hidden global i32")
                .count(),
            1
        );
    }

    #[test]
    fn an_opaque_function_checks_at_its_entry_and_makes_loops_heavy() {
        let ir = "define internal i64 @tz.fn.M.apply(ptr %arg0) nounwind {\nentry:\n  br label %loop\nloop:\n  br label %b0\nb0:\n  %x = call i64 %arg0(i64 1)\n  %y = call i64 @tz.fn.M.walk(i64 %x)\n  br i1 true, label %b0, label %b1\nb1:\n  ret i64 %x\n}\n\
define internal i64 @tz.fn.M.walk(i64 %arg0) nounwind {\nentry:\n  %slot = alloca i64\n  br label %loop\nloop:\n  ret i64 %arg0\n}\n";
        assert!(checks(ir, &[]).is_empty());
        assert_eq!(
            checks(ir, &["@tz.fn.M.walk"]),
            [
                "@tz.fn.M.apply br i1 true, label %b0, label %b1",
                "@tz.fn.M.walk br label %loop",
            ]
        );
    }

    #[test]
    fn the_quoted_versions_of_a_cpu_function_are_source() {
        // `@cpu` versions have quoted symbols (`llvm_cpu.rs`; Copilot on #91).
        let ir = "define internal void @\"tz.fn.M.spin.cpu.avx2\"(ptr %arg0) nounwind {\nentry:\n  br label %b0\nb0:\n  call void @tsuzuri_tz_yield()\n  br label %b1\nb1:\n  %x = load i1, ptr %arg0\n  br i1 %x, label %b0, label %b2\nb2:\n  ret void\n}\ndeclare void @tsuzuri_tz_yield()\n";
        assert_eq!(
            checks(ir, &[]),
            ["@\"tz.fn.M.spin.cpu.avx2\" br i1 %x, label %b0, label %b2"]
        );
    }

    #[test]
    fn generated_functions_go_to_their_section_and_the_runtime_stays() {
        let ir = "define internal i64 @tz.fn.M.f(i64 %arg0) nounwind \"probe-stack\"=\"inline-asm\" !dbg !7 {\nentry:\n  ret i64 0\n}\n\
define internal i64 @tz.specialized.3(i64 %arg0) nounwind {\nentry:\n  ret i64 0\n}\n\
define internal ptr @tz.alloc(i64 %size) nounwind {\nentry:\n  ret ptr null\n}\n\
define internal i8 @tz.apply.$once.4.1(ptr %env) nounwind alwaysinline {\nentry:\n  ret i8 0\n}\n\
define internal void @tz.tzroutine.start.$once.4.1(ptr %env) {\nentry:\n  ret void\n}\n\
define internal void @tz.parallel.chunk.0(ptr %context, i64 %start, i64 %end) {\nentry:\n  ret void\n}\n\
define internal i64 @\"tz.fn.M.spin.cpu.avx2\"(i64 %arg0) nounwind {\nentry:\n  ret i64 0\n}\n\
define internal i64 @tz.callback.Main.spin(i64 %arg0) nounwind {\nentry:\n  ret i64 0\n}\n\
define i64 @tz_spin_export(i64 %arg0) nounwind {\nentry:\n  ret i64 0\n}\n\
define internal ptr @tz_small_realloc(ptr %block, i64 %size) nounwind {\nentry:\n  ret ptr null\n}\n";
        let exports = BTreeSet::from(["tz_spin_export".to_owned()]);
        let elf = with_generated_section(ir.to_owned(), false, &exports);
        // The adapter and the entry of a tzroutine, into which -O3 inlines a body that is only a
        // loop (Copilot on #91).
        assert!(elf.contains(
            "@tz.apply.$once.4.1(ptr %env) nounwind alwaysinline section \"tz_text\" {\n"
        ));
        assert!(elf.contains("@tz.tzroutine.start.$once.4.1(ptr %env) section \"tz_text\" {\n"));
        assert!(elf.contains(
            "@tz.parallel.chunk.0(ptr %context, i64 %start, i64 %end) section \"tz_text\" {\n"
        ));
        assert!(elf.contains("@tz.fn.M.f(i64 %arg0) nounwind \"probe-stack\"=\"inline-asm\" section \"tz_text\" !dbg !7 {\n"));
        assert!(elf.contains("@tz.specialized.3(i64 %arg0) nounwind section \"tz_text\" {\n"));
        assert!(elf.contains("@tz.alloc(i64 %size) nounwind {\n"));
        let mach_o = with_generated_section(ir.to_owned(), true, &exports);
        // the quoted symbol of a `@cpu` version (Copilot on #91)
        assert!(
            elf.contains("@\"tz.fn.M.spin.cpu.avx2\"(i64 %arg0) nounwind section \"tz_text\" {\n")
        );
        // the wrappers of a callback and of an export that a G enters inside an extern call, but not
        // a runtime function with the same `tz_` prefix (Copilot on #91)
        assert!(elf.contains("@tz.callback.Main.spin(i64 %arg0) nounwind section \"tz_text\" {\n"));
        assert!(elf.contains("@tz_spin_export(i64 %arg0) nounwind section \"tz_text\" {\n"));
        assert!(elf.contains("@tz_small_realloc(ptr %block, i64 %size) nounwind {\n"));
        assert_eq!(mach_o.matches(MACH_O_SECTION).count(), 8);
        assert_eq!(with_generated_section(elf.clone(), false, &exports), elf);
    }

    #[test]
    fn the_callers_of_the_source_go_to_the_section_unless_they_read_thread_storage() {
        // A `dyn` slot and the drops of an environment and of a `dyn` value call a method or a
        // `Drop.drop` directly, which -O3 inlines with its loop; their callers may inline them in
        // turn (Copilot on #91). A function that reads thread storage stays out, and so does one
        // that reaches the source only through it.
        let ir = "define internal i64 @tz.fn.Main.spin(ptr %arg0) nounwind {\nentry:\n  ret i64 0\n}\n\
define internal i64 @\"tz.dyn.slot.Main.$instance.8.spin\"(ptr %self) nounwind {\nentry:\n  %v = call i64 @tz.fn.Main.spin(ptr %self)\n  ret i64 %v\n}\n\
define internal void @\"tz.dyn.drop[Main.Spinner]\"(ptr %self) nounwind {\nentry:\n  %v = call i64 @tz.fn.Main.spin(ptr %self)\n  ret void\n}\n\
define internal void @tz.env.drop.$lambda.4.1(ptr %env) nounwind {\nentry:\n  call void @\"tz.dyn.drop[Main.Spinner]\"(ptr %env)\n  call void @tz.free(ptr %env)\n  ret void\n}\n\
define internal void @tz.free(ptr %block) nounwind {\nentry:\n  ret void\n}\n\
define internal void @tz.closure.drop(ptr %code, ptr %env) nounwind {\nentry:\n  call void %code(ptr %env)\n  ret void\n}\n\
define internal ptr @tz.cache(ptr %env) nounwind noinline {\nentry:\n  %t = call ptr @llvm.threadlocal.address.p0(ptr @tls)\n  %v = call i64 @tz.fn.Main.spin(ptr %t)\n  ret ptr %t\n}\n\
define internal ptr @tz.cached(ptr %env) nounwind {\nentry:\n  %t = call ptr @tz.cache(ptr %env)\n  ret ptr %t\n}\n\
@tls = internal thread_local global i64 0\n";
        let elf = with_generated_section(ir.to_owned(), false, &BTreeSet::new());
        for inside in [
            "@tz.fn.Main.spin(ptr %arg0) nounwind section \"tz_text\" {\n",
            "@\"tz.dyn.slot.Main.$instance.8.spin\"(ptr %self) nounwind section \"tz_text\" {\n",
            "@\"tz.dyn.drop[Main.Spinner]\"(ptr %self) nounwind section \"tz_text\" {\n",
            "@tz.env.drop.$lambda.4.1(ptr %env) nounwind section \"tz_text\" {\n",
        ] {
            assert!(elf.contains(inside), "{inside}");
        }
        for outside in [
            "@tz.free(ptr %block) nounwind {\n",
            "@tz.closure.drop(ptr %code, ptr %env) nounwind {\n",
            "@tz.cache(ptr %env) nounwind noinline {\n",
            "@tz.cached(ptr %env) nounwind {\n",
        ] {
            assert!(elf.contains(outside), "{outside}");
        }
        assert_eq!(elf.matches(ELF_SECTION).count(), 4);
    }

    #[test]
    fn runtime_and_marked_functions_are_left_alone_and_thread_storage_stays_out_of_line() {
        let ir = "define internal ptr @tz.cache(i64 %size) nounwind {\nentry:\n  %t = call ptr @llvm.threadlocal.address.p0(ptr @tls)\n  br label %b0\nb0:\n  call void @tsuzuri_tz_yield()\n  br i1 true, label %b0, label %b1\nb1:\n  ret ptr %t\n}\n\
define internal i64 @tz.fn.M.pure(i64 %arg0) nounwind memory(none) {\nentry:\n  call void @tsuzuri_tz_yield()\n  ret i64 0\n}\n\
define internal i64 @tz.fn.M.spawn(i64 %arg0) nounwind {\nentry:\n  br label %b0\nb0:\n  call void @tsuzuri_tz_yield()\n  br i1 true, label %b0, label %b1\nb1:\n  ret i64 0\n}\n\
@tls = internal thread_local global i64 0\ndeclare void @tsuzuri_tz_yield()\n";
        assert_eq!(
            checks(ir, &[]),
            ["@tz.fn.M.spawn br i1 true, label %b0, label %b1"]
        );
        let output = with_preemption_checks(ir.to_owned(), &BTreeSet::new());
        assert!(output.contains("define internal ptr @tz.cache(i64 %size) nounwind noinline {\n"));
        // Without a check site the function is still kept out of line, and nothing else changes
        // (Copilot on #91).
        let no_sites = "define internal ptr @tz.cache(i64 %size) nounwind {\nentry:\n  %t = call ptr @llvm.threadlocal.address.p0(ptr @tls)\n  ret ptr %t\n}\n\
define internal i64 @tz.fn.M.leaf(i64 %arg0) nounwind {\nentry:\n  ret i64 %arg0\n}\n\
@tls = internal thread_local global i64 0\n";
        let output = with_preemption_checks(no_sites.to_owned(), &BTreeSet::new());
        assert_eq!(
            output,
            no_sites.replace(
                "@tz.cache(i64 %size) nounwind {",
                "@tz.cache(i64 %size) nounwind noinline {"
            )
        );
    }

    #[test]
    fn thread_storage_stays_out_of_line_without_the_checks() {
        // TZ14: a helper of `Parallel.*` whose chunk may wait (here `@tz.fn.M.spin` yields) moves
        // between threads in a program that does not spawn, which gets no check: only the runtime's
        // function that reads thread storage changes.
        let ir = "define internal ptr @tz.cache(i64 %size) nounwind {\nentry:\n  %t = call ptr @llvm.threadlocal.address.p0(ptr @tls)\n  ret ptr %t\n}\n\
define internal ptr @tz.kept(i64 %size) nounwind noinline {\nentry:\n  %t = call ptr @llvm.threadlocal.address.p0(ptr @tls)\n  ret ptr %t\n}\n\
define internal i64 @tz.fn.M.spin(i64 %arg0) nounwind {\nentry:\n  %t = call ptr @llvm.threadlocal.address.p0(ptr @tls)\n  br label %b0\nb0:\n  call void @tsuzuri_tz_yield()\n  br i1 true, label %b0, label %b1\nb1:\n  ret i64 %arg0\n}\n\
define internal i64 @\"tz.fn.M.spin.cpu.avx2\"(i64 %arg0) nounwind {\nentry:\n  %t = call ptr @llvm.threadlocal.address.p0(ptr @tls)\n  ret i64 %arg0\n}\n\
define internal void @tz.parallel.chunk.0(ptr %context, i64 %chunk) nounwind {\nentry:\n  %v = call i64 @tz.fn.M.spin(i64 %chunk)\n  ret void\n}\n\
define internal i64 @tz.fn.M.run(i64 %arg0) nounwind {\nentry:\n  call i32 @tsuzuri_tz_parallel(ptr @tz.parallel.chunk.0, ptr null, i64 %arg0)\n  ret i64 %arg0\n}\n\
@tls = internal thread_local global i64 0\ndeclare void @tsuzuri_tz_yield()\ndeclare i32 @tsuzuri_tz_parallel(ptr, ptr, i64)\n";
        let output = with_thread_storage_noinline(ir.to_owned());
        assert_eq!(
            output,
            ir.replace(
                "@tz.cache(i64 %size) nounwind {",
                "@tz.cache(i64 %size) nounwind noinline {"
            )
        );
        // The functions that the checks of a program that spawns keep out of line (TZ10).
        let checked = with_preemption_checks(ir.to_owned(), &BTreeSet::new());
        assert!(checked.contains(POLL_CALL));
        assert_eq!(
            checked.replace(POLL_CALL, "").replace(POLL_DEFINITION, ""),
            output
        );
        // Without such a function the IR is returned as it is.
        let plain = "define internal i64 @tz.fn.M.leaf(i64 %arg0) nounwind {\nentry:\n  ret i64 %arg0\n}\n\
define internal void @tz.free(ptr %block) nounwind {\nentry:\n  ret void\n}\n";
        assert_eq!(with_thread_storage_noinline(plain.to_owned()), plain);
    }

    /// A program that does not spawn and calls `Parallel.*`: `@tz.fn.M.run` passes
    /// `@tz.parallel.chunk.0` to the runtime, and `@tz.fn.M.nested` calls `Parallel.*` itself.
    /// `{chunk}` is the chunk's code, `{operand}` the chunk that `@tz.fn.M.run` passes.
    const PARALLEL_PROGRAM: &str = "define internal ptr @tz.cache(i64 %size) nounwind {\nentry:\n  %t = call ptr @llvm.threadlocal.address.p0(ptr @tls)\n  ret ptr %t\n}\n\
define internal i64 @tz.fn.M.grow(i64 %arg0) nounwind {\nentry:\n  %p = call ptr @tz.cache(i64 %arg0)\n  call void @llvm.memset.p0.i64(ptr %p, i8 0, i64 8, i1 false)\n  %q = tail call ptr @malloc(i64 16) nounwind\n  call void @free(ptr %q)\n  %d = call i32 @tz.digits(i64 %arg0)\n  ret i64 %arg0\n}\n\
define internal i64 @tz.text(i64 %arg0) nounwind {\nentry:\n  ret i64 %arg0\n}; a piece of the runtime's IR\n\
define internal i32 @tz.digits(i64 %value) nounwind {\nentry:\n  ret i32 1\n}\n\
define internal void @tz.parallel.chunk.0(ptr %context, i64 %chunk) nounwind {\nentry:\n  %v = call i64 @tz.fn.M.grow(i64 %chunk)\n{chunk}  ret void\n}\n\
define internal i64 @tz.fn.M.nested(i64 %arg0) nounwind {\nentry:\n  call i32 @tsuzuri_tz_parallel(ptr @tz.parallel.chunk.0, ptr null, i64 %arg0)\n  ret i64 %arg0\n}\n\
define internal i64 @tz.fn.M.run(i64 %arg0) nounwind {\nentry:\n  %p = call ptr @tz.cache(i64 %arg0)\n  call i32 @tsuzuri_tz_parallel(ptr {operand}, ptr null, i64 %arg0)\n  ret i64 %arg0\n}\n\
@tls = internal thread_local global i64 0\ndeclare i32 @tsuzuri_tz_parallel(ptr, ptr, i64)\ndeclare void @tsuzuri_tz_yield()\ndeclare ptr @malloc(i64)\ndeclare void @free(ptr)\n";

    #[test]
    fn helpers_that_cannot_wait_keep_the_ir_of_main() {
        let program = |chunk: &str, operand: &str| {
            PARALLEL_PROGRAM
                .replace("{chunk}", chunk)
                .replace("{operand}", operand)
        };
        let reason = |ir: &str| {
            let (lines, functions) = runtime_definitions(ir);
            helper_wait(&lines, &functions).map(|line| line.trim().to_owned())
        };
        // A chunk that allocates, fills, frees and calls a defined function after a closing brace
        // with a comment: it runs to its end on one thread, and the caller of `Parallel.*` is a
        // pseudo-G, so the IR stays as it is.
        let pure = program("", "@tz.parallel.chunk.0");
        assert_eq!(reason(&pure), None);
        assert_eq!(with_thread_storage_noinline(pure.clone()), pure);
        // Each of these may wait, or the analysis cannot tell: the function that reads thread
        // storage becomes `noinline`.
        let nested = "  %n = call i64 @tz.fn.M.nested(i64 %chunk)\n";
        let indirect = "  %f = load ptr, ptr %context\n  call void %f(ptr %context)\n";
        let assembly = "  call void asm sideeffect \"yield\", \"\"()\n";
        let external = "  call void @tsuzuri_tz_yield()\n";
        let invoke = "  invoke void @tz.text(i64 0) to label %a unwind label %b\n";
        let stored = "  store ptr @tsuzuri_tz_parallel, ptr %context\n";
        for (ir, line) in [
            (
                program(nested, "@tz.parallel.chunk.0"),
                "call i32 @tsuzuri_tz_parallel(ptr @tz.parallel.chunk.0, ptr null, i64 %arg0)",
            ),
            (
                program(indirect, "@tz.parallel.chunk.0"),
                "call void %f(ptr %context)",
            ),
            (
                program(assembly, "@tz.parallel.chunk.0"),
                "call void asm sideeffect \"yield\", \"\"()",
            ),
            (
                program(external, "@tz.parallel.chunk.0"),
                "call void @tsuzuri_tz_yield()",
            ),
            (
                program(invoke, "@tz.parallel.chunk.0"),
                "invoke void @tz.text(i64 0) to label %a unwind label %b",
            ),
            (
                program(stored, "@tz.parallel.chunk.0"),
                "store ptr @tsuzuri_tz_parallel, ptr %context",
            ),
            (
                program("", "%chunk"),
                "call i32 @tsuzuri_tz_parallel(ptr %chunk, ptr null, i64 %arg0)",
            ),
            (
                program("", "@tz.missing"),
                "call i32 @tsuzuri_tz_parallel(ptr @tz.missing, ptr null, i64 %arg0)",
            ),
        ] {
            assert_eq!(reason(&ir).as_deref(), Some(line), "{ir}");
            assert_eq!(
                with_thread_storage_noinline(ir.clone()),
                ir.replace(
                    "@tz.cache(i64 %size) nounwind {",
                    "@tz.cache(i64 %size) nounwind noinline {"
                )
            );
        }
    }

    /// IR joined piece by piece, where a piece that ends without a newline puts the next piece on
    /// the line of its last closing brace (the runtime's pieces did so before TZ15: character.ll,
    /// display.ll, debug.ll and recursive.ll). Here each top-level form follows a brace in that way,
    /// and `@tz.parallel.chunk.0` calls every function. `  ; <name>` marks a line of each function.
    const JOINED_PROGRAM: &str = "define internal i64 @tz.text(i64 %arg0) nounwind {\nentry:\n  ; tz.text\n  ret i64 %arg0\n\
}define internal ptr @tz.cache(i64 %size) nounwind {\nentry:\n  %t = call ptr @llvm.threadlocal.address.p0(ptr @tls)\n  ; tz.cache\n  ret ptr %t\n\
}define internal void @tz.string.copy(ptr %to, ptr %from, i64 %length) nounwind {\nentry:\n  call void @llvm.memcpy.p0.p0.i64(ptr %to, ptr %from, i64 %length, i1 false)\n  ; tz.string.copy\n  ret void\n\
}@tz.debug.newline = private unnamed_addr constant [1 x i8] c\"\\0A\"\n\
define internal i64 @tz.debug.line(i64 %arg0) nounwind {\nentry:\n  ; tz.debug.line\n  ret i64 %arg0\n\
}%tz.rec.header = type { ptr, ptr, ptr }\n\
define internal i64 @tz.rec.size(i64 %arg0) nounwind {\nentry:\n  ; tz.rec.size\n  ret i64 %arg0\n\
}  ; PM08: a piece of the runtime's IR\n\
define internal i32 @tz.integer.digits(i64 %value) nounwind {\nentry:\n  ; tz.integer.digits\n  ret i32 1\n\
}declare i32 @tsuzuri_tz_parallel(ptr, ptr, i64)\n\
define internal void @tz.closure.drop(ptr %env) nounwind {\nentry:\n  ; tz.closure.drop\n  ret void\n\
}attributes #0 = { nounwind }\n\
define internal void @tz.parallel.chunk.0(ptr %context, i64 %chunk) nounwind {\nentry:\n  %p = call ptr @tz.cache(i64 %chunk)\n  call void @tz.string.copy(ptr %p, ptr %context, i64 8)\n  %a = call i64 @tz.debug.line(i64 %chunk)\n  %b = call i64 @tz.rec.size(i64 %a)\n  %c = call i32 @tz.integer.digits(i64 %b)\n  call void @tz.closure.drop(ptr %context)\n  %d = call i64 @tz.text(i64 %chunk)\n  ret void\n}\n\
define internal i64 @tz.fn.M.run(i64 %arg0) nounwind {\nentry:\n  call i32 @tsuzuri_tz_parallel(ptr @tz.parallel.chunk.0, ptr null, i64 %arg0)\n  ret i64 %arg0\n}\n\
@tls = internal thread_local global i64 0\ndeclare void @tsuzuri_tz_yield()\n";

    #[test]
    fn runtime_pieces_after_a_closing_brace_are_functions_of_their_own() {
        // The lines, each function's name and the text from its header to its closing brace, and
        // the line that makes `helpers_may_wait` true.
        let pieces = |ir: &str| -> (String, Vec<(String, String)>, Option<String>) {
            let (lines, functions) = runtime_definitions(ir);
            (
                lines.concat(),
                functions
                    .iter()
                    .map(|function| {
                        let text = lines[function.header..=function.close].concat();
                        (function.name.to_owned(), text)
                    })
                    .collect(),
                helper_wait(&lines, &functions).map(|line| line.trim().to_owned()),
            )
        };
        // The lines make the IR again, and every function is found and closes at its own brace.
        let (text, functions, reason) = pieces(JOINED_PROGRAM);
        assert_eq!(text, JOINED_PROGRAM);
        for (name, text) in &functions {
            assert!(text.starts_with("define internal "), "{text}");
            assert!(text.trim_end().ends_with("\n}"), "{text}");
            assert_eq!(text.matches("define ").count(), 1, "{text}");
            if name.starts_with("@tz.parallel.") || name.starts_with("@tz.fn.") {
                continue;
            }
            assert!(text.contains(&format!("  ; {}\n", &name[1..])), "{text}");
        }
        assert_eq!(
            functions
                .iter()
                .map(|(name, _)| name.as_str())
                .collect::<Vec<_>>(),
            [
                "@tz.text",
                "@tz.cache",
                "@tz.string.copy",
                "@tz.debug.line",
                "@tz.rec.size",
                "@tz.integer.digits",
                "@tz.closure.drop",
                "@tz.parallel.chunk.0",
                "@tz.fn.M.run",
            ]
        );
        // The chunk reaches only those functions and an intrinsic, and nothing waits: the IR stays
        // as it is.
        assert_eq!(reason, None);
        assert_eq!(
            with_thread_storage_noinline(JOINED_PROGRAM.to_owned()),
            JOINED_PROGRAM
        );
        // A wait in any of them, before or after a brace that a piece follows, is seen, and the
        // header of `@tz.cache`, which a closing brace precedes, gets the `noinline`.
        for name in [
            "tz.text",
            "tz.cache",
            "tz.string.copy",
            "tz.debug.line",
            "tz.rec.size",
            "tz.integer.digits",
            "tz.closure.drop",
        ] {
            let marker = format!("  ; {name}\n");
            let wait = format!("  call void @tsuzuri_tz_yield() ; {name}\n");
            let ir = JOINED_PROGRAM.replace(&marker, &wait);
            assert_ne!(ir, JOINED_PROGRAM);
            assert_eq!(pieces(&ir).2.as_deref(), Some(wait.trim()), "{name}");
            assert_eq!(
                with_thread_storage_noinline(ir.clone()),
                ir.replace(
                    "}define internal ptr @tz.cache(i64 %size) nounwind {",
                    "}define internal ptr @tz.cache(i64 %size) nounwind noinline {"
                ),
                "{name}"
            );
        }
    }
}
