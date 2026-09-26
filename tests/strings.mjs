import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const compiler = resolve(process.argv[2] ?? join(root, "target/debug/tsuzuri"));
const clang = process.env.TSUZURI_CLANG ?? "clang";
const fixture = join(root, "tests/fixtures/strings");
const scratch = join(root, "target", `strings-tests-${process.pid}`);
mkdirSync(scratch, { recursive: true });
const environment = { ...process.env, TMPDIR: scratch };
function execute(program, args, success = true, encoding = "utf8") {
  const result = spawnSync(program, args, {
    cwd: root, env: environment, encoding, timeout: 180_000, maxBuffer: 8 * 1024 * 1024,
  });
  if (result.error) throw result.error;
  if (success) assert.equal(result.status, 0, `${program} ${args.join(" ")}\n${result.stdout}\n${result.stderr}`);
  return result;
}
const cli = (args, success, encoding) => execute(compiler, args, success, encoding);
const cValue = (value) => typeof value === "bigint" ? `${value}LL` : String(value);

// These are ECMAScript string values, not Unicode-scalar iteration.
const samples = [
  "", "A", "é", "e\u0301", "日本語", "😀", "\uD83D\uDE00", "\u{1f600}",
  "\0A\0", "\uD800", "\uDC00", "\uDC00\uD800", "\uD800A\uDC00",
  "\uD800\uD800\uDC00\uDC00", "\u{d800}", "\uD7FF", "\uE000", "\uFFFF",
  "\u{10FFFF}", "\u{10000}", "\uFFFD", "a", "aa", "a\0", "\\\"\n\r\t\0",
  "\u0001\u007F\u0080\u07FF\u0800",
  "\u{000000000000000000001f600}", "\u{00000000000000000000D800}",
  "\u{000000000000000000000000}", "\u{00000000000000000010ffff}", "\uD801", "\uDC01",
];
const legacy = Buffer.from("\0aé日本語😀\"\\\n\r\t", "utf8");
const units = (text) => Array.from({ length: text.length }, (_, index) => text.charCodeAt(index));
const weighted = (values) => BigInt(values.reduce((sum, value, index) => sum + value * (index + 1), 0));
const comparison = (a, b) => (a === b ? 1n : 0n) | (a !== b ? 2n : 0n) |
  (a < b ? 4n : 0n) | (a <= b ? 8n : 0n) | (a > b ? 16n : 0n) | (a >= b ? 32n : 0n);
const cases = [];
const traps = [];
for (const [index, text] of samples.entries()) {
  const id = BigInt(index);
  const repaired = text.toWellFormed();
  cases.push(
    ["length", [id], BigInt(text.length)],
    ["module_length", [id], BigInt(text.length)],
    ["utf8_module_length", [id], BigInt(Buffer.byteLength(repaired, "utf8"))],
    ["iterated", [id], weighted(units(text))],
    ["displayed", [id], BigInt(text.length * 2)],
    ["well_formed", [id], Number(text.isWellFormed())],
    ["repaired_length", [id], BigInt(repaired.length)],
  );
  for (let offset = 0; offset < text.length; ++offset) {
    cases.push(["unit_at", [id, BigInt(offset)], text.charCodeAt(offset)]);
    cases.push(["display_unit", [id, BigInt(offset)], text.charCodeAt(offset)]);
  }
  for (let offset = 0; offset < repaired.length; ++offset) {
    cases.push(["repaired_unit", [id, BigInt(offset)], repaired.charCodeAt(offset)]);
  }
  traps.push(["unit_at", [id, -1n]], ["unit_at", [id, BigInt(text.length)]]);
  if (text.isWellFormed()) {
    const encoded = Buffer.from(text, "utf8");
    cases.push(["round_trip", [id], BigInt(encoded.length)]);
    for (const [offset, byte] of encoded.entries()) {
      cases.push(["encoded_byte", [id, BigInt(offset)], byte]);
    }
  } else {
    traps.push(["round_trip", [id]]);
  }
  for (const [other, right] of samples.entries()) {
    const args = [id, BigInt(other)];
    cases.push(
      ["compared", args, comparison(text, right)],
      ["generic_less", args, Number(text < right)],
      ["method_less", args, Number(text < right)],
    );
  }
}
const pairs = samples.flatMap((_, index) => [[0, index], [index, 0], [index, (index + 1) % samples.length]]);
pairs.push([9, 10], [10, 9], [13, 13], [3, 2], [5, 16]);
for (const [left, right] of pairs) {
  const text = samples[left] + samples[right];
  const args = [BigInt(left), BigInt(right)];
  cases.push(["concat_length", args, BigInt(text.length)]);
  for (let offset = 0; offset < text.length; ++offset) {
    cases.push(["concat_unit", [...args, BigInt(offset)], text.charCodeAt(offset)]);
  }
}
cases.push(["utf8_length", [], BigInt(legacy.length)], ["utf8_iterated", [], weighted([...legacy])]);
for (const [index, byte] of legacy.entries()) {
  cases.push(["utf8_byte", [BigInt(index)], byte]);
}
cases.push(
  ["parses", [], 1], ["patterns", [], 1], ["task_values", [], 1],
  ["owned_paths", [0n], 0n], ["owned_paths", [1n], 42n],
  ["owned_paths", [4096n], 4096n * 42n],
  ["pipeline_paths", [0n], 0n], ["pipeline_paths", [1n], 12n],
  ["pipeline_paths", [4096n], 4096n * 12n],
  ["allocation_baseline", [], 3n], ["allocation_move", [], 3n], ["allocation_repair", [], 3n],
);
traps.push(["utf8_byte", [-1n]], ["utf8_byte", [BigInt(legacy.length)]]);

try {
  cli(["check", fixture]);
  const header = join(scratch, "tz-strings.h");
  const ir = join(scratch, "strings.ll");
  const again = join(scratch, "again.ll");
  cli(["build", fixture, "--emit", "header", "-o", header]);
  cli(["build", fixture, "--emit", "llvm", "-o", ir]);
  cli(["build", fixture, "--emit", "llvm", "-o", again]);
  const sourceIr = readFileSync(ir, "utf8");
  assert.equal(sourceIr, readFileSync(again, "utf8"), "string IR is deterministic");
  const declarations = sourceIr.match(/^declare .*$/gm) ?? [];
  assert.equal(new Set(declarations).size, declarations.length, "runtime declarations are deduplicated");
  writeFileSync(ir, sourceIr.replaceAll("@malloc", "@tracked_alloc").replaceAll("@free", "@tracked_free"));

  const runtimeBridges = [
    ["decode", "string", "tz.string.from_utf8"],
    ["encode", "utf8string", "tz.utf8string.from_string"],
    ["repair", "string", "tz.string.to_well_formed"],
  ].map(([name, type, implementation]) => `
define i64 @test_${name}(ptr %input, i64 %length, ptr %output) {
  %value = call %tz.${type} @${implementation}(ptr %input, i64 %length)
  %data = extractvalue %tz.${type} %value, 0
  %size = extractvalue %tz.${type} %value, 1
  store ptr %data, ptr %output
  ret i64 %size
}
`).join("") + `
define i32 @test_well_formed(ptr %input, i64 %length) {
  %valid = call i1 @tz.string.is_well_formed(ptr %input, i64 %length)
  %result = zext i1 %valid to i32
  ret i32 %result
}
define void @test_drop(ptr %data) {
  call void @tz.free(ptr %data)
  ret void
}
define void @test_allocate(i64 %length) {
  %value = call %tz.string @tz.string.allocate(i64 %length)
  ret void
}
define void @test_concat(i64 %left, i64 %right) {
  %a = insertvalue %tz.string zeroinitializer, i64 %left, 1
  %b = insertvalue %tz.string zeroinitializer, i64 %right, 1
  %value = call %tz.string @tz.string.concat(%tz.string %a, %tz.string %b)
  ret void
}
`;
  const nativeRuntimeIr = join(scratch, "runtime-native.ll");
  writeFileSync(nativeRuntimeIr, (sourceIr + runtimeBridges)
    .replaceAll("@malloc", "@tracked_alloc").replaceAll("@free", "@tracked_free"));
  const wasmRuntimeIr = join(scratch, "runtime-wasm.ll");
  cli(["build", fixture, "--target", "wasm32", "--emit", "llvm", "-o", wasmRuntimeIr]);
  writeFileSync(wasmRuntimeIr, readFileSync(wasmRuntimeIr, "utf8") + runtimeBridges);
  const runtimeHarness = join(root, "tests/strings_runtime.c");

  // Exercise the real length guards without attempting petabyte allocations.
  const maxLength = BigInt(Number.MAX_SAFE_INTEGER);
  const guardCases = [
    ["guard_length", [0n], 0n], ["guard_length", [1n], 1n],
    ["guard_length", [maxLength], maxLength],
    ["guard_concat", [maxLength, 0n], maxLength],
    ["guard_concat", [maxLength - 1n, 1n], maxLength],
    ["guard_concat", [0n, 0n], 0n],
  ];
  const guardTraps = [
    ["guard_length", [maxLength + 1n]], ["guard_length", [-1n]],
    ["guard_length", [(1n << 63n) - 1n]],
    ["guard_concat", [maxLength, 1n]], ["guard_concat", [maxLength + 1n, 0n]],
    ["guard_concat", [-1n, 1n]],
  ];
  const guardIr = join(scratch, "length-guards.ll");
  const runtimeGuards = ["allocate", "concat"].map((name) => {
    const definition = sourceIr.match(new RegExp(`^define internal %tz\\.string @tz\\.string\\.${name}\\([\\s\\S]*?^}`, "m"));
    assert.ok(definition, `missing string ${name} runtime`);
    return definition[0];
  }).join("\n");
  writeFileSync(guardIr, `%tz.string = type { ptr, i64 }
@recorded_bytes = internal global i64 0
declare void @llvm.trap()
define internal ptr @tz.alloc(i64 %bytes) {
  store i64 %bytes, ptr @recorded_bytes
  ret ptr null
}
define internal void @tz.string.copy(ptr %target, ptr %source, i64 %length) {
  ret void
}
${runtimeGuards}
define i64 @tz_guard_length(i64 %length) {
  store i64 0, ptr @recorded_bytes
  %value = call %tz.string @tz.string.allocate(i64 %length)
  %result = extractvalue %tz.string %value, 1
  ret i64 %result
}
define i64 @tz_guard_concat(i64 %left, i64 %right) {
  store i64 0, ptr @recorded_bytes
  %a = insertvalue %tz.string zeroinitializer, i64 %left, 1
  %b = insertvalue %tz.string zeroinitializer, i64 %right, 1
  %value = call %tz.string @tz.string.concat(%tz.string %a, %tz.string %b)
  %result = extractvalue %tz.string %value, 1
  ret i64 %result
}
define i64 @tz_guard_bytes() {
  %bytes = load i64, ptr @recorded_bytes
  ret i64 %bytes
}
`);
  const guardHost = join(scratch, "length-guards.c");
  writeFileSync(guardHost, `
#include <assert.h>
#include <stdint.h>
#include <stdlib.h>
extern int64_t tz_guard_length(int64_t);
extern int64_t tz_guard_concat(int64_t, int64_t);
extern int64_t tz_guard_bytes(void);
int main(int argc, char **argv) {
    if (argc == 2) {
        switch (atoi(argv[1])) {
            ${guardTraps.map(([name, args], index) =>
    `case ${index}: (void)tz_${name}(${args.map(cValue).join(", ")}); break;`).join("\n")}
            default: return 2;
        }
        return 0;
    }
    ${guardCases.map(([name, args, expected]) =>
    `assert(tz_${name}(${args.map(cValue).join(", ")}) == ${cValue(expected)}); assert(tz_guard_bytes() == ${cValue(expected * 2n)});`).join("\n")}
    return 0;
}
`);
  const host = join(scratch, "host.c");
  writeFileSync(host, `
#include <assert.h>
#include <stdatomic.h>
#include <stdint.h>
#include <stdlib.h>
#include "tz-strings.h"
static _Atomic uint64_t live, allocations;
void *tracked_alloc(uint64_t size) {
    uint64_t *p = malloc((size_t)size + 16);
    assert(p);
    p[0] = size;
    p[1] = UINT64_C(0x51a110ca7e);
    atomic_fetch_add(&live, size);
    atomic_fetch_add(&allocations, 1);
    return p + 2;
}
void tracked_free(void *value) {
    if (!value) return;
    uint64_t *p = (uint64_t *)value - 2;
    assert(p[1] == UINT64_C(0x51a110ca7e));
    p[1] = 0;
    uint64_t before = atomic_fetch_sub(&live, p[0]);
    assert(before >= p[0]);
    free(p);
}
int main(int argc, char **argv) {
    if (argc == 2) {
        switch (atoi(argv[1])) {
            ${traps.map(([name, args], index) =>
    `case ${index}: (void)tz_${name}(${args.map(cValue).join(", ")}); break;`).join("\n")}
            default: return 2;
        }
        return 0;
    }
    ${cases.map(([name, args, expected]) =>
    `assert(tz_${name}(${args.map(cValue).join(", ")}) == ${cValue(expected)}); assert(atomic_load(&live) == 0);`).join("\n")}
    uint64_t before = atomic_load(&allocations);
    assert(tz_allocation_baseline() == 3);
    uint64_t baseline = atomic_load(&allocations) - before;
    before = atomic_load(&allocations);
    assert(tz_allocation_move() == 3);
    assert(atomic_load(&allocations) - before == baseline);
    assert(atomic_load(&live) == 0);
    return 0;
}
`);
  const noTextFixture = join(fixture, "no_text");
  const noTextIr = join(scratch, "no-text.ll");
  const noTextHost = join(scratch, "no-text.c");
  cli(["build", noTextFixture, "--emit", "llvm", "-o", noTextIr]);
  writeFileSync(noTextHost, `
#include <assert.h>
#include <stdint.h>
extern int64_t tz_value(void);
int main(void) { assert(tz_value() == 42); return 0; }
`);
  for (const optimization of [0, 3]) {
    const native = join(scratch, `strings-O${optimization}`);
    execute(clang, [
      "-std=c11", `-O${optimization}`, "-Wno-override-module", host, ir,
      join(root, "src/runtime/task.c"), "-pthread", "-lm", "-o", native,
    ]);
    execute(native, []);
    for (const [index, [name, args]] of traps.entries()) {
      const result = execute(native, [String(index)], false);
      assert.notEqual(result.status, 0, `native O${optimization}: ${name}(${args}) must trap`);
    }
    const wasm = join(scratch, `strings-O${optimization}.wasm`);
    cli(["build", fixture, "--target", "wasm32", `-O${optimization}`, "-o", wasm]);
    const module = new WebAssembly.Module(readFileSync(wasm));
    assert.deepEqual(WebAssembly.Module.imports(module), [], "strings require no WASM imports");
    const api = new WebAssembly.Instance(module).exports;
    for (const [name, args, expected] of cases) {
      assert.equal(api[`tz_${name}`](...args), expected, `WASM O${optimization}: ${name}(${args})`);
    }
    const before = api.memory.buffer.byteLength;
    assert.equal(api.tz_owned_paths(16384n), 16384n * 42n);
    assert.ok(api.memory.buffer.byteLength <= before + 65536, "string buffers and captures reuse freed memory");
    assert.ok(api.memory.buffer.byteLength <= 16 * 1024 * 1024, "WASM allocation cap");
    for (const [name, args] of traps) {
      const isolated = new WebAssembly.Instance(module).exports;
      assert.throws(() => isolated[`tz_${name}`](...args), WebAssembly.RuntimeError, `${name}(${args})`);
    }
    const nativeGuards = join(scratch, `length-guards-O${optimization}`);
    execute(clang, [`-O${optimization}`, "-Wno-override-module", guardHost, guardIr, "-o", nativeGuards]);
    execute(nativeGuards, []);
    for (const [index, [name, args]] of guardTraps.entries()) {
      assert.notEqual(execute(nativeGuards, [String(index)], false).status, 0, `${name}(${args}) must trap`);
    }
    const wasmGuards = join(scratch, `length-guards-O${optimization}.wasm`);
    execute(clang, [
      "--target=wasm32-unknown-unknown", `-O${optimization}`, "-Wno-override-module",
      "-nostdlib", "-Wl,--no-entry", "-Wl,--max-memory=16777216", guardIr,
      ...["tz_guard_length", "tz_guard_concat", "tz_guard_bytes"].map((name) => `-Wl,--export=${name}`),
      "-o", wasmGuards,
    ]);
    const guardModule = new WebAssembly.Module(readFileSync(wasmGuards));
    assert.deepEqual(WebAssembly.Module.imports(guardModule), []);
    const guards = new WebAssembly.Instance(guardModule).exports;
    for (const [name, args, expected] of guardCases) {
      assert.equal(guards[`tz_${name}`](...args), expected, `${name}(${args})`);
      assert.equal(guards.tz_guard_bytes(), expected * 2n, "UTF-16 allocation sizes count two bytes per unit");
    }
    for (const [name, args] of guardTraps) {
      assert.throws(() => guards[`tz_${name}`](...args), WebAssembly.RuntimeError, `${name}(${args})`);
    }
    const noTextNative = join(scratch, `no-text-O${optimization}`);
    execute(clang, [`-O${optimization}`, "-Wno-override-module", noTextHost, noTextIr, "-lm", "-o", noTextNative]);
    execute(noTextNative, []);
    const noTextWasm = join(scratch, `no-text-O${optimization}.wasm`);
    cli(["build", noTextFixture, "--target", "wasm32", `-O${optimization}`, "-o", noTextWasm]);
    const noTextModule = new WebAssembly.Module(readFileSync(noTextWasm));
    assert.deepEqual(WebAssembly.Module.imports(noTextModule), []);
    assert.equal(new WebAssembly.Instance(noTextModule).exports.tz_value(), 42n);

    const nativeRuntime = join(scratch, `runtime-O${optimization}`);
    execute(clang, [
      "-std=c11", `-O${optimization}`, "-Wno-override-module", runtimeHarness, nativeRuntimeIr,
      join(root, "src/runtime/task.c"), "-pthread", "-lm", "-o", nativeRuntime,
    ]);
    execute(nativeRuntime, []);
    for (let index = 0; index < 28; ++index) {
      const result = execute(nativeRuntime, [String(index)], false);
      assert.ok(["SIGILL", "SIGTRAP"].includes(result.signal),
        `native runtime trap ${index}: expected llvm.trap, got ${result.signal ?? result.status}\n${result.stderr}`);
    }
    const wasmRuntime = join(scratch, `runtime-O${optimization}.wasm`);
    execute(clang, [
      "--target=wasm32-unknown-unknown", `-O${optimization}`, "-std=c11",
      "-ffreestanding", "-fno-builtin", "-Wno-override-module", "-mbulk-memory",
      "-nostdlib", runtimeHarness, wasmRuntimeIr, "-Wl,--no-entry", "-Wl,--export-memory",
      "-Wl,-z,stack-size=1048576", "-Wl,--max-memory=16777216",
      ...["all_scalars", "runtime_trap_case", "runtime_trap_count"].map((name) => `-Wl,--export=${name}`),
      "-o", wasmRuntime,
    ]);
    const runtimeModule = new WebAssembly.Module(readFileSync(wasmRuntime));
    assert.deepEqual(WebAssembly.Module.imports(runtimeModule), []);
    const runtime = new WebAssembly.Instance(runtimeModule).exports;
    const initialMemory = runtime.memory.buffer.byteLength;
    assert.equal(runtime.all_scalars(), 4344, "all 1,112,064 Unicode scalars round-trip in 256-scalar batches");
    assert.ok(runtime.memory.buffer.byteLength <= initialMemory + 65536, "scalar batches release their buffers");
    assert.equal(runtime.runtime_trap_count(), 28);
    for (let index = 0; index < runtime.runtime_trap_count(); ++index) {
      const isolated = new WebAssembly.Instance(runtimeModule).exports;
      assert.throws(() => isolated.runtime_trap_case(index),
        (error) => error instanceof WebAssembly.RuntimeError && /unreachable/.test(error.message),
        `runtime trap ${index} must be explicit, not an out-of-bounds memory access`);
    }
    console.log(`strings O${optimization}: ${cases.length} ECMAScript/reference and ownership cases, ${traps.length} traps, 2^53 length guards on native/WASM`);
    console.log(`strings runtime O${optimization}: all Unicode scalars in 4344 bounded batches and 28 strict conversion/bounds traps on native/WASM`);
  }

  const consoleDirectory = join(scratch, "console");
  mkdirSync(consoleDirectory);
  const source = join(consoleDirectory, "Main.tz");
  const consoleCases = [
    ['"a\\0é日本語😀"', "a\0é日本語😀\n"],
    ['u8"a\\0é日本語\\u{1f600}"', "a\0é日本語😀\n"],
    ['"\\uD83D\\uDE00"', "😀\n"],
    ['to_string u8"é\\0😀"', "é\0😀\n"],
    ['let text = "\\uD800"; String.to_well_formed ref text', "\uFFFD\n"],
    ['let text = "\\uDC00\\uD800"; String.to_well_formed ref text', "\uFFFD\uFFFD\n"],
    ['"\\uD800"', null],
    ['"\\uDC00"', null],
    ['let text = "\\uD800"; Display.display ref text', null],
    ['let text = "\\uDC00\\uD800"; Utf8String.from_string ref text', null],
  ];
  for (const [expression, expected] of consoleCases) {
    writeFileSync(source, `${expression}\n`);
    for (const optimization of [0, 3]) {
      const result = cli(["run", source, `-O${optimization}`], expected !== null, null);
      if (expected === null) {
        assert.notEqual(result.status, 0, `console must reject lone surrogates: ${expression}`);
      } else {
        assert.deepEqual(result.stdout, Buffer.from(expected, "utf8"), `console O${optimization}: ${expression}`);
      }
    }
  }
  console.log("strings: strict UTF-16 console encoding and legacy UTF-8 byte output passed");
} finally {
  rmSync(scratch, { recursive: true, force: true });
}
