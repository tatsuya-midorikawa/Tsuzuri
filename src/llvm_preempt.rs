//! The cooperative preemption checks of TZ10 Phase 1 (D2, D4): in a native program that spawns, the
//! code that the compiler emits from Tsuzuri source reads `@tsuzuri_tz_preempt_pending` and, when
//! sysmon has asked some P to give up its tzroutine, calls `tsuzuri_tz_preempt_check`.
//!
//! The check is one line, `call void @tz.preempt.poll()`, whose body is `alwaysinline`: LLVM splits
//! the block for the cold call itself, so the pass never edits the control flow or a `phi`. It goes
//!
//! - at the entry of every emitted function (`@tz.fn.*`, `@tz.specialized.*`) that is not a *leaf*,
//!   after the static `alloca`s of the entry block, and
//! - before the back edge of every loop of such a function whose blocks call something that is not a
//!   leaf.
//!
//! A leaf is a defined function that calls nothing but LLVM intrinsics and other leaves and is not in
//! a cycle of calls. LLVM may inline a leaf into a loop that is then free of calls and vectorize it,
//! so a check in a leaf, or in a loop whose only calls are leaves, would cost the vector loop that the
//! same program has without tzroutines (S2). A call that is not a leaf (an extern function, the C
//! runtime, a call through a pointer, a recursive function) stays a call after inlining, so a check
//! beside it does not change what the loop can become. Every long computation is a loop or a cycle
//! of calls: a cycle has a non-leaf entry in it, and a loop with a non-leaf call has a check at its
//! back edge. What is left without a check is a loop whose calls all inline away (D2: Phase 2).
//!
//! A call through a pointer is transparent: a closure that a loop calls is usually a constant there,
//! and LLVM turns the call into a direct one and inlines it (`Array.map` with a lambda vectorizes).
//! The recursion that only a call through a pointer closes is found in the source instead
//! (`opaque_functions`): a function that can reach itself through the functions and lambdas that it
//! names, and a `dyn` dispatcher, whose calls go through a table that LLVM cannot see through, count
//! as opaque calls.
//!
//! Loops are found from the text of each function: its blocks, the labels of their terminators,
//! the dominators, and the natural loop of each back edge (an edge to a block that dominates its
//! source). A function that `facts` marks with a memory effect is straight-line and calls only
//! intrinsics, so it is a leaf; the pass leaves it alone in any case.

use std::collections::{BTreeMap, BTreeSet};

use super::traps::callable;
use crate::check::{CheckedModule, FunctionRef, TypedExpr, TypedExprKind};

/// The check that the pass inserts.
pub(super) const POLL_CALL: &str = "  call void @tz.preempt.poll()\n";

/// The check's body and the runtime's two symbols (src/runtime/tzroutine.c). The request is rare, so
/// the call is weighted cold.
const POLL_DEFINITION: &str = "@tsuzuri_tz_preempt_pending = external global i32\n\
declare void @tsuzuri_tz_preempt_check()\n\
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

/// The functions whose code is Tsuzuri source: the ones that get checks (and, in Phase 2, the section
/// of generated code).
pub(super) fn emitted_from_source(name: &str) -> bool {
    name.starts_with("@tz.fn.") || name.starts_with("@tz.specialized.")
}

/// The symbols of the functions whose calls stay calls whatever LLVM inlines: a function in a cycle
/// of the functions and lambdas that the source names (recursion, also through a closure), and a
/// `dyn` dispatcher.
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
    /// An LLVM intrinsic, or a call through a pointer (a closure, which LLVM resolves and inlines
    /// where it is a constant): nothing that keeps a loop from becoming free of calls.
    Transparent,
    /// A function that the module defines.
    Defined(String),
    /// A declared function, an opaque function of the source, inline assembly or a form not
    /// recognized.
    Opaque,
}

fn callee(line: &str, defined: &BTreeSet<&str>, opaque: &BTreeSet<String>) -> Option<Callee> {
    let body = line.trim_start();
    if body.starts_with(';') || !body.contains("call ") {
        return None;
    }
    let Some(call) = callable(line, false) else {
        return Some(Callee::Opaque);
    };
    let name = call.name;
    Some(if name.starts_with("@llvm.") || name.starts_with('%') {
        Callee::Transparent
    } else if defined.contains(name) && !opaque.contains(name) {
        Callee::Defined(name.to_owned())
    } else {
        Callee::Opaque
    })
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

/// The defined functions that are leaves: no call but to transparent callees and leaves, and not in a
/// cycle.
fn leaves(calls: &BTreeMap<&str, Vec<Callee>>) -> BTreeSet<String> {
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
    let mut leaf = vec![false; names.len()];
    for component in components(&edges) {
        let cyclic = component.len() > 1 || edges[component[0]].contains(&component[0]);
        for &member in &component {
            leaf[member] = !cyclic
                && !calls[names[member]].contains(&Callee::Opaque)
                && edges[member].iter().all(|&target| leaf[target]);
        }
    }
    names
        .iter()
        .enumerate()
        .filter(|(id, _)| leaf[*id])
        .map(|(_, name)| (*name).to_owned())
        .collect()
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

/// One block of a function: where its terminator starts, and its successors.
struct Block<'a> {
    terminator: usize,
    successors: Vec<&'a str>,
    calls_non_leaf: bool,
}

/// The lines before which a check goes in the function between `header` and `close`: before the
/// entry block's terminator, and before the terminator of each latch of a loop with a non-leaf call.
/// `None` when the function's blocks cannot be read.
fn check_sites<'a>(
    lines: &[&'a str],
    header: usize,
    close: usize,
    non_leaf_call: &dyn Fn(&str) -> bool,
) -> Option<BTreeSet<usize>> {
    let mut names: Vec<&'a str> = Vec::new();
    let mut blocks: Vec<Block<'a>> = Vec::new();
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
            let calls_non_leaf = (start..terminator).any(|line| non_leaf_call(lines[line]));
            names.push(name);
            blocks.push(Block {
                terminator,
                successors,
                calls_non_leaf,
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
    sites.insert(blocks[0].terminator);
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
            if body.iter().any(|&node| blocks[node].calls_non_leaf) {
                sites.insert(blocks[latch].terminator);
            }
        }
    }
    // A `musttail` call must stay right before its `ret`.
    let entry = blocks[0].terminator;
    if lines[entry].trim_start().starts_with("ret")
        && entry > header + 1
        && lines[entry - 1].contains("musttail call ")
    {
        sites.remove(&entry);
        sites.insert(entry - 1);
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
            .filter_map(|line| callee(line, &defined, opaque))
            .collect();
        if opaque.contains(function.name) {
            found.push(Callee::Opaque);
        }
        calls.insert(function.name, found);
    }
    let leaves = leaves(&calls);
    let non_leaf_call = |line: &str| match callee(line, &defined, opaque) {
        None | Some(Callee::Transparent) => false,
        Some(Callee::Defined(name)) => !leaves.contains(&name),
        Some(Callee::Opaque) => true,
    };
    let mut sites = BTreeSet::new();
    for function in &functions {
        if !emitted_from_source(function.name)
            || leaves.contains(function.name)
            || lines[function.header].contains(" memory(")
        {
            continue;
        }
        if let Some(found) = check_sites(&lines, function.header, function.close, &non_leaf_call) {
            sites.extend(found);
        }
    }
    if sites.is_empty() {
        return ir;
    }
    let mut output = String::with_capacity(ir.len() + sites.len() * POLL_CALL.len() + 512);
    for (index, line) in lines.iter().enumerate() {
        if sites.contains(&index) {
            output.push_str(POLL_CALL);
        }
        output.push_str(line);
    }
    if !output.ends_with('\n') {
        output.push('\n');
    }
    output.push_str(POLL_DEFINITION);
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    fn checks(ir: &str) -> Vec<String> {
        let output = with_preemption_checks(ir.to_owned(), &BTreeSet::new());
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
    fn leaves_and_call_free_loops_get_no_check() {
        let ir = "define internal i64 @tz.fn.M.square(i64 %arg0) nounwind {\nentry:\n  br label %loop\nloop:\n  %v = mul i64 %arg0, %arg0\n  ret i64 %v\n}\n\
define internal i64 @tz.fn.M.sum(i64 %arg0) nounwind {\nentry:\n  %s = alloca i64\n  br label %loop\nloop:\n  br label %b0\nb0:\n  %x = call i64 @tz.fn.M.square(i64 %arg0)\n  %c = call { i64, i1 } @llvm.sadd.with.overflow.i64(i64 %x, i64 1)\n  br i1 true, label %b0, label %b1\nb1:\n  ret i64 %x\n}\n";
        assert!(checks(ir).is_empty());
        assert_eq!(with_preemption_checks(ir.to_owned(), &BTreeSet::new()), ir);
    }

    #[test]
    fn a_cycle_and_a_loop_with_an_opaque_call_are_checked() {
        let ir = "define internal i64 @tz.fn.M.fib(i64 %arg0) nounwind {\nentry:\n  %slot = alloca i64\n  br label %loop\nloop:\n  %a = call i64 @tz.fn.M.fib(i64 %arg0)\n  ret i64 %a\n}\n\
define internal void @tz.fn.M.spin(ptr %arg0) nounwind {\nentry:\n  br label %loop\nloop:\n  br label %b0\nb0:\n  call void @tsuzuri_tz_yield()\n  br label %b1\nb1:\n  %x = load i1, ptr %arg0\n  switch i32 0, label %b0 [\n    i32 1, label %b2\n  ]\nb2:\n  ret void\n}\n\
declare void @tsuzuri_tz_yield()\n";
        assert_eq!(
            checks(ir),
            [
                "@tz.fn.M.fib br label %loop",
                "@tz.fn.M.spin br label %loop",
                "@tz.fn.M.spin switch i32 0, label %b0 [",
            ]
        );
        let output = with_preemption_checks(ir.to_owned(), &BTreeSet::new());
        assert!(
            output
                .contains("%slot = alloca i64\n  call void @tz.preempt.poll()\n  br label %loop\n")
        );
        assert_eq!(
            output
                .matches("define internal void @tz.preempt.poll()")
                .count(),
            1
        );
    }

    #[test]
    fn a_closure_call_is_transparent_and_an_opaque_function_is_not() {
        let ir = "define internal i64 @tz.fn.M.apply(ptr %arg0) nounwind {\nentry:\n  br label %loop\nloop:\n  br label %b0\nb0:\n  %x = call i64 %arg0(i64 1)\n  %y = call i64 @tz.fn.M.walk(i64 %x)\n  br i1 true, label %b0, label %b1\nb1:\n  ret i64 %x\n}\n\
define internal i64 @tz.fn.M.walk(i64 %arg0) nounwind {\nentry:\n  br label %loop\nloop:\n  ret i64 %arg0\n}\n";
        assert!(checks(ir).is_empty());
        let opaque = BTreeSet::from(["@tz.fn.M.walk".to_owned()]);
        let output = with_preemption_checks(ir.to_owned(), &opaque);
        assert_eq!(output.matches(POLL_CALL).count(), 3, "{output}");
    }

    #[test]
    fn runtime_functions_and_marked_functions_are_left_alone() {
        let ir = "define internal ptr @tz.alloc(i64 %size) nounwind {\nentry:\n  %p = call ptr @malloc(i64 %size)\n  br label %b0\nb0:\n  br i1 true, label %b0, label %b1\nb1:\n  ret ptr %p\n}\n\
define internal i64 @tz.fn.M.pure(i64 %arg0) nounwind memory(none) {\nentry:\n  %x = call ptr @tz.alloc(i64 1)\n  ret i64 0\n}\n\
declare ptr @malloc(i64)\n";
        assert!(checks(ir).is_empty());
    }
}
