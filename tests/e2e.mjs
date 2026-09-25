import assert from "node:assert/strict";
import {
  existsSync, linkSync, mkdirSync, mkdtempSync, readFileSync, readdirSync,
  rmSync, symlinkSync, writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { basename, dirname, join, resolve } from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const compiler = resolve(process.argv[2] ?? join(root, "target/debug/tsuzuri"));
const clang = process.env.TSUZURI_CLANG ?? "clang";
const temporary = mkdtempSync(join(tmpdir(), "tsuzuri-e2e-"));
const fixture = join(root, "tests/fixtures/Semantics.tz");
const cases = [];
const minimum = -(1n << 63n);
const maximum = (1n << 63n) - 1n;
const wrap = (value) => BigInt.asIntN(64, value);

function execute(program, args, { success = true, env = {}, cwd = root } = {}) {
  const result = spawnSync(program, args, {
    encoding: "utf8",
    timeout: 90_000,
    maxBuffer: 4 * 1024 * 1024,
    env: { ...process.env, ...env },
    cwd,
  });
  if (result.error) throw result.error;
  if (success) {
    assert.equal(result.status, 0,
      `${program} ${args.join(" ")}\n${result.stdout}\n${result.stderr}\n${result.signal ?? ""}`);
  }
  return result;
}

const cli = (args, options) => execute(compiler, args, options);

function add(name, args, expected) {
  const kind = expected === undefined ? "unit"
    : typeof expected === "boolean" ? "bool"
      : typeof expected === "bigint" ? "i64" : "f64";
  cases.push({ name, args, expected: kind === "bool" ? Number(expected) : expected, kind });
}

function cValue(value) {
  if (typeof value === "boolean") return value ? "1" : "0";
  if (typeof value === "bigint") {
    if (value === minimum) return "(-INT64_C(9223372036854775807) - 1)";
    return value < 0n ? `(-INT64_C(${-value}))` : `INT64_C(${value})`;
  }
  if (Number.isNaN(value)) return "NAN";
  if (value === Infinity) return "INFINITY";
  if (value === -Infinity) return "(-INFINITY)";
  if (Object.is(value, -0)) return "-0.0";
  return value.toPrecision(17);
}

function call(test) {
  return `tz_${test.name}(${test.args.map(cValue).join(", ")})`;
}

for (const [a, b] of [[maximum, 1n], [minimum, -1n], [minimum, maximum], [-7n, 3n], [7n, -3n]]) {
  add("add", [a, b], wrap(a + b));
  add("subtract", [a, b], wrap(a - b));
  add("multiply", [a, b], wrap(a * b));
  add("negate", [a], wrap(-a));
  add("less", [a, b], a < b);
  add("equal", [a, b], a === b);
  if (!(a === minimum && b === -1n)) {
    add("divide", [a, b], a / b);
    add("remainder", [a, b], a % b);
  }
}

let random = 0x12345678n;
for (let index = 0; index < 32; index++) {
  random = wrap(random * 6364136223846793005n + 1442695040888963407n);
  const a = random;
  random = wrap(random * 6364136223846793005n + 1442695040888963407n);
  const b = random;
  const shift = BigInt(index * 7 - 80);
  const amount = shift & 63n;
  add("add", [a, b], wrap(a + b));
  add("multiply", [a, b], wrap(a * b));
  add("bit_mix", [a, b], wrap((a & b) ^ (a | b)));
  add("bit_not", [a], wrap(~a));
  add("shift_left", [a, shift], wrap(a << amount));
  add("shift_right", [a, shift], a >> amount);
  add("shift_unsigned", [a, shift], wrap(BigInt.asUintN(64, a) >> amount));
  add("divide", [a, b], a / b);
  add("remainder", [a, b], a % b);
}

for (const flag of [0, 1, 2, -1]) add("bool_not", [flag], !flag);
add("short_and", [false, 0n], false);
add("short_and", [true, 2n], true);
add("short_or", [true, 0n], true);
add("short_or", [false, 20n], false);
add("else_if", [true], 22n);
add("else_if", [false], 26n);
for (const a of [false, true]) {
  for (const b of [false, true]) {
    add("nested_if", [a, b], BigInt((a ? (b ? 10 : 20) : 30) + (b ? 1 : 2)));
  }
}
add("tail_sum", [1_000_000n], 500000500000n);
add("tail_sum", [-1n], 0n);
let polynomial = 0n;
for (let n = 1n; n <= 10_000n; n++) polynomial = wrap(polynomial + n ** 4n);
add("sum_polynomial", [10_000n], polynomial);
add("tail_swap", [1_000_001n], 21n);
add("tail_swap", [2n], 12n);
add("tail_pipe", [1_000_000n], 0n);
add("mutual_recursion", [100n], true);
add("mutual_recursion", [101n], false);
add("higher_order", [true, 3n], 81n);
add("higher_order", [false, -5n], -5n);
add("function_record", [true, 8n], 64n);
add("function_record", [false, 8n], -8n);
add("function_array", [0n, 9n], 81n);
add("function_array", [1n, 9n], -9n);
add("functional_sum", [1n], 30n);
add("functional_sum", [-2n], 6n);
add("tail_array", [1_000_000n], 500000n);
add("index", [0n], 10n);
add("index", [3n], 40n);
add("large_array", [10n, 63n], 73n);
add("large_array", [maximum, 2n], wrap(maximum + 2n));
add("nested_array", [1n], 4n);
add("bool_array", [1n], false);
add("bool_array", [2n], true);
add("empty_values", [], 2n);
add("empty_index", [0n], true);
add("empty_index", [1n], true);
add("unit_index", [0n], true);
add("unit_index", [1n], true);
add("distance", [3, 4], 5);
add("rounded", [-1.2], -2);
add("ceiling", [-1.2], -1);
add("absolute", [-0], 0);
add("absolute", [-3.5], 3.5);
for (const value of [NaN, Infinity, -Infinity, 0, -0, 3.9, -3.9, 2 ** 63, -(2 ** 63), 2 ** 63 - 1024]) {
  const expected = Number.isNaN(value) ? 0n
    : value >= 2 ** 63 ? maximum
      : value <= -(2 ** 63) ? minimum : BigInt(Math.trunc(value));
  add("convert_int", [value], expected);
}
for (const value of [minimum, maximum, 0n, 42n]) add("convert_float", [value], Number(value));
add("float_divide", [1, 0], Infinity);
add("float_divide", [0, 0], NaN);
add("float_divide", [1, -0], -Infinity);
add("float_equal", [NaN, NaN], false);
add("float_not_equal", [NaN, 1], true);
add("float_less", [NaN, 1], false);
add("float_order", [1e16, -1e16, 1], 1);
add("negative_zero", [], -0);
add("require", [true], undefined);
add("unit_equal", [], true);
add("unused_field", [1n], 42n);
add("unused_array", [1n], 1n);

const traps = [
  { name: "divide", args: [1n, 0n] },
  { name: "divide", args: [minimum, -1n] },
  { name: "remainder", args: [minimum, -1n] },
  { name: "index", args: [-1n] },
  { name: "index", args: [4n] },
  { name: "index", args: [maximum] },
  { name: "function_array", args: [2n, 0n] },
  { name: "bool_array", args: [3n] },
  { name: "require", args: [false] },
  { name: "short_and", args: [true, 0n] },
  { name: "short_or", args: [false, 0n] },
  { name: "unused_field", args: [0n] },
  { name: "unused_array", args: [0n] },
  { name: "empty_index", args: [2n] },
  { name: "unit_index", args: [-1n] },
];

function hostSource() {
  const body = cases.map((test) => {
    const invocation = call(test);
    if (test.kind === "i64") return `printf("%lld\\n", (long long)${invocation});`;
    if (test.kind === "bool") return `printf("%d\\n", (int)${invocation});`;
    if (test.kind === "unit") return `${invocation}; puts("unit");`;
    return `printf("%.17g\\n", ${invocation});`;
  }).join("\n  ");
  return `
#include <math.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include "semantics.h"
static void trapped(int signal_number) { (void)signal_number; _Exit(86); }
int main(int argc, char **argv) {
  if (argc > 1) {
    signal(SIGILL, trapped);
#ifdef SIGTRAP
    signal(SIGTRAP, trapped);
#endif
    switch (atoi(argv[1])) {
      ${traps.map((test, index) => `case ${index}: ${call(test)}; break;`).join("\n      ")}
      default: return 2;
    }
    return 0;
  }
  ${body}
  return 0;
}
`;
}

function nativeValue(text, kind) {
  if (kind === "i64") return BigInt(text);
  if (kind === "unit") { assert.equal(text, "unit"); return undefined; }
  if (/nan/i.test(text)) return NaN;
  if (/inf/i.test(text)) return text.startsWith("-") ? -Infinity : Infinity;
  return Number(text);
}

function diagnostic(source, code) {
  const directory = join(temporary, "diagnostics");
  mkdirSync(directory, { recursive: true });
  const input = join(directory, "Invalid.tz");
  writeFileSync(input, source);
  const result = cli(["check", input, "--json"], { success: false });
  assert.equal(result.status, 1, result.stderr);
  assert.equal(result.stdout, "");
  const errors = result.stderr.trim().split("\n").map(JSON.parse);
  assert.ok(errors.every((error) => error.severity === "error" || error.severity === "note"));
  const error = errors[0];
  assert.equal(error.severity, "error");
  assert.equal(error.code, code);
  assert.equal(error.path, input);
  return error;
}

function multipleDiagnosticChecks() {
  const directory = join(temporary, "multiple-diagnostics");
  mkdirSync(directory);
  const before = join(directory, "Before.tz");
  const main = join(directory, "Main.tz");
  const output = join(directory, "output");
  writeFileSync(before, "def first :: i64\nfn first = true\n");
  writeFileSync(main, "def bad :: Missing -> i64\nfn bad x = x\ndef main :: i64\nfn main = bad 1\n");
  writeFileSync(output, "preserved");
  let expected;
  for (const args of [
    ["check", directory, "--json"],
    ["build", directory, "-o", output, "--json"],
    ["run", directory, "--json"],
  ]) {
    const result = cli(args, {
      success: false, env: { TSUZURI_CLANG: join(directory, "must-not-run") },
    });
    assert.equal(result.status, 1);
    assert.equal(result.stdout, "");
    const errors = result.stderr.trim().split("\n").map(JSON.parse);
    assert.deepEqual(errors.map(({ code, path }) => [code, path]), [
      ["E1003", before], ["E1004", main],
    ]);
    expected ??= errors;
    assert.deepEqual(errors, expected);
    assert.equal(readFileSync(output, "utf8"), "preserved");
  }
  const human = cli(["check", directory], { success: false }).stderr;
  assert.equal((human.match(/error\[E/g) ?? []).length, 2);
  assert.ok(human.includes("\n\n"));
  assert.equal(readdirSync(directory).some((name) => name.startsWith(".tsuzuri-")), false);
  writeFileSync(before, "");
  for (const [count, note] of [
    [62, "12 more errors not shown"],
    [1003, "at least 950 more errors not shown"],
  ]) {
    writeFileSync(main, Array.from({ length: count }, (_, index) =>
      `def f${index} :: i64\nfn f${index} = true\n`).join(""));
    const result = cli(["check", directory, "--json"], { success: false });
    assert.equal(result.status, 1);
    const errors = result.stderr.trim().split("\n").map(JSON.parse);
    assert.equal(errors.length, 51);
    assert.deepEqual(errors.at(-1), { severity: "note", message: note });
    assert.ok(errors.slice(0, 50).every((error) => error.code === "E1003" && error.path === main));
    assert.equal(cli(["check", directory, "--json"], { success: false }).stderr, result.stderr);
    assert.ok(cli(["check", directory], { success: false }).stderr.endsWith(`error: ${note}\n`));
  }
  for (const [source, code] of [
    ["fn first =\nrecord R { x: i64 }\nfn second =\n", "E0002"],
    ["? ?", "E0001"],
    ['def consume :: string -> unit\nfn consume s = ()\ndef a :: unit\nfn a = { let s = "a"; consume s; consume s }\ndef b :: unit\nfn b = { let s = "b"; consume s; consume s }', "E1012"],
  ]) {
    writeFileSync(main, source);
    const result = cli(["check", directory, "--json"], { success: false });
    assert.equal(result.status, 1);
    const errors = result.stderr.trim().split("\n").map(JSON.parse);
    assert.equal(errors.length, 2, result.stderr);
    assert.ok(errors.every((error) => error.code === code));
  }
}

async function runtimeChecks() {
  const runtime = readFileSync(join(root, "src/runtime/wasm.ll"), "utf8");
  const operations = [
    ["multiply", "__multi3", (a, b) => a * b],
    ["unsigned_divide", "__udivti3", (a, b) => a / b],
    ["unsigned_remainder", "__umodti3", (a, b) => a % b],
    ["signed_divide", "__divti3", (a, b) => BigInt.asIntN(128, a) / BigInt.asIntN(128, b)],
    ["signed_remainder", "__modti3", (a, b) => BigInt.asIntN(128, a) % BigInt.asIntN(128, b)],
    ["divmod", "__udivmodti4", (a, b) => a % b],
    ["divmod_null", "__udivmodti4", (a, b) => a / b],
  ];
  const wrappers = operations.flatMap(([name, helper]) => [0, 1].map((half) => `
define i64 @${name}_${half}(i64 %al, i64 %ah, i64 %bl, i64 %bh) {
  %a0 = zext i64 %al to i128
  %a1 = zext i64 %ah to i128
  %a2 = shl i128 %a1, 64
  %a = or i128 %a0, %a2
  %b0 = zext i64 %bl to i128
  %b1 = zext i64 %bh to i128
  %b2 = shl i128 %b1, 64
  %b = or i128 %b0, %b2
  ${name === "divmod" ? `%slot = alloca i128
  %quotient = call i128 @${helper}(i128 %a, i128 %b, ptr %slot)
  %value = load i128, ptr %slot`
    : `%value = call i128 @${helper}(i128 %a, i128 %b${name === "divmod_null" ? ", ptr null" : ""})`}
  %shifted = lshr i128 %value, ${half * 64}
  %result = trunc i128 %shifted to i64
  ret i64 %result
}
`)).join("\n");
  const input = join(temporary, "runtime.ll");
  writeFileSync(input, `declare void @llvm.trap()\n${runtime}\n${wrappers}`);
  const pairs = [
    [0n, 1n], [1n, 1n], [(1n << 128n) - 1n, 1n],
    [(1n << 128n) - 1n, (1n << 128n) - 1n],
    [1n << 127n, (1n << 128n) - 1n],
    [1n << 127n, 1n << 127n],
    [(1n << 64n) - 1n, (1n << 64n) - 1n],
  ];
  let state = 42n;
  for (let index = 0; index < 100; index++) {
    state = BigInt.asUintN(128, state * 0x2360ed051fc65da44385df649fccf645n + 1n);
    const a = state;
    state = BigInt.asUintN(128, state * 0x2360ed051fc65da44385df649fccf645n + 1n);
    pairs.push([a, state || 1n]);
  }
  for (const optimization of [0, 3]) {
    const object = join(temporary, "runtime.o");
    const wasm = join(temporary, "runtime.wasm");
    execute(clang, ["--target=wasm32-unknown-unknown", "-mbulk-memory", "-Wno-override-module",
      `-O${optimization}`, "-c", input, "-o", object]);
    execute(process.env.TSUZURI_WASM_LD ?? "wasm-ld", [
      "--no-entry", "--stack-first", "--strip-all", object, "-o", wasm,
      ...operations.flatMap(([name]) => [0, 1].map((half) => `--export=${name}_${half}`)),
    ]);
    const { instance, module } = await WebAssembly.instantiate(readFileSync(wasm));
    assert.deepEqual(WebAssembly.Module.imports(module), []);
    for (const [a, b] of pairs) {
      const args = [a, a >> 64n, b, b >> 64n].map((value) => BigInt.asIntN(64, value));
      for (const [name, , operation] of operations) {
        const expected = BigInt.asUintN(128, operation(a, b));
        const low = BigInt.asUintN(64, instance.exports[`${name}_0`](...args));
        const high = BigInt.asUintN(64, instance.exports[`${name}_1`](...args));
        assert.equal(low | (high << 64n), expected, `${name}, runtime -O${optimization}: ${a}, ${b}`);
      }
    }
    for (const [name] of operations.filter(([name]) => name !== "multiply")) {
      assert.throws(() => instance.exports[`${name}_0`](1n, 0n, 0n, 0n), WebAssembly.RuntimeError);
    }
  }
  console.log(`LLVM runtime: ${pairs.length} full-width 128-bit cases per operation at -O0/-O3`);
}

async function polymorphismChecks() {
  const source = join(root, "tests/fixtures/polymorphism/Main.tz");
  const directory = join(temporary, "polymorphism");
  mkdirSync(directory);
  const header = join(directory, "polymorphism.h");
  const host = join(directory, "host.c");
  cli(["build", source, "--emit", "header", "-o", header]);
  assert.doesNotMatch(readFileSync(header, "utf8"), /tz_add\b|\$mono/);
  writeFileSync(host, `
#include <assert.h>
#include "polymorphism.h"
int main(void) {
  assert(tz_integer(20, 22) == 42);
  assert(tz_integer(INT32_MAX, 1) == INT32_MIN);
  assert(tz_floating(1.5, 2.25) == 3.75);
  assert(tz_narrow(127) == -128);
  assert(tz_decimal() == 0.3);
  assert(tz_wide() == 42);
  assert(tz_parametric() == 42);
  assert(tz_classes() == 42);
  assert(tz_intrinsic() == 42);
  assert(tz_recursion() == INT64_C(2000000));
  assert(tz_owned() == 11);
  assert(tz_borrowed() == 8);
  assert(tz_ordered() == 12);
  assert(tz_constrained_integer() == 42);
  assert(tz_constrained_floating(3.0, 4.0) == 5.0);
  assert(tz_constrained_floating(-3.0, 4.0) == 5.0);
  assert(tz_constrained_floating(0.0, 0.0) == 0.0);
  assert(tz_constrained_partial() == 10.0);
  assert(tz_constrained_owned() == 5);
  assert(tz_constrained_union() == 42);
  return 0;
}
`);
  for (const optimization of [0, 3]) {
    assert.equal(cli(["run", source, `-O${optimization}`]).stdout, "42\n");
    const object = join(directory, `polymorphism-${optimization}.o`);
    const wasm = join(directory, `polymorphism-${optimization}.wasm`);
    const native = join(directory, `host-${optimization}${process.platform === "win32" ? ".exe" : ""}`);
    cli(["build", source, "--emit", "object", `-O${optimization}`, "-o", object]);
    execute(clang, ["-std=c11", "-Wall", "-Wextra", "-Werror", host, object, "-o", native,
      ...(process.platform === "win32" ? [] : ["-lm"])]);
    execute(native, []);
    cli(["build", source, "--target", "wasm32", `-O${optimization}`, "-o", wasm]);
    const { instance, module } = await WebAssembly.instantiate(readFileSync(wasm));
    assert.deepEqual(WebAssembly.Module.imports(module), []);
    const api = instance.exports;
    assert.equal(api.tz_integer(20, 22), 42);
    assert.equal(api.tz_integer(2147483647, 1), -2147483648);
    assert.equal(api.tz_floating(1.5, 2.25), 3.75);
    assert.equal(api.tz_narrow(127), -128);
    assert.equal(api.tz_decimal(), 0.3);
    assert.equal(api.tz_wide(), 42n);
    assert.equal(api.tz_parametric(), 42);
    assert.equal(api.tz_classes(), 42);
    assert.equal(api.tz_intrinsic(), 42);
    assert.equal(api.tz_recursion(), 2000000n);
    assert.equal(api.tz_owned(), 11n);
    assert.equal(api.tz_borrowed(), 8n);
    assert.equal(api.tz_ordered(), 12n);
    assert.equal(api.tz_constrained_integer(), 42);
    assert.equal(api.tz_constrained_floating(3.0, 4.0), 5.0);
    assert.equal(api.tz_constrained_floating(-3.0, 4.0), 5.0);
    assert.equal(api.tz_constrained_floating(0.0, 0.0), 0.0);
    assert.equal(api.tz_constrained_partial(), 10.0);
    assert.equal(api.tz_constrained_owned(), 5n);
    assert.equal(api.tz_constrained_union(), 42n);
  }
  console.log("Polymorphism: signatures, specialization, class/function constraints, ownership, evaluation order and tail recursion at -O0/-O3");
}

async function moduleChecks() {
  const directory = join(temporary, "modules");
  mkdirSync(directory);
  const main = join(directory, "Main.tz");
  const point = join(directory, "Point.tz");
  const pointSource = `
record Point { x: f64, y: f64 }
fn distance(point: Point) -> f64 {
  sqrt(point.x * point.x + point.y * point.y)
}
export fn hypotenuse(x: f64, y: f64) -> f64 {
  distance(Point { x: x, y: y })
}
`;
  writeFileSync(point, pointSource);
  writeFileSync(main, "let p = Point { x: 10.0, y: 20.5}\nlet d = Point.distance(p)");
  cli(["check", directory]);
  for (const optimization of [0, 3]) {
    assert.equal(cli(["run", main, `-O${optimization}`]).stdout, "");
  }
  for (const [source, expected] of [
    ["let x = 40\n(x + 2)", "42\n"],
    ["let x = 40\n-x", "-40\n"],
    ["let x = 40 +\n2\nx", "42\n"],
    ["let x = (40\n+ 2)\nx", "42\n"],
    ["fn f() -> i64 { 42 }\nlet g = f\n{ g() }", "42\n"],
    ["fn f() -> i64 { 42 }\nlet g = Main.f\n{ g() }", "42\n"],
  ]) {
    writeFileSync(main, source);
    assert.equal(cli(["run", main]).stdout, expected);
  }
  writeFileSync(main, "let _ = assert(false)");
  for (const optimization of [0, 3]) {
    const result = cli(["run", main, `-O${optimization}`, "--json"], { success: false });
    assert.equal(result.status, 1);
    assert.equal(JSON.parse(result.stderr).code, "E2005");
    assert.equal(JSON.parse(result.stderr).path, main);
  }
  const library = join(directory, "no-auto-entry.wasm");
  cli(["build", main, "--target", "wasm32", "-o", library]);
  assert.equal((await WebAssembly.instantiate(readFileSync(library))).instance.exports.tz_hypotenuse(3, 4), 5);

  const sources = {
    "Right.tz": `
record Value { x: i64 }
fn value(v: Value) -> i64 { v.x + 1 }
`,
    "Odd.tz": "fn rec test(n: i64) -> bool { if n == 0 { false } else { Even.test(n - 1) } }",
    "Loop.tz": `
fn rec sum(n: i64, values: [i64]) -> i64 {
  let total = values[0];
  if n == 0 { total } else { Loop.sum(n - 1, [total + n, values[1]]) }
}
fn rec down(n: i64) -> i64 { if n == 0 { 0 } else { n - 1 |> Loop.down } }
`,
    "Left.tz": `
record Value { x: i64 }
fn value(v: Value) -> i64 { v.x }
fn main() -> i64 { 999 }
`,
    "Even.tz": "fn rec test(n: i64) -> bool { if n == 0 { true } else { Odd.test(n - 1) } }",
    "Main.tz": `
record Callback { distance: fn(Point.Point) -> f64 }
fn apply(f: fn(Point) -> f64, p: Point) -> f64 { p |> f }
export fn module_result() -> f64 {
  let p: Point.Point = Point.Point { x: 3.0, y: 4.0 };
  let functions = [Point.distance];
  let Point = Callback { distance: functions[0] };
  let left: Left.Value = Left.Value { x: 20 };
  let right = Right.Value { x: 16 };
  apply(Point.distance, p) + to_float(Left.value(left) + Right.value(right))
}
export fn module_tail() -> i64 { Loop.sum(1_000_000, [0, 1]) }
export fn module_pipe() -> i64 { Loop.down(1_000_000) }
export fn module_even(n: i64) -> bool { Even.test(n) }
fn main() -> f64 { module_result() }
`,
  };
  for (const [name, source] of Object.entries(sources)) {
    writeFileSync(join(directory, name), source);
  }
  const header = join(directory, "modules.h");
  const host = join(directory, "host.c");
  cli(["build", directory, "--emit", "header", "-o", header]);
  writeFileSync(host, `
#include <assert.h>
#include "modules.h"
int main(void) {
  assert(tz_hypotenuse(3.0, 4.0) == 5.0);
  assert(tz_module_result() == 42.0);
  assert(tz_module_tail() == INT64_C(500000500000));
  assert(tz_module_pipe() == 0);
  assert(tz_module_even(100) == 1);
  assert(tz_module_even(101) == 0);
  return 0;
}
`);
  for (const optimization of [0, 3]) {
    assert.equal(cli(["run", directory, `-O${optimization}`]).stdout, "42\n");
    const object = join(directory, `modules-${optimization}.o`);
    const wasm = join(directory, `modules-${optimization}.wasm`);
    const native = join(directory, `host-${optimization}${process.platform === "win32" ? ".exe" : ""}`);
    cli(["build", point, "--emit", "object", `-O${optimization}`, "-o", object]);
    execute(clang, ["-std=c11", host, object, "-o", native, ...(process.platform === "win32" ? [] : ["-lm"])]);
    execute(native, []);
    cli(["build", main, "--target", "wasm32", `-O${optimization}`, "-o", wasm]);
    const { instance, module } = await WebAssembly.instantiate(readFileSync(wasm));
    assert.deepEqual(WebAssembly.Module.imports(module), []);
    assert.equal(instance.exports.tz_hypotenuse(3, 4), 5);
    assert.equal(instance.exports.tz_module_result(), 42);
    assert.equal(instance.exports.tz_module_tail(), 500000500000n);
    assert.equal(instance.exports.tz_module_pipe(), 0n);
    assert.equal(instance.exports.tz_module_even(100n), 1);
    assert.equal(instance.exports.tz_module_even(101n), 0);
    assert.equal(instance.exports.tz_distance, undefined);
  }

  cli(["build", directory, "--emit", "llvm"]);
  const ir = readFileSync(join(directory, "Main.ll"), "utf8");
  const repeat = join(directory, "repeat.ll");
  cli(["build", main, "--emit", "llvm", "-o", repeat]);
  assert.equal(readFileSync(repeat, "utf8"), ir);
  assert.match(ir, /@tz\.fn\.Left\.value/);
  assert.match(ir, /@tz\.fn\.Right\.value/);
  const wrongEntry = cli(["run", point, "--json"], { success: false });
  assert.equal(wrongEntry.status, 1);
  assert.equal(JSON.parse(wrongEntry.stderr).code, "E2004");
  assert.equal(JSON.parse(wrongEntry.stderr).path, point);

  const broken = join(directory, "Broken.tz");
  for (const [source, code, line] of [
    ["// 日本語\nfn broken() -> i64 { false }", "E1003", 2],
    ["fn broken() -> i64 { ? }", "E0001", 1],
    ["fn broken() -> i64 {", "E0002", 1],
  ]) {
    writeFileSync(broken, source);
    const result = cli(["check", main, "--json"], { success: false });
    assert.equal(result.status, 1);
    const error = JSON.parse(result.stderr);
    assert.equal(error.code, code);
    assert.equal(error.path, broken);
    assert.equal(error.span.line, line);
    if (code === "E1003") {
      assert.equal(error.span.start, Buffer.byteLength(source.slice(0, source.indexOf("false"))));
    }
  }
  rmSync(broken);

  if (process.platform !== "win32") {
    const alias = join(directory, "point-alias.ll");
    linkSync(point, alias);
    const result = cli(["build", main, "--emit", "llvm", "-o", alias, "--json"], { success: false });
    assert.equal(result.status, 1);
    assert.equal(JSON.parse(result.stderr).code, "E2003");
    assert.equal(readFileSync(point, "utf8"), pointSource);
  }

  const badName = join(directory, "Bad-Name.tz");
  writeFileSync(badName, "fn value() -> i64 { 0 }");
  const invalidName = cli(["check", badName, "--json"], { success: false });
  assert.equal(invalidName.status, 1);
  assert.equal(JSON.parse(invalidName.stderr).code, "E1011");
  assert.equal(JSON.parse(invalidName.stderr).path, badName);
  rmSync(badName);

  const unsupported = join(directory, "Unsupported.tsz");
  writeFileSync(unsupported, "fn main() -> i64 { 0 }");
  const rejected = cli(["check", unsupported, "--json"], { success: false });
  assert.equal(rejected.status, 1);
  const extensionError = JSON.parse(rejected.stderr);
  assert.equal(extensionError.code, "E2000");
  assert.match(extensionError.message, /\.tz/);
  console.log("Modules: file namespaces, Main.tz entry, higher-order calls, records, recursion, per-file diagnostics, source protection");
}

try {
  cli(["check", fixture], { env: { TSUZURI_CLANG: join(temporary, "missing-clang") } });
  const header = join(temporary, "semantics.h");
  cli(["build", fixture, "--emit", "header", "-o", header]);
  const host = join(temporary, "host.c");
  writeFileSync(host, hostSource());
  execute(clang, ["-x", "c++", "-std=c++17", "-fsyntax-only", host]);

  for (const optimization of [0, 3]) {
    const object = join(temporary, `semantics-${optimization}.o`);
    const wasm = join(temporary, `semantics-${optimization}.wasm`);
    const native = join(temporary, `host-${optimization}${process.platform === "win32" ? ".exe" : ""}`);
    cli(["build", fixture, "--emit", "object", `-O${optimization}`, "-o", object]);
    cli(["build", fixture, "--target", "wasm32", `-O${optimization}`, "-o", wasm]);
    execute(clang, ["-std=c11", "-O2", host, object, "-o", native, ...(process.platform === "win32" ? [] : ["-lm"])]);
    const bytes = readFileSync(wasm);
    assert.equal(WebAssembly.validate(bytes), true);
    const { module, instance } = await WebAssembly.instantiate(bytes);
    assert.deepEqual(WebAssembly.Module.imports(module), []);
    const expectedExports = [...readFileSync(fixture, "utf8").matchAll(/export fn (\w+)/g)]
      .map((match) => `tz_${match[1]}`).sort();
    assert.deepEqual(
      WebAssembly.Module.exports(module).filter((entry) => entry.kind === "function").map((entry) => entry.name).sort(),
      expectedExports,
    );
    const nativeResults = execute(native, []).stdout.trimEnd().split(/\r?\n/);
    assert.equal(nativeResults.length, cases.length);
    cases.forEach((test, index) => {
      const label = `-O${optimization}: ${test.name}(${test.args.map(String).join(", ")})`;
      let actualWasm;
      assert.doesNotThrow(() => {
        actualWasm = instance.exports[`tz_${test.name}`](...test.args);
      }, `WASM ${label}`);
      assert.deepEqual(actualWasm, test.expected, `WASM ${label}`);
      const actualNative = nativeValue(nativeResults[index], test.kind);
      assert.deepEqual(actualNative, test.expected, `native ${label}`);
    });
    traps.forEach((test, index) => {
      assert.throws(() => instance.exports[`tz_${test.name}`](...test.args), WebAssembly.RuntimeError, test.name);
      const result = execute(native, [String(index)], { success: false });
      assert.equal(result.status, 86, `native trap: ${test.name}, signal ${result.signal}`);
    });
    const repeated = join(temporary, `repeat-${optimization}.wasm`);
    cli(["build", fixture, "--target", "wasm32", `-O${optimization}`, "-o", repeated]);
    assert.deepEqual(readFileSync(repeated), bytes, "WASM output must be reproducible");
    console.log(`-O${optimization}: ${cases.length} native/WASM results, ${traps.length} traps, ${bytes.length} bytes, no imports`);
  }
  await runtimeChecks();
  await moduleChecks();
  await polymorphismChecks();
  multipleDiagnosticChecks();

  for (const [type, value, expected] of [
    ["i64", "-9223372036854775808", "-9223372036854775808\n"],
    ["f64", "1.25", "1.25\n"],
    ["bool", "true", "true\n"],
    ["unit", "()", ""],
  ]) {
    const directory = join(temporary, `scalar-${type}`);
    mkdirSync(directory);
    const input = join(directory, "Main.tz");
    writeFileSync(input, `fn main() -> ${type} { ${value} }`);
    assert.equal(cli(["run", input]).stdout, expected);
  }
  assert.equal(cli([], { success: false }).status, 2);
  assert.match(cli(["--version"]).stdout, /^tsuzuri 0\.1\.0/);
  assert.match(cli(["--help"]).stdout, /TSUZURI_WASM_LD/);
  assert.equal(cli(["run", fixture, "--target", "wasm32"], { success: false }).status, 2);
  assert.equal(diagnostic("// 日本語\r\nfn main() -> i64 { true }\r\n", "E1003").span.line, 2);
  diagnostic("fn f() -> i64 { let x = 1; x = 2; x }", "E1014");
  diagnostic("fn f() -> i64 { 1e400 }", "E1009");
  diagnostic("fn f() -> i64 { 1__2 }", "E0001");
  diagnostic("record R { r: R }", "E1010");
  diagnostic("def id :: 'a -> 'a\nfn id x = x\ndef f :: unit\nfn f = { let value = id; }", "E1015");
  diagnostic("class C<'a> { def f :: 'a -> i32 }\ninstance C<bool> {}", "E1018");
  diagnostic("def add :: 'a -> 'a -> 'a\nfn add x y = x + y\ndef f :: bool\nfn f = add true false", "E1005");
  diagnostic("fn add :: i32 -> i32 -> i32\nfn add x y = x + y", "E0002");
  diagnostic("def f :: Add<'a> -> 'a\nfn f x = x\ndef g :: bool\nfn g = f true", "E1005");
  diagnostic("def f :: i64\nfn f = { let x = 1; let g: &i64 -> unit = r -> { *r = 2; }; 0 }", "E1014");
  diagnostic(Buffer.from([0xff]), "E2001");
  diagnostic(" ".repeat(1024 * 1024 + 1), "E0003");

  const spaced = join(temporary, "space 日本語");
  mkdirSync(spaced);
  const input = join(spaced, "Main.tz");
  writeFileSync(input, "\uFEFFexport fn main() -> i64 {\r\n  42\r\n}\r\n");
  const output = join(spaced, "output.wasm");
  cli(["build", "--target", "wasm32", "-o", output, "--", input]);
  assert.equal((await WebAssembly.instantiate(readFileSync(output))).instance.exports.tz_main(), 42n);
  const dashOutput = join("-output", process.platform === "win32" ? "program.exe" : "program");
  cli(["build", input, "-o", dashOutput], { cwd: temporary });
  assert.equal(execute(join(temporary, dashOutput), []).stdout, "42\n");

  writeFileSync(output, "preserved");
  const failedCompiler = cli(["build", input, "-o", output, "--json"], {
    success: false, env: { TSUZURI_CLANG: join(temporary, "missing clang") },
  });
  assert.equal(JSON.parse(failedCompiler.stderr).code, "E2002");
  assert.equal(readFileSync(output, "utf8"), "preserved");
  const failedLinker = cli(["build", input, "--target", "wasm32", "-o", output, "--json"], {
    success: false, env: { TSUZURI_WASM_LD: join(temporary, "missing linker") },
  });
  assert.equal(JSON.parse(failedLinker.stderr).code, "E2002");
  assert.equal(readFileSync(output, "utf8"), "preserved");
  assert.equal(cli(["build", input, "--emit", "llvm", "-o", input], { success: false }).status, 1);
  assert.equal(cli(["build", input, "--emit", "llvm", "-o", spaced], { success: false }).status, 1);
  if (process.platform !== "win32") {
    const hardlink = join(spaced, "hardlink.ll");
    const symlink = join(spaced, "symlink.ll");
    linkSync(input, hardlink);
    symlinkSync(input, symlink);
    assert.equal(cli(["build", input, "--emit", "llvm", "-o", hardlink], { success: false }).status, 1);
    assert.equal(cli(["build", input, "--emit", "llvm", "-o", symlink], { success: false }).status, 1);
    const pipe = join(spaced, "Pipe.tz");
    execute("mkfifo", [pipe]);
    const pipeResult = cli(["check", pipe, "--json"], { success: false });
    assert.equal(JSON.parse(pipeResult.stderr).code, "E2001");
  }
  assert.match(readFileSync(input, "utf8"), /42/);
  assert.equal(readdirSync(spaced).some((name) => name.startsWith(".tsuzuri-")), false);

  const ir1 = join(temporary, "first.ll");
  const ir2 = join(temporary, "second.ll");
  cli(["build", fixture, "--emit", "llvm", "-o", ir1]);
  cli(["build", fixture, "--emit", "llvm", "-o", ir2]);
  assert.deepEqual(readFileSync(ir1), readFileSync(ir2));
  assert.equal(existsSync(join(dirname(fixture), "Semantics.o")), false);
  console.log("CLI: scalar entry points, UTF-8 paths, JSON diagnostics, output protection, cleanup, deterministic IR");
} finally {
  assert.match(basename(temporary), /^tsuzuri-e2e-/);
  rmSync(temporary, { recursive: true, force: true });
}
