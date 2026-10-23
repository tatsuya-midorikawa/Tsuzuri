use super::*;
use std::collections::VecDeque;

fn failure(span: Span, message: impl Into<String>) -> Diagnostic {
    Diagnostic::new("E1029", message, span)
}

fn opaque_input(ty: &Type, types: &TypeContext<'_>) -> bool {
    types.reaches_closed(
        ty,
        |part| {
            matches!(part, Type::Function(parameters, _) if !parameters.pure)
                || matches!(
                    part,
                    Type::Dyn(_) | Type::Task(_) | Type::Reference(_, true)
                )
        },
        |part| {
            matches!(part, Type::Record(id, _) if types.records[*id].name == "IO.IO"
        && types.records[*id].origin == ModuleOrigin::Std)
        },
    )
}

pub(super) fn check_function_type(
    ty: &Type,
    types: &TypeContext<'_>,
    span: Span,
) -> Result<(), Diagnostic> {
    let Type::Function(parameters, _) = ty else {
        unreachable!()
    };
    for parameter in parameters.iter() {
        if opaque_input(parameter, types) {
            return Err(failure(
                span,
                "a pure function type takes only shared data and pure callbacks; mutable references, ordinary callbacks, dyn and task inputs are not supported",
            ));
        }
        if !matches!(parameter, Type::Reference(..))
            && own_violation_with(parameter, types, true).is_some()
        {
            return Err(failure(
                span,
                "a value parameter of a pure function type must be plain data; use a shared reference",
            ));
        }
    }
    Ok(())
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
    own_violation_with(ty, types, false)
}

fn own_violation_with(ty: &Type, types: &TypeContext<'_>, template: bool) -> Option<Type> {
    let found = std::cell::RefCell::new(None);
    types.stored_all(ty, |part| {
        let plain = plain_part(part, types)
            || (template
                && matches!(
                    part,
                    Type::Variable(_) | Type::Application(..) | Type::FixedArray(..)
                ));
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
    } else if owned {
        own_violation_with(ty, types, true).map(|part| format!(
            "an 'own' parameter of a pure def must be plain data; '{}' holds {}; take it with 'ref'",
            ty.display(types), what(&part, types)
        ))
    } else if opaque_input(ty, types) {
        Some(format!(
            "a pure def cannot take a value that holds a function, 'dyn', or a task ('{}'); pass the data it needs instead",
            ty.display(types)
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
    Io,
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
            Self::Io => "may read or write the outside world by executing an I/O action".into(),
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
    templates: bool,
    trusted: BTreeSet<usize>,
}

fn position(span: Span) -> (usize, usize, usize) {
    (span.source.unwrap_or(usize::MAX), span.start, span.end)
}

struct CallTarget {
    function: FunctionRef,
    captures: usize,
    direct: bool,
    staged: bool,
}

fn closed_callable<'a>(
    expression: &'a TypedExpr,
    aliases: &BTreeMap<usize, &'a TypedExpr>,
    trusted: &BTreeSet<usize>,
    templates: bool,
    nodes: &mut usize,
    maximum: usize,
    active: &mut Vec<&'a TypedExpr>,
) -> Result<Option<Vec<CallTarget>>, LimitExceeded> {
    let staged =
        expression.ty.function_purity() == Some(true) || matches!(expression.ty, Type::Task(_));
    let mut pending = vec![(expression, true, staged)];
    let mut locals = BTreeSet::new();
    let mut targets = Vec::new();
    while let Some((value, direct, staged)) = pending.pop() {
        *nodes += 1;
        if *nodes > maximum {
            return Err(LimitExceeded);
        }
        let mut ty = &value.ty;
        while let Type::Reference(inner, false) = ty {
            ty = inner;
        }
        if !staged
            && matches!(ty, Type::Function(parameters, result)
            if parameters.pure && result.function_purity() == Some(false))
        {
            return Ok(None);
        }
        use TypedExprKind as K;
        match &value.kind {
            K::Function(function) => targets.push(CallTarget {
                function: function.clone(),
                captures: 0,
                direct,
                staged,
            }),
            K::Closure(id, captures) => {
                targets.push(CallTarget {
                    function: FunctionRef::User(*id),
                    captures: captures.len(),
                    direct,
                    staged,
                });
                pending.extend(
                    captures
                        .iter()
                        .filter(|capture| capture.ty.function_purity().is_some())
                        .map(|capture| {
                            (capture, false, capture.ty.function_purity() == Some(true))
                        }),
                );
            }
            K::GenericFunction(id, _) if templates => targets.push(CallTarget {
                function: FunctionRef::User(*id),
                captures: 0,
                direct,
                staged,
            }),
            K::Lambda { body, .. } if templates => active.push(body),
            K::Method(..) | K::TypeFunction { .. } if templates => {}
            K::Local(id) if trusted.contains(id) || value.ty.function_purity() == Some(true) => {}
            K::Local(id) if locals.insert(*id) => {
                let Some(value) = aliases.get(id) else {
                    return Ok(None);
                };
                pending.push((value, direct, staged));
            }
            K::Cast(value)
            | K::Borrow(value, _)
            | K::BorrowOperand(value)
            | K::Dereference(value) => pending.push((value, direct, staged)),
            K::Block { result, .. } => pending.push((result, direct, staged)),
            K::If {
                then_branch,
                else_branch,
                ..
            } => {
                pending.push((then_branch, direct, staged));
                pending.push((else_branch, direct, staged));
            }
            _ if value.ty.function_purity() == Some(true) => {}
            _ => return Ok(None),
        }
    }
    Ok(Some(targets))
}

fn standard_io(module: &CheckedModule, ty: &Type) -> bool {
    matches!(ty, Type::Record(id, _) if module.records[*id].name == "IO.IO"
        && module.records[*id].origin == ModuleOrigin::Std)
}

fn io_function(function: &CheckedFunction) -> bool {
    function.module == "IO" && function.origin.module == ModuleOrigin::Std
}

fn unproven_call_stage(mut ty: &Type, mut arguments: usize) -> bool {
    loop {
        while let Type::Reference(inner, false) = ty {
            ty = inner;
        }
        let Type::Function(parameters, result) = ty else {
            return true;
        };
        if arguments <= parameters.len() {
            return false;
        }
        if parameters.is_empty() || result.function_purity() != Some(true) {
            return true;
        }
        arguments -= parameters.len();
        ty = result;
    }
}

impl<'a> Scanner<'a> {
    fn new(module: &'a CheckedModule, limits: Limits) -> Self {
        Self {
            module,
            limits,
            nodes: 0,
            scanned: BTreeMap::new(),
            drops: BTreeMap::new(),
            templates: false,
            trusted: BTreeSet::new(),
        }
    }

    fn targets(
        &mut self,
        targets: Vec<CallTarget>,
        arguments: Option<usize>,
        edges: &mut Vec<(usize, Span)>,
        span: Span,
    ) -> bool {
        for target in targets {
            let FunctionRef::User(id) = target.function else {
                if matches!(target.function, FunctionRef::Builtin(instance) if builtin_impure(instance.builtin) && !closed_task_operation(instance.builtin))
                {
                    return false;
                }
                continue;
            };
            let function = &self.module.functions[id];
            let supplied = target.captures
                + if target.direct {
                    arguments.unwrap_or(0)
                } else {
                    0
                };
            if arguments.is_none()
                && !target.staged
                && function.signature.result.function_purity() == Some(false)
            {
                return false;
            }
            if supplied > function.signature.parameters.len()
                && function.signature.result.function_purity() != Some(true)
            {
                return false;
            }
            if function
                .signature
                .parameters
                .iter()
                .skip(supplied)
                .any(|ty| ty.function_purity() == Some(false))
            {
                return false;
            }
            self.trusted.insert(id);
            edges.push((id, span));
        }
        true
    }

    fn scan(&mut self, id: usize) -> Result<(), LimitExceeded> {
        if self.scanned.contains_key(&id) {
            return Ok(());
        }
        if self.scanned.len() >= self.limits.functions {
            return Err(LimitExceeded);
        }
        let function = &self.module.functions[id];
        let mut effect = (io_function(function)
            && function.name.split(".$mono.").next() == Some("run"))
        .then_some((Effect::Io, function.span));
        for parameter in function.parameters.iter().take(function.capture_count) {
            if !self.module.types().stored_all(&parameter.ty, |part| {
                !part.has_user_drop(&self.module.types())
            }) {
                effect = Some((
                    Effect::UserDrop(parameter.ty.display(&self.module.types())),
                    parameter.span,
                ));
            }
        }
        let mut edges = Vec::new();
        let mut aliases = BTreeMap::new();
        let mut read_places = BTreeSet::new();
        let trusted = function
            .parameters
            .iter()
            .filter(|parameter| {
                self.trusted.contains(&id)
                    && !parameter.mutable
                    && parameter.ty.function_purity().is_some()
            })
            .map(|parameter| parameter.id)
            .collect();
        let types = self.module.types();
        let mut pending = vec![&self.module.functions[id].body];
        while let Some(expression) = pending.pop() {
            self.nodes += 1;
            if self.nodes > self.limits.nodes {
                return Err(LimitExceeded);
            }
            if let TypedExprKind::Block { bindings, .. } = &expression.kind {
                for (local, value) in bindings {
                    if !local.mutable {
                        aliases.insert(local.id, value);
                    }
                }
            }
            let place = match &expression.kind {
                TypedExprKind::Field(value, _)
                | TypedExprKind::Index(value, _)
                | TypedExprKind::Length(value)
                | TypedExprKind::Borrow(value, _)
                | TypedExprKind::BorrowOperand(value) => Some(value.as_ref()),
                _ => None,
            };
            if let Some(place) = place
                && matches!(
                    place.kind,
                    TypedExprKind::Local(_)
                        | TypedExprKind::Dereference(_)
                        | TypedExprKind::Field(..)
                        | TypedExprKind::Index(..)
                )
            {
                read_places.insert(place as *const TypedExpr);
            }
            let has_drop = !read_places.contains(&(expression as *const TypedExpr))
                && *self.drops.entry(expression.ty.clone()).or_insert_with(|| {
                    !types.stored_all(&expression.ty, |part| !part.has_user_drop(&types))
                });
            let found = if has_drop {
                Some(Effect::UserDrop(expression.ty.display(&types)))
            } else {
                use TypedExprKind as K;
                match &expression.kind {
                    K::HostCall(import, _) => Some(Effect::Host(import.native_symbol.clone())),
                    K::TaskRun(_) | K::TaskParallel(_) | K::TaskParallelResults(_) => None,
                    K::Parallel(_, arguments) => {
                        let mut closed = true;
                        for argument in arguments
                            .iter()
                            .filter(|argument| argument.ty.function_purity().is_some())
                        {
                            let targets = closed_callable(
                                argument,
                                &aliases,
                                &trusted,
                                self.templates,
                                &mut self.nodes,
                                self.limits.nodes,
                                &mut pending,
                            )?;
                            closed &= targets.is_some_and(|targets| {
                                self.targets(targets, None, &mut edges, argument.span)
                            });
                        }
                        (!closed).then_some(Effect::Unresolved)
                    }
                    K::DynDispatch { .. } => Some(Effect::Dyn),
                    K::Call(callee, arguments) => {
                        let targets = closed_callable(
                            callee,
                            &aliases,
                            &trusted,
                            self.templates,
                            &mut self.nodes,
                            self.limits.nodes,
                            &mut pending,
                        )?;
                        let deferred = targets.as_ref().is_some_and(|targets| !targets.is_empty() && targets.iter().all(|target| {
                            matches!(target.function, FunctionRef::User(id) if io_function(&self.module.functions[id])
                                && !self.module.functions[id].name.starts_with('$')
                                && standard_io(self.module, &self.module.functions[id].signature.result))
                        }));
                        let builtin_effect_of = |targets: &Option<Vec<CallTarget>>| {
                            targets.as_ref().and_then(|targets| {
                                targets.iter().find_map(|target| {
                                    if let FunctionRef::Builtin(instance) = &target.function {
                                        builtin_effect(instance.builtin)
                                            .filter(|_| !closed_task_operation(instance.builtin))
                                            .map(|category| {
                                                Effect::Builtin(instance.builtin.name(), category)
                                            })
                                    } else {
                                        None
                                    }
                                })
                            })
                        };
                        let mut call_effect = builtin_effect_of(&targets);
                        let mut closed =
                            targets.is_some() && !unproven_call_stage(&callee.ty, arguments.len());
                        if let Some(targets) = targets {
                            closed &= self.targets(
                                targets,
                                Some(arguments.len()),
                                &mut edges,
                                expression.span,
                            );
                        }
                        for argument in arguments {
                            if argument.ty.function_purity().is_some() {
                                if deferred {
                                    continue;
                                }
                                let targets = closed_callable(
                                    argument,
                                    &aliases,
                                    &trusted,
                                    self.templates,
                                    &mut self.nodes,
                                    self.limits.nodes,
                                    &mut pending,
                                )?;
                                call_effect = call_effect.or_else(|| builtin_effect_of(&targets));
                                closed &= targets.is_some();
                                if let Some(targets) = targets {
                                    closed &=
                                        self.targets(targets, None, &mut edges, argument.span);
                                }
                            }
                        }
                        call_effect.or_else(|| (!closed).then_some(Effect::Unresolved))
                    }
                    K::GenericFunction(..) if self.templates => None,
                    K::Method(..) | K::TypeFunction { .. } | K::Lambda { .. } if self.templates => {
                        None
                    }
                    K::Error
                    | K::GenericFunction(..)
                    | K::Method(..)
                    | K::TypeFunction { .. }
                    | K::Lambda { .. }
                    | K::CaseConstructor { .. } => Some(Effect::Unresolved),
                    K::Function(_) | K::Closure(..) if matches!(expression.ty, Type::Task(_)) => {
                        let targets = closed_callable(
                            expression,
                            &aliases,
                            &trusted,
                            self.templates,
                            &mut self.nodes,
                            self.limits.nodes,
                            &mut pending,
                        )?;
                        (!targets.is_some_and(|targets| {
                            self.targets(targets, None, &mut edges, expression.span)
                        }))
                        .then_some(Effect::Unresolved)
                    }
                    K::Function(_) | K::Closure(..) => None,
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
            if !matches!(expression.kind, TypedExprKind::Lambda { .. }) {
                pending.extend(expression.children());
            } else if matches!(expression.ty, Type::Task(_))
                && let TypedExprKind::Lambda { body, .. } = &expression.kind
            {
                pending.push(body);
            }
        }
        edges.sort_by_key(|(id, span)| (position(*span), *id));
        edges.dedup();
        self.scanned.insert(id, Scanned { effect, edges });
        Ok(())
    }

    fn witness(&mut self, root: usize) -> Result<Option<(Effect, Impurity)>, LimitExceeded> {
        let function = &self.module.functions[root];
        if function
            .signature
            .parameters
            .iter()
            .skip(if self.trusted.contains(&root) {
                function.capture_count
            } else {
                0
            })
            .any(|ty| opaque_input(ty, &self.module.types()))
        {
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

pub(super) fn validate_templates(module: &CheckedModule) -> Vec<Diagnostic> {
    validate_mode(module, LIMITS, true)
}

fn validate_with(module: &CheckedModule, limits: Limits) -> Vec<Diagnostic> {
    validate_mode(module, limits, false)
}

fn validate_mode(module: &CheckedModule, limits: Limits, templates: bool) -> Vec<Diagnostic> {
    let mut scanner = Scanner::new(module, limits);
    scanner.templates = templates;
    let mut diagnostics = Vec::new();
    let mut roots: BTreeMap<usize, Span> = module
        .functions
        .iter()
        .enumerate()
        .filter(|(_, function)| {
            function.origin.pure
                && (!templates
                    || !function.type_parameters.is_empty()
                    || !function.constraints.is_empty())
        })
        .map(|(id, function)| (id, function.span))
        .collect();
    if !templates {
        let mut nodes = 0;
        for function in module
            .functions
            .iter()
            .filter(|function| function.origin.pure_values)
        {
            let mut pending = vec![&function.body];
            let mut aliases = BTreeMap::new();
            while let Some(expression) = pending.pop() {
                nodes += 1;
                if nodes > limits.nodes {
                    diagnostics.push(Diagnostic::new("E1017", "pure function value validation exceeds the expression analysis limit; split the program", expression.span));
                    return diagnostics;
                }
                if let TypedExprKind::Block { bindings, .. } = &expression.kind {
                    aliases.extend(
                        bindings
                            .iter()
                            .filter(|(local, _)| !local.mutable)
                            .map(|(local, value)| (local.id, value)),
                    );
                }
                if expression.ty.function_purity() == Some(true) {
                    match &expression.kind {
                        TypedExprKind::Function(FunctionRef::User(id))
                        | TypedExprKind::Closure(id, _) => {
                            let captures = match &expression.kind {
                                TypedExprKind::Closure(_, captures) => captures.len(),
                                _ => 0,
                            };
                            let mut qualified = &expression.ty;
                            while let Type::Reference(inner, false) = qualified {
                                qualified = inner;
                            }
                            if matches!(qualified, Type::Function(parameters, _) if parameters.len() + captures != module.functions[*id].signature.parameters.len())
                            {
                                diagnostics.push(failure(expression.span, "a returned function has a separate effect contract; qualify each call stage explicitly"));
                                continue;
                            }
                            let targets = closed_callable(
                                expression,
                                &aliases,
                                &BTreeSet::new(),
                                false,
                                &mut nodes,
                                limits.nodes,
                                &mut pending,
                            );
                            let mut edges = Vec::new();
                            let captures_closed = match targets {
                                Ok(Some(targets)) => {
                                    scanner.targets(targets, None, &mut edges, expression.span)
                                }
                                _ => false,
                            };
                            if !captures_closed {
                                diagnostics.push(failure(expression.span, "this pure function value captures a callback whose effects are unknown"));
                                continue;
                            }
                            roots.entry(*id).or_insert(expression.span);
                        }
                        TypedExprKind::Function(FunctionRef::Builtin(instance)) => {
                            if let Some(category) = builtin_effect(instance.builtin) {
                                diagnostics.push(failure(
                                expression.span,
                                format!(
                                    "this builtin cannot satisfy a pure function contract: it {}",
                                    Effect::Builtin(instance.builtin.name(), category).reason()
                                ),
                            ));
                            }
                        }
                        _ => {}
                    }
                }
                pending.extend(expression.children());
            }
        }
    }
    for (id, span) in roots {
        let function = &module.functions[id];
        if !templates
            && let Some((index, part)) = function
                .signature
                .parameters
                .iter()
                .enumerate()
                .filter(|(index, ty)| {
                    !matches!(ty, Type::Reference(..))
                        && !(*index < function.capture_count && ty.function_purity().is_some())
                })
                .find_map(|(index, ty)| {
                    own_violation(ty, &module.types()).map(|part| (index, part))
                })
        {
            diagnostics.push(failure(
                function.parameters[index].span,
                format!(
                    "an 'own' parameter of a pure specialization must be plain data; found {}",
                    what(&part, &module.types())
                ),
            ));
            continue;
        }
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
                    span,
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
