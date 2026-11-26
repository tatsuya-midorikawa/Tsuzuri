//! W19: static thread confinement for `extern type local` handles.
//!
//! A function whose `Effect::Pinned` bit is set touches a thread-bound extern handle (`extern type
//! local`) directly or through unpinned calls. Such a function may be called directly, passed to a
//! thread-pinning boundary (`Routine.with_locked_thread`, `FnPtr.of`, or a callback `extern`), or
//! passed to a synchronous lock/cell helper that runs it on the calling tzroutine before returning
//! (`Mutex.with_lock`, `RwMutex.with_read`, `RwMutex.with_write`, `OnceValue.get`,
//! `Cond.wait_with`, `Anchor.read`, `Anchor.update`). Spawning it onto another tzroutine/worker
//! (`E1013` `pinned_code_spawned`) or turning it into a general function / `dyn` value (`E1013`
//! `pinned_function_value`) is rejected at compile time.
//!
//! Programs that declare no `extern type local` return immediately before starting a timing phase
//! or computing any summary.

use super::effects::{self, Effect, EffectSet, FunctionEffects};
use super::*;
use crate::diagnostic::{Diagnostic, DiagnosticKind, RelatedRole, Span};

pub(crate) const MAIN_THREAD_ADVICE: &str = "an API that needs the process main thread (AppKit on macOS) must run in 'main' or in a callback that the main thread calls";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Position {
    /// Pins the OS thread before running its callback (`Routine.with_locked_thread`, `FnPtr.of`).
    Pinning,
    /// Calls its callback synchronously on the calling tzroutine before returning and never retains
    /// or forwards it.
    Inline,
    /// Starts its callback on another tzroutine or worker thread (`pinned_code_spawned`).
    Spawned,
    /// Any other built-in: a `pinned` function value passed to it may escape (`pinned_function_value`).
    Escaping,
}

/// Classifies every `Builtin` without a wildcard arm so new built-ins must choose explicitly (W19).
pub(crate) fn builtin_position(builtin: Builtin) -> Position {
    match builtin {
        Builtin::RoutineWithLockedThread | Builtin::FnPtrOf => Position::Pinning,

        Builtin::MutexWith
        | Builtin::RwMutexWithRead
        | Builtin::RwMutexWithWrite
        | Builtin::OnceValueGet
        | Builtin::CondWaitWith
        | Builtin::AnchorRead
        | Builtin::AnchorUpdate
        | Builtin::DomainRun => Position::Inline,

        Builtin::RoutineGo
        | Builtin::WaitGroupGo
        | Builtin::RoutineScope
        | Builtin::TimeAfterFunc
        | Builtin::ContextAfterFunc
        | Builtin::ParallelInit
        | Builtin::ParallelMap
        | Builtin::ParallelMapRef
        | Builtin::ParallelReduce
        | Builtin::ParallelForEachChunk
        | Builtin::ParallelForEachChunkWith => Position::Spawned,

        Builtin::Sqrt
        | Builtin::Floor
        | Builtin::Ceil
        | Builtin::Abs
        | Builtin::MathSqrt
        | Builtin::MathFloor
        | Builtin::MathCeil
        | Builtin::MathTrunc
        | Builtin::MathRound
        | Builtin::MathRoundEven
        | Builtin::MathAbs
        | Builtin::MathMin
        | Builtin::MathMax
        | Builtin::MathClamp
        | Builtin::MathFma
        | Builtin::MathCopysign
        | Builtin::MathIsNan
        | Builtin::MathIsInfinite
        | Builtin::MathIsFinite
        | Builtin::MathPi
        | Builtin::MathE
        | Builtin::MathSin
        | Builtin::MathCos
        | Builtin::MathTan
        | Builtin::MathAsin
        | Builtin::MathAcos
        | Builtin::MathAtan
        | Builtin::MathAtan2
        | Builtin::MathExp
        | Builtin::MathExp2
        | Builtin::MathLog
        | Builtin::MathLog10
        | Builtin::MathLog2
        | Builtin::MathPow
        | Builtin::MathCbrt
        | Builtin::MathHypot
        | Builtin::MathSinh
        | Builtin::MathCosh
        | Builtin::MathTanh
        | Builtin::MathAsinh
        | Builtin::MathAcosh
        | Builtin::MathAtanh
        | Builtin::MathExpm1
        | Builtin::MathLog1p
        | Builtin::MathLogaddexp
        | Builtin::MathLogaddexp2
        | Builtin::MathSinc
        | Builtin::MathDegrees
        | Builtin::MathRadians
        | Builtin::MathFmod
        | Builtin::MathFloorDiv
        | Builtin::MathFloorMod
        | Builtin::MathIeeeRemainder
        | Builtin::MathModf
        | Builtin::MathFrexp
        | Builtin::MathLdexp
        | Builtin::MathNextafter
        | Builtin::MathSign
        | Builtin::MathSignbit
        | Builtin::ToFloat
        | Builtin::ToInt
        | Builtin::Assert
        | Builtin::ContractRequires
        | Builtin::ContractEnsures
        | Builtin::CloneString
        | Builtin::CloneUtf8String
        | Builtin::StringFromUtf8
        | Builtin::Utf8StringFromString
        | Builtin::StringIsWellFormed
        | Builtin::StringToWellFormed
        | Builtin::Unreachable
        | Builtin::ToString
        | Builtin::DebugPrintString
        | Builtin::IOReadLine
        | Builtin::IOWrite
        | Builtin::OsRead
        | Builtin::OsArgs
        | Builtin::OsWrite
        | Builtin::OsRandom
        | Builtin::OsClock
        | Builtin::OsSleep
        | Builtin::OsOpen
        | Builtin::OsHandle
        | Builtin::OsClose
        | Builtin::OsSpawn
        | Builtin::NetResolve
        | Builtin::NetOpen
        | Builtin::NetAccept
        | Builtin::NetRead
        | Builtin::NetWrite
        | Builtin::NetClose
        | Builtin::NetClassify
        | Builtin::UnicodeTableLength
        | Builtin::UnicodeTableEntry
        | Builtin::Display
        | Builtin::Parse
        | Builtin::Default
        | Builtin::Hash
        | Builtin::HashMix
        | Builtin::DisplayQuoted
        | Builtin::ArraySet
        | Builtin::ArrayUpdate
        | Builtin::ArraySwap
        | Builtin::ArrayWrite
        | Builtin::ArraySwapIn
        | Builtin::ArraySplitAtMut
        | Builtin::FixedArrayInit
        | Builtin::DynOf
        | Builtin::AnchorCreate
        | Builtin::AnchorRelease
        | Builtin::ZStringLiteral
        | Builtin::ZUtf8StringLiteral
        | Builtin::ListCons
        | Builtin::ListTail
        | Builtin::ArrayConcat
        | Builtin::ArrayToList
        | Builtin::ArraySortBy
        | Builtin::ListMap
        | Builtin::ListMapRef
        | Builtin::ListReverse
        | Builtin::ListToArray
        | Builtin::ListFoldRef
        | Builtin::VecEmpty
        | Builtin::SeqNext
        | Builtin::SimdSplat
        | Builtin::SimdOfLanes2
        | Builtin::SimdOfLanes4
        | Builtin::SimdOfLanes8
        | Builtin::SimdOfLanes16
        | Builtin::SimdOfLanes32
        | Builtin::SimdExtract
        | Builtin::SimdReplace
        | Builtin::SimdLoad
        | Builtin::SimdStore
        | Builtin::SimdSum
        | Builtin::SimdEq
        | Builtin::SimdNe
        | Builtin::SimdLt
        | Builtin::SimdLe
        | Builtin::SimdGt
        | Builtin::SimdGe
        | Builtin::SimdSelect
        | Builtin::SimdAll
        | Builtin::SimdAny
        | Builtin::VecWithCapacity
        | Builtin::VecLength
        | Builtin::VecCapacity
        | Builtin::VecIsEmpty
        | Builtin::VecPush
        | Builtin::VecPushMut
        | Builtin::VecPop
        | Builtin::VecReserve
        | Builtin::VecTruncate
        | Builtin::VecClear
        | Builtin::VecSet
        | Builtin::VecSwap
        | Builtin::VecGet
        | Builtin::VecAt
        | Builtin::VecClone
        | Builtin::VecOfArray
        | Builtin::VecToArray
        | Builtin::VecView
        | Builtin::VecSetCapacity
        | Builtin::VecInsert
        | Builtin::VecSplice
        | Builtin::CharToU16
        | Builtin::CharOfU16
        | Builtin::Utf8CharToU32
        | Builtin::Utf8CharOfU32
        | Builtin::Utf8CharOfU32Unchecked
        | Builtin::StringToCodeUnits
        | Builtin::StringFromCodeUnits
        | Builtin::Utf8StringToBytes
        | Builtin::Utf8StringFromBytes
        | Builtin::Utf8StringDecodeAt
        | Builtin::StringCompare
        | Builtin::Utf8StringCompare
        | Builtin::IntMin
        | Builtin::IntMax
        | Builtin::IntClamp
        | Builtin::IntAbs
        | Builtin::IntUnsignedAbs
        | Builtin::IntAbsDiff
        | Builtin::IntCountOnes
        | Builtin::IntLeadingZeros
        | Builtin::IntTrailingZeros
        | Builtin::IntRotateLeft
        | Builtin::IntRotateRight
        | Builtin::IntSwapBytes
        | Builtin::IntReverseBits
        | Builtin::IntIsPowerOfTwo
        | Builtin::IntCheckedAdd
        | Builtin::IntCheckedSub
        | Builtin::IntCheckedMul
        | Builtin::IntCheckedDiv
        | Builtin::IntCheckedRem
        | Builtin::IntCheckedNeg
        | Builtin::IntSaturatingAdd
        | Builtin::IntSaturatingSub
        | Builtin::IntSaturatingMul
        | Builtin::IntWrappingPow
        | Builtin::IntWrappingAdd
        | Builtin::IntWrappingSub
        | Builtin::IntWrappingMul
        | Builtin::IntWrappingNeg
        | Builtin::IntCheckedPow
        | Builtin::IntWideningMul
        | Builtin::IntGcd
        | Builtin::IntLcm
        | Builtin::IntCheckedGcd
        | Builtin::IntCheckedLcm
        | Builtin::IntDivFloor
        | Builtin::IntRemFloor
        | Builtin::OwnedDrop
        | Builtin::OwnedFunction
        | Builtin::OwnedCall
        | Builtin::Not
        | Builtin::Ignore
        | Builtin::ArenaNextId
        | Builtin::BenchNow
        | Builtin::BenchConsume
        | Builtin::AllocCounter
        | Builtin::GenSeed
        | Builtin::GpuOpen
        | Builtin::GpuFeatures
        | Builtin::GpuRun
        | Builtin::GpuSelect
        | Builtin::GpuLast
        | Builtin::DomainMessage
        | Builtin::RcNew
        | Builtin::RcShare
        | Builtin::RcGet
        | Builtin::RcStrongCount
        | Builtin::RcWeakCount
        | Builtin::RcTryUnwrap
        | Builtin::RcDowngrade
        | Builtin::RcUpgrade
        | Builtin::RcPtrEq
        | Builtin::ArcNew
        | Builtin::ArcShare
        | Builtin::ArcGet
        | Builtin::ArcStrongCount
        | Builtin::ArcWeakCount
        | Builtin::ArcTryUnwrap
        | Builtin::ArcDowngrade
        | Builtin::ArcUpgrade
        | Builtin::ArcPtrEq
        | Builtin::AtomicCreate
        | Builtin::AtomicLoad
        | Builtin::AtomicStore
        | Builtin::AtomicSwap
        | Builtin::AtomicCompareExchange
        | Builtin::AtomicFetchAdd
        | Builtin::AtomicFetchSub
        | Builtin::AtomicFetchAnd
        | Builtin::AtomicFetchOr
        | Builtin::AtomicFetchXor
        | Builtin::AtomicIntoInner
        | Builtin::MutexCreate
        | Builtin::MutexIntoInner
        | Builtin::DataChannelBounded
        | Builtin::DataChannelSend
        | Builtin::DataChannelRecv
        | Builtin::DataChannelCloneSender
        | Builtin::DataChannelCloseSender
        | Builtin::DataChannelCloseReceiver
        | Builtin::RoutineYield
        | Builtin::RoutineCount
        | Builtin::RoutineMaxProcs
        | Builtin::RoutineCpuCount
        | Builtin::ChannelMake
        | Builtin::ChannelSend
        | Builtin::ChannelRecv
        | Builtin::ChannelTrySend
        | Builtin::ChannelTryRecv
        | Builtin::ChannelClose
        | Builtin::ChannelLength
        | Builtin::ChannelCapacity
        | Builtin::ChannelShare
        | Builtin::ChannelRelease
        | Builtin::ChannelSendOnly
        | Builtin::ChannelReceiveOnly
        | Builtin::SendOnlySend
        | Builtin::SendOnlyTrySend
        | Builtin::SendOnlyClose
        | Builtin::SendOnlyLength
        | Builtin::SendOnlyCapacity
        | Builtin::SendOnlyShare
        | Builtin::SendOnlyRelease
        | Builtin::ReceiveOnlyRecv
        | Builtin::ReceiveOnlyTryRecv
        | Builtin::ReceiveOnlyLength
        | Builtin::ReceiveOnlyCapacity
        | Builtin::ReceiveOnlyShare
        | Builtin::ReceiveOnlyRelease
        | Builtin::TimeSleep
        | Builtin::TimeAfter
        | Builtin::TimeTick
        | Builtin::TimeNanosecond
        | Builtin::TimeMicrosecond
        | Builtin::TimeMillisecond
        | Builtin::TimeSecond
        | Builtin::TimeMinute
        | Builtin::TimeHour
        | Builtin::TimerCreate
        | Builtin::TimerChannel
        | Builtin::TimerStop
        | Builtin::TimerReset
        | Builtin::TimerFree
        | Builtin::TickerCreate
        | Builtin::TickerChannel
        | Builtin::TickerStop
        | Builtin::TickerReset
        | Builtin::TickerFree
        | Builtin::WaitGroupCreate
        | Builtin::WaitGroupAdd
        | Builtin::WaitGroupWait
        | Builtin::WaitGroupShare
        | Builtin::WaitGroupDrop
        | Builtin::OnceCreate
        | Builtin::OnceBegin
        | Builtin::OnceEnd
        | Builtin::OnceDone
        | Builtin::OnceShare
        | Builtin::OnceDrop
        | Builtin::OnceValueCreate
        | Builtin::OnceValueIntoInner
        | Builtin::RwMutexCreate
        | Builtin::RwMutexIntoInner
        | Builtin::CondCreate
        | Builtin::CondSignal
        | Builtin::CondBroadcast
        | Builtin::CondShare
        | Builtin::CondDrop
        | Builtin::SemaphoreCreate
        | Builtin::SemaphoreAcquire
        | Builtin::SemaphoreTryAcquire
        | Builtin::SemaphoreRelease
        | Builtin::SemaphoreShare
        | Builtin::SemaphoreDrop
        | Builtin::SyncMapReplace
        | Builtin::ContextBackground
        | Builtin::ContextWithCancel
        | Builtin::ContextWithTimeout
        | Builtin::ContextWithDeadline
        | Builtin::ContextCancel
        | Builtin::ContextDone
        | Builtin::ContextErr
        | Builtin::ContextDeadline
        | Builtin::ContextShare
        | Builtin::ContextRelease
        | Builtin::ContextStopAfter
        | Builtin::ContextFreeAfter => Position::Escaping,

        #[cfg(test)]
        Builtin::TestAdd | Builtin::TestUnsigned | Builtin::TestWiden => Position::Escaping,
    }
}

pub(super) fn validate(module: &CheckedModule) -> Vec<Diagnostic> {
    if !module.thread_bound {
        return Vec::new();
    }
    let _phase = crate::timings::Phase::start("pinned");
    let options = effects::Options::new(effects::Scope::All, EffectSet::of(&[Effect::Pinned]));
    let effects = match effects::summarize(module, options) {
        Ok(summary) => summary,
        Err(error) => return vec![error],
    };
    let std_call_spans = first_user_call_spans(module);
    let types = module.types();
    let mut checker = Checker {
        module,
        types: &types,
        effects: &effects,
        override_span: None,
        diagnostics: Vec::new(),
    };
    for (id, function) in module.functions.iter().enumerate() {
        if function.module == "$builtin" || !function.type_parameters.is_empty() {
            continue;
        }
        if function.origin.module == ModuleOrigin::Std {
            let Some(&call_span) = std_call_spans.get(&id) else {
                continue;
            };
            checker.override_span = Some(call_span);
        } else {
            checker.override_span = None;
        }
        checker.visit(&function.body);
    }
    checker.diagnostics
}

/// For each specialized `Std` function reachable from non-`Std` code, the source span of the first
/// non-`Std` expression that calls or references it.
fn first_user_call_spans(module: &CheckedModule) -> BTreeMap<usize, Span> {
    let mut direct: BTreeMap<usize, Span> = BTreeMap::new();
    let mut std_edges: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (caller_id, function) in module.functions.iter().enumerate() {
        if function.module == "$builtin" || !function.type_parameters.is_empty() {
            continue;
        }
        let is_std = function.origin.module == ModuleOrigin::Std;
        let mut stack = vec![&function.body];
        while let Some(expr) = stack.pop() {
            let callee = match &expr.kind {
                TypedExprKind::Function(FunctionRef::User(id)) => Some(*id),
                TypedExprKind::Closure(id, _) => Some(*id),
                _ => None,
            };
            if let Some(target_id) = callee
                && module.functions[target_id].origin.module == ModuleOrigin::Std
            {
                if is_std {
                    std_edges.entry(caller_id).or_default().push(target_id);
                } else {
                    direct.entry(target_id).or_insert(expr.span);
                }
            }
            for child in expr.children().into_iter().rev() {
                stack.push(child);
            }
        }
    }
    let mut result = direct.clone();
    let mut queue: std::collections::VecDeque<(usize, Span)> = direct.into_iter().collect();
    while let Some((curr, span)) = queue.pop_front() {
        if let Some(targets) = std_edges.get(&curr) {
            for &next in targets {
                if let std::collections::btree_map::Entry::Vacant(slot) = result.entry(next) {
                    slot.insert(span);
                    queue.push_back((next, span));
                }
            }
        }
    }
    result
}

struct Checker<'a> {
    module: &'a CheckedModule,
    types: &'a TypeContext<'a>,
    effects: &'a FunctionEffects,
    override_span: Option<Span>,
    diagnostics: Vec<Diagnostic>,
}

impl<'a> Checker<'a> {
    fn is_pinned(&self, id: usize) -> bool {
        self.effects.summary(id).effects.contains(Effect::Pinned)
    }

    fn visit(&mut self, expression: &TypedExpr) {
        use TypedExprKind as K;
        match &expression.kind {
            // `tzroutine { ... }` spawns its `$once` closure on another tzroutine.
            K::RoutineGo(body) => {
                if let Some((id, captures)) = direct_callable(self.module, body) {
                    for capture in captures {
                        self.visit(capture);
                    }
                    if self.is_pinned(id) {
                        self.report_spawned(id, expression.span);
                    }
                } else {
                    self.visit(body);
                }
            }

            // `Routine.scope` and `Parallel.*` run their callback across tzroutines / workers.
            K::Parallel(builtin, arguments) => {
                let callback_index = builtin.parallel_callback();
                for (index, arg) in arguments.iter().enumerate() {
                    if index == callback_index {
                        self.visit_spawned_arg(arg, expression.span);
                    } else {
                        self.visit(arg);
                    }
                }
            }

            // Pipeline `left |> right`: treat `right` as a direct call with `left` appended.
            K::Binary(crate::syntax::BinaryOp::Pipe, left, right) => {
                let inner_right = peel(right);
                match &inner_right.kind {
                    K::Call(callee, partial_args) => {
                        let mut supplied: Vec<&TypedExpr> = partial_args.iter().collect();
                        supplied.push(left);
                        self.visit_call_parts(callee, &supplied, expression.span);
                    }
                    _ => {
                        self.visit_call_parts(right, &[left.as_ref()], expression.span);
                    }
                }
            }

            K::Call(callee, arguments) => {
                let supplied: Vec<&TypedExpr> = arguments.iter().collect();
                self.visit_call_parts(callee, &supplied, expression.span);
            }

            K::Closure(id, captures) => {
                for capture in captures {
                    self.visit(capture);
                }
                if self.is_pinned(*id) {
                    self.report_function_value(*id, expression.span);
                }
            }

            K::Function(FunctionRef::User(id)) => {
                if self.is_pinned(*id) {
                    self.report_function_value(*id, expression.span);
                }
            }

            _ => {
                for child in expression.children() {
                    self.visit(child);
                }
            }
        }
    }

    fn visit_call_parts(&mut self, callee: &TypedExpr, arguments: &[&TypedExpr], call_span: Span) {
        let peeled_callee = peel(callee);

        // `Dyn.of value` packs `value`'s class methods into a vtable value (`dyn C`).
        if let Some((Builtin::DynOf, types, 1)) = builtin_of_callee(self.module, peeled_callee)
            && arguments.len() == 1
        {
            self.visit(arguments[0]);
            if let [stored_ty, Type::Dyn(target)] = types
                && let Some(slots) = self
                    .module
                    .vtables
                    .get(&(target.vtable_key(), stored_ty.clone()))
                && let Some(&pinned_slot) = slots.iter().find(|&&slot| self.is_pinned(slot))
            {
                self.report_function_value(pinned_slot, call_span);
            }
            return;
        }

        // Full call to a built-in (direct or `$builtin` wrapper).
        if let Some((builtin, _, arity)) = builtin_of_callee(self.module, peeled_callee)
            && arguments.len() == arity
        {
            let pos = builtin_position(builtin);
            for arg in arguments {
                match pos {
                    Position::Pinning | Position::Inline => self.visit_allowed_callback(arg),
                    Position::Spawned => self.visit_spawned_arg(arg, call_span),
                    Position::Escaping => self.visit(arg),
                }
            }
            return;
        }

        // Full call to a `std` spawn wrapper (`ErrGroup.go`, `Context.after_func`).
        if let TypedExprKind::Function(FunctionRef::User(id)) = &peeled_callee.kind
            && is_std_spawn_wrapper(&self.module.functions[*id], arguments.len())
        {
            for arg in arguments {
                self.visit_spawned_arg(arg, call_span);
            }
            return;
        }

        // Direct call or partial application of a known user function / closure.
        if let Some((id, captures)) = direct_callable(self.module, peeled_callee) {
            for capture in captures {
                self.visit(capture);
            }
            let target = &self.module.functions[id];
            let needed = target.parameters.len().saturating_sub(captures.len());
            if arguments.len() < needed {
                // Partial application produces a function value!
                for arg in arguments {
                    self.visit(arg);
                }
                if self.is_pinned(id) {
                    self.report_function_value(id, call_span);
                }
                return;
            }
            // Full direct call: `callee` runs inline on the current tzroutine; check callback
            // `extern` positions for its arguments.
            let callback_mask = if let TypedExprKind::HostCall(import, _) = &target.body.kind
                && import.callbacks
            {
                effects::pinning_argument_mask(self.module, peeled_callee, arguments.len())
            } else {
                0
            };
            for (index, arg) in arguments.iter().enumerate() {
                if index < 64 && (callback_mask & (1 << index)) != 0 {
                    self.visit_allowed_callback(arg);
                } else {
                    self.visit(arg);
                }
            }
            return;
        }

        self.visit(callee);
        for arg in arguments {
            self.visit(arg);
        }
    }

    /// Visits an argument in a `Pinning` or `Inline` callback slot: a direct function/lambda whose
    /// arity matches the callback type is allowed even if `pinned`, while anything else (such as a
    /// partial application or nested function value) is checked normally.
    fn visit_allowed_callback(&mut self, arg: &TypedExpr) {
        let peeled = peel(arg);
        if matches!(arg.ty, Type::Function(..))
            && let Some((_, captures)) = direct_callable(self.module, peeled)
        {
            for capture in captures {
                self.visit(capture);
            }
            return;
        }
        self.visit(arg);
    }

    /// Visits an argument to a tzroutine/worker spawn (`Routine.go`, `WaitGroup.go`, `ErrGroup.go`,
    /// `Time.after_func`, `Context.after_func`, `Routine.scope`, `Parallel.*`), unwrapping
    /// `Owned.function (\() -> ...)` so the spawn reports `pinned_code_spawned`.
    fn visit_spawned_arg(&mut self, arg: &TypedExpr, spawn_span: Span) {
        let peeled = peel(arg);
        if let TypedExprKind::Call(inner_callee, inner_args) = &peeled.kind
            && let Some((Builtin::OwnedFunction, _, 1)) =
                builtin_of_callee(self.module, peel(inner_callee))
            && inner_args.len() == 1
        {
            self.visit_spawned_arg(&inner_args[0], spawn_span);
            return;
        }
        if let Some((id, captures)) = direct_callable(self.module, peeled) {
            for capture in captures {
                self.visit(capture);
            }
            if self.is_pinned(id) {
                self.report_spawned(id, spawn_span);
            }
            return;
        }
        self.visit(arg);
    }

    fn report_spawned(&mut self, target_id: usize, span: Span) {
        let span = self.override_span.unwrap_or(span);
        let handle = self.source_handle_name(target_id);
        let message = format!(
            "this starts code that uses the thread-bound extern type '{handle}' on another tzroutine or thread, which may run on any OS thread; do that work inside 'Routine.with_locked_thread' in the new tzroutine, or run it on the calling thread; {MAIN_THREAD_ADVICE}"
        );
        let diagnostic = self.attach_witness(
            Diagnostic::of(DiagnosticKind::PinnedCodeSpawned, message, span),
            target_id,
            &handle,
        );
        self.diagnostics.push(diagnostic);
    }

    fn report_function_value(&mut self, target_id: usize, span: Span) {
        let span = self.override_span.unwrap_or(span);
        let handle = self.source_handle_name(target_id);
        let message = format!(
            "this function value runs code that uses the thread-bound extern type '{handle}', and a function value may reach another tzroutine or thread; call the function directly, pass it straight to 'Routine.with_locked_thread', or use 'for' or 'match' instead of a callback"
        );
        let diagnostic = self.attach_witness(
            Diagnostic::of(DiagnosticKind::PinnedFunctionValue, message, span),
            target_id,
            &handle,
        );
        self.diagnostics.push(diagnostic);
    }

    fn source_handle_name(&self, target_id: usize) -> String {
        let chain = self.effects.chain(target_id, Effect::Pinned);
        let leaf_id = chain.last().map_or(target_id, |(id, _)| *id);
        function_local_handle(&self.module.functions[leaf_id], self.types)
            .unwrap_or_else(|| "local".into())
    }

    fn attach_witness(
        &self,
        mut diagnostic: Diagnostic,
        target_id: usize,
        handle: &str,
    ) -> Diagnostic {
        let chain = self.effects.chain(target_id, Effect::Pinned);
        if chain.is_empty() {
            return diagnostic;
        }
        let help = if chain.len() == 1 {
            format!(
                "uses '{handle}' directly in '{}'",
                effects::shown(self.module, chain[0].0)
            )
        } else {
            let path = chain
                .iter()
                .map(|(id, _)| format!("'{}'", effects::shown(self.module, *id)))
                .collect::<Vec<_>>()
                .join(" -> ");
            format!("reaches '{handle}' via {path}")
        };
        diagnostic = diagnostic.with_help(help);
        for pair in chain.windows(2).take(crate::diagnostic::MAX_RELATED - 1) {
            diagnostic = diagnostic.related(
                RelatedRole::ViaCall,
                format!("calls '{}'", effects::shown(self.module, pair[1].0)),
                pair[0].1,
            );
        }
        if let Some(&(_, direct_span)) = chain.last()
            && direct_span != Span::default()
        {
            diagnostic = diagnostic.related(
                RelatedRole::Note,
                format!("uses thread-bound extern type '{handle}' here"),
                direct_span,
            );
        }
        diagnostic
    }
}

fn peel(mut expr: &TypedExpr) -> &TypedExpr {
    while let TypedExprKind::Cast(inner)
    | TypedExprKind::BorrowOperand(inner)
    | TypedExprKind::Borrow(inner, false)
    | TypedExprKind::Dereference(inner) = &expr.kind
    {
        expr = inner;
    }
    expr
}

fn direct_callable<'a>(
    module: &CheckedModule,
    expr: &'a TypedExpr,
) -> Option<(usize, &'a [TypedExpr])> {
    match &expr.kind {
        TypedExprKind::Closure(id, captures) => Some((*id, captures.as_slice())),
        TypedExprKind::Function(FunctionRef::User(id))
            if module.functions[*id].module != "$builtin" =>
        {
            Some((*id, &[]))
        }
        _ => None,
    }
}

fn builtin_of_callee<'a>(
    module: &'a CheckedModule,
    callee: &'a TypedExpr,
) -> Option<(Builtin, &'a [Type], usize)> {
    match &callee.kind {
        TypedExprKind::Function(FunctionRef::Builtin(instance)) => {
            let arity = match &callee.ty {
                Type::Function(params, _) => params.len(),
                _ => instance.builtin.scheme().parameters.len(),
            };
            Some((instance.builtin, instance.types.as_slice(), arity))
        }
        TypedExprKind::Function(FunctionRef::User(id)) => {
            let function = &module.functions[*id];
            if function.module != "$builtin" {
                return None;
            }
            let mut pending = vec![&function.body];
            while let Some(expr) = pending.pop() {
                if let TypedExprKind::Function(FunctionRef::Builtin(instance)) = &expr.kind {
                    return Some((
                        instance.builtin,
                        instance.types.as_slice(),
                        function.parameters.len(),
                    ));
                }
                pending.extend(expr.children());
            }
            None
        }
        _ => None,
    }
}

fn is_std_spawn_wrapper(function: &CheckedFunction, supplied: usize) -> bool {
    if function.origin.module != ModuleOrigin::Std || supplied != function.parameters.len() {
        return false;
    }
    let name = function.qualified_name();
    name.starts_with("ErrGroup.go")
        || name.starts_with("ErrGroup.try_go")
        || name.starts_with("Context.after_func")
}

fn function_local_handle(function: &CheckedFunction, types: &TypeContext<'_>) -> Option<String> {
    for parameter in &function.parameters {
        if let Some(name) = parameter.ty.first_local_handle_name(types) {
            return Some(name);
        }
    }
    if let Some(name) = function.signature.result.first_local_handle_name(types) {
        return Some(name);
    }
    let mut stack = vec![&function.body];
    while let Some(expr) = stack.pop() {
        if let Some(name) = expr.ty.first_local_handle_name(types) {
            return Some(name);
        }
        for child in expr.children().into_iter().rev() {
            stack.push(child);
        }
    }
    None
}
