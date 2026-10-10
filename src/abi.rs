use std::collections::BTreeSet;

use crate::check::{Type, TypeContext};
use crate::diagnostic::Diagnostic;
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
    "eight_decimal_digits",
    "format_exponent",
    "format_fixed",
    "format_float",
    "free",
    "init_jk",
    "ipio2",
    "magnitude",
    "main",
    "malloc",
    "multiply_small",
    "pack",
    "parse_float",
    "power",
    "putchar",
    "realloc",
    "specialcase",
    "store",
    "subtract",
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
        return Some(Diagnostic::new(
            "E1008",
            format!("invalid extern symbol '{symbol}'; use a C identifier of at most 255 bytes"),
            link.span,
        ));
    }
    if RESERVED_SYMBOL_PREFIXES
        .iter()
        .any(|prefix| symbol.starts_with(prefix))
    {
        return Some(Diagnostic::new(
            "E1008",
            format!(
                "extern symbol '{symbol}' is reserved by Tsuzuri; choose a name that does not start with 'tz_', 'tsuzuri' or '__'"
            ),
            link.span,
        ));
    }
    if RESERVED_HOST_SYMBOLS.contains(&symbol.as_str()) {
        return Some(Diagnostic::new(
            "E1008",
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
        Diagnostic::new(
            "E1008",
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
    matches!(ty, Type::Handle(_))
}

/// A parameter the C ABI passes as one machine scalar: a number, a bool, an
/// extern handle, or a shared reference to one (which passes the handle itself).
/// Unlike `Type::exportable`, this is not a valid record field.
pub fn abi_scalar(ty: &Type) -> bool {
    ty.exportable()
        || is_handle(ty)
        || matches!(ty, Type::Reference(inner, false) if is_handle(inner))
}

/// A function type the host can call as a C function pointer: its parameters
/// are scalars or handles (a lone `unit` is left out of the C signature) and
/// its result a scalar or `unit`. Buffers, records and nested functions are not.
pub fn callback(ty: &Type) -> bool {
    let Type::Function(parameters, result) = ty else {
        return false;
    };
    (matches!(parameters.as_slice(), [Type::Unit]) || parameters.iter().all(abi_scalar))
        && (result.exportable() || **result == Type::Unit)
}

pub fn out_result(ty: &Type) -> bool {
    Buffer::of(ty).is_some() || matches!(ty, Type::Record(..))
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
            let own = ["tz", "tsuzuri", "__"]
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
