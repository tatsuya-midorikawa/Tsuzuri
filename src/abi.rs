use std::collections::BTreeSet;

use crate::check::{Type, TypeContext};

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
        let Type::Record(id, arguments) = ty else {
            return false;
        };
        if depth >= crate::syntax::MAX_NESTING {
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
    ty.exportable()
        || scalar_record(ty, types)
        || matches!(ty, Type::Reference(inner, false) if Buffer::of(inner).is_some() || scalar_record(inner, types))
}

pub fn result(ty: &Type, types: &TypeContext<'_>) -> bool {
    ty.exportable() || *ty == Type::Unit || Buffer::of(ty).is_some() || scalar_record(ty, types)
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
