//! The effects of the facts (G26 phase 3), Level B: without A20's effect summary, the effects that
//! the built-in functions of a body name (appendix F1, the impure built-ins of A19 mapped to the
//! D-53 vocabulary), `extern` for a host call, `task` for a task or parallel node and `user_drop`
//! for a value with a user `Drop`, carried along the references between the final functions. The
//! vocabulary covers only these; `alloc`, `trap`, `copy` and `diverge` need A20, so
//! `effects_complete` is `false`.

use std::collections::{BTreeMap, VecDeque};

use crate::check::{Builtin, CheckedModule, FunctionRef, Type, TypedExprKind};
use crate::diagnostic::Span;

/// The number of built-in functions that have an effect in Level B.
#[cfg(test)]
const IMPURE_BUILTINS: usize = 90;

/// The D-53 effect of a built-in function in Level B, or `None` (every built-in by name).
pub(crate) fn builtin_effect(builtin: Builtin) -> Option<&'static str> {
    use Builtin::*;
    match builtin {
        DebugPrintString | AllocCounter | BenchConsume => Some("debug"),
        IOReadLine | IOWrite | AsyncResume | AsyncTake | AsyncPut | AsyncNextId | AsyncClock
        | AsyncWait | AsyncPosted | AsyncRetire => Some("io"),
        OsRead | OsArgs | OsWrite | OsRandom | OsClock | OsSleep | OsOpen | OsHandle | OsClose
        | OsSpawn | BenchNow | GenSeed => Some("os"),
        NetResolve | NetOpen | NetAccept | NetRead | NetWrite | NetClose | NetClassify
        | NetWatch | NetUnwatch | NetConnect | NetNames | NetSend => Some("net"),
        TaskRun | TaskParallel | TaskParallelResults | TaskScope | ParallelInit | ParallelMap
        | ParallelMapRef | ParallelReduce | ParallelForEachChunk => Some("task"),
        AtomicCreate
        | AtomicLoad
        | AtomicStore
        | AtomicSwap
        | AtomicCompareExchange
        | AtomicFetchAdd
        | AtomicFetchSub
        | AtomicFetchAnd
        | AtomicFetchOr
        | AtomicFetchXor
        | AtomicIntoInner
        | MutexCreate
        | MutexWith
        | MutexIntoInner
        | ChannelBounded
        | ChannelSend
        | ChannelRecv
        | ChannelCloneSender
        | ChannelCloseSender
        | ChannelCloseReceiver
        | ArenaNextId => Some("atomic"),
        RcNew | RcShare | RcGet | RcStrongCount | RcWeakCount | RcTryUnwrap | RcDowngrade
        | RcUpgrade | RcPtrEq | ArcNew | ArcShare | ArcGet | ArcStrongCount | ArcWeakCount
        | ArcTryUnwrap | ArcDowngrade | ArcUpgrade | ArcPtrEq => Some("rc"),
        GpuOpen | GpuFeatures | GpuRun | GpuSelect | GpuLast => Some("gpu"),
        Sqrt
        | Floor
        | Ceil
        | Abs
        | MathSqrt
        | MathFloor
        | MathCeil
        | MathTrunc
        | MathRound
        | MathRoundEven
        | MathAbs
        | MathMin
        | MathMax
        | MathClamp
        | MathFma
        | MathCopysign
        | MathIsNan
        | MathIsInfinite
        | MathIsFinite
        | MathPi
        | MathE
        | MathSin
        | MathCos
        | MathTan
        | MathAsin
        | MathAcos
        | MathAtan
        | MathAtan2
        | MathExp
        | MathExp2
        | MathLog
        | MathLog2
        | MathLog10
        | MathPow
        | MathCbrt
        | MathHypot
        | ToFloat
        | ToInt
        | Assert
        | CloneString
        | CloneUtf8String
        | StringFromUtf8
        | Utf8StringFromString
        | StringIsWellFormed
        | StringToWellFormed
        | Unreachable
        | ToString
        | UnicodeTableLength
        | UnicodeTableEntry
        | Display
        | Parse
        | Default
        | Hash
        | HashMix
        | DisplayQuoted
        | ArraySet
        | ArrayUpdate
        | ArraySwap
        | ArrayWrite
        | ArraySwapIn
        | ArraySplitAtMut
        | FixedArrayInit
        | DynOf
        | ListCons
        | ListTail
        | ArrayConcat
        | ArrayToList
        | ArraySortBy
        | ListMap
        | ListMapRef
        | ListReverse
        | ListToArray
        | ListFoldRef
        | VecEmpty
        | SeqNext
        | SimdSplat
        | SimdOfLanes2
        | SimdOfLanes4
        | SimdOfLanes8
        | SimdOfLanes16
        | SimdOfLanes32
        | SimdExtract
        | SimdReplace
        | SimdLoad
        | SimdStore
        | SimdSum
        | SimdEq
        | SimdNe
        | SimdLt
        | SimdLe
        | SimdGt
        | SimdGe
        | SimdSelect
        | SimdAll
        | SimdAny
        | VecWithCapacity
        | VecLength
        | VecCapacity
        | VecIsEmpty
        | VecPush
        | VecPushMut
        | VecPop
        | VecReserve
        | VecTruncate
        | VecClear
        | VecSet
        | VecSwap
        | VecGet
        | VecAt
        | VecClone
        | VecOfArray
        | VecToArray
        | VecView
        | VecSetCapacity
        | VecInsert
        | VecSplice
        | CharToU16
        | CharOfU16
        | Utf8CharToU32
        | Utf8CharOfU32
        | Utf8CharOfU32Unchecked
        | StringToCodeUnits
        | StringFromCodeUnits
        | Utf8StringToBytes
        | Utf8StringFromBytes
        | Utf8StringDecodeAt
        | StringCompare
        | Utf8StringCompare
        | IntMin
        | IntMax
        | IntClamp
        | IntAbs
        | IntUnsignedAbs
        | IntAbsDiff
        | IntCountOnes
        | IntLeadingZeros
        | IntTrailingZeros
        | IntRotateLeft
        | IntRotateRight
        | IntSwapBytes
        | IntReverseBits
        | IntIsPowerOfTwo
        | IntCheckedAdd
        | IntCheckedSub
        | IntCheckedMul
        | IntCheckedDiv
        | IntCheckedRem
        | IntCheckedNeg
        | IntSaturatingAdd
        | IntSaturatingSub
        | IntSaturatingMul
        | IntWrappingPow
        | IntCheckedPow
        | IntWideningMul
        | OwnedDrop
        | OwnedFunction
        | OwnedCall
        | Not
        | Ignore => None,
        #[cfg(test)]
        TestAdd | TestUnsigned | TestWiden => None,
    }
}

/// Where a final function shows an effect: the expression that has it directly (`next` is
/// `None`), or the reference to `next`, the next function on a shortest chain to one that has it
/// directly. The chain is a list of links, so that a long call graph does not copy it into every
/// caller; [`via`] spells it out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Evidence {
    pub(crate) span: Span,
    pub(crate) next: Option<usize>,
    /// The number of named functions on the whole chain (lifted lambdas and generated helpers have
    /// no name and are left out).
    pub(crate) length: usize,
}

/// What one body shows by itself.
#[derive(Default)]
pub(crate) struct Scan {
    pub(crate) direct: BTreeMap<&'static str, Span>,
    /// The functions that the body names (`Function(User)` and `Closure`), with where.
    pub(crate) references: Vec<(usize, Span)>,
    pub(crate) truncated: bool,
}

pub(crate) fn order(span: Span) -> (usize, usize, usize) {
    (span.source.unwrap_or(usize::MAX), span.start, span.end)
}

/// The direct effects and the references of the body of `function`, visiting at most `limit`
/// nodes (and taking them from `budget`).
pub(crate) fn scan(
    module: &CheckedModule,
    function: usize,
    limit: usize,
    budget: &mut usize,
    drops: &mut BTreeMap<Type, bool>,
) -> Scan {
    let types = module.types();
    let mut found = Scan::default();
    let note = |effect: &'static str, span: Span, found: &mut Scan| {
        let best = found.direct.entry(effect).or_insert(span);
        if order(span) < order(*best) {
            *best = span;
        }
    };
    let mut stack = vec![&module.functions[function].body];
    let mut visited = 0;
    while let Some(expression) = stack.pop() {
        if visited == limit || *budget == 0 {
            found.truncated = true;
            break;
        }
        visited += 1;
        *budget -= 1;
        match &expression.kind {
            TypedExprKind::Function(FunctionRef::Builtin(instance)) => {
                if let Some(effect) = builtin_effect(instance.builtin) {
                    note(effect, expression.span, &mut found);
                }
            }
            TypedExprKind::Function(FunctionRef::User(id)) => {
                found.references.push((*id, expression.span));
            }
            TypedExprKind::Closure(id, _) => found.references.push((*id, expression.span)),
            TypedExprKind::HostCall(..) => note("extern", expression.span, &mut found),
            TypedExprKind::TaskRun(_)
            | TypedExprKind::TaskParallel(_)
            | TypedExprKind::TaskParallelResults(_)
            | TypedExprKind::Parallel(..) => note("task", expression.span, &mut found),
            _ => {}
        }
        let holds_drop = *drops
            .entry(expression.ty.clone())
            .or_insert_with(|| expression.ty.holds_user_drop(&types));
        if holds_drop {
            note("user_drop", expression.span, &mut found);
        }
        stack.extend(expression.children().into_iter().rev());
    }
    found
        .references
        .sort_by_key(|(id, span)| (order(*span), *id));
    found.references.dedup();
    found
}

/// The effects of every final function with their evidence (algorithm A6): the effects that a
/// function has directly, and those of the functions it names, through any number of references.
/// `names` is the node or std ID of each final function, for the chains.
pub(crate) fn summarize(
    scans: &[Scan],
    names: &[Option<String>],
) -> Vec<BTreeMap<&'static str, Evidence>> {
    let count = scans.len();
    let mut callers: Vec<Vec<usize>> = vec![Vec::new(); count];
    for (function, found) in scans.iter().enumerate() {
        for &(callee, _) in &found.references {
            if callee < count {
                callers[callee].push(function);
            }
        }
    }
    let effects: std::collections::BTreeSet<&'static str> = scans
        .iter()
        .flat_map(|found| found.direct.keys().copied())
        .collect();
    let mut result: Vec<BTreeMap<&'static str, Evidence>> = vec![BTreeMap::new(); count];
    for effect in effects {
        // Breadth-first from the functions that have the effect directly, against the references:
        // `distance` is the length of a shortest chain to one of them.
        let mut distance = vec![usize::MAX; count];
        let mut queue = VecDeque::new();
        for (function, found) in scans.iter().enumerate() {
            if found.direct.contains_key(effect) {
                distance[function] = 0;
                queue.push_back(function);
            }
        }
        let mut order_found = Vec::new();
        while let Some(callee) = queue.pop_front() {
            order_found.push(callee);
            for &caller in &callers[callee] {
                if distance[caller] == usize::MAX {
                    distance[caller] = distance[callee] + 1;
                    queue.push_back(caller);
                }
            }
        }
        // In order of distance, so that the next function on a chain is already done.
        for function in order_found {
            let evidence = if distance[function] == 0 {
                Evidence {
                    span: scans[function].direct[effect],
                    next: None,
                    length: 0,
                }
            } else {
                let &(next, span) = scans[function]
                    .references
                    .iter()
                    .filter(|(callee, _)| {
                        *callee < count
                            && distance[*callee].checked_add(1) == Some(distance[function])
                    })
                    .min_by_key(|(callee, span)| (order(*span), *callee))
                    .expect("a function at a distance names one nearer");
                Evidence {
                    span,
                    next: Some(next),
                    length: result[next][effect].length + usize::from(names[next].is_some()),
                }
            };
            result[function].insert(effect, evidence);
        }
    }
    result
}

/// The named functions on the chain of `evidence` for `effect`, nearest first: at most `limit` of
/// them, with how many more the chain has.
pub(crate) fn via(
    summary: &[BTreeMap<&'static str, Evidence>],
    names: &[Option<String>],
    effect: &str,
    evidence: &Evidence,
    limit: usize,
) -> (Vec<String>, usize) {
    let mut chain = Vec::new();
    let mut next = evidence.next;
    while let Some(function) = next
        && chain.len() < limit
    {
        chain.extend(names[function].iter().cloned());
        next = summary[function][effect].next;
    }
    let left_out = evidence.length - chain.len();
    (chain, left_out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_builtin_has_a_verdict() {
        let impure = Builtin::ALL
            .iter()
            .filter(|builtin| builtin_effect(**builtin).is_some())
            .count();
        assert_eq!(impure, IMPURE_BUILTINS);
        assert_eq!(builtin_effect(Builtin::DebugPrintString), Some("debug"));
        assert_eq!(builtin_effect(Builtin::DynOf), None);
        assert_eq!(builtin_effect(Builtin::Sqrt), None);
    }

    /// A body stops at exactly `limit` nodes: with as many as it has, it is complete; with one
    /// fewer, it is truncated.
    #[test]
    fn honors_the_exact_node_limit() {
        let module = crate::analyze("def f :: i64 -> i64 = \\n -> n + 1\n").unwrap();
        let function = module
            .functions
            .iter()
            .position(|function| function.name == "f")
            .unwrap();
        let mut nodes = 0;
        let mut stack = vec![&module.functions[function].body];
        while let Some(expression) = stack.pop() {
            nodes += 1;
            stack.extend(expression.children());
        }
        let mut budget = usize::MAX;
        let mut drops = BTreeMap::new();
        assert!(!scan(&module, function, nodes, &mut budget, &mut drops).truncated);
        assert!(scan(&module, function, nodes - 1, &mut budget, &mut drops).truncated);
        let mut budget = nodes - 1;
        assert!(scan(&module, function, nodes, &mut budget, &mut drops).truncated);
        assert_eq!(budget, 0);
    }

    #[test]
    fn evidence_follows_a_shortest_chain() {
        let span = |start| Span {
            start,
            end: start + 1,
            source: Some(0),
        };
        // 0 names 1 (at 10) and 2 (at 20); 1 names 0 (at 5); 2 has `io` directly.
        let scans = vec![
            Scan {
                references: vec![(1, span(10)), (2, span(20))],
                ..Scan::default()
            },
            Scan {
                references: vec![(0, span(5))],
                ..Scan::default()
            },
            Scan {
                direct: BTreeMap::from([("io", span(30))]),
                ..Scan::default()
            },
        ];
        let names = vec![
            Some("A".to_owned()),
            Some("B".to_owned()),
            Some("C".to_owned()),
        ];
        let effects = summarize(&scans, &names);
        let via = |function: usize| via(&effects, &names, "io", &effects[function]["io"], 10);
        assert_eq!(effects[0]["io"].span, span(20));
        assert_eq!(via(0), (vec!["C".to_owned()], 0));
        assert_eq!(via(1), (vec!["A".to_owned(), "C".to_owned()], 0));
        assert_eq!(via(2), (Vec::new(), 0));
    }

    /// A chain of 10,000 functions keeps one link per function, and a chain names at most
    /// `limit` functions with the count of the rest; unnamed functions are not counted.
    #[test]
    fn long_chains_are_links_and_bounded() {
        let count = 10_000;
        let span = |start| Span {
            start,
            end: start + 1,
            source: Some(0),
        };
        // Function `i` names `i + 1`; the last one has `io` directly; every tenth has no name.
        let mut scans: Vec<Scan> = (0..count - 1)
            .map(|function| Scan {
                references: vec![(function + 1, span(function))],
                ..Scan::default()
            })
            .collect();
        scans.push(Scan {
            direct: BTreeMap::from([("io", span(count))]),
            ..Scan::default()
        });
        let names: Vec<Option<String>> = (0..count)
            .map(|function| (function % 10 != 0).then(|| format!("F{function}")))
            .collect();
        let effects = summarize(&scans, &names);
        // Functions 1..count, without the 999 multiples of ten among them.
        assert_eq!(effects[0]["io"].length, count - 1 - 999);
        let (chain, left_out) = via(&effects, &names, "io", &effects[0]["io"], 5);
        assert_eq!(chain, ["F1", "F2", "F3", "F4", "F5"]);
        assert_eq!(left_out, count - 1 - 999 - 5);
        let (chain, left_out) = via(&effects, &names, "io", &effects[count - 3]["io"], 5);
        assert_eq!(
            chain,
            [format!("F{}", count - 2), format!("F{}", count - 1)]
        );
        assert_eq!(left_out, 0);
    }
}
