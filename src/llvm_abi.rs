use super::*;
use crate::abi::{Buffer, abi_scalar, field_name, out_result};

pub(super) fn extended(function: &CheckedFunction) -> bool {
    function.exported
        && (function
            .signature
            .parameters
            .iter()
            .any(|ty| !abi_scalar(ty))
            || out_result(&function.signature.result))
}

/// NS01: native exports taking non-`Maybe` `H` or `ref H` validate non-null entry pointers.
pub(super) fn has_handle_parameter(function: &CheckedFunction) -> bool {
    function.signature.parameters.iter().any(|ty| match ty {
        Type::Handle(..) => true,
        Type::Reference(inner, false) => matches!(inner.as_ref(), Type::Handle(..)),
        _ => false,
    })
}

fn host_function(function: &CheckedFunction) -> bool {
    function.exported
        || matches!(&function.body.kind, TypedExprKind::HostCall(import, _) if !import.lowers_inline())
}

pub(crate) fn uses_host_abi(module: &CheckedModule) -> bool {
    let types = module.types();
    module.functions.iter().any(|function| {
        extended(function)
            || (matches!(&function.body.kind, TypedExprKind::HostCall(import, _) if !import.lowers_inline())
                && (function.signature.parameters.iter().any(|ty| {
                    !crate::abi::extern_scalar(ty, &types)
                        && *ty != Type::Unit
                        && !matches!(ty, Type::Function(..) | Type::FnPtr(_))
                }) || out_result(&function.signature.result)))
    })
}

/// The C typedef name of an `extern type`, spelled like `record_name` so that
/// module boundaries cannot collide (`A_B.C` and `A.B_C` differ).
pub(crate) fn handle_c_name(name: &str) -> String {
    let mut text = String::from("tz_handle");
    for segment in name.split('.') {
        let _ = write!(text, "_{}{segment}", segment.len());
    }
    text
}

/// Typedefs for the extern handles that the header's prototypes and `@c_layout` structs mention,
/// in name order.
pub(super) fn handle_typedefs(module: &CheckedModule) -> String {
    fn visit(
        ty: &Type,
        module: &CheckedModule,
        types: &crate::check::TypeContext<'_>,
        names: &mut BTreeSet<String>,
        seen_records: &mut BTreeSet<Type>,
    ) {
        let viewed = crate::abi::c_view(ty, types);
        match viewed.as_ref() {
            Type::Handle(name, _) => {
                names.insert(name.to_string());
            }
            Type::Reference(inner, _) => visit(inner, module, types, names, seen_records),
            Type::FnPtr(inner) => visit(inner, module, types, names, seen_records),
            Type::Function(parameters, result) => {
                parameters
                    .iter()
                    .for_each(|ty| visit(ty, module, types, names, seen_records));
                visit(result, module, types, names, seen_records);
            }
            Type::Record(id, arguments) if module.records[*id].c_layout.is_some() => {
                if !seen_records.insert(viewed.as_ref().clone()) {
                    return;
                }
                for field in types.record_fields(*id, arguments) {
                    visit(&field, module, types, names, seen_records);
                }
            }
            _ => {}
        }
    }
    let types = module.types();
    let mut names = BTreeSet::new();
    let mut seen_records = BTreeSet::new();
    for function in module.functions.iter().filter(|function| {
        // An explicit import has no prototype in the header, though its `@c_layout` structs do.
        host_function(function)
            && !matches!(&function.body.kind, TypedExprKind::HostCall(import, _) if import.explicit)
    }) {
        for ty in function
            .signature
            .parameters
            .iter()
            .chain([&function.signature.result])
        {
            visit(ty, module, &types, &mut names, &mut seen_records);
        }
    }
    for record in record_types(module) {
        visit(&record, module, &types, &mut names, &mut seen_records);
    }
    // Each behind a guard of its own: the headers of two libraries that pass the same handle then
    // define it once in a translation unit (C11 and C++ accept a second identical typedef, C99 does not).
    let mut output: String = names
        .iter()
        .map(|name| {
            let c_name = handle_c_name(name);
            format!(
                "#ifndef TSUZURI_HANDLE_{c_name}\n#define TSUZURI_HANDLE_{c_name}\ntypedef struct {c_name}_s *{c_name};\n#endif\n"
            )
        })
        .collect();
    if !output.is_empty() {
        output.push('\n');
    }
    output
}

/// Whether the header contains any `@c_layout` struct definition (W21).
pub(super) fn has_c_layout_records(module: &CheckedModule) -> bool {
    record_types(module)
        .iter()
        .any(|ty| matches!(ty, Type::Record(id, _) if module.records[*id].c_layout.is_some()))
}

pub(crate) fn record_name(ty: &Type, module: &CheckedModule) -> String {
    let Type::Record(id, arguments) = ty else {
        unreachable!("ABI record checked")
    };
    let mut name = String::from("tz_record");
    for segment in module.records[*id].name.split('.') {
        let _ = write!(name, "_{}{segment}", segment.len());
    }
    for argument in arguments {
        let text = canonical_type(argument, module);
        let _ = write!(name, "_T{}_", text.len());
        for byte in text.bytes() {
            let _ = write!(name, "{byte:02X}");
        }
    }
    name
}

fn record_types_for(module: &CheckedModule, include_views: bool) -> Vec<Type> {
    fn visit(ty: &Type, module: &CheckedModule, seen: &mut BTreeSet<Type>, result: &mut Vec<Type>) {
        let viewed = crate::abi::c_view(ty, &module.types());
        let ty = if let Type::Reference(inner, _) = viewed.as_ref() {
            inner.as_ref()
        } else {
            viewed.as_ref()
        };
        if let Type::Record(id, arguments) = ty
            && !crate::abi::any_zstring(ty, &module.types())
        {
            // The convention variants of a record are one type (A29).
            let ty = &codegen_identity(ty);
            if !seen.insert(ty.clone()) {
                return;
            }
            for field in module.types().record_fields(*id, arguments) {
                visit(&field, module, seen, result);
            }
            result.push(ty.clone());
        }
    }
    let mut seen = BTreeSet::new();
    let mut result = Vec::new();
    for function in module.functions.iter().filter(|function| {
        host_function(function)
            || (include_views
                && matches!(&function.body.kind, TypedExprKind::HostCall(import, _) if import.lowers_inline()))
    }) {
        for ty in function
            .signature
            .parameters
            .iter()
            .chain([&function.signature.result])
        {
            visit(ty, module, &mut seen, &mut result);
        }
    }
    result
}

fn record_types(module: &CheckedModule) -> Vec<Type> {
    record_types_for(module, false)
}

/// W21: layout of one field inside a `@c_layout` record.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CField {
    pub offset: usize,
    pub size: usize,
    pub align: usize,
    pub llvm_index: usize,
}

fn c_layout_element_layout(ty: &Type, module: &CheckedModule) -> (usize, usize) {
    match ty {
        Type::Integer(8, _) => (1, 1),
        Type::Integer(16, _) => (2, 2),
        Type::Integer(32, _) | Type::Binary(32) => (4, 4),
        Type::Integer(64, _) | Type::Binary(64) => (8, 8),
        Type::Handle(..) | Type::FnPtr(_) | Type::Reference(..) => (8, 8),
        Type::FixedArray(element, _) => {
            let (size, align) = c_layout_element_layout(element, module);
            (size * fixed_length(ty), align)
        }
        Type::Record(..) => record_layout(ty, module),
        _ => {
            debug_assert!(
                crate::abi::nullable(ty, &module.types()).is_some(),
                "unexpected @c_layout field type: {ty:?}"
            );
            (8, 8)
        }
    }
}

/// W21: exact C layout fields, total size, and struct alignment for a `@c_layout` record.
pub(crate) fn c_layout_fields(
    ty: &Type,
    module: &CheckedModule,
) -> Option<(Vec<CField>, usize, usize)> {
    let Type::Record(id, arguments) = ty else {
        return None;
    };
    let layout = module.records[*id].c_layout?;
    let pack = layout.pack.map(usize::from);
    let mut fields = Vec::new();
    let mut offset = 0usize;
    let mut struct_align = 1usize;
    let mut llvm_index = 0usize;
    for field in module.types().record_fields(*id, arguments) {
        let (fsize, natural_align) = c_layout_element_layout(&field, module);
        let eff_align = pack.map_or(natural_align, |cap| natural_align.min(cap));
        let aligned = offset.div_ceil(eff_align) * eff_align;
        if aligned > offset {
            llvm_index += 1;
        }
        fields.push(CField {
            offset: aligned,
            size: fsize,
            align: eff_align,
            llvm_index,
        });
        llvm_index += 1;
        offset = aligned + fsize;
        struct_align = struct_align.max(eff_align);
    }
    let total_size = offset.max(1).div_ceil(struct_align) * struct_align;
    Some((fields, total_size, struct_align))
}

fn c_layout_llvm_field(ty: &Type, module: &CheckedModule) -> String {
    match ty {
        Type::Integer(bits, _) => format!("i{bits}"),
        Type::Binary(32) => "float".into(),
        Type::Binary(64) => "double".into(),
        Type::Record(..) => format!("%{}", record_name(ty, module)),
        Type::FixedArray(element, _) => {
            format!(
                "[{} x {}]",
                fixed_length(ty),
                c_layout_llvm_field(element, module)
            )
        }
        _ => "ptr".into(),
    }
}

fn c_layout_c_number(ty: &Type) -> &'static str {
    match ty {
        Type::Integer(8, true) => "int8_t",
        Type::Integer(8, false) => "uint8_t",
        Type::Integer(16, true) => "int16_t",
        Type::Integer(16, false) => "uint16_t",
        Type::Integer(32, true) => "int32_t",
        Type::Integer(32, false) => "uint32_t",
        Type::Integer(64, true) => "int64_t",
        Type::Integer(64, false) => "uint64_t",
        Type::Binary(32) => "float",
        Type::Binary(64) => "double",
        _ => unreachable!("expected @c_layout number: {ty:?}"),
    }
}

fn c_layout_c_field(ty: &Type, name: &str, module: &CheckedModule) -> String {
    let types = module.types();
    let viewed = crate::abi::c_view(ty, &types);
    match viewed.as_ref() {
        Type::Integer(..) | Type::Binary(..) => {
            format!("{} {name}", c_layout_c_number(viewed.as_ref()))
        }
        Type::FixedArray(element, _) => format!(
            "{} {name}[{}]",
            c_layout_c_number(element),
            fixed_length(viewed.as_ref())
        ),
        Type::Record(..) => format!("{} {name}", record_name(viewed.as_ref(), module)),
        Type::Handle(hname, _) => format!("{} {name}", handle_c_name(hname)),
        Type::Reference(inner, false) if crate::abi::zstring(inner, &types) => {
            format!("const uint16_t *{name}")
        }
        Type::Reference(inner, false) if crate::abi::zutf8string(inner, &types) => {
            format!("const char *{name}")
        }
        Type::FnPtr(inner) => {
            let Type::Function(inputs, output) = inner.as_ref() else {
                unreachable!("validated FnPtr")
            };
            let inputs = if matches!(inputs.as_slice(), [Type::Unit]) {
                "void".to_owned()
            } else {
                inputs
                    .iter()
                    .map(|input| c_type(&crate::abi::c_view(input, &types)))
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            format!("{} (*{name})({inputs})", c_type(output))
        }
        other => unreachable!("validated @c_layout field type: {other:?}"),
    }
}

pub(super) fn type_definitions(module: &CheckedModule) -> String {
    let has_views = module.functions.iter().any(
        |f| matches!(&f.body.kind, TypedExprKind::HostCall(import, _) if import.lowers_inline()),
    );
    if !uses_host_abi(module) && !has_views {
        return String::new();
    }
    let mut output = String::new();
    for ty in record_types_for(module, true) {
        let Type::Record(id, arguments) = &ty else {
            unreachable!()
        };
        if let Some((cfields, total_size, _)) = c_layout_fields(&ty, module) {
            let field_types = module.types().record_fields(*id, arguments);
            let mut parts = Vec::new();
            let mut cursor = 0usize;
            for (cfield, fty) in cfields.iter().zip(&field_types) {
                if cfield.offset > cursor {
                    parts.push(format!("[{} x i8]", cfield.offset - cursor));
                }
                parts.push(c_layout_llvm_field(fty, module));
                cursor = cfield.offset + cfield.size;
            }
            if total_size > cursor {
                parts.push(format!("[{} x i8]", total_size - cursor));
            }
            let _ = writeln!(
                output,
                "%{} = type <{{ {} }}>",
                record_name(&ty, module),
                parts.join(", ")
            );
            continue;
        }
        let fields = module.types().record_fields(*id, arguments);
        let fields = if fields.is_empty() {
            "i8".into()
        } else {
            fields
                .iter()
                .map(|ty| record_field_type(ty, module))
                .collect::<Vec<_>>()
                .join(", ")
        };
        let _ = writeln!(
            output,
            "%{} = type {{ {fields} }}",
            record_name(&ty, module)
        );
    }
    output
}

fn record_field_type(ty: &Type, module: &CheckedModule) -> String {
    match ty {
        Type::Record(..) => format!("%{}", record_name(ty, module)),
        Type::FixedArray(element, _) => format!("[{} x {}]", fixed_length(ty), abi_type(element)),
        _ => abi_type(ty),
    }
}

/// The version of `common_declarations`. Raise it whenever their text changes, so that headers of an
/// earlier shape and of the new one stop with `#error` in one translation unit instead of
/// disagreeing about a type that crosses every library boundary.
pub(super) const COMMON_DECLARATIONS_VERSION: u32 = 1;

/// The buffer descriptors that a header can name (`Buffer::of`), in the order of `Buffer`.
const HEADER_BUFFERS: [Buffer; 5] = [
    Buffer::I64,
    Buffer::F64,
    Buffer::UByte,
    Buffer::String,
    Buffer::Utf8String,
];

/// The declarations that every header of the host ABI or of `--trap-mode return` shares, whatever
/// its module uses: the buffer descriptors and `tsuzuri_trap_info`. They are the same bytes in
/// every header of one version and stand behind one guard, so the headers of several libraries can
/// be included in one C or C++ translation unit. Each struct has a tag, so a type is one type
/// however many headers name it (two untagged `typedef struct { ... }` of the same shape are two
/// types, and a second one is an error in C and C++).
///
/// Headers of two versions in one translation unit are an error (`#error`): their types would carry
/// the same names with different shapes, which no compiler accepts, and silently keeping the first
/// would give the other library's prototypes the wrong layout. The version changes only with the
/// text, so headers of compilers that agree on it mix freely. `TSUZURI_TRAP_INFO_DEFINED` stays
/// inside, as the runtime's `trap.c` defines the same type under that guard.
///
/// The prototypes of `tsuzuri_alloc` and `tsuzuri_free` stay outside (`header_types`): a header
/// declares them only when its library defines them, and C and C++ accept the same function
/// declared again. `tsuzuri_allocation_stats` keeps its own guard, as only `--allocator counting`
/// has it.
pub(super) fn common_declarations() -> String {
    let version = COMMON_DECLARATIONS_VERSION;
    let mut output = format!(
        "/* Shared by every Tsuzuri header (version {version}), so that the headers of several libraries can be\n   included in one translation unit. Headers with another version of these declarations stop with #error. */\n\
         #ifndef TSUZURI_COMMON_DECLARATIONS_{version}\n\
         #ifdef TSUZURI_COMMON_DECLARATIONS_VERSION\n\
         #error \"Tsuzuri headers with different versions of the common declarations cannot be included together; generate them with the same Tsuzuri release\"\n\
         #else\n\
         #define TSUZURI_COMMON_DECLARATIONS_{version}\n\
         #define TSUZURI_COMMON_DECLARATIONS_VERSION {version}\n"
    );
    for buffer in HEADER_BUFFERS {
        let (name, element) = (buffer.name(), buffer.c_element());
        let _ = writeln!(
            output,
            "typedef struct tsuzuri_{name}_slice {{ const {element} *ptr; int64_t len; }} tsuzuri_{name}_slice;\n\
             typedef struct tsuzuri_{name}_buffer {{ {element} *ptr; int64_t len; }} tsuzuri_{name}_buffer;"
        );
    }
    output.push_str(
        "/* What --trap-mode return reports of a trap: `site` is an id of <output>.trap.json, `kind` a trap kind. */\n\
         #ifndef TSUZURI_TRAP_INFO_DEFINED\n\
         #define TSUZURI_TRAP_INFO_DEFINED\n\
         typedef struct tsuzuri_trap_info {\n    uint32_t site;\n    uint32_t kind;\n} tsuzuri_trap_info;\n\
         #endif\n\
         #endif\n\
         #endif\n\n",
    );
    output
}

/// `definition` behind a guard named after the record, so that the headers of two libraries that
/// export the same record (a module of a shared dependency) define it once. The guard's value
/// identifies the definition's text: a header whose record of that name has other fields (two
/// versions of the module, or two modules of one name such as each library's `Main`) stops with
/// `#error` instead of giving one library's functions the other's layout.
fn guarded_record(output: &mut String, rname: &str, definition: &str) {
    let mut hash = crate::cache::Sha256::new();
    hash.update(definition.as_bytes());
    let layout: String = hash.finalize()[..8]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    let _ = write!(
        output,
        "#ifndef TSUZURI_LAYOUT_{rname}\n#define TSUZURI_LAYOUT_{rname} 0x{layout}\n{definition}\
         #elif TSUZURI_LAYOUT_{rname} != 0x{layout}\n\
         #error \"{rname} has other fields in another Tsuzuri header: two libraries export different records of this name\"\n\
         #endif\n"
    );
}

/// The record structs of the header, each behind its guard (`guarded_record`), and the prototypes
/// of the allocator. The buffer descriptors are in `common_declarations`.
pub(super) fn header_types(module: &CheckedModule) -> String {
    if !uses_host_abi(module) {
        return String::new();
    }
    let mut output = String::new();
    for ty in record_types(module) {
        let Type::Record(id, arguments) = &ty else {
            unreachable!()
        };
        let rname = record_name(&ty, module);
        let mut definition = String::new();
        if let Some((cfields, total_size, _)) = c_layout_fields(&ty, module) {
            let pack = module.records[*id].c_layout.and_then(|c| c.pack);
            if let Some(n) = pack {
                let _ = writeln!(definition, "#pragma pack(push, {n})");
            }
            let _ = writeln!(definition, "typedef struct {rname} {{");
            for ((name, _), field) in module.records[*id]
                .fields
                .iter()
                .zip(module.types().record_fields(*id, arguments))
            {
                let cname = field_name(name);
                let declaration = c_layout_c_field(&field, &cname, module);
                let _ = writeln!(definition, "    {declaration};");
            }
            let _ = writeln!(definition, "}} {rname};");
            if pack.is_some() {
                definition.push_str("#pragma pack(pop)\n");
            }
            let _ = writeln!(
                definition,
                "typedef char {rname}__size[(sizeof({rname}) == {total_size}) ? 1 : -1];"
            );
            for ((name, _), cfield) in module.records[*id].fields.iter().zip(&cfields) {
                let cname = field_name(name);
                let _ = writeln!(
                    definition,
                    "typedef char {rname}__f_{cname}[(offsetof({rname}, {cname}) == {}) ? 1 : -1];",
                    cfield.offset
                );
            }
        } else {
            let _ = writeln!(definition, "typedef struct {rname} {{");
            if module.records[*id].fields.is_empty() {
                definition.push_str("    uint8_t tz_empty;\n");
            }
            for ((name, _), field) in module.records[*id]
                .fields
                .iter()
                .zip(module.types().record_fields(*id, arguments))
            {
                let declaration = match &field {
                    Type::Record(..) => {
                        format!("{} {}", record_name(&field, module), field_name(name))
                    }
                    Type::FixedArray(element, _) => format!(
                        "{} {}[{}]",
                        c_type(element),
                        field_name(name),
                        fixed_length(&field)
                    ),
                    _ => format!("{} {}", c_type(&field), field_name(name)),
                };
                let _ = writeln!(definition, "    {declaration};");
            }
            let _ = writeln!(definition, "}} {rname};");
        }
        guarded_record(&mut output, &rname, &definition);
    }
    if !output.is_empty() {
        output.push('\n');
    }
    output.push_str("void *tsuzuri_alloc(int64_t size);\nvoid tsuzuri_free(void *ptr);\n\n");
    output
}

pub(super) fn c_parameters(function: &CheckedFunction, module: &CheckedModule) -> String {
    let types = module.types();
    let import = match &function.body.kind {
        TypedExprKind::HostCall(import, _) => Some(import.as_ref()),
        _ => None,
    };
    // W36: an `extern def` passes and returns a `@c_layout` record without `ref` as the C struct
    // itself. An `export def` keeps the pointer forms for every record.
    let by_value = |ty: &Type| {
        import.is_some()
            && crate::abi::extern_record_passing(ty, &types)
                == Some(crate::abi::RecordPassing::ByValue)
    };
    let mut parameters = Vec::new();
    let result = &function.signature.result;
    if let Some(buffer) = Buffer::of(result) {
        parameters.push(format!("tsuzuri_{}_buffer *out", buffer.name()));
    } else if matches!(result, Type::Record(..)) && !by_value(result) {
        parameters.push(format!("{} *out", record_name(result, module)));
    }
    for (index, ty) in function.signature.parameters.iter().enumerate() {
        if *ty == Type::Unit {
            continue;
        }
        if by_value(ty) {
            parameters.push(format!("{} arg{index}", record_name(ty, module)));
            continue;
        }
        if let Type::Function(inputs, output) = match ty {
            Type::FnPtr(inner) => inner.as_ref(),
            other => other,
        } {
            // A callback is a C function pointer; a lone `unit` input is `void`.
            let inputs = if matches!(inputs.as_slice(), [Type::Unit]) {
                "void".to_owned()
            } else {
                inputs
                    .iter()
                    .map(|input| c_type(&crate::abi::c_view(input, &types)))
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            parameters.push(format!("{} (*arg{index})({inputs})", c_type(output)));
            continue;
        }
        if let Some((buffer, mutable)) = crate::abi::extern_buffer_parameter(ty) {
            let qual = if mutable { "" } else { "const " };
            parameters.push(format!("{qual}{} *arg{index}_ptr", buffer.c_element()));
            let count_len = import.and_then(|imp| imp.count_at(index));
            if let Some(len_c_ty) =
                count_len.map_or(Some("int64_t"), crate::syntax::CountLength::c_type)
            {
                parameters.push(format!("{len_c_ty} arg{index}_len"));
            }
            continue;
        }
        if let Type::Reference(inner, true) = ty {
            if matches!(inner.as_ref(), Type::Record(..)) {
                parameters.push(format!("{} *arg{index}", record_name(inner, module)));
            } else {
                let pointee = match inner.as_ref() {
                    Type::Integer(8, true) => "int8_t".to_owned(),
                    Type::Integer(8, false) => "uint8_t".to_owned(),
                    Type::Integer(16, true) => "int16_t".to_owned(),
                    Type::Integer(16, false) => "uint16_t".to_owned(),
                    Type::Integer(32, true) | Type::Bool => "int32_t".to_owned(),
                    Type::Integer(32, false) => "uint32_t".to_owned(),
                    Type::Integer(64, true) => "int64_t".to_owned(),
                    Type::Integer(64, false) => "uint64_t".to_owned(),
                    Type::Binary(32) => "float".to_owned(),
                    Type::Binary(64) => "double".to_owned(),
                    Type::Handle(name, _) => handle_c_name(name),
                    _ => unreachable!("validated mutable extern scalar: {inner:?}"),
                };
                parameters.push(format!("{pointee} *arg{index}"));
            }
            continue;
        }
        let viewed = crate::abi::c_view(ty, &types);
        let ty = viewed.as_ref();
        if let Type::Reference(inner, false) = ty
            && crate::abi::zstring(inner, &types)
        {
            parameters.push(format!("const uint16_t *arg{index}"));
            continue;
        }
        if let Type::Reference(inner, false) = ty
            && crate::abi::zutf8string(inner, &types)
        {
            parameters.push(format!("const char *arg{index}"));
            continue;
        }
        let inner = if let Type::Reference(inner, false) = ty {
            inner.as_ref()
        } else {
            ty
        };
        if let Some(buffer) = Buffer::of(inner) {
            parameters.push(format!("const {} *arg{index}_ptr", buffer.c_element()));
            parameters.push(format!("int64_t arg{index}_len"));
        } else if matches!(inner, Type::Record(..)) {
            parameters.push(format!("const {} *arg{index}", record_name(inner, module)));
        } else {
            parameters.push(format!("{} arg{index}", c_type(ty)));
        }
    }
    if parameters.is_empty() {
        "void".into()
    } else {
        parameters.join(", ")
    }
}

/// The result type and `(type, name)` parameters of the `define` of `symbol` in `ir`.
fn defined_signature(ir: &str, symbol: &str) -> Option<(String, Vec<(String, String)>)> {
    ir.lines().find_map(|line| {
        if !line.starts_with("define ") {
            return None;
        }
        let call = super::traps::callable(line, true).filter(|call| call.name == symbol)?;
        let result = line["define ".len()..line.find('@')?].trim().to_owned();
        let parameters = line[call.open + 1..call.close]
            .split(',')
            .map(str::trim)
            .filter(|parameter| !parameter.is_empty())
            .map(|parameter| {
                let mut words = parameter.split_whitespace();
                let ty = words.next().unwrap_or_default().to_owned();
                (ty, words.next_back().unwrap_or_default().to_owned())
            })
            .collect();
        Some((result, parameters))
    })
}

/// `tsuzuri_try_<name>` for every export (E14 Phase 2): the same arguments as `tz_<name>` behind a
/// trap boundary, with the result in `*result` (or the export's own `out`) and status 0, 1 or 2.
pub(super) fn try_wrappers(ir: &str, module: &CheckedModule) -> String {
    let mut output = String::from("\ndeclare i32 @tsuzuri_boundary_run(ptr, ptr, ptr)\n");
    for function in module.functions.iter().filter(|function| function.exported) {
        let name = &function.name;
        let Some((result, parameters)) = defined_signature(ir, &format!("@tz_{name}")) else {
            continue;
        };
        let slot = result != "void";
        let mut fields = vec!["ptr"];
        fields.extend(parameters.iter().map(|(ty, _)| ty.as_str()));
        let frame = format!("{{ {} }}", fields.join(", "));
        let mut signature = vec!["ptr %trap".to_owned()];
        if slot {
            signature.push("ptr %result".to_owned());
        }
        signature.extend(parameters.iter().map(|(ty, value)| format!("{ty} {value}")));
        let _ = write!(
            output,
            "define i32 @tsuzuri_try_{name}({}) nounwind {{\nentry:\n  %frame = alloca {frame}, align 16\n  %slot0 = getelementptr inbounds {frame}, ptr %frame, i32 0, i32 0\n  store ptr {}, ptr %slot0\n",
            signature.join(", "),
            if slot { "%result" } else { "null" }
        );
        for (index, (ty, value)) in parameters.iter().enumerate() {
            let slot = index + 1;
            let _ = writeln!(
                output,
                "  %slot{slot} = getelementptr inbounds {frame}, ptr %frame, i32 0, i32 {slot}\n  store {ty} {value}, ptr %slot{slot}"
            );
        }
        let _ = write!(
            output,
            "  %status = call i32 @tsuzuri_boundary_run(ptr @tz.try.{name}, ptr %frame, ptr %trap)\n  ret i32 %status\n}}\ndefine internal void @tz.try.{name}(ptr %frame) nounwind {{\nentry:\n  %slot0 = getelementptr inbounds {frame}, ptr %frame, i32 0, i32 0\n  %destination = load ptr, ptr %slot0\n"
        );
        let mut arguments = Vec::new();
        for (index, (ty, _)) in parameters.iter().enumerate() {
            let slot = index + 1;
            let _ = writeln!(
                output,
                "  %slot{slot} = getelementptr inbounds {frame}, ptr %frame, i32 0, i32 {slot}\n  %value{slot} = load {ty}, ptr %slot{slot}"
            );
            arguments.push(format!("{ty} %value{slot}"));
        }
        if slot {
            let _ = writeln!(
                output,
                "  %result = call {result} @tz_{name}({})\n  store {result} %result, ptr %destination",
                arguments.join(", ")
            );
        } else {
            let _ = writeln!(output, "  call void @tz_{name}({})", arguments.join(", "));
        }
        output.push_str("  ret void\n}\n");
    }
    output
}

/// The C prototype of `tsuzuri_try_<name>`: the trap record, then the result slot, then the arguments.
pub(super) fn try_prototype(function: &CheckedFunction, module: &CheckedModule) -> String {
    let result = &function.signature.result;
    let parameters = c_parameters(function, module);
    let mut list = vec!["tsuzuri_trap_info *trap".to_owned()];
    if !out_result(result) && *result != Type::Unit {
        list.push(format!("{} *result", c_type(result)));
    }
    if parameters != "void" {
        list.push(parameters);
    }
    format!(
        "int32_t tsuzuri_try_{}({});",
        function.name,
        list.join(", ")
    )
}

/// Whether a function of the program can call itself again through direct calls. Only such a program
/// can exhaust its stack, so only its native executable carries the overflow report (E14 Phase 3).
/// Closures and other indirect calls are not followed, and runtime helpers are not considered.
pub(super) fn has_recursion(ir: &str) -> bool {
    use std::collections::HashMap;
    let tracked = |name: &str| {
        ["@tz.fn.", "@tz.specialized.", "@tz.apply."]
            .iter()
            .any(|prefix| name.starts_with(prefix))
    };
    let mut ids = HashMap::new();
    for line in ir.lines().filter(|line| line.starts_with("define ")) {
        if let Some(call) = super::traps::callable(line, true).filter(|call| tracked(call.name)) {
            let next = ids.len();
            ids.entry(call.name).or_insert(next);
        }
    }
    let mut edges = vec![Vec::new(); ids.len()];
    let mut current = None;
    for line in ir.lines() {
        if line.starts_with("define ") {
            current =
                super::traps::callable(line, true).and_then(|call| ids.get(call.name).copied());
        } else if line == "}" {
            current = None;
        } else if let Some(from) = current
            && line.contains("call ")
            && let Some(to) =
                super::traps::callable(line, false).and_then(|call| ids.get(call.name))
        {
            edges[from].push(*to);
        }
    }
    // 0: unvisited, 1: on the path being walked, 2: finished.
    let mut state = vec![0u8; edges.len()];
    for start in 0..edges.len() {
        if state[start] != 0 {
            continue;
        }
        state[start] = 1;
        let mut path = vec![(start, 0usize)];
        while let Some(&(node, next)) = path.last() {
            let Some(&to) = edges[node].get(next) else {
                state[node] = 2;
                path.pop();
                continue;
            };
            path.last_mut().unwrap().1 += 1;
            match state[to] {
                1 => return true,
                0 => {
                    state[to] = 1;
                    path.push((to, 0));
                }
                _ => {}
            }
        }
    }
    false
}

pub(super) fn allocator() -> &'static str {
    "\ndefine weak ptr @tsuzuri_alloc(i64 %size) nounwind {\nentry:\n  %valid = icmp sge i64 %size, 0\n  br i1 %valid, label %allocate, label %bad\nbad:\n  call void @llvm.trap()\n  unreachable\nallocate:\n  %empty = icmp eq i64 %size, 0\n  %bytes = select i1 %empty, i64 1, i64 %size\n  %value = call ptr @tz.alloc(i64 %bytes)\n  ret ptr %value\n}\ndefine weak void @tsuzuri_free(ptr %value) nounwind {\nentry:\n  call void @tz.free(ptr %value)\n  ret void\n}\n"
}

/// The functions a host defines for `--allocator host` (F13).
pub(super) const HOST_ALLOCATOR_PROTOTYPES: &str = "\n/* --allocator host: the host defines these. align is 16 and every size includes a 16-byte header;\n   free and realloc receive the size of the block's allocation. Return null to trap. */\n\
void *tsuzuri_host_alloc(uint64_t size, uint64_t align);\n\
void tsuzuri_host_free(void *ptr, uint64_t size, uint64_t align);\n\
void *tsuzuri_host_realloc(void *ptr, uint64_t old_size, uint64_t new_size, uint64_t align);\n";

/// The statistics of `--allocator counting` (F13 Phase 2). The headers of several such libraries
/// share the struct by its guard alone, so a new shape renames the guard (`llvm.rs` pins the text).
pub(super) const ALLOCATION_STATS_PROTOTYPE: &str = "\n/* --allocator counting: allocations and frees since the start, and the requested bytes\n   that are live now and were live at most. */\n\
#ifndef TSUZURI_ALLOCATION_STATS_DEFINED\n\
#define TSUZURI_ALLOCATION_STATS_DEFINED\n\
typedef struct tsuzuri_allocation_stats {\n    uint64_t allocations;\n    uint64_t frees;\n    uint64_t live_bytes;\n    uint64_t peak_bytes;\n} tsuzuri_allocation_stats;\n\
#endif\n\
void tsuzuri_alloc_stats(tsuzuri_allocation_stats *stats);\n";

pub(super) fn native_allocator() -> String {
    let mut output = allocator()
        .replace("call ptr @tz.alloc(", "call ptr @tz.abi.alloc(")
        .replace("call void @tz.free(", "call void @tz.abi.free(");
    output.push_str(
                "\n@tsuzuri_trap_hooks = weak hidden global [5 x ptr] zeroinitializer, align 8\n\
                 define internal ptr @tz.abi.alloc(i64 %size) nounwind {\nentry:\n\
                     %slot = getelementptr inbounds [5 x ptr], ptr @tsuzuri_trap_hooks, i32 0, i32 3\n\
                     %allocate = load ptr, ptr %slot\n\
                     %tracked = icmp ne ptr %allocate, null\n\
                     br i1 %tracked, label %boundary, label %ordinary\n\
                 boundary:\n  %tracked_value = call ptr %allocate(i64 %size)\n\
                     %failed = icmp eq ptr %tracked_value, null\n\
                     br i1 %failed, label %fail, label %done\n\
                 fail:\n  call void @llvm.trap()\n  unreachable\n\
                 done:\n  ret ptr %tracked_value\n\
                 ordinary:\n  %value = call ptr @tz.alloc(i64 %size)\n  ret ptr %value\n}\n\
                 define internal void @tz.abi.free(ptr %value) nounwind {\nentry:\n\
                     %slot = getelementptr inbounds [5 x ptr], ptr @tsuzuri_trap_hooks, i32 0, i32 4\n\
                     %release = load ptr, ptr %slot\n\
                     %tracked = icmp ne ptr %release, null\n\
                     br i1 %tracked, label %boundary, label %ordinary\n\
                 boundary:\n  call void %release(ptr %value)\n  ret void\n\
                 ordinary:\n  call void @tz.free(ptr %value)\n  ret void\n}\n",
        );
    output
}

/// What a C-ABI wrapper is for.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum WrapperKind {
    /// The public `tz_<name>` of an `export def`.
    Export,
    /// An internal `tz.callback.<name>` whose address goes to the host.
    Callback,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn wrapper(
    module: &CheckedModule,
    function: &CheckedFunction,
    id: usize,
    kind: WrapperKind,
    builtins: &mut Builtins,
    intrinsics: &mut BTreeSet<String>,
    globals: &mut Globals,
    specializations: &mut Specializations,
) -> String {
    let mut emitter = FunctionEmitter::new(
        module,
        function,
        id,
        builtins,
        intrinsics,
        globals,
        specializations,
    );
    emitter.current_span = function.span;
    if kind == WrapperKind::Callback && emitter.globals.guard_callbacks_after_main {
        let done = emitter.value("load atomic i32, ptr @tz.main.done acquire, align 4");
        let after = emitter.value(format!("icmp ne i32 {done}, 0"));
        let trap_label = emitter.label();
        let ready_label = emitter.label();
        emitter.branch(&after, &trap_label, &ready_label);
        emitter.begin(&trap_label);
        emitter.instruction("call void @tz.callback.after_main()");
        emitter.instruction("unreachable");
        emitter.begin(&ready_label);
    }
    let result = &function.signature.result;
    let mut parameters = Vec::new();
    if out_result(result) {
        parameters.push("ptr %out".into());
        let (size, align) = if matches!(result, Type::Record(..)) {
            record_layout(result, module)
        } else {
            (16, 8)
        };
        emitter.host_pointer("%out", &size.to_string(), align, false);
    }
    let mut values = Vec::new();
    for (index, ty) in function.signature.parameters.iter().enumerate() {
        // A callback over `unit` has no C parameter.
        if *ty == Type::Unit {
            values.push(format!("{} 0", emitter.ty(ty)));
            continue;
        }
        let inner = if let Type::Reference(inner, false) = ty {
            inner.as_ref()
        } else {
            ty
        };
        if let Some(buffer) = Buffer::of(inner) {
            let pointer = format!("%arg{index}_ptr");
            let length = format!("%arg{index}_len");
            parameters.extend([format!("ptr {pointer}"), format!("i64 {length}")]);
            let valid = emitter.value(format!(
                "icmp ule i64 {length}, {}",
                i64::MAX as u64 / buffer.width() as u64
            ));
            emitter.guard(&valid, TrapKind::AllocationSize);
            if buffer == Buffer::String {
                let valid = emitter.value(format!("icmp ule i64 {length}, 9007199254740991"));
                emitter.guard(&valid, TrapKind::StringConcatOverflow);
            }
            let bytes = emitter.value(format!("mul i64 {length}, {}", buffer.width()));
            emitter.host_pointer(&pointer, &bytes, buffer.width(), true);
            if buffer == Buffer::Utf8String {
                emitter.validate_host_utf8(&pointer, &length);
            }
            let descriptor = emitter.value(format!(
                "insertvalue {} zeroinitializer, ptr {pointer}, 0",
                emitter.ty(inner)
            ));
            let descriptor = emitter.value(format!(
                "insertvalue {} {descriptor}, i64 {length}, 1",
                emitter.ty(inner)
            ));
            let value = if matches!(inner, Type::Array(_)) {
                descriptor
            } else {
                emitter.spill(inner, &descriptor)
            };
            values.push(format!("{} {value}", emitter.ty(ty)));
        } else if matches!(inner, Type::Record(..)) {
            parameters.push(format!("ptr %arg{index}"));
            let (size, align) = record_layout(inner, module);
            emitter.host_pointer(&format!("%arg{index}"), &size.to_string(), align, false);
            let value = emitter.read_host_record(inner, &format!("%arg{index}"));
            let value = if matches!(ty, Type::Reference(..)) {
                emitter.spill(inner, &value)
            } else {
                value
            };
            values.push(format!("{} {value}", emitter.ty(ty)));
        } else if crate::abi::nullable(ty, &module.types()).is_some() {
            parameters.push(format!("ptr %arg{index}"));
            let value = emitter.nullable_from_c(ty, &format!("%arg{index}"));
            values.push(format!("{} {value}", emitter.ty(ty)));
        } else if let Type::Reference(handle, false) = ty
            && matches!(handle.as_ref(), Type::Handle(..))
        {
            // The host passes the handle itself; a borrow needs it in a slot.
            parameters.push(format!("ptr %arg{index}"));
            if !emitter.globals.wasm {
                let valid = emitter.value(format!("icmp ne ptr %arg{index}, null"));
                emitter.guard(&valid, TrapKind::NullHandle);
            }
            let slot = emitter.spill(handle, &format!("%arg{index}"));
            values.push(format!("ptr {slot}"));
        } else {
            parameters.push(format!("{} %arg{index}", abi_type(ty)));
            if !emitter.globals.wasm && matches!(ty, Type::Handle(..)) {
                let valid = emitter.value(format!("icmp ne ptr %arg{index}, null"));
                emitter.guard(&valid, TrapKind::NullHandle);
            }
            let value = emitter.decode_host_scalar(ty, &format!("%arg{index}"));
            values.push(format!("{} {value}", emitter.ty(ty)));
        }
    }
    let value = emitter.value(format!(
        "call {} @tz.fn.{}({})",
        emitter.ty(result),
        function.qualified_name(),
        values.join(", ")
    ));
    if Buffer::of(result).is_some() {
        // The host reads UTF-16 units: the buffer of a Latin-1 string is widened, and the host takes it.
        let value = if *result == Type::String && emitter.globals.strings == StringForm::Compact {
            emitter.value(format!(
                "call %tz.string @tz.string.to_utf16(%tz.string {value})"
            ))
        } else {
            value
        };
        let data = emitter.value(format!("extractvalue {} {value}, 0", emitter.ty(result)));
        let length = emitter.value(format!("extractvalue {} {value}, 1", emitter.ty(result)));
        emitter.instruction(format!("store ptr {data}, ptr %out"));
        let pointer =
            emitter.value("getelementptr inbounds %tz.abi.buffer, ptr %out, i32 0, i32 1");
        emitter.instruction(format!("store i64 {length}, ptr {pointer}"));
        emitter.instruction("ret void");
    } else if matches!(result, Type::Record(..)) {
        emitter.write_host_record(result, &value, "%out");
        emitter.instruction("ret void");
    } else if *result == Type::Unit {
        emitter.instruction("ret void");
    } else {
        let value = emitter.encode_host_scalar(result, &value);
        emitter.instruction(format!("ret {} {value}", abi_type(result)));
    }
    let result = if out_result(result) {
        "void".to_owned()
    } else {
        abi_type(result)
    };
    let name = match kind {
        WrapperKind::Export => format!("tz_{}", function.name),
        WrapperKind::Callback => format!("tz.callback.{}", function.qualified_name()),
    };
    let definition = emitter.auxiliary(&format!("{result} @{name}({})", parameters.join(", ")));
    match kind {
        WrapperKind::Export => definition.replacen("define internal ", "define ", 1),
        WrapperKind::Callback => definition,
    }
}

pub(crate) fn record_layout(ty: &Type, module: &CheckedModule) -> (usize, usize) {
    if let Some((_, size, align)) = c_layout_fields(ty, module) {
        return (size, align);
    }
    if let Type::FixedArray(element, _) = ty {
        let (size, alignment) = record_layout(element, module);
        return (size * fixed_length(ty), alignment);
    }
    let Type::Record(id, arguments) = ty else {
        return match ty {
            Type::Integer(64, _) | Type::Binary(64) => (8, 8),
            _ => (4, 4),
        };
    };
    let mut size = 0usize;
    let mut alignment = 1;
    for field in module.types().record_fields(*id, arguments) {
        let (field_size, field_alignment) = record_layout(&field, module);
        size = size.div_ceil(field_alignment) * field_alignment + field_size;
        alignment = alignment.max(field_alignment);
    }
    (size.max(1).div_ceil(alignment) * alignment, alignment)
}

impl FunctionEmitter<'_, '_> {
    pub(super) fn host_pointer(
        &mut self,
        pointer: &str,
        bytes: &str,
        alignment: usize,
        empty_null: bool,
    ) {
        let nonnull = self.value(format!("icmp ne ptr {pointer}, null"));
        let valid = if empty_null {
            let empty = self.value(format!("icmp eq i64 {bytes}, 0"));
            self.value(format!("or i1 {nonnull}, {empty}"))
        } else {
            nonnull
        };
        self.guard(&valid, TrapKind::BoundsCheck);
        let address = self.value(format!("ptrtoint ptr {pointer} to i64"));
        if alignment > 1 {
            let low = self.value(format!("and i64 {address}, {}", alignment - 1));
            let aligned = self.value(format!("icmp eq i64 {low}, 0"));
            self.guard(&aligned, TrapKind::BoundsCheck);
        }
        if self.globals.wasm {
            let pages = if self.globals.memory64 {
                self.value("call i64 @llvm.wasm.memory.size.i64(i32 0)")
            } else {
                let pages = self.value("call i32 @llvm.wasm.memory.size.i32(i32 0)");
                self.value(format!("zext i32 {pages} to i64"))
            };
            let memory = self.value(format!("shl i64 {pages}, 16"));
            let fits = self.value(format!("icmp ule i64 {address}, {memory}"));
            self.guard(&fits, TrapKind::BoundsCheck);
            let available = self.value(format!("sub i64 {memory}, {address}"));
            let fits = self.value(format!("icmp ule i64 {bytes}, {available}"));
            self.guard(&fits, TrapKind::BoundsCheck);
        }
    }

    pub(super) fn validate_host_utf8(&mut self, pointer: &str, length: &str) {
        let valid = self.value(format!(
            "call i1 @tz.string.validate_utf8(ptr {pointer}, i64 {length})"
        ));
        self.guard(&valid, TrapKind::Encoding);
    }

    /// W15: lowers a `Maybe<H>` / `Maybe<ref H>` / `Maybe<ref R>` SSA value to a C `ptr` (`None` -> `null`).
    pub(crate) fn nullable_to_c(&mut self, ty: &Type, value: &str) -> String {
        let types = self.module.types();
        match crate::abi::nullable(ty, &types).expect("checked nullable ABI type") {
            crate::abi::Nullable::Handle(_) => {
                if matches!(
                    self.union_layout(ty),
                    layout::UnionLayout::Niche {
                        niche: layout::Niche::NullPointer,
                        ..
                    }
                ) {
                    return value.to_owned();
                }
                let Type::Union(id, _) = ty else {
                    unreachable!()
                };
                let some_case = self.module.unions[*id]
                    .cases
                    .iter()
                    .position(|(name, payload)| name == "Some" && payload.is_some())
                    .expect("std::Maybe has Some");
                let tag = self.tag_value(ty, value);
                let is_some = self.value(format!("icmp eq i32 {tag}, {some_case}"));
                let slot = self.spill(ty, value);
                let payload_ptr = self.payload_pointer(ty, &slot);
                let handle = self.value(format!("load ptr, ptr {payload_ptr}"));
                self.value(format!("select i1 {is_some}, ptr {handle}, ptr null"))
            }
            crate::abi::Nullable::BorrowedHandle(_) => {
                let null_slot = self.fresh();
                self.allocas
                    .push(format!("{null_slot} = alloca ptr, align 8"));
                self.instruction(format!("store ptr null, ptr {null_slot}"));
                let is_null = self.value(format!("icmp eq ptr {value}, null"));
                let safe_slot =
                    self.value(format!("select i1 {is_null}, ptr {null_slot}, ptr {value}"));
                self.value(format!("load ptr, ptr {safe_slot}"))
            }
            crate::abi::Nullable::BorrowedZString(zstr)
            | crate::abi::Nullable::BorrowedZUtf8String(zstr) => {
                let nonnull = self.value(format!("icmp ne ptr {value}, null"));
                let from_none = self.block.clone();
                let some_block = self.label();
                let done_block = self.label();
                self.branch(&nonnull, &some_block, &done_block);
                self.begin(&some_block);
                let rec = self.value(format!("load {}, ptr {value}", self.ty(zstr)));
                let arr = self.value(format!("extractvalue {} {rec}, 0", self.ty(zstr)));
                let data = self.value(format!("extractvalue %tz.array {arr}, 0"));
                let from_some = self.block.clone();
                self.jump(&done_block);
                self.begin(&done_block);
                self.value(format!(
                    "phi ptr [ null, %{from_none} ], [ {data}, %{from_some} ]"
                ))
            }
            crate::abi::Nullable::OwnedZUtf8String(_) => {
                unreachable!("Maybe<ZUtf8String> is only an extern result")
            }
            crate::abi::Nullable::BorrowedRecord(record) => {
                let slot = self.host_result_slot(record);
                let nonnull = self.value(format!("icmp ne ptr {value}, null"));
                let from_none = self.block.clone();
                let copy_block = self.label();
                let done_block = self.label();
                self.branch(&nonnull, &copy_block, &done_block);
                self.begin(&copy_block);
                let loaded = self.value(format!("load {}, ptr {value}", self.ty(record)));
                self.write_host_record(record, &loaded, &slot);
                let from_some = self.block.clone();
                self.jump(&done_block);
                self.begin(&done_block);
                self.value(format!(
                    "phi ptr [ null, %{from_none} ], [ {slot}, %{from_some} ]"
                ))
            }
        }
    }

    /// W15: decodes a C `ptr` (`null` -> `None`) into a `Maybe<H>` or `Maybe<ref H>` SSA value.
    pub(crate) fn nullable_from_c(&mut self, ty: &Type, pointer: &str) -> String {
        let types = self.module.types();
        match crate::abi::nullable(ty, &types).expect("checked nullable ABI type") {
            crate::abi::Nullable::Handle(_) => {
                if matches!(
                    self.union_layout(ty),
                    layout::UnionLayout::Niche {
                        niche: layout::Niche::NullPointer,
                        ..
                    }
                ) {
                    return pointer.to_owned();
                }
                let Type::Union(id, _) = ty else {
                    unreachable!()
                };
                let some_case = self.module.unions[*id]
                    .cases
                    .iter()
                    .position(|(name, payload)| name == "Some" && payload.is_some())
                    .expect("std::Maybe has Some");
                let is_null = self.value(format!("icmp eq ptr {pointer}, null"));
                let none_val = self.nullary_value(ty, "None");
                let some_val =
                    self.construct_value(ty, some_case, Some((pointer.to_owned(), "ptr".into())));
                let llvm = self.ty(ty);
                self.value(format!(
                    "select i1 {is_null}, {llvm} {none_val}, {llvm} {some_val}"
                ))
            }
            crate::abi::Nullable::BorrowedHandle(handle) => {
                let slot = self.spill(handle, pointer);
                let is_null = self.value(format!("icmp eq ptr {pointer}, null"));
                self.value(format!("select i1 {is_null}, ptr null, ptr {slot}"))
            }
            crate::abi::Nullable::OwnedZUtf8String(zutf8_ty) => {
                let Type::Union(id, _) = ty else {
                    unreachable!()
                };
                let some_case = self.module.unions[*id]
                    .cases
                    .iter()
                    .position(|(name, payload)| name == "Some" && payload.is_some())
                    .expect("std::Maybe has Some");
                let is_nonnull = self.value(format!("icmp ne ptr {pointer}, null"));
                let none_block = self.label();
                let scan_entry = self.label();
                let scan_loop = self.label();
                let scan_step = self.label();
                let copy_block = self.label();
                let done_block = self.label();
                self.branch(&is_nonnull, &scan_entry, &none_block);

                self.begin(&none_block);
                let none_val = self.nullary_value(ty, "None");
                let from_none = self.block.clone();
                self.jump(&done_block);

                self.begin(&scan_entry);
                let entry_pred = self.block.clone();
                self.jump(&scan_loop);

                self.begin(&scan_loop);
                let idx_next = self.fresh();
                let idx = self.value(format!(
                    "phi i64 [ 0, %{entry_pred} ], [ {idx_next}, %{scan_step} ]"
                ));
                let ch_ptr = self.value(format!("getelementptr i8, ptr {pointer}, i64 {idx}"));
                let ch = self.value(format!("load i8, ptr {ch_ptr}, align 1"));
                let not_nul = self.value(format!("icmp ne i8 {ch}, 0"));
                self.branch(&not_nul, &scan_step, &copy_block);

                self.begin(&scan_step);
                self.instruction(format!("{idx_next} = add i64 {idx}, 1"));
                self.jump(&scan_loop);

                self.begin(&copy_block);
                self.validate_host_utf8(pointer, &idx);
                let cap = self.value(format!("add i64 {idx}, 1"));
                let buf = self.value(format!("call ptr @tz.alloc(i64 {cap})"));
                self.intrinsics.insert(
                    "declare void @llvm.memcpy.p0.p0.i64(ptr noalias writeonly, ptr noalias readonly, i64, i1 immarg)"
                        .into(),
                );
                self.instruction(format!(
                    "call void @llvm.memcpy.p0.p0.i64(ptr {buf}, ptr {pointer}, i64 {cap}, i1 false)"
                ));
                let arr = self.value(format!(
                    "insertvalue %tz.array zeroinitializer, ptr {buf}, 0"
                ));
                let arr = self.value(format!("insertvalue %tz.array {arr}, i64 {cap}, 1"));
                let rec_llvm = self.ty(zutf8_ty);
                let rec_val = self.value(format!(
                    "insertvalue {rec_llvm} zeroinitializer, %tz.array {arr}, 0"
                ));
                let some_val = self.construct_value(ty, some_case, Some((rec_val, rec_llvm)));
                let from_some = self.block.clone();
                self.jump(&done_block);

                self.begin(&done_block);
                let llvm = self.ty(ty);
                self.value(format!(
                    "phi {llvm} [ {none_val}, %{from_none} ], [ {some_val}, %{from_some} ]"
                ))
            }
            crate::abi::Nullable::BorrowedRecord(_)
            | crate::abi::Nullable::BorrowedZString(_)
            | crate::abi::Nullable::BorrowedZUtf8String(_) => {
                unreachable!(
                    "Maybe<ref R>, Maybe<ref ZString>, and Maybe<ref ZUtf8String> are only extern input parameters"
                )
            }
        }
    }

    fn decode_host_scalar(&mut self, ty: &Type, value: &str) -> String {
        match ty {
            Type::Bool => self.value(format!("icmp ne i32 {value}, 0")),
            Type::Integer(bits @ (8 | 16), _) => {
                self.value(format!("trunc i32 {value} to i{bits}"))
            }
            _ => value.into(),
        }
    }

    fn encode_host_scalar(&mut self, ty: &Type, value: &str) -> String {
        match ty {
            Type::Bool => self.value(format!("zext i1 {value} to i32")),
            Type::Integer(bits @ (8 | 16), signed) => self.value(format!(
                "{} i{bits} {value} to i32",
                if *signed { "sext" } else { "zext" }
            )),
            _ => value.into(),
        }
    }

    pub(super) fn read_host_record(&mut self, ty: &Type, pointer: &str) -> String {
        self.read_host_record_aligned(ty, pointer, None)
    }

    fn read_host_record_aligned(
        &mut self,
        ty: &Type,
        pointer: &str,
        align_cap: Option<usize>,
    ) -> String {
        let Type::Record(id, arguments) = ty else {
            unreachable!()
        };
        if let Some((cfields, _, _)) = c_layout_fields(ty, self.module) {
            let mut record = "zeroinitializer".to_owned();
            let fields = self.module.types().record_fields(*id, arguments);
            let rname = record_name(ty, self.module);
            for (index, (field, cfield)) in fields.iter().zip(&cfields).enumerate() {
                let leaf_align = align_cap.map_or(cfield.align, |cap| cap.min(cfield.align));
                let slot = self.value(format!(
                    "getelementptr inbounds %{rname}, ptr {pointer}, i32 0, i32 {}",
                    cfield.llvm_index
                ));
                let value = if matches!(field, Type::Record(..)) {
                    self.read_host_record_aligned(field, &slot, Some(leaf_align))
                } else if let Type::FixedArray(element, _) = field {
                    let array = self.slot(field);
                    let elem_llvm = c_layout_llvm_field(element, self.module);
                    let host = format!("[{} x {elem_llvm}]", fixed_length(field));
                    self.array_loop(&fixed_length(field).to_string(), |emitter, idx| {
                        let from = emitter.value(format!(
                            "getelementptr inbounds {host}, ptr {slot}, i64 0, i64 {idx}"
                        ));
                        let loaded = emitter
                            .value(format!("load {elem_llvm}, ptr {from}, align {leaf_align}"));
                        let to = emitter.fixed_pointer(field, &array, idx);
                        emitter.instruction(format!("store {elem_llvm} {loaded}, ptr {to}"));
                    });
                    self.value(format!("load {}, ptr {array}", self.ty(field)))
                } else if matches!(field, Type::Handle(..)) {
                    let loaded = self.value(format!("load ptr, ptr {slot}, align {leaf_align}"));
                    if !self.globals.wasm {
                        let valid = self.value(format!("icmp ne ptr {loaded}, null"));
                        self.guard(&valid, TrapKind::NullHandle);
                    }
                    loaded
                } else if crate::abi::nullable(field, &self.module.types()).is_some() {
                    let loaded = self.value(format!("load ptr, ptr {slot}, align {leaf_align}"));
                    self.nullable_from_c(field, &loaded)
                } else {
                    let elem_llvm = c_layout_llvm_field(field, self.module);
                    self.value(format!("load {elem_llvm}, ptr {slot}, align {leaf_align}"))
                };
                let internal = self.field_slot(ty, index);
                record = self.value(format!(
                    "insertvalue {} {record}, {} {value}, {internal}",
                    self.ty(ty),
                    self.ty(field)
                ));
            }
            return record;
        }
        let mut record = "zeroinitializer".to_owned();
        for (index, field) in self
            .module
            .types()
            .record_fields(*id, arguments)
            .iter()
            .enumerate()
        {
            let slot = self.value(format!(
                "getelementptr inbounds %{}, ptr {pointer}, i32 0, i32 {index}",
                record_name(ty, self.module)
            ));
            let value = if matches!(field, Type::Record(..)) {
                self.read_host_record(field, &slot)
            } else if let Type::FixedArray(element, _) = field {
                // Each C array element converts like a scalar field.
                let array = self.slot(field);
                let host = format!("[{} x {}]", fixed_length(field), abi_type(element));
                self.array_loop(&fixed_length(field).to_string(), |emitter, index| {
                    let from = emitter.value(format!(
                        "getelementptr inbounds {host}, ptr {slot}, i64 0, i64 {index}"
                    ));
                    let value = emitter.value(format!("load {}, ptr {from}", abi_type(element)));
                    let value = emitter.decode_host_scalar(element, &value);
                    let to = emitter.fixed_pointer(field, &array, index);
                    emitter.instruction(format!("store {} {value}, ptr {to}", emitter.ty(element)));
                });
                self.value(format!("load {}, ptr {array}", self.ty(field)))
            } else {
                let value = self.value(format!("load {}, ptr {slot}", abi_type(field)));
                self.decode_host_scalar(field, &value)
            };
            // The C struct keeps the declared order; the record may not.
            let internal = self.field_slot(ty, index);
            record = self.value(format!(
                "insertvalue {} {record}, {} {value}, {internal}",
                self.ty(ty),
                self.ty(field)
            ));
        }
        record
    }

    pub(super) fn write_host_record(&mut self, ty: &Type, value: &str, pointer: &str) {
        self.write_host_record_aligned(ty, value, pointer, None);
    }

    fn write_host_record_aligned(
        &mut self,
        ty: &Type,
        value: &str,
        pointer: &str,
        align_cap: Option<usize>,
    ) {
        let Type::Record(id, arguments) = ty else {
            unreachable!()
        };
        if let Some((cfields, _, _)) = c_layout_fields(ty, self.module) {
            let fields = self.module.types().record_fields(*id, arguments);
            let rname = record_name(ty, self.module);
            for (index, (field, cfield)) in fields.iter().zip(&cfields).enumerate() {
                let leaf_align = align_cap.map_or(cfield.align, |cap| cap.min(cfield.align));
                let slot = self.value(format!(
                    "getelementptr inbounds %{rname}, ptr {pointer}, i32 0, i32 {}",
                    cfield.llvm_index
                ));
                let internal = self.field_slot(ty, index);
                let extracted =
                    self.value(format!("extractvalue {} {value}, {internal}", self.ty(ty)));
                if matches!(field, Type::Record(..)) {
                    self.write_host_record_aligned(field, &extracted, &slot, Some(leaf_align));
                } else if let Type::FixedArray(element, _) = field {
                    let array = self.spill(field, &extracted);
                    let elem_llvm = c_layout_llvm_field(element, self.module);
                    let host = format!("[{} x {elem_llvm}]", fixed_length(field));
                    self.array_loop(&fixed_length(field).to_string(), |emitter, idx| {
                        let from = emitter.fixed_pointer(field, &array, idx);
                        let loaded = emitter.value(format!("load {elem_llvm}, ptr {from}"));
                        let to = emitter.value(format!(
                            "getelementptr inbounds {host}, ptr {slot}, i64 0, i64 {idx}"
                        ));
                        emitter.instruction(format!(
                            "store {elem_llvm} {loaded}, ptr {to}, align {leaf_align}"
                        ));
                    });
                } else if matches!(field, Type::Handle(..) | Type::FnPtr(_)) {
                    self.instruction(format!(
                        "store ptr {extracted}, ptr {slot}, align {leaf_align}"
                    ));
                } else if let Type::Reference(inner, false) = field
                    && crate::abi::any_zstring(inner, &self.module.types())
                {
                    let rec = self.value(format!("load {}, ptr {extracted}", self.ty(inner)));
                    let arr = self.value(format!("extractvalue {} {rec}, 0", self.ty(inner)));
                    let data = self.value(format!("extractvalue %tz.array {arr}, 0"));
                    self.instruction(format!("store ptr {data}, ptr {slot}, align {leaf_align}"));
                } else if crate::abi::nullable(field, &self.module.types()).is_some() {
                    let ptr_val = self.nullable_to_c(field, &extracted);
                    self.instruction(format!(
                        "store ptr {ptr_val}, ptr {slot}, align {leaf_align}"
                    ));
                } else {
                    let elem_llvm = c_layout_llvm_field(field, self.module);
                    self.instruction(format!(
                        "store {elem_llvm} {extracted}, ptr {slot}, align {leaf_align}"
                    ));
                }
            }
            return;
        }
        let fields = self.module.types().record_fields(*id, arguments);
        if fields.is_empty() {
            self.instruction(format!("store i8 0, ptr {pointer}"));
        }
        for (index, field) in fields.iter().enumerate() {
            let slot = self.value(format!(
                "getelementptr inbounds %{}, ptr {pointer}, i32 0, i32 {index}",
                record_name(ty, self.module)
            ));
            let internal = self.field_slot(ty, index);
            let extracted = self.value(format!("extractvalue {} {value}, {internal}", self.ty(ty)));
            if matches!(field, Type::Record(..)) {
                self.write_host_record(field, &extracted, &slot);
            } else if let Type::FixedArray(element, _) = field {
                let array = self.spill(field, &extracted);
                let host = format!("[{} x {}]", fixed_length(field), abi_type(element));
                self.array_loop(&fixed_length(field).to_string(), |emitter, index| {
                    let from = emitter.fixed_pointer(field, &array, index);
                    let value = emitter.value(format!("load {}, ptr {from}", emitter.ty(element)));
                    let value = emitter.encode_host_scalar(element, &value);
                    let to = emitter.value(format!(
                        "getelementptr inbounds {host}, ptr {slot}, i64 0, i64 {index}"
                    ));
                    emitter.instruction(format!("store {} {value}, ptr {to}", abi_type(element)));
                });
            } else {
                let converted = self.encode_host_scalar(field, &extracted);
                self.instruction(format!("store {} {converted}, ptr {slot}", abi_type(field)));
            }
        }
    }
}
