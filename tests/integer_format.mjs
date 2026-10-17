// PM08: differential test of the integer runtime in src/runtime/integer.ll: the decimal text of an integer
// (to_string, Display, interpolation), the text of a format hole ({n:+}, {n:x}, {n:X}, {n:o}, {n:b}), and
// Parse.parse of an integer type.
//   node tests/integer_format.mjs
// The expected results come from tests/integer_format_reference.py (Python's own text and its own reading of
// the grammar, for boundary, repdigit, power, mutated, and seeded random inputs of all ten integer types),
// and from the C library and BigInt for every value of the 8- and 16-bit types. The native builds also run
// the numeric runtime's tz_soft_format, tz_soft_format_spec, and tz_soft_parse (the code the integer runtime
// replaced for integers) on every input and require the same result. The runtime is built as a native
// program and as a WebAssembly module at -O0, -O3, -Os, and -Oz; it must also write exactly the text it
// returns and read nothing beyond the text it parses.
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
  // A program that stops at its first mismatch stops reading its input; its output tells which.
  if (result.error && result.error.code !== "EPIPE") throw result.error;
  assert.equal(result.status, 0, `${program} ${args.join(" ")}\n${result.stdout.slice(-600)}\n${result.stderr.slice(-600)}`);
  return result.stdout;
}

try {
  const { cases, specs, parses } = JSON.parse(execute("python3", ["tests/integer_format_reference.py"]));
  assert.ok(cases.length > 20000 && specs.length > 10000 && parses.length > 50000, "the reference covers all ten types");
  for (const list of [cases, specs, parses]) assert.equal(new Set(list.map(([kind]) => kind)).size, 10);
  assert.ok(parses.some(([, , expected]) => expected === null) && parses.some(([, , expected]) => expected !== null));
  // The runtime's functions are internal and take an i1 and an i128; the host calls these wrappers instead.
  const source = readFileSync(join(root, "src/runtime/integer.ll"), "utf8");
  for (const name of ["format", "format128", "spec", "spec128", "parse", "parse128"]) {
    assert.match(source, new RegExp(`define internal \\S+ @tz\\.integer\\.${name}\\(`), name);
  }
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

define i32 @tz_integer_spec(ptr %end, i64 %value, i32 %signed, i32 %flags) {
  %flag = trunc i32 %signed to i1
  %length = call i32 @tz.integer.spec(ptr %end, i64 %value, i1 %flag, i32 %flags)
  ret i32 %length
}

define i32 @tz_integer_spec128(ptr %end, i64 %low, i64 %high, i32 %signed, i32 %flags) {
  %lower = zext i64 %low to i128
  %upper = zext i64 %high to i128
  %shifted = shl i128 %upper, 64
  %value = or i128 %shifted, %lower
  %flag = trunc i32 %signed to i1
  %length = call i32 @tz.integer.spec128(ptr %end, i128 %value, i1 %flag, i32 %flags)
  ret i32 %length
}

define i32 @tz_integer_parse(ptr %units, i64 %length, i32 %bits, i32 %signed, ptr %out) {
  %flag = trunc i32 %signed to i1
  %ok = call i1 @tz.integer.parse(ptr %units, i64 %length, i32 %bits, i1 %flag, ptr %out)
  %result = zext i1 %ok to i32
  ret i32 %result
}

define i32 @tz_integer_parse128(ptr %units, i64 %length, i32 %signed, ptr %out) {
  %flag = trunc i32 %signed to i1
  %ok = call i1 @tz.integer.parse128(ptr %units, i64 %length, i1 %flag, ptr %out)
  %result = zext i1 %ok to i32
  ret i32 %result
}
`;
  const runtime = join(temporary, "integer.ll");
  writeFileSync(runtime, source.replaceAll("define internal ", "define ") + wrappers);
  // LLVM turns an i128 division into a call of __udivti3 or a relative. On Windows x64 that call passes the operands by pointer and
  // returns in XMM0, which the helpers of src/runtime/wasm.ll do not follow, so no entry point may divide an i128 on any target.
  for (const triple of ["x86_64-pc-windows-msvc", "aarch64-pc-windows-msvc", "x86_64-unknown-linux-gnu", "aarch64-apple-macos14"]) {
    for (const level of ["-O0", "-O3"]) {
      const result = spawnSync(clang, [`--target=${triple}`, level, "-S", "-Wno-override-module", "-x", "ir", runtime, "-o", "-"],
        { encoding: "utf8", timeout: 300_000, maxBuffer: 256 * 1024 * 1024 });
      if (result.status !== 0) {
        console.log(`Integer runtime: ${triple} is not a target of this Clang; its library-call check is skipped`);
        break;
      }
      assert.doesNotMatch(result.stdout, /__u?(?:div|mod)ti3/, `the integer runtime calls a 128-bit division routine for ${triple} ${level}`);
    }
  }
  const harness = join(root, "tests/integer_format_runtime.c");
  const numeric = join(root, "src/runtime/numeric.ll");
  // The numeric runtime's radix loop divides an i128 by a variable base, which at -O0 is the library call whose Windows x64
  // convention the helpers of src/runtime/wasm.ll do not follow, so Windows compares with the reference only.
  const crossCheck = process.platform !== "win32";
  const halves = (raw) => {
    const value = BigInt(`0x${raw}`);
    return [value & mask64, value >> 64n];
  };
  const pair = (raw) => halves(raw).map((half) => half.toString(16)).join(" ");
  const requests = [
    ...cases.map(([kind, raw]) => `f ${kind} ${pair(raw)}`),
    ...specs.map(([kind, flags, raw]) => `p ${kind} ${flags} ${pair(raw)}`),
    ...parses.map(([kind, units]) => `r ${kind} ${units.length / 4} ${units}`.trimEnd()),
  ];
  const expected = [
    ...cases.map(([, , text]) => text),
    ...specs.map(([, , , text]) => text),
    ...parses.map(([, , raw]) => raw ?? "none"),
  ];
  const sweeps = [16, 17, 24, 25];
  const describe = (request) => (request.length > 160 ? `${request.slice(0, 160)}...` : request);
  // What a format hole shows, computed again here with BigInt for the sweeps of the WebAssembly build.
  const specText = (value, flags) => {
    const style = (flags >> 4) & 15;
    const magnitude = value < 0n ? -value : value;
    const digits = style === 3 ? magnitude.toString(16) : style === 4 ? magnitude.toString(16).toUpperCase()
      : style === 5 ? magnitude.toString(8) : style === 6 ? magnitude.toString(2) : magnitude.toString();
    return (value < 0n ? "-" : flags & 1 ? "+" : "") + digits;
  };
  const unitsOf = (text) => Array.from(text, (char) => char.charCodeAt(0).toString(16).padStart(4, "0")).join("");
  for (const level of ["0", "3", "s", "z"]) {
    // The native program is linked with numeric.ll, whose results it compares with the runtime's.
    const native = join(temporary, `integer-O${level}${process.platform === "win32" ? ".exe" : ""}`);
    execute(clang, [`-O${level}`, ...(crossCheck ? ["-DCROSS_CHECK"] : []), "-Wno-override-module", harness, runtime, ...(crossCheck ? [numeric] : []), "-o", native]);
    const lines = execute(native, [], `${requests.join("\n")}\n${sweeps.map((kind) => `s ${kind}\n`).join("")}`)
      .replaceAll("\r\n", "\n").trimEnd().split("\n");
    assert.equal(lines.length, requests.length + sweeps.length);
    for (const [index, line] of expected.entries()) {
      assert.equal(lines[index], line, `native O${level}: ${describe(requests[index])}`);
    }
    assert.deepEqual(lines.slice(requests.length), sweeps.map((kind) => `swept ${kind}`));

    const wasm = join(temporary, `integer-O${level}.wasm`);
    execute(clang, ["--target=wasm32-unknown-unknown", `-O${level}`, "-ffreestanding", "-fno-builtin", "-Wno-override-module",
      "-mbulk-memory", "-nostdlib", harness, runtime, join(root, "src/runtime/wasm.ll"), "-Wl,--no-entry",
      "-Wl,--export-memory", "-Wl,-z,stack-size=1048576", "-Wl,--max-memory=16777216",
      ...["format_value", "spec_value", "parse_value", "text_pointer", "input_pointer", "result_low", "result_high"]
        .map((name) => `-Wl,--export=${name}`),
      "-o", wasm]);
    const module = new WebAssembly.Module(readFileSync(wasm));
    assert.deepEqual(WebAssembly.Module.imports(module), []);
    const api = new WebAssembly.Instance(module).exports;
    const memory = new Uint8Array(api.memory.buffer);
    const input = new Uint16Array(api.memory.buffer, api.input_pointer(), 4100 + 4);
    const decoder = new TextDecoder();
    const shown = (length) => {
      const pointer = api.text_pointer();
      return decoder.decode(memory.subarray(pointer, pointer + length));
    };
    const formatted = (kind, low, high) => shown(api.format_value(kind, low, high));
    const specified = (kind, flags, low, high) => shown(api.spec_value(kind, flags, low, high));
    const parsed = (kind, units) => {
      const length = units.length / 4;
      for (let index = 0; index < length; ++index) input[index] = Number.parseInt(units.slice(4 * index, 4 * index + 4), 16);
      return api.parse_value(kind, length)
        ? ((BigInt.asUintN(64, api.result_high()) << 64n) | BigInt.asUintN(64, api.result_low())).toString(16) : "none";
    };
    let index = 0;
    for (const [kind, raw] of cases) {
      const [low, high] = halves(raw);
      assert.equal(formatted(kind, low, high), expected[index], `WASM O${level}: ${describe(requests[index])}`);
      ++index;
    }
    for (const [kind, flags, raw] of specs) {
      const [low, high] = halves(raw);
      assert.equal(specified(kind, flags, low, high), expected[index], `WASM O${level}: ${describe(requests[index])}`);
      ++index;
    }
    for (const [kind, units] of parses) {
      assert.equal(parsed(kind, units), expected[index], `WASM O${level}: ${describe(requests[index])}`);
      ++index;
    }
    for (const kind of sweeps) {
      const bits = 8 << (kind & 7);
      // Every value of an 8-bit type and every thirteenth of a 16-bit type go through the formats and the parse.
      const step = bits === 8 ? 1n : 13n;
      for (let raw = 0n; raw < 1n << BigInt(bits); ++raw) {
        const value = kind < 24 ? BigInt.asIntN(bits, raw) : raw;
        assert.equal(formatted(kind, raw, 0n), value.toString(), `WASM O${level}: kind ${kind} raw ${raw}`);
        if (raw % step !== 0n && raw !== (1n << BigInt(bits)) - 1n) continue;
        for (const [style, prefix] of [[0, ""], [3, "0x"], [4, "0x"], [5, null], [6, "0b"]]) {
          for (const plus of [0, 1]) {
            const flags = plus | (style << 4);
            const text = specText(value, flags);
            assert.equal(specified(kind, flags, raw, 0n), text, `WASM O${level}: kind ${kind} flags ${flags} raw ${raw}`);
            if (prefix === null) continue;
            const sign = /^[-+]/.test(text) ? 1 : 0;
            assert.equal(parsed(kind, unitsOf(text.slice(0, sign) + prefix + text.slice(sign))), raw.toString(16),
              `WASM O${level}: parse kind ${kind} raw ${raw}`);
          }
        }
      }
    }
    console.log(`Integer runtime O${level}: ${cases.length} format, ${specs.length} spec, and ${parses.length} parse cases, ` +
      `${sweeps.length} sweeps on native (${crossCheck ? "checked against numeric.ll" : "reference only"}) and WASM`);
  }
} finally {
  rmSync(temporary, { recursive: true, force: true });
}
