use std::path::Path;

use serde_json::json;
use tsuzuri::bindings;
use tsuzuri::check::CheckedModule;

fn fixture() -> CheckedModule {
    tsuzuri::driver::Project::load(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/bindings/Main.tz"),
    )
    .unwrap()
    .analyze()
    .unwrap()
}

#[test]
fn bindings_table_lists_sorted_exports_imports_and_records() {
    let table = bindings::table(&fixture());
    assert_eq!(table["abi"], json!(1));
    assert_eq!(table["hostAbi"], json!(true));
    let exports: Vec<_> = table["exports"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry[0].as_str().unwrap())
        .collect();
    assert_eq!(
        exports,
        [
            "add",
            "add_u64",
            "area",
            "callback_trap",
            "callbacks",
            "checksum",
            "copy_text",
            "copy_utf8",
            "copy_values",
            "counters",
            "deep",
            "divide",
            "greeting",
            "make_bytes",
            "make_window",
            "mix",
            "negate",
            "note",
            "pass_through",
            "peek",
            "relay_fail",
            "relay_flags",
            "scaled",
            "stamp",
            "sum_float",
            "twice32",
            "update",
            "visit",
            "widen",
            "window_total",
        ]
    );
    let export = |name: &str| {
        table["exports"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry[0] == name)
            .cloned()
            .unwrap()
    };
    assert_eq!(
        export("widen"),
        json!(["widen", ["i8", "i16u", "i32u"], "i64"])
    );
    assert_eq!(
        export("copy_values"),
        json!(["copy_values", ["slice:i64"], "buffer:i64"])
    );
    assert_eq!(
        export("copy_text"),
        json!(["copy_text", ["slice:string"], "buffer:string"])
    );
    assert_eq!(
        export("update"),
        json!([
            "update",
            ["record:tz_record_4Main_6Sample"],
            "record:tz_record_4Main_6Sample"
        ])
    );
    assert_eq!(
        export("pass_through"),
        json!([
            "pass_through",
            ["handle:tz_handle_4Main_7Counter"],
            "handle:tz_handle_4Main_7Counter"
        ])
    );
    assert_eq!(export("negate"), json!(["negate", ["bool"], "bool"]));
    assert_eq!(export("twice32"), json!(["twice32", ["f32"], "f32"]));
    // Byte order puts the `Main.` imports before the explicit symbols; `unit` has no parameter and the
    // unreachable `unused` extern is left out.
    assert_eq!(
        table["imports"],
        json!([
            ["Main.host_fail", "tsuzuri", ["i64"], "i64"],
            [
                "Main.host_flags",
                "tsuzuri",
                ["record:tz_record_4Main_5Flags"],
                "record:tz_record_4Main_6Sample"
            ],
            [
                "Main.host_greeting",
                "tsuzuri",
                ["slice:utf8string"],
                "buffer:utf8string"
            ],
            [
                "Main.host_mix",
                "tsuzuri",
                ["i32u", "i64u", "bool", "i16"],
                "i64u"
            ],
            ["Main.host_note", "tsuzuri", ["slice:string"], "unit"],
            [
                "Main.host_scale",
                "tsuzuri",
                ["slice:f64", "f64"],
                "buffer:f64"
            ],
            ["Main.now", "tsuzuri", [], "i64"],
            [
                "e13_apply",
                "env",
                [["callback", ["i64"], "i64"], "i64"],
                "i64"
            ],
            [
                "e13_counter_add",
                "tsuzuri",
                ["handle:tz_handle_4Main_7Counter", "i64"],
                "i64"
            ],
            [
                "e13_counter_free",
                "tsuzuri",
                ["handle:tz_handle_4Main_7Counter"],
                "i64"
            ],
            [
                "e13_counter_new",
                "tsuzuri",
                ["i64"],
                "handle:tz_handle_4Main_7Counter"
            ],
            [
                "e13_visit",
                "env",
                [
                    [
                        "callback",
                        ["handle:tz_handle_4Main_7Counter", "i8"],
                        "bool"
                    ],
                    "handle:tz_handle_4Main_7Counter"
                ],
                "i64"
            ]
        ])
    );
    // The natural C layout of the normalized fields, worked out by hand: narrow integers and bool
    // take 4 bytes, i64/f64 8 bytes aligned to 8, a C array its elements, and the size rounds up to
    // the alignment.
    assert_eq!(
        table["records"],
        json!({
            "tz_record_4Main_5Flags": {
                "size": 12,
                "align": 4,
                "fields": [["tiny", 0, "i8"], ["wide", 4, "i16u"], ["flag", 8, "bool"]]
            },
            "tz_record_4Main_6Sample": {
                "size": 24,
                "align": 8,
                "fields": [["flags", 0, "record:tz_record_4Main_5Flags"], ["amount", 16, "f64"]]
            },
            "tz_record_4Main_6Window": {
                "size": 32,
                "align": 8,
                "fields": [["values", 0, ["array", "i32", 3]], ["scale", 16, "f64"], ["mark", 24, "i8u"]]
            },
            "tz_record_8Geometry_5Point": {
                "size": 16,
                "align": 8,
                "fields": [["x", 0, "f64"], ["y", 8, "f64"]]
            }
        })
    );
}

#[test]
fn bindings_are_deterministic() {
    let module = fixture();
    let javascript = bindings::javascript(&module);
    let declarations = bindings::declarations(&module);
    assert_eq!(javascript, bindings::javascript(&fixture()));
    assert_eq!(declarations, bindings::declarations(&fixture()));
    let banner = format!(
        "// Generated by Tsuzuri {}. Bindings ABI 1. Do not edit.\n",
        env!("CARGO_PKG_VERSION")
    );
    assert!(javascript.starts_with(&format!("{banner}const TABLE = {{\"abi\":1,")));
    assert!(declarations.starts_with(&banner));
    // The runtime follows the table and exports only `load` and `TsuzuriTrap`.
    for flavor in [bindings::JsFlavor::Single, bindings::JsFlavor::Threads] {
        let javascript = bindings::javascript_for(&module, flavor);
        assert_eq!(javascript, bindings::javascript_for(&fixture(), flavor));
        assert_eq!(
            bindings::declarations_for(&module, flavor),
            bindings::declarations_for(&fixture(), flavor)
        );
        let exported: Vec<_> = javascript
            .lines()
            .filter(|line| line.starts_with("export "))
            .collect();
        assert_eq!(
            exported,
            [
                "export async function load(source, options = {}) {",
                "export { TsuzuriTrap };"
            ],
            "{flavor:?}"
        );
        assert!(!javascript.contains("node:") && !javascript.contains("fetch("));
        assert!(!javascript.lines().any(|line| line.starts_with("import ")));
        assert!(javascript.starts_with(&format!("{banner}const TABLE = {{\"abi\":1,")));
    }
    assert!(
        bindings::javascript_for(&module, bindings::JsFlavor::Threads)
            .contains("crossOriginIsolated")
    );
    assert!(!javascript.contains("crossOriginIsolated"));
}

#[test]
fn declarations_snapshot_for_small_module() {
    let module = tsuzuri::analyze(
        "record Point { x: f64, y: f64 }\n\
         extern def now :: unit -> i64\n\
         export def add :: i64 -> i64 -> i64\n\
         fn add a b = a + b + now ()\n\
         export def norm :: ref Point -> f64\n\
         fn norm point = point.x * point.x + point.y * point.y\n\
         export def tag :: ref [ubyte] -> bool -> string\n\
         fn tag bytes flag = if flag then to_string bytes.length else \"\"\n",
    )
    .unwrap();
    let banner = format!(
        "// Generated by Tsuzuri {}. Bindings ABI 1. Do not edit.\n",
        env!("CARGO_PKG_VERSION")
    );
    let expected = banner
        + r#"export interface tz_record_4Main_5Point { x: number; y: number }
export interface Exports {
  add(arg0: bigint, arg1: bigint): bigint;
  norm(arg0: tz_record_4Main_5Point): number;
  tag(arg0: Uint8Array | Borrowed<Uint8Array>, arg1: boolean): string;
}
export interface Imports {
  "Main.now"(): bigint;
}
export interface TrapInfo { reason: "trap" | "stack"; site: number; kind?: string; path?: string; line?: number; column?: number }
export interface TrapSite { id: number; kind: string; path: string; span: { line: number; column: number } }
export declare class TsuzuriTrap extends Error { readonly trap: TrapInfo }
export interface Borrowed<T> { readonly length: number; view(): T }
export interface Bindings {
  readonly exports: Exports;
  withBorrowed<R>(kind: "i64", length: number, callback: (buffer: Borrowed<BigInt64Array>) => R): R;
  withBorrowed<R>(kind: "f64", length: number, callback: (buffer: Borrowed<Float64Array>) => R): R;
  withBorrowed<R>(kind: "ubyte", length: number, callback: (buffer: Borrowed<Uint8Array>) => R): R;
  ready(): Promise<void>;
}
export declare function load(source: ArrayBuffer | ArrayBufferView | WebAssembly.Module, options: { imports: Imports; sites?: readonly TrapSite[] }): Promise<Bindings>;
"#;
    assert_eq!(bindings::declarations(&module), expected);
    // Without imports the options are optional and `Imports` is empty.
    let scalar = tsuzuri::analyze("export def one :: i64\nfn one = 1\n").unwrap();
    let declarations = bindings::declarations(&scalar);
    assert!(declarations.contains("export interface Imports {}\n"));
    assert!(declarations.ends_with(
        "export declare function load(source: ArrayBuffer | ArrayBufferView | WebAssembly.Module, options?: { sites?: readonly TrapSite[] }): Promise<Bindings>;\n"
    ));
    assert_eq!(bindings::table(&scalar)["hostAbi"], json!(false));
    // The thread pool's calls return promises and take no Borrowed buffers.
    let threads = bindings::declarations_for(&module, bindings::JsFlavor::Threads);
    assert!(threads.contains("  add(arg0: bigint, arg1: bigint): Promise<bigint>;\n"));
    assert!(threads.contains("  tag(arg0: Uint8Array, arg1: boolean): Promise<string>;\n"));
    assert!(!threads.contains("Borrowed"));
    assert!(threads.contains(
        "export type CreateImports = (context: ImportsContext) => Imports | Promise<Imports>;\n"
    ));
    assert!(threads.ends_with(
        "export declare function load(source: ArrayBuffer | ArrayBufferView | WebAssembly.Module, options: { importsModule: string | URL; importData?: unknown; workers?: number; memory?: WebAssembly.Memory; sites?: readonly TrapSite[] }): Promise<ThreadBindings>;\n"
    ));
}

fn native_fixture() -> CheckedModule {
    tsuzuri::driver::Project::load(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/bindings_native/Main.tz"),
    )
    .unwrap()
    .analyze()
    .unwrap()
}

#[test]
fn native_bindings_are_deterministic_and_follow_the_c_abi() {
    let module = native_fixture();
    for trap in [false, true] {
        assert_eq!(
            bindings::csharp(&module, "native", trap),
            bindings::csharp(&native_fixture(), "native", trap)
        );
        assert_eq!(
            bindings::python(&module, "native", trap),
            bindings::python(&native_fixture(), "native", trap)
        );
        assert_eq!(
            bindings::cpp(&module, "native", trap),
            bindings::cpp(&native_fixture(), "native", trap)
        );
    }
    let banner = format!(
        "Generated by Tsuzuri {}. Bindings ABI 1. Do not edit.\n",
        env!("CARGO_PKG_VERSION")
    );
    // C#: LibraryImport over the header's C types, spans for borrowed inputs, SafeHandle results,
    // and records in the normalized C layout with typed properties for the 32-bit fields.
    let csharp = bindings::csharp(&module, "native", false);
    assert!(csharp.starts_with(&format!("// {banner}")));
    for line in [
        "public static unsafe partial class Native",
        "    public const string LibraryName = \"native\";",
        "        internal static partial long tz_add(long arg0, long arg1);",
        "        internal static partial long tz_widen(int arg0, uint arg1, uint arg2);",
        "        internal static partial void tz_copy_values(Descriptor* @out, long* arg0_ptr, long arg0_len);",
        "        internal static partial void tz_update(tz_record_4Main_6Sample* @out, tz_record_4Main_6Sample* arg0);",
        "        internal static partial nint tz_pass_through(nint arg0);",
        "        internal static partial void tsuzuri_free(nint ptr);",
        "    public static OwnedBuffer<long> copy_values(ReadOnlySpan<long> arg0)",
        "    public static OwnedString copy_text(ReadOnlySpan<char> arg0)",
        "    public static OwnedUtf8String copy_utf8(ReadOnlySpan<byte> arg0)",
        "    public static tz_record_4Main_6Sample update(in tz_record_4Main_6Sample arg0)",
        "    public static long peek(tz_handle_4Main_7Counter arg0)",
        "        public sbyte tiny { readonly get => (sbyte)tiny__abi; set => tiny__abi = value; }",
        "        public bool flag { readonly get => flag__abi != 0; set => flag__abi = value ? 1 : 0; }",
        "        public Fixed3_Int32 values;",
        "    [InlineArray(3)]",
        "    public readonly record struct tz_handle_4Main_7Counter(nint Value);",
        // The copies hold a reference to the handle, so neither Dispose nor the finalizer of an
        // unreachable result frees the memory under them.
        "                DangerousAddRef(ref added);",
        "                    DangerousRelease();",
        "        public T[] ToArray() => Read(static buffer => buffer.Elements.ToArray());",
        "        public override string ToString() => Read(static buffer => new string(buffer.Elements));",
        "        public override string ToString() => Read(static buffer => System.Text.Encoding.UTF8.GetString(buffer.Elements));",
    ] {
        assert!(csharp.contains(&format!("{line}\n")), "{line}");
    }
    assert!(!csharp.contains("TsuzuriTrapException"));
    assert!(!csharp.contains("Span.ToArray()") && !csharp.contains("(Span)"));
    let trapping = bindings::csharp(&module, "native_trap", true);
    assert!(trapping.contains("public static unsafe partial class NativeTrap\n"));
    assert!(trapping.contains(
        "        internal static partial int tsuzuri_try_add(TrapInfo* trap, long* result, long arg0, long arg1);\n"
    ));
    assert!(trapping.contains(
        "        internal static partial int tsuzuri_try_copy_values(TrapInfo* trap, Descriptor* @out, long* arg0_ptr, long arg0_len);\n"
    ));
    assert!(trapping.contains("\"integer division by zero\""));
    // Python: ctypes argtypes of the same C types, and records after the records of their fields.
    let python = bindings::python(&module, "native", false);
    assert!(python.starts_with(&format!("# {banner}")));
    for line in [
        "LIBRARY_NAME = \"native\"",
        "        library.tz_add.argtypes = (_ctypes.c_int64, _ctypes.c_int64)",
        "        library.tz_widen.argtypes = (_ctypes.c_int32, _ctypes.c_uint32, _ctypes.c_uint32)",
        "        library.tz_widen.restype = _ctypes.c_int64",
        "        library.tz_copy_text.argtypes = (_ctypes.POINTER(_Buffer), _ctypes.POINTER(_ctypes.c_uint16), _ctypes.c_int64)",
        "        library.tz_pass_through.restype = _ctypes.c_void_p",
        "    _fields_ = [(\"f0\", _ctypes.c_int32 * 3), (\"f1\", _ctypes.c_double), (\"f2\", _ctypes.c_uint32)]",
    ] {
        assert!(python.contains(&format!("{line}\n")), "{line}");
    }
    assert!(
        python.find("class tz_record_4Main_5Flags:").unwrap()
            < python.find("class tz_record_4Main_6Sample:").unwrap()
    );
    let trapping = bindings::python(&module, "native", true);
    assert!(trapping.contains("        library.tsuzuri_try_add.argtypes = (_ctypes.POINTER(_Trap), _ctypes.POINTER(_ctypes.c_int64), _ctypes.c_int64, _ctypes.c_int64)\n"));
    assert!(trapping.contains("class TsuzuriTrap(Exception):\n"));
    // C++: inline functions over the C header of the same stem, with RAII results.
    let cpp = bindings::cpp(&module, "native", false);
    assert!(cpp.starts_with(&format!("// {banner}")));
    for line in [
        "#include \"native.h\"",
        "namespace tsuzuri::native {",
        "inline std::int64_t add(std::int64_t arg0, std::int64_t arg1) {",
        "inline std::int8_t narrow(std::int64_t arg0) {",
        "inline buffer<std::int64_t> copy_values(std::span<const std::int64_t> arg0) {",
        "inline string_buffer copy_text(std::u16string_view arg0) {",
        "inline utf8string_buffer copy_utf8(std::string_view arg0) {",
        "inline tz_record_4Main_6Sample update(const tz_record_4Main_6Sample &arg0) {",
        "    return static_cast<std::int8_t>(::tz_narrow(arg0));",
    ] {
        assert!(cpp.contains(&format!("{line}\n")), "{line}");
    }
    let trapping = bindings::cpp(&module, "native", true);
    assert!(
        trapping
            .contains("    detail::check(::tsuzuri_try_add(&trap, &value, arg0, arg1), trap);\n")
    );
}

#[test]
fn native_bindings_escape_names_of_each_language() {
    let module = tsuzuri::analyze(
        "record Pair { import: i64, object: i8 }\n\
         export def delete :: ref Pair -> i64\n\
         fn delete pair = pair.import + pair.object as i64\n\
         export def lambda_ :: i64 -> i64\n\
         fn lambda_ value = value\n",
    )
    .unwrap();
    let csharp = bindings::csharp(&module, "9 lives", false);
    assert!(csharp.contains("public static unsafe partial class Library9Lives\n"));
    assert!(csharp.contains("    public const string LibraryName = \"9 lives\";\n"));
    assert!(csharp.contains("        public long import;\n"));
    assert!(csharp.contains(
        "        public sbyte @object { readonly get => (sbyte)object__abi; set => object__abi = value; }\n"
    ));
    let python = bindings::python(&module, "pairs", false);
    assert!(python.contains("    import_: int\n"));
    assert!(python.contains("    def lambda__(self, arg0):\n"));
    let cpp = bindings::cpp(&module, "pairs", false);
    assert!(cpp.contains("inline std::int64_t delete_(const tz_record_4Main_4Pair &arg0) {\n"));
}
