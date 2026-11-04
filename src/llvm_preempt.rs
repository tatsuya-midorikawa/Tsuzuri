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
//! the intrinsic's result across calls).

use std::collections::{BTreeMap, BTreeSet};

use super::traps::callable;
use crate::check::{CheckedModule, FunctionRef, TypedExpr, TypedExprKind};

/// The check that the pass inserts.
pub(super) const POLL_CALL: &str = "  call void @tz.preempt.poll()\n";

/// The check's body and the runtime's two symbols (src/runtime/tzroutine.c), which the runtime
/// defines hidden in the same image (a direct load, no GOT). The request is rare, so the call is
/// weighted cold. A yield is no synchronization of the program's memory (a program has no data race,
/// D-61), so the call touches only memory that the program cannot see, and LLVM keeps the program's
/// values in registers across it; it is never removed.
const POLL_DEFINITION: &str = "@tsuzuri_tz_preempt_pending = external hidden global i32\n\
declare void @tsuzuri_tz_preempt_check() nounwind memory(inaccessiblemem: readwrite)\n\
define internal void @tz.preempt.poll() nounwind alwaysinline {\n\
entry:\n\
\x20 %pending = load atomic i32, ptr @tsuzuri_tz_preempt_pending monotonic, align 4\n\
\x20 %asked = icmp ne i32 %pending, 0\n\
\x20 br i1 %asked, label %yield, label %done, !prof !{!\"branch_weights\", i32 1, i32 1048575}\n\
yield:\n\
\x20 call void @tsuzuri_tz_preempt_check()\n\
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

/// The functions whose code is Tsuzuri source: the ones that get checks.
pub(super) fn emitted_from_source(name: &str) -> bool {
    name.starts_with("@tz.fn.") || name.starts_with("@tz.specialized.")
}

/// The functions that the signal of Phase 2 may stop: the source's, and the generated functions that
/// call them directly, into which LLVM inlines them at -O3: the adapters, the entries of tzroutines (a
/// body that is only a loop ends up in its `@tz.tzroutine.start.*`) and the chunks of `Parallel.*` and
/// `Routine.scope` (a callback that is only a loop ends up in its `@tz.parallel.chunk.*`). None of
/// them holds a runtime lock or reads thread storage itself.
fn preemptible(name: &str) -> bool {
    emitted_from_source(name)
        || name.starts_with("@tz.apply.")
        || name.starts_with("@tz.tzroutine.start.")
        || name.starts_with("@tz.parallel.chunk.")
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
    let mut thread_bound = BTreeSet::new();
    for function in &functions {
        if !emitted_from_source(function.name) {
            let body = &lines[function.header + 1..function.close];
            if body
                .iter()
                .any(|line| line.contains("@llvm.threadlocal.address"))
                && !lines[function.header].contains(" noinline")
            {
                thread_bound.insert(function.header);
            }
            continue;
        }
        if !heavies.contains(function.name) || lines[function.header].contains(" memory(") {
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
        if thread_bound.contains(&index)
            && let Some(at) = line.rfind(" {")
        {
            output.push_str(&line[..at]);
            output.push_str(" noinline");
            output.push_str(&line[at..]);
            continue;
        }
        output.push_str(line);
    }
    if !sites.is_empty() {
        if !output.ends_with('\n') {
            output.push('\n');
        }
        output.push_str(POLL_DEFINITION);
    }
    output
}

/// The section of the generated code (TZ10 Phase 2): the only code where the runtime's signal
/// handler may stop a tzroutine. Every function emitted from source goes there, after the checks
/// (whose cold call LLVM inlines into it), and so do the adapters and tzroutine entries that call
/// them (`preemptible`); the runtime's functions in IR stay in `.text`.
pub(super) const MACH_O_SECTION: &str = "__TEXT,__tz_text,regular,pure_instructions";
pub(super) const ELF_SECTION: &str = "tz_text";

pub(crate) fn with_generated_section(ir: String, mach_o: bool) -> String {
    let section = format!(
        " section \"{}\"",
        if mach_o { MACH_O_SECTION } else { ELF_SECTION }
    );
    let mut output = String::with_capacity(ir.len() + ir.len() / 64);
    for line in ir.split_inclusive('\n') {
        let header = line.trim_end();
        let at = (header.starts_with("define ")
            && header.ends_with('{')
            && !header.contains(" section \"")
            && callable(line, true).is_some_and(|call| preemptible(call.name)))
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
    fn generated_functions_go_to_their_section_and_the_runtime_stays() {
        let ir = "define internal i64 @tz.fn.M.f(i64 %arg0) nounwind \"probe-stack\"=\"inline-asm\" !dbg !7 {\nentry:\n  ret i64 0\n}\n\
define internal i64 @tz.specialized.3(i64 %arg0) nounwind {\nentry:\n  ret i64 0\n}\n\
define internal ptr @tz.alloc(i64 %size) nounwind {\nentry:\n  ret ptr null\n}\n\
define internal i8 @tz.apply.$once.4.1(ptr %env) nounwind alwaysinline {\nentry:\n  ret i8 0\n}\n\
define internal void @tz.tzroutine.start.$once.4.1(ptr %env) {\nentry:\n  ret void\n}\n\
define internal void @tz.parallel.chunk.0(ptr %context, i64 %start, i64 %end) {\nentry:\n  ret void\n}\n";
        let elf = with_generated_section(ir.to_owned(), false);
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
        let mach_o = with_generated_section(ir.to_owned(), true);
        assert_eq!(mach_o.matches(MACH_O_SECTION).count(), 5);
        assert_eq!(with_generated_section(elf.clone(), false), elf);
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
}
