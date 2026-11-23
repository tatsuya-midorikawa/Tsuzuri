use std::collections::BTreeSet;

use crate::check::{Type, TypeContext};
use crate::diagnostic::{Diagnostic, DiagnosticKind};
use crate::syntax::LinkName;

/// C names that generated code declares or defines itself, in ascending order:
/// the C library functions it calls, `main`, and the helpers and tables of the
/// embedded numeric and math runtime. An explicit extern symbol cannot reuse
/// them, because LLVM rejects a second declaration whose type differs and a
/// redefinition of an internal function. `reserved_symbols_cover_the_runtime_names`
/// keeps the runtime part in step with `src/runtime/*.ll`.
pub const RESERVED_HOST_SYMBOLS: &[&str] = &[
    "PIo2",
    "_setmode",
    "_write",
    "atan_centers",
    "atan_series",
    "atanhi",
    "atanlo",
    "decimal_digits",
    "decimal_scaled",
    "decode",
    "divide",
    "divide_u128_small",
    "eight_decimal_digits",
    "format_binary_fast",
    "format_exponent",
    "format_fixed",
    "format_float",
    "free",
    "init_jk",
    "ipio2",
    "layout_decimal",
    "magnitude",
    "main",
    "malloc",
    "multiply_small",
    "pack",
    "parse_binary_fast",
    "parse_float",
    "power",
    "putchar",
    "realloc",
    "specialcase",
    "store",
    "subtract",
    "tzrt_pow5",
    "tzrt_schubfach_g",
    "write",
];

/// Prefixes an explicit extern symbol cannot start with: `tz_` names exports, `tsuzuri`
/// the runtime and the default host imports, and `__` the C implementation.
pub const RESERVED_SYMBOL_PREFIXES: &[&str] = &["tz_", "tsuzuri", "__"];

/// The error for an extern link name that cannot become a host import, if any.
pub fn link_name_error(link: &LinkName) -> Option<Diagnostic> {
    let symbol = &link.symbol;
    let mut bytes = symbol.bytes();
    let identifier = bytes
        .next()
        .is_some_and(|byte| byte == b'_' || byte.is_ascii_alphabetic())
        && bytes.all(|byte| byte == b'_' || byte.is_ascii_alphanumeric());
    if !identifier || symbol.len() > 255 {
        return Some(Diagnostic::of(
            DiagnosticKind::InvalidExternSymbol,
            format!("invalid extern symbol '{symbol}'; use a C identifier of at most 255 bytes"),
            link.span,
        ));
    }
    if RESERVED_SYMBOL_PREFIXES
        .iter()
        .any(|prefix| symbol.starts_with(prefix))
    {
        return Some(Diagnostic::of(
            DiagnosticKind::ReservedExternSymbol,
            format!(
                "extern symbol '{symbol}' is reserved by Tsuzuri; choose a name that does not start with 'tz_', 'tsuzuri' or '__'"
            ),
            link.span,
        ));
    }
    if RESERVED_HOST_SYMBOLS.contains(&symbol.as_str()) {
        return Some(Diagnostic::of(
            DiagnosticKind::ReservedExternSymbol,
            format!(
                "extern symbol '{symbol}' is reserved by Tsuzuri because the generated code declares it; wrap the host function under another symbol"
            ),
            link.span,
        ));
    }
    let (module, span) = link.module.as_ref()?;
    let valid = !module.is_empty()
        && module.len() <= 255
        && module
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
        && !module.starts_with("tsuzuri_");
    (!valid).then(|| {
        Diagnostic::of(DiagnosticKind::InvalidWasmImportModule,
            format!(
                "invalid WASM import module '{module}'; use 1 to 255 ASCII letters, digits, '_', '-' or '.' not starting with 'tsuzuri_'"
            ),
            *span,
        )
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Buffer {
    I64,
    F64,
    UByte,
    String,
    Utf8String,
}

impl Buffer {
    pub fn of(ty: &Type) -> Option<Self> {
        match ty {
            Type::Array(element) => match element.as_ref() {
                Type::Integer(64, true) => Some(Self::I64),
                Type::Binary(64) => Some(Self::F64),
                Type::Integer(8, false) => Some(Self::UByte),
                _ => None,
            },
            Type::String => Some(Self::String),
            Type::Utf8String => Some(Self::Utf8String),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::I64 => "i64",
            Self::F64 => "f64",
            Self::UByte => "ubyte",
            Self::String => "string",
            Self::Utf8String => "utf8string",
        }
    }

    pub fn c_element(self) -> &'static str {
        match self {
            Self::I64 => "int64_t",
            Self::F64 => "double",
            Self::UByte | Self::Utf8String => "uint8_t",
            Self::String => "uint16_t",
        }
    }

    pub fn width(self) -> usize {
        match self {
            Self::I64 | Self::F64 => 8,
            Self::String => 2,
            Self::UByte | Self::Utf8String => 1,
        }
    }
}

pub fn scalar_record(ty: &Type, types: &TypeContext<'_>) -> bool {
    fn valid(ty: &Type, types: &TypeContext<'_>, depth: usize) -> bool {
        if ty.exportable() {
            return true;
        }
        // A16 Phase 2: a C array field `T name[N]` of scalars. ISO C has no zero-length
        // arrays, so `[T; 0]` stays inside Tsuzuri.
        if let Type::FixedArray(element, _) = ty {
            return element.exportable() && ty.fixed_length().is_some_and(|length| length > 0);
        }
        let Type::Record(id, arguments) = ty else {
            return false;
        };
        if depth >= crate::syntax::MAX_NESTING {
            return false;
        }
        // A Drop record carries a hidden live flag and owns a resource, not plain data.
        if types.records[*id].user_drop {
            return false;
        }
        let names: BTreeSet<_> = types.records[*id]
            .fields
            .iter()
            .map(|(name, _)| field_name(name))
            .collect();
        names.len() == types.records[*id].fields.len()
            && types
                .record_fields(*id, arguments)
                .iter()
                .all(|field| valid(field, types, depth + 1))
    }
    matches!(ty, Type::Record(..)) && valid(ty, types, 0)
}

pub fn parameter(ty: &Type, types: &TypeContext<'_>) -> bool {
    abi_scalar(ty)
        || scalar_record(ty, types)
        || matches!(ty, Type::Reference(inner, false) if Buffer::of(inner).is_some() || scalar_record(inner, types))
}

pub fn result(ty: &Type, types: &TypeContext<'_>) -> bool {
    ty.exportable()
        || is_handle(ty)
        || *ty == Type::Unit
        || Buffer::of(ty).is_some()
        || scalar_record(ty, types)
}

/// Whether `ty` is an `extern type` handle.
pub fn is_handle(ty: &Type) -> bool {
    matches!(ty, Type::Handle(..))
}

/// A parameter the C ABI passes as one machine scalar: a number, a bool, an
/// extern handle, or a shared reference to one (which passes the handle itself).
/// Unlike `Type::exportable`, this is not a valid record field.
pub fn abi_scalar(ty: &Type) -> bool {
    ty.exportable()
        || is_handle(ty)
        || matches!(ty, Type::Reference(inner, false) if is_handle(inner))
}

/// Returns the payload `P` when `ty` is `std::Maybe<P>`.
pub fn std_maybe_payload<'a>(ty: &'a Type, types: &TypeContext<'_>) -> Option<&'a Type> {
    let Type::Union(id, arguments) = ty else {
        return None;
    };
    let union = &types.unions[*id];
    (union.origin == crate::check::ModuleOrigin::Std && union.name == "Maybe.Maybe")
        .then(|| &arguments[0])
}

/// W15: a `std::Maybe<P>` whose `None` passes as a C null pointer on native targets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Nullable<'a> {
    Handle(&'a Type),
    BorrowedHandle(&'a Type),
    BorrowedRecord(&'a Type),
}

pub fn nullable<'a>(ty: &'a Type, types: &TypeContext<'_>) -> Option<Nullable<'a>> {
    let payload = std_maybe_payload(ty, types)?;
    match payload {
        Type::Handle(..) => Some(Nullable::Handle(payload)),
        Type::Reference(inner, false) if is_handle(inner) => Some(Nullable::BorrowedHandle(inner)),
        Type::Reference(inner, false) if scalar_record(inner, types) => {
            Some(Nullable::BorrowedRecord(inner))
        }
        _ => None,
    }
}

/// The type as the C side sees it: `Maybe<H>` -> `H`, `Maybe<ref H>` -> `ref H`,
/// `Maybe<ref R>` -> `ref R`, and any other type unchanged (W15).
pub fn c_view<'a>(ty: &'a Type, types: &TypeContext<'_>) -> std::borrow::Cow<'a, Type> {
    match nullable(ty, types) {
        Some(Nullable::Handle(handle)) => std::borrow::Cow::Borrowed(handle),
        Some(Nullable::BorrowedHandle(handle)) => {
            std::borrow::Cow::Owned(Type::Reference(Box::new(handle.clone()), false))
        }
        Some(Nullable::BorrowedRecord(record)) => {
            std::borrow::Cow::Owned(Type::Reference(Box::new(record.clone()), false))
        }
        None => std::borrow::Cow::Borrowed(ty),
    }
}

pub fn extern_scalar(ty: &Type, types: &TypeContext<'_>) -> bool {
    abi_scalar(ty)
        || matches!(
            nullable(ty, types),
            Some(Nullable::Handle(_) | Nullable::BorrowedHandle(_))
        )
}

pub fn extern_parameter(ty: &Type, types: &TypeContext<'_>) -> bool {
    parameter(ty, types)
        || extern_scalar(ty, types)
        || matches!(nullable(ty, types), Some(Nullable::BorrowedRecord(_)))
}

pub fn extern_result(ty: &Type, types: &TypeContext<'_>) -> bool {
    result(ty, types) || matches!(nullable(ty, types), Some(Nullable::Handle(_)))
}

/// Whether an `extern` signature uses `std::Maybe` in an unsupported ABI position or payload (W15).
pub fn has_invalid_extern_maybe(
    parameters: &[Type],
    result: &Type,
    types: &TypeContext<'_>,
) -> bool {
    let ref_to_maybe = |ty: &Type| matches!(ty, Type::Reference(inner, _) if std_maybe_payload(inner, types).is_some());
    let bad_param = parameters.iter().any(|param| match param {
        Type::Function(cb_params, cb_result) => {
            cb_params.iter().any(|p| {
                (std_maybe_payload(p, types).is_some() && !extern_scalar(p, types))
                    || ref_to_maybe(p)
            }) || std_maybe_payload(cb_result, types).is_some()
                || ref_to_maybe(cb_result)
        }
        _ => {
            (std_maybe_payload(param, types).is_some() && !extern_parameter(param, types))
                || ref_to_maybe(param)
        }
    });
    bad_param
        || (std_maybe_payload(result, types).is_some() && !extern_result(result, types))
        || ref_to_maybe(result)
}

/// Whether an `extern` signature uses any native-only `Maybe` ABI form (W15).
pub fn uses_nullable_abi(parameters: &[Type], result: &Type, types: &TypeContext<'_>) -> bool {
    parameters.iter().any(|param| match param {
        Type::Function(cb_params, _) => cb_params.iter().any(|p| nullable(p, types).is_some()),
        _ => nullable(param, types).is_some(),
    }) || nullable(result, types).is_some()
}

/// A function type the host can call as a C function pointer: its parameters
/// are scalars or handles (a lone `unit` is left out of the C signature) and
/// its result a scalar or `unit`. Buffers, records and nested functions are not.
pub fn callback(ty: &Type, types: &TypeContext<'_>) -> bool {
    let Type::Function(parameters, result) = ty else {
        return false;
    };
    (matches!(parameters.as_slice(), [Type::Unit])
        || parameters.iter().all(|param| extern_scalar(param, types)))
        && (result.exportable() || **result == Type::Unit)
}

pub fn out_result(ty: &Type) -> bool {
    Buffer::of(ty).is_some() || matches!(ty, Type::Record(..))
}

/// The LLVM / C scalar or pointer type of one lowered host parameter or result (W14).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum CType {
    I32,
    I64,
    F32,
    F64,
    Ptr,
    Void,
}

/// The native 64-bit C ABI register/stack class of a `CType` (W14).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum AbiClass {
    I32,
    Word64,
    F32,
    F64,
    Void,
}

impl CType {
    pub fn class(self) -> AbiClass {
        match self {
            Self::I32 => AbiClass::I32,
            Self::I64 | Self::Ptr => AbiClass::Word64,
            Self::F32 => AbiClass::F32,
            Self::F64 => AbiClass::F64,
            Self::Void => AbiClass::Void,
        }
    }

    pub fn llvm(self) -> &'static str {
        match self {
            Self::I32 => "i32",
            Self::I64 => "i64",
            Self::F32 => "float",
            Self::F64 => "double",
            Self::Ptr => "ptr",
            Self::Void => "void",
        }
    }

    fn canonical(self, other: Self) -> Self {
        debug_assert_eq!(self.class(), other.class());
        if self == other { self } else { Self::I64 }
    }
}

impl AbiClass {
    pub fn description(self) -> &'static str {
        match self {
            Self::I32 => "a 32-bit integer",
            Self::Word64 => "a 64-bit integer or pointer",
            Self::F32 => "a 32-bit float",
            Self::F64 => "a 64-bit float",
            Self::Void => "void",
        }
    }
}

/// The lowered C parameter and result types of an `extern` declaration (W14).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CSignature {
    pub parameters: Vec<CType>,
    pub result: CType,
}

impl CSignature {
    /// Combines another C-ABI-compatible signature into the canonical `declare` signature.
    /// Returns `true` when any position differed in `CType` (`ptr` vs `i64`).
    pub fn merge_canonical(&mut self, other: &Self) -> bool {
        let mut mixed = false;
        for (slot, &next) in self.parameters.iter_mut().zip(&other.parameters) {
            if *slot != next {
                mixed = true;
                *slot = slot.canonical(next);
            }
        }
        if self.result != other.result {
            mixed = true;
            self.result = self.result.canonical(other.result);
        }
        mixed
    }

    /// Returns a human-readable mismatch reason against `first`, or `None` when C ABI classes match.
    pub fn mismatch(&self, first: &Self) -> Option<String> {
        if self.parameters.len() != first.parameters.len() {
            let here = self.parameters.len();
            let noun = if here == 1 { "parameter" } else { "parameters" };
            return Some(format!(
                "takes {here} C {noun} here but {} in the first declaration",
                first.parameters.len()
            ));
        }
        for (index, (here, expected)) in self.parameters.iter().zip(&first.parameters).enumerate() {
            if here.class() != expected.class() {
                return Some(format!(
                    "parameter {} is {}, but {} in the first declaration",
                    index + 1,
                    here.class().description(),
                    expected.class().description()
                ));
            }
        }
        if self.result.class() != first.result.class() {
            return Some(format!(
                "the result is {}, but {} in the first declaration",
                self.result.class().description(),
                first.result.class().description()
            ));
        }
        None
    }
}

fn c_scalar(ty: &Type) -> CType {
    match ty {
        Type::Bool | Type::Integer(8 | 16 | 32, _) => CType::I32,
        Type::Integer(64, _) => CType::I64,
        Type::Binary(32) => CType::F32,
        Type::Binary(64) => CType::F64,
        Type::Handle(..) => CType::Ptr,
        Type::Reference(inner, false) if is_handle(inner) => CType::Ptr,
        _ => unreachable!("checked ABI scalar: {ty:?}"),
    }
}

pub fn c_signature(parameters: &[Type], result: &Type, types: &TypeContext<'_>) -> CSignature {
    let mut lowered = Vec::new();
    let viewed_result = c_view(result, types);
    if out_result(&viewed_result) {
        lowered.push(CType::Ptr);
    }
    for parameter in parameters {
        if *parameter == Type::Unit {
            continue;
        }
        if matches!(parameter, Type::Function(..)) {
            lowered.push(CType::Ptr);
            continue;
        }
        let viewed = c_view(parameter, types);
        let inner = match viewed.as_ref() {
            Type::Reference(inner, false) => inner.as_ref(),
            other => other,
        };
        if Buffer::of(inner).is_some() {
            lowered.extend([CType::Ptr, CType::I64]);
        } else if matches!(inner, Type::Record(..) | Type::Handle(..)) {
            lowered.push(CType::Ptr);
        } else {
            lowered.push(c_scalar(&viewed));
        }
    }
    let lowered_result = if *viewed_result == Type::Unit || out_result(&viewed_result) {
        CType::Void
    } else {
        c_scalar(&viewed_result)
    };
    CSignature {
        parameters: lowered,
        result: lowered_result,
    }
}

pub fn field_name(name: &str) -> String {
    const KEYWORDS: &[&str] = &[
        "alignas",
        "alignof",
        "and",
        "and_eq",
        "asm",
        "auto",
        "bitand",
        "bitor",
        "bool",
        "break",
        "case",
        "catch",
        "char",
        "char8_t",
        "char16_t",
        "char32_t",
        "class",
        "compl",
        "concept",
        "const",
        "consteval",
        "constexpr",
        "constinit",
        "const_cast",
        "continue",
        "co_await",
        "co_return",
        "co_yield",
        "decltype",
        "default",
        "delete",
        "do",
        "double",
        "dynamic_cast",
        "else",
        "enum",
        "explicit",
        "export",
        "extern",
        "false",
        "float",
        "for",
        "friend",
        "goto",
        "if",
        "inline",
        "int",
        "long",
        "mutable",
        "namespace",
        "new",
        "noexcept",
        "not",
        "not_eq",
        "nullptr",
        "operator",
        "or",
        "or_eq",
        "private",
        "protected",
        "public",
        "register",
        "reinterpret_cast",
        "requires",
        "restrict",
        "return",
        "short",
        "signed",
        "sizeof",
        "static",
        "static_assert",
        "static_cast",
        "struct",
        "switch",
        "template",
        "this",
        "thread_local",
        "throw",
        "true",
        "try",
        "typedef",
        "typeid",
        "typename",
        "union",
        "unsigned",
        "using",
        "virtual",
        "void",
        "volatile",
        "wchar_t",
        "while",
        "xor",
        "xor_eq",
        "_Alignas",
        "_Alignof",
        "_Atomic",
        "_BitInt",
        "_Bool",
        "_Complex",
        "_Decimal32",
        "_Decimal64",
        "_Decimal128",
        "_Generic",
        "_Imaginary",
        "_Noreturn",
        "_Static_assert",
        "_Thread_local",
        "typeof",
        "typeof_unqual",
    ];
    if KEYWORDS.contains(&name) || name.starts_with('_') {
        format!("tz_{name}")
    } else {
        name.into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reserved_symbols_cover_the_runtime_names() {
        assert!(
            RESERVED_HOST_SYMBOLS
                .windows(2)
                .all(|pair| pair[0] < pair[1])
        );
        let runtime = [
            include_str!("runtime/channel-wasm.ll"),
            include_str!("runtime/character.ll"),
            include_str!("runtime/closure.ll"),
            include_str!("runtime/console.ll"),
            include_str!("runtime/debug.ll"),
            include_str!("runtime/display.ll"),
            include_str!("runtime/heap-native.ll"),
            include_str!("runtime/heap-wasm-threads.ll"),
            include_str!("runtime/heap-wasm.ll"),
            include_str!("runtime/heap-wasm64.ll"),
            include_str!("runtime/i128-windows-x64.ll"),
            include_str!("runtime/integer.ll"),
            include_str!("runtime/math.ll"),
            include_str!("runtime/numeric.ll"),
            include_str!("runtime/recursive.ll"),
            include_str!("runtime/string.ll"),
            include_str!("runtime/sync-wasm.ll"),
            include_str!("runtime/sync.ll"),
            include_str!("runtime/task-wasm.ll"),
            include_str!("runtime/unicode.ll"),
            include_str!("runtime/utf8string.ll"),
            include_str!("runtime/wasm.ll"),
        ];
        let mut unreserved = BTreeSet::new();
        for line in runtime.iter().flat_map(|text| text.lines()) {
            // `define ... @function(`, `declare ... @function(` and `@global = ...`.
            let rest = if line.starts_with("define ") || line.starts_with("declare ") {
                line.split_once('@').map(|(_, rest)| rest)
            } else {
                line.strip_prefix('@')
            };
            let Some(rest) = rest else { continue };
            let end = rest
                .find(|character: char| !(character.is_ascii_alphanumeric() || character == '_'))
                .unwrap_or(rest.len());
            let (name, after) = rest.split_at(end);
            let defined = after.starts_with('(') || after.starts_with(" =");
            let own = RESERVED_SYMBOL_PREFIXES
                .iter()
                .any(|prefix| name.starts_with(prefix));
            if defined && !own && !RESERVED_HOST_SYMBOLS.contains(&name) {
                unreserved.insert(name.to_owned());
            }
        }
        assert!(
            unreserved.is_empty(),
            "add these runtime names to RESERVED_HOST_SYMBOLS: {unreserved:?}"
        );
    }
}
