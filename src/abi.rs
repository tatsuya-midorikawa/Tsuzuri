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
    I16U,
    I32,
    I32U,
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

    fn of_element(element: &Type) -> Option<Self> {
        match element {
            Type::Integer(64, true) => Some(Self::I64),
            Type::Binary(64) => Some(Self::F64),
            Type::Integer(8, false) => Some(Self::UByte),
            Type::Integer(16, false) => Some(Self::I16U),
            Type::Integer(32, true) => Some(Self::I32),
            Type::Integer(32, false) => Some(Self::I32U),
            _ => None,
        }
    }

    /// W13: shared buffer targets accepted on native `extern` parameters (the 5 portable buffers plus
    /// `[i16u]`, `[i32]`, and `[i32u]`).
    pub fn of_extern(ty: &Type) -> Option<Self> {
        match ty {
            Type::Array(element) => Self::of_element(element),
            Type::String => Some(Self::String),
            Type::Utf8String => Some(Self::Utf8String),
            _ => None,
        }
    }

    /// W13: mutable buffer targets (`[T]` and `[T..]`) accepted on native `extern` `ref mut` parameters.
    pub fn of_extern_mutable(ty: &Type) -> Option<Self> {
        match ty {
            Type::Array(element) | Type::ArrayView(element) => Self::of_element(element),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::I64 => "i64",
            Self::F64 => "f64",
            Self::UByte => "ubyte",
            Self::I16U => "i16u",
            Self::I32 => "i32",
            Self::I32U => "i32u",
            Self::String => "string",
            Self::Utf8String => "utf8string",
        }
    }

    pub fn c_element(self) -> &'static str {
        match self {
            Self::I64 => "int64_t",
            Self::F64 => "double",
            Self::UByte | Self::Utf8String => "uint8_t",
            Self::I16U | Self::String => "uint16_t",
            Self::I32 => "int32_t",
            Self::I32U => "uint32_t",
        }
    }

    pub fn width(self) -> usize {
        match self {
            Self::I64 | Self::F64 => 8,
            Self::I32 | Self::I32U => 4,
            Self::I16U | Self::String => 2,
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
        // A `@c_layout` record uses exact C layout and cannot sit inside an unmarked normalized record (W21).
        if types.records[*id].user_drop || types.records[*id].c_layout.is_some() {
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

/// W21: exact-width scalar number types allowed as `@c_layout` fields (not `bool`).
pub fn c_layout_number(ty: &Type) -> bool {
    matches!(
        ty,
        Type::Integer(8 | 16 | 32 | 64, _) | Type::Binary(32 | 64)
    )
}

/// W21: whether `ty` is a validated `@c_layout` record.
pub fn c_layout_record(ty: &Type, types: &TypeContext<'_>) -> bool {
    let Type::Record(id, arguments) = ty else {
        return false;
    };
    let record = &types.records[*id];
    arguments.is_empty()
        && record.c_layout.is_some()
        && !record.user_drop
        && !record.fields.is_empty()
}

/// W21: whether `ty` is either a normalized scalar record or a `@c_layout` record.
pub fn host_record(ty: &Type, types: &TypeContext<'_>) -> bool {
    scalar_record(ty, types) || c_layout_record(ty, types)
}

/// W21: whether a `@c_layout` record contains any pointer-width field (`Handle`, `Maybe<H>`,
/// `FnPtr`, `ref ZString`, `ref ZUtf8String`, or `Maybe` of one) that is native-only on WASM.
pub fn c_layout_has_pointer_field(ty: &Type, types: &TypeContext<'_>) -> bool {
    fn visit(ty: &Type, types: &TypeContext<'_>, depth: usize) -> bool {
        if depth >= crate::syntax::MAX_NESTING {
            return false;
        }
        let Type::Record(id, arguments) = ty else {
            return false;
        };
        if types.records[*id].c_layout.is_none() {
            return false;
        }
        types
            .record_fields(*id, arguments)
            .iter()
            .any(|field| match field {
                Type::Handle(..) | Type::FnPtr(_) => true,
                Type::Reference(inner, false) if any_zstring(inner, types) => true,
                Type::Record(..) => visit(field, types, depth + 1),
                _ => matches!(
                    nullable(field, types),
                    Some(
                        Nullable::Handle(_)
                            | Nullable::BorrowedZString(_)
                            | Nullable::BorrowedZUtf8String(_)
                    )
                ),
            })
    }
    visit(ty, types, 0)
}

/// W21 Phase 2 / M06 Phase 2: whether a `@c_layout` record contains a borrowed `ZString` or
/// `ZUtf8String` field (directly or inside `Maybe`).
pub fn c_layout_has_zstring_field(ty: &Type, types: &TypeContext<'_>) -> bool {
    fn visit(ty: &Type, types: &TypeContext<'_>, depth: usize) -> bool {
        if depth >= crate::syntax::MAX_NESTING {
            return false;
        }
        let Type::Record(id, arguments) = ty else {
            return false;
        };
        if types.records[*id].c_layout.is_none() {
            return false;
        }
        types
            .record_fields(*id, arguments)
            .iter()
            .any(|field| match field {
                Type::Reference(inner, false) if any_zstring(inner, types) => true,
                Type::Record(..) => visit(field, types, depth + 1),
                _ => matches!(
                    nullable(field, types),
                    Some(Nullable::BorrowedZString(_) | Nullable::BorrowedZUtf8String(_))
                ),
            })
    }
    visit(ty, types, 0)
}

/// W21 Phase 2: whether a `@c_layout` record holds any input-only field (`FnPtr` or borrowed `ZString`).
pub fn c_layout_input_only(ty: &Type, types: &TypeContext<'_>) -> bool {
    c_layout_has_zstring_field(ty, types) || reaches_fnptr(ty, types)
}

/// W21: whether a record can appear in bidirectional ABI positions (export, extern result, extern `ref mut`).
pub fn host_record_bidirectional(ty: &Type, types: &TypeContext<'_>) -> bool {
    scalar_record(ty, types) || (c_layout_record(ty, types) && !c_layout_input_only(ty, types))
}

/// W21: if `ty` (or the inner record of `ref R` / `ref mut R` / `Maybe<ref R>`) is an unmarked record
/// containing a `@c_layout` record field, returns `(outer_name, field_name, inner_name)`.
pub fn plain_record_contains_c_layout(
    ty: &Type,
    types: &TypeContext<'_>,
) -> Option<(String, String, String)> {
    fn find(ty: &Type, types: &TypeContext<'_>, depth: usize) -> Option<(String, String, String)> {
        if depth >= crate::syntax::MAX_NESTING {
            return None;
        }
        let Type::Record(id, arguments) = ty else {
            return None;
        };
        let record = &types.records[*id];
        if record.c_layout.is_some() {
            return None;
        }
        for ((fname, _), fty) in record
            .fields
            .iter()
            .zip(types.record_fields(*id, arguments))
        {
            if let Type::Record(inner_id, _) = &fty {
                if types.records[*inner_id].c_layout.is_some() {
                    let outer = record
                        .name
                        .rsplit('.')
                        .next()
                        .unwrap_or(&record.name)
                        .to_owned();
                    let inner = types.records[*inner_id]
                        .name
                        .rsplit('.')
                        .next()
                        .unwrap_or(&types.records[*inner_id].name)
                        .to_owned();
                    return Some((outer, fname.clone(), inner));
                }
                if let Some(found) = find(&fty, types, depth + 1) {
                    return Some(found);
                }
            }
        }
        None
    }
    let candidate = match ty {
        Type::Reference(inner, _) => inner.as_ref(),
        _ => match std_maybe_payload(ty, types) {
            Some(Type::Reference(inner, _)) => inner.as_ref(),
            _ => ty,
        },
    };
    find(candidate, types, 0)
}

/// W24 (Phase 1 & Phase 2): validates that `ty` is a record allowed as the target of `extern view`.
/// Phase 1 accepts non-empty `scalar_record`s whose leaves are 32/64-bit numbers (`i32`, `i32u`,
/// `i64`, `i64u`, `f32`, `f64`), and Phase 2 additionally accepts bidirectional `@c_layout` records
/// (exact-width numbers, fixed arrays, nested `@c_layout` records, `copy` handles, `Maybe<Handle>`,
/// without `FnPtr` or borrowed `ZString` fields).
pub fn view_record(ty: &Type, types: &TypeContext<'_>, view_name: &str) -> Result<(), String> {
    fn check_plain_leaves(
        ty: &Type,
        types: &TypeContext<'_>,
        record_name: &str,
        view_name: &str,
        depth: usize,
    ) -> Result<(), String> {
        if depth >= crate::syntax::MAX_NESTING {
            return Err(format!(
                "extern view '{view_name}' copies the record '{record_name}', which exceeds the maximum record nesting"
            ));
        }
        let Type::Record(id, arguments) = ty else {
            return Err(format!("extern view '{view_name}' must copy a record type"));
        };
        let rec = &types.records[*id];
        if rec.fields.is_empty() {
            return Err(format!(
                "extern view '{view_name}' copies the empty record '{record_name}'; a view record must have at least one field"
            ));
        }
        for ((fname, _), fty) in rec.fields.iter().zip(types.record_fields(*id, arguments)) {
            let leaf = match &fty {
                Type::FixedArray(element, _) => element.as_ref(),
                other => other,
            };
            match leaf {
                Type::Integer(32 | 64, _) | Type::Binary(32 | 64) => {}
                Type::Record(..) => {
                    check_plain_leaves(leaf, types, record_name, view_name, depth + 1)?;
                }
                _ => {
                    let shown = leaf.display(types);
                    return Err(format!(
                        "extern view '{view_name}' copies the record '{record_name}', whose field '{fname}' is {shown}; the record ABI widens it, so its layout differs from C; use 32-bit or 64-bit fields or declare '{record_name}' with '@c_layout'"
                    ));
                }
            }
        }
        Ok(())
    }

    let Type::Record(id, _) = ty else {
        return Err(format!("extern view '{view_name}' must copy a record type"));
    };
    if any_zstring(ty, types) {
        let short = types.records[*id]
            .name
            .rsplit('.')
            .next()
            .unwrap_or(&types.records[*id].name);
        return Err(format!(
            "extern view '{view_name}' copies the record '{short}', which is an opaque NUL-terminated string rather than a C struct"
        ));
    }
    let rec = &types.records[*id];
    let short = rec.name.rsplit('.').next().unwrap_or(&rec.name);
    if rec.c_layout.is_some() {
        if c_layout_input_only(ty, types) {
            return Err(format!(
                "extern view '{view_name}' copies '@c_layout' record '{short}', which holds a FnPtr or borrowed ZString field; extern views only copy data and handle fields"
            ));
        }
        if c_layout_record(ty, types) {
            return Ok(());
        }
        return Err(format!(
            "extern view '{view_name}' copies invalid '@c_layout' record '{short}'"
        ));
    }
    if !scalar_record(ty, types) {
        if let Some((_, fname, inner)) = plain_record_contains_c_layout(ty, types) {
            return Err(format!(
                "extern view '{view_name}' copies the record '{short}', whose field '{fname}' is the '@c_layout' record '{inner}'; declare '{short}' with '@c_layout'"
            ));
        }
        return Err(format!(
            "extern view '{view_name}' copies the record '{short}', which is not a C-compatible record; use 32-bit or 64-bit scalar fields or declare '{short}' with '@c_layout'"
        ));
    }
    check_plain_leaves(ty, types, short, view_name, 0)
}

pub fn parameter(ty: &Type, types: &TypeContext<'_>) -> bool {
    abi_scalar(ty)
        || host_record_bidirectional(ty, types)
        || matches!(ty, Type::Reference(inner, false) if Buffer::of(inner).is_some() || host_record_bidirectional(inner, types))
}

pub fn result(ty: &Type, types: &TypeContext<'_>) -> bool {
    ty.exportable()
        || is_handle(ty)
        || *ty == Type::Unit
        || Buffer::of(ty).is_some()
        || host_record_bidirectional(ty, types)
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

/// W20: whether `ty` is the standard `ZString` opaque record.
pub fn zstring(ty: &Type, types: &TypeContext<'_>) -> bool {
    matches!(
        ty,
        Type::Record(id, args)
            if args.is_empty()
                && types.records[*id].origin == crate::check::ModuleOrigin::Std
                && types.records[*id].name == "ZString.ZString"
    )
}

/// M06 Phase 2: whether `ty` is the standard `ZUtf8String` opaque record.
pub fn zutf8string(ty: &Type, types: &TypeContext<'_>) -> bool {
    matches!(
        ty,
        Type::Record(id, args)
            if args.is_empty()
                && types.records[*id].origin == crate::check::ModuleOrigin::Std
                && types.records[*id].name == "ZUtf8String.ZUtf8String"
    )
}

/// Whether `ty` is either `ZString` or `ZUtf8String`.
pub fn any_zstring(ty: &Type, types: &TypeContext<'_>) -> bool {
    zstring(ty, types) || zutf8string(ty, types)
}

/// M06: whether `ty` is allowed as the read result `T` of `extern "sym" view def f :: unit -> T`.
pub fn data_symbol_read_type(ty: &Type, types: &TypeContext<'_>, name: &str) -> bool {
    match ty {
        Type::Integer(8 | 16 | 32 | 64, _) | Type::Binary(32 | 64) => true,
        Type::Handle(_, crate::syntax::HandleKind::Copy) => true,
        Type::Record(..) => view_record(ty, types, name).is_ok(),
        _ => matches!(
            nullable(ty, types),
            Some(Nullable::Handle(Type::Handle(
                _,
                crate::syntax::HandleKind::Copy
            )))
        ),
    }
}

/// M06: whether `ty` is allowed as the result `H` of `extern "sym" address def f :: unit -> H`.
pub fn data_symbol_address_type(ty: &Type) -> bool {
    matches!(ty, Type::Handle(_, crate::syntax::HandleKind::Copy))
}

/// W15 / W20 / M06: a `std::Maybe<P>` whose `None` passes or returns as a C null pointer on native targets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Nullable<'a> {
    Handle(&'a Type),
    BorrowedHandle(&'a Type),
    BorrowedRecord(&'a Type),
    BorrowedZString(&'a Type),
    BorrowedZUtf8String(&'a Type),
    OwnedZUtf8String(&'a Type),
}

pub fn nullable<'a>(ty: &'a Type, types: &TypeContext<'_>) -> Option<Nullable<'a>> {
    let payload = std_maybe_payload(ty, types)?;
    match payload {
        Type::Handle(..) => Some(Nullable::Handle(payload)),
        Type::Reference(inner, false) if is_handle(inner) => Some(Nullable::BorrowedHandle(inner)),
        Type::Reference(inner, false) if zstring(inner, types) => {
            Some(Nullable::BorrowedZString(inner))
        }
        Type::Reference(inner, false) if zutf8string(inner, types) => {
            Some(Nullable::BorrowedZUtf8String(inner))
        }
        Type::Reference(inner, false) if host_record(inner, types) => {
            Some(Nullable::BorrowedRecord(inner))
        }
        _ if zutf8string(payload, types) => Some(Nullable::OwnedZUtf8String(payload)),
        _ => None,
    }
}

/// The type as the C side sees it: `Maybe<H>` -> `H`, `Maybe<ref H>` -> `ref H`,
/// `Maybe<ref R>` -> `ref R`, `Maybe<ref ZString>` -> `ref ZString`,
/// `Maybe<ref ZUtf8String>` / `Maybe<ZUtf8String>` -> `ref ZUtf8String`, and any other type unchanged.
pub fn c_view<'a>(ty: &'a Type, types: &TypeContext<'_>) -> std::borrow::Cow<'a, Type> {
    match nullable(ty, types) {
        Some(Nullable::Handle(handle)) => std::borrow::Cow::Borrowed(handle),
        Some(Nullable::BorrowedHandle(handle)) => {
            std::borrow::Cow::Owned(Type::Reference(Box::new(handle.clone()), false))
        }
        Some(
            Nullable::BorrowedRecord(record)
            | Nullable::BorrowedZString(record)
            | Nullable::BorrowedZUtf8String(record)
            | Nullable::OwnedZUtf8String(record),
        ) => std::borrow::Cow::Owned(Type::Reference(Box::new(record.clone()), false)),
        None => std::borrow::Cow::Borrowed(ty),
    }
}

/// W22: whether `ty` is `FnPtr<F>` for a C-callable function type `F`.
pub fn fnptr(ty: &Type, types: &TypeContext<'_>) -> bool {
    matches!(ty, Type::FnPtr(inner) if callback(inner, types))
}

/// W22: whether `ty` contains `FnPtr` in a position that receives values from the host.
pub fn reaches_fnptr(ty: &Type, types: &TypeContext<'_>) -> bool {
    types.reaches(ty, |part| matches!(part, Type::FnPtr(_)))
        || matches!(ty, Type::Function(params, res) if params.iter().any(|p| reaches_fnptr(p, types)) || reaches_fnptr(res, types))
}

pub fn extern_scalar(ty: &Type, types: &TypeContext<'_>) -> bool {
    abi_scalar(ty)
        || matches!(
            nullable(ty, types),
            Some(Nullable::Handle(_) | Nullable::BorrowedHandle(_))
        )
}

/// W13: whether `inner` may be passed as `ref mut inner` with copy-in/copy-out on a native `extern`.
pub fn mutable_extern_scalar(inner: &Type) -> bool {
    inner.exportable() || matches!(inner, Type::Handle(_, crate::syntax::HandleKind::Copy))
}

/// W13: returns `(buffer, mutable)` when `ty` is a shared or mutable `extern` buffer parameter.
pub fn extern_buffer_parameter(ty: &Type) -> Option<(Buffer, bool)> {
    match ty {
        Type::Reference(inner, false) => Buffer::of_extern(inner).map(|b| (b, false)),
        Type::Reference(inner, true) => Buffer::of_extern_mutable(inner).map(|b| (b, true)),
        _ => None,
    }
}

/// W13 / W20 / M06: whether an `extern` uses native-only `ref mut`, `[i16u]`/`[i32]`/`[i32u]` buffers,
/// `ref ZString`, `ref ZUtf8String`, or `@count`.
pub fn uses_w13_native_only(
    parameters: &[Type],
    counts: &[(usize, crate::syntax::CountLength)],
    types: &TypeContext<'_>,
) -> bool {
    !counts.is_empty()
        || parameters.iter().any(|ty| match ty {
            Type::Reference(_, true) => true,
            Type::Reference(inner, false) => {
                any_zstring(inner, types)
                    || matches!(
                        Buffer::of_extern(inner),
                        Some(Buffer::I16U | Buffer::I32 | Buffer::I32U)
                    )
            }
            _ => false,
        })
}

pub fn extern_parameter(ty: &Type, types: &TypeContext<'_>) -> bool {
    parameter(ty, types)
        || extern_scalar(ty, types)
        || fnptr(ty, types)
        || c_layout_record(ty, types)
        || matches!(ty, Type::Reference(inner, false) if c_layout_record(inner, types))
        || matches!(
            nullable(ty, types),
            Some(
                Nullable::BorrowedRecord(_)
                    | Nullable::BorrowedZString(_)
                    | Nullable::BorrowedZUtf8String(_)
            )
        )
        || matches!(ty, Type::Reference(inner, false) if any_zstring(inner, types))
        || extern_buffer_parameter(ty).is_some()
        || matches!(
            ty,
            Type::Reference(inner, true)
                if host_record_bidirectional(inner, types) || mutable_extern_scalar(inner)
        )
}

pub fn extern_result(ty: &Type, types: &TypeContext<'_>) -> bool {
    result(ty, types)
        || matches!(
            nullable(ty, types),
            Some(Nullable::Handle(_) | Nullable::OwnedZUtf8String(_))
        )
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
        Type::FnPtr(inner) => match inner.as_ref() {
            Type::Function(cb_params, _) => cb_params.iter().any(|p| nullable(p, types).is_some()),
            _ => false,
        },
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

/// W36: how a record in an `extern def` parameter or result position crosses the C boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecordPassing {
    /// `const T *` / `T *` for a parameter, the leading out parameter `T *out` for a result (E05, W21).
    Pointer,
    /// The C struct itself, passed and returned as the C ABI of the target does it.
    ByValue,
}

/// W36: the passing of `ty` in a parameter or result position of an `extern def`. A `@c_layout`
/// record without `ref` is the C struct by value; an unmarked record, `ref R` and `ref mut R` are
/// pointers; anything that is not a record is `None`. `export def` and callbacks do not use this:
/// their records stay pointers.
pub fn extern_record_passing(ty: &Type, types: &TypeContext<'_>) -> Option<RecordPassing> {
    match ty {
        Type::Record(..) if any_zstring(ty, types) => None,
        Type::Record(..) if c_layout_record(ty, types) => Some(RecordPassing::ByValue),
        Type::Record(..) => Some(RecordPassing::Pointer),
        Type::Reference(inner, _) if matches!(inner.as_ref(), Type::Record(..)) => {
            extern_record_passing(inner, types).map(|_| RecordPassing::Pointer)
        }
        _ => None,
    }
}

/// W36: the C shape of a `@c_layout` record, independent of the target: its packing and the C types
/// of its fields in order. Two records of one shape have one layout and pass the same way on every
/// target, so two declarations of one symbol may name either (W14).
fn c_layout_shape(ty: &Type, types: &TypeContext<'_>, depth: usize, shape: &mut String) {
    match ty {
        Type::Integer(bits, _) => {
            shape.push('i');
            shape.push_str(&bits.to_string());
        }
        Type::Binary(32) => shape.push_str("float"),
        Type::Binary(64) => shape.push_str("double"),
        Type::FixedArray(element, _) => {
            shape.push('[');
            shape.push_str(&ty.fixed_length().unwrap_or(0).to_string());
            shape.push_str(" x ");
            c_layout_shape(element, types, depth + 1, shape);
            shape.push(']');
        }
        Type::Record(id, arguments)
            if depth < crate::syntax::MAX_NESTING && types.records[*id].c_layout.is_some() =>
        {
            shape.push_str("struct");
            if let Some(pack) = types.records[*id].c_layout.and_then(|layout| layout.pack) {
                shape.push_str(" pack(");
                shape.push_str(&pack.to_string());
                shape.push(')');
            }
            shape.push_str(" { ");
            for (index, field) in types.record_fields(*id, arguments).iter().enumerate() {
                if index > 0 {
                    shape.push_str(", ");
                }
                c_layout_shape(field, types, depth + 1, shape);
            }
            shape.push_str(" }");
        }
        // Handles, `Maybe` of one, `FnPtr` and borrowed NUL-terminated strings are one pointer.
        _ => shape.push_str("ptr"),
    }
}

/// The LLVM / C scalar or pointer type of one lowered host parameter or result (W14), or a
/// `@c_layout` record by value (W36), whose lowering depends on the target.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum CType {
    I32,
    I64,
    F32,
    F64,
    Ptr,
    Void,
    /// W36: a C struct by value, named by its target-independent shape (`c_layout_shape`).
    ByValue(String),
}

/// The native 64-bit C ABI register/stack class of a `CType` (W14).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum AbiClass {
    I32,
    Word64,
    F32,
    F64,
    Void,
    /// W36: a C struct by value. Only a struct of the same shape passes the same way on every target.
    ByValue(String),
}

impl CType {
    pub fn class(&self) -> AbiClass {
        match self {
            Self::I32 => AbiClass::I32,
            Self::I64 | Self::Ptr => AbiClass::Word64,
            Self::F32 => AbiClass::F32,
            Self::F64 => AbiClass::F64,
            Self::Void => AbiClass::Void,
            Self::ByValue(shape) => AbiClass::ByValue(shape.clone()),
        }
    }

    /// The LLVM type of a scalar or pointer; `None` for a struct by value, which
    /// `llvm_byvalue::classify` lowers for the target.
    pub fn llvm(&self) -> Option<&'static str> {
        match self {
            Self::I32 => Some("i32"),
            Self::I64 => Some("i64"),
            Self::F32 => Some("float"),
            Self::F64 => Some("double"),
            Self::Ptr => Some("ptr"),
            Self::Void => Some("void"),
            Self::ByValue(_) => None,
        }
    }

    fn canonical(&self, other: &Self) -> Self {
        debug_assert_eq!(self.class(), other.class());
        if self == other {
            self.clone()
        } else {
            Self::I64
        }
    }
}

impl AbiClass {
    pub fn description(&self) -> String {
        match self {
            Self::I32 => "a 32-bit integer".into(),
            Self::Word64 => "a 64-bit integer or pointer".into(),
            Self::F32 => "a 32-bit float".into(),
            Self::F64 => "a 64-bit float".into(),
            Self::Void => "void".into(),
            Self::ByValue(shape) => format!("the C {shape} by value"),
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
        for (slot, next) in self.parameters.iter_mut().zip(&other.parameters) {
            if slot != next {
                mixed = true;
                *slot = slot.canonical(next);
            }
        }
        if self.result != other.result {
            mixed = true;
            self.result = self.result.canonical(&other.result);
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
        Type::Handle(..) | Type::FnPtr(_) => CType::Ptr,
        Type::Reference(inner, false)
            if is_handle(inner) || matches!(inner.as_ref(), Type::Record(..)) =>
        {
            CType::Ptr
        }
        _ => unreachable!("checked ABI scalar: {ty:?}"),
    }
}

pub fn c_signature(
    parameters: &[Type],
    result: &Type,
    counts: &[(usize, crate::syntax::CountLength)],
    types: &TypeContext<'_>,
) -> CSignature {
    let by_value = |ty: &Type| {
        (extern_record_passing(ty, types) == Some(RecordPassing::ByValue)).then(|| {
            let mut shape = String::new();
            c_layout_shape(ty, types, 0, &mut shape);
            CType::ByValue(shape)
        })
    };
    let mut lowered = Vec::new();
    let viewed_result = c_view(result, types);
    // W36: a `@c_layout` record result is the C struct by value, not the leading out pointer. Where
    // the target returns it in memory, the `sret` pointer is not a parameter of this signature.
    let result_by_value = by_value(&viewed_result);
    if result_by_value.is_none() && out_result(&viewed_result) {
        lowered.push(CType::Ptr);
    }
    for (index, parameter) in parameters.iter().enumerate() {
        if *parameter == Type::Unit {
            continue;
        }
        if matches!(parameter, Type::Function(..) | Type::FnPtr(_)) {
            lowered.push(CType::Ptr);
            continue;
        }
        if extern_buffer_parameter(parameter).is_some() {
            lowered.push(CType::Ptr);
            match counts
                .iter()
                .find_map(|(pos, len)| (*pos == index).then_some(*len))
            {
                Some(crate::syntax::CountLength::None) => {}
                Some(crate::syntax::CountLength::I32 | crate::syntax::CountLength::I32U) => {
                    lowered.push(CType::I32);
                }
                Some(crate::syntax::CountLength::I64 | crate::syntax::CountLength::I64U)
                | Option::None => {
                    lowered.push(CType::I64);
                }
            }
            continue;
        }
        if matches!(parameter, Type::Reference(_, true)) {
            lowered.push(CType::Ptr);
            continue;
        }
        if let Some(record) = by_value(parameter) {
            lowered.push(record);
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
    let lowered_result = if let Some(record) = result_by_value {
        record
    } else if *viewed_result == Type::Unit || out_result(&viewed_result) {
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
            include_str!("runtime/debug-windows.ll"),
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
