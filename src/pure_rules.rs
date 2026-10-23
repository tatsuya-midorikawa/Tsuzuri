use super::*;
use std::collections::VecDeque;

fn failure(span: Span, message: impl Into<String>) -> Diagnostic {
    Diagnostic::new("E1029", message, span)
}

fn plain_part(ty: &Type, types: &TypeContext<'_>) -> bool {
    match ty {
        Type::Integer(..)
        | Type::Binary(_)
        | Type::Decimal(_)
        | Type::Simd(_)
        | Type::Bool
        | Type::Unit
        | Type::Char
        | Type::Utf8Char
        | Type::String
        | Type::Utf8String
        | Type::Array(_)
        | Type::List(_)
        | Type::Vec(_)
        | Type::Tuple(_) => true,
        Type::FixedArray(_, length) => matches!(**length, Type::Length(_)),
        Type::Record(id, _) => !types.records[*id].user_drop,
        Type::Union(id, _) => !types.unions[*id].user_drop,
        Type::Reference(..)
        | Type::Shared(..)
        | Type::Handle(_)
        | Type::Task(_)
        | Type::Dyn(_)
        | Type::Function(..)
        | Type::ArrayView(_)
        | Type::Length(_)
        | Type::Error
        | Type::Variable(_)
        | Type::Infer(_)
        | Type::Partial(_)
        | Type::Application(..) => false,
    }
}

pub(super) fn own_violation(ty: &Type, types: &TypeContext<'_>) -> Option<Type> {
    let found = std::cell::RefCell::new(None);
    types.stored_all(ty, |part| {
        let plain = plain_part(part, types);
        if !plain {
            found.borrow_mut().get_or_insert_with(|| part.clone());
        }
        plain
    });
    found.into_inner()
}

pub(crate) fn what(ty: &Type, types: &TypeContext<'_>) -> &'static str {
    match ty {
        Type::Reference(..) => "a reference",
        Type::Shared(..) => "an 'Rc' or 'Arc'",
        Type::Handle(_) => "an extern handle",
        Type::Function(..) | Type::Dyn(_) | Type::Task(_) => "a function, 'dyn', or a task",
        _ if ty.has_user_drop(types) => "a value with a user Drop",
        _ => "a type that is not plain data",
    }
}

pub(super) fn check_parameter(
    parameter: &Parameter,
    ty: &Type,
    owned: bool,
    types: &TypeContext<'_>,
) -> Result<(), Diagnostic> {
    let message = if parameter.mutable {
        Some(
            "a pure def parameter cannot be 'mut'; bind a mutable local in the body instead".into(),
        )
    } else if owned && matches!(ty, Type::Reference(..)) {
        Some("'own' cannot apply to a reference; write 'ref T' or 'own T'".into())
    } else if matches!(ty, Type::Reference(_, true)) {
        Some(format!(
            "a pure def takes only shared references; take '{}' with 'ref' and return the new value",
            parameter.name.text
        ))
    } else if types.reaches(ty, |part| {
        matches!(part, Type::Function(..) | Type::Dyn(_) | Type::Task(_))
    }) {
        Some(format!(
            "a pure def cannot take a value that holds a function, 'dyn', or a task ('{}'); pass the data it needs instead",
            ty.display(types)
        ))
    } else if owned {
        own_violation(ty, types).map(|part| format!(
            "an 'own' parameter of a pure def must be plain data; '{}' holds {}; take it with 'ref'",
            ty.display(types), what(&part, types)
        ))
    } else {
        None
    };
    message.map_or(Ok(()), |message| Err(failure(parameter.name.span, message)))
}

pub(super) fn borrow_parameter(ty: &Type, owned: bool, types: &TypeContext<'_>) -> bool {
    !owned && !matches!(ty, Type::Reference(..)) && !(ty.is_copy(types) && !ty.needs_drop(types))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Category {
    Outside,
    Debug,
    Clock,
    Task,
    State,
    Gpu,
    Dyn,
}

fn builtin_effect(builtin: Builtin) -> Option<Category> {
    use Builtin::*;
    use Category::*;
    match builtin {
        DebugPrintString => Some(Debug),
        IOReadLine | IOWrite | OsRead | OsArgs | OsWrite | OsRandom | OsClock | OsSleep
        | OsOpen | OsHandle | OsClose | OsSpawn | NetResolve | NetOpen | NetAccept | NetRead
        | NetWrite | NetClose | NetClassify | NetWatch | NetUnwatch | NetConnect | NetNames
        | NetSend => Some(Outside),
        BenchNow | BenchConsume | GenSeed => Some(Clock),
        TaskRun | TaskParallel | TaskParallelResults | TaskScope | ParallelInit | ParallelMap
        | ParallelMapRef | ParallelReduce | ParallelForEachChunk | AsyncResume | AsyncTake
        | AsyncPut | AsyncNextId | AsyncClock | AsyncWait | AsyncPosted | AsyncRetire => Some(Task),
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
        | RcNew
        | RcShare
        | RcGet
        | RcStrongCount
        | RcWeakCount
        | RcTryUnwrap
        | RcDowngrade
        | RcUpgrade
        | RcPtrEq
        | ArcNew
        | ArcShare
        | ArcGet
        | ArcStrongCount
        | ArcWeakCount
        | ArcTryUnwrap
        | ArcDowngrade
        | ArcUpgrade
        | ArcPtrEq
        | ArenaNextId => Some(State),
        GpuOpen | GpuFeatures | GpuRun | GpuSelect | GpuLast => Some(Gpu),
        DynOf => Some(Dyn),
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

pub(crate) fn builtin_impure(builtin: Builtin) -> bool {
    builtin_effect(builtin).is_some()
}

fn closed_task_operation(builtin: Builtin) -> bool {
    matches!(
        builtin,
        Builtin::TaskRun
            | Builtin::TaskParallel
            | Builtin::TaskParallelResults
            | Builtin::TaskScope
            | Builtin::ParallelInit
            | Builtin::ParallelMap
            | Builtin::ParallelMapRef
            | Builtin::ParallelReduce
            | Builtin::ParallelForEachChunk
    )
}

#[derive(Clone, Debug)]
pub(crate) enum Effect {
    Host(String),
    Builtin(&'static str, Category),
    #[allow(dead_code)]
    Concurrency,
    Dyn,
    UserDrop(String),
    Unresolved,
}

impl Effect {
    fn reason(&self) -> String {
        match self {
            Self::Host(name) => format!("calls the extern function '{name}'"),
            Self::Builtin(name, category) => match category {
                Category::Outside => format!("reads or writes the outside world ('{name}')"),
                Category::Debug => "prints debug output".into(),
                Category::Clock => {
                    format!("reads the clock, a random source, or a benchmark counter ('{name}')")
                }
                Category::Task => {
                    format!("starts or waits for a task or a parallel operation ('{name}')")
                }
                Category::State => format!("uses shared mutable state ('{name}')"),
                Category::Gpu => format!("uses the GPU ('{name}')"),
                Category::Dyn => "uses dyn dispatch".into(),
            },
            Self::Concurrency => "starts or waits for a task or a parallel operation".into(),
            Self::Dyn => "uses dyn dispatch".into(),
            Self::UserDrop(ty) => format!("holds a value whose type has a user Drop ('{ty}')"),
            Self::Unresolved => "cannot be proven pure".into(),
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Impurity {
    pub reason: String,
    pub chain: String,
    pub span: Span,
}

#[derive(Clone, Copy)]
struct Limits {
    functions: usize,
    nodes: usize,
}

const LIMITS: Limits = Limits {
    functions: 4096,
    nodes: 2_097_152,
};

#[derive(Debug)]
pub(crate) struct LimitExceeded;

struct Scanned {
    effect: Option<(Effect, Span)>,
    edges: Vec<(usize, Span)>,
}

struct Scanner<'a> {
    module: &'a CheckedModule,
    limits: Limits,
    nodes: usize,
    scanned: BTreeMap<usize, Scanned>,
    drops: BTreeMap<Type, bool>,
}

fn position(span: Span) -> (usize, usize, usize) {
    (span.source.unwrap_or(usize::MAX), span.start, span.end)
}

impl<'a> Scanner<'a> {
    fn new(module: &'a CheckedModule, limits: Limits) -> Self {
        Self {
            module,
            limits,
            nodes: 0,
            scanned: BTreeMap::new(),
            drops: BTreeMap::new(),
        }
    }

    fn scan(&mut self, id: usize) -> Result<(), LimitExceeded> {
        if self.scanned.contains_key(&id) {
            return Ok(());
        }
        if self.scanned.len() >= self.limits.functions {
            return Err(LimitExceeded);
        }
        let mut effect = None;
        let mut edges = Vec::new();
        let types = self.module.types();
        let mut pending = vec![&self.module.functions[id].body];
        while let Some(expression) = pending.pop() {
            self.nodes += 1;
            if self.nodes > self.limits.nodes {
                return Err(LimitExceeded);
            }
            let has_drop = *self.drops.entry(expression.ty.clone()).or_insert_with(|| {
                !types.stored_all(&expression.ty, |part| !part.has_user_drop(&types))
            });
            let found = if has_drop {
                Some(Effect::UserDrop(expression.ty.display(&types)))
            } else {
                use TypedExprKind as K;
                match &expression.kind {
                    K::HostCall(import, _) => Some(Effect::Host(import.native_symbol.clone())),
                    K::TaskRun(_)
                    | K::TaskParallel(_)
                    | K::TaskParallelResults(_)
                    | K::Parallel(..) => None,
                    K::DynDispatch { .. } => Some(Effect::Dyn),
                    K::Error
                    | K::GenericFunction(..)
                    | K::Method(..)
                    | K::TypeFunction { .. }
                    | K::Lambda { .. }
                    | K::CaseConstructor { .. } => Some(Effect::Unresolved),
                    K::Function(FunctionRef::User(target)) | K::Closure(target, _) => {
                        edges.push((*target, expression.span));
                        None
                    }
                    K::Function(FunctionRef::Builtin(instance)) => {
                        if builtin_impure(instance.builtin)
                            && !closed_task_operation(instance.builtin)
                        {
                            builtin_effect(instance.builtin)
                                .map(|category| Effect::Builtin(instance.builtin.name(), category))
                        } else {
                            None
                        }
                    }
                    K::Int(_)
                    | K::Float(_)
                    | K::GenericInteger(..)
                    | K::GenericFloat(_)
                    | K::String(_)
                    | K::Bool(_)
                    | K::Unit
                    | K::Break
                    | K::Continue
                    | K::Local(_)
                    | K::Static(_)
                    | K::Unary(..)
                    | K::Binary(..)
                    | K::Call(..)
                    | K::StructuralCompare(..)
                    | K::StructuralHash(_)
                    | K::StructuralDisplay(_)
                    | K::Interpolated(_)
                    | K::If { .. }
                    | K::While { .. }
                    | K::ForRange { .. }
                    | K::ForEach { .. }
                    | K::Match { .. }
                    | K::Block { .. }
                    | K::Record(_)
                    | K::RecordUpdate { .. }
                    | K::Array(_)
                    | K::List(_)
                    | K::Tuple(_)
                    | K::ListTail(..)
                    | K::NewArray(..)
                    | K::NewList(..)
                    | K::NewLiteral(_)
                    | K::Field(..)
                    | K::Index(..)
                    | K::Slice { .. }
                    | K::Length(_)
                    | K::StringLength(_)
                    | K::Borrow(..)
                    | K::BorrowOperand(_)
                    | K::Dereference(_)
                    | K::Assign(..)
                    | K::Cast(_)
                    | K::Construct { .. }
                    | K::UnionTag(_)
                    | K::UnionPayload { .. }
                    | K::Checked(_)
                    | K::Try(_)
                    | K::RaisedException
                    | K::Reraise => None,
                }
            };
            if let Some(found) = found
                && effect
                    .as_ref()
                    .is_none_or(|(_, span)| position(expression.span) < position(*span))
            {
                effect = Some((found, expression.span));
            }
            pending.extend(expression.children());
        }
        edges.sort_by_key(|(id, span)| (position(*span), *id));
        edges.dedup();
        self.scanned.insert(id, Scanned { effect, edges });
        Ok(())
    }

    fn witness(&mut self, root: usize) -> Result<Option<(Effect, Impurity)>, LimitExceeded> {
        let function = &self.module.functions[root];
        if function.signature.parameters.iter().any(|ty| {
            self.module.types().reaches(ty, |part| {
                matches!(part, Type::Function(..) | Type::Task(_) | Type::Dyn(_))
            })
        }) {
            let effect = Effect::Unresolved;
            return Ok(Some((
                effect.clone(),
                Impurity {
                    reason: effect.reason(),
                    chain: String::new(),
                    span: function.span,
                },
            )));
        }
        let mut pending = VecDeque::from([root]);
        let mut parents = BTreeMap::from([(root, None)]);
        while let Some(id) = pending.pop_front() {
            self.scan(id)?;
            let scanned = &self.scanned[&id];
            if let Some((effect, span)) = &scanned.effect {
                let mut path = Vec::new();
                let mut current = id;
                while let Some((parent, edge)) = parents[&current] {
                    path.push((current, edge));
                    current = parent;
                }
                path.reverse();
                let outside = path
                    .iter()
                    .find(|(function, _)| !self.family(*function, root));
                let span = outside.map_or(*span, |(_, span)| *span);
                let chain = path
                    .iter()
                    .take(4)
                    .map(|(function, _)| format!("'{}'", shown(&self.module.functions[*function])))
                    .collect::<Vec<_>>()
                    .join(" -> ");
                return Ok(Some((
                    effect.clone(),
                    Impurity {
                        reason: effect.reason(),
                        chain,
                        span,
                    },
                )));
            }
            for &(target, span) in &scanned.edges {
                if let std::collections::btree_map::Entry::Vacant(entry) = parents.entry(target) {
                    entry.insert(Some((id, span)));
                    pending.push_back(target);
                }
            }
        }
        Ok(None)
    }

    fn family(&self, mut id: usize, root: usize) -> bool {
        for _ in 0..self.module.functions.len() {
            if id == root {
                return true;
            }
            let Some(parent) = self.module.functions[id].origin.parent else {
                return false;
            };
            id = parent;
        }
        false
    }
}

fn shown(function: &CheckedFunction) -> String {
    function
        .qualified_name()
        .split(".$mono.")
        .next()
        .unwrap()
        .into()
}

// D-53's query boundary is shared with B10 and F15, independently of `pure def` roots.
#[allow(dead_code)]
pub(crate) fn reachable_effect(
    module: &CheckedModule,
    root: usize,
) -> Result<Option<Effect>, LimitExceeded> {
    Scanner::new(module, LIMITS)
        .witness(root)
        .map(|witness| witness.map(|(effect, _)| effect))
}

#[allow(dead_code)]
pub(crate) fn impurity_of_roots(
    module: &CheckedModule,
    roots: &[usize],
) -> BTreeMap<usize, Impurity> {
    let mut scanner = Scanner::new(module, LIMITS);
    roots
        .iter()
        .filter_map(|&root| match scanner.witness(root) {
            Ok(Some((_, impurity))) => Some((root, impurity)),
            Ok(None) => None,
            Err(_) => Some((
                root,
                Impurity {
                    reason: "cannot be proven pure within the analysis limits".into(),
                    chain: String::new(),
                    span: module.functions[root].span,
                },
            )),
        })
        .collect()
}

pub(super) fn validate(module: &CheckedModule) -> Vec<Diagnostic> {
    validate_with(module, LIMITS)
}

fn validate_with(module: &CheckedModule, limits: Limits) -> Vec<Diagnostic> {
    let mut scanner = Scanner::new(module, limits);
    let mut diagnostics = Vec::new();
    for (id, function) in module
        .functions
        .iter()
        .enumerate()
        .filter(|(_, function)| function.origin.pure)
    {
        match scanner.witness(id) {
            Ok(None) => {}
            Ok(Some((_, impurity))) => {
                let message = if impurity.chain.is_empty() {
                    format!(
                        "this operation is not allowed in a pure def: it {}; move it to a def",
                        impurity.reason
                    )
                } else {
                    format!(
                        "a reachable function is not pure: it {} (through {}); call a pure function or move this call to a def",
                        impurity.reason, impurity.chain
                    )
                };
                diagnostics.push(failure(impurity.span, message));
            }
            Err(_) => {
                diagnostics.push(Diagnostic::new(
                    "E1017",
                    format!("a pure def reaches more than {} functions or {} expression nodes; split it", limits.functions, limits.nodes),
                    function.span,
                ));
                break;
            }
        }
    }
    diagnostics
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_classification_covers_every_production_variant() {
        assert_eq!(
            Builtin::ALL
                .iter()
                .filter(|&&builtin| builtin_impure(builtin))
                .count(),
            90
        );
        assert!(!builtin_impure(Builtin::ArrayUpdate));
    }

    #[test]
    fn bounded_analysis_rejects_instead_of_assuming_purity() {
        let mut module = crate::analyze("def f :: i64 -> i64 = \\n -> n + 1\n").unwrap();
        module
            .functions
            .iter_mut()
            .find(|function| function.name == "f")
            .unwrap()
            .origin
            .pure = true;
        for limits in [
            Limits {
                functions: 0,
                nodes: 100,
            },
            Limits {
                functions: 100,
                nodes: 0,
            },
        ] {
            assert_eq!(validate_with(&module, limits)[0].code, "E1017");
        }
    }

    #[test]
    fn shared_queries_reject_opaque_callback_and_task_inputs() {
        let module = crate::analyze(
            "def apply :: (i64 -> i64) -> i64 -> i64 = \\f n -> f n\n\
             def run :: Task<i64> -> i64 = \\job -> Task.run job\n",
        )
        .unwrap();
        for name in ["apply", "run"] {
            let root = module
                .functions
                .iter()
                .position(|function| function.name == name)
                .unwrap();
            assert!(matches!(
                reachable_effect(&module, root),
                Ok(Some(Effect::Unresolved))
            ));
            assert!(impurity_of_roots(&module, &[root]).contains_key(&root));
        }
    }
}
