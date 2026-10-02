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

fn host_function(function: &CheckedFunction) -> bool {
    function.exported || matches!(function.body.kind, TypedExprKind::HostCall(..))
}

pub(crate) fn uses_host_abi(module: &CheckedModule) -> bool {
    module.functions.iter().any(|function| {
        extended(function)
            || (matches!(function.body.kind, TypedExprKind::HostCall(..))
                && (function.signature.parameters.iter().any(|ty| {
                    !abi_scalar(ty) && *ty != Type::Unit && !matches!(ty, Type::Function(..))
                }) || out_result(&function.signature.result)))
    })
}

/// The C typedef name of an `extern type`, spelled like `record_name` so that
/// module boundaries cannot collide (`A_B.C` and `A.B_C` differ).
pub(super) fn handle_c_name(name: &str) -> String {
    let mut text = String::from("tz_handle");
    for segment in name.split('.') {
        let _ = write!(text, "_{}{segment}", segment.len());
    }
    text
}

/// Typedefs for the extern handles that the header's prototypes mention, in name order.
pub(super) fn handle_typedefs(module: &CheckedModule) -> String {
    fn visit(ty: &Type, names: &mut BTreeSet<String>) {
        match ty {
            Type::Handle(name) => {
                names.insert(name.to_string());
            }
            Type::Reference(inner, _) => visit(inner, names),
            Type::Function(parameters, result) => {
                parameters.iter().for_each(|ty| visit(ty, names));
                visit(result, names);
            }
            _ => {}
        }
    }
    let mut names = BTreeSet::new();
    for function in module.functions.iter().filter(|function| {
        // An explicit import has no prototype in the header.
        host_function(function)
            && !matches!(&function.body.kind, TypedExprKind::HostCall(import, _) if import.explicit)
    }) {
        for ty in function
            .signature
            .parameters
            .iter()
            .chain([&function.signature.result])
        {
            visit(ty, &mut names);
        }
    }
    names
        .iter()
        .map(|name| {
            let c_name = handle_c_name(name);
            format!("typedef struct {c_name}_s *{c_name};\n")
        })
        .collect()
}

pub(super) fn record_name(ty: &Type, module: &CheckedModule) -> String {
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

fn record_types(module: &CheckedModule) -> Vec<Type> {
    fn visit(ty: &Type, module: &CheckedModule, seen: &mut BTreeSet<Type>, result: &mut Vec<Type>) {
        let ty = if let Type::Reference(inner, false) = ty {
            inner
        } else {
            ty
        };
        if let Type::Record(id, arguments) = ty {
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
    for function in module
        .functions
        .iter()
        .filter(|function| host_function(function))
    {
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

pub(super) fn type_definitions(module: &CheckedModule) -> String {
    if !uses_host_abi(module) {
        return String::new();
    }
    let mut output = String::new();
    for ty in record_types(module) {
        let Type::Record(id, arguments) = &ty else {
            unreachable!()
        };
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
    if matches!(ty, Type::Record(..)) {
        format!("%{}", record_name(ty, module))
    } else {
        abi_type(ty)
    }
}

pub(super) fn header_types(module: &CheckedModule) -> String {
    if !uses_host_abi(module) {
        return String::new();
    }
    let mut buffers = BTreeSet::new();
    for function in module
        .functions
        .iter()
        .filter(|function| host_function(function))
    {
        for ty in function
            .signature
            .parameters
            .iter()
            .chain([&function.signature.result])
        {
            let ty = if let Type::Reference(inner, false) = ty {
                inner
            } else {
                ty
            };
            if let Some(buffer) = Buffer::of(ty) {
                buffers.insert(buffer);
            }
        }
    }
    let mut output = String::new();
    for buffer in buffers {
        let _ = writeln!(
            output,
            "typedef struct {{ const {} *ptr; int64_t len; }} tsuzuri_{}_slice;\ntypedef struct {{ {} *ptr; int64_t len; }} tsuzuri_{}_buffer;",
            buffer.c_element(),
            buffer.name(),
            buffer.c_element(),
            buffer.name()
        );
    }
    for ty in record_types(module) {
        let Type::Record(id, arguments) = &ty else {
            unreachable!()
        };
        output.push_str("typedef struct {\n");
        if module.records[*id].fields.is_empty() {
            output.push_str("    uint8_t tz_empty;\n");
        }
        for ((name, _), field) in module.records[*id]
            .fields
            .iter()
            .zip(module.types().record_fields(*id, arguments))
        {
            let ctype = if matches!(field, Type::Record(..)) {
                record_name(&field, module)
            } else {
                c_type(&field)
            };
            let _ = writeln!(output, "    {ctype} {};", field_name(name));
        }
        let _ = writeln!(output, "}} {};", record_name(&ty, module));
    }
    output.push_str("\nvoid *tsuzuri_alloc(int64_t size);\nvoid tsuzuri_free(void *ptr);\n\n");
    output
}

pub(super) fn c_parameters(function: &CheckedFunction, module: &CheckedModule) -> String {
    let mut parameters = Vec::new();
    let result = &function.signature.result;
    if let Some(buffer) = Buffer::of(result) {
        parameters.push(format!("tsuzuri_{}_buffer *out", buffer.name()));
    } else if matches!(result, Type::Record(..)) {
        parameters.push(format!("{} *out", record_name(result, module)));
    }
    for (index, ty) in function.signature.parameters.iter().enumerate() {
        if *ty == Type::Unit {
            continue;
        }
        if let Type::Function(inputs, output) = ty {
            // A callback is a C function pointer; a lone `unit` input is `void`.
            let inputs = if matches!(inputs.as_slice(), [Type::Unit]) {
                "void".to_owned()
            } else {
                inputs.iter().map(c_type).collect::<Vec<_>>().join(", ")
            };
            parameters.push(format!("{} (*arg{index})({inputs})", c_type(output)));
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

pub(super) fn allocator() -> &'static str {
    "\ndefine weak ptr @tsuzuri_alloc(i64 %size) nounwind {\nentry:\n  %valid = icmp sge i64 %size, 0\n  br i1 %valid, label %allocate, label %bad\nbad:\n  call void @llvm.trap()\n  unreachable\nallocate:\n  %empty = icmp eq i64 %size, 0\n  %bytes = select i1 %empty, i64 1, i64 %size\n  %value = call ptr @tz.alloc(i64 %bytes)\n  ret ptr %value\n}\ndefine weak void @tsuzuri_free(ptr %value) nounwind {\nentry:\n  call void @tz.free(ptr %value)\n  ret void\n}\n"
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
        } else if let Type::Reference(handle, false) = ty
            && matches!(handle.as_ref(), Type::Handle(_))
        {
            // The host passes the handle itself; a borrow needs it in a slot.
            parameters.push(format!("ptr %arg{index}"));
            let slot = emitter.spill(handle, &format!("%arg{index}"));
            values.push(format!("ptr {slot}"));
        } else {
            parameters.push(format!("{} %arg{index}", abi_type(ty)));
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

fn record_layout(ty: &Type, module: &CheckedModule) -> (usize, usize) {
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
        let index = self.spill(&Type::I64, "0");
        let check = self.label();
        let body = self.label();
        let done = self.label();
        self.jump(&check);
        self.begin(&check);
        let offset = self.value(format!("load i64, ptr {index}"));
        let more = self.value(format!("icmp ult i64 {offset}, {length}"));
        self.branch(&more, &body, &done);
        self.begin(&body);
        let decoded = self.value(format!("call {{ i32, i64 }} @tz.string.try_decode_utf8(ptr {pointer}, i64 {length}, i64 {offset}, i1 false)"));
        let scalar = self.value(format!("extractvalue {{ i32, i64 }} {decoded}, 0"));
        let valid = self.value(format!("icmp sge i32 {scalar}, 0"));
        self.guard(&valid, TrapKind::Encoding);
        let next = self.value(format!("extractvalue {{ i32, i64 }} {decoded}, 1"));
        self.instruction(format!("store i64 {next}, ptr {index}"));
        self.jump(&check);
        self.begin(&done);
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
        let Type::Record(id, arguments) = ty else {
            unreachable!()
        };
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
            } else {
                let value = self.value(format!("load {}, ptr {slot}", abi_type(field)));
                self.decode_host_scalar(field, &value)
            };
            record = self.value(format!(
                "insertvalue {} {record}, {} {value}, {index}",
                self.ty(ty),
                self.ty(field)
            ));
        }
        record
    }

    pub(super) fn write_host_record(&mut self, ty: &Type, value: &str, pointer: &str) {
        let Type::Record(id, arguments) = ty else {
            unreachable!()
        };
        let fields = self.module.types().record_fields(*id, arguments);
        if fields.is_empty() {
            self.instruction(format!("store i8 0, ptr {pointer}"));
        }
        for (index, field) in fields.iter().enumerate() {
            let slot = self.value(format!(
                "getelementptr inbounds %{}, ptr {pointer}, i32 0, i32 {index}",
                record_name(ty, self.module)
            ));
            let extracted = self.value(format!("extractvalue {} {value}, {index}", self.ty(ty)));
            if matches!(field, Type::Record(..)) {
                self.write_host_record(field, &extracted, &slot);
            } else {
                let converted = self.encode_host_scalar(field, &extracted);
                self.instruction(format!("store {} {converted}, ptr {slot}", abi_type(field)));
            }
        }
    }
}
