//! W36: how a `@c_layout` record crosses a C call by value.
//!
//! LLVM IR does not lower a struct that is passed or returned by value to the C ABI: a front end
//! turns it into integers, floats, arrays or a pointer for each target, as Clang does. This module
//! is the one place that does it for Tsuzuri. It reads the layout of the record from W21
//! (`host_abi::c_layout_fields`) and the target from `Platform`, and nothing here depends on a
//! `declare`: a call through a function pointer (`objc_msgSend`, a COM method) classifies its
//! records with the same functions.
//!
//! The rules follow Clang, and `tests/c_layout_by_value.mjs` compares every `declare` of
//! `tests/fixtures/c_layout_by_value` with the one Clang prints for the same C prototype
//! (`tests/c_layout_by_value_reference.c`) on each target.
//!
//! | target | argument | result |
//! | --- | --- | --- |
//! | Windows x64 | 1, 2, 4, 8 bytes: `iN`; other sizes: `ptr` to a copy | the same sizes: `iN`; other sizes: `sret` |
//! | AArch64 (Windows, Linux, macOS) | HFA of 1 to 4 `float` or `double`: `[n x T]` (`alignstack(8)` on Linux); up to 8 bytes: `i64`; up to 16: `[2 x i64]`; larger: `ptr` to a copy | HFA: `{ T, ... }`; up to 8 bytes: `i{8 * size}`; up to 16: `[2 x i64]`; larger: `sret` |
//! | System V x86-64 | over 16 bytes or an unaligned field: `byval`; else one or two eightbytes, while registers are left | the same eightbytes as one value or `{ a, b }`; memory: `sret` |

use super::*;
use crate::target::Platform;

/// One scalar of a `@c_layout` record: a number or a pointer-width field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Leaf {
    /// `i8`, `i16`, `i32` or `i64` (the sign does not change how a struct passes).
    Int(u32),
    F32,
    F64,
    /// An extern handle, `Maybe` of one, `FnPtr` or a borrowed NUL-terminated string.
    Pointer,
}

impl Leaf {
    /// The size of the leaf, which is also its natural C alignment.
    pub(crate) fn size(self) -> usize {
        match self {
            Self::Int(bits) => bits as usize / 8,
            Self::F32 => 4,
            Self::F64 | Self::Pointer => 8,
        }
    }

    fn float(self) -> bool {
        matches!(self, Self::F32 | Self::F64)
    }
}

/// The size, the alignment and the scalars of a `@c_layout` record, with nested records and
/// fixed-length arrays opened: everything the classification needs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CLayoutLeaves {
    pub size: usize,
    pub align: usize,
    /// `(offset, leaf)` in ascending offset order. Empty for a record over `LARGEST_IN_REGISTERS`
    /// bytes, which every target passes in memory whatever its fields are.
    pub leaves: Vec<(usize, Leaf)>,
}

/// The largest struct that any supported target passes or returns in registers: an AArch64 HFA of
/// four `double`.
const LARGEST_IN_REGISTERS: usize = 32;

impl CLayoutLeaves {
    /// The leaves of the `@c_layout` record `ty`, from the one layout computation of W21
    /// (`c_layout_fields`); `None` when `ty` is not a `@c_layout` record.
    pub(crate) fn of(ty: &Type, module: &CheckedModule) -> Option<Self> {
        let (_, size, align) = host_abi::c_layout_fields(ty, module)?;
        let mut leaves = Vec::new();
        if size <= LARGEST_IN_REGISTERS {
            Self::open(ty, module, 0, &mut leaves);
        }
        Some(Self {
            size,
            align,
            leaves,
        })
    }

    fn open(ty: &Type, module: &CheckedModule, base: usize, leaves: &mut Vec<(usize, Leaf)>) {
        let Type::Record(id, arguments) = ty else {
            return;
        };
        let Some((fields, _, _)) = host_abi::c_layout_fields(ty, module) else {
            return;
        };
        for (field, layout) in module
            .types()
            .record_fields(*id, arguments)
            .iter()
            .zip(&fields)
        {
            let offset = base + layout.offset;
            match field {
                Type::Record(..) => Self::open(field, module, offset, leaves),
                Type::FixedArray(element, _) => {
                    let leaf = Self::scalar(element);
                    for index in 0..fixed_length(field) {
                        leaves.push((offset + index * leaf.size(), leaf));
                    }
                }
                _ => leaves.push((offset, Self::scalar(field))),
            }
        }
    }

    fn scalar(ty: &Type) -> Leaf {
        match ty {
            Type::Integer(bits, _) => Leaf::Int(u32::from(*bits)),
            Type::Binary(32) => Leaf::F32,
            Type::Binary(64) => Leaf::F64,
            _ => Leaf::Pointer,
        }
    }

    /// A homogeneous floating-point aggregate: one to four leaves, all `float` or all `double`,
    /// that fill the record. Returns whether they are `double`, and how many there are.
    fn hfa(&self) -> Option<(bool, u8)> {
        let (_, first) = *self.leaves.first()?;
        let homogeneous = first.float()
            && self.leaves.len() <= 4
            && self.leaves.iter().all(|(_, leaf)| *leaf == first)
            && self.leaves.len() * first.size() == self.size;
        homogeneous.then_some((first == Leaf::F64, self.leaves.len() as u8))
    }
}

/// One eightbyte of a System V struct that passes in registers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Eightbyte {
    /// A general-purpose register: `i64`, or `iN` when only the first `N / 8` bytes hold data.
    Integer(u32),
    Double,
    Float,
    /// Two `float` in one SSE register: `<2 x float>`.
    FloatPair,
}

impl Eightbyte {
    fn ty(self) -> String {
        match self {
            Self::Integer(bits) => format!("i{bits}"),
            Self::Double => "double".into(),
            Self::Float => "float".into(),
            Self::FloatPair => "<2 x float>".into(),
        }
    }

    fn sse(self) -> bool {
        !matches!(self, Self::Integer(_))
    }

    /// `(allocation size, ABI alignment)` of the IR type on x86-64.
    fn layout(self) -> (usize, usize) {
        match self {
            Self::Integer(bits) => {
                let size = (bits as usize / 8).next_power_of_two();
                (size, size)
            }
            Self::Float => (4, 4),
            Self::Double | Self::FloatPair => (8, 8),
        }
    }
}

/// How a `@c_layout` record crosses a C call by value on the target being emitted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ByValue {
    /// One integer: `iN` (Windows x64 for 1, 2, 4 and 8 bytes; AArch64 `i64` arguments and
    /// `i{8 * size}` results; a System V struct of at most 8 bytes that goes to the stack after
    /// the integer registers ran out).
    Integer(u32),
    /// AArch64, 9 to 16 bytes that are not an HFA: `[2 x i64]`.
    IntegerPair,
    /// AArch64 HFA: `[n x float|double]` as an argument (`alignstack(8)` on Linux), `{ float, ... }`
    /// as a result.
    Floats {
        double: bool,
        count: u8,
        align_stack: bool,
    },
    /// System V: one or two eightbytes, each `i64`, `iN`, `double`, `float` or `<2 x float>`.
    Eightbytes(Vec<Eightbyte>),
    /// A copy in memory: a `ptr` to a fresh copy (Windows, AArch64), `ptr byval([N x i8]) align 8`
    /// (System V), or as a result the first parameter `ptr sret([N x i8])`, `N` being the size.
    Memory,
}

/// Why a record cannot pass by value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Unsupported {
    /// The target has no classification here: an architecture other than x86-64 and AArch64, or a
    /// record aligned to more than 8 bytes (`abi_record_by_value_target`).
    Target,
    /// WebAssembly (`abi_native_only`).
    Wasm,
}

/// One lowered parameter of a call, as far as the registers of System V count it.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Position<'a> {
    /// An integer, a `bool` or a pointer: one general-purpose register.
    Integer,
    /// A `float` or a `double`: one SSE register.
    Sse,
    /// A `@c_layout` record by value.
    Record(&'a CLayoutLeaves),
}

/// The C calling convention of a target, as far as structs by value are concerned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Abi {
    WindowsX64,
    /// AAPCS64. Windows, macOS and Linux differ only in `alignstack(8)` on an HFA argument.
    Aarch64 {
        align_stack: bool,
    },
    SystemV,
}

impl Abi {
    fn of(platform: Platform) -> Result<Self, Unsupported> {
        if platform.aarch64() {
            Ok(Self::Aarch64 {
                align_stack: platform.linux(),
            })
        } else if platform.arch_name() != "x86_64" {
            Err(Unsupported::Target)
        } else if platform.windows() {
            Ok(Self::WindowsX64)
        } else {
            Ok(Self::SystemV)
        }
    }
}

/// The class of one record alone. It is the final answer on Windows and AArch64, where each
/// position is classified by itself, and for every result. A System V argument also depends on the
/// registers that the earlier parameters used: `classify_call` decides it.
pub(crate) fn classify(
    layout: &CLayoutLeaves,
    platform: Platform,
    result: bool,
) -> Result<ByValue, Unsupported> {
    let abi = Abi::of(platform)?;
    // W21 aligns a field to at most 8 bytes. A record aligned to 16 would need `i128` on AArch64
    // and other stack alignment on System V, which no test here covers.
    if layout.align > 8 {
        return Err(Unsupported::Target);
    }
    Ok(match abi {
        Abi::WindowsX64 => match layout.size {
            // No HFA on Windows x64: `{ float, float }` is one integer register.
            size @ (1 | 2 | 4 | 8) => ByValue::Integer(size as u32 * 8),
            _ => ByValue::Memory,
        },
        Abi::Aarch64 { align_stack } => {
            if let Some((double, count)) = layout.hfa() {
                ByValue::Floats {
                    double,
                    count,
                    align_stack: align_stack && !result,
                }
            } else if layout.size <= 8 {
                // An argument fills the register; a result is exactly as wide as the record.
                ByValue::Integer(if result { layout.size as u32 * 8 } else { 64 })
            } else if layout.size <= 16 {
                ByValue::IntegerPair
            } else {
                ByValue::Memory
            }
        }
        Abi::SystemV => system_v(layout),
    })
}

/// System V x86-64: the eightbytes of a record that passes in registers, or `Memory`.
fn system_v(layout: &CLayoutLeaves) -> ByValue {
    // A field that `pack` moved off its natural alignment sends the whole record to memory.
    if layout.size > 16
        || layout
            .leaves
            .iter()
            .any(|(offset, leaf)| offset % leaf.size() != 0)
    {
        return ByValue::Memory;
    }
    let mut parts = Vec::new();
    for start in (0..layout.size).step_by(8) {
        let inside = || {
            layout
                .leaves
                .iter()
                .filter(move |(offset, _)| (start..start + 8).contains(offset))
        };
        // An eightbyte of padding only has no class and no register.
        let Some((first_offset, first)) = inside().next().copied() else {
            continue;
        };
        let leaf_at = |offset: usize| {
            inside()
                .find(|(at, _)| *at == offset)
                .map(|(_, leaf)| *leaf)
        };
        let part = if inside().all(|(_, leaf)| leaf.float()) {
            match leaf_at(start) {
                Some(Leaf::F32) if leaf_at(start + 4) == Some(Leaf::F32) => Eightbyte::FloatPair,
                Some(Leaf::F32) => Eightbyte::Float,
                _ => Eightbyte::Double,
            }
        } else {
            let rest = (layout.size - start).min(8) as u32 * 8;
            match first {
                _ if first_offset != start => Eightbyte::Integer(rest),
                Leaf::Pointer | Leaf::Int(64) => Eightbyte::Integer(64),
                // A narrow integer keeps its width only when the rest of the eightbyte is padding.
                Leaf::Int(bits) if inside().count() == 1 => Eightbyte::Integer(bits),
                _ => Eightbyte::Integer(rest),
            }
        };
        parts.push(part);
    }
    // The second eightbyte must start at offset 8 of the IR pair `{ a, b }`: a short first part
    // widens when the alignment of the second would not put it there.
    if let [low, high] = parts.as_mut_slice() {
        let (size, _) = low.layout();
        let (_, align) = high.layout();
        if size.div_ceil(align) * align != 8 {
            *low = if low.sse() {
                Eightbyte::Double
            } else {
                Eightbyte::Integer(64)
            };
        }
    }
    if parts.is_empty() {
        ByValue::Memory
    } else {
        ByValue::Eightbytes(parts)
    }
}

/// The classes of the records among the parameters of one call, in order (one entry for each
/// `Position::Record`). `result` is the record that the call returns by value, if any: its class is
/// `classify(result, platform, true)`, and where that is `Memory` its `sret` pointer uses a register.
///
/// System V counts registers over the whole call, left to right: six integer and eight SSE
/// registers. A record whose eightbytes do not all fit goes to memory as a whole, and the scalars
/// after it still use the registers that are left. The other targets classify each position alone
/// (LLVM places `[n x T]` wholly in registers or wholly on the stack).
pub(crate) fn classify_call(
    parameters: &[Position<'_>],
    result: Option<&CLayoutLeaves>,
    platform: Platform,
) -> Result<Vec<ByValue>, Unsupported> {
    let records = parameters.iter().filter_map(|position| match position {
        Position::Record(layout) => Some(*layout),
        _ => None,
    });
    if Abi::of(platform)? != Abi::SystemV {
        return records
            .map(|layout| classify(layout, platform, false))
            .collect();
    }
    let mut integer = 6usize;
    let mut sse = 8usize;
    if let Some(layout) = result
        && classify(layout, platform, true)? == ByValue::Memory
    {
        integer -= 1;
    }
    let mut classes = Vec::new();
    for position in parameters {
        match position {
            Position::Integer => integer = integer.saturating_sub(1),
            Position::Sse => sse = sse.saturating_sub(1),
            Position::Record(layout) => {
                // Memory with no integer register left: Clang passes a record of at most 8 bytes
                // as one integer, which lands in the same stack slot as the `byval` copy would.
                let memory = |integer: usize| {
                    if integer == 0 && layout.size <= 8 {
                        ByValue::Integer(layout.size as u32 * 8)
                    } else {
                        ByValue::Memory
                    }
                };
                classes.push(match classify(layout, platform, false)? {
                    ByValue::Eightbytes(parts) => {
                        let floats = parts.iter().filter(|part| part.sse()).count();
                        let integers = parts.len() - floats;
                        if integers <= integer && floats <= sse {
                            integer -= integers;
                            sse -= floats;
                            ByValue::Eightbytes(parts)
                        } else {
                            memory(integer)
                        }
                    }
                    _ => memory(integer),
                });
            }
        }
    }
    Ok(classes)
}

impl ByValue {
    /// The IR types of the argument with their parameter attributes, as the `declare` and the call
    /// site both spell them. The pointee of `byval` and of `sret` is `[size x i8]`, not the named
    /// type of the record: only its size counts, and two records of one C shape (two declarations
    /// of one symbol, W14) then give the same `declare` line.
    pub(crate) fn argument_types(&self, layout: &CLayoutLeaves, platform: Platform) -> Vec<String> {
        match self {
            Self::Integer(bits) => vec![format!("i{bits}")],
            Self::IntegerPair => vec!["[2 x i64]".into()],
            Self::Floats {
                double,
                count,
                align_stack,
            } => vec![format!(
                "[{count} x {}]{}",
                if *double { "double" } else { "float" },
                if *align_stack { " alignstack(8)" } else { "" }
            )],
            Self::Eightbytes(parts) => parts.iter().map(|part| part.ty()).collect(),
            Self::Memory if Abi::of(platform) == Ok(Abi::SystemV) => {
                vec![format!("ptr byval([{} x i8]) align 8", layout.size)]
            }
            Self::Memory => vec!["ptr".into()],
        }
    }

    /// The IR type that a call returning the record has: `void` for `Memory`, whose call takes
    /// `sret_parameter` first.
    pub(crate) fn result_type(&self) -> String {
        match self {
            Self::Integer(bits) => format!("i{bits}"),
            Self::IntegerPair => "[2 x i64]".into(),
            Self::Floats { double, count, .. } => {
                let element = if *double { "double" } else { "float" };
                format!("{{ {} }}", vec![element; usize::from(*count)].join(", "))
            }
            Self::Eightbytes(parts) => match parts.as_slice() {
                [part] => part.ty(),
                parts => format!(
                    "{{ {} }}",
                    parts
                        .iter()
                        .map(|part| part.ty())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            },
            Self::Memory => "void".into(),
        }
    }

    /// The first parameter of a call whose result is `Memory`. It is not an ordinary leading
    /// pointer: AArch64 passes it in `x8`.
    pub(crate) fn sret_parameter(layout: &CLayoutLeaves) -> String {
        format!("ptr sret([{} x i8])", layout.size)
    }

    /// How many bytes the temporary of a record with this class needs: the record, and the IR values
    /// that are loaded from it or stored to it (an AArch64 `i64` argument of a 3-byte record reads 8).
    pub(crate) fn storage(&self, layout: &CLayoutLeaves) -> usize {
        let size = layout.size;
        size.max(match self {
            Self::Integer(bits) => (*bits as usize).div_ceil(8),
            Self::IntegerPair => 16,
            Self::Floats { double, count, .. } => usize::from(*count) * if *double { 8 } else { 4 },
            Self::Eightbytes(parts) => parts.len() * 8,
            Self::Memory => size,
        })
    }
}

/// Rejects, before any function is emitted, the reachable `extern` functions whose by-value records
/// the target cannot pass: every one on WebAssembly, and on a native target those that
/// `classify` refuses. `host_call` relies on this.
pub(crate) fn validate(
    module: &CheckedModule,
    reachable: &BTreeSet<usize>,
    wasm: bool,
) -> Result<(), Diagnostic> {
    let types = module.types();
    let platform = crate::target::current_platform();
    for &id in reachable {
        let function = &module.functions[id];
        let TypedExprKind::HostCall(import, _) = &function.body.kind else {
            continue;
        };
        if import.lowers_inline() {
            continue;
        }
        let signature = &function.signature;
        let refused = signature
            .parameters
            .iter()
            .map(|ty| (ty, false))
            .chain([(&signature.result, true)])
            .filter(|(ty, _)| {
                crate::abi::extern_record_passing(ty, &types)
                    == Some(crate::abi::RecordPassing::ByValue)
            })
            .find_map(|(ty, result)| {
                let reason = if wasm {
                    Some(Unsupported::Wasm)
                } else {
                    CLayoutLeaves::of(ty, module)
                        .and_then(|layout| classify(&layout, platform, result).err())
                };
                reason.map(|reason| (ty, result, reason))
            });
        if let Some((ty, result, reason)) = refused {
            let qualified = ty.display(&types);
            let record = qualified.rsplit('.').next().unwrap_or(&qualified);
            let name = &function.name;
            let verb = if result { "returns" } else { "passes" };
            let pointer = if result {
                format!("return it through a 'ref mut {record}' parameter")
            } else {
                format!("declare the parameter 'ref {record}'")
            };
            return Err(match reason {
                Unsupported::Wasm => Diagnostic::of(
                    DiagnosticKind::AbiNativeOnly,
                    format!(
                        "extern '{name}' {verb} the '@c_layout' record '{record}' by value, which only native targets support; build for a native target"
                    ),
                    function.span,
                ),
                Unsupported::Target => Diagnostic::of(
                    DiagnosticKind::AbiRecordByValueTarget,
                    format!(
                        "extern '{name}' {verb} the '@c_layout' record '{record}' by value, which this compiler supports on x86-64 and AArch64 for records aligned to at most 8 bytes; {pointer} if the C function takes a pointer"
                    ),
                    function.span,
                ),
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::target::Triple;

    const WINDOWS_X64: [Triple; 2] = [Triple::X86_64WindowsMsvc, Triple::X86_64WindowsGnu];
    const WINDOWS_ARM64: [Triple; 2] = [Triple::Aarch64WindowsMsvc, Triple::Aarch64WindowsGnu];
    const LINUX_ARM64: [Triple; 2] = [Triple::Aarch64LinuxGnu, Triple::Aarch64LinuxMusl];
    const LINUX_X64: [Triple; 2] = [Triple::X86_64LinuxGnu, Triple::X86_64LinuxMusl];

    /// The leaves of a C struct whose fields have their natural alignment, in declaration order.
    fn natural(fields: &[Leaf]) -> CLayoutLeaves {
        packed(fields, 8)
    }

    fn packed(fields: &[Leaf], pack: usize) -> CLayoutLeaves {
        let mut leaves = Vec::new();
        let mut offset = 0usize;
        let mut align = 1usize;
        for leaf in fields {
            let step = leaf.size().min(pack);
            offset = offset.div_ceil(step) * step;
            leaves.push((offset, *leaf));
            offset += leaf.size();
            align = align.max(step);
        }
        CLayoutLeaves {
            size: offset.div_ceil(align) * align,
            align,
            leaves,
        }
    }

    fn platform(triple: Triple) -> Platform {
        Platform::new(Some(triple))
    }

    /// `argument / result` as the `declare` spells them, with `sret` for a result in memory.
    fn spelled(layout: &CLayoutLeaves, triple: Triple) -> String {
        let platform = platform(triple);
        let argument = classify(layout, platform, false)
            .unwrap()
            .argument_types(layout, platform)
            .join(", ");
        let result = match classify(layout, platform, true).unwrap() {
            ByValue::Memory => "sret".to_owned(),
            class => class.result_type(),
        };
        format!("{argument} / {result}")
    }

    use Leaf::{F32, F64, Int, Pointer};

    /// The rows of the reference table of the ticket (Clang 22.1.8, and Apple Clang 21 for the
    /// macOS column), copied by hand: Windows x64, Windows Arm64, System V. AArch64 Linux and
    /// macOS are the Windows Arm64 column with and without `alignstack(8)` on an HFA argument.
    #[test]
    fn classes_match_the_reference_table() {
        let byte = Int(8);
        let rows: Vec<(&str, CLayoutLeaves, &str, &str, &str)> = vec![
            (
                "{int x, y}",
                natural(&[Int(32), Int(32)]),
                "i64 / i64",
                "i64 / i64",
                "i64 / i64",
            ),
            (
                "{float x, y}",
                natural(&[F32, F32]),
                "i64 / i64",
                "[2 x float] / { float, float }",
                "<2 x float> / <2 x float>",
            ),
            (
                "{int l, t, r, b}",
                natural(&[Int(32); 4]),
                "ptr / sret",
                "[2 x i64] / [2 x i64]",
                "i64, i64 / { i64, i64 }",
            ),
            (
                "{unsigned char r, g, b}",
                natural(&[byte; 3]),
                "ptr / sret",
                "i64 / i24",
                "i24 / i24",
            ),
            (
                "{double x, y}",
                natural(&[F64, F64]),
                "ptr / sret",
                "[2 x double] / { double, double }",
                "double, double / { double, double }",
            ),
            (
                "{float x, y, z}",
                natural(&[F32; 3]),
                "ptr / sret",
                "[3 x float] / { float, float, float }",
                "<2 x float>, float / { <2 x float>, float }",
            ),
            (
                "{float f}",
                natural(&[F32]),
                "i32 / i32",
                "[1 x float] / { float }",
                "float / float",
            ),
            (
                "{double d}",
                natural(&[F64]),
                "i64 / i64",
                "[1 x double] / { double }",
                "double / double",
            ),
            (
                "{short s}",
                natural(&[Int(16)]),
                "i16 / i16",
                "i64 / i16",
                "i16 / i16",
            ),
            (
                "{double d; long long i}",
                natural(&[F64, Int(64)]),
                "ptr / sret",
                "[2 x i64] / [2 x i64]",
                "double, i64 / { double, i64 }",
            ),
            (
                "{double d; float f}",
                natural(&[F64, F32]),
                "ptr / sret",
                "[2 x i64] / [2 x i64]",
                "double, float / { double, float }",
            ),
            (
                "{unsigned char b[5]}",
                natural(&[byte; 5]),
                "ptr / sret",
                "i64 / i40",
                "i40 / i40",
            ),
            (
                "{unsigned short a, b, c}",
                natural(&[Int(16); 3]),
                "ptr / sret",
                "i64 / i48",
                "i48 / i48",
            ),
            (
                "pack(1) {unsigned char tag; int value}",
                packed(&[byte, Int(32)], 1),
                "ptr / sret",
                "i64 / i40",
                "ptr byval([5 x i8]) align 8 / sret",
            ),
            (
                "{float v[4]}",
                natural(&[F32; 4]),
                "ptr / sret",
                "[4 x float] / { float, float, float, float }",
                "<2 x float>, <2 x float> / { <2 x float>, <2 x float> }",
            ),
            (
                "pack(2) {float a[3]; float b}",
                packed(&[F32; 4], 2),
                "ptr / sret",
                "[4 x float] / { float, float, float, float }",
                "<2 x float>, <2 x float> / { <2 x float>, <2 x float> }",
            ),
            (
                "pack(1) {float a, b}",
                packed(&[F32, F32], 1),
                "i64 / i64",
                "[2 x float] / { float, float }",
                "<2 x float> / <2 x float>",
            ),
            (
                "{float a, b, c, d, e}",
                natural(&[F32; 5]),
                "ptr / sret",
                "ptr / sret",
                "ptr byval([20 x i8]) align 8 / sret",
            ),
            (
                "{long long a, b, c}",
                natural(&[Int(64); 3]),
                "ptr / sret",
                "ptr / sret",
                "ptr byval([24 x i8]) align 8 / sret",
            ),
            (
                "{void *h; int x}",
                natural(&[Pointer, Int(32)]),
                "ptr / sret",
                "[2 x i64] / [2 x i64]",
                "i64, i32 / { i64, i32 }",
            ),
            // Rows that the ticket's table does not have, from the same Clang
            // (`tests/c_layout_by_value_reference.c`).
            (
                "{unsigned char b[7]}",
                natural(&[byte; 7]),
                "ptr / sret",
                "i64 / i56",
                "i56 / i56",
            ),
            (
                "{unsigned char b[9]}",
                natural(&[byte; 9]),
                "ptr / sret",
                "[2 x i64] / [2 x i64]",
                "i64, i8 / { i64, i8 }",
            ),
            (
                "{double x, y, w, h}",
                natural(&[F64; 4]),
                "ptr / sret",
                "[4 x double] / { double, double, double, double }",
                "ptr byval([32 x i8]) align 8 / sret",
            ),
            (
                "{signed char c; double d}",
                natural(&[byte, F64]),
                "ptr / sret",
                "[2 x i64] / [2 x i64]",
                "i8, double / { i8, double }",
            ),
            (
                "{int a; signed char b}",
                natural(&[Int(32), byte]),
                "i64 / i64",
                "i64 / i64",
                "i64 / i64",
            ),
            (
                "{int a, b, c}",
                natural(&[Int(32); 3]),
                "ptr / sret",
                "[2 x i64] / [2 x i64]",
                "i64, i32 / { i64, i32 }",
            ),
            (
                "{float f; int i}",
                natural(&[F32, Int(32)]),
                "i64 / i64",
                "i64 / i64",
                "i64 / i64",
            ),
            (
                "{float a, b; double c}",
                natural(&[F32, F32, F64]),
                "ptr / sret",
                "[2 x i64] / [2 x i64]",
                "<2 x float>, double / { <2 x float>, double }",
            ),
            (
                "{float a, b, c; int d}",
                natural(&[F32, F32, F32, Int(32)]),
                "ptr / sret",
                "[2 x i64] / [2 x i64]",
                "<2 x float>, i64 / { <2 x float>, i64 }",
            ),
            (
                "{signed char a; short b}",
                natural(&[byte, Int(16)]),
                "i32 / i32",
                "i64 / i32",
                "i32 / i32",
            ),
            (
                "{signed char c}",
                natural(&[byte]),
                "i8 / i8",
                "i64 / i8",
                "i8 / i8",
            ),
        ];
        for (c, layout, windows_x64, arm64, system_v) in &rows {
            for triple in WINDOWS_X64 {
                assert_eq!(spelled(layout, triple), *windows_x64, "{c} on {triple:?}");
            }
            for triple in WINDOWS_ARM64
                .into_iter()
                .chain([Triple::Aarch64AppleDarwin])
            {
                assert_eq!(spelled(layout, triple), *arm64, "{c} on {triple:?}");
            }
            // Linux marks an HFA argument, and only that, with `alignstack(8)`.
            let linux = match arm64.split_once(" / ") {
                Some((argument, result)) if argument.ends_with("float]") => {
                    format!("{argument} alignstack(8) / {result}")
                }
                Some((argument, result)) if argument.ends_with("double]") => {
                    format!("{argument} alignstack(8) / {result}")
                }
                _ => (*arm64).to_owned(),
            };
            for triple in LINUX_ARM64 {
                assert_eq!(spelled(layout, triple), linux, "{c} on {triple:?}");
            }
            for triple in LINUX_X64 {
                assert_eq!(spelled(layout, triple), *system_v, "{c} on {triple:?}");
            }
        }
    }

    fn call(parameters: &[Position<'_>], result: Option<&CLayoutLeaves>, triple: Triple) -> String {
        let platform = platform(triple);
        let records = parameters.iter().filter_map(|position| match position {
            Position::Record(layout) => Some(*layout),
            _ => None,
        });
        classify_call(parameters, result, platform)
            .unwrap()
            .iter()
            .zip(records)
            .map(|(class, layout)| class.argument_types(layout, platform).join(", "))
            .collect::<Vec<_>>()
            .join(" | ")
    }

    /// System V counts registers over the call (the functions of the same names in the reference C).
    #[test]
    fn system_v_sends_a_record_to_memory_when_registers_run_out() {
        use Position::{Integer, Record, Sse};
        let rect = natural(&[Int(32); 4]);
        let p2d = natural(&[F64, F64]);
        let point = natural(&[Int(32), Int(32)]);
        let pointf = natural(&[F32, F32]);
        let packed5 = packed(&[Int(8), Int(32)], 1);
        let l3 = natural(&[Int(64); 3]);
        let linux = Triple::X86_64LinuxGnu;
        // bv_five_then_rect: one integer register is left for a record that needs two.
        let five = [
            Integer,
            Integer,
            Integer,
            Integer,
            Integer,
            Record(&rect),
            Integer,
        ];
        assert_eq!(call(&five, None, linux), "ptr byval([16 x i8]) align 8");
        assert_eq!(call(&five[1..], None, linux), "i64, i64");
        // bv_seven_doubles_then_p2d: one SSE register is left for a record that needs two.
        let seven = [Sse, Sse, Sse, Sse, Sse, Sse, Sse, Record(&p2d), Sse];
        assert_eq!(call(&seven, None, linux), "ptr byval([16 x i8]) align 8");
        // bv_six_then_small: with no integer register left, a small record is one integer on the
        // stack; an SSE record still has its register.
        let six = [
            Integer,
            Integer,
            Integer,
            Integer,
            Integer,
            Integer,
            Record(&point),
            Record(&packed5),
            Record(&pointf),
        ];
        assert_eq!(call(&six, None, linux), "i64 | i40 | <2 x float>");
        // bv_sse_exhausted: the second record finds no SSE register, and integer registers are free.
        let sse = [
            Sse,
            Sse,
            Sse,
            Sse,
            Sse,
            Sse,
            Sse,
            Record(&pointf),
            Record(&pointf),
            Sse,
            Record(&point),
        ];
        assert_eq!(
            call(&sse, None, linux),
            "<2 x float> | ptr byval([8 x i8]) align 8 | i64"
        );
        // bv_sret_counts: the `sret` pointer of the result uses an integer register.
        let four = [Integer, Integer, Integer, Integer, Record(&rect)];
        assert_eq!(
            call(&four, Some(&l3), linux),
            "ptr byval([16 x i8]) align 8"
        );
        assert_eq!(call(&four, Some(&point), linux), "i64, i64");
        assert_eq!(call(&four, None, linux), "i64, i64");
        // The other targets classify each position alone.
        assert_eq!(call(&five, None, Triple::Aarch64AppleDarwin), "[2 x i64]");
        assert_eq!(call(&five, None, Triple::X86_64WindowsMsvc), "ptr");
        assert_eq!(
            call(&six, None, Triple::Aarch64LinuxGnu),
            "i64 | i64 | [2 x float] alignstack(8)"
        );
    }

    #[test]
    fn temporaries_cover_the_coerced_value() {
        let rgb = natural(&[Int(8); 3]);
        let nine = natural(&[Int(8); 9]);
        let darwin = platform(Triple::Aarch64AppleDarwin);
        assert_eq!(classify(&rgb, darwin, false).unwrap().storage(&rgb), 8);
        assert_eq!(classify(&rgb, darwin, true).unwrap().storage(&rgb), 3);
        assert_eq!(classify(&nine, darwin, false).unwrap().storage(&nine), 16);
        let linux = platform(Triple::X86_64LinuxGnu);
        assert_eq!(classify(&nine, linux, false).unwrap().storage(&nine), 16);
        let l3 = natural(&[Int(64); 3]);
        assert_eq!(ByValue::Memory.storage(&l3), 24);
    }

    #[test]
    fn records_aligned_past_eight_bytes_are_refused() {
        let wide = CLayoutLeaves {
            size: 16,
            align: 16,
            leaves: vec![(0, Int(64)), (8, Int(64))],
        };
        for triple in [
            Triple::X86_64WindowsMsvc,
            Triple::Aarch64AppleDarwin,
            Triple::X86_64LinuxGnu,
        ] {
            assert_eq!(
                classify(&wide, platform(triple), false),
                Err(Unsupported::Target)
            );
            assert_eq!(
                classify_call(&[Position::Record(&wide)], None, platform(triple)),
                Err(Unsupported::Target)
            );
        }
    }
}
