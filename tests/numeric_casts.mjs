import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const compiler = resolve(process.argv[2] ?? join(root, "target/debug/tsuzuri"));
const clang = process.env.TSUZURI_CLANG ?? "clang";
const temporary = mkdtempSync(join(tmpdir(), "tsuzuri-numeric-casts-"));
function run(program, args, success = true) {
  const result = spawnSync(program, args, { cwd: root, encoding: "utf8", timeout: 180_000 });
  if (result.error) throw result.error;
  if (success) assert.equal(result.status, 0, `${program} ${args.join(" ")}\n${result.stderr}`);
  return result;
}
const cli = (args, success) => run(compiler, args, success);
const integers = [8, 16, 32, 64].flatMap((bits) =>
  [true, false].map((signed) => ({ name: `i${bits}${signed ? "" : "u"}`, bits, signed })));
const floats = [32, 64].map((bits) => ({ name: `f${bits}`, bits }));
const conversions = integers.flatMap((integer) =>
  floats.flatMap((float) => [[integer, float], [float, integer]]));
conversions.push([floats[0], floats[1]], [floats[1], floats[0]]);
const isInteger = (type) => type.signed !== undefined;
const limits = (type) => ({
  min: type.signed ? -(1n << BigInt(type.bits - 1)) : 0n,
  max: (1n << BigInt(type.bits - (type.signed ? 1 : 0))) - 1n,
});

function integerInputs(type) {
  const { min, max } = limits(type);
  const inputs = [min, min + 1n, -1n, 0n, 1n, max - 1n, max];
  for (const exponent of [24n, 53n, 62n, 63n]) {
    for (const delta of [-1n, 0n, 1n, 3n]) {
      inputs.push((1n << exponent) + delta);
      // Values on either side of an f32 midpoint expose double rounding through f64.
      const halfway = (1n << exponent) + (1n << (exponent - 24n)) + delta;
      inputs.push(halfway, -halfway);
    }
  }
  let seed = 42n;
  for (let i = 0; i < 16; ++i) {
    seed = BigInt.asUintN(64, seed * 6364136223846793005n + 1442695040888963407n);
    inputs.push(type.signed ? BigInt.asIntN(64, seed) : seed);
  }
  return [...new Set(inputs.filter((value) => value >= min && value <= max))];
}

const floatInputs = [
  NaN, Infinity, -Infinity, 0, -0, 1.5, -1.5, 3.9, -3.9,
  Number.MIN_VALUE, -Number.MIN_VALUE, Number.MAX_VALUE, -Number.MAX_VALUE,
  2 ** -149, -(2 ** -149), 2 ** -150, -(2 ** -150), 2 ** -126,
  1 + 2 ** -24, 1 + 3 * 2 ** -24, (2 - 2 ** -23) * 2 ** 127,
  (2 - 2 ** -24) * 2 ** 127, 2 ** 63 - 1024, 2 ** 64 - 2048,
  ...[7, 8, 15, 16, 31, 32, 63, 64].flatMap((bits) =>
    [2 ** bits - 1, 2 ** bits, 2 ** bits + 1, -(2 ** bits), -(2 ** bits) - 1]),
];

function expected(value, to) {
  if (isInteger(to)) {
    const { min, max } = limits(to);
    if (Number.isNaN(value)) return 0n;
    if (value === Infinity) return max;
    if (value === -Infinity) return min;
    const truncated = BigInt(Math.trunc(value));
    return truncated < min ? min : truncated > max ? max : truncated;
  }
  if (typeof value === "bigint") {
    const negative = value < 0n;
    let magnitude = negative ? -value : value;
    const shift = magnitude.toString(2).length - (to.bits === 32 ? 24 : 53);
    if (shift > 0) {
      const amount = BigInt(shift), half = 1n << (amount - 1n);
      let rounded = magnitude >> amount;
      const remainder = magnitude - (rounded << amount);
      if (remainder > half || (remainder === half && (rounded & 1n))) ++rounded;
      magnitude = rounded << amount;
    }
    return Number(negative ? -magnitude : magnitude);
  }
  return to.bits === 32 ? Math.fround(value) : value;
}

function literal(value) {
  if (typeof value === "bigint") {
    if (value === -(1n << 63n)) return "(-INT64_C(9223372036854775807) - 1)";
    return value < 0n ? `(-INT64_C(${-value}))` : `UINT64_C(${value})`;
  }
  if (Number.isNaN(value)) return "NAN";
  if (value === Infinity) return "INFINITY";
  if (value === -Infinity) return "(-INFINITY)";
  if (Object.is(value, -0)) return "-0.0";
  return value.toPrecision(17);
}

try {
  const source = join(temporary, "Main.tz");
  const definitions = [], cases = [];
  for (const [from, to] of conversions) {
    const name = `cast_${from.name}_${to.name}`;
    definitions.push(`export def ${name} :: ${from.name} -> ${to.name}\nfn ${name} x = x as ${to.name}`);
    definitions.push(`export def ref_${name} :: ${from.name} -> ${to.name}\nfn ref_${name} x = (x as f128) as ${to.name}`);
    const inputs = isInteger(from) ? integerInputs(from)
      : floatInputs.map((value) => from.bits === 32 ? Math.fround(value) : value);
    for (const input of inputs) cases.push({ name, from, to, input, expected: expected(input, to) });
  }
  definitions.push("export def builtin_int :: f64 -> i64\nfn builtin_int x = to_int x");
  definitions.push("export def builtin_float :: i64 -> f64\nfn builtin_float x = to_float x");
  for (const value of floatInputs) {
    cases.push({ name: "builtin_int", from: floats[1], to: integers[6], input: value, expected: expected(value, integers[6]) });
  }
  for (const value of integerInputs(integers[6])) {
    cases.push({ name: "builtin_float", from: integers[6], to: floats[1], input: value, expected: expected(value, floats[1]) });
  }
  definitions.push("def main :: i64\nfn main = (to_float 42) as i64");
  writeFileSync(source, definitions.join("\n"));
  cli(["build", source, "--emit", "header", "-o", join(temporary, "casts.h")]);
  const statements = cases.flatMap((test) => {
    const calls = [`tz_${test.name}`];
    if (test.name.startsWith("cast_")) calls.push(`tz_ref_${test.name}`);
    return calls.map((name) => {
      const call = `${name}(${literal(test.input)})`;
      if (Number.isNaN(test.expected)) return `assert(isnan(${call}));`;
      if (test.expected === 0 && !isInteger(test.to)) {
        return `assert(${call} == 0.0 && !!signbit(${call}) == ${Object.is(test.expected, -0) ? 1 : 0});`;
      }
      return `assert(${call} == ${literal(test.expected)});`;
    });
  });
  const host = join(temporary, "host.c");
  writeFileSync(host, `#include <assert.h>\n#include <math.h>\n#include "casts.h"\nint main(void) {\n${statements.join("\n")}\nreturn 0;\n}\n`);
  for (const [optimization, cpu] of [[0, "generic"], [3, "generic"], [3, "native"]]) {
    const object = join(temporary, "casts.o");
    const native = join(temporary, process.platform === "win32" ? "host.exe" : "host");
    cli(["build", source, "--emit", "object", `-O${optimization}`, "--cpu", cpu, "-o", object]);
    run(clang, ["-std=c11", "-Wall", "-Wextra", "-Werror", host, object, "-o", native,
      ...(process.platform === "win32" ? [] : ["-lm"])]);
    run(native, []);
    assert.equal(cli(["run", source, `-O${optimization}`, "--cpu", cpu]).stdout, "42\n");
  }
  for (const optimization of [0, 3]) {
    const wasm = join(temporary, "casts.wasm");
    cli(["build", source, "--target", "wasm32", `-O${optimization}`, "-o", wasm]);
    const { instance, module } = await WebAssembly.instantiate(readFileSync(wasm));
    assert.deepEqual(WebAssembly.Module.imports(module), []);
    for (const test of cases) {
      const input = isInteger(test.from) && test.from.bits < 64 ? Number(test.input) : test.input;
      const names = [test.name, ...(test.name.startsWith("cast_") ? [`ref_${test.name}`] : [])];
      for (const name of names) {
        let actual = instance.exports[`tz_${name}`](input);
        if (isInteger(test.to)) {
          actual = test.to.signed ? BigInt(actual) : BigInt.asUintN(test.to.bits, BigInt(actual));
        }
        assert.equal(actual, test.expected, `${name}(${test.input}), WASM -O${optimization}`);
      }
    }
  }
  for (const options of [
    ["check", source, "--cpu", "native"],
    ["build", source, "--target", "wasm32", "--cpu", "native"],
    ["build", source, "--emit", "llvm", "--cpu", "native"],
    ["build", source, "--emit", "header", "--cpu", "native"],
    ["run", source, "--cpu", "unknown"],
  ]) {
    const result = cli([...options, "--json"], false);
    assert.equal(result.status, 2);
    assert.equal(JSON.parse(result.stderr).code, "E2000");
  }
  console.log(`Numeric casts: ${cases.length} boundary/rounding cases, software reference, native generic/native CPU, WASM -O0/-O3`);
} finally {
  rmSync(temporary, { recursive: true, force: true });
}
