// PM08: differential test of the integer formatter in src/runtime/integer.ll.
//   node tests/integer_format.mjs
// The expected text comes from tests/integer_format_reference.py (Python's own decimal text of boundary,
// repdigit, power, and seeded random values of all ten integer types), and from the C library and BigInt
// for every value of the 8- and 16-bit types. The formatter is built as a native program and as a
// WebAssembly module at -O0, -O3, -Os, and -Oz; it must also write exactly the text it returns.
// It needs Clang (TSUZURI_CLANG) and Python 3, like tests/display_parse.mjs, but not the compiler.
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const clang = process.env.TSUZURI_CLANG ?? "clang";
const scratch = join(root, "target", "integer-format");
mkdirSync(scratch, { recursive: true });
const temporary = mkdtempSync(join(scratch, "run-"));
const mask64 = (1n << 64n) - 1n;

function execute(program, args, input) {
  const result = spawnSync(program, args, { cwd: root, input, encoding: "utf8", timeout: 300_000, maxBuffer: 256 * 1024 * 1024 });
  if (result.error) throw result.error;
  assert.equal(result.status, 0, `${program} ${args.join(" ")}\n${result.stdout}\n${result.stderr}`);
  return result.stdout;
}

try {
  const { cases } = JSON.parse(execute("python3", ["tests/integer_format_reference.py"]));
  assert.ok(cases.length > 20000, "the reference covers all ten types");
  assert.equal(new Set(cases.map(([kind]) => kind)).size, 10);
  // The runtime's functions are internal and take an i1 and an i128; the host calls these wrappers instead.
  const source = readFileSync(join(root, "src/runtime/integer.ll"), "utf8");
  assert.ok(source.includes("define internal i32 @tz.integer.format("));
  const wrappers = `
define i32 @tz_integer_format(ptr %end, i64 %value, i32 %signed) {
  %flag = trunc i32 %signed to i1
  %length = call i32 @tz.integer.format(ptr %end, i64 %value, i1 %flag)
  ret i32 %length
}

define i32 @tz_integer_format128(ptr %end, i64 %low, i64 %high, i32 %signed) {
  %lower = zext i64 %low to i128
  %upper = zext i64 %high to i128
  %shifted = shl i128 %upper, 64
  %value = or i128 %shifted, %lower
  %flag = trunc i32 %signed to i1
  %length = call i32 @tz.integer.format128(ptr %end, i128 %value, i1 %flag)
  ret i32 %length
}
`;
  const runtime = join(temporary, "integer.ll");
  writeFileSync(runtime, source.replaceAll("define internal ", "define ") + wrappers);
  const harness = join(root, "tests/integer_format_runtime.c");
  const halves = (raw) => {
    const value = BigInt(`0x${raw}`);
    return [value & mask64, value >> 64n];
  };
  const protocol = cases.map(([kind, raw]) => {
    const [low, high] = halves(raw);
    return `f ${kind} ${low.toString(16)} ${high.toString(16)}\n`;
  }).join("");
  const sweeps = [16, 17, 24, 25];
  for (const level of ["0", "3", "s", "z"]) {
    const native = join(temporary, `integer-O${level}${process.platform === "win32" ? ".exe" : ""}`);
    execute(clang, [`-O${level}`, "-Wno-override-module", harness, runtime,
      ...(process.platform === "win32" ? [join(root, "src/runtime/wasm.ll")] : []), "-o", native]);
    const lines = execute(native, [], protocol + sweeps.map((kind) => `s ${kind}\n`).join(""))
      .replaceAll("\r\n", "\n").trimEnd().split("\n");
    assert.equal(lines.length, cases.length + sweeps.length);
    for (const [index, [kind, raw, expected]] of cases.entries()) {
      assert.equal(lines[index], expected, `native O${level}: kind ${kind} raw ${raw}`);
    }
    assert.deepEqual(lines.slice(cases.length), sweeps.map((kind) => `swept ${kind}`));

    const wasm = join(temporary, `integer-O${level}.wasm`);
    execute(clang, ["--target=wasm32-unknown-unknown", `-O${level}`, "-ffreestanding", "-fno-builtin", "-Wno-override-module",
      "-mbulk-memory", "-nostdlib", harness, runtime, join(root, "src/runtime/wasm.ll"), "-Wl,--no-entry",
      "-Wl,--export-memory", "-Wl,-z,stack-size=1048576", "-Wl,--max-memory=16777216",
      "-Wl,--export=format_value", "-Wl,--export=text_pointer", "-o", wasm]);
    const module = new WebAssembly.Module(readFileSync(wasm));
    assert.deepEqual(WebAssembly.Module.imports(module), []);
    const api = new WebAssembly.Instance(module).exports;
    const memory = new Uint8Array(api.memory.buffer);
    const decoder = new TextDecoder();
    const text = (kind, low, high) => {
      const length = api.format_value(kind, low, high);
      const pointer = api.text_pointer();
      return decoder.decode(memory.subarray(pointer, pointer + length));
    };
    for (const [kind, raw, expected] of cases) {
      const [low, high] = halves(raw);
      assert.equal(text(kind, low, high), expected, `WASM O${level}: kind ${kind} raw ${raw}`);
    }
    for (const kind of sweeps) {
      const bits = 8 << (kind & 7);
      for (let raw = 0n; raw < 1n << BigInt(bits); ++raw) {
        const value = kind < 24 ? BigInt.asIntN(bits, raw) : raw;
        assert.equal(text(kind, raw, 0n), value.toString(), `WASM O${level}: kind ${kind} raw ${raw}`);
      }
    }
    console.log(`Integer format O${level}: ${cases.length} reference cases and ${sweeps.length} full sweeps on native and WASM`);
  }
} finally {
  rmSync(temporary, { recursive: true, force: true });
}
