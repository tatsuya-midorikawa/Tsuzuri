//! The bindings of a native shared library (`--emit shared`, E13 Phase 2): C# `LibraryImport`,
//! Python `ctypes` and a C++20 wrapper over the C header. Each reads the same exports, records and
//! handles as the JavaScript glue, in the C ABI of `--emit header`.

use std::collections::BTreeSet;
use std::fmt::Write as _;

use super::{banner, exports, fields, handles, records};
use crate::abi::Buffer;
use crate::check::{CheckedFunction, CheckedModule, Type};
use crate::llvm::fixed_length;
use crate::llvm::host_abi::{handle_c_name, record_name};
use crate::trap::TrapKind;

/// A scalar of the C ABI.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Scalar {
    I8,
    I16,
    I32,
    I64,
    U8,
    U16,
    U32,
    U64,
    F32,
    F64,
    Bool,
}

impl Scalar {
    fn of(ty: &Type) -> Option<Self> {
        Some(match ty {
            Type::Integer(8, true) => Self::I8,
            Type::Integer(16, true) => Self::I16,
            Type::Integer(32, true) => Self::I32,
            Type::Integer(64, true) => Self::I64,
            Type::Integer(8, false) => Self::U8,
            Type::Integer(16, false) => Self::U16,
            Type::Integer(32, false) => Self::U32,
            Type::Integer(64, false) => Self::U64,
            Type::Binary(32) => Self::F32,
            Type::Binary(64) => Self::F64,
            Type::Bool => Self::Bool,
            _ => return None,
        })
    }

    /// The Tsuzuri spelling, for messages.
    fn tsuzuri(self) -> &'static str {
        match self {
            Self::I8 => "i8",
            Self::I16 => "i16",
            Self::I32 => "i32",
            Self::I64 => "i64",
            Self::U8 => "i8u",
            Self::U16 => "i16u",
            Self::U32 => "i32u",
            Self::U64 => "i64u",
            Self::F32 => "f32",
            Self::F64 => "f64",
            Self::Bool => "bool",
        }
    }

    /// The inclusive range of an integer, as Python and C# literals.
    fn range(self) -> Option<(&'static str, &'static str)> {
        match self {
            Self::I8 => Some(("-128", "127")),
            Self::I16 => Some(("-32768", "32767")),
            Self::I32 => Some(("-2147483648", "2147483647")),
            Self::I64 => Some(("-9223372036854775808", "9223372036854775807")),
            Self::U8 => Some(("0", "255")),
            Self::U16 => Some(("0", "65535")),
            Self::U32 => Some(("0", "4294967295")),
            Self::U64 => Some(("0", "18446744073709551615")),
            _ => None,
        }
    }

    /// Whether the C ABI passes the value in a wider 32-bit integer (bool and 8/16-bit integers).
    fn normalized(self) -> bool {
        matches!(
            self,
            Self::I8 | Self::I16 | Self::U8 | Self::U16 | Self::Bool
        )
    }

    fn csharp(self) -> &'static str {
        match self {
            Self::I8 => "sbyte",
            Self::I16 => "short",
            Self::I32 => "int",
            Self::I64 => "long",
            Self::U8 => "byte",
            Self::U16 => "ushort",
            Self::U32 => "uint",
            Self::U64 => "ulong",
            Self::F32 => "float",
            Self::F64 => "double",
            Self::Bool => "bool",
        }
    }

    /// The C# type of the C ABI value.
    fn csharp_abi(self) -> &'static str {
        match self {
            Self::I8 | Self::I16 | Self::I32 | Self::Bool => "int",
            Self::U8 | Self::U16 | Self::U32 => "uint",
            other => other.csharp(),
        }
    }

    fn ctypes(self) -> &'static str {
        match self {
            Self::I8 | Self::I16 | Self::I32 | Self::Bool => "_ctypes.c_int32",
            Self::U8 | Self::U16 | Self::U32 => "_ctypes.c_uint32",
            Self::I64 => "_ctypes.c_int64",
            Self::U64 => "_ctypes.c_uint64",
            Self::F32 => "_ctypes.c_float",
            Self::F64 => "_ctypes.c_double",
        }
    }

    fn cpp(self) -> &'static str {
        match self {
            Self::I8 => "std::int8_t",
            Self::I16 => "std::int16_t",
            Self::I32 => "std::int32_t",
            Self::I64 => "std::int64_t",
            Self::U8 => "std::uint8_t",
            Self::U16 => "std::uint16_t",
            Self::U32 => "std::uint32_t",
            Self::U64 => "std::uint64_t",
            Self::F32 => "float",
            Self::F64 => "double",
            Self::Bool => "bool",
        }
    }
}

/// How an ABI type crosses the C boundary of a shared library.
enum Shape {
    Scalar(Scalar),
    Unit,
    /// An `extern type` handle, by value or shared borrow: the pointer itself.
    Handle(String),
    /// A borrowed input buffer: pointer and length.
    Slice(Buffer),
    /// An owned result buffer, written to the leading `out` descriptor.
    Owned(Buffer),
    /// A scalar record, by value or shared borrow: a pointer to the normalized C struct.
    Record(String),
}

fn shape(ty: &Type, module: &CheckedModule) -> Shape {
    if let Some(scalar) = Scalar::of(ty) {
        return Shape::Scalar(scalar);
    }
    match ty {
        Type::Unit => Shape::Unit,
        Type::Handle(name) => Shape::Handle(handle_c_name(name)),
        Type::Record(..) => Shape::Record(record_name(ty, module)),
        Type::Reference(inner, false) => match Buffer::of(inner) {
            Some(buffer) => Shape::Slice(buffer),
            None => shape(inner, module),
        },
        _ => Shape::Owned(Buffer::of(ty).expect("ABI type checked")),
    }
}

/// The trap kinds by their `repr(u32)` value, as `tsuzuri_trap_info.kind` reports them.
fn trap_kinds() -> Vec<&'static str> {
    TrapKind::ALL
        .iter()
        .map(|kind| kind.description())
        .collect()
}

/// The records in name order, each after the records of its fields, so that a definition never
/// names a later one.
fn ordered_records(module: &CheckedModule) -> Vec<(String, Type)> {
    fn visit(
        name: &str,
        ty: &Type,
        module: &CheckedModule,
        done: &mut BTreeSet<String>,
        order: &mut Vec<(String, Type)>,
    ) {
        if !done.insert(name.to_owned()) {
            return;
        }
        for (_, _, field) in fields(ty, module) {
            if matches!(field, Type::Record(..)) {
                visit(&record_name(&field, module), &field, module, done, order);
            }
        }
        order.push((name.to_owned(), ty.clone()));
    }
    let mut done = BTreeSet::new();
    let mut order = Vec::new();
    for (name, ty) in records(module) {
        visit(&name, &ty, module, &mut done, &mut order);
    }
    order
}

/// `name`, or `name` with `suffix` repeated until it is not in `taken`.
fn unique(name: String, taken: &BTreeSet<String>, suffix: &str) -> String {
    let mut name = name;
    while taken.contains(&name) {
        name.push_str(suffix);
    }
    name
}

/// The stem as an identifier: other characters become `_`, and a leading digit gets one.
fn identifier(stem: &str) -> String {
    let mut text: String = stem
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '_' {
                character
            } else {
                '_'
            }
        })
        .collect();
    if text.is_empty() || text.starts_with(|character: char| character.is_ascii_digit()) {
        text.insert(0, '_');
    }
    text
}

const CSHARP_KEYWORDS: &[&str] = &[
    "abstract",
    "as",
    "base",
    "bool",
    "break",
    "byte",
    "case",
    "catch",
    "char",
    "checked",
    "class",
    "const",
    "continue",
    "decimal",
    "default",
    "delegate",
    "do",
    "double",
    "else",
    "enum",
    "event",
    "explicit",
    "extern",
    "false",
    "finally",
    "fixed",
    "float",
    "for",
    "foreach",
    "goto",
    "if",
    "implicit",
    "in",
    "int",
    "interface",
    "internal",
    "is",
    "lock",
    "long",
    "namespace",
    "new",
    "null",
    "object",
    "operator",
    "out",
    "override",
    "params",
    "private",
    "protected",
    "public",
    "readonly",
    "ref",
    "return",
    "sbyte",
    "sealed",
    "short",
    "sizeof",
    "stackalloc",
    "static",
    "string",
    "struct",
    "switch",
    "this",
    "throw",
    "true",
    "try",
    "typeof",
    "uint",
    "ulong",
    "unchecked",
    "unsafe",
    "ushort",
    "using",
    "virtual",
    "void",
    "volatile",
    "while",
];

/// A Tsuzuri name as a C# identifier: keywords take the verbatim prefix `@`.
fn csharp_name(name: &str) -> String {
    if CSHARP_KEYWORDS.contains(&name) {
        format!("@{name}")
    } else {
        name.to_owned()
    }
}

fn csharp_owned(buffer: Buffer) -> &'static str {
    match buffer {
        Buffer::I64 => "OwnedBuffer<long>",
        Buffer::F64 => "OwnedBuffer<double>",
        Buffer::UByte => "OwnedBuffer<byte>",
        Buffer::String => "OwnedString",
        Buffer::Utf8String => "OwnedUtf8String",
    }
}

fn csharp_element(buffer: Buffer) -> &'static str {
    match buffer {
        Buffer::I64 => "long",
        Buffer::F64 => "double",
        Buffer::UByte | Buffer::Utf8String => "byte",
        Buffer::String => "char",
    }
}

/// The C# struct that holds a C array field `T name[N]`, with its ABI element type.
fn csharp_fixed(ty: &Type) -> (String, &'static str, usize) {
    let Type::FixedArray(element, _) = ty else {
        unreachable!("fixed-length array field")
    };
    let element = Scalar::of(element).expect("scalar elements").csharp_abi();
    let length = fixed_length(ty);
    let title = match element {
        "int" => "Int32",
        "uint" => "UInt32",
        "long" => "Int64",
        "ulong" => "UInt64",
        "float" => "Single",
        _ => "Double",
    };
    (format!("Fixed{length}_{title}"), element, length)
}

const CSHARP_SUPPORT: &str = r#"    [StructLayout(LayoutKind.Sequential)]
    internal struct Descriptor
    {
        public nint Pointer;
        public long Length;
    }

    /// <summary>An owned result of the library. <see cref="SafeHandle.Dispose()"/> or the finalizer
    /// passes its memory to <c>tsuzuri_free</c>.</summary>
    public class OwnedBuffer<T> : SafeHandle where T : unmanaged
    {
        private int length;

        internal OwnedBuffer() : base(IntPtr.Zero, ownsHandle: true)
        {
        }

        internal void Own(Descriptor descriptor)
        {
            SetHandle(descriptor.Pointer);
            length = checked((int)descriptor.Length);
        }

        public override bool IsInvalid => handle == IntPtr.Zero;

        /// <summary>The number of elements.</summary>
        public int Length => length;

        /// <summary>The elements, valid until the buffer is disposed. Keep the buffer alive while
        /// the span is in use (for example with <c>using</c>): the finalizer of an unreachable
        /// buffer frees the memory under the span. <see cref="ToArray"/> copies safely.</summary>
        public ReadOnlySpan<T> Span
        {
            get
            {
                ObjectDisposedException.ThrowIf(IsClosed, this);
                return Elements;
            }
        }

        internal ReadOnlySpan<T> Elements => length == 0 ? default : new ReadOnlySpan<T>((void*)handle, length);

        /// <summary>Reads the elements while neither <see cref="SafeHandle.Dispose()"/> nor the
        /// finalizer can free them.</summary>
        internal TResult Read<TResult>(Func<OwnedBuffer<T>, TResult> read)
        {
            bool added = false;
            try
            {
                DangerousAddRef(ref added);
                return read(this);
            }
            finally
            {
                if (added)
                {
                    DangerousRelease();
                }
            }
        }

        /// <summary>A managed copy of the elements.</summary>
        public T[] ToArray() => Read(static buffer => buffer.Elements.ToArray());

        protected override bool ReleaseHandle()
        {
            Library.tsuzuri_free(handle);
            return true;
        }
    }

    /// <summary>An owned <c>string</c> result: UTF-16 code units, lone surrogates included.</summary>
    public sealed class OwnedString : OwnedBuffer<char>
    {
        internal OwnedString()
        {
        }

        public override string ToString() => Read(static buffer => new string(buffer.Elements));
    }

    /// <summary>An owned <c>utf8string</c> result: valid UTF-8 bytes.</summary>
    public sealed class OwnedUtf8String : OwnedBuffer<byte>
    {
        internal OwnedUtf8String()
        {
        }

        public override string ToString() => Read(static buffer => System.Text.Encoding.UTF8.GetString(buffer.Elements));
    }

"#;

/// The C# bindings of `--emit bindings-cs`: a static class whose methods call the library `name`
/// through `LibraryImport`. With `trap_return` they call `tsuzuri_try_<name>` and throw
/// `TsuzuriTrapException` (`--trap-mode return`).
pub fn csharp(module: &CheckedModule, name: &str, trap_return: bool) -> String {
    let mut class = identifier(name)
        .split('_')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut characters = part.chars();
            characters
                .next()
                .map(|first| first.to_ascii_uppercase().to_string() + characters.as_str())
                .unwrap_or_default()
        })
        .collect::<String>();
    if class.is_empty() || class.starts_with(|character: char| character.is_ascii_digit()) {
        class.insert_str(0, "Library");
    }
    // The nested class of the C entry points is `Library`; a member cannot share its type's name.
    if class == "Library" {
        class.push_str("Bindings");
    }
    let records = records(module);
    let handles = handles(module);
    let owned = exports(module)
        .iter()
        .any(|function| Buffer::of(&function.signature.result).is_some());
    let mut fixed = BTreeSet::new();
    let mut taken: BTreeSet<String> = [
        "LibraryName",
        "Library",
        "Descriptor",
        "OwnedBuffer",
        "OwnedString",
        "OwnedUtf8String",
        "TrapInfo",
        "TsuzuriTrapException",
        "Check",
    ]
    .into_iter()
    .map(str::to_owned)
    .chain(records.keys().cloned())
    .chain(handles.iter().cloned())
    .collect();
    let mut output = banner("//");
    output.push_str("// <auto-generated/>\n#nullable enable\nusing System;\nusing System.Runtime.CompilerServices;\nusing System.Runtime.InteropServices;\n\nnamespace Tsuzuri.Bindings;\n\n");
    let _ = write!(
        output,
        "/// <summary>The exports of the Tsuzuri shared library <c>{name}</c>, built from the same sources with\n/// <c>tsuzuri build --emit shared{}</c>.</summary>\npublic static unsafe partial class {class}\n{{\n    /// <summary>The library that <see cref=\"LibraryImportAttribute\"/> loads: the runtime probes\n    /// {name}.dll, lib{name}.so, lib{name}.dylib and {name}.dylib. Register a\n    /// <see cref=\"NativeLibrary.SetDllImportResolver\"/> to load it from another path.</summary>\n    public const string LibraryName = \"{name}\";\n\n",
        if trap_return {
            " --trap-mode return"
        } else {
            ""
        }
    );
    // Records, in the normalized C layout of the header.
    for (record, ty) in &records {
        let fields = fields(ty, module);
        let names: BTreeSet<String> = fields.iter().map(|(field, ..)| field.clone()).collect();
        let mut backing = names.clone();
        let mut storage = Vec::new();
        let mut accessors = Vec::new();
        for (field, _, field_type) in &fields {
            let public = csharp_name(field);
            if let Some(scalar) = Scalar::of(field_type) {
                if scalar.normalized() {
                    let store = unique(format!("{field}__abi"), &backing, "_");
                    backing.insert(store.clone());
                    storage.push(format!("private {} {store};", scalar.csharp_abi()));
                    let (get, set) = match scalar {
                        Scalar::Bool => {
                            (format!("{store} != 0"), format!("{store} = value ? 1 : 0"))
                        }
                        _ => (
                            format!("({}){store}", scalar.csharp()),
                            format!("{store} = value"),
                        ),
                    };
                    accessors.push(format!(
                        "public {} {public} {{ readonly get => {get}; set => {set}; }}",
                        scalar.csharp()
                    ));
                } else {
                    storage.push(format!("public {} {public};", scalar.csharp()));
                }
            } else if matches!(field_type, Type::FixedArray(..)) {
                let (array, element, length) = csharp_fixed(field_type);
                fixed.insert((array.clone(), element, length));
                storage.push(format!("public {array} {public};"));
            } else {
                storage.push(format!(
                    "public {} {public};",
                    record_name(field_type, module)
                ));
            }
        }
        if fields.is_empty() {
            storage.push(format!(
                "private byte {};",
                unique("empty__abi".into(), &backing, "_")
            ));
        }
        let _ = writeln!(
            output,
            "    /// <summary>The Tsuzuri record <c>{}</c> in the C layout of the header: bool and 8/16-bit\n    /// integers are stored as 32-bit values.</summary>\n    [StructLayout(LayoutKind.Sequential)]\n    public struct {record}\n    {{",
            xml_text(&ty.display(&module.types()))
        );
        for line in storage.iter().chain(&accessors) {
            let _ = writeln!(output, "        {line}");
        }
        output.push_str("    }\n\n");
    }
    for (array, element, length) in &fixed {
        let _ = writeln!(
            output,
            "    /// <summary>A C array field of {length} elements, in its 32/64-bit ABI type.</summary>\n    [InlineArray({length})]\n    public struct {array}\n    {{\n        private {element} element0;\n    }}\n"
        );
        taken.insert(array.clone());
    }
    for handle in &handles {
        let _ = writeln!(
            output,
            "    /// <summary>An <c>extern type</c> handle: a pointer that the host owns.</summary>\n    public readonly record struct {handle}(nint Value);\n"
        );
    }
    if owned {
        output.push_str(CSHARP_SUPPORT);
    }
    if trap_return {
        let kinds: Vec<_> = trap_kinds()
            .iter()
            .map(|kind| format!("\"{kind}\""))
            .collect();
        let _ = writeln!(
            output,
            "    [StructLayout(LayoutKind.Sequential)]\n    internal struct TrapInfo\n    {{\n        public uint Site;\n        public uint Kind;\n    }}\n\n    /// <summary>A trap inside the library, which <c>--trap-mode return</c> reports instead of ending the\n    /// process. <see cref=\"Site\"/> is an id of the library's <c>.trap.json</c>.</summary>\n    public sealed class TsuzuriTrapException : Exception\n    {{\n        private static readonly string[] Kinds = [{}];\n\n        internal TsuzuriTrapException(uint site, uint kind)\n            : base($\"trap: {{(kind < Kinds.Length ? Kinds[kind] : \"unknown trap\")}} (site {{site}})\")\n        {{\n            Site = site;\n            Kind = kind;\n        }}\n\n        public uint Site {{ get; }}\n\n        public uint Kind {{ get; }}\n\n        public string KindName => Kind < Kinds.Length ? Kinds[Kind] : \"unknown trap\";\n    }}\n\n    private static void Check(int status, in TrapInfo trap)\n    {{\n        if (status == 1)\n        {{\n            throw new TsuzuriTrapException(trap.Site, trap.Kind);\n        }}\n        if (status != 0)\n        {{\n            throw new InvalidOperationException(\"a Tsuzuri export was called while another call of the same thread was running\");\n        }}\n    }}\n",
            kinds.join(", ")
        );
    }
    let mut imports = String::new();
    for function in exports(module) {
        let method = unique(csharp_name(&function.name), &taken, "_");
        taken.insert(method.clone());
        csharp_export(
            &mut output,
            &mut imports,
            function,
            module,
            &method,
            trap_return,
        );
    }
    if owned {
        imports.push_str(
            "        [LibraryImport(LibraryName)]\n        internal static partial void tsuzuri_free(nint ptr);\n",
        );
    }
    let _ = write!(
        output,
        "    /// <summary>The C entry points of the library.</summary>\n    internal static unsafe partial class Library\n    {{\n{imports}    }}\n}}\n"
    );
    output
}

/// One public method of the C# bindings and its `LibraryImport` declaration.
fn csharp_export(
    output: &mut String,
    imports: &mut String,
    function: &CheckedFunction,
    module: &CheckedModule,
    method: &str,
    trap_return: bool,
) {
    let name = &function.name;
    let result = shape(&function.signature.result, module);
    let mut public = Vec::new();
    let mut native = Vec::new();
    let mut arguments = Vec::new();
    let mut guards = Vec::new();
    let mut pins = Vec::new();
    let out = match &result {
        Shape::Owned(_) => Some("Descriptor".to_owned()),
        Shape::Record(record) => Some(record.clone()),
        _ => None,
    };
    for (index, ty) in function.signature.parameters.iter().enumerate() {
        let argument = format!("arg{index}");
        match shape(ty, module) {
            Shape::Scalar(scalar) => {
                public.push(format!("{} {argument}", scalar.csharp()));
                native.push(format!("{} {argument}", scalar.csharp_abi()));
                arguments.push(if scalar == Scalar::Bool {
                    format!("{argument} ? 1 : 0")
                } else {
                    argument
                });
            }
            Shape::Handle(handle) => {
                public.push(format!("{handle} {argument}"));
                native.push(format!("nint {argument}"));
                arguments.push(format!("{argument}.Value"));
            }
            Shape::Slice(buffer) => {
                let element = csharp_element(buffer);
                public.push(format!("ReadOnlySpan<{element}> {argument}"));
                native.push(format!("{element}* {argument}_ptr"));
                native.push(format!("long {argument}_len"));
                if buffer == Buffer::Utf8String {
                    guards.push(format!(
                        "if (!System.Text.Unicode.Utf8.IsValid({argument}))\n        {{\n            throw new ArgumentException(\"argument {index} of '{name}' must be valid UTF-8\", nameof({argument}));\n        }}"
                    ));
                }
                pins.push(format!("fixed ({element}* pointer{index} = {argument})"));
                arguments.push(format!("pointer{index}"));
                arguments.push(format!("{argument}.Length"));
            }
            Shape::Record(record) => {
                public.push(format!("in {record} {argument}"));
                native.push(format!("{record}* {argument}"));
                pins.push(format!("fixed ({record}* pointer{index} = &{argument})"));
                arguments.push(format!("pointer{index}"));
            }
            Shape::Unit | Shape::Owned(_) => unreachable!("export parameters are checked"),
        }
    }
    let (public_result, native_result) = match &result {
        Shape::Scalar(scalar) => (scalar.csharp().to_owned(), scalar.csharp_abi().to_owned()),
        Shape::Handle(handle) => (handle.clone(), "nint".to_owned()),
        Shape::Unit => ("void".to_owned(), "void".to_owned()),
        Shape::Owned(buffer) => (csharp_owned(*buffer).to_owned(), "void".to_owned()),
        Shape::Record(record) => (record.clone(), "void".to_owned()),
        Shape::Slice(_) => unreachable!("export results are checked"),
    };
    // The C call: the out slot leads, and `--trap-mode return` puts the trap and result slots first.
    let mut call_arguments = Vec::new();
    let mut native_parameters = Vec::new();
    let symbol = if trap_return {
        native_parameters.push("TrapInfo* trap".to_owned());
        call_arguments.push("&trap".to_owned());
        if !matches!(result, Shape::Unit) && out.is_none() {
            native_parameters.push(format!("{native_result}* result"));
            call_arguments.push("&value".to_owned());
        }
        format!("tsuzuri_try_{name}")
    } else {
        format!("tz_{name}")
    };
    if let Some(slot) = &out {
        native_parameters.push(format!("{slot}* @out"));
        call_arguments.push(if slot == "Descriptor" {
            "&descriptor".to_owned()
        } else {
            "&value".to_owned()
        });
    }
    native_parameters.extend(native);
    call_arguments.extend(arguments);
    let native_return = if trap_return {
        "int"
    } else {
        native_result.as_str()
    };
    let _ = writeln!(
        imports,
        "        [LibraryImport(LibraryName)]\n        internal static partial {native_return} {symbol}({});\n",
        native_parameters.join(", ")
    );
    let _ = writeln!(
        output,
        "    /// <summary><c>tz_{name}</c>: <c>{}</c>.</summary>\n    public static {public_result} {method}({})\n    {{",
        xml_text(&signature_text(function, module)),
        public.join(", ")
    );
    for guard in &guards {
        let _ = writeln!(output, "        {guard}");
    }
    let mut body = Vec::new();
    if trap_return {
        body.push("TrapInfo trap;".to_owned());
    }
    match &result {
        Shape::Owned(buffer) => {
            body.push(format!("var result = new {}();", csharp_owned(*buffer)));
            body.push("Descriptor descriptor;".to_owned());
        }
        Shape::Record(record) => body.push(format!("{record} value;")),
        Shape::Unit => {}
        _ if trap_return => body.push(format!("{native_result} value;")),
        _ => {}
    }
    let call = format!("Library.{symbol}({})", call_arguments.join(", "));
    let invoke = if trap_return {
        format!("Check({call}, trap);")
    } else if out.is_some() || matches!(result, Shape::Unit) {
        format!("{call};")
    } else {
        format!("value = {call};")
    };
    if !trap_return && out.is_none() && !matches!(result, Shape::Unit) {
        body.push(format!("{native_result} value;"));
    }
    let finish = match &result {
        Shape::Owned(_) => "result.Own(descriptor);\nreturn result;".to_owned(),
        Shape::Record(_) => "return value;".to_owned(),
        Shape::Unit => String::new(),
        Shape::Scalar(Scalar::Bool) => "return value != 0;".to_owned(),
        Shape::Scalar(scalar) if scalar.normalized() => {
            format!("return ({})value;", scalar.csharp())
        }
        Shape::Scalar(_) => "return value;".to_owned(),
        Shape::Handle(handle) => format!("return new {handle}(value);"),
        Shape::Slice(_) => unreachable!("export results are checked"),
    };
    for line in &body {
        let _ = writeln!(output, "        {line}");
    }
    if pins.is_empty() {
        let _ = writeln!(output, "        {invoke}");
    } else {
        for pin in &pins {
            let _ = writeln!(output, "        {pin}");
        }
        let _ = writeln!(output, "        {{\n            {invoke}\n        }}");
    }
    for line in finish.lines() {
        let _ = writeln!(output, "        {line}");
    }
    output.push_str("    }\n\n");
}

/// The Tsuzuri signature of an export, for documentation comments.
fn signature_text(function: &CheckedFunction, module: &CheckedModule) -> String {
    let types = module.types();
    function
        .signature
        .parameters
        .iter()
        .chain([&function.signature.result])
        .map(|ty| ty.display(&types))
        .collect::<Vec<_>>()
        .join(" -> ")
}

fn xml_text(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

const PYTHON_KEYWORDS: &[&str] = &[
    "False", "None", "True", "and", "as", "assert", "async", "await", "break", "class", "continue",
    "def", "del", "elif", "else", "except", "finally", "for", "from", "global", "if", "import",
    "in", "is", "lambda", "nonlocal", "not", "or", "pass", "raise", "return", "try", "while",
    "with", "yield",
];

/// A Tsuzuri name as a Python attribute: keywords and names that end with `_` get one more `_`
/// (so no two names meet), and a leading `__` gets `tz` before it (no name mangling).
fn python_name(name: &str) -> String {
    let mut text = if name.starts_with("__") {
        format!("tz{name}")
    } else {
        name.to_owned()
    };
    if PYTHON_KEYWORDS.contains(&text.as_str()) || text.ends_with('_') {
        text.push('_');
    }
    text
}

/// The converters and ctypes declarations that every Python module shares.
const PYTHON_SUPPORT: &str = r#"
class _Buffer(_ctypes.Structure):
    _fields_ = [("ptr", _ctypes.c_void_p), ("len", _ctypes.c_int64)]


class _Trap(_ctypes.Structure):
    _fields_ = [("site", _ctypes.c_uint32), ("kind", _ctypes.c_uint32)]


def _integer(value, low, high, kind, where):
    if isinstance(value, bool):
        raise TypeError(f"{where} must be an int")
    try:
        value = _operator.index(value)
    except TypeError:
        raise TypeError(f"{where} must be an int") from None
    if not low <= value <= high:
        raise OverflowError(f"{where} is out of range for {kind}")
    return value


def _float(value, where):
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        raise TypeError(f"{where} must be a float")
    return float(value)


def _bool(value, where):
    if not isinstance(value, bool):
        raise TypeError(f"{where} must be a bool")
    return 1 if value else 0


def _numbers(value, ctype, formats, check, where):
    """A C array of `ctype` with the elements of a buffer or a sequence: (array or None, length)."""
    try:
        view = memoryview(value)
    except TypeError:
        view = None
    if view is not None:
        if view.ndim != 1 or view.itemsize != _ctypes.sizeof(ctype) or view.format.lstrip("@=<") not in formats:
            raise TypeError(f"{where} must be a buffer of {formats[0]!r} items or a sequence")
        length = len(view)
        if length == 0:
            return None, 0
        if not view.c_contiguous:
            view = memoryview(view.tobytes())
        if not view.readonly:
            array = (ctype * length).from_buffer(view)
            if _ctypes.addressof(array) % _ctypes.alignment(ctype) == 0:
                return array, length
        return (ctype * length).from_buffer_copy(view), length
    if isinstance(value, (str, bytes)):
        raise TypeError(f"{where} must be a buffer or a sequence of numbers")
    try:
        items = list(value)
    except TypeError:
        raise TypeError(f"{where} must be a buffer or a sequence of numbers") from None
    if not items:
        return None, 0
    return (ctype * len(items))(*(check(item, f"{where} element {index}") for index, item in enumerate(items))), len(items)


def _utf16(value, where):
    if not isinstance(value, str):
        raise TypeError(f"{where} must be a str")
    data = value.encode("utf-16-le", "surrogatepass")
    if not data:
        return None, 0
    return (_ctypes.c_uint16 * (len(data) // 2)).from_buffer_copy(data), len(data) // 2


def _utf8(value, where):
    if isinstance(value, str):
        try:
            data = value.encode("utf-8")
        except UnicodeEncodeError:
            raise ValueError(f"{where} must be a well-formed str") from None
    else:
        try:
            data = bytes(memoryview(value).cast("B"))
        except TypeError:
            raise TypeError(f"{where} must be a str or bytes") from None
        try:
            data.decode("utf-8")
        except UnicodeDecodeError:
            raise ValueError(f"{where} must be valid UTF-8") from None
    if not data:
        return None, 0
    return (_ctypes.c_uint8 * len(data)).from_buffer_copy(data), len(data)


def _fixed(value, length, ctype, check, where):
    """The C array of a fixed-length array field from a sequence of exactly `length` elements."""
    if isinstance(value, (str, bytes)):
        raise TypeError(f"{where} must be a sequence of {length} elements")
    try:
        items = list(value)
    except TypeError:
        raise TypeError(f"{where} must be a sequence of {length} elements") from None
    if len(items) != length:
        raise TypeError(f"{where} must be a sequence of {length} elements")
    return (ctype * length)(*(check(item, f"{where}[{index}]") for index, item in enumerate(items)))


def _take(library, descriptor, kind):
    """Copies an owned result out of the library and frees it at once."""
    pointer, length = descriptor.ptr, descriptor.len
    try:
        width = {"q": 8, "d": 8, "B": 1, "string": 2, "utf8string": 1}[kind]
        data = _ctypes.string_at(pointer, length * width) if length else b""
    finally:
        library.tsuzuri_free(pointer)
    if kind == "string":
        return data.decode("utf-16-le", "surrogatepass")
    if kind == "utf8string":
        return data.decode("utf-8")
    if kind == "B":
        return data
    values = _array.array(kind)
    values.frombytes(data)
    return values


def _find(path):
    if path is not None:
        return _os.fspath(path)
    directory = _os.path.dirname(_os.path.abspath(__file__))
    if _sys.platform == "darwin":
        patterns = ("lib{}.dylib", "{}.dylib")
    elif _sys.platform == "win32":
        patterns = ("{}.dll",)
    else:
        patterns = ("lib{}.so", "{}.so")
    for pattern in patterns:
        candidate = _os.path.join(directory, pattern.format(LIBRARY_NAME))
        if _os.path.exists(candidate):
            return candidate
    raise OSError(f"cannot find the shared library {LIBRARY_NAME!r} next to {__file__}; build it with 'tsuzuri build --emit shared' or pass its path to load()")
"#;

/// The ctypes type of an ABI value at the C boundary.
fn python_ctype(shape: &Shape) -> String {
    match shape {
        Shape::Scalar(scalar) => scalar.ctypes().to_owned(),
        Shape::Handle(_) => "_ctypes.c_void_p".to_owned(),
        Shape::Record(record) => format!("_ctypes.POINTER(_{record})"),
        Shape::Unit => "None".to_owned(),
        Shape::Slice(_) | Shape::Owned(_) => unreachable!("buffers have two C values"),
    }
}

/// The Python expression that checks `value` and makes the C value of `ty`.
fn python_lower(ty: &Type, module: &CheckedModule, value: &str, at: &str) -> String {
    if let Some(scalar) = Scalar::of(ty) {
        return match (scalar, scalar.range()) {
            (Scalar::Bool, _) => format!("_bool({value}, {at})"),
            (_, Some((low, high))) => {
                format!(
                    "_integer({value}, {low}, {high}, \"{}\", {at})",
                    scalar.tsuzuri()
                )
            }
            _ => format!("_float({value}, {at})"),
        };
    }
    match ty {
        Type::Record(..) => format!("_to_{}({value}, {at})", record_name(ty, module)),
        Type::FixedArray(element, _) => {
            let scalar = Scalar::of(element).expect("scalar elements");
            format!(
                "_fixed({value}, {}, {}, lambda item, where: {}, {at})",
                fixed_length(ty),
                scalar.ctypes(),
                python_lower(element, module, "item", "where")
            )
        }
        _ => unreachable!("record fields are scalars, arrays and records"),
    }
}

/// The Python expression that reads the C value `raw` of a record field.
fn python_lift(ty: &Type, module: &CheckedModule, raw: &str) -> String {
    match ty {
        Type::Bool => format!("{raw} != 0"),
        Type::Record(..) => format!("_from_{}({raw})", record_name(ty, module)),
        Type::FixedArray(element, _) => {
            format!("tuple({})", python_lift_items(element, raw))
        }
        _ => raw.to_owned(),
    }
}

fn python_lift_items(element: &Type, raw: &str) -> String {
    if *element == Type::Bool {
        format!("item != 0 for item in {raw}")
    } else {
        raw.to_owned()
    }
}

/// The Python bindings of `--emit bindings-py`: a `ctypes` module whose `Library` methods check
/// their arguments, call the library `name`, and copy owned results before `tsuzuri_free`. With
/// `trap_return` they call `tsuzuri_try_<name>` and raise `TsuzuriTrap`.
pub fn python(module: &CheckedModule, name: &str, trap_return: bool) -> String {
    let records = ordered_records(module);
    let handles = handles(module);
    let mut output = banner("#");
    let _ = writeln!(
        output,
        "\"\"\"ctypes bindings for the exports of the Tsuzuri shared library {name:?}.\n\nBuild the library from the same sources with `tsuzuri build --emit shared{}`, then call\n`load()` (or `load(path)`) and the methods of the returned `Library`.\n\"\"\"\n\nimport array as _array\nimport ctypes as _ctypes\nimport dataclasses as _dataclasses\nimport operator as _operator\nimport os as _os\nimport sys as _sys\n",
        if trap_return {
            " --trap-mode return"
        } else {
            ""
        }
    );
    let mut public = vec![
        "Library".to_owned(),
        "load".to_owned(),
        "LIBRARY_NAME".to_owned(),
    ];
    if trap_return {
        public.push("TsuzuriTrap".to_owned());
    }
    public.extend(records.iter().map(|(record, _)| record.clone()));
    public.extend(handles.iter().cloned());
    let _ = writeln!(
        output,
        "__all__ = [{}]\n\nLIBRARY_NAME = {name:?}",
        public
            .iter()
            .map(|item| format!("{item:?}"))
            .collect::<Vec<_>>()
            .join(", ")
    );
    output.push_str(PYTHON_SUPPORT);
    if trap_return {
        let kinds: Vec<_> = trap_kinds()
            .iter()
            .map(|kind| format!("{kind:?}"))
            .collect();
        let _ = writeln!(
            output,
            "\n\nTRAP_KINDS = ({},)\n\n\nclass TsuzuriTrap(Exception):\n    \"\"\"A trap inside the library, which --trap-mode return reports instead of ending the process.\n\n    `site` is an id of the library's .trap.json; `kind` indexes TRAP_KINDS.\n    \"\"\"\n\n    def __init__(self, site, kind):\n        self.site = site\n        self.kind = kind\n        self.kind_name = TRAP_KINDS[kind] if kind < len(TRAP_KINDS) else \"unknown trap\"\n        super().__init__(f\"trap: {{self.kind_name}} (site {{site}})\")\n\n\ndef _check(status, trap):\n    if status == 1:\n        raise TsuzuriTrap(trap.site, trap.kind)\n    if status != 0:\n        raise RuntimeError(\"a Tsuzuri export was called while another call of the same thread was running\")",
            kinds.join(", ")
        );
    }
    for handle in &handles {
        let _ = writeln!(
            output,
            "\n\n@_dataclasses.dataclass(frozen=True)\nclass {handle}:\n    \"\"\"An extern type handle: a pointer that the host owns.\"\"\"\n\n    value: int"
        );
    }
    for (record, ty) in &records {
        let fields = fields(ty, module);
        let _ = writeln!(
            output,
            "\n\n@_dataclasses.dataclass\nclass {record}:\n    \"\"\"A Tsuzuri record; fixed-length array fields are tuples.\"\"\"\n"
        );
        if fields.is_empty() {
            output.push_str("    pass\n");
        }
        for (field, _, field_type) in &fields {
            let annotation = match field_type {
                Type::Record(..) => record_name(field_type, module),
                Type::FixedArray(..) => "tuple".to_owned(),
                Type::Bool => "bool".to_owned(),
                Type::Binary(_) => "float".to_owned(),
                _ => "int".to_owned(),
            };
            let _ = writeln!(output, "    {}: {annotation}", python_name(field));
        }
        let structure: Vec<_> = fields
            .iter()
            .enumerate()
            .map(|(index, (_, _, field_type))| {
                let ctype = match field_type {
                    Type::Record(..) => format!("_{}", record_name(field_type, module)),
                    Type::FixedArray(element, _) => format!(
                        "{} * {}",
                        Scalar::of(element).expect("scalar elements").ctypes(),
                        fixed_length(field_type)
                    ),
                    _ => Scalar::of(field_type)
                        .expect("scalar field")
                        .ctypes()
                        .to_owned(),
                };
                format!("(\"f{index}\", {ctype})")
            })
            .collect();
        let structure = if fields.is_empty() {
            "(\"empty\", _ctypes.c_uint8)".to_owned()
        } else {
            structure.join(", ")
        };
        let _ = write!(
            output,
            "\n\nclass _{record}(_ctypes.Structure):\n    _fields_ = [{structure}]\n\n\ndef _to_{record}(value, where):\n    try:\n        return _{record}(\n"
        );
        for (field, _, field_type) in &fields {
            let attribute = python_name(field);
            let at = format!("f\"{{where}} field '{field}'\"");
            let _ = writeln!(
                output,
                "            {},",
                python_lower(field_type, module, &format!("value.{attribute}"), &at)
            );
        }
        let lifted: Vec<_> = fields
            .iter()
            .enumerate()
            .map(|(index, (_, _, field_type))| {
                python_lift(field_type, module, &format!("raw.f{index}"))
            })
            .collect();
        let _ = writeln!(
            output,
            "        )\n    except AttributeError as error:\n        raise TypeError(f\"{{where}} must be a {record}: {{error}}\") from None\n\n\ndef _from_{record}(raw):\n    return {record}({})",
            lifted.join(", ")
        );
    }
    let mut setup = String::new();
    let mut methods = String::new();
    let mut taken = BTreeSet::from(["_tz_library".to_owned()]);
    for function in exports(module) {
        let method = unique(python_name(&function.name), &taken, "_");
        taken.insert(method.clone());
        python_export(
            &mut setup,
            &mut methods,
            function,
            module,
            &method,
            trap_return,
        );
    }
    if exports(module)
        .iter()
        .any(|function| Buffer::of(&function.signature.result).is_some())
    {
        setup.push_str("        library.tsuzuri_free.argtypes = (_ctypes.c_void_p,)\n        library.tsuzuri_free.restype = None\n");
    }
    let _ = write!(
        output,
        "\n\nclass Library:\n    \"\"\"One loaded copy of the shared library; its methods are the library's exports.\"\"\"\n\n    def __init__(self, path=None):\n        library = _ctypes.CDLL(_find(path))\n{setup}        self._tz_library = library\n{methods}\n\ndef load(path=None):\n    \"\"\"Loads the shared library: `path`, or lib{name} next to this module.\"\"\"\n    return Library(path)\n"
    );
    output
}

/// One method of the Python `Library` and the ctypes signature it sets up.
fn python_export(
    setup: &mut String,
    methods: &mut String,
    function: &CheckedFunction,
    module: &CheckedModule,
    method: &str,
    trap_return: bool,
) {
    let name = &function.name;
    let result = shape(&function.signature.result, module);
    let symbol = if trap_return {
        format!("tsuzuri_try_{name}")
    } else {
        format!("tz_{name}")
    };
    let mut argtypes = Vec::new();
    let mut parameters = Vec::new();
    let mut prepare = Vec::new();
    let mut arguments = Vec::new();
    if trap_return {
        argtypes.push("_ctypes.POINTER(_Trap)".to_owned());
        arguments.push("_ctypes.byref(trap)".to_owned());
        prepare.push("trap = _Trap()".to_owned());
        if !matches!(result, Shape::Unit | Shape::Owned(_) | Shape::Record(_)) {
            argtypes.push(format!("_ctypes.POINTER({})", python_ctype(&result)));
            arguments.push("_ctypes.byref(result)".to_owned());
            prepare.push(format!("result = {}()", python_ctype(&result)));
        }
    }
    match &result {
        Shape::Owned(_) => {
            argtypes.push("_ctypes.POINTER(_Buffer)".to_owned());
            arguments.push("_ctypes.byref(out)".to_owned());
            prepare.push("out = _Buffer()".to_owned());
        }
        Shape::Record(record) => {
            argtypes.push(format!("_ctypes.POINTER(_{record})"));
            arguments.push("_ctypes.byref(out)".to_owned());
            prepare.push(format!("out = _{record}()"));
        }
        _ => {}
    }
    for (index, ty) in function.signature.parameters.iter().enumerate() {
        let argument = format!("arg{index}");
        parameters.push(argument.clone());
        let at = format!("\"argument {index} of '{name}'\"");
        match shape(ty, module) {
            Shape::Scalar(scalar) => {
                argtypes.push(scalar.ctypes().to_owned());
                arguments.push(python_lower(ty, module, &argument, &at));
            }
            Shape::Handle(handle) => {
                argtypes.push("_ctypes.c_void_p".to_owned());
                prepare.push(format!(
                    "if not isinstance({argument}, {handle}):\n            raise TypeError(f\"argument {index} of '{name}' must be a {handle}\")"
                ));
                arguments.push(format!("{argument}.value"));
            }
            Shape::Slice(buffer) => {
                let (ctype, convert) = match buffer {
                    Buffer::I64 => (
                        "_ctypes.c_int64",
                        format!(
                            "_numbers({argument}, _ctypes.c_int64, (\"q\", \"l\"), lambda item, where: _integer(item, -9223372036854775808, 9223372036854775807, \"i64\", where), {at})"
                        ),
                    ),
                    Buffer::F64 => (
                        "_ctypes.c_double",
                        format!("_numbers({argument}, _ctypes.c_double, (\"d\",), _float, {at})"),
                    ),
                    Buffer::UByte => (
                        "_ctypes.c_uint8",
                        format!(
                            "_numbers({argument}, _ctypes.c_uint8, (\"B\",), lambda item, where: _integer(item, 0, 255, \"ubyte\", where), {at})"
                        ),
                    ),
                    Buffer::String => ("_ctypes.c_uint16", format!("_utf16({argument}, {at})")),
                    Buffer::Utf8String => ("_ctypes.c_uint8", format!("_utf8({argument}, {at})")),
                };
                argtypes.push(format!("_ctypes.POINTER({ctype})"));
                argtypes.push("_ctypes.c_int64".to_owned());
                prepare.push(format!("data{index}, length{index} = {convert}"));
                arguments.push(format!("data{index}"));
                arguments.push(format!("length{index}"));
            }
            Shape::Record(record) => {
                argtypes.push(format!("_ctypes.POINTER(_{record})"));
                prepare.push(format!("record{index} = _to_{record}({argument}, {at})"));
                arguments.push(format!("_ctypes.byref(record{index})"));
            }
            Shape::Unit | Shape::Owned(_) => unreachable!("export parameters are checked"),
        }
    }
    let restype = if trap_return {
        "_ctypes.c_int32".to_owned()
    } else {
        match &result {
            Shape::Scalar(_) | Shape::Handle(_) => python_ctype(&result),
            _ => "None".to_owned(),
        }
    };
    let _ = writeln!(
        setup,
        "        library.{symbol}.argtypes = ({}{})\n        library.{symbol}.restype = {restype}",
        argtypes.join(", "),
        if argtypes.len() == 1 { "," } else { "" }
    );
    let call = format!("library.{symbol}({})", arguments.join(", "));
    let raw = if trap_return { "result.value" } else { "raw" };
    let finish = match &result {
        Shape::Unit => "return None".to_owned(),
        Shape::Owned(buffer) => {
            let kind = match buffer {
                Buffer::I64 => "q",
                Buffer::F64 => "d",
                Buffer::UByte => "B",
                Buffer::String => "string",
                Buffer::Utf8String => "utf8string",
            };
            format!("return _take(library, out, \"{kind}\")")
        }
        Shape::Record(record) => format!("return _from_{record}(out)"),
        Shape::Scalar(Scalar::Bool) => format!("return {raw} != 0"),
        Shape::Scalar(_) => format!("return {raw}"),
        Shape::Handle(handle) => format!("return {handle}({raw} or 0)"),
        Shape::Slice(_) => unreachable!("export results are checked"),
    };
    let invoke = if trap_return {
        format!("_check({call}, trap)")
    } else if matches!(result, Shape::Scalar(_) | Shape::Handle(_)) {
        format!("raw = {call}")
    } else {
        call
    };
    let _ = writeln!(
        methods,
        "\n    def {method}({}):\n        \"\"\"tz_{name}: {}\"\"\"\n        library = self._tz_library",
        ["self".to_owned()]
            .into_iter()
            .chain(parameters)
            .collect::<Vec<_>>()
            .join(", "),
        signature_text(function, module)
    );
    for line in prepare {
        let _ = writeln!(methods, "        {line}");
    }
    let _ = writeln!(methods, "        {invoke}\n        {finish}");
}

const CPP_KEYWORDS: &[&str] = &[
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
];

/// A name as a C++ identifier: keywords and the names of the wrapper's own types get a trailing `_`.
fn cpp_name(name: &str, taken: &BTreeSet<String>) -> String {
    let name = if CPP_KEYWORDS.contains(&name) {
        format!("{name}_")
    } else {
        name.to_owned()
    };
    unique(name, taken, "_")
}

const CPP_SUPPORT: &str = r#"/// An owned result: tsuzuri_free releases its memory when the buffer is destroyed.
template <class T>
class buffer {
public:
    buffer() noexcept = default;
    buffer(T *data, std::int64_t size) noexcept : data_(data), size_(static_cast<std::size_t>(size)) {}
    // A moved-from buffer is empty: no memory and size 0.
    buffer(buffer &&other) noexcept : data_(std::move(other.data_)), size_(std::exchange(other.size_, 0)) {}
    buffer &operator=(buffer &&other) noexcept {
        data_ = std::move(other.data_);
        size_ = std::exchange(other.size_, 0);
        return *this;
    }
    buffer(const buffer &) = delete;
    buffer &operator=(const buffer &) = delete;
    [[nodiscard]] T *data() const noexcept { return data_.get(); }
    [[nodiscard]] std::size_t size() const noexcept { return size_; }
    [[nodiscard]] bool empty() const noexcept { return size_ == 0; }
    [[nodiscard]] T *begin() const noexcept { return data_.get(); }
    // An empty buffer (default-constructed or moved-from) holds no memory, so its range is built
    // without pointer arithmetic on the null pointer.
    [[nodiscard]] T *end() const noexcept { return size_ == 0 ? data_.get() : data_.get() + size_; }
    [[nodiscard]] std::span<T> span() const noexcept { return size_ == 0 ? std::span<T>{} : std::span<T>{data_.get(), size_}; }
    T &operator[](std::size_t index) const noexcept { return data_.get()[index]; }

private:
    struct release {
        void operator()(T *pointer) const noexcept { ::tsuzuri_free(pointer); }
    };
    std::unique_ptr<T, release> data_;
    std::size_t size_ = 0;
};

/// An owned string result: UTF-16 code units, lone surrogates included.
class string_buffer : public buffer<char16_t> {
public:
    using buffer::buffer;
    [[nodiscard]] std::u16string_view view() const noexcept { return empty() ? std::u16string_view{} : std::u16string_view{data(), size()}; }
};

/// An owned utf8string result: valid UTF-8 bytes.
class utf8string_buffer : public buffer<char> {
public:
    using buffer::buffer;
    [[nodiscard]] std::string_view view() const noexcept { return empty() ? std::string_view{} : std::string_view{data(), size()}; }
};

"#;

const CPP_UTF8: &str = r#"/// Whether `text` is valid UTF-8; the library traps on anything else.
[[nodiscard]] inline bool valid_utf8(std::string_view text) noexcept {
    const auto *bytes = reinterpret_cast<const unsigned char *>(text.data());
    std::size_t index = 0;
    while (index < text.size()) {
        const unsigned char lead = bytes[index];
        std::size_t count = 0;
        std::uint32_t scalar = 0;
        if (lead < 0x80) {
            ++index;
            continue;
        }
        if (lead >= 0xC2 && lead <= 0xDF) {
            count = 1;
            scalar = lead & 0x1F;
        } else if (lead >= 0xE0 && lead <= 0xEF) {
            count = 2;
            scalar = lead & 0x0F;
        } else if (lead >= 0xF0 && lead <= 0xF4) {
            count = 3;
            scalar = lead & 0x07;
        } else {
            return false;
        }
        if (text.size() - index <= count) return false;
        for (std::size_t next = 1; next <= count; ++next) {
            const unsigned char byte = bytes[index + next];
            if ((byte & 0xC0) != 0x80) return false;
            scalar = (scalar << 6) | (byte & 0x3F);
        }
        const std::uint32_t minimum = count == 1 ? 0x80 : count == 2 ? 0x800 : 0x10000;
        if (scalar < minimum || scalar > 0x10FFFF || (scalar >= 0xD800 && scalar <= 0xDFFF)) return false;
        index += count + 1;
    }
    return true;
}

"#;

/// The C++ bindings of `--emit bindings-cpp`: a header-only C++20 wrapper over `<name>.h`, the C
/// header of the same sources, with `std::span` and string-view inputs and RAII owned results.
/// With `trap_return` the functions call `tsuzuri_try_<name>` and throw `trap_error`.
pub fn cpp(module: &CheckedModule, name: &str, trap_return: bool) -> String {
    let mut namespace = identifier(name);
    if CPP_KEYWORDS.contains(&namespace.as_str()) {
        namespace.push('_');
    }
    let utf8 = exports(module).iter().any(|function| {
        function
            .signature
            .parameters
            .iter()
            .any(|ty| matches!(shape(ty, module), Shape::Slice(Buffer::Utf8String)))
    });
    let owned = exports(module)
        .iter()
        .any(|function| Buffer::of(&function.signature.result).is_some());
    let mut output = banner("//");
    let _ = write!(
        output,
        "// A header-only C++20 wrapper over \"{name}.h\": generate it from the same sources with\n// tsuzuri build --emit header{} and link the library of tsuzuri build --emit shared (or --emit object).\n#pragma once\n#include \"{name}.h\"\n\n#include <cstddef>\n#include <cstdint>\n#include <memory>\n#include <span>\n#include <stdexcept>\n#include <string>\n#include <string_view>\n#include <utility>\n\nnamespace tsuzuri::{namespace} {{\n\n",
        if trap_return {
            " --trap-mode return"
        } else {
            ""
        }
    );
    let mut taken: BTreeSet<String> = [
        "buffer",
        "string_buffer",
        "utf8string_buffer",
        "trap_error",
        "detail",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    if owned {
        output.push_str(CPP_SUPPORT);
    }
    if trap_return {
        let kinds: Vec<_> = trap_kinds()
            .iter()
            .map(|kind| format!("\"{kind}\""))
            .collect();
        let _ = write!(
            output,
            "/// A trap inside the library, which --trap-mode return reports instead of ending the process.\n/// site() is an id of the library's .trap.json.\nclass trap_error : public std::runtime_error {{\npublic:\n    trap_error(std::uint32_t site, std::uint32_t kind)\n        : std::runtime_error(std::string(\"trap: \") + name(kind) + \" (site \" + std::to_string(site) + \")\"), site_(site), kind_(kind) {{}}\n    [[nodiscard]] std::uint32_t site() const noexcept {{ return site_; }}\n    [[nodiscard]] std::uint32_t kind() const noexcept {{ return kind_; }}\n    [[nodiscard]] const char *kind_name() const noexcept {{ return name(kind_); }}\n\nprivate:\n    static const char *name(std::uint32_t kind) noexcept {{\n        static constexpr const char *kinds[] = {{{}}};\n        return kind < sizeof kinds / sizeof kinds[0] ? kinds[kind] : \"unknown trap\";\n    }}\n    std::uint32_t site_;\n    std::uint32_t kind_;\n}};\n\n",
            kinds.join(", ")
        );
    }
    if trap_return || utf8 {
        output.push_str("namespace detail {\n\n");
        if utf8 {
            output.push_str(CPP_UTF8);
        }
        if trap_return {
            output.push_str("inline void check(std::int32_t status, const tsuzuri_trap_info &trap) {\n    if (status == 1) throw trap_error(trap.site, trap.kind);\n    if (status != 0) throw std::logic_error(\"a Tsuzuri export was called while another call of the same thread was running\");\n}\n\n");
        }
        output.push_str("} // namespace detail\n\n");
    }
    for function in exports(module) {
        let function_name = cpp_name(&function.name, &taken);
        taken.insert(function_name.clone());
        cpp_export(&mut output, function, module, &function_name, trap_return);
    }
    let _ = writeln!(output, "}} // namespace tsuzuri::{namespace}");
    output
}

/// One inline function of the C++ wrapper.
fn cpp_export(
    output: &mut String,
    function: &CheckedFunction,
    module: &CheckedModule,
    function_name: &str,
    trap_return: bool,
) {
    let name = &function.name;
    let result = shape(&function.signature.result, module);
    let mut parameters = Vec::new();
    let mut guards = Vec::new();
    let mut arguments = Vec::new();
    for (index, ty) in function.signature.parameters.iter().enumerate() {
        let argument = format!("arg{index}");
        match shape(ty, module) {
            Shape::Scalar(scalar) => {
                parameters.push(format!("{} {argument}", scalar.cpp()));
                arguments.push(match scalar {
                    Scalar::Bool => format!("{argument} ? 1 : 0"),
                    _ => argument,
                });
            }
            Shape::Handle(handle) => {
                parameters.push(format!("{handle} {argument}"));
                arguments.push(argument);
            }
            Shape::Slice(buffer) => {
                let (parameter, pointer) = match buffer {
                    Buffer::I64 => (
                        "std::span<const std::int64_t>",
                        format!("{argument}.data()"),
                    ),
                    Buffer::F64 => ("std::span<const double>", format!("{argument}.data()")),
                    Buffer::UByte => (
                        "std::span<const std::uint8_t>",
                        format!("{argument}.data()"),
                    ),
                    Buffer::String => (
                        "std::u16string_view",
                        format!("reinterpret_cast<const std::uint16_t *>({argument}.data())"),
                    ),
                    Buffer::Utf8String => (
                        "std::string_view",
                        format!("reinterpret_cast<const std::uint8_t *>({argument}.data())"),
                    ),
                };
                if buffer == Buffer::Utf8String {
                    guards.push(format!(
                        "if (!detail::valid_utf8({argument})) throw std::invalid_argument(\"argument {index} of '{name}' must be valid UTF-8\");"
                    ));
                }
                parameters.push(format!("{parameter} {argument}"));
                arguments.push(pointer);
                arguments.push(format!("static_cast<std::int64_t>({argument}.size())"));
            }
            Shape::Record(record) => {
                parameters.push(format!("const {record} &{argument}"));
                arguments.push(format!("&{argument}"));
            }
            Shape::Unit | Shape::Owned(_) => unreachable!("export parameters are checked"),
        }
    }
    let (returned, c_result, slot) = match &result {
        Shape::Scalar(scalar) => (scalar.cpp().to_owned(), c_scalar(*scalar), None),
        Shape::Handle(handle) => (handle.clone(), handle.clone(), None),
        Shape::Unit => ("void".to_owned(), "void".to_owned(), None),
        Shape::Owned(buffer) => {
            let (owner, descriptor) = match buffer {
                Buffer::I64 => ("buffer<std::int64_t>", "tsuzuri_i64_buffer"),
                Buffer::F64 => ("buffer<double>", "tsuzuri_f64_buffer"),
                Buffer::UByte => ("buffer<std::uint8_t>", "tsuzuri_ubyte_buffer"),
                Buffer::String => ("string_buffer", "tsuzuri_string_buffer"),
                Buffer::Utf8String => ("utf8string_buffer", "tsuzuri_utf8string_buffer"),
            };
            (
                owner.to_owned(),
                "void".to_owned(),
                Some(descriptor.to_owned()),
            )
        }
        Shape::Record(record) => (record.clone(), "void".to_owned(), Some(record.clone())),
        Shape::Slice(_) => unreachable!("export results are checked"),
    };
    let _ = writeln!(
        output,
        "/// tz_{name}: {}\ninline {returned} {function_name}({}) {{",
        signature_text(function, module),
        parameters.join(", ")
    );
    for guard in &guards {
        let _ = writeln!(output, "    {guard}");
    }
    let mut call_arguments = Vec::new();
    if trap_return {
        output.push_str("    tsuzuri_trap_info trap{};\n");
        call_arguments.push("&trap".to_owned());
    }
    if let Some(slot) = &slot {
        let _ = writeln!(output, "    {slot} out{{}};");
        call_arguments.push("&out".to_owned());
    } else if trap_return && c_result != "void" {
        let _ = writeln!(output, "    {c_result} value{{}};");
        call_arguments.insert(1, "&value".to_owned());
    }
    call_arguments.extend(arguments);
    let symbol = if trap_return {
        format!("tsuzuri_try_{name}")
    } else {
        format!("tz_{name}")
    };
    let call = format!("::{symbol}({})", call_arguments.join(", "));
    let value = if trap_return {
        let _ = writeln!(output, "    detail::check({call}, trap);");
        "value".to_owned()
    } else if slot.is_some() || c_result == "void" {
        let _ = writeln!(output, "    {call};");
        String::new()
    } else {
        call
    };
    let returned_value = match &result {
        Shape::Unit => None,
        Shape::Owned(buffer) => Some(match buffer {
            Buffer::String => "{reinterpret_cast<char16_t *>(out.ptr), out.len}".to_owned(),
            Buffer::Utf8String => "{reinterpret_cast<char *>(out.ptr), out.len}".to_owned(),
            _ => "{out.ptr, out.len}".to_owned(),
        }),
        Shape::Record(_) => Some("out".to_owned()),
        Shape::Scalar(Scalar::Bool) => Some(format!("{value} != 0")),
        Shape::Scalar(scalar) if scalar.normalized() => {
            Some(format!("static_cast<{}>({value})", scalar.cpp()))
        }
        Shape::Scalar(_) | Shape::Handle(_) => Some(value),
        Shape::Slice(_) => unreachable!("export results are checked"),
    };
    if let Some(returned_value) = returned_value {
        let _ = writeln!(output, "    return {returned_value};");
    }
    output.push_str("}\n\n");
}

/// The C type of a scalar result in the header: bool and 8/16-bit integers are 32-bit.
fn c_scalar(scalar: Scalar) -> String {
    match scalar {
        Scalar::I8 | Scalar::I16 | Scalar::I32 | Scalar::Bool => "std::int32_t",
        Scalar::U8 | Scalar::U16 | Scalar::U32 => "std::uint32_t",
        Scalar::I64 => "std::int64_t",
        Scalar::U64 => "std::uint64_t",
        Scalar::F32 => "float",
        Scalar::F64 => "double",
    }
    .to_owned()
}
