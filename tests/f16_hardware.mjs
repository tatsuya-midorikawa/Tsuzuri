// f16 hardware paths (D10): the f16 operations that the compiler lowers to LLVM `half` instructions, against a
// reference that does not come from the compiler. The software path (`numeric.c`, which is what WASM always runs) is a
// second reference: the native results must match both, NaN sign and payload included.
//
//   node tests/f16_hardware.mjs target/release/tsuzuri [--quick]
//
// Every host prints the path it exercised: `hardware`, `conversions`, or `soft` for the code of `--cpu generic` and of
// `--cpu native`. The compiler picks the path from the target and the CPU, so a host that lacks a capability falls back
// to `soft` and the run still passes; `TSUZURI_F16_EXPECT` (this test's own variable, the compiler does not read it)
// turns a fallback into a failure: `hardware`, or `generic=soft,native=conversions` for one value per build.
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

const compiler = resolve(process.argv[2] ?? "target/release/tsuzuri");
const clang = process.env.TSUZURI_CLANG ?? "clang";
const wasmLinker = process.env.TSUZURI_WASM_LD ?? "wasm-ld";
const quick = process.argv.includes("--quick");
// A full run sweeps every binary16 pattern; a quick run takes every 61st and a quarter of the boundary values.
const stride = quick ? 61 : 1;
const windows = process.platform === "win32";
const temporary = mkdtempSync(join(tmpdir(), "tsuzuri-f16-"));

const execute = (program, args, options = {}) => {
  const result = spawnSync(program, args, { encoding: "utf8", timeout: 600_000, maxBuffer: 256 * 1024 * 1024, ...options });
  if (result.error) throw result.error;
  assert.equal(result.status, 0, `${program} ${args.join(" ")}\n${result.stderr}`);
  return result;
};
const cli = (args) => execute(compiler, args);

const PATHS = ["soft", "conversions", "hardware"];
function parseExpectation(text) {
  const expected = {};
  for (const part of (text ?? "").split(",").filter(Boolean)) {
    const [key, value] = part.includes("=") ? part.split("=") : ["both", part];
    assert.ok(["both", "generic", "native"].includes(key) && PATHS.includes(value), `TSUZURI_F16_EXPECT: "${part}" is not [generic=|native=]${PATHS.join("|")}`);
    for (const build of key === "both" ? ["generic", "native"] : [key]) expected[build] = value;
  }
  return expected;
}
const expectation = parseExpectation(process.env.TSUZURI_F16_EXPECT);

// ---------------------------------------------------------------------------------------------------------------
// The reference: binary16 by exact arithmetic on JavaScript numbers (doubles), which does not use the compiler.
// A sum, a difference, and a product of two binary16 values are exact in a double, and a quotient and a square root
// rounded to a double and then to binary16 round once: 53 bits are at least 2 × 11 + 2.
// ---------------------------------------------------------------------------------------------------------------
const view = new DataView(new ArrayBuffer(8));
const float32Bits = (value) => { view.setFloat32(0, value); return view.getUint32(0); };
const float32Value = (bits) => { view.setUint32(0, bits); return view.getFloat32(0); };
const float64Bits = (value) => { view.setFloat64(0, value); return view.getBigUint64(0); };
const float64Value = (bits) => { view.setBigUint64(0, bits); return view.getFloat64(0); };

const QUIET_NAN = 0x7e00;
const isNan = (bits) => (bits & 0x7fff) > 0x7c00;
function halfValue(bits) {
  const magnitude = bits & 0x7fff, exponent = magnitude >> 10, fraction = magnitude & 1023;
  const value = exponent === 31 ? (fraction ? NaN : Infinity) : exponent === 0 ? fraction * 2 ** -24 : (1024 + fraction) * 2 ** (exponent - 25);
  return bits & 0x8000 ? -value : value;
}
// The binary16 nearest to a double, ties to even; a NaN is the caller's business.
function halfRound(value) {
  const sign = value < 0 || Object.is(value, -0) ? 0x8000 : 0;
  const magnitude = Math.abs(value);
  if (magnitude >= 65520) return sign | 0x7c00;
  if (magnitude === 0) return sign;
  view.setFloat64(0, magnitude);
  const exponent = Math.max((view.getUint16(0) >> 4) - 1023, -14);
  const scaled = magnitude / 2 ** (exponent - 10);
  const whole = Math.floor(scaled), fraction = scaled - whole;
  const rounded = fraction > 0.5 || (fraction === 0.5 && whole % 2 === 1) ? whole + 1 : whole;
  return sign | (exponent === -14 ? rounded : ((exponent + 15) << 10) + (rounded - 1024));
}
const productSign = (a, b) => (a ^ b) & 0x8000;
function arithmetic(name, a, b) {
  const nan = QUIET_NAN | ((name === "mul" || name === "div" ? productSign(a, b) : a) & 0x8000);
  if (isNan(a) || isNan(b)) return nan;
  const x = halfValue(a), y = halfValue(b);
  const value = name === "add" ? x + y : name === "sub" ? x - y : name === "mul" ? x * y : x / y;
  return Number.isNaN(value) ? nan : halfRound(value);
}
function comparison(name, a, b) {
  const x = halfValue(a), y = halfValue(b);
  return Number(name === "eq" ? x === y : name === "ne" ? x !== y : name === "lt" ? x < y : name === "le" ? x <= y : name === "gt" ? x > y : x >= y);
}
function squareRoot(a) {
  if (isNan(a)) return a;
  const x = halfValue(a);
  if (x === 0) return a;
  return x < 0 ? QUIET_NAN : halfRound(Math.sqrt(x));
}
const widen32 = (a) => isNan(a) ? (0x7fc00000 | ((a & 0x8000) << 16)) >>> 0 : float32Bits(halfValue(a));
const widen64 = (a) => isNan(a) ? 0x7ff8000000000000n | (BigInt(a & 0x8000) << 48n) : float64Bits(halfValue(a));
function narrow32(bits) {
  const value = float32Value(bits);
  return Number.isNaN(value) ? QUIET_NAN | ((bits >>> 16) & 0x8000) : halfRound(value);
}
function narrow64(bits) {
  const value = float64Value(bits);
  return Number.isNaN(value) ? QUIET_NAN | Number((bits >> 48n) & 0x8000n) : halfRound(value);
}

// ---------------------------------------------------------------------------------------------------------------
// The inputs.
// ---------------------------------------------------------------------------------------------------------------
let state = 0x2545f491;
const random = () => { state ^= state << 13; state >>>= 0; state ^= state >>> 17; state ^= state << 5; return state >>> 0; };
const randomHalves = () => {
  const a = random() & 0xffff;
  const near = Math.min(30, Math.max(0, ((a >> 10) & 31) + (random() % 13) - 6));
  switch (random() & 3) {
    case 0: return [a, random() & 0xffff];
    case 1: return [a, (a & 0xfc00) | (random() & 0x83ff)];
    case 2: return [a, (random() & 0x8000) | (near << 10) | (random() & 1023)];
    default: return [a, ((random() & 0x8000) | ((random() % 31) << 10) | (random() & 1023))];
  }
};
const magnitudes = [0x0000, 0x0001, 0x0002, 0x0200, 0x03ff, 0x0400, 0x0401, 0x07ff, 0x1400, 0x1401, 0x3800, 0x3bff, 0x3c00, 0x3c01, 0x3c02, 0x3e00, 0x4000, 0x4200, 0x4c00, 0x5bff, 0x5c00, 0x7800, 0x7bfe, 0x7bff, 0x7c00, 0x7c01, 0x7d00, 0x7dff, 0x7e00, 0x7e01, 0x7f00, 0x7fff];
const boundary = magnitudes.flatMap((magnitude) => [magnitude, magnitude | 0x8000]);
assert.equal(boundary.length, 64);

// The midpoints of neighbouring binary16 values, which are exactly the f32 ties of a narrowing, and the values next to
// them. `limit` is the value that follows the largest finite binary16, 2^16, so the last midpoint is 65520.
const midpoints = [];
for (let bits = 0; bits <= 0x7bff; bits++) midpoints.push((halfValue(bits) + (bits === 0x7bff ? 65536 : halfValue(bits + 1))) / 2);
const nextFloat32 = (bits, up) => (bits + (up ? 1 : -1)) >>> 0;

function narrow32Inputs() {
  const inputs = new Set();
  for (let bits = 0; bits <= 0xffff; bits += stride) inputs.add(widen32(bits));
  for (let index = 0; index < midpoints.length; index += stride) {
    const bits = float32Bits(midpoints[index]);
    for (const value of [bits, nextFloat32(bits, true), nextFloat32(bits, false)]) { inputs.add(value); inputs.add((value | 0x80000000) >>> 0); }
  }
  for (const value of [65504, 65519.99609375, 65520, 65536, 3.4028234663852886e38, 2 ** -24, 2 ** -25, 2 ** -26, 2 ** -14, 1e-30, 1, 0.1, 1 / 3, Infinity, 0]) {
    const bits = float32Bits(value);
    for (const near of [bits, nextFloat32(bits, true), nextFloat32(bits, false)]) { inputs.add(near); inputs.add((near | 0x80000000) >>> 0); }
  }
  for (const bits of [0x7fc00001, 0x7f800001, 0xffffffff, 0xff800001, 0x7fffffff, 0xffc00000, 0x7fc00000, 0x80000000, 0x7f800000, 0xff800000, 0x00000001, 0x80000001]) inputs.add(bits);
  for (let count = 0; count < (quick ? 300 : 65536); count++) {
    inputs.add(random());
    inputs.add((((random() & 1) << 31) | ((127 + (random() % 48) - 30) << 23) | (random() & 0x7fffff)) >>> 0);
  }
  return [...inputs];
}

// The doubles that a detour through binary32 narrows wrongly: a midpoint m with m × 2^-30 added or taken away rounds to
// m in binary32, whose tie then goes to the even neighbour, which is the wrong one half of the time.
const trapped = [];
const traps = [];
for (const midpoint of midpoints) {
  for (const value of [midpoint * (1 + 2 ** -30), midpoint * (1 - 2 ** -30), -midpoint * (1 + 2 ** -30), -midpoint * (1 - 2 ** -30)]) {
    traps.push(value);
    if (halfRound(Math.fround(value)) !== halfRound(value)) trapped.push(value);
  }
}
assert.ok(trapped.length >= 1000, `the double rounding traps catch a detour through binary32 (${trapped.length})`);

function narrow64Inputs(narrow32List) {
  const inputs = new Set();
  for (const bits of narrow32List) {
    const value = float32Value(bits);
    inputs.add(Number.isNaN(value) ? 0x7ff8000000000000n | (BigInt(bits >>> 31) << 63n) | (BigInt(bits & 0x7fffff) << 29n) : float64Bits(value));
  }
  for (let index = 0; index < traps.length; index += stride) inputs.add(float64Bits(traps[index]));
  for (const value of trapped.filter((_, index) => index % (quick ? 97 : 1) === 0)) inputs.add(float64Bits(value));
  for (const value of [65519.99999999999, 65520, 65520.00000000001, 2 ** -25 * (1 + 2 ** -40), 2 ** -25 * (1 - 2 ** -40), 1.0004882812500001, 1.00048828125, 1.0004882812499999, 1e-300, 5e-324, 1.7976931348623157e308])
    for (const signed of [value, -value]) inputs.add(float64Bits(signed));
  for (const bits of [0x7ff8000000000001n, 0x7ff0000000000001n, 0xffffffffffffffffn, 0xfff0000000000001n, 0x7ff0000000000000n, 0xfff0000000000000n]) inputs.add(bits);
  for (let count = 0; count < (quick ? 300 : 65536); count++) {
    inputs.add((BigInt(random()) << 32n) | BigInt(random()));
    inputs.add(float64Bits((random() / 2 ** 32 - 0.5) * 2 ** (random() % 40 - 20)));
  }
  return [...inputs];
}
const narrowing32 = narrow32Inputs(), narrowing64 = narrow64Inputs(narrowing32);

// ---------------------------------------------------------------------------------------------------------------
// Operations: the Tsuzuri definitions that the wrapper calls, and the reference of each.
// ---------------------------------------------------------------------------------------------------------------
const arithmeticNames = ["add", "sub", "mul", "div"];
const comparisonNames = ["eq", "ne", "lt", "le", "gt", "ge"];
const symbols = { add: "+", sub: "-", mul: "*", div: "/", eq: "==", ne: "!=", lt: "<", le: "<=", gt: ">", ge: ">=" };
const operations = [
  ...arithmeticNames.map((name) => ({ name, arguments: ["f16", "f16"], result: "f16", source: `value0 ${symbols[name]} value1`, reference: (a, b) => BigInt(arithmetic(name, a, b)) })),
  ...comparisonNames.map((name) => ({ name, arguments: ["f16", "f16"], result: "bool", source: `value0 ${symbols[name]} value1`, reference: (a, b) => BigInt(comparison(name, a, b)) })),
  { name: "sqrt", arguments: ["f16"], result: "f16", source: "Math.sqrt value0", reference: (a) => BigInt(squareRoot(a)) },
  { name: "widen32", arguments: ["f16"], result: "f32", source: "value0 as f32", reference: (a) => BigInt(widen32(a)) },
  { name: "widen64", arguments: ["f16"], result: "f64", source: "value0 as f64", reference: (a) => widen64(a) },
  { name: "narrow32", arguments: ["f32"], result: "f16", source: "value0 as f16", reference: (a) => BigInt(narrow32(Number(a))) },
  { name: "narrow64", arguments: ["f64"], result: "f16", source: "value0 as f16", reference: (a) => BigInt(narrow64(a)) },
];
operations.forEach((operation, id) => { operation.id = id; });
const operation = Object.fromEntries(operations.map((entry) => [entry.name, entry]));

class Cases {
  constructor() { this.capacity = 1 << 20; this.input = new BigUint64Array(this.capacity * 3); this.expected = new BigUint64Array(this.capacity); this.length = 0; }
  // `a` and `b` are numbers, except the operand of a narrowing from f64, which is a BigInt of the 64 bits.
  add(name, a, b = 0) {
    if (this.length === this.capacity) {
      const input = new BigUint64Array(this.capacity * 6), expected = new BigUint64Array(this.capacity * 2);
      input.set(this.input); expected.set(this.expected);
      this.input = input; this.expected = expected; this.capacity *= 2;
    }
    const { id, reference } = operation[name];
    this.input[this.length * 3] = BigInt(id); this.input[this.length * 3 + 1] = BigInt(a); this.input[this.length * 3 + 2] = BigInt(b);
    this.expected[this.length++] = reference(a, b);
  }
}
const pairValues = quick ? boundary.filter((_, index) => index % 4 === 0) : boundary;
function buildCases() {
  const cases = new Cases();
  for (let bits = 0; bits <= 0xffff; bits += stride) for (const name of ["sqrt", "widen32", "widen64"]) cases.add(name, bits);
  for (const bits of narrowing32) cases.add("narrow32", bits);
  for (const bits of narrowing64) cases.add("narrow64", bits);
  for (const a of pairValues) for (const b of pairValues) for (const name of [...arithmeticNames, ...comparisonNames]) cases.add(name, a, b);
  for (const name of arithmeticNames) for (let count = 0; count < (quick ? 400 : 262144); count++) cases.add(name, ...randomHalves());
  for (const name of comparisonNames) for (let count = 0; count < (quick ? 200 : 65536); count++) cases.add(name, ...randomHalves());
  return cases;
}

// ---------------------------------------------------------------------------------------------------------------
// The harness: a wrapper that calls the operations over a buffer of (operation, a, b) triples and stores one 64-bit
// result for each, built into the IR of the compiler (the functions take raw bits: f16 is an i16 in the IR).
// ---------------------------------------------------------------------------------------------------------------
const definitions = operations.map((entry) => {
  const parameters = entry.arguments.map((type, index) => `value${index}`);
  return `def op_${entry.name} :: ${[...entry.arguments, entry.result].join(" -> ")}\nfn op_${entry.name} ${parameters.join(" ")} = ${entry.source}`;
}).join("\n");
const irType = { f16: "i16", f32: "float", f64: "double", bool: "i1" };
function wrapperIr() {
  const blocks = operations.map((entry) => {
    const lines = [];
    const arguments_ = entry.arguments.map((type, index) => {
      const raw = index === 0 ? "%a" : "%b", name = `%x${entry.id}_${index}`;
      if (type === "f16") lines.push(`${name} = trunc i64 ${raw} to i16`);
      else if (type === "f32") lines.push(`${name}_bits = trunc i64 ${raw} to i32`, `${name} = bitcast i32 ${name}_bits to float`);
      else lines.push(`${name} = bitcast i64 ${raw} to double`);
      return `${irType[type]} ${name}`;
    });
    lines.push(`%r${entry.id} = call ${irType[entry.result]} @tz.fn.Main.op_${entry.name}(${arguments_.join(", ")})`);
    if (entry.result === "f16") lines.push(`%w${entry.id} = zext i16 %r${entry.id} to i64`);
    else if (entry.result === "bool") lines.push(`%w${entry.id} = zext i1 %r${entry.id} to i64`);
    else if (entry.result === "f32") lines.push(`%b${entry.id} = bitcast float %r${entry.id} to i32`, `%w${entry.id} = zext i32 %b${entry.id} to i64`);
    else lines.push(`%w${entry.id} = bitcast double %r${entry.id} to i64`);
    return `case${entry.id}:\n${lines.map((line) => `  ${line}`).join("\n")}\n  br label %store`;
  });
  return `
define void @f16_run(ptr %input, ptr %output, i64 %count) {
entry:
  br label %head
head:
  %i = phi i64 [ 0, %entry ], [ %next, %store ]
  %done = icmp eq i64 %i, %count
  br i1 %done, label %exit, label %load
load:
  %base = mul i64 %i, 3
  %slot = getelementptr i64, ptr %input, i64 %base
  %id64 = load i64, ptr %slot
  %second = getelementptr i64, ptr %slot, i64 1
  %a = load i64, ptr %second
  %third = getelementptr i64, ptr %slot, i64 2
  %b = load i64, ptr %third
  %id = trunc i64 %id64 to i32
  switch i32 %id, label %bad [${operations.map((entry) => ` i32 ${entry.id}, label %case${entry.id}`).join("")} ]
${blocks.join("\n")}
store:
  %result = phi i64 ${operations.map((entry) => `[ %w${entry.id}, %case${entry.id} ]`).join(", ")}
  %target = getelementptr i64, ptr %output, i64 %i
  store i64 %result, ptr %target
  %next = add i64 %i, 1
  br label %head
bad:
  call void @llvm.trap()
  unreachable
exit:
  ret void
}
`;
}
const host = `#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
void *f16_test_malloc(uint64_t size) { (void)size; abort(); }
extern void f16_run(const uint64_t *input, uint64_t *output, uint64_t count);
int main(int argc, char **argv) {
  if (argc != 3) return 2;
  FILE *in = fopen(argv[1], "rb");
  if (!in || fseek(in, 0, SEEK_END)) return 3;
  long size = ftell(in);
  rewind(in);
  uint64_t count = (uint64_t)size / 24;
  uint64_t *input = malloc((size_t)size + 8), *output = malloc(count * 8 + 8);
  if (!input || !output || fread(input, 1, (size_t)size, in) != (size_t)size) return 4;
  fclose(in);
  f16_run(input, output, count);
  FILE *out = fopen(argv[2], "wb");
  if (!out || fwrite(output, 8, count, out) != count) return 5;
  return fclose(out) ? 6 : 0;
}
`;

const cases = buildCases();
const samples = new Map(operations.map((entry) => [entry.name, 0]));
for (let index = 0; index < cases.length; index++) samples.set(operations[Number(cases.input[index * 3])].name, samples.get(operations[Number(cases.input[index * 3])].name) + 1);

function check(actual, label) {
  assert.equal(actual.length, cases.length, `${label}: ${actual.length} results for ${cases.length} cases`);
  for (let index = 0; index < cases.length; index++) {
    if (actual[index] === cases.expected[index]) continue;
    const entry = operations[Number(cases.input[index * 3])];
    assert.fail(`${label}: ${entry.name}(${cases.input[index * 3 + 1].toString(16)}, ${cases.input[index * 3 + 2].toString(16)}) = ${actual[index].toString(16)}, expected ${cases.expected[index].toString(16)}`);
  }
}
const runNative = (executable, label) => {
  const input = join(temporary, "cases.bin"), output = join(temporary, "results.bin");
  writeFileSync(input, new Uint8Array(cases.input.buffer, 0, cases.length * 24));
  execute(executable, [input, output]);
  const bytes = readFileSync(output);
  check(new BigUint64Array(bytes.buffer, bytes.byteOffset, bytes.byteLength / 8), label);
};

const report = [];
try {
  const source = join(temporary, "Main.tz"), directory = temporary;
  writeFileSync(source, definitions);
  writeFileSync(join(temporary, "host.c"), host);

  // ----- native IR of `--emit llvm` (`--cpu generic`) and WASM IR ------------------------------------------------
  const irPath = join(temporary, "f16.ll"), again = join(temporary, "again.ll"), wasmPath = join(temporary, "f16-wasm.ll");
  cli(["build", directory, "--emit", "llvm", "-o", irPath]);
  cli(["build", directory, "--emit", "llvm", "-o", again]);
  const ir = readFileSync(irPath, "utf8");
  assert.equal(ir, readFileSync(again, "utf8"), "the IR is deterministic");
  cli(["build", directory, "--target", "wasm32", "--emit", "llvm", "-o", wasmPath]);
  const wasmIr = readFileSync(wasmPath, "utf8");
  assert.ok(!/\bhalf\b/.test(wasmIr), "WASM IR has no half");
  assert.ok(!/__extendhfsf2|__truncsfhf2|__truncdfhf2/.test(wasmIr));
  const declarations = ir.match(/^declare .*$/gm) ?? [];
  assert.equal(new Set(declarations).size, declarations.length, "intrinsics are declared once");
  assert.ok(!/\b(fast|reassoc|contract|afn|nnan|ninf|nsz|arcp)\b|llvm\.fmuladd|llvm\.fma\.f16/.test(ir), "no fast-math flags and no FMA");
  const irHasHalf = /\bhalf\b/.test(ir);
  if (irHasHalf) {
    for (const match of ir.matchAll(/^define [^\n]*@tz\.(?:fn\.Main\.op_|builtin\.Math\.sqrt\.f16)[^\n]*\{([\s\S]*?)^\}/gm)) {
      assert.ok(!/@(?:malloc|calloc|realloc|tz\.alloc)\(/.test(match[1]), "operation bodies do not allocate");
      assert.ok(!/tz_soft_/.test(match[1]), "a hardware operation does not call the runtime");
    }
    for (const needle of ["fadd half", "fsub half", "fmul half", "fdiv half", "@llvm.sqrt.f16", "fpext half", "fptrunc double"]) assert.ok(ir.includes(needle), needle);
  }
  if (process.platform === "darwin" && process.arch === "arm64") assert.ok(irHasHalf, "Apple Silicon has FEAT_FP16: the IR of --cpu generic uses half");
  const generatedPath = irHasHalf ? "hardware" : "soft";
  report.push(`IR of --emit llvm: ${generatedPath}, WASM IR: soft (no half, no import)`);

  const wrapper = wrapperIr();
  const irWithProbe = ir.replaceAll("@malloc", "@f16_test_malloc") + wrapper;
  writeFileSync(irPath, irWithProbe);
  writeFileSync(wasmPath, wasmIr.replaceAll("@malloc", "@f16_test_malloc") + wrapper);

  // ----- native: the IR of the compiler, built by Clang at -O0 and -O3, with the CPU lowerings this host can run ----
  const variants = [{ name: "default", flags: [] }];
  const hostHas = (pattern) => { try { return pattern.test(readFileSync("/proc/cpuinfo", "utf8")); } catch { return false; } };
  if (process.arch === "arm64") {
    if (process.platform === "darwin") variants.push({ name: "no-fullfp16", flags: ["-mcpu=generic"] });
    else if (process.platform === "linux" && hostHas(/\b(fphp|asimdhp)\b/)) variants.push({ name: "fullfp16", flags: ["-march=armv8.2-a+fp16"] });
  }
  if (irHasHalf && process.arch === "arm64" && process.platform === "darwin") {
    execute(clang, ["-O3", "-mcpu=generic", "-S", "-Wno-override-module", irPath, "-o", join(temporary, "promoted.s")]);
    const promoted = readFileSync(join(temporary, "promoted.s"), "utf8");
    assert.ok(/^\s+fcvt\s+s\d+, h\d+/m.test(promoted) && !/^\s+fadd\s+h\d+/m.test(promoted), "-mcpu=generic lowers half to binary32 operations between conversions");
    assert.ok(!/__extendhfsf2|__truncsfhf2|__truncdfhf2/.test(promoted), "no libcall");
  }
  const executed = [];
  for (const optimization of [0, 3]) {
    for (const variant of variants) {
      const executable = join(temporary, `native-${optimization}-${variant.name}${windows ? ".exe" : ""}`);
      execute(clang, [`-O${optimization}`, "-ffp-contract=off", "-Wno-override-module", ...variant.flags, irPath, join(temporary, "host.c"), ...(windows ? [] : ["-lm"]), "-o", executable]);
      runNative(executable, `native ${generatedPath} IR, ${variant.name}, O${optimization}`);
      if (optimization === 3) executed.push(variant.name);
    }
    if (irHasHalf) {
      const objectCode = join(temporary, `native-${optimization}.s`);
      execute(clang, [`-O${optimization}`, "-S", "-Wno-override-module", irPath, "-o", objectCode]);
      assert.ok(!/__extendhfsf2|__truncsfhf2|__truncdfhf2/.test(readFileSync(objectCode, "utf8")), "no half libcall in the native code");
    }
  }
  report.push(`${generatedPath} IR run at O0/O3 with ${executed.join(" and ")}`);

  // ----- WASM: the software path, run by V8 ----------------------------------------------------------------------
  for (const optimization of [0, 3]) {
    const object = join(temporary, `wasm-${optimization}.o`), runtime = join(temporary, `runtime-${optimization}.o`), module = join(temporary, `f16-${optimization}.wasm`);
    execute(clang, ["--target=wasm32", `-O${optimization}`, "-ffp-contract=off", "-Wno-override-module", "-c", wasmPath, "-o", object]);
    execute(clang, ["--target=wasm32", `-O${optimization}`, "-Wno-override-module", "-c", resolve("src/runtime/wasm.ll"), "-o", runtime]);
    execute(wasmLinker, [object, runtime, "--no-entry", "--export=f16_run", "--export-memory", "-z", "stack-size=1048576", "--max-memory=268435456", "-o", module]);
    const compiled = new WebAssembly.Module(readFileSync(module));
    assert.deepEqual(WebAssembly.Module.imports(compiled), [], "the WASM module has no imports");
    const exports = new WebAssembly.Instance(compiled).exports;
    const chunk = 65536, actual = new BigUint64Array(cases.length);
    // One region past the module's own memory holds a chunk of the cases and its results.
    const base = exports.memory.buffer.byteLength;
    exports.memory.grow(Math.ceil(chunk * 32 / 65536));
    for (let start = 0; start < cases.length; start += chunk) {
      const count = Math.min(chunk, cases.length - start);
      new BigUint64Array(exports.memory.buffer, base, count * 3).set(cases.input.subarray(start * 3, (start + count) * 3));
      exports.f16_run(base, base + count * 24, BigInt(count));
      actual.set(new BigUint64Array(exports.memory.buffer, base + count * 24, count), start);
    }
    check(actual, `WASM soft, O${optimization}`);
  }
  report.push("WASM soft run at O0/O3");

  // ----- the driver's own builds: the code of `--cpu generic` and of `--cpu native` ------------------------------
  const probes = {
    arithmetic: "export def probe_arithmetic :: i64 -> bool\nfn probe_arithmetic n =\n    let xs = [1.5f16, 2.5f16, 0.1f16]\n    Math.sqrt (xs[n] + xs[0]) < xs[1] * xs[n] - xs[0] / xs[1]\n",
    widen: "export def probe_widen :: i64 -> f64\nfn probe_widen n =\n    let xs = [1.5f16, 2.5f16]\n    ((xs[n] as f32) as f64) + (xs[n] as f64)\n",
    narrow32: "export def probe_narrow32 :: f32 -> f32\nfn probe_narrow32 x = (x as f16) as f32\n",
    narrow64: "export def probe_narrow64 :: f64 -> f32\nfn probe_narrow64 x = (x as f16) as f32\n",
  };
  const exported = `
export def x_add :: f32 -> f32 -> f32
fn x_add a b = ((a as f16) + (b as f16)) as f32
export def x_sub :: f32 -> f32 -> f32
fn x_sub a b = ((a as f16) - (b as f16)) as f32
export def x_mul :: f32 -> f32 -> f32
fn x_mul a b = ((a as f16) * (b as f16)) as f32
export def x_div :: f32 -> f32 -> f32
fn x_div a b = ((a as f16) / (b as f16)) as f32
${comparisonNames.map((name) => `export def x_${name} :: f32 -> f32 -> bool\nfn x_${name} a b = (a as f16) ${symbols[name]} (b as f16)`).join("\n")}
export def x_sqrt :: f32 -> f32
fn x_sqrt a = Math.sqrt (a as f16) as f32
export def x_round32 :: f32 -> f32
fn x_round32 a = (a as f16) as f32
export def x_round64 :: f32 -> f64
fn x_round64 a = (a as f16) as f64
export def x_narrow64 :: f64 -> f32
fn x_narrow64 a = (a as f16) as f32
`;
  // The exported functions take f32 and f64: a binary16 value enters as a widened f32, or as any f32 or f64.
  const exportedNames = [...arithmeticNames.map((name) => `x_${name}`), ...comparisonNames.map((name) => `x_${name}`), "x_sqrt", "x_round32", "x_round64", "x_narrow64"];
  const exportedCases = [];
  const add = (name, a, b = 0n) => exportedCases.push([BigInt(exportedNames.indexOf(name)), BigInt(a), BigInt(b)]);
  const pairs = pairValues.flatMap((a) => pairValues.map((b) => [a, b]));
  for (let count = 0; count < (quick ? 200 : 20000); count++) pairs.push(randomHalves());
  for (const [a, b] of pairs) for (const name of [...arithmeticNames, ...comparisonNames]) add(`x_${name}`, widen32(a), widen32(b));
  for (let count = 0; count < (quick ? 200 : 20000); count++) for (const name of arithmeticNames) add(`x_${name}`, random(), random());
  for (let bits = 0; bits <= 0xffff; bits += stride) add("x_sqrt", widen32(bits));
  for (const bits of narrowing32) { add("x_round32", bits); add("x_round64", bits); }
  for (const bits of narrowing64) add("x_narrow64", bits);
  const exportedReference = ([id, a, b]) => {
    const name = exportedNames[Number(id)].slice(2);
    const first = narrow32(Number(a)), second = narrow32(Number(b));
    if (arithmeticNames.includes(name)) return BigInt(widen32(arithmetic(name, first, second)));
    if (comparisonNames.includes(name)) return BigInt(comparison(name, first, second));
    if (name === "sqrt") return BigInt(widen32(squareRoot(first)));
    if (name === "round32") return BigInt(widen32(first));
    if (name === "round64") return widen64(first);
    return BigInt(widen32(narrow64(a)));
  };
  const exportedInput = new BigUint64Array(exportedCases.length * 3), exportedExpected = new BigUint64Array(exportedCases.length);
  exportedCases.forEach((entry, index) => { exportedInput.set(entry, index * 3); exportedExpected[index] = exportedReference(entry); });
  const exportedHost = `#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
${arithmeticNames.map((name) => `float tz_x_${name}(float, float);`).join("\n")}
${comparisonNames.map((name) => `int32_t tz_x_${name}(float, float);`).join("\n")}
float tz_x_sqrt(float);
float tz_x_round32(float);
double tz_x_round64(float);
float tz_x_narrow64(double);
static float as_float(uint64_t bits) { uint32_t narrow = (uint32_t)bits; float value; memcpy(&value, &narrow, 4); return value; }
static double as_double(uint64_t bits) { double value; memcpy(&value, &bits, 8); return value; }
static uint64_t of_float(float value) { uint32_t bits; memcpy(&bits, &value, 4); return bits; }
static uint64_t of_double(double value) { uint64_t bits; memcpy(&bits, &value, 8); return bits; }
int main(int argc, char **argv) {
  if (argc != 3) return 2;
  FILE *in = fopen(argv[1], "rb");
  if (!in || fseek(in, 0, SEEK_END)) return 3;
  long size = ftell(in);
  rewind(in);
  uint64_t count = (uint64_t)size / 24;
  uint64_t *input = malloc((size_t)size + 8), *output = malloc(count * 8 + 8);
  if (!input || !output || fread(input, 1, (size_t)size, in) != (size_t)size) return 4;
  fclose(in);
  for (uint64_t index = 0; index < count; index++) {
    uint64_t id = input[index * 3], a = input[index * 3 + 1], b = input[index * 3 + 2], result = 0;
    switch (id) {
${exportedNames.map((name, id) => {
    const symbol = `tz_${name}`;
    const kind = name.slice(2);
    if (arithmeticNames.includes(kind)) return `      case ${id}: result = of_float(${symbol}(as_float(a), as_float(b))); break;`;
    if (comparisonNames.includes(kind)) return `      case ${id}: result = (uint32_t)${symbol}(as_float(a), as_float(b)); break;`;
    if (kind === "round64") return `      case ${id}: result = of_double(${symbol}(as_float(a))); break;`;
    if (kind === "narrow64") return `      case ${id}: result = of_float(${symbol}(as_double(a))); break;`;
    return `      case ${id}: result = of_float(${symbol}(as_float(a))); break;`;
  }).join("\n")}
      default: return 7;
    }
    output[index] = result;
  }
  FILE *out = fopen(argv[2], "wb");
  if (!out || fwrite(output, 8, count, out) != count) return 5;
  return fclose(out) ? 6 : 0;
}
`;
  const classified = {};
  for (const build of ["generic", "native"]) {
    if (build === "native" && !["x64", "arm64"].includes(process.arch)) { report.push(`--cpu native: skipped on ${process.arch}`); continue; }
    const flags = build === "native" ? ["--cpu", "native"] : [];
    const soft = {};
    for (const [name, text] of Object.entries(probes)) {
      const probe = join(temporary, `probe-${name}`);
      mkdirSync(probe, { recursive: true });
      writeFileSync(join(probe, "Main.tz"), text);
      const object = join(temporary, `probe-${name}-${build}.o`);
      cli(["build", probe, "--emit", "object", "-O3", ...flags, "--no-cache", "-o", object]);
      soft[name] = readFileSync(object).includes("tz_soft_");
    }
    const arithmeticHardware = !soft.arithmetic, widenHardware = !soft.widen && !soft.narrow32, narrow64Hardware = !soft.narrow64;
    let path = "partial";
    if (arithmeticHardware && widenHardware && narrow64Hardware) path = "hardware";
    else if (!arithmeticHardware && widenHardware && !narrow64Hardware) path = "conversions";
    else if (!arithmeticHardware && !widenHardware && !narrow64Hardware) path = "soft";
    classified[build] = path;
    const detail = `arithmetic ${arithmeticHardware ? "hardware" : "soft"}, f16 to f32/f64 ${!soft.widen ? "hardware" : "soft"}, f32 to f16 ${!soft.narrow32 ? "hardware" : "soft"}, f64 to f16 ${narrow64Hardware ? "hardware" : "soft"}`;
    report.push(`--cpu ${build} build: ${path} (${detail})`);
    if (expectation[build]) assert.equal(path, expectation[build], `--cpu ${build} took the ${path} path (${detail}); TSUZURI_F16_EXPECT wants ${expectation[build]}`);

    const exportDirectory = join(temporary, `exported-${build}`);
    mkdirSync(exportDirectory, { recursive: true });
    writeFileSync(join(exportDirectory, "Main.tz"), exported);
    writeFileSync(join(temporary, `exported-${build}.c`), exportedHost);
    writeFileSync(join(temporary, `exported-${build}.bin`), new Uint8Array(exportedInput.buffer));
    for (const optimization of [0, 3]) {
      const object = join(temporary, `exported-${build}-${optimization}.o`), executable = join(temporary, `exported-${build}-${optimization}${windows ? ".exe" : ""}`);
      cli(["build", exportDirectory, "--emit", "object", `-O${optimization}`, ...flags, "--no-cache", "-o", object]);
      execute(clang, [`-O${optimization}`, join(temporary, `exported-${build}.c`), object, ...(windows ? [] : ["-lm"]), "-o", executable]);
      const results = join(temporary, `exported-${build}-results.bin`);
      execute(executable, [join(temporary, `exported-${build}.bin`), results]);
      const bytes = readFileSync(results), actual = new BigUint64Array(bytes.buffer, bytes.byteOffset, bytes.byteLength / 8);
      assert.equal(actual.length, exportedCases.length);
      for (let index = 0; index < actual.length; index++) {
        if (actual[index] !== exportedExpected[index]) assert.fail(`--cpu ${build} O${optimization}: ${exportedNames[Number(exportedInput[index * 3])]}(${exportedInput[index * 3 + 1].toString(16)}, ${exportedInput[index * 3 + 2].toString(16)}) = ${actual[index].toString(16)}, expected ${exportedExpected[index].toString(16)}`);
      }
    }
  }
  report.push(`exported f32/f64 functions of ${exportedCases.length} cases run at O0/O3 for each build`);

  // ----- the language level: `run` at -O0 and -O3 -----------------------------------------------------------------
  const runDirectory = join(temporary, "run");
  mkdirSync(runDirectory, { recursive: true });
  writeFileSync(join(runDirectory, "Main.tz"), `let x = 1.5f16
let y = 2.25f16
assert (x + y == 3.75f16 && y - x == 0.75f16 && x * y == 3.375f16 && y / x == 1.5f16)
assert (Math.sqrt 2.25f16 == 1.5f16 && x < y && !(y < x) && x != y && 65519.0f32 as f16 as f32 == 65504.0f32)
let nan = 0.0f16 / 0.0f16
assert (nan != nan && !(nan < nan) && !(nan == nan))
assert (((1.0f32 + 0.000488f32) as f16) as f32 == 1.0f32 && 0.1f64 as f16 as f64 == 0.0999755859375)
42
`);
  for (const optimization of [0, 3]) assert.equal(cli(["run", runDirectory, `-O${optimization}`]).stdout, "42\n");

  const total = cases.length;
  console.log(`f16_hardware: host ${process.platform}/${process.arch}; ${report.join("; ")}`);
  console.log(`f16_hardware: ${total} cases${quick ? " (quick)" : ""}, native ${generatedPath}${classified.native ? ` (--cpu native: ${classified.native})` : ""} and WASM soft, O0/O3, bit identity including NaN`);
} finally { rmSync(temporary, { recursive: true, force: true }); }
