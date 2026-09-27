use tsuzuri::analyze;

#[test]
fn host_buffers_and_records_roundtrip_on_native_and_wasm() {
    use std::{
        fs,
        path::Path,
        process::Command,
        time::{SystemTime, UNIX_EPOCH},
    };
    let root = std::env::temp_dir().join(format!(
        "tsuzuri-host-abi-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&root).unwrap();
    let project = tsuzuri::driver::Project::load(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/host_abi/Main.tz"),
    )
    .unwrap();
    let module = project.analyze().unwrap();
    fs::write(root.join("api.h"), tsuzuri::llvm::header(&module)).unwrap();
    let native_ir = root.join("native.ll");
    let ir = tsuzuri::llvm::emit_target(&module, tsuzuri::llvm::Entry::Library, false).unwrap();
    fs::write(
        &native_ir,
        ir.replace("@malloc", "@tracked_alloc")
            .replace("@free", "@tracked_free"),
    )
    .unwrap();
    let host = root.join("host.c");
    fs::write(&host, r#"
#include <assert.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include "api.h"
static uint64_t live, allocations;
void *tracked_alloc(uint64_t size) {
    uint64_t *memory = malloc(size + 16);
    assert(memory);
    memory[0] = size;
    memory[1] = 0x71A11;
    live += size;
    ++allocations;
    return memory + 2;
}
void tracked_free(void *pointer) {
    if (!pointer) return;
    uint64_t *memory = (uint64_t *)pointer - 2;
    assert(memory[1] == 0x71A11 && live >= memory[0]);
    live -= memory[0];
    free(memory);
}
int main(int argc, char **argv) {
    int64_t values[] = { 1, 2, 3 };
    uint16_t text[] = { 65, 0, 0xD800, 0xD83D, 0xDE00 };
    uint8_t utf8[] = { 65, 0, 0xF0, 0x9F, 0x98, 0x80 };
    tz_record_4Main_5Outer outer = { { 255, 65537, 9 }, 1.5 }, result;
    if (argc > 1) {
        switch (argv[1][0]) {
            case '0': (void)tz_sum(values, -1); break;
            case '1': (void)tz_sum(NULL, 1); break;
            case '2': tz_make_bytes(NULL, 3); break;
            case '3': tz_update(&result, NULL); break;
            case '4': (void)tz_sum((const int64_t *)((const char *)values + 1), 1); break;
            case '5': (void)tz_byte_length((const uint8_t *)"\xED\xA0\x80", 3); break;
            case '6': (void)tsuzuri_alloc(-1); break;
            case '7': (void)tz_sum(values, INT64_MAX); break;
        }
        return 0;
    }
    assert(tz_sum(values, 3) == 6 && tz_sum(NULL, 0) == 0);
    double decimals[] = { 1.5, 2.5 };
    assert(tz_sum_float(decimals, 2) == 4.0);
    assert(tz_checksum(utf8, 6) == 744);
    assert(tz_text_length(text, 5) == 5 && tz_byte_length(utf8, 6) == 6);
    assert(allocations == 0 && live == 0);
    tsuzuri_i64_buffer copied;
    tz_copy_values(&copied, values, 3);
    assert(copied.len == 3 && copied.ptr != values && copied.ptr[2] == 3);
    values[2] = 99;
    assert(copied.ptr[2] == 3);
    tsuzuri_free(copied.ptr);
    tsuzuri_string_buffer copied_text;
    tz_copy_text(&copied_text, text, 5);
    assert(copied_text.len == 5 && memcmp(copied_text.ptr, text, sizeof(text)) == 0);
    tsuzuri_free(copied_text.ptr);
    tsuzuri_utf8string_buffer copied_utf8;
    tz_copy_utf8(&copied_utf8, utf8, 6);
    assert(copied_utf8.len == 6 && memcmp(copied_utf8.ptr, utf8, 6) == 0);
    tsuzuri_free(copied_utf8.ptr);
    tsuzuri_ubyte_buffer bytes;
    for (int index = 0; index < 1024; ++index) {
        tz_make_bytes(&bytes, 257);
        assert(bytes.len == 257 && bytes.ptr[255] == 255 && bytes.ptr[256] == 0);
        tsuzuri_free(bytes.ptr);
        assert(live == 0);
    }
    tz_update(&result, &outer);
    assert(result.inner.tiny == -1 && result.inner.wide == 1 && result.inner.flag == 1 && result.amount == 2.5);
    assert(tz_owned_record(&outer) == 0.5);
    tz_record_4Main_5Empty empty = { 7 }, empty_out;
    tz_empty_record(&empty_out, &empty);
    assert(empty_out.tz_empty == 0);
    void *allocated = tsuzuri_alloc(0);
    assert(allocated);
    tsuzuri_free(allocated);
    tsuzuri_free(NULL);
    assert(live == 0);
    return 0;
}
"#).unwrap();
    let script = root.join("host.mjs");
    fs::write(&script, r#"
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
const module = new WebAssembly.Module(readFileSync(process.argv[2]));
assert.deepEqual(WebAssembly.Module.imports(module), []);
const api = new WebAssembly.Instance(module).exports;
const out = api.tsuzuri_alloc(32n);
const input = api.tsuzuri_alloc(32n);
new BigInt64Array(api.memory.buffer, input, 3).set([1n, 2n, 3n]);
assert.equal(api.tz_sum(input, 3n), 6n);
assert.equal(api.tz_sum(0, 0n), 0n);
api.tz_copy_values(out, input, 3n);
let view = new DataView(api.memory.buffer);
let pointer = view.getUint32(out, true), length = view.getBigInt64(out + 8, true);
assert.equal(length, 3n);
assert.deepEqual([...new BigInt64Array(api.memory.buffer, pointer, 3)], [1n, 2n, 3n]);
api.tsuzuri_free(pointer);
new Uint16Array(api.memory.buffer, input, 5).set([65, 0, 0xd800, 0xd83d, 0xde00]);
api.tz_copy_text(out, input, 5n);
view = new DataView(api.memory.buffer);
pointer = view.getUint32(out, true);
assert.equal(view.getBigInt64(out + 8, true), 5n);
assert.deepEqual([...new Uint16Array(api.memory.buffer, pointer, 5)], [65, 0, 0xd800, 0xd83d, 0xde00]);
api.tsuzuri_free(pointer);
const bytes = [65, 0, 0xf0, 0x9f, 0x98, 0x80];
new Uint8Array(api.memory.buffer, input, bytes.length).set(bytes);
api.tz_copy_utf8(out, input, BigInt(bytes.length));
view = new DataView(api.memory.buffer);
pointer = view.getUint32(out, true);
assert.deepEqual([...new Uint8Array(api.memory.buffer, pointer, bytes.length)], bytes);
api.tsuzuri_free(pointer);
view = new DataView(api.memory.buffer);
view.setInt32(input, 255, true); view.setUint32(input + 4, 65537, true); view.setInt32(input + 8, 9, true); view.setFloat64(input + 16, 1.5, true);
api.tz_update(out, input);
view = new DataView(api.memory.buffer);
assert.equal(view.getInt32(out, true), -1); assert.equal(view.getUint32(out + 4, true), 1); assert.equal(view.getInt32(out + 8, true), 1); assert.equal(view.getFloat64(out + 16, true), 2.5);
for (const count of [0n, 257n, 1000000n, 1000000n]) {
  api.tz_make_bytes(out, count);
  view = new DataView(api.memory.buffer);
  pointer = view.getUint32(out, true);
  assert.equal(view.getBigInt64(out + 8, true), count);
  if (count > 0) assert.equal(new Uint8Array(api.memory.buffer)[pointer + Number(count) - 1], Number((count - 1n) & 255n));
  api.tsuzuri_free(pointer);
}
for (const invoke of [()=>api.tz_sum(input,-1n),()=>api.tz_sum(0,1n),()=>api.tz_make_bytes(0,3n),()=>api.tz_update(out,0),()=>api.tz_sum(input+1,1n),()=>api.tsuzuri_alloc(-1n),()=>api.tz_sum(input,9223372036854775807n),()=>api.tz_sum(0xfffffff8,1n)]) assert.throws(invoke,WebAssembly.RuntimeError);
for (const invalid of [[0x80],[0xc0,0x80],[0xed,0xa0,0x80],[0xf4,0x90,0x80,0x80],[0xe0,0xa0]]) {
  new Uint8Array(api.memory.buffer, input, invalid.length).set(invalid);
  assert.throws(()=>api.tz_byte_length(input,BigInt(invalid.length)),WebAssembly.RuntimeError);
}
api.tsuzuri_free(input); api.tsuzuri_free(out); api.tsuzuri_free(0);
assert.ok(api.memory.buffer.byteLength <= 16 * 1024 * 1024);
"#).unwrap();
    let clang = std::env::var_os("TSUZURI_CLANG").unwrap_or_else(|| "clang".into());
    let cpp = root.join("header.cpp");
    fs::write(&cpp, "#include \"api.h\"\nstatic_assert(sizeof(tz_record_4Main_5Outer) == 24);\nstatic_assert(sizeof(tsuzuri_i64_buffer) == 16);\nint main() { return 0; }\n").unwrap();
    let checked_header = Command::new(&clang)
        .args([
            "--driver-mode=g++",
            "-std=c++20",
            "-Wall",
            "-Wextra",
            "-Werror",
            "-fsyntax-only",
        ])
        .arg(&cpp)
        .output()
        .unwrap();
    assert!(
        checked_header.status.success(),
        "{}",
        String::from_utf8_lossy(&checked_header.stderr)
    );
    for optimization in [0, 3] {
        let executable = root.join(format!("native-{optimization}"));
        let output = Command::new(&clang)
            .args([
                "-std=c11",
                "-Wno-override-module",
                "-Wall",
                "-Wextra",
                "-Werror",
                &format!("-O{optimization}"),
            ])
            .arg(&host)
            .arg(&native_ir)
            .args(["-lm", "-o"])
            .arg(&executable)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let output = Command::new(&executable).output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        for index in 0..8 {
            assert!(
                !Command::new(&executable)
                    .arg(index.to_string())
                    .output()
                    .unwrap()
                    .status
                    .success(),
                "trap {index}"
            );
        }
        let wasm = root.join(format!("module-{optimization}.wasm"));
        tsuzuri::driver::build(
            &module,
            &project,
            &wasm,
            tsuzuri::driver::BuildOptions {
                target: tsuzuri::driver::Target::Wasm32,
                emit: tsuzuri::driver::Emit::Wasm,
                optimization,
                trap_info: optimization == 3,
                ..Default::default()
            },
        )
        .unwrap();
        let output = Command::new("node")
            .arg(&script)
            .arg(&wasm)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn host_header_and_wrappers_are_deterministic_and_normalized() {
    let source = "record Inner { small: i8, flag: bool }\nrecord Outer { inner: Inner, restrict: i64 }\n\
        export def sum :: ref [i64] -> i64\nfn sum values = Array.sum values\n\
        export def buffer :: i64 -> [ubyte]\nfn buffer count = new [ubyte](count, index -> index as ubyte)\n\
        export def text :: ref string -> string\nfn text value = clone_string value\n\
        export def record :: ref Outer -> Outer\nfn record value = deref value";
    let source = source
        .replace("def record ::", "def roundtrip ::")
        .replace("fn record value", "fn roundtrip value");
    let module = analyze(&source).unwrap();
    let header = tsuzuri::llvm::header(&module);
    assert!(header.contains("int64_t tz_sum(const int64_t *arg0_ptr, int64_t arg0_len)"));
    assert!(header.contains("void tz_buffer(tsuzuri_ubyte_buffer *out, int64_t arg0)"));
    assert!(header.contains(
        "void tz_text(tsuzuri_string_buffer *out, const uint16_t *arg0_ptr, int64_t arg0_len)"
    ));
    assert!(header.contains("tz_record_4Main_5Inner"));
    assert!(header.contains("int32_t small;") && header.contains("int32_t flag;"));
    assert!(header.contains("int64_t tz_restrict;"));
    assert!(header.contains("const tz_record_4Main_5Outer *arg0"));
    assert!(header.contains("void *tsuzuri_alloc(int64_t size)"));
    for wasm in [false, true] {
        let ir = tsuzuri::llvm::emit_target(&module, tsuzuri::llvm::Entry::Library, wasm).unwrap();
        assert_eq!(
            ir,
            tsuzuri::llvm::emit_target(&module, tsuzuri::llvm::Entry::Library, wasm).unwrap()
        );
        assert!(ir.contains("define i64 @tz_sum(ptr %arg0_ptr, i64 %arg0_len)"));
        assert!(ir.contains("define void @tz_buffer(ptr %out, i64 %arg0)"));
        assert!(ir.contains("define weak ptr @tsuzuri_alloc"));
    }
}

#[test]
fn record_header_names_keep_module_boundaries_and_type_arguments() {
    let module = tsuzuri::analyze_modules(&[
        ("A_B.tz", "record C { value: i64 }\nexport def first :: C -> C\nfn first value = value"),
        ("A.tz", "record B_C { value: i64 }\nrecord Pair<'a> { value: 'a }\nexport def second :: B_C -> B_C\nfn second value = value\nexport def pair :: Pair<i64> -> Pair<i64>\nfn pair value = value"),
    ]).unwrap();
    let header = tsuzuri::llvm::header(&module);
    assert!(header.contains("tz_record_3A_B_1C"));
    assert!(header.contains("tz_record_1A_3B_C"));
    assert!(header.contains("tz_record_1A_4Pair_T3_693634"));
    for wasm in [false, true] {
        tsuzuri::llvm::emit_target(&module, tsuzuri::llvm::Entry::Library, wasm).unwrap();
    }
}

#[test]
fn classifies_borrowed_buffers_owned_results_and_scalar_records() {
    for source in [
        "export def sum :: ref [i64] -> i64\nfn sum values = Array.sum values",
        "export def mean :: ref [f64] -> f64\nfn mean values = Array.sum values / values.length as f64",
        "export def bytes :: i64 -> [ubyte]\nfn bytes count = new [ubyte](count, index -> index as ubyte)",
        "export def text :: ref string -> string\nfn text value = clone_string value",
        "export def text :: ref utf8string -> utf8string\nfn text value = Utf8String.clone value",
        "record Inner { small: i8, flag: bool }\nrecord Outer { inner: Inner, count: i64 }\nexport def update :: ref Outer -> Outer\nfn update value = { deref value with count = value.count + 1 }",
        "record Pair<'a> { first: 'a, second: 'a }\nexport def pair :: Pair<i64> -> Pair<i64>\nfn pair value = value",
    ] {
        analyze(source).unwrap_or_else(|error| panic!("{source}\n{error:?}"));
    }
    for source in [
        "export def bad :: [i64] -> i64\nfn bad values = values.length",
        "export def bad :: ref mut [i64] -> unit\nfn bad values = ()",
        "export def bad :: ref [i32] -> i64\nfn bad values = values.length",
        "export def bad :: [i32]\nfn bad = [1i32]",
        "export def bad :: ref [string] -> i64\nfn bad values = values.length",
        "export def bad :: ref i64 -> i64\nfn bad value = deref value",
        "record Owned { text: string }\nexport def bad :: Owned -> i64\nfn bad value = value.text.length",
        "record Collision { restrict: i64, tz_restrict: i64 }\nexport def bad :: Collision -> i64\nfn bad value = value.restrict",
    ] {
        assert_eq!(analyze(source).unwrap_err().code, "E1008", "{source}");
    }
}
