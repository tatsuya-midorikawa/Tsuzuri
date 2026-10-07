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
    let exported: Vec<_> = javascript
        .lines()
        .filter(|line| line.starts_with("export "))
        .collect();
    assert_eq!(
        exported,
        [
            "export class TsuzuriTrap extends Error {",
            "export async function load(source, options = {}) {"
        ]
    );
    assert!(!javascript.contains("node:") && !javascript.contains("fetch("));
    assert!(!javascript.lines().any(|line| line.starts_with("import ")));
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
}
