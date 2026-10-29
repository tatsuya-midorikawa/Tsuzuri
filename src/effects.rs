//! The effect summary of each function (A20): a set of the 14 effects of D-53, inferred from the
//! typed IR after specialization and closure lowering, and the checks of the effect attributes
//! `@no_alloc`, `@no_copy`, `@no_trap` and `@terminates`.
//!
//! The analysis only reads the typed IR: it changes no type, ownership or specialization rule and
//! no emitted byte. Each function's own body gives its direct sources (appendix A and B of A20);
//! the references between functions are edges, and a strongly connected component of the call
//! graph shares one summary, the union of its members' sources and of its callees' summaries.
//! A program without effect attributes computes nothing (`needed`).

use std::collections::VecDeque;

use super::*;
use crate::copies::{CopyCost, CopySite};
use crate::syntax::EffectKind;

/// One effect of the D-53 vocabulary. The order is the order of the names.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
#[repr(u8)]
pub enum Effect {
    Alloc,
    Atomic,
    Copy,
    Debug,
    Diverge,
    Extern,
    Gpu,
    Io,
    Net,
    Os,
    Rc,
    Task,
    Trap,
    UserDrop,
}

impl Effect {
    pub const ALL: [Effect; 14] = [
        Effect::Alloc,
        Effect::Atomic,
        Effect::Copy,
        Effect::Debug,
        Effect::Diverge,
        Effect::Extern,
        Effect::Gpu,
        Effect::Io,
        Effect::Net,
        Effect::Os,
        Effect::Rc,
        Effect::Task,
        Effect::Trap,
        Effect::UserDrop,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Effect::Alloc => "alloc",
            Effect::Atomic => "atomic",
            Effect::Copy => "copy",
            Effect::Debug => "debug",
            Effect::Diverge => "diverge",
            Effect::Extern => "extern",
            Effect::Gpu => "gpu",
            Effect::Io => "io",
            Effect::Net => "net",
            Effect::Os => "os",
            Effect::Rc => "rc",
            Effect::Task => "task",
            Effect::Trap => "trap",
            Effect::UserDrop => "user_drop",
        }
    }

    pub fn from_name(name: &str) -> Option<Effect> {
        Effect::ALL.into_iter().find(|effect| effect.name() == name)
    }
}

/// A set of effects, one bit per `Effect` in name order.
#[derive(Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct EffectSet(u16);

impl EffectSet {
    pub const EMPTY: EffectSet = EffectSet(0);
    pub const ALL: EffectSet = EffectSet((1 << 14) - 1);
    /// What A19's `pure` forbids: the 10 effects that are not `alloc`, `copy`, `diverge` or `trap`.
    pub const PURE_FORBIDDEN: EffectSet = EffectSet::of(&[
        Effect::Atomic,
        Effect::Debug,
        Effect::Extern,
        Effect::Gpu,
        Effect::Io,
        Effect::Net,
        Effect::Os,
        Effect::Rc,
        Effect::Task,
        Effect::UserDrop,
    ]);

    pub const fn of(effects: &[Effect]) -> EffectSet {
        let mut bits = 0u16;
        let mut index = 0;
        while index < effects.len() {
            bits |= 1 << effects[index] as u8;
            index += 1;
        }
        EffectSet(bits)
    }

    pub fn contains(self, effect: Effect) -> bool {
        self.0 & (1 << effect as u8) != 0
    }

    pub fn insert(&mut self, effect: Effect) {
        self.0 |= 1 << effect as u8;
    }

    pub fn union(self, other: EffectSet) -> EffectSet {
        EffectSet(self.0 | other.0)
    }

    pub fn intersection(self, other: EffectSet) -> EffectSet {
        EffectSet(self.0 & other.0)
    }

    pub fn intersects(self, other: EffectSet) -> bool {
        self.0 & other.0 != 0
    }

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// The effects of the set in name order.
    pub fn iter(self) -> impl Iterator<Item = Effect> {
        Effect::ALL
            .into_iter()
            .filter(move |effect| self.contains(*effect))
    }

    pub fn names(self) -> Vec<&'static str> {
        self.iter().map(Effect::name).collect()
    }
}

/// The effects that this compiler computes. A checked attribute whose effect is not here is
/// rejected (`effect_attribute_unavailable`, A20 D19).
pub const SUPPORTED: EffectSet = EffectSet::ALL;

/// The effects that an attribute forbids in the summary; an unknown call breaks every attribute.
pub fn forbidden(kind: EffectKind) -> EffectSet {
    match kind {
        EffectKind::NoAlloc => EffectSet::of(&[Effect::Alloc, Effect::Extern]),
        EffectKind::NoCopy => EffectSet::of(&[Effect::Copy]),
        EffectKind::NoTrap => EffectSet::of(&[Effect::Trap, Effect::Extern]),
        EffectKind::Terminates => EffectSet::of(&[Effect::Diverge, Effect::Extern]),
    }
}

/// E15: the `@trusted` property that vouches for an extern function under an attribute.
pub fn trusted_property(kind: EffectKind) -> u8 {
    crate::syntax::trusted_property_bit(kind.name()).expect("every effect attribute is a property")
}

/// The effect that an attribute promises to be absent.
pub fn attribute_effect(kind: EffectKind) -> Effect {
    match kind {
        EffectKind::NoAlloc => Effect::Alloc,
        EffectKind::NoCopy => Effect::Copy,
        EffectKind::NoTrap => Effect::Trap,
        EffectKind::Terminates => Effect::Diverge,
    }
}

/// Why a summary cannot say what a function does.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Unknown {
    /// It reaches a `dyn` dispatch.
    Dyn,
    /// It reaches an expression that specialization did not resolve.
    Unresolved,
    /// It takes a function value and reaches a call of a function or task value that its caller
    /// can choose.
    ValueCall,
}

impl Unknown {
    pub fn name(self) -> &'static str {
        match self {
            Unknown::Dyn => "dyn",
            Unknown::Unresolved => "unresolved",
            Unknown::ValueCall => "value_call",
        }
    }
}

/// The reason of a direct source: a name of appendix J.1 and the built-in function, if any.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Why {
    pub code: &'static str,
    pub builtin: Option<Builtin>,
}

/// Where a function gets an effect: its own body, or a call of `callee` at `span`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Witness {
    Direct { span: Span, why: Why },
    Via { callee: usize, span: Span },
}

#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct Summary {
    pub effects: EffectSet,
    pub unknown: Option<Unknown>,
    /// The parameters (bit `i` for `parameters[i]`, captures first) that the function calls as
    /// functions; their effects are checked where the function is called (Phase 6).
    pub calls_params: u64,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Scope {
    /// The functions that an effect attribute's function reaches.
    Reachable,
    All,
}

#[derive(Clone, Copy, Debug)]
pub struct Options {
    pub scope: Scope,
    pub computed: EffectSet,
    /// Keep every direct source and exemption of each body (`FunctionEffects::sites`).
    pub sites: bool,
    /// E15: a `@trusted` property bit (`syntax::TRUSTED_PROPERTIES`). An extern function whose
    /// mark states it is not a source of `extern`; others still are.
    pub trusted: Option<u8>,
}

impl Options {
    /// What `check` computes: the effects that the program's attributes forbid.
    pub fn checking(module: &CheckedModule) -> Options {
        let mut computed = EffectSet::EMPTY;
        for function in &module.functions {
            for kind in function.origin.asserts.kinds() {
                computed = computed.union(forbidden(kind));
            }
        }
        Options::new(Scope::Reachable, computed.intersection(SUPPORTED))
    }

    pub fn new(scope: Scope, computed: EffectSet) -> Options {
        Options {
            scope,
            computed: computed.intersection(SUPPORTED),
            sites: false,
            trusted: None,
        }
    }
}

/// A direct source of an effect in a body, or an exemption from one.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Site {
    pub effect: Effect,
    pub span: Span,
    pub why: &'static str,
    pub exempt: bool,
}

/// One function's row for the facts (G26): its effects, the first site of each and the exemptions.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct EffectRow {
    pub id: usize,
    pub effects: Vec<&'static str>,
    pub unknown: Option<&'static str>,
    pub sites: Vec<EffectSite>,
    pub exempt_sites: Vec<EffectSite>,
    pub complete: bool,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct EffectSite {
    pub effect: &'static str,
    pub span: Span,
    pub why: &'static str,
    /// The function ids of the chain to the direct source; empty for a direct source.
    pub via: Vec<usize>,
}

/// The summaries of a module, indexed like `module.functions`.
#[derive(Clone, Debug)]
pub struct FunctionEffects {
    pub summaries: Vec<Summary>,
    pub computed: EffectSet,
    reached: Vec<bool>,
    locals: Vec<EffectSet>,
    witnesses: Vec<[Option<Witness>; BITS]>,
    sites: Vec<Vec<Site>>,
    calls: Vec<Vec<CallSite>>,
    /// The references to attributed functions in each function.
    references: Vec<Vec<Reference>>,
    /// The function that the `cycle` source of `diverge` in each function refers to.
    cycle_targets: BTreeMap<usize, usize>,
    empty: Summary,
}

const BITS: usize = 17;
const DYN: usize = 14;
const UNRESOLVED: usize = 15;
const VALUE_CALL: usize = 16;

fn bit(effect: Effect) -> usize {
    effect as usize
}

/// The 14 effects and the three sources of an unknown summary, as one bit set.
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
struct Bits(u32);

impl Bits {
    fn contains(self, index: usize) -> bool {
        self.0 & (1 << index) != 0
    }

    fn insert(&mut self, index: usize) {
        self.0 |= 1 << index;
    }

    fn effects(self) -> EffectSet {
        EffectSet((self.0 & ((1 << 14) - 1)) as u16)
    }
}

/// What a built-in function does (appendix B of A20).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct BuiltinFacts {
    /// Its effect of the A19 vocabulary, `None` for a pure one.
    pub effect: Option<Effect>,
    pub alloc: bool,
    pub trap: bool,
    pub diverge: bool,
    /// It calls a function or task value that it takes.
    pub callbacks: bool,
    /// It dispatches through `dyn`.
    pub dynamic: bool,
    /// Why the traps of its IR are not traps of the program's logic.
    pub exempt_trap: Option<&'static str>,
}

const fn facts(
    effect: Option<Effect>,
    alloc: bool,
    trap: bool,
    diverge: bool,
    callbacks: bool,
    dynamic: bool,
    exempt_trap: Option<&'static str>,
) -> BuiltinFacts {
    BuiltinFacts {
        effect,
        alloc,
        trap,
        diverge,
        callbacks,
        dynamic,
        exempt_trap,
    }
}

/// The classification of every built-in function; a new one does not compile until it is here.
pub(crate) fn builtin_facts(builtin: Builtin) -> BuiltinFacts {
    use Builtin::*;
    match builtin {
        // B1
        Sqrt | Floor | Ceil | Abs | MathSqrt | MathFloor | MathCeil | MathTrunc | MathRound
        | MathRoundEven | MathAbs | MathMin | MathMax | MathClamp | MathFma | MathCopysign
        | MathIsNan | MathIsInfinite | MathIsFinite | MathPi | MathE | MathSin | MathCos
        | MathTan | MathAsin | MathAcos | MathAtan | MathAtan2 | MathExp | MathExp2 | MathLog
        | MathLog2 | MathLog10 | MathPow | MathCbrt | MathHypot | ToFloat | ToInt
        | StringIsWellFormed | UnicodeTableLength | Default | Hash | HashMix | VecEmpty
        | SimdSplat | SimdOfLanes2 | SimdOfLanes4 | SimdOfLanes8 | SimdOfLanes16
        | SimdOfLanes32 | SimdSum | SimdEq | SimdNe | SimdLt | SimdLe | SimdGt | SimdGe
        | SimdSelect | SimdAll | SimdAny | VecLength | VecCapacity | VecIsEmpty | VecPop
        | VecOfArray | VecToArray | VecView | CharToU16 | CharOfU16 | Utf8CharToU32
        | Utf8CharOfU32 | Utf8StringToBytes | Utf8StringFromBytes | StringCompare
        | Utf8StringCompare | IntMin | IntMax | IntAbs | IntUnsignedAbs | IntAbsDiff
        | IntCountOnes | IntLeadingZeros | IntTrailingZeros | IntRotateLeft | IntRotateRight
        | IntSwapBytes | IntReverseBits | IntIsPowerOfTwo | IntCheckedAdd | IntCheckedSub
        | IntCheckedMul | IntCheckedDiv | IntCheckedRem | IntCheckedNeg | IntSaturatingAdd
        | IntSaturatingSub | IntSaturatingMul | IntWrappingAdd | IntWrappingSub
        | IntWrappingMul | IntWrappingNeg | IntCheckedPow | IntWideningMul | OwnedDrop
        | OwnedFunction | Not | Ignore => facts(None, false, false, false, false, false, None),
        #[cfg(test)]
        TestAdd | TestUnsigned | TestWiden => facts(None, false, false, false, false, false, None),
        // B2: the index is checked before the bounds guard, which never fails.
        VecClear | VecGet => facts(None, false, false, false, false, false, Some("dead_guard")),
        // B3
        FixedArrayInit | ListFoldRef | SeqNext | OwnedCall => {
            facts(None, false, false, false, true, false, None)
        }
        // B4
        Assert
        | ContractRequires
        | ContractEnsures
        | Unreachable
        | UnicodeTableEntry
        | ArraySet
        | ArraySwap
        | ArrayWrite
        | ArraySwapIn
        | ArraySplitAtMut
        | ListTail
        | SimdExtract
        | SimdReplace
        | SimdLoad
        | SimdStore
        | VecSet
        | VecSwap
        | VecAt
        | VecTruncate
        | Utf8CharOfU32Unchecked
        | Utf8StringDecodeAt
        | IntClamp
        | IntWrappingPow => facts(None, false, true, false, false, false, None),
        // B5
        ArrayUpdate => facts(None, false, true, false, true, false, None),
        // B6. `String.to_code_units` and `String.from_code_units` widen or narrow the units into a
        // new buffer when the program's strings are compact; the rule does not depend on the form.
        CloneString | CloneUtf8String | StringFromUtf8 | Utf8StringFromString
        | StringToWellFormed | ToString | Display | Parse | DisplayQuoted | ListCons
        | ArrayConcat | ArrayToList | ListReverse | ListToArray | VecWithCapacity | VecPush
        | VecPushMut | VecReserve | VecClone | StringToCodeUnits | StringFromCodeUnits => {
            facts(None, true, false, false, false, false, None)
        }
        // B7
        ArraySortBy | ListMap | ListMapRef => facts(None, true, false, false, true, false, None),
        // B8
        DynOf => facts(None, true, false, false, false, true, None),
        // B9
        IOWrite | AsyncClock | AsyncRetire => {
            facts(Some(Effect::Io), false, false, false, false, false, None)
        }
        // B10
        AsyncTake | AsyncPut | AsyncNextId | AsyncPosted => facts(
            Some(Effect::Io),
            false,
            false,
            false,
            false,
            false,
            Some("runtime_internal"),
        ),
        // B11
        AsyncResume => facts(Some(Effect::Io), false, false, false, true, false, None),
        // B12
        AsyncWait => facts(
            Some(Effect::Io),
            false,
            false,
            true,
            false,
            false,
            Some("runtime_internal"),
        ),
        // B13
        IOReadLine => facts(
            Some(Effect::Io),
            true,
            false,
            true,
            false,
            false,
            Some("host_status"),
        ),
        // B14
        BenchNow | GenSeed => facts(Some(Effect::Os), false, false, false, false, false, None),
        // B15
        OsClock | OsSleep | OsClose => facts(
            Some(Effect::Os),
            false,
            false,
            false,
            false,
            false,
            Some("host_status"),
        ),
        // B16
        OsArgs | OsRandom => facts(
            Some(Effect::Os),
            true,
            false,
            false,
            false,
            false,
            Some("host_status"),
        ),
        // B17: `Os.__write` and `Os.__open` copy the path into a buffer of the C heap, and opening
        // a FIFO waits for its other end (review of #63).
        OsRead | OsHandle | OsSpawn | OsWrite | OsOpen => facts(
            Some(Effect::Os),
            true,
            false,
            true,
            false,
            false,
            Some("host_status"),
        ),
        // B18
        NetClose | NetClassify | NetWatch | NetUnwatch | NetSend => facts(
            Some(Effect::Net),
            false,
            false,
            false,
            false,
            false,
            Some("host_status"),
        ),
        // B19
        NetWrite | NetConnect => facts(
            Some(Effect::Net),
            false,
            false,
            true,
            false,
            false,
            Some("host_status"),
        ),
        // B20
        NetNames => facts(
            Some(Effect::Net),
            true,
            false,
            false,
            false,
            false,
            Some("host_status"),
        ),
        // B21
        NetResolve | NetOpen | NetAccept | NetRead => facts(
            Some(Effect::Net),
            true,
            false,
            true,
            false,
            false,
            Some("host_status"),
        ),
        // B22
        TaskRun | TaskParallel | TaskParallelResults => {
            facts(Some(Effect::Task), true, false, false, false, false, None)
        }
        // B23
        ParallelInit | ParallelMap | ParallelMapRef | ParallelReduce | TaskScope => {
            facts(Some(Effect::Task), true, false, false, true, false, None)
        }
        // B24
        ParallelForEachChunk => facts(Some(Effect::Task), true, true, false, true, false, None),
        // B25
        AtomicLoad
        | AtomicStore
        | AtomicSwap
        | AtomicCompareExchange
        | AtomicFetchAdd
        | AtomicFetchSub
        | AtomicFetchAnd
        | AtomicFetchOr
        | AtomicFetchXor
        | AtomicIntoInner
        | MutexIntoInner
        | ChannelCloneSender
        | ChannelCloseSender
        | ChannelCloseReceiver => facts(
            Some(Effect::Atomic),
            false,
            false,
            false,
            false,
            false,
            None,
        ),
        // B26
        ArenaNextId => facts(
            Some(Effect::Atomic),
            false,
            false,
            false,
            false,
            false,
            Some("runtime_internal"),
        ),
        // B27
        ChannelSend | ChannelRecv => {
            facts(Some(Effect::Atomic), false, true, true, false, false, None)
        }
        // B28
        MutexWith => facts(Some(Effect::Atomic), false, true, true, true, false, None),
        // B29
        AtomicCreate | MutexCreate => {
            facts(Some(Effect::Atomic), true, false, false, false, false, None)
        }
        // B30
        ChannelBounded => facts(Some(Effect::Atomic), true, true, false, false, false, None),
        // B31
        RcGet | RcStrongCount | RcWeakCount | RcTryUnwrap | RcPtrEq | ArcGet | ArcStrongCount
        | ArcWeakCount | ArcTryUnwrap | ArcPtrEq => {
            facts(Some(Effect::Rc), false, false, false, false, false, None)
        }
        // B32
        RcShare | RcDowngrade | RcUpgrade | ArcShare | ArcDowngrade | ArcUpgrade => facts(
            Some(Effect::Rc),
            false,
            false,
            false,
            false,
            false,
            Some("refcount_overflow"),
        ),
        // B33
        RcNew | ArcNew => facts(Some(Effect::Rc), true, false, false, false, false, None),
        // B34
        BenchConsume | AllocCounter => {
            facts(Some(Effect::Debug), false, false, false, false, false, None)
        }
        // B35
        DebugPrintString => facts(Some(Effect::Debug), true, false, false, false, false, None),
        // B36
        GpuOpen | GpuFeatures | GpuSelect | GpuLast => {
            facts(Some(Effect::Gpu), false, false, false, false, false, None)
        }
        // B37
        GpuRun => facts(Some(Effect::Gpu), true, false, false, false, false, None),
        // B38: the growing `Vec` operations, which check their index or length.
        VecSetCapacity | VecInsert | VecSplice => {
            facts(None, true, true, false, false, false, None)
        }
    }
}

/// Whether some function of the module carries an effect attribute.
pub fn needed(module: &CheckedModule) -> bool {
    module
        .functions
        .iter()
        .any(|function| !function.origin.asserts.is_empty())
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Limits {
    pub functions: usize,
    pub nodes: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            functions: 4_096,
            nodes: 2_097_152,
        }
    }
}

pub fn summarize(module: &CheckedModule, options: Options) -> Result<FunctionEffects, Diagnostic> {
    summarize_with(module, options, Limits::default())
}

/// The parameters whose calls a summary records, one bit each; a call of a later parameter is a
/// call of a value that the analysis cannot follow.
const PARAMETER_BITS: usize = 64;

/// What an argument of a call gives a parameter that the callee calls (Phase 6).
#[derive(Clone, Debug, PartialEq, Eq)]
enum Arg {
    /// One of these functions, whose bodies are edges of the caller.
    Known(Vec<usize>),
    /// The caller's own parameter.
    Param(usize),
    /// A value that the analysis cannot follow.
    Unknown,
}

/// A direct call of a user function, with what each parameter receives (captures first).
#[derive(Clone, Debug)]
struct CallSite {
    callee: usize,
    span: Span,
    arguments: Vec<Arg>,
}

/// What the scan of one body found.
#[derive(Default)]
struct Local {
    bits: Bits,
    first: [Option<(Span, Why)>; BITS],
    edges: Vec<(usize, Span)>,
    calls: Vec<CallSite>,
    /// Calls of the function's own parameters: the bit set and the first call of each.
    param_calls: u64,
    /// The references to attributed functions, with what uses them (resolved after the scan).
    references: Vec<(usize, Span, Consumer)>,
    /// The uses of each immutable local.
    alias_uses: BTreeMap<usize, Vec<Consumer>>,
    sites: Vec<Site>,
    nodes: usize,
    /// The frame bytes of the literal trees that stay in the frame so far (A7).
    frame: usize,
}

/// Whether a function has a `string` parameter, which a host call may copy for the host.
fn takes_string(function: &CheckedFunction) -> bool {
    function.parameters.iter().any(|parameter| {
        let mut ty = &parameter.ty;
        while let Type::Reference(inner, _) = ty {
            ty = inner;
        }
        *ty == Type::String
    })
}

/// The arguments that every call of a value used as `consumer` supplies: through the uses of
/// the immutable local it is bound to (`usize::MAX` when nothing calls it or passes it on).
/// `known` keeps the answer of each local, which an alias that uses its source twice
/// (`let b = if c then a else a`) would otherwise compute once for each path (review 3 of #63).
fn supplied(
    consumer: Consumer,
    uses: &BTreeMap<usize, Vec<Consumer>>,
    known: &mut BTreeMap<usize, usize>,
    depth: usize,
) -> usize {
    let Some(local) = consumer.alias else {
        return consumer.supplied;
    };
    // Aliases of aliases deeper than this are not followed: the value counts as passed on.
    if depth > 64 {
        return consumer.supplied;
    }
    let rest = match known.get(&local) {
        Some(&rest) => rest,
        None => {
            let rest = uses.get(&local).map_or(usize::MAX, |found| {
                found
                    .iter()
                    .map(|&next| supplied(next, uses, known, depth + 1))
                    .min()
                    .unwrap_or(usize::MAX)
            });
            known.insert(local, rest);
            rest
        }
    };
    consumer.supplied.saturating_add(rest)
}

fn position(span: Span) -> (usize, usize, usize) {
    (span.source.unwrap_or(usize::MAX), span.start, span.end)
}

impl Local {
    fn source(&mut self, index: usize, span: Span, why: Why) {
        self.bits.insert(index);
        if self.first[index].is_none_or(|(first, _)| position(span) < position(first)) {
            self.first[index] = Some((span, why));
        }
    }
}

#[derive(Clone, Copy, Default)]
struct Ctx {
    /// The value stays in place and is only read (A7).
    read: bool,
    /// Inside the steps and bindings of a `match` pattern.
    pattern: bool,
    /// An element of a literal tree whose root already counted the frame bytes.
    nested: bool,
    /// What the enclosing expressions do with the value, when it is a function.
    consumer: Consumer,
}

/// How the enclosing expressions use a function value: the arguments that the calls of the value
/// supply (a value used otherwise is called later with none of them), and the immutable local it
/// is bound to, whose uses supply the rest. A reference to an attributed function that calls its
/// parameters must be called with them, or the functions passed to it go unchecked (review of #63).
#[derive(Clone, Copy, Default, Debug)]
struct Consumer {
    supplied: usize,
    alias: Option<usize>,
}

/// A reference to a function with an effect attribute, and the arguments that every call of it
/// supplies (`usize::MAX` for a value that is never called).
#[derive(Clone, Copy, Debug)]
struct Reference {
    function: usize,
    span: Span,
    supplied: usize,
}

enum Item<'a> {
    Node(&'a TypedExpr, Ctx),
    Enter(&'a TypedExpr),
    EnterElse(&'a TypedExpr),
    Leave,
}

/// What a scan shares between the functions of one `summarize`.
struct State<'a> {
    module: &'a CheckedModule,
    options: Options,
    copy_sites: Vec<Vec<&'a CopySite>>,
    drops: BTreeMap<Type, Vec<usize>>,
    open: BTreeMap<Type, bool>,
    nodes: usize,
    limits: Limits,
}

impl State<'_> {
    /// Whether a value of `ty` can carry a function or task value (cached).
    fn is_open(&mut self, ty: &Type) -> bool {
        if let Some(&open) = self.open.get(ty) {
            return open;
        }
        let open = open_type(ty, &self.module.types());
        self.open.insert(ty.clone(), open);
        open
    }

    fn computes(&self, effect: Effect) -> bool {
        self.options.computed.contains(effect)
    }

    fn drop_functions(&mut self, ty: &Type) -> Vec<usize> {
        if self.module.user_drops.is_empty() {
            return Vec::new();
        }
        if let Some(found) = self.drops.get(ty) {
            return found.clone();
        }
        let types = self.module.types();
        let found = std::cell::RefCell::new(Vec::new());
        types.stored_all(ty, |part| {
            if part.has_user_drop(&types) {
                if let Some(&id) = self.module.user_drops.get(part) {
                    found.borrow_mut().push(id);
                } else {
                    // A Drop type without a specialized `Drop.drop` still runs user code.
                    found.borrow_mut().push(usize::MAX);
                }
            }
            true
        });
        let mut found = found.into_inner();
        found.sort_unstable();
        found.dedup();
        self.drops.insert(ty.clone(), found.clone());
        found
    }
}

fn limit_error(limits: Limits, span: Span) -> Diagnostic {
    Diagnostic::of(
        DiagnosticKind::EffectSummaryLimit,
        format!(
            "an effect summary reaches more than {} functions or {} expression nodes; split the function",
            limits.functions, limits.nodes
        ),
        span,
    )
}

fn builtin_why(builtin: Builtin) -> Why {
    Why {
        code: "builtin",
        builtin: Some(builtin),
    }
}

fn why(code: &'static str) -> Why {
    Why {
        code,
        builtin: None,
    }
}

/// Follows a value that may carry functions back to the functions it may hold: named functions,
/// closures, partial applications, literals and immutable locals bound to them, or a parameter of
/// the scanned function.
fn argument<'a>(
    value: &'a TypedExpr,
    aliases: &BTreeMap<usize, &'a TypedExpr>,
    parameters: &BTreeMap<usize, usize>,
    open: &mut dyn FnMut(&Type) -> bool,
) -> Arg {
    let mut pending = vec![value];
    let mut functions = Vec::new();
    let mut param = None;
    let mut seen = BTreeSet::new();
    let mut steps = 0;
    while let Some(value) = pending.pop() {
        steps += 1;
        if steps > 256 {
            return Arg::Unknown;
        }
        if !open(&value.ty) {
            continue;
        }
        match &value.kind {
            TypedExprKind::Function(FunctionRef::User(id)) => functions.push(*id),
            TypedExprKind::Closure(id, captures) => {
                functions.push(*id);
                pending.extend(captures);
            }
            TypedExprKind::Call(callee, arguments)
                if matches!(
                    callee.kind,
                    TypedExprKind::Function(FunctionRef::User(_)) | TypedExprKind::Closure(..)
                ) && matches!(&callee.ty, Type::Function(parameters, _) if arguments.len() < parameters.len()) =>
            {
                pending.push(callee);
                pending.extend(arguments);
            }
            TypedExprKind::Local(id) => {
                if let Some(&index) = parameters.get(id) {
                    if param.replace(index).is_some_and(|other| other != index) {
                        return Arg::Unknown;
                    }
                } else if let Some(value) = aliases.get(id) {
                    // An alias reached twice (two branches, two elements) is followed once.
                    if seen.insert(*id) {
                        pending.push(value);
                    }
                } else {
                    return Arg::Unknown;
                }
            }
            TypedExprKind::Cast(inner)
            | TypedExprKind::Borrow(inner, false)
            | TypedExprKind::BorrowOperand(inner)
            | TypedExprKind::Dereference(inner) => pending.push(inner),
            TypedExprKind::Block { result, .. } => pending.push(result),
            TypedExprKind::If {
                then_branch,
                else_branch,
                ..
            } => {
                pending.push(then_branch);
                pending.push(else_branch);
            }
            TypedExprKind::Match { arms, .. } => pending.extend(arms.iter().map(|arm| &arm.body)),
            TypedExprKind::Array(values)
            | TypedExprKind::List(values)
            | TypedExprKind::Tuple(values) => pending.extend(values),
            TypedExprKind::Record(fields) => pending.extend(fields.iter().map(|(_, value)| value)),
            TypedExprKind::Construct {
                payload: Some(value),
                ..
            } => pending.push(value),
            TypedExprKind::Construct { payload: None, .. } => {}
            _ => return Arg::Unknown,
        }
    }
    match param {
        Some(index) if functions.is_empty() => Arg::Param(index),
        Some(_) => Arg::Unknown,
        None => {
            functions.sort_unstable();
            functions.dedup();
            Arg::Known(functions)
        }
    }
}

/// Whether a value of this type can carry a function or task value into the function.
fn open_type(ty: &Type, types: &TypeContext<'_>) -> bool {
    types.reaches(ty, |part| {
        matches!(part, Type::Function(..) | Type::Dyn(_) | Type::Task(_))
            || part.is_owned_function(types)
            || matches!(part, Type::Record(id, _) if types.records[*id].origin == ModuleOrigin::Std
                && matches!(types.records[*id].name.as_str(), "Seq.Seq" | "Async.Async" | "Async.Next"))
    })
}

struct Scan<'s, 'a> {
    state: &'s mut State<'a>,
    function: &'a CheckedFunction,
    local: Local,
    aliases: BTreeMap<usize, &'a TypedExpr>,
    parameters: BTreeMap<usize, usize>,
    facts: Option<crate::ranges::RangeFacts>,
    staying: BTreeSet<usize>,
    /// The copies of function values (`CopyCost::Environment`) that no local of a function
    /// without captures explains yet.
    environment_copies: BTreeSet<(usize, usize, usize)>,
}

impl<'s, 'a> Scan<'s, 'a> {
    fn source(&mut self, effect: Effect, span: Span, code: &'static str) {
        self.source_why(effect, span, why(code));
    }

    fn source_why(&mut self, effect: Effect, span: Span, why: Why) {
        if !self.state.computes(effect) {
            return;
        }
        self.local.source(bit(effect), span, why);
        if self.state.options.sites {
            self.local.sites.push(Site {
                effect,
                span,
                why: why.code,
                exempt: false,
            });
        }
    }

    fn exempt(&mut self, effect: Effect, span: Span, code: &'static str) {
        if self.state.computes(effect) && self.state.options.sites {
            self.local.sites.push(Site {
                effect,
                span,
                why: code,
                exempt: true,
            });
        }
    }

    fn unknown(&mut self, index: usize, span: Span, code: &'static str) {
        self.local.source(index, span, why(code));
    }

    fn builtin(&mut self, builtin: Builtin, span: Span) {
        let facts = builtin_facts(builtin);
        let why = builtin_why(builtin);
        if let Some(effect) = facts.effect {
            self.source_why(effect, span, why);
        }
        if facts.dynamic {
            self.unknown(DYN, span, "dyn");
        }
        if facts.alloc {
            self.source_why(Effect::Alloc, span, why);
        }
        if facts.trap {
            self.source_why(Effect::Trap, span, why);
        } else if let Some(reason) = facts.exempt_trap {
            self.exempt(Effect::Trap, span, reason);
        }
        if facts.diverge {
            self.source(Effect::Diverge, span, "blocking");
        }
    }

    fn classify(&mut self, value: &'a TypedExpr) -> Arg {
        let state = &mut *self.state;
        argument(value, &self.aliases, &self.parameters, &mut |ty| {
            state.is_open(ty)
        })
    }

    /// A call of a function value: of a parameter (Phase 6) or of a value the analysis cannot
    /// follow. The arguments that can carry functions may be called too.
    fn value_call(
        &mut self,
        callee: Option<&'a TypedExpr>,
        arguments: &[&'a TypedExpr],
        span: Span,
    ) {
        // A call of a known function through an immutable alias, a partial application or a
        // branch is a call of that function too, so its callbacks are checked as at a direct
        // call. A value that the classification follows to functions but `targets` does not is
        // a call that cannot be followed.
        if let Some(callee) = callee {
            match self.targets(callee) {
                Some(targets) => {
                    for (target, bound) in targets {
                        let mut supplied = bound;
                        supplied.extend(arguments.iter().copied());
                        self.record_call(target, supplied, span);
                    }
                }
                None => {
                    if matches!(self.classify(callee), Arg::Known(functions) if !functions.is_empty())
                    {
                        self.unknown(VALUE_CALL, span, "value-call");
                    }
                }
            }
        }
        let mut values: Vec<&'a TypedExpr> = callee.into_iter().collect();
        values.extend(arguments.iter().copied());
        for value in values {
            if !self.state.is_open(&value.ty) {
                continue;
            }
            match self.classify(value) {
                Arg::Param(index) if index < PARAMETER_BITS => self.local.param_calls |= 1 << index,
                // A call of a parameter beyond the bit set is a call that cannot be followed.
                Arg::Param(_) | Arg::Unknown => self.unknown(VALUE_CALL, span, "value-call"),
                Arg::Known(_) => {}
            }
        }
    }

    /// The functions that a function value is one of, with the arguments that each already binds
    /// (captures, then the arguments of a partial application): followed through immutable
    /// locals, borrows, casts, blocks and the branches of `if` and `match`, as `argument` follows
    /// them. `None` for a value that cannot be followed to functions.
    fn targets(&self, value: &'a TypedExpr) -> Option<Vec<(usize, Vec<&'a TypedExpr>)>> {
        use TypedExprKind as K;
        let mut pending = vec![value];
        let mut found = Vec::new();
        let mut seen = BTreeSet::new();
        let mut steps = 0;
        while let Some(value) = pending.pop() {
            steps += 1;
            if steps > 256 {
                return None;
            }
            match &value.kind {
                K::Function(FunctionRef::User(id)) => found.push((*id, Vec::new())),
                K::Closure(id, captures) => found.push((*id, captures.iter().collect())),
                K::Call(inner, bound) if matches!(&inner.ty, Type::Function(parameters, _) if bound.len() < parameters.len()) => {
                    match &inner.kind {
                        K::Function(FunctionRef::User(id)) => {
                            found.push((*id, bound.iter().collect()))
                        }
                        K::Closure(id, captures) => {
                            found.push((*id, captures.iter().chain(bound.iter()).collect()))
                        }
                        _ => return None,
                    }
                }
                K::Local(id) if !self.parameters.contains_key(id) => {
                    let alias = self.aliases.get(id)?;
                    if seen.insert(*id) {
                        pending.push(alias);
                    }
                }
                K::Borrow(inner, false)
                | K::BorrowOperand(inner)
                | K::Dereference(inner)
                | K::Cast(inner) => pending.push(inner),
                K::Block { result, .. } => pending.push(result),
                K::If {
                    then_branch,
                    else_branch,
                    ..
                } => {
                    pending.push(then_branch);
                    pending.push(else_branch);
                }
                K::Match { arms, .. } => pending.extend(arms.iter().map(|arm| &arm.body)),
                _ => return None,
            }
        }
        Some(found)
    }

    /// A host import that takes a callback is lowered at each call (`llvm_imports`), so the
    /// copies of its string arguments are made in the caller.
    fn host_call_here(&mut self, callee: usize, span: Span) {
        let function = &self.state.module.functions[callee];
        if let TypedExprKind::HostCall(import, _) = &function.body.kind
            && import.callbacks
            && takes_string(function)
        {
            self.source(Effect::Alloc, span, "string-marshalling");
        }
    }

    /// The `Drop` implementations that dropping a value of type `ty` runs: a source of
    /// `user_drop` and an edge to each.
    fn user_drop(&mut self, ty: &Type, span: Span) {
        let drops = self.state.drop_functions(ty);
        if drops.is_empty() {
            return;
        }
        self.source(Effect::UserDrop, span, "user-drop");
        for id in drops {
            if id == usize::MAX {
                self.unknown(UNRESOLVED, span, "user-drop");
            } else {
                self.local.edges.push((id, span));
            }
        }
    }

    fn call_site(
        &mut self,
        callee: usize,
        captures: &'a [TypedExpr],
        arguments: &[&'a TypedExpr],
        span: Span,
    ) {
        let supplied: Vec<&'a TypedExpr> =
            captures.iter().chain(arguments.iter().copied()).collect();
        self.record_call(callee, supplied, span);
    }

    /// Records a call of `callee` with all the values it receives (captures first).
    fn record_call(&mut self, callee: usize, supplied: Vec<&'a TypedExpr>, span: Span) {
        let target = &self.state.module.functions[callee];
        let open: Vec<bool> = target
            .parameters
            .iter()
            .map(|parameter| parameter.ty.clone())
            .collect::<Vec<_>>()
            .iter()
            .map(|ty| self.state.is_open(ty))
            .collect();
        if !open.contains(&true) {
            return;
        }
        let arguments = supplied
            .iter()
            .enumerate()
            .map(|(index, value)| {
                if open.get(index).copied().unwrap_or(false) {
                    self.classify(value)
                } else {
                    Arg::Known(Vec::new())
                }
            })
            .collect();
        self.local.calls.push(CallSite {
            callee,
            span,
            arguments,
        });
    }

    fn scan(&mut self) -> Result<(), Diagnostic> {
        let function = self.function;
        let user_drops = !self.state.module.user_drops.is_empty();
        // The compact form replaces the body of `String.slice` and `String.join` with a runtime
        // call that builds the result; the rule counts it in both forms.
        if crate::llvm::compact_replacement(function).is_some() {
            self.source(Effect::Alloc, function.body.span, "string-build");
        }
        if user_drops {
            // The function drops the owned parameters that its body does not move on, so their
            // destructors run here even when no expression names them (review of #63). A
            // parameter read by default is a reference, and the environment of an owned function
            // outlives each call.
            let borrowed = if function.owned_captures {
                function.capture_count
            } else {
                0
            };
            for parameter in &function.parameters[borrowed..] {
                let span = if parameter.span == Span::default() {
                    function.body.span
                } else {
                    parameter.span
                };
                self.user_drop(&parameter.ty, span);
            }
        }
        self.scan_root(&function.body)
    }

    fn scan_root(&mut self, root: &'a TypedExpr) -> Result<(), Diagnostic> {
        let module = self.state.module;
        let types = module.types();
        let user_drops = !module.user_drops.is_empty();
        let mut entered = Vec::new();
        let mut stack = vec![Item::Node(root, Ctx::default())];
        while let Some(item) = stack.pop() {
            let (expression, ctx) = match item {
                Item::Enter(condition) => {
                    if let Some(facts) = &mut self.facts {
                        entered.push(facts.enter_condition(module, condition));
                    }
                    continue;
                }
                Item::EnterElse(condition) => {
                    if let Some(facts) = &mut self.facts {
                        entered.push(facts.enter_else_condition(module, condition));
                    }
                    continue;
                }
                Item::Leave => {
                    if let (Some(facts), Some(entered)) = (&mut self.facts, entered.pop()) {
                        facts.leave_condition(entered);
                    }
                    continue;
                }
                Item::Node(expression, ctx) => (expression, ctx),
            };
            self.local.nodes += 1;
            self.state.nodes += 1;
            if self.state.nodes > self.state.limits.nodes {
                return Err(limit_error(self.state.limits, expression.span));
            }
            if user_drops {
                self.user_drop(&expression.ty, expression.span);
            }
            let pushed = self.node(expression, ctx, &types, &mut stack);
            if !pushed {
                // A dereference or a cast of a function value is the same function.
                let consumer = if matches!(
                    expression.kind,
                    TypedExprKind::Dereference(_) | TypedExprKind::Cast(_)
                ) {
                    ctx.consumer
                } else {
                    Consumer::default()
                };
                let child = Ctx {
                    read: false,
                    pattern: ctx.pattern,
                    nested: false,
                    consumer,
                };
                for value in expression.children().into_iter().rev() {
                    stack.push(Item::Node(value, child));
                }
            }
        }
        Ok(())
    }

    /// The literal rule (A6, A7): a non-empty literal stays in the frame only when it is read in
    /// place and its tree fits the frame; otherwise it is copied to the heap.
    fn literal(&mut self, expression: &'a TypedExpr, ctx: Ctx, moved: &'static str) {
        if !ctx.read {
            self.source(Effect::Alloc, expression.span, moved);
            return;
        }
        if ctx.nested {
            return;
        }
        let bytes = crate::llvm::frame_bytes(expression, self.state.module);
        self.local.frame = self.local.frame.saturating_add(bytes);
        if bytes > MAX_VALUE_BYTES || self.local.frame > MAX_FRAME_BYTES {
            self.source(Effect::Alloc, expression.span, "literal-too-large");
        }
    }

    /// Counts the frame bytes of a record or tuple that stays in place (A7).
    fn aggregate(&mut self, expression: &'a TypedExpr, ctx: Ctx) {
        if ctx.read && !ctx.nested {
            let bytes = crate::llvm::frame_bytes(expression, self.state.module);
            self.local.frame = self.local.frame.saturating_add(bytes);
            if bytes > MAX_VALUE_BYTES || self.local.frame > MAX_FRAME_BYTES {
                self.source(Effect::Alloc, expression.span, "literal-too-large");
            }
        }
    }

    /// Classifies one node (appendix A of A20) and pushes the children whose context it decides.
    /// Returns whether it pushed the children.
    fn node(
        &mut self,
        expression: &'a TypedExpr,
        ctx: Ctx,
        types: &TypeContext<'_>,
        stack: &mut Vec<Item<'a>>,
    ) -> bool {
        use TypedExprKind as K;
        let span = expression.span;
        let read = |inherit: bool| Ctx {
            read: inherit,
            pattern: ctx.pattern,
            nested: false,
            consumer: Consumer::default(),
        };
        let element = Ctx {
            read: ctx.read,
            pattern: false,
            nested: ctx.read,
            consumer: Consumer::default(),
        };
        match &expression.kind {
            K::Error
            | K::GenericFunction(..)
            | K::Method(..)
            | K::TypeFunction { .. }
            | K::GenericInteger(..)
            | K::GenericFloat(_)
            | K::Lambda { .. }
            | K::CaseConstructor { .. } => {
                self.unknown(UNRESOLVED, span, "unresolved");
                true
            }
            K::Local(local) => {
                if self.aliases.contains_key(local) {
                    self.local
                        .alias_uses
                        .entry(*local)
                        .or_default()
                        .push(ctx.consumer);
                }
                // Copying a function value without captures copies no environment.
                if !self.environment_copies.is_empty()
                    && matches!(
                        self.aliases.get(local).map(|value| &value.kind),
                        Some(K::Closure(_, captures)) if captures.is_empty()
                    )
                    || matches!(
                        self.aliases.get(local).map(|value| &value.kind),
                        Some(K::Function(FunctionRef::User(_)))
                    )
                {
                    self.environment_copies.remove(&position(span));
                }
                false
            }
            K::Int(_)
            | K::Float(_)
            | K::Bool(_)
            | K::Unit
            | K::Break
            | K::Continue
            | K::Static(_)
            | K::RaisedException => false,
            K::String(literal) => {
                let empty = match literal {
                    StringLiteral::Utf16(units) => units.is_empty(),
                    StringLiteral::Utf8(text) => text.is_empty(),
                };
                if !empty {
                    self.literal(expression, ctx, "string-literal-moved");
                }
                false
            }
            K::Function(FunctionRef::User(id)) => {
                self.local.edges.push((*id, span));
                if !self.state.module.functions[*id].origin.asserts.is_empty() {
                    self.local.references.push((*id, span, ctx.consumer));
                }
                false
            }
            K::Function(FunctionRef::Builtin(instance)) => {
                self.builtin(instance.builtin, span);
                if builtin_facts(instance.builtin).callbacks && !ctx.read {
                    self.unknown(VALUE_CALL, span, "value-call");
                }
                false
            }
            K::Unary(..) => false,
            K::Binary(operator, left, right) => {
                let integer = matches!(expression.ty, Type::Integer(..));
                match operator {
                    BinaryOp::Add if left.ty.is_string() => {
                        self.source(Effect::Alloc, span, "string-concat");
                    }
                    BinaryOp::Divide | BinaryOp::Remainder if integer => {
                        let exempt = match (&right.kind, &right.ty) {
                            (K::Int(value), Type::Integer(bits, signed)) => {
                                *value != 0
                                    && (!*signed
                                        || *value != (u128::MAX >> (128 - u32::from(*bits))))
                            }
                            _ => false,
                        };
                        if exempt {
                            self.exempt(Effect::Trap, span, "literal-divisor");
                        } else {
                            self.source(Effect::Trap, span, "integer-division");
                        }
                    }
                    BinaryOp::Power if integer => {
                        let exempt = match (&right.kind, &expression.ty) {
                            (_, Type::Integer(_, false)) => true,
                            (K::Int(value), Type::Integer(bits, true)) => {
                                *value < (1u128 << (bits - 1))
                            }
                            _ => false,
                        };
                        if exempt {
                            self.exempt(Effect::Trap, span, "unsigned-or-literal-exponent");
                        } else {
                            self.source(Effect::Trap, span, "negative-exponent");
                        }
                    }
                    _ => {}
                }
                if *operator == BinaryOp::Pipe {
                    match &right.kind {
                        K::Function(FunctionRef::User(id)) | K::Closure(id, _) => {
                            let captures: &'a [TypedExpr] = match &right.kind {
                                K::Closure(_, captures) => captures,
                                _ => &[],
                            };
                            self.host_call_here(*id, span);
                            self.call_site(*id, captures, &[left.as_ref()], span);
                        }
                        K::Call(callee, arguments)
                            if matches!(
                                callee.kind,
                                K::Function(FunctionRef::User(_)) | K::Closure(..)
                            ) =>
                        {
                            let (K::Function(FunctionRef::User(id)) | K::Closure(id, _)) =
                                &callee.kind
                            else {
                                unreachable!()
                            };
                            let captures: &'a [TypedExpr] = match &callee.kind {
                                K::Closure(_, captures) => captures,
                                _ => &[],
                            };
                            let supplied: Vec<&'a TypedExpr> = arguments
                                .iter()
                                .chain(std::iter::once(left.as_ref()))
                                .collect();
                            self.host_call_here(*id, span);
                            self.call_site(*id, captures, &supplied, span);
                        }
                        _ => self.value_call(Some(right), &[left.as_ref()], span),
                    }
                    stack.push(Item::Node(
                        right,
                        Ctx {
                            consumer: Consumer {
                                supplied: 1,
                                alias: None,
                            },
                            ..read(true)
                        },
                    ));
                    stack.push(Item::Node(left, read(false)));
                    return true;
                }
                if matches!(
                    operator,
                    BinaryOp::Equal
                        | BinaryOp::NotEqual
                        | BinaryOp::Less
                        | BinaryOp::LessEqual
                        | BinaryOp::Greater
                        | BinaryOp::GreaterEqual
                ) {
                    stack.push(Item::Node(right, read(true)));
                    stack.push(Item::Node(left, read(true)));
                    return true;
                }
                false
            }
            K::Call(callee, arguments) => {
                // A call borrows the environment of the function value that it calls.
                if !self.environment_copies.is_empty() {
                    let mut target = callee.as_ref();
                    while let K::Dereference(inner)
                    | K::BorrowOperand(inner)
                    | K::Borrow(inner, false) = &target.kind
                    {
                        target = inner;
                    }
                    if matches!(target.kind, K::Local(_)) {
                        self.environment_copies.remove(&position(target.span));
                        self.environment_copies.remove(&position(callee.span));
                    }
                }
                let values: Vec<&'a TypedExpr> = arguments.iter().collect();
                if let K::Function(FunctionRef::User(id)) = &callee.kind {
                    self.host_call_here(*id, span);
                }
                match &callee.kind {
                    K::Function(FunctionRef::User(id)) => self.call_site(*id, &[], &values, span),
                    K::Closure(id, captures) => self.call_site(*id, captures, &values, span),
                    K::Function(FunctionRef::Builtin(instance)) => {
                        if builtin_facts(instance.builtin).callbacks {
                            self.value_call(None, &values, span);
                        }
                    }
                    _ => self.value_call(Some(callee), &values, span),
                }
                if matches!(&callee.ty, Type::Function(parameters, _) if arguments.len() < parameters.len())
                {
                    self.source(Effect::Alloc, span, "partial-application");
                }
                for value in arguments.iter().rev() {
                    stack.push(Item::Node(value, read(false)));
                }
                // The callee receives these arguments; a partial application is used as its
                // result is, with these arguments already supplied.
                let consumer = if matches!(&callee.ty, Type::Function(parameters, _) if arguments.len() < parameters.len())
                {
                    Consumer {
                        supplied: ctx.consumer.supplied.saturating_add(arguments.len()),
                        alias: ctx.consumer.alias,
                    }
                } else {
                    Consumer {
                        supplied: arguments.len(),
                        alias: None,
                    }
                };
                stack.push(Item::Node(
                    callee,
                    Ctx {
                        consumer,
                        ..read(true)
                    },
                ));
                true
            }
            K::HostCall(..) => {
                // E15: a person vouched for this property of the extern function.
                let vouched = self.state.options.trusted.is_some_and(|bit| {
                    self.function
                        .origin
                        .trusted
                        .is_some_and(|bits| bits & bit != 0)
                });
                if !vouched {
                    self.source(Effect::Extern, span, "extern-call");
                }
                // The call itself may copy a string argument for the host (into UTF-16, when the
                // program's strings are compact): an allocation of the marshalling, which no
                // property of the C function covers.
                if takes_string(self.function) {
                    self.source(Effect::Alloc, span, "string-marshalling");
                }
                self.exempt(Effect::Trap, span, "host-call");
                false
            }
            K::Closure(id, captures) => {
                self.local.edges.push((*id, span));
                if !captures.is_empty() || matches!(expression.ty, Type::Task(_)) {
                    self.source(Effect::Alloc, span, "closure-environment");
                }
                if !captures.is_empty() {
                    self.call_site(*id, captures, &[], span);
                }
                false
            }
            K::TaskRun(_) => {
                self.source(Effect::Task, span, "task");
                self.source(Effect::Alloc, span, "task");
                self.unknown(VALUE_CALL, span, "value-call");
                false
            }
            K::TaskParallel(_) | K::TaskParallelResults(_) => {
                self.source(Effect::Task, span, "task");
                self.source(Effect::Alloc, span, "task");
                false
            }
            K::Parallel(builtin, arguments) => {
                let facts = builtin_facts(*builtin);
                if let Some(effect) = facts.effect {
                    self.source(effect, span, "parallel");
                }
                self.source(Effect::Alloc, span, "parallel");
                if facts.trap {
                    self.source_why(Effect::Trap, span, builtin_why(*builtin));
                }
                if facts.diverge {
                    self.source(Effect::Diverge, span, "blocking");
                }
                if facts.callbacks {
                    let values: Vec<&'a TypedExpr> = arguments.iter().collect();
                    self.value_call(None, &values, span);
                }
                false
            }
            K::StructuralCompare(..) | K::StructuralHash(_) => false,
            K::StructuralDisplay(_) => {
                self.source(Effect::Alloc, span, "string-build");
                false
            }
            K::Interpolated(_) => {
                self.source(Effect::Alloc, span, "string-build");
                false
            }
            K::If {
                condition,
                then_branch,
                else_branch,
            } => {
                let branch = Ctx {
                    read: ctx.read,
                    pattern: ctx.pattern,
                    nested: ctx.nested,
                    consumer: ctx.consumer,
                };
                stack.push(Item::Leave);
                stack.push(Item::Node(else_branch, branch));
                stack.push(Item::EnterElse(condition));
                stack.push(Item::Leave);
                stack.push(Item::Node(then_branch, branch));
                stack.push(Item::Enter(condition));
                stack.push(Item::Node(condition, read(false)));
                true
            }
            K::While { .. } => {
                self.source(Effect::Diverge, span, "while");
                false
            }
            K::ForRange { step, .. } => {
                if matches!(step.kind, K::Int(value) if value != 0) {
                    self.exempt(Effect::Trap, span, "literal-step");
                } else {
                    self.source(Effect::Trap, span, "range-step");
                }
                false
            }
            K::ForEach { source, body, .. } => {
                stack.push(Item::Node(body, read(false)));
                stack.push(Item::Node(source, read(true)));
                true
            }
            K::Match { value, arms, .. } => {
                let pattern = Ctx {
                    read: false,
                    pattern: true,
                    nested: false,
                    consumer: Consumer::default(),
                };
                let body = Ctx {
                    read: ctx.read,
                    pattern: ctx.pattern,
                    nested: ctx.nested,
                    consumer: ctx.consumer,
                };
                for arm in arms.iter().rev() {
                    stack.push(Item::Node(&arm.body, body));
                    if let Some(guard) = &arm.guard {
                        stack.push(Item::Node(guard, read(false)));
                    }
                    for alternative in arm.alternatives.iter().rev() {
                        for (_, value) in alternative.bindings.iter().rev() {
                            stack.push(Item::Node(value, pattern));
                        }
                        for step in alternative.steps.iter().rev() {
                            stack.push(Item::Node(step.expression(), pattern));
                        }
                    }
                }
                stack.push(Item::Node(value, read(false)));
                true
            }
            K::Block { bindings, result } => {
                for (local, value) in bindings {
                    if !local.mutable {
                        self.aliases.insert(local.id, value);
                    }
                }
                stack.push(Item::Node(
                    result,
                    Ctx {
                        read: ctx.read,
                        pattern: ctx.pattern,
                        nested: ctx.nested,
                        consumer: ctx.consumer,
                    },
                ));
                for (local, value) in bindings.iter().rev() {
                    let stays = !local.mutable && self.staying.contains(&local.id);
                    // The uses of an immutable local decide how its function value is called.
                    let consumer = Consumer {
                        supplied: 0,
                        alias: (!local.mutable).then_some(local.id),
                    };
                    stack.push(Item::Node(
                        value,
                        Ctx {
                            consumer,
                            ..read(stays)
                        },
                    ));
                }
                true
            }
            K::Record(fields) => {
                self.aggregate(expression, ctx);
                for (_, value) in fields.iter().rev() {
                    stack.push(Item::Node(value, element));
                }
                true
            }
            K::Tuple(values) => {
                self.aggregate(expression, ctx);
                for value in values.iter().rev() {
                    stack.push(Item::Node(value, element));
                }
                true
            }
            K::Array(values) => {
                if matches!(expression.ty, Type::Array(_)) && !values.is_empty() {
                    self.literal(expression, ctx, "array-literal-moved");
                } else {
                    self.aggregate(expression, ctx);
                }
                for value in values.iter().rev() {
                    stack.push(Item::Node(value, element));
                }
                true
            }
            K::List(values) => {
                if !values.is_empty() {
                    self.literal(expression, ctx, "list-literal-moved");
                }
                for value in values.iter().rev() {
                    stack.push(Item::Node(value, element));
                }
                true
            }
            K::RecordUpdate { .. } => false,
            K::ListTail(..) => {
                self.exempt(Effect::Trap, span, "pattern-projection");
                false
            }
            K::NewArray(length, initializer) | K::NewList(length, initializer) => {
                self.source(Effect::Alloc, span, "new-collection");
                // The initializer is called with each index, as the callback of `Array.init` is
                // (review 3 of #63).
                self.value_call(None, &[initializer.as_ref()], span);
                let non_negative = matches!(length.kind, K::Length(_) | K::StringLength(_))
                    || matches!(length.kind, K::Int(value) if value < 1 << 63);
                if non_negative {
                    self.exempt(Effect::Trap, span, "non-negative-length");
                } else {
                    self.source(Effect::Trap, span, "allocation-size");
                }
                false
            }
            K::NewLiteral(_) => {
                self.source(Effect::Alloc, span, "new-literal");
                false
            }
            K::Field(..) => false,
            K::Index(array, index) => {
                let verdict = match &array.ty {
                    Type::FixedArray(_, length) => match (&**length, &index.kind) {
                        (Type::Length(count), K::Int(value)) if *value < u128::from(*count) => {
                            Some("fixed-literal-index")
                        }
                        _ => None,
                    },
                    _ if ctx.pattern && matches!(index.kind, K::Int(_)) => {
                        Some("pattern-projection")
                    }
                    _ if self
                        .facts
                        .as_ref()
                        .is_some_and(|facts| facts.index_in_bounds(array, index)) =>
                    {
                        Some("proven-index")
                    }
                    _ => None,
                };
                match verdict {
                    Some(reason) => self.exempt(Effect::Trap, span, reason),
                    None => self.source(Effect::Trap, span, "index"),
                }
                stack.push(Item::Node(index, read(false)));
                stack.push(Item::Node(
                    array,
                    Ctx {
                        read: true,
                        pattern: ctx.pattern,
                        nested: false,
                        consumer: Consumer::default(),
                    },
                ));
                true
            }
            K::Slice { value, start, end } => {
                self.source(Effect::Trap, span, "slice");
                for bound in [end, start].into_iter().flatten() {
                    stack.push(Item::Node(bound, read(false)));
                }
                stack.push(Item::Node(value, read(true)));
                true
            }
            K::Length(value) | K::StringLength(value) => {
                stack.push(Item::Node(value, read(true)));
                true
            }
            // A shared borrow of a function value is the same function.
            K::BorrowOperand(value) => {
                stack.push(Item::Node(
                    value,
                    Ctx {
                        consumer: ctx.consumer,
                        ..read(true)
                    },
                ));
                true
            }
            K::Borrow(value, mutable) => {
                let consumer = if *mutable {
                    Consumer::default()
                } else {
                    ctx.consumer
                };
                stack.push(Item::Node(
                    value,
                    Ctx {
                        consumer,
                        ..read(!*mutable)
                    },
                ));
                true
            }
            K::Dereference(_)
            | K::Assign(..)
            | K::Cast(_)
            | K::SaturatingIndex(_)
            | K::UnionTag(_)
            | K::UnionPayload { .. } => false,
            K::Construct { payload, .. } => {
                if payload.is_some() && types.recursive(&expression.ty) {
                    self.source(Effect::Alloc, span, "recursive-node");
                }
                false
            }
            K::Checked(_) => {
                self.source(Effect::Trap, span, "checked-arithmetic");
                false
            }
            K::Try(_) => false,
            K::Reraise => {
                self.source(Effect::Trap, span, "reraise");
                false
            }
            K::DynDispatch { .. } => {
                self.unknown(DYN, span, "dyn");
                false
            }
        }
    }
}

/// The components of the reached graph, callees before callers (Kosaraju, iterative).
fn components(graph: &BTreeMap<usize, Vec<usize>>) -> Vec<Vec<usize>> {
    let mut reverse: BTreeMap<usize, Vec<usize>> =
        graph.keys().map(|&id| (id, Vec::new())).collect();
    for (&caller, edges) in graph {
        for &callee in edges {
            if let Some(callers) = reverse.get_mut(&callee) {
                callers.push(caller);
            }
        }
    }
    let mut visited = BTreeSet::new();
    let mut order = Vec::new();
    for &root in graph.keys() {
        let mut stack = vec![(root, false)];
        while let Some((node, finishing)) = stack.pop() {
            if finishing {
                order.push(node);
            } else if visited.insert(node) {
                stack.push((node, true));
                stack.extend(
                    graph[&node]
                        .iter()
                        .rev()
                        .filter(|next| graph.contains_key(next))
                        .map(|&next| (next, false)),
                );
            }
        }
    }
    visited.clear();
    let mut components = Vec::new();
    for root in order.into_iter().rev() {
        if !visited.insert(root) {
            continue;
        }
        let mut component = vec![root];
        let mut next = 0;
        while next < component.len() {
            for &caller in &reverse[&component[next]] {
                if visited.insert(caller) {
                    component.push(caller);
                }
            }
            next += 1;
        }
        component.sort_unstable();
        components.push(component);
    }
    // Kosaraju finds the components in topological order of the graph, callers first.
    components.reverse();
    components
}

/// The summaries of `roots` and of every function that they reach.
pub(crate) fn summarize_from(
    module: &CheckedModule,
    options: Options,
    roots: &[usize],
) -> Result<FunctionEffects, Diagnostic> {
    summarize_roots(module, options, Limits::default(), Some(roots))
}

pub(crate) fn summarize_with(
    module: &CheckedModule,
    options: Options,
    limits: Limits,
) -> Result<FunctionEffects, Diagnostic> {
    summarize_roots(module, options, limits, None)
}

fn summarize_roots(
    module: &CheckedModule,
    options: Options,
    limits: Limits,
    given: Option<&[usize]>,
) -> Result<FunctionEffects, Diagnostic> {
    let count = module.functions.len();
    let mut result = FunctionEffects {
        summaries: vec![Summary::default(); count],
        computed: options.computed,
        reached: vec![false; count],
        locals: vec![EffectSet::EMPTY; count],
        witnesses: vec![[None; BITS]; count],
        sites: vec![Vec::new(); count],
        calls: vec![Vec::new(); count],
        references: vec![Vec::new(); count],
        cycle_targets: BTreeMap::new(),
        empty: Summary::default(),
    };
    let roots: Vec<usize> = match (given, options.scope) {
        (Some(given), _) => {
            let mut given = given.to_vec();
            given.sort_unstable();
            given.dedup();
            given
        }
        (None, Scope::All) => (0..count).collect(),
        (None, Scope::Reachable) => roots(module),
    };
    if roots.is_empty() {
        return Ok(result);
    }
    let mut copy_sites: Vec<Vec<&CopySite>> = vec![Vec::new(); count];
    let all_sites;
    if options.computed.contains(Effect::Copy) || options.computed.contains(Effect::Alloc) {
        all_sites = crate::copies::sites(module);
        for site in &all_sites {
            copy_sites[site.function].push(site);
        }
    }
    let mut state = State {
        module,
        options,
        copy_sites,
        drops: BTreeMap::new(),
        open: BTreeMap::new(),
        nodes: 0,
        limits,
    };
    let mut open = vec![false; count];
    let mut locals: BTreeMap<usize, Local> = BTreeMap::new();
    let mut pending: VecDeque<usize> = roots.iter().copied().collect();
    let mut queued: BTreeSet<usize> = roots.iter().copied().collect();
    while let Some(id) = pending.pop_front() {
        if locals.len() >= limits.functions {
            return Err(limit_error(limits, module.functions[id].span));
        }
        let function = &module.functions[id];
        open[id] = function
            .parameters
            .iter()
            .any(|parameter| state.is_open(&parameter.ty));
        let local = scan_function(&mut state, id)?;
        for &(callee, _) in &local.edges {
            if queued.insert(callee) {
                pending.push_back(callee);
            }
        }
        locals.insert(id, local);
    }
    let graph: BTreeMap<usize, Vec<usize>> = locals
        .iter()
        .map(|(&id, local)| {
            let mut edges: Vec<usize> = local.edges.iter().map(|&(callee, _)| callee).collect();
            edges.sort_unstable();
            edges.dedup();
            (id, edges)
        })
        .collect();
    let mut bits: BTreeMap<usize, Bits> = BTreeMap::new();
    for component in components(&graph) {
        let members: BTreeSet<usize> = component.iter().copied().collect();
        let cyclic = component.len() > 1 || graph[&component[0]].contains(&component[0]);
        if cyclic && options.computed.contains(Effect::Diverge) {
            for &id in &component {
                let local = locals.get_mut(&id).unwrap();
                if let Some(&(target, span)) = local
                    .edges
                    .iter()
                    .filter(|(callee, _)| members.contains(callee))
                    .min_by_key(|(_, span)| position(*span))
                {
                    result.cycle_targets.insert(id, target);
                    local.source(bit(Effect::Diverge), span, why("cycle"));
                    if options.sites {
                        local.sites.push(Site {
                            effect: Effect::Diverge,
                            span,
                            why: "cycle",
                            exempt: false,
                        });
                    }
                }
            }
        }
        // Phase 6: the parameters that a member calls, and the call sites that pass a value the
        // analysis cannot follow, until nothing changes. Each round only adds bits.
        for _ in 0..component.len() * 64 + 1 {
            let mut changed = false;
            for &id in &component {
                let mut calls_params = locals[&id].param_calls;
                let mut unknown_calls = Vec::new();
                for site in &locals[&id].calls {
                    let callee_params = if members.contains(&site.callee) {
                        locals[&site.callee].param_calls
                    } else {
                        result.summaries[site.callee].calls_params
                    };
                    for index in
                        (0..PARAMETER_BITS).filter(|index| callee_params & (1 << index) != 0)
                    {
                        // A parameter that the call does not supply comes from a later call of the
                        // value, whose arguments are classified there.
                        match site.arguments.get(index) {
                            Some(Arg::Known(_)) | None => {}
                            Some(Arg::Param(parameter)) if *parameter < PARAMETER_BITS => {
                                calls_params |= 1 << *parameter
                            }
                            // A parameter beyond the bit set cannot carry the obligation.
                            Some(Arg::Param(_) | Arg::Unknown) => unknown_calls.push(site.span),
                        }
                    }
                }
                let local = locals.get_mut(&id).unwrap();
                if calls_params != local.param_calls {
                    local.param_calls = calls_params;
                    changed = true;
                }
                for span in unknown_calls {
                    if !local.bits.contains(VALUE_CALL)
                        || local.first[VALUE_CALL]
                            .is_some_and(|(first, _)| position(span) < position(first))
                    {
                        local.source(VALUE_CALL, span, why("value-call"));
                        changed = true;
                    }
                }
            }
            if !changed {
                break;
            }
        }
        let mut union = Bits::default();
        for &id in &component {
            union.0 |= locals[&id].bits.0;
            for &(callee, _) in &locals[&id].edges {
                if members.contains(&callee) {
                    continue;
                }
                let mut callee_bits = bits[&callee];
                if !open[callee] {
                    // A closed callee only calls the function values that its own reach creates.
                    callee_bits.0 &= !(1 << VALUE_CALL);
                }
                union.0 |= callee_bits.0;
            }
        }
        for &id in &component {
            let mut own = union;
            if !open[id] {
                own.0 &= !(1 << VALUE_CALL);
            }
            bits.insert(id, union);
            let unknown = if union.contains(DYN) {
                Some(Unknown::Dyn)
            } else if union.contains(UNRESOLVED) {
                Some(Unknown::Unresolved)
            } else if own.contains(VALUE_CALL) {
                Some(Unknown::ValueCall)
            } else {
                None
            };
            result.summaries[id] = Summary {
                effects: union.effects().intersection(options.computed),
                unknown,
                calls_params: if open[id] { locals[&id].param_calls } else { 0 },
            };
            result.reached[id] = true;
            result.locals[id] = locals[&id].bits.effects().intersection(options.computed);
        }
        // The witnesses: a direct source, else the smallest call of a callee outside the component
        // that has the bit, else breadth-first from those through the component's callers.
        for index in 0..BITS {
            if !union.contains(index) {
                continue;
            }
            let mut queue = VecDeque::new();
            for &id in &component {
                let local = &locals[&id];
                if let Some((span, why)) = local.first[index] {
                    result.witnesses[id][index] = Some(Witness::Direct { span, why });
                    queue.push_back(id);
                    continue;
                }
                if let Some(&(callee, span)) = local
                    .edges
                    .iter()
                    .filter(|(callee, _)| {
                        !members.contains(callee)
                            && bits[callee].contains(index)
                            && (index != VALUE_CALL || open[*callee])
                    })
                    .min_by_key(|(_, span)| position(*span))
                {
                    result.witnesses[id][index] = Some(Witness::Via { callee, span });
                    queue.push_back(id);
                }
            }
            while let Some(callee) = queue.pop_front() {
                for &caller in &component {
                    if result.witnesses[caller][index].is_some() {
                        continue;
                    }
                    if let Some(&(_, span)) = locals[&caller]
                        .edges
                        .iter()
                        .filter(|(target, _)| *target == callee)
                        .min_by_key(|(_, span)| position(*span))
                    {
                        result.witnesses[caller][index] = Some(Witness::Via { callee, span });
                        queue.push_back(caller);
                    }
                }
            }
        }
    }
    for (id, local) in locals {
        let mut sites = local.sites;
        sites.sort_by_key(|site| (position(site.span), site.effect, site.exempt));
        sites.dedup();
        result.sites[id] = sites;
        let mut known = BTreeMap::new();
        result.references[id] = local
            .references
            .iter()
            .map(|&(function, span, consumer)| Reference {
                function,
                span,
                supplied: supplied(consumer, &local.alias_uses, &mut known, 0),
            })
            .collect();
        result.calls[id] = local.calls;
    }
    Ok(result)
}

/// The functions with an effect attribute, and, when one of them calls a parameter, the
/// functions that refer to it: their arguments are checked against its attribute (Phase 6).
fn roots(module: &CheckedModule) -> Vec<usize> {
    let attributed: BTreeSet<usize> = module
        .functions
        .iter()
        .enumerate()
        .filter(|(_, function)| !function.origin.asserts.is_empty())
        .map(|(id, _)| id)
        .collect();
    let mut roots = attributed.clone();
    let higher_order: BTreeSet<usize> = attributed
        .iter()
        .copied()
        .filter(|&id| {
            module.functions[id]
                .parameters
                .iter()
                .any(|parameter| open_type(&parameter.ty, &module.types()))
        })
        .collect();
    if !higher_order.is_empty() {
        for (id, function) in module.functions.iter().enumerate() {
            let mut pending = vec![&function.body];
            while let Some(expression) = pending.pop() {
                if let TypedExprKind::Function(FunctionRef::User(target))
                | TypedExprKind::Closure(target, _) = expression.kind
                    && higher_order.contains(&target)
                {
                    roots.insert(id);
                    break;
                }
                pending.extend(expression.children());
            }
        }
    }
    roots.into_iter().collect()
}

fn scan_function(state: &mut State<'_>, id: usize) -> Result<Local, Diagnostic> {
    let module = state.module;
    let function = &module.functions[id];
    let parameters = function
        .parameters
        .iter()
        .enumerate()
        .map(|(index, parameter)| (parameter.id, index))
        .collect();
    let facts = state
        .options
        .computed
        .contains(Effect::Trap)
        .then(|| crate::ranges::analyze(module, function, false));
    let staying = if state.options.computed.contains(Effect::Alloc) {
        crate::llvm::staying_locals(&function.body)
    } else {
        BTreeSet::new()
    };
    let mut scan = Scan {
        state,
        function,
        local: Local::default(),
        aliases: BTreeMap::new(),
        parameters,
        facts,
        staying,
        environment_copies: BTreeSet::new(),
    };
    let copy_sites = std::mem::take(&mut scan.state.copy_sites[id]);
    let user = function.origin.module == ModuleOrigin::User;
    for site in &copy_sites {
        if site.cost == CopyCost::Length {
            scan.source(Effect::Alloc, site.span, "copy-site");
            // Like W1006, the copies that std's own bodies make are the API they implement
            // (`Array.copy`): explicit at the call.
            if user {
                scan.source(Effect::Copy, site.span, "copy-site");
            }
        } else {
            scan.environment_copies.insert(position(site.span));
        }
    }
    scan.scan()?;
    // An environment copy that is not of a local bound to a function without captures.
    for (source, start, end) in std::mem::take(&mut scan.environment_copies) {
        let span = Span {
            start,
            end,
            source: (source != usize::MAX).then_some(source),
        };
        scan.source(Effect::Alloc, span, "environment-copy");
    }
    Ok(scan.local)
}

impl FunctionEffects {
    pub fn summary(&self, id: usize) -> &Summary {
        self.summaries.get(id).unwrap_or(&self.empty)
    }

    /// Whether the summary of `id` was computed (it is reached from a root).
    pub fn reached(&self, id: usize) -> bool {
        self.reached.get(id).copied().unwrap_or(false)
    }

    pub fn witness(&self, id: usize, effect: Effect) -> Option<&Witness> {
        self.witnesses.get(id)?[bit(effect)].as_ref()
    }

    fn witness_bit(&self, id: usize, index: usize) -> Option<&Witness> {
        self.witnesses.get(id)?[index].as_ref()
    }

    /// The chain of calls from `id` to the direct source of `effect`: each function and the span
    /// of the reference in its caller, `id` itself first with its direct source's span last.
    pub fn chain(&self, id: usize, effect: Effect) -> Vec<(usize, Span)> {
        self.chain_bit(id, bit(effect))
    }

    fn chain_bit(&self, id: usize, index: usize) -> Vec<(usize, Span)> {
        let mut chain = Vec::new();
        let mut current = id;
        let mut seen = BTreeSet::new();
        while seen.insert(current) {
            match self.witness_bit(current, index) {
                Some(Witness::Direct { span, .. }) => {
                    chain.push((current, *span));
                    break;
                }
                Some(Witness::Via { callee, span }) => {
                    chain.push((current, *span));
                    current = *callee;
                }
                None => break,
            }
        }
        chain
    }

    /// The direct source at the end of the chain of `effect` from `id`.
    fn direct(&self, id: usize, index: usize) -> Option<(usize, Span, Why)> {
        let (last, _) = *self.chain_bit(id, index).last()?;
        match self.witness_bit(last, index)? {
            Witness::Direct { span, why } => Some((last, *span, *why)),
            Witness::Via { .. } => None,
        }
    }

    /// The effects of the function's own body, without its callees and exemptions.
    pub fn local(&self, id: usize) -> EffectSet {
        self.locals.get(id).copied().unwrap_or_default()
    }

    /// The direct sources and exemptions of the body in source order, when `Options::sites` holds.
    pub fn sites(&self, id: usize) -> &[Site] {
        self.sites.get(id).map_or(&[], Vec::as_slice)
    }

    /// The facts rows (G26) of the reached functions.
    pub fn rows(&self, module: &CheckedModule) -> Vec<EffectRow> {
        let complete = self.computed == EffectSet::ALL;
        (0..module.functions.len())
            .filter(|&id| self.reached(id))
            .map(|id| {
                let summary = self.summary(id);
                let sites = summary
                    .effects
                    .iter()
                    .filter_map(|effect| {
                        let chain = self.chain(id, effect);
                        let (_, span, why) = self.direct(id, bit(effect))?;
                        Some(EffectSite {
                            effect: effect.name(),
                            span,
                            why: why.code,
                            via: chain
                                .iter()
                                .skip(1)
                                .map(|(function, _)| *function)
                                .collect(),
                        })
                    })
                    .collect();
                let exempt_sites = self
                    .sites(id)
                    .iter()
                    .filter(|site| site.exempt)
                    .map(|site| EffectSite {
                        effect: site.effect.name(),
                        span: site.span,
                        why: site.why,
                        via: Vec::new(),
                    })
                    .collect();
                EffectRow {
                    id,
                    effects: summary.effects.names(),
                    unknown: summary.unknown.map(Unknown::name),
                    sites,
                    exempt_sites,
                    complete,
                }
            })
            .collect()
    }
}

/// An effect attribute as a module declares it: what `validate` reports W1008 and the related
/// location of `E1031` at.
#[derive(Clone, Debug)]
pub struct DeclaredAttribute {
    /// The module key (`ModuleInput::name`) and the function's name in it.
    pub module: String,
    pub function: String,
    pub kind: EffectKind,
    pub span: Span,
    /// The declaration is a `const def`.
    pub constant: bool,
}

#[derive(Default, Debug)]
pub struct Validation {
    pub errors: Vec<Diagnostic>,
    pub warnings: Vec<Diagnostic>,
}

/// The `{what}` and `{help}` of a direct source (appendix J.3 of A20).
fn phrase(effect: Effect, code: &str) -> (&'static str, &'static str) {
    const GUARD: &str = "guard it ('if i >= 0 && i < xs.length then xs[i] else ...') or use 'Array.try_item' and match the Maybe";
    match (effect, code) {
        (_, "copy-site") => (
            "implicitly copies every element",
            "borrow it with 'ref', or call 'Array.copy' to make the copy explicit",
        ),
        (Effect::Alloc, "environment-copy") => (
            "copies a function value, whose environment may be heap allocated",
            "borrow the function with 'ref', or use it once",
        ),
        (Effect::Alloc, "new-collection" | "new-literal") => (
            "allocates an array or list",
            "write into an existing buffer ('ref mut [T..]' with 'Array.write'), or remove the attribute",
        ),
        (Effect::Alloc, "string-literal-moved" | "array-literal-moved" | "list-literal-moved") => (
            "leaves its slot, so it is copied to the heap",
            "borrow it ('ref') instead of passing or returning it",
        ),
        (Effect::Alloc, "literal-too-large") => (
            "is larger than 65536 bytes, so it is heap allocated",
            "use static data ('const') or a fixed-length array",
        ),
        (Effect::Alloc, "closure-environment" | "partial-application") => (
            "needs a closure environment",
            "use a named function, or a lambda that captures nothing",
        ),
        (Effect::Alloc, "string-concat" | "string-build") => (
            "builds a new string",
            "write bytes into a buffer you own, or remove the attribute",
        ),
        (Effect::Alloc, "recursive-node") => (
            "creates a heap-allocated node of a recursive union",
            "use an array and indices instead of a recursive union",
        ),
        (Effect::Alloc, "string-marshalling") => (
            "passes a string to an extern function, which may copy it for the host",
            "pass the bytes as a 'ref [i8u]' buffer, or remove the attribute",
        ),
        (Effect::Alloc, "task" | "parallel") => {
            ("creates tasks", "remove the tasks, or remove the attribute")
        }
        (Effect::Alloc, _) => (
            "allocates",
            "use a non-allocating alternative, or remove the attribute",
        ),
        (Effect::Trap, "index") => ("indexes without a proof that the index is in range", GUARD),
        (Effect::Trap, "slice") => ("slices without a proof that the range is valid", GUARD),
        (Effect::Trap, "integer-division") => (
            "divides by a value that may be zero",
            "check the divisor first, or divide by a non-zero literal",
        ),
        (Effect::Trap, "negative-exponent" | "allocation-size" | "range-step") => (
            "uses a value that may be negative or zero",
            "check it before use, or use a non-negative literal",
        ),
        (Effect::Trap, "checked-arithmetic" | "reraise") => (
            "can raise an unhandled OverflowException",
            "use the default (wrapping) arithmetic, or 'Int.checked_*' and match the Maybe",
        ),
        (Effect::Trap, _) => (
            "can trap",
            "check its arguments first, or remove the attribute",
        ),
        (Effect::Diverge, "while") => (
            "contains a 'while' loop, which may not terminate",
            "use 'for' over a range or a collection",
        ),
        (Effect::Diverge, "cycle") => (
            "is recursive",
            "'@terminates' accepts only non-recursive functions; use a loop over a range",
        ),
        (Effect::Diverge, _) => (
            "waits for an event outside the program",
            "remove the wait, or remove the attribute",
        ),
        (_, _) => ("has this effect", "remove the attribute"),
    }
}

fn unknown_phrase(unknown: Unknown) -> &'static str {
    match unknown {
        Unknown::Dyn => "uses dynamic dispatch ('dyn')",
        Unknown::Unresolved => "contains an expression that is not resolved",
        Unknown::ValueCall => "runs a function or task value that its caller can choose",
    }
}

fn violation_kind(kind: EffectKind) -> DiagnosticKind {
    match kind {
        EffectKind::NoAlloc => DiagnosticKind::EffectAllocInNoAlloc,
        EffectKind::NoCopy => DiagnosticKind::EffectCopyInNoCopy,
        EffectKind::NoTrap => DiagnosticKind::EffectTrapInNoTrap,
        EffectKind::Terminates => DiagnosticKind::EffectDivergeInTerminates,
    }
}

/// The declaration's name of a function: an instance's `.$mono.N` dropped.
fn declared_name(function: &CheckedFunction) -> String {
    function
        .qualified_name()
        .split(".$mono.")
        .next()
        .unwrap()
        .to_owned()
}

/// How a diagnostic names a function: a generated built-in wrapper by the built-in's name.
fn shown(module: &CheckedModule, id: usize) -> String {
    let function = &module.functions[id];
    if function.module == "$builtin" {
        let mut pending = vec![&function.body];
        while let Some(expression) = pending.pop() {
            if let TypedExprKind::Function(FunctionRef::Builtin(instance)) = &expression.kind {
                return instance.builtin.name().to_owned();
            }
            pending.extend(expression.children());
        }
    }
    declared_name(function)
}

/// Whether `id` is `root` or a helper that `root` generated (its lambdas), not a built-in wrapper.
fn in_family(module: &CheckedModule, mut id: usize, root: usize) -> bool {
    for _ in 0..module.functions.len() {
        if id == root {
            return true;
        }
        if module.functions[id].module == "$builtin" {
            return false;
        }
        match module.functions[id].origin.parent {
            Some(parent) if parent != id => id = parent,
            _ => return false,
        }
    }
    false
}

/// The function whose attributes a function carries: itself, or the function that generated it.
fn family_root(module: &CheckedModule, mut id: usize) -> usize {
    for _ in 0..module.functions.len() {
        let function = &module.functions[id];
        if !function.origin.asserts.is_empty() || function.module == "$builtin" {
            return id;
        }
        match function.origin.parent {
            Some(parent) if parent != id => id = parent,
            _ => return id,
        }
    }
    id
}

/// Where a chain of `explain` ends: the location, the callee outside the family (with its chain
/// text, `'\0'`-separated) or empty for a direct source, the related calls, the reason and the
/// function of the direct source.
struct Explained {
    span: Span,
    callee: String,
    related: Vec<(String, Span)>,
    why: Option<Why>,
    last: usize,
}

struct Report<'a> {
    module: &'a CheckedModule,
    effects: &'a FunctionEffects,
    attributes: &'a [DeclaredAttribute],
}

impl Report<'_> {
    fn attribute_span(&self, id: usize, kind: EffectKind) -> Option<Span> {
        let function = &self.module.functions[id];
        let name = function.name.split(".$mono.").next().unwrap();
        self.attributes
            .iter()
            .find(|attribute| {
                attribute.module == function.module
                    && attribute.function == name
                    && attribute.kind == kind
            })
            .map(|attribute| attribute.span)
    }

    /// The location, the reason and the related calls of the chain of `index` from `root`.
    fn explain(&self, root: usize, index: usize) -> Option<Explained> {
        let chain = self.effects.chain_bit(root, index);
        let (last, _) = *chain.last()?;
        let Witness::Direct { why, .. } = self.effects.witness_bit(last, index)? else {
            return None;
        };
        let outside = chain
            .iter()
            .position(|&(function, _)| !in_family(self.module, function, root));
        let related = chain
            .windows(2)
            .take(crate::diagnostic::MAX_RELATED - 1)
            .map(|pair| {
                (
                    format!("calls '{}'", shown(self.module, pair[1].0)),
                    pair[0].1,
                )
            })
            .collect();
        match outside {
            None => Some(Explained {
                span: chain.last()?.1,
                callee: String::new(),
                related,
                why: Some(*why),
                last,
            }),
            Some(at) => {
                let span = chain[at - 1].1;
                let callee = shown(self.module, chain[at].0);
                let rest: Vec<String> = chain[at..]
                    .iter()
                    .map(|&(function, _)| format!("'{}'", shown(self.module, function)))
                    .collect();
                let through = if rest.len() >= 2 && why.code != "cycle" {
                    let mut names = rest[..rest.len().min(4)].join(" -> ");
                    if rest.len() > 4 {
                        names.push_str(" -> ...");
                    }
                    format!(" (through {names})")
                } else {
                    String::new()
                };
                Some(Explained {
                    span,
                    callee: format!("{callee}\u{0}{through}"),
                    related,
                    why: Some(*why),
                    last,
                })
            }
        }
    }

    fn cycle(&self, function: usize) -> String {
        let target = self
            .effects
            .cycle_targets
            .get(&function)
            .copied()
            .unwrap_or(function);
        format!(
            "'{}' -> '{}'",
            shown(self.module, function),
            shown(self.module, target)
        )
    }

    /// The `E1031` of attribute `kind` on function `id`, or `None` when the summary keeps it.
    fn check(&self, id: usize, kind: EffectKind) -> Option<Diagnostic> {
        let summary = self.effects.summary(id);
        let forbidden = forbidden(kind).intersection(summary.effects);
        let mut candidates: Vec<usize> = forbidden.iter().map(bit).collect();
        match summary.unknown {
            Some(Unknown::Dyn) => candidates.push(DYN),
            Some(Unknown::Unresolved) => candidates.push(UNRESOLVED),
            Some(Unknown::ValueCall) => candidates.push(VALUE_CALL),
            None => {}
        }
        let index = candidates.into_iter().min_by_key(|&index| {
            self.effects
                .witness_bit(id, index)
                .map(|witness| match witness {
                    Witness::Direct { span, .. } | Witness::Via { span, .. } => position(*span),
                })
                .unwrap_or((usize::MAX, usize::MAX, usize::MAX))
        })?;
        let name = declared_name(&self.module.functions[id]);
        let attribute = kind.name();
        let Explained {
            span,
            callee,
            related,
            why,
            last,
        } = self.explain(id, index)?;
        let (diagnostic_kind, message, help) = if index == DYN
            || index == UNRESOLVED
            || index == VALUE_CALL
        {
            let unknown = summary.unknown?;
            (
                DiagnosticKind::EffectUnknownCallee,
                format!(
                    "'{name}' is marked '@{attribute}' but it {}; pass a named function or remove the attribute",
                    unknown_phrase(unknown)
                ),
                "pass a named function or remove the attribute".to_owned(),
            )
        } else if index == bit(Effect::Extern) {
            let host = shown(self.module, last);
            (
                DiagnosticKind::EffectExternInAttribute,
                format!(
                    "'{name}' is marked '@{attribute}' but this call reaches the extern function '{host}', whose effects are not known; remove the call or the attribute"
                ),
                "remove the call or the attribute".to_owned(),
            )
        } else {
            let effect = Effect::ALL[index];
            let why = why?;
            let (what, help) = phrase(effect, why.code);
            let what = if why.code == "cycle" {
                format!("{what} (through {})", self.cycle(last))
            } else {
                what.to_owned()
            };
            let because = if callee.is_empty() {
                format!("this expression {what}")
            } else {
                let (callee, through) = callee.split_once('\u{0}').unwrap();
                format!("this call reaches '{callee}', which {what}{through}")
            };
            (
                violation_kind(kind),
                format!("'{name}' is marked '@{attribute}' but {because}; {help}"),
                help.to_owned(),
            )
        };
        let mut diagnostic = Diagnostic::of(diagnostic_kind, message, span).with_help(help);
        for (message, at) in related {
            diagnostic = diagnostic.related(RelatedRole::ViaCall, message, at);
        }
        if let Some(at) = self.attribute_span(id, kind) {
            diagnostic =
                diagnostic.related(RelatedRole::RequiredBy, "required by this attribute", at);
        }
        Some(diagnostic)
    }

    /// Phase 6: a call of an attributed function that calls some of its parameters, from a
    /// function without the same attribute, passes functions whose effects must keep it.
    fn call_sites(
        &self,
        only: EffectKind,
        errors: &mut Vec<(Span, usize, EffectKind, Diagnostic)>,
    ) {
        for (caller, sites) in self.effects.calls.iter().enumerate() {
            let caller_root = family_root(self.module, caller);
            let caller_asserts = self.module.functions[caller_root].origin.asserts;
            for site in sites {
                let callee = &self.module.functions[site.callee];
                let calls_params = self.effects.summary(site.callee).calls_params;
                if callee.origin.asserts.is_empty() || calls_params == 0 {
                    continue;
                }
                for kind in callee.origin.asserts.kinds() {
                    if kind != only || caller_asserts.contains(kind) {
                        continue;
                    }
                    let name = declared_name(callee);
                    let attribute = kind.name();
                    for index in
                        (0..PARAMETER_BITS).filter(|index| calls_params & (1 << index) != 0)
                    {
                        let diagnostic = match site.arguments.get(index) {
                            None => continue,
                            Some(Arg::Known(functions)) => {
                                let Some(diagnostic) = functions.iter().find_map(|&function| {
                                    self.argument_violation(&name, kind, function, site.span)
                                }) else {
                                    continue;
                                };
                                diagnostic
                            }
                            Some(Arg::Param(_)) => {
                                let caller_name = declared_name(&self.module.functions[caller_root]);
                                let help = format!(
                                    "add '@{attribute}' to '{caller_name}' so that its callers are checked"
                                );
                                Diagnostic::of(
                                    DiagnosticKind::EffectUnknownCallee,
                                    format!(
                                        "'{name}' is marked '@{attribute}' but it {}; {help}",
                                        unknown_phrase(Unknown::ValueCall)
                                    ),
                                    site.span,
                                )
                                .with_help(help)
                            }
                            Some(Arg::Unknown) => Diagnostic::of(
                                DiagnosticKind::EffectUnknownCallee,
                                format!(
                                    "'{name}' is marked '@{attribute}' but it {}; pass a named function or remove the attribute",
                                    unknown_phrase(Unknown::ValueCall)
                                ),
                                site.span,
                            )
                            .with_help("pass a named function or remove the attribute"),
                        };
                        let diagnostic = match self.attribute_span(site.callee, kind) {
                            Some(at) => diagnostic.related(
                                RelatedRole::RequiredBy,
                                "required by this attribute",
                                at,
                            ),
                            None => diagnostic,
                        };
                        errors.push((site.span, site.callee, kind, diagnostic));
                        break;
                    }
                }
            }
        }
    }

    /// A reference to an attributed function that calls some of its parameters, used where the
    /// calls of the value do not supply all of them (stored, returned, passed on or captured):
    /// the functions that those calls pass are never checked, so the attribute cannot hold.
    fn escapes(&self, only: EffectKind, errors: &mut Vec<(Span, usize, EffectKind, Diagnostic)>) {
        for reference in self.effects.references.iter().flatten() {
            let calls_params = self.effects.summary(reference.function).calls_params;
            let unchecked = if reference.supplied >= PARAMETER_BITS {
                0
            } else {
                calls_params >> reference.supplied
            };
            if unchecked == 0 {
                continue;
            }
            let function = &self.module.functions[reference.function];
            let name = declared_name(function);
            for kind in function.origin.asserts.kinds() {
                if kind != only {
                    continue;
                }
                let attribute = kind.name();
                let help = format!(
                    "call '{name}' with the functions it calls, directly or through a 'let' that is only called"
                );
                let mut diagnostic = Diagnostic::of(
                    DiagnosticKind::EffectUnknownCallee,
                    format!(
                        "'{name}' is marked '@{attribute}' and calls the functions it is given, but here it is used as a value whose calls are not checked; {help}"
                    ),
                    reference.span,
                )
                .with_help(help);
                if let Some(at) = self.attribute_span(reference.function, kind) {
                    diagnostic = diagnostic.related(
                        RelatedRole::RequiredBy,
                        "required by this attribute",
                        at,
                    );
                }
                errors.push((reference.span, reference.function, kind, diagnostic));
            }
        }
    }

    /// The violation of attribute `kind` of `name` by a function that a call passes to it.
    fn argument_violation(
        &self,
        name: &str,
        kind: EffectKind,
        function: usize,
        span: Span,
    ) -> Option<Diagnostic> {
        let summary = self.effects.summary(function);
        let attribute = kind.name();
        if let Some(unknown) = summary.unknown {
            return Some(
                Diagnostic::of(
                    DiagnosticKind::EffectUnknownCallee,
                    format!(
                        "'{name}' is marked '@{attribute}' but it {}; pass a named function or remove the attribute",
                        unknown_phrase(unknown)
                    ),
                    span,
                )
                .with_help("pass a named function or remove the attribute"),
            );
        }
        let effect = forbidden(kind)
            .intersection(summary.effects)
            .iter()
            .next()?;
        let chain = self.effects.chain(function, effect);
        let (last, _) = *chain.last()?;
        let Some(Witness::Direct { why, .. }) = self.effects.witness(last, effect) else {
            return None;
        };
        let callee = shown(self.module, function);
        let mut diagnostic = if effect == Effect::Extern {
            Diagnostic::of(
                DiagnosticKind::EffectExternInAttribute,
                format!(
                    "'{name}' is marked '@{attribute}' but this call reaches the extern function '{}', whose effects are not known; remove the call or the attribute",
                    shown(self.module, last)
                ),
                span,
            )
            .with_help("remove the call or the attribute")
        } else {
            let (what, help) = phrase(effect, why.code);
            let what = if why.code == "cycle" {
                format!("{what} (through {})", self.cycle(last))
            } else {
                what.to_owned()
            };
            Diagnostic::of(
                violation_kind(kind),
                format!("'{name}' is marked '@{attribute}' but this call reaches '{callee}', which {what}; {help}"),
                span,
            )
            .with_help(help)
        };
        for pair in chain.windows(2).take(crate::diagnostic::MAX_RELATED - 1) {
            diagnostic = diagnostic.related(
                RelatedRole::ViaCall,
                format!("calls '{}'", shown(self.module, pair[1].0)),
                pair[0].1,
            );
        }
        Some(diagnostic)
    }
}

/// The diagnostics of the effect attributes: `E1031` for a summary that breaks one, `E1032` for
/// one that this compiler cannot check, and the `W1008` warnings.
pub fn validate(
    module: &CheckedModule,
    effects: Option<&FunctionEffects>,
    attributes: &[DeclaredAttribute],
) -> Validation {
    validate_trusted(module, effects, &BTreeMap::new(), attributes)
}

/// The attributes whose `@trusted` property some extern function of `module` states, and
/// which forbid `extern`: each needs its own summary (E15, D14).
pub fn trusted_kinds(module: &CheckedModule) -> Vec<EffectKind> {
    EffectKind::ALL
        .into_iter()
        .filter(|kind| {
            forbidden(*kind).contains(Effect::Extern)
                && module.functions.iter().any(|function| {
                    function
                        .origin
                        .trusted
                        .is_some_and(|bits| bits & trusted_property(*kind) != 0)
                })
        })
        .collect()
}

/// `validate` where the attributes of a kind in `trusted` are checked against that summary,
/// computed with the kind's `@trusted` property (E15).
pub fn validate_trusted(
    module: &CheckedModule,
    effects: Option<&FunctionEffects>,
    trusted: &BTreeMap<EffectKind, FunctionEffects>,
    attributes: &[DeclaredAttribute],
) -> Validation {
    let mut validation = Validation::default();
    let mut found: Vec<(Span, usize, EffectKind, Diagnostic)> = Vec::new();
    let empty = FunctionEffects {
        summaries: Vec::new(),
        computed: EffectSet::EMPTY,
        reached: Vec::new(),
        locals: Vec::new(),
        witnesses: Vec::new(),
        sites: Vec::new(),
        calls: Vec::new(),
        references: Vec::new(),
        cycle_targets: BTreeMap::new(),
        empty: Summary::default(),
    };
    let base = effects.unwrap_or(&empty);
    let report_of = |kind: EffectKind| Report {
        module,
        effects: trusted.get(&kind).unwrap_or(base),
        attributes,
    };
    let mut unavailable = BTreeSet::new();
    for attribute in attributes {
        if !SUPPORTED.contains(attribute_effect(attribute.kind)) {
            unavailable.insert((
                attribute.module.clone(),
                attribute.function.clone(),
                attribute.kind,
            ));
            validation.errors.push(Diagnostic::of(
                DiagnosticKind::EffectAttributeUnavailable,
                format!(
                    "'@{}' cannot be checked by this compiler yet; remove it",
                    attribute.kind.name()
                ),
                attribute.span,
            ));
            continue;
        }
        if attribute.constant {
            validation.warnings.push(Diagnostic::of(
                DiagnosticKind::EffectAttributeOnConstDef,
                format!(
                    "a const def is evaluated at compile time; '@{}' only checks its run-time body",
                    attribute.kind.name()
                ),
                attribute.span,
            ));
        }
        let instantiated = module.functions.iter().any(|function| {
            function.module == attribute.module
                && function.name.split(".$mono.").next() == Some(attribute.function.as_str())
                && function.origin.asserts.contains(attribute.kind)
        });
        if !instantiated {
            validation.warnings.push(Diagnostic::of(
                DiagnosticKind::EffectAttributeUncheckedGeneric,
                format!(
                    "'@{}' on '{}.{}' is checked for each instantiation, and nothing instantiates it",
                    attribute.kind.name(),
                    attribute.module,
                    attribute.function
                ),
                attribute.span,
            ));
        }
    }
    if effects.is_some() {
        for (id, function) in module.functions.iter().enumerate() {
            for kind in function.origin.asserts.kinds() {
                let name = function.name.split(".$mono.").next().unwrap().to_owned();
                if unavailable.contains(&(function.module.clone(), name, kind)) {
                    continue;
                }
                if let Some(diagnostic) = report_of(kind).check(id, kind) {
                    found.push((diagnostic.span, id, kind, diagnostic));
                }
            }
        }
        for kind in EffectKind::ALL {
            let report = report_of(kind);
            report.call_sites(kind, &mut found);
            report.escapes(kind, &mut found);
        }
    }
    // One diagnostic per declaration and attribute: the first in source order, of any instance.
    found.sort_by_key(|(span, _, _, _)| position(*span));
    let mut seen = BTreeSet::new();
    for (_, id, kind, diagnostic) in found {
        if seen.insert((declared_name(&module.functions[id]), kind)) {
            validation.errors.push(diagnostic);
        }
    }
    validation
        .errors
        .sort_by_key(|diagnostic| position(diagnostic.span));
    validation
        .warnings
        .sort_by_key(|diagnostic| position(diagnostic.span));
    validation
}

/// Why an impure source makes a contract condition impure, in A19's words (`E1029`).
fn impurity_reason(effect: Effect, why: Why) -> String {
    let named = |text: &str| match why.builtin {
        Some(builtin) => format!("{text} ('{}')", builtin.name()),
        None => text.to_owned(),
    };
    match (effect, why.builtin) {
        (_, Some(Builtin::BenchNow | Builtin::BenchConsume | Builtin::GenSeed)) => {
            named("reads the clock, a random source, or a benchmark counter")
        }
        (_, Some(Builtin::AllocCounter)) => "reads allocation counters".into(),
        (Effect::Io | Effect::Os | Effect::Net, _) => named("reads or writes the outside world"),
        (Effect::Debug, _) => "prints debug output".into(),
        (Effect::Task, _) => named("starts or waits for a task or a parallel operation"),
        (Effect::Atomic | Effect::Rc, _) => named("uses shared mutable state"),
        (Effect::Gpu, _) => named("uses the GPU"),
        (Effect::UserDrop, _) => "holds a value whose type has a user Drop".into(),
        _ => "cannot be proven pure".into(),
    }
}

/// A21: the first impure source of the contract conditions of each function, as `E1033`
/// (`contract_impure`): a source of an effect that A19's `pure` forbids, a `dyn` dispatch, an
/// unresolved expression, or a call of a function value that is not a pure function type.
/// `roots` holds each function with its conditions and the span of their contract lines.
pub(crate) fn impurities(
    module: &CheckedModule,
    roots: &[(usize, Vec<(&TypedExpr, Span)>)],
) -> Result<Vec<(usize, Diagnostic)>, Diagnostic> {
    let count = module.functions.len();
    // E15: an extern function marked `@trusted pure` is as pure as a `pure def`.
    let options = Options {
        trusted: crate::syntax::trusted_property_bit("pure"),
        ..Options::new(Scope::All, EffectSet::PURE_FORBIDDEN)
    };
    let mut state = State {
        module,
        options,
        copy_sites: vec![Vec::new(); count],
        drops: BTreeMap::new(),
        open: BTreeMap::new(),
        nodes: 0,
        limits: Limits::default(),
    };
    let mut scanned = Vec::new();
    let mut callees = Vec::new();
    for (function, conditions) in roots {
        let declaration = &module.functions[*function];
        for (condition, line) in conditions {
            let mut scan = Scan {
                state: &mut state,
                function: declaration,
                local: Local::default(),
                aliases: BTreeMap::new(),
                parameters: declaration
                    .parameters
                    .iter()
                    .enumerate()
                    .map(|(index, parameter)| (parameter.id, index))
                    .collect(),
                facts: None,
                staying: BTreeSet::new(),
                environment_copies: BTreeSet::new(),
            };
            scan.scan_root(condition)?;
            let local = scan.local;
            callees.extend(local.edges.iter().map(|(callee, _)| *callee));
            scanned.push((*function, *line, local));
        }
    }
    let summaries = summarize_from(module, options, &callees)?;
    let mut found = Vec::new();
    for (function, line, local) in scanned {
        let declaration = &module.functions[function];
        let mut candidates: Vec<(Span, String)> = Vec::new();
        for index in 0..BITS {
            if !local.bits.contains(index) {
                continue;
            }
            let Some((span, why)) = local.first[index] else {
                continue;
            };
            let reason = if index < 14 {
                let effect = Effect::ALL[index];
                if !EffectSet::PURE_FORBIDDEN.contains(effect) {
                    continue;
                }
                impurity_reason(effect, why)
            } else if index == DYN {
                "uses dyn dispatch".into()
            } else {
                "cannot be proven pure".into()
            };
            candidates.push((span, reason));
        }
        // A function value that the condition calls must be a pure function type.
        for (index, parameter) in declaration.parameters.iter().enumerate() {
            if index < PARAMETER_BITS
                && local.param_calls & (1 << index) != 0
                && parameter.ty.function_purity() != Some(true)
            {
                candidates.push((parameter.span, "cannot be proven pure".into()));
            }
        }
        for site in &local.calls {
            let calls_params = summaries.summary(site.callee).calls_params;
            let open = (0..PARAMETER_BITS)
                .filter(|index| calls_params & (1 << index) != 0)
                .any(|index| match site.arguments.get(index) {
                    Some(Arg::Param(parameter)) => declaration
                        .parameters
                        .get(*parameter)
                        .is_none_or(|parameter| parameter.ty.function_purity() != Some(true)),
                    Some(Arg::Unknown) => true,
                    Some(Arg::Known(_)) | None => false,
                });
            if open {
                candidates.push((site.span, "cannot be proven pure".into()));
            }
        }
        for &(callee, span) in &local.edges {
            let summary = summaries.summary(callee);
            let impure = summary.effects.intersection(EffectSet::PURE_FORBIDDEN);
            if let Some(effect) = impure.iter().next() {
                let chain = summaries.chain(callee, effect);
                let Some(&(last, _)) = chain.last() else {
                    continue;
                };
                let why = match summaries.witness(last, effect) {
                    Some(Witness::Direct { why, .. }) => *why,
                    _ => why_unknown(),
                };
                let mut reason = impurity_reason(effect, why);
                if effect == Effect::Extern {
                    reason = format!("calls the extern function '{}'", shown(module, last));
                }
                // A lambda of the condition: the source inside it; another function: the call.
                let outside = chain
                    .iter()
                    .position(|&(id, _)| !in_family(module, id, function));
                let at = match outside {
                    None => chain.last().map_or(span, |(_, at)| *at),
                    Some(0) => span,
                    Some(index) => chain[index - 1].1,
                };
                let names: Vec<String> = chain
                    .iter()
                    .skip(outside.unwrap_or(chain.len()))
                    .filter(|(id, _)| !module.functions[*id].module.starts_with('$'))
                    .map(|(id, _)| format!("'{}'", shown(module, *id)))
                    .collect();
                if !names.is_empty() {
                    let mut text = names[..names.len().min(4)].join(" -> ");
                    if names.len() > 4 {
                        text.push_str(" -> ...");
                    }
                    reason.push_str(&format!(" (through {text})"));
                }
                candidates.push((at, reason));
            } else if summary
                .unknown
                .is_some_and(|unknown| unknown != Unknown::ValueCall)
            {
                candidates.push((span, "cannot be proven pure".into()));
            }
        }
        if let Some((span, reason)) = candidates
            .into_iter()
            .min_by_key(|(span, _)| position(*span))
        {
            found.push((
                function,
                Diagnostic::of(
                    DiagnosticKind::ContractImpure,
                    format!(
                        "a contract must be pure: it {reason}; call only pure functions in a contract"
                    ),
                    span,
                )
                .related(RelatedRole::RequiredBy, "required by this contract line", line),
            ));
        }
    }
    found.sort_by_key(|(_, diagnostic)| position(diagnostic.span));
    Ok(found)
}

fn why_unknown() -> Why {
    why("unresolved")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn count(predicate: impl Fn(BuiltinFacts) -> bool) -> usize {
        Builtin::ALL
            .iter()
            .filter(|builtin| predicate(builtin_facts(**builtin)))
            .count()
    }

    #[test]
    fn effect_names_are_sorted_and_round_trip() {
        assert_eq!(Effect::ALL.len(), 14);
        let names: Vec<_> = Effect::ALL.iter().map(|effect| effect.name()).collect();
        let mut sorted = names.clone();
        sorted.sort_unstable();
        assert_eq!(names, sorted);
        for effect in Effect::ALL {
            assert_eq!(Effect::from_name(effect.name()), Some(effect));
            assert_eq!(effect as usize, bit(effect));
        }
        assert_eq!(Effect::from_name("pure"), None);
        assert_eq!(EffectSet::ALL.iter().count(), 14);
        assert_eq!(EffectSet::ALL.names(), names);
    }

    #[test]
    fn pure_forbidden_has_ten_effects() {
        assert_eq!(EffectSet::PURE_FORBIDDEN.iter().count(), 10);
        for effect in [Effect::Alloc, Effect::Copy, Effect::Diverge, Effect::Trap] {
            assert!(!EffectSet::PURE_FORBIDDEN.contains(effect));
        }
    }

    #[test]
    fn attributes_forbid_their_effect() {
        for kind in EffectKind::ALL {
            assert!(forbidden(kind).contains(attribute_effect(kind)));
            assert_eq!(
                forbidden(kind).contains(Effect::Extern),
                kind != EffectKind::NoCopy
            );
        }
    }

    /// The three tables of the built-in functions' effects agree: this one, A19's purity and the
    /// Level B effects of the facts (G26), which were written independently.
    #[test]
    fn every_builtin_has_a_verdict() {
        for &builtin in Builtin::ALL {
            let facts = builtin_facts(builtin);
            assert_eq!(
                facts.effect.is_some() || facts.dynamic,
                super::super::pure_rules::builtin_impure(builtin),
                "{builtin:?}"
            );
            assert_eq!(
                facts.effect.map(Effect::name),
                crate::kg::effects::builtin_effect(builtin),
                "{builtin:?}"
            );
            assert!(
                !(facts.trap && facts.exempt_trap.is_some()),
                "{builtin:?} both traps and is exempt"
            );
        }
        assert_eq!(Builtin::ALL.len(), 258);
        assert_eq!(count(|facts| facts.effect.is_some() || facts.dynamic), 91);
        assert_eq!(count(|facts| facts.effect.is_none() && !facts.dynamic), 167);
    }

    #[test]
    fn impure_builtins_are_pinned_by_effect() {
        let by = |effect| count(|facts| facts.effect == Some(effect));
        assert_eq!(by(Effect::Io), 10);
        assert_eq!(by(Effect::Os), 12);
        assert_eq!(by(Effect::Net), 12);
        assert_eq!(by(Effect::Task), 9);
        assert_eq!(by(Effect::Atomic), 21);
        assert_eq!(by(Effect::Rc), 18);
        assert_eq!(by(Effect::Debug), 3);
        assert_eq!(by(Effect::Gpu), 5);
        assert_eq!(count(|facts| facts.dynamic), 1);
    }

    #[test]
    fn alloc_builtins_are_pinned() {
        assert_eq!(count(|facts| facts.alloc), 57);
    }

    #[test]
    fn trap_builtins_are_pinned() {
        assert_eq!(count(|facts| facts.trap), 32);
        assert_eq!(count(|facts| facts.exempt_trap.is_some()), 37);
    }

    #[test]
    fn diverge_builtins_are_pinned() {
        assert_eq!(count(|facts| facts.diverge), 16);
    }

    #[test]
    fn callback_builtins_are_pinned() {
        assert_eq!(count(|facts| facts.callbacks), 16);
    }

    fn checked(source: &str) -> CheckedModule {
        crate::analyze(source).unwrap_or_else(|error| panic!("{}: {}", error.code, error.message))
    }

    #[test]
    fn honors_the_exact_limits() {
        let module = checked(
            "def a :: i64 -> i64 = \\n -> n + 1\ndef b :: i64 -> i64 = \\n -> a n * 2\n\
             def c :: i64 -> i64 = \\n -> b n - a n\n@no_copy\ndef d :: i64 -> i64 = \\n -> c n\n\
             def main :: unit -> i32 = \\() -> 0\n",
        );
        let options = Options::new(Scope::Reachable, EffectSet::ALL);
        let reached = summarize(&module, options).unwrap();
        let functions = (0..module.functions.len())
            .filter(|&id| reached.reached(id))
            .count();
        let nodes: usize = 4 + 9 + 2;
        let exact = Limits {
            functions,
            nodes: usize::MAX,
        };
        assert!(summarize_with(&module, options, exact).is_ok());
        let error = summarize_with(
            &module,
            options,
            Limits {
                functions: functions - 1,
                nodes: usize::MAX,
            },
        )
        .unwrap_err();
        assert_eq!(error.code, "E1017");
        assert_eq!(error.kind, Some("effect_summary_limit"));
        let total = {
            let mut low = 1;
            let mut high = 1 << 20;
            while low < high {
                let middle = (low + high) / 2;
                let limits = Limits {
                    functions: usize::MAX,
                    nodes: middle,
                };
                if summarize_with(&module, options, limits).is_ok() {
                    high = middle;
                } else {
                    low = middle + 1;
                }
            }
            low
        };
        assert!(total >= nodes);
        assert!(
            summarize_with(
                &module,
                options,
                Limits {
                    functions: usize::MAX,
                    nodes: total
                }
            )
            .is_ok()
        );
        assert!(
            summarize_with(
                &module,
                options,
                Limits {
                    functions: usize::MAX,
                    nodes: total - 1
                }
            )
            .is_err()
        );
    }

    #[test]
    fn no_attribute_means_no_summary_work() {
        let module =
            checked("def f :: i64 -> i64 = \\n -> n + 1\ndef main :: unit -> i32 = \\() -> 0\n");
        assert!(!needed(&module));
        assert!(module.effects.is_none());
        let empty = summarize(&module, Options::checking(&module)).unwrap();
        assert!((0..module.functions.len()).all(|id| !empty.reached(id)));
    }

    #[test]
    fn components_put_callees_first() {
        let graph: BTreeMap<usize, Vec<usize>> =
            BTreeMap::from([(0, vec![1]), (1, vec![2]), (2, vec![1, 3]), (3, vec![])]);
        assert_eq!(components(&graph), vec![vec![3], vec![1, 2], vec![0]]);
    }
}
