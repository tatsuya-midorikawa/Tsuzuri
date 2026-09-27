import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, writeFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

const compiler = resolve(process.argv[2] ?? "target/release/tsuzuri");
const clang = process.env.TSUZURI_CLANG ?? "clang";
const temporary = mkdtempSync(join(tmpdir(), "tsuzuri-math-"));
const quick = process.argv.includes("--quick");
const elementaryOnly = process.argv.includes("--elementary-only");
const basicOnly = process.argv.includes("--basic-only");
const execute = (program, args, options = {}) => {
  const result = spawnSync(program, args, { encoding: "utf8", timeout: 240_000, maxBuffer: 256 * 1024 * 1024, ...options });
  if (result.error) throw result.error;
  assert.equal(result.status, 0, `${program} ${args.join(" ")}\n${result.stderr}`);
  return result;
};
const cli = (args) => execute(compiler, args);
const formats = [
  { name: "f16", bits: 16, precision: 11, minimum: -24, maximum: 5, fraction: 10, bias: 15 },
  { name: "f32", bits: 32, precision: 24, minimum: -149, maximum: 104, fraction: 23, bias: 127 },
  { name: "f64", bits: 64, precision: 53, minimum: -1074, maximum: 971, fraction: 52, bias: 1023 },
  { name: "f128", bits: 128, precision: 113, minimum: -16494, maximum: 16271, fraction: 112, bias: 16383 },
  { name: "d32", bits: 32, precision: 7, minimum: -101, maximum: 90, fraction: 23, bias: 101, decimal: true },
  { name: "d64", bits: 64, precision: 16, minimum: -398, maximum: 369, fraction: 53, bias: 398, decimal: true },
  { name: "d128", bits: 128, precision: 34, minimum: -6176, maximum: 6111, fraction: 113, bias: 6176, decimal: true },
];
const operations = ["sqrt", "floor", "ceil", "trunc", "round", "round_even", "abs", "min", "max", "clamp", "fma", "copysign", "is_nan", "is_infinite", "is_finite", "pi", "e"];
const elementary = ["sin", "cos", "tan", "asin", "acos", "atan", "atan2", "exp", "exp2", "log", "log2", "log10", "pow", "cbrt", "hypot"];
const arity = (name) => name === "pi" || name === "e" ? 0 : ["clamp", "fma"].includes(name) ? 3 : ["min", "max", "copysign", "atan2", "pow", "hypot"].includes(name) ? 2 : 1;
const numberBits = (value, bits) => {
  const buffer = new ArrayBuffer(8), view = new DataView(buffer);
  if (bits === 32) { view.setFloat32(0, value, true); return BigInt(view.getUint32(0, true)); }
  view.setFloat64(0, value, true); return view.getBigUint64(0, true);
};
const mask = (bits) => (1n << BigInt(bits)) - 1n;
const shift = (value, bits) => bits < 0 ? value >> BigInt(-bits) : value << BigInt(bits);
const power = (format, exponent) => (format.decimal ? 10n : 2n) ** BigInt(exponent);
const special = (format, kind, negative = false) => (negative ? 1n << BigInt(format.bits - 1) : 0n) | (format.decimal
  ? BigInt(kind === "nan" ? 31 : 30) << BigInt(format.bits - 6)
  : BigInt(2 * format.bias + 1) << BigInt(format.fraction) | (kind === "nan" ? 1n << BigInt(format.fraction - 1) : 0n));

function unpack(raw, format) {
  const negative = Boolean(raw >> BigInt(format.bits - 1));
  const magnitude = raw & mask(format.bits - 1);
  let coefficient, exponent, kind;
  if (format.decimal) {
    const combination = Number(magnitude >> BigInt(format.bits - 6));
    if (combination >= 30) kind = combination === 30 ? "inf" : "nan";
    if (format.bits !== 128 && magnitude >> BigInt(format.bits - 3) === 3n) {
      exponent = Number(magnitude >> BigInt(format.fraction - 2) & mask(format.bits - format.fraction - 1)) - format.bias;
      coefficient = magnitude & mask(format.fraction - 2) | 1n << BigInt(format.fraction);
    } else {
      exponent = Number(magnitude >> BigInt(format.fraction)) - format.bias;
      coefficient = magnitude & mask(format.fraction);
    }
    if (coefficient >= power(format, format.precision) || (format.bits === 128 && magnitude >> 125n === 3n)) coefficient = 0n;
  } else {
    const encoded = Number(magnitude >> BigInt(format.fraction));
    coefficient = magnitude & mask(format.fraction);
    if (encoded === 2 * format.bias + 1) kind = coefficient ? "nan" : "inf";
    if (encoded) coefficient |= 1n << BigInt(format.fraction);
    exponent = encoded ? encoded - format.bias - format.fraction : format.minimum;
  }
  return { raw, negative, coefficient, exponent, kind };
}

function encode(coefficient, exponent, negative, format) {
  let raw = negative ? 1n << BigInt(format.bits - 1) : 0n;
  if (!coefficient) return raw | (format.decimal ? BigInt(format.bias) << BigInt(format.fraction) : 0n);
  const base = format.decimal ? 10n : 2n;
  while (coefficient >= power(format, format.precision)) { coefficient /= base; exponent++; }
  if (format.decimal) {
    while (exponent < format.maximum && coefficient % 10n === 0n) { coefficient /= 10n; exponent++; }
    if (exponent > format.maximum) return special(format, "inf", negative);
    if (format.bits !== 128 && coefficient >> BigInt(format.fraction)) {
      raw |= 3n << BigInt(format.bits - 3) | BigInt(exponent + format.bias) << BigInt(format.fraction - 2) | coefficient & mask(format.fraction - 2);
    } else raw |= BigInt(exponent + format.bias) << BigInt(format.fraction) | coefficient;
  } else {
    while (coefficient < 1n << BigInt(format.fraction) && exponent > format.minimum) { coefficient <<= 1n; exponent--; }
    if (exponent > format.maximum) return special(format, "inf", negative);
    const encoded = coefficient >> BigInt(format.fraction) ? exponent + format.fraction + format.bias : 0;
    raw |= BigInt(encoded) << BigInt(format.fraction) | coefficient & mask(format.fraction);
  }
  return raw;
}

function normalized(raw, format) {
  const value = unpack(raw, format);
  if (value.kind) return special(format, value.kind, value.kind === "inf" && value.negative);
  return format.decimal ? encode(value.coefficient, value.exponent, value.negative, format) : raw;
}

function compare(left, right, format) {
  if (left.kind === "nan" || right.kind === "nan") return null;
  if (!left.kind && !right.kind && !left.coefficient && !right.coefficient) return 0;
  if (left.negative !== right.negative) return left.negative ? -1 : 1;
  let order;
  if (left.kind || right.kind) order = Number(left.kind === "inf") - Number(right.kind === "inf");
  else {
    const exponent = Math.min(left.exponent, right.exponent);
    const first = left.coefficient * power(format, left.exponent - exponent);
    const second = right.coefficient * power(format, right.exponent - exponent);
    order = first < second ? -1 : first > second ? 1 : 0;
  }
  return left.negative ? -order : order;
}

function roundedRational(numerator, denominator, exponent, negative, format) {
  let magnitude = numerator.toString(format.decimal ? 10 : 2).length - denominator.toString(format.decimal ? 10 : 2).length;
  const lessPower = (amount) => amount >= 0 ? numerator < denominator * power(format, amount) : numerator * power(format, -amount) < denominator;
  while (lessPower(magnitude)) magnitude--;
  while (!lessPower(magnitude + 1)) magnitude++;
  let quantum = Math.max(format.minimum, magnitude + exponent - format.precision + 1);
  const scale = exponent - quantum;
  if (scale >= 0) numerator *= power(format, scale); else denominator *= power(format, -scale);
  let coefficient = numerator / denominator;
  const remainder = numerator % denominator;
  if (remainder * 2n > denominator || (remainder * 2n === denominator && coefficient % 2n)) coefficient++;
  return encode(coefficient, quantum, negative, format);
}

function rootInteger(value) {
  if (!value) return 0n;
  let guess = 1n << BigInt(Math.ceil(value.toString(2).length / 2));
  for (;;) { const next = (guess + value / guess) / 2n; if (next >= guess) return guess; guess = next; }
}

function reference(name, raw, otherRaw, boundRaw, format) {
  const value = unpack(raw, format), other = unpack(otherRaw, format);
  if (name === "fma") {
    const addend = unpack(boundRaw, format);
    const negative = value.negative !== other.negative;
    if ([value, other, addend].some((part) => part.kind === "nan")) return special(format, "nan");
    if (value.kind || other.kind) {
      if ((!value.kind && !value.coefficient) || (!other.kind && !other.coefficient) || (addend.kind && addend.negative !== negative)) return special(format, "nan");
      return special(format, "inf", negative);
    }
    if (addend.kind) return special(format, "inf", addend.negative);
    const exponent = Math.min(value.exponent + other.exponent, addend.exponent);
    const product = value.coefficient * other.coefficient * power(format, value.exponent + other.exponent - exponent);
    const last = addend.coefficient * power(format, addend.exponent - exponent);
    const total = (negative ? -product : product) + (addend.negative ? -last : last);
    if (!total) return encode(0n, 0, !product && !last && negative && addend.negative, format);
    return roundedRational(total < 0n ? -total : total, 1n, exponent, total < 0n, format);
  }
  if (name === "pi" || name === "e") {
    const digits = name === "pi" ? "3141592653589793238462643383279502884197169399375105820974944592307816406286208998628034825342117067982148086513282306647" : "2718281828459045235360287471352662497757247093699959574966967627724076630353547594571382178525166427427466391932003059922";
    return roundedRational(BigInt(digits), 10n ** BigInt(digits.length - 1), 0, false, format);
  }
  if (name === "is_nan") return BigInt(value.kind === "nan");
  if (name === "is_infinite") return BigInt(value.kind === "inf");
  if (name === "is_finite") return BigInt(!value.kind);
  if (name === "abs") return raw & mask(format.bits - 1);
  if (name === "copysign") return raw & mask(format.bits - 1) | otherRaw & (1n << BigInt(format.bits - 1));
  if (name === "clamp") {
    const order = compare(other, unpack(boundRaw, format), format);
    return order === null || order > 0 ? special(format, "nan") : reference("min", reference("max", raw, otherRaw, 0n, format), boundRaw, 0n, format);
  }
  if (name === "min" || name === "max") {
    const order = compare(value, other, format);
    if (order === null) return special(format, "nan");
    if (!value.kind && !other.kind && !value.coefficient && !other.coefficient) return encode(0n, 0, name === "min" ? value.negative || other.negative : value.negative && other.negative, format);
    return (name === "max" ? order >= 0 : order <= 0) ? raw : otherRaw;
  }
  if (value.kind === "nan") return special(format, "nan");
  if (name === "sqrt") {
    if (!value.kind && !value.coefficient) return raw;
    if (value.negative) return special(format, "nan");
    if (value.kind) return raw;
    const magnitude = value.coefficient.toString(format.decimal ? 10 : 2).length - 1 + value.exponent;
    const quantum = Math.max(format.minimum, Math.floor(magnitude / 2) - format.precision + 1);
    const scale = value.exponent - quantum * 2;
    const numerator = value.coefficient * (scale >= 0 ? power(format, scale) : 1n);
    const denominator = scale < 0 ? power(format, -scale) : 1n;
    let coefficient = rootInteger(numerator / denominator);
    const midpoint = (2n * coefficient + 1n) ** 2n * denominator;
    if (4n * numerator > midpoint || (4n * numerator === midpoint && coefficient % 2n)) coefficient++;
    return encode(coefficient, quantum, false, format);
  }
  if (value.kind || !value.coefficient || value.exponent >= 0) return raw;
  const denominator = power(format, -value.exponent);
  let integral = value.coefficient / denominator;
  const remainder = value.coefficient % denominator;
  if (!remainder) return raw;
  if ((name === "floor" && value.negative) || (name === "ceil" && !value.negative)) integral++;
  if (["round", "round_even"].includes(name) && (2n * remainder > denominator || (2n * remainder === denominator && (name === "round" || integral % 2n)))) integral++;
  return encode(integral, 0, value.negative, format);
}

const definitions = [], wrappers = [], cases = [];
let seed = 42n;
for (const [formatIndex, format] of formats.entries()) {
  const type = format.name, bits = format.bits;
  const values = [0n, 1n, mask(bits), 1n << BigInt(bits - 1), mask(bits - 1), special(format, "inf"), special(format, "inf", true), special(format, "nan")];
  for (const numerator of [0n, 1n, 2n, 3n, 5n, 7n, 15n, 31n]) for (const negative of [false, true]) {
    values.push(numerator ? roundedRational(numerator, 2n, 0, negative, format) : encode(0n, 0, negative, format));
  }
  for (let index = 0; index < (quick ? 16 : 1000); index++) {
    seed = BigInt.asUintN(128, seed * 6364136223846793005n + 1442695040888963407n);
    values.push(seed & mask(bits));
  }
  const selected = [...elementaryOnly ? [] : operations, ...!basicOnly && ["f32", "f64"].includes(type) ? elementary : []];
  for (const name of selected) {
    const id = definitions.length, count = arity(name), predicate = name.startsWith("is_");
    const inputs = Array.from({ length: count }, (_, index) => `value${index}`);
    definitions.push(`def operation_${id} :: ${[...Array(count).fill(type), predicate ? "bool" : type].join(" -> ")}\nfn operation_${id} ${inputs.join(" ")} = Math.${name}${count ? " " + inputs.join(" ") : "()"}`);
    const lowered = type === "f32" ? "float" : type === "f64" ? "double" : `i${bits}`;
    const argumentLines = inputs.flatMap((_, index) => [
      `%wide_low${index} = zext i64 %low${index} to i128`, `%wide_high${index} = zext i64 %high${index} to i128`,
      `%upper${index} = shl i128 %wide_high${index}, 64`, `%raw${index} = or i128 %wide_low${index}, %upper${index}`,
      ...(bits < 128 ? [`%bits${index} = trunc i128 %raw${index} to i${bits}`] : []),
      ...(["f32", "f64"].includes(type) ? [`%value${index} = bitcast i${bits} %bits${index} to ${lowered}`] : []),
    ]);
    const args = inputs.map((_, index) => `${lowered} ${["f32", "f64"].includes(type) ? `%value${index}` : bits === 128 ? `%raw${index}` : `%bits${index}`}`).join(", ");
    const returnType = predicate ? "i1" : lowered;
    const normalizedType = predicate ? "i1" : `i${bits}`;
    const resultBits = ["f32", "f64"].includes(type) && !predicate ? "%result_bits" : "%result";
    wrappers.push(`case${id}:\n${argumentLines.map((line) => "  " + line).join("\n")}\n  %result = call ${returnType} @tz.fn.Main.operation_${id}(${args})\n${resultBits === "%result_bits" ? `  %result_bits = bitcast ${lowered} %result to i${bits}\n` : ""}  ${predicate || bits < 128 ? `%wide = zext ${normalizedType} ${resultBits} to i128` : `%wide = or i128 ${resultBits}, 0`}\n  store i128 %wide, ptr @math_result\n  ret void` .replaceAll(/%(?!low\d|high\d)([a-z_]+\d*)/g, `%$1_${id}`));
    if (elementary.includes(name)) {
      const specialValues = [0, -0, 1, -1, 2, -2, 0.5, -0.5, 3, -3, Infinity, -Infinity, NaN, Number.MIN_VALUE, Number.MAX_VALUE, 2 ** -149, 2 ** -126, 2 ** -1022, 2 ** 127, Math.PI / 6, Math.PI / 2, Math.PI, 709.782712893384, -745.1332191019411, 88.72283905206835, -103.972084045410, 1024, -1075];
      const pairs = specialValues.flatMap((left) => (count === 2 ? specialValues : [0]).map((right) => [numberBits(left, bits), numberBits(right, bits)]));
      for (let index = 0; index < (quick ? 16 : 700); index++) {
        seed = BigInt.asUintN(128, seed * 6364136223846793005n + 1442695040888963407n);
        const unit = Number(seed & mask(53)) / 2 ** 53;
        let left = seed & mask(bits), right = shift(seed, -64) & mask(bits);
        if (["asin", "acos"].includes(name)) left = numberBits(2 * unit - 1, bits);
        if (["exp", "exp2"].includes(name)) left = numberBits((unit - 0.5) * (bits === 32 ? 300 : 2200), bits);
        if (name === "pow" && index % 2) { left = numberBits(0.5 + unit, bits); right = numberBits((Number(seed >> 64n & 65535n) - 32768) / 32, bits); }
        pairs.push([left, right]);
      }
      for (const [left, right] of pairs) cases.push({ id, formatIndex, name, args: [left, right, 0n], elementary: true });
      continue;
    }
    const inputsToTest = count === 0 ? [0n] : (!quick && type === "f16" && count === 1 ? Array.from({ length: 65536 }, (_, index) => BigInt(index)) : values);
    if (name === "fma") {
      const specialValues = values.slice(0, 8).concat(encode(1n, 0, false, format), encode(1n, 0, true, format));
      for (const left of specialValues) for (const right of specialValues) for (const addend of specialValues) cases.push({ id, formatIndex, name, args: [left, right, addend], expected: reference(name, left, right, addend, format) });
      const unit = power(format, format.precision - 1);
      const triples = [
        [encode(unit + 1n, 1 - format.precision, false, format), encode(unit - 1n, 1 - format.precision, false, format), encode(1n, 0, true, format)],
        [encode(1n, format.minimum, false, format), encode(1n, format.minimum, false, format), encode(1n, format.maximum, true, format)],
        [encode(1n, format.maximum, false, format), encode(1n, format.maximum, false, format), encode(1n, format.minimum, true, format)],
      ];
      for (const args of triples) cases.push({ id, formatIndex, name, args, expected: reference(name, ...args, format) });
    }
    for (const [index, raw] of inputsToTest.entries()) {
      const other = values[(index * 13 + 5) % values.length], bound = values[(index * 7 + 3) % values.length];
      cases.push({ id, formatIndex, name, args: [raw, other, bound], expected: reference(name, raw, other, bound, format) });
    }
  }
}

try {
  const elementaryCases = cases.filter((test) => test.elementary);
  if (elementaryCases.length) {
    const input = elementaryCases.map((test) => JSON.stringify([test.name, formats[test.formatIndex].bits, test.args[0].toString(16), test.args[1].toString(16)])).join("\n") + "\n";
    const python = process.env.TSUZURI_MATH_PYTHON ?? resolve("target/math-reference-env/bin/python");
    const references = execute(python, [resolve("tests/math_reference.py")], { input }).stdout.trim().split("\n").map((line) => JSON.parse(line));
    assert.equal(references.length, elementaryCases.length);
    elementaryCases.forEach((test, index) => { test.expected = BigInt(references[index][0]); test.exact = references[index][1] === null ? null : BigInt(references[index][1]); test.exponent = references[index][2]; });
  }
  const source = join(temporary, "Main.tz"), irPath = join(temporary, "math.ll"), again = join(temporary, "again.ll");
  writeFileSync(source, definitions.join("\n"));
  cli(["build", source, "--emit", "llvm", "-o", irPath]);
  cli(["build", source, "--emit", "llvm", "-o", again]);
  const ir = readFileSync(irPath, "utf8");
  assert.equal(ir, readFileSync(again, "utf8"));
  const wasmIrPath = join(temporary, "math-wasm.ll");
  cli(["build", source, "--target", "wasm32", "--emit", "llvm", "-o", wasmIrPath]);
  const wasmIr = readFileSync(wasmIrPath, "utf8");
  const declarations = ir.match(/^declare .*$/gm) ?? [];
  assert.equal(new Set(declarations).size, declarations.length);
  assert.ok(!/\b(fast|reassoc|contract|afn|nnan|ninf)\b|llvm\.fmuladd/.test(ir));
  for (const match of ir.matchAll(/^define [^\n]*@tz\.builtin\.Math\.[^\n]*\{([\s\S]*?)^\}/gm)) {
    assert.ok(!/@(?:malloc|calloc|realloc|tz\.alloc)\(/.test(match[1]), "math bodies do not allocate heap storage");
  }
  const probe = `\n@math_result = internal global i128 0, align 16\ndefine i64 @math_low() { %value = load i128, ptr @math_result %low = trunc i128 %value to i64 ret i64 %low }\ndefine i64 @math_high() { %value = load i128, ptr @math_result %upper = lshr i128 %value, 64 %high = trunc i128 %upper to i64 ret i64 %high }\ndefine void @math_probe(i32 %id, i64 %low0, i64 %high0, i64 %low1, i64 %high1, i64 %low2, i64 %high2) {\nentry:\n switch i32 %id, label %bad [${wrappers.map((_, id) => `i32 ${id}, label %case${id}`).join(" ")}]\n${wrappers.join("\n")}\nbad: call void @llvm.trap() unreachable\n}\n`;
  writeFileSync(irPath, ir.replaceAll("@malloc", "@math_test_malloc") + probe);
  writeFileSync(wasmIrPath, wasmIr.replaceAll("@malloc", "@math_test_malloc") + probe);
  const host = join(temporary, "host.c");
  writeFileSync(host, `#include <stdint.h>\n#include <stdio.h>\n#include <stdlib.h>\n#include <inttypes.h>\nvoid *math_test_malloc(uint64_t size) { (void)size; abort(); }\nextern void math_probe(int, uint64_t, uint64_t, uint64_t, uint64_t, uint64_t, uint64_t);\nextern uint64_t math_low(void), math_high(void);\nint main(void) { int id; uint64_t values[6]; while(scanf("%d %" SCNx64 " %" SCNx64 " %" SCNx64 " %" SCNx64 " %" SCNx64 " %" SCNx64, &id, &values[0], &values[1], &values[2], &values[3], &values[4], &values[5]) == 7) { math_probe(id, values[0], values[1], values[2], values[3], values[4], values[5]); printf("%016" PRIx64 "%016" PRIx64 "\\n", math_high(), math_low()); } return 0; }\n`);
  const input = cases.map((test) => [test.id, ...test.args.flatMap((value) => [value & mask(64), value >> 64n]).map((value) => value.toString(16))].join(" ")).join("\n");
  let baseline;
  const validate = (actual, index, target) => {
    const test = cases[index], format = formats[test.formatIndex];
    const result = test.name.startsWith("is_") ? actual : normalized(actual, format);
    const expected = test.name.startsWith("is_") ? test.expected : normalized(test.expected, format);
    const label = `${target} ${format.name} ${test.name}(${test.args.map((value) => value.toString(16))}): raw ${actual.toString(16)} expected ${test.expected.toString(16)}`;
    if (test.elementary && test.exact !== null && test.exact !== 0n) {
      const received = unpack(actual, format), rounded = unpack(test.expected, format);
      assert.ok(!received.kind, label);
      const exponent = Math.min(test.exponent, received.exponent, rounded.exponent);
      const actualInteger = shift(received.negative ? -received.coefficient : received.coefficient, received.exponent - exponent);
      const exactInteger = shift(test.exact, test.exponent - exponent);
      const difference = actualInteger >= exactInteger ? actualInteger - exactInteger : exactInteger - actualInteger;
      const ulp = shift(1n, Math.max(received.exponent, rounded.exponent) - exponent);
      assert.ok(difference <= ulp, `${label}: error ${(Number(difference) / Number(ulp)).toPrecision(6)} ulp`);
    } else assert.equal(result, expected, label);
    if (baseline && !test.name.startsWith("is_") && unpack(actual, format).kind !== "nan") assert.equal(actual, baseline[index], "non-NaN results are bit-identical");
  };
  for (const optimization of [0, 3]) {
    const native = join(temporary, `native-${optimization}`);
    execute(clang, [`-O${optimization}`, "-ffp-contract=off", "-Wno-override-module", irPath, host, "-lm", "-o", native]);
    const output = execute(native, [], { input }).stdout.trim().split("\n").map((line) => BigInt(`0x${line}`));
    assert.equal(output.length, cases.length);
    output.forEach((value, index) => validate(value, index, `native O${optimization}`));
    baseline ??= output;
    const object = join(temporary, `math-${optimization}.o`), runtime = join(temporary, `runtime-${optimization}.o`), wasm = join(temporary, `math-${optimization}.wasm`);
    execute(clang, ["--target=wasm32", `-O${optimization}`, "-ffp-contract=off", "-Wno-override-module", "-c", wasmIrPath, "-o", object]);
    execute(clang, ["--target=wasm32", `-O${optimization}`, "-Wno-override-module", "-c", resolve("src/runtime/wasm.ll"), "-o", runtime]);
    execute(process.env.TSUZURI_WASM_LD ?? "wasm-ld", [object, runtime, "--no-entry", "--export=math_probe", "--export=math_low", "--export=math_high", "--export-memory", "-z", "stack-size=1048576", "--max-memory=16777216", "-o", wasm]);
    const module = new WebAssembly.Module(readFileSync(wasm));
    assert.deepEqual(WebAssembly.Module.imports(module), []);
    const api = new WebAssembly.Instance(module).exports;
    for (const [index, test] of cases.entries()) {
      api.math_probe(test.id, ...test.args.flatMap((value) => [BigInt.asIntN(64, value), BigInt.asIntN(64, value >> 64n)]));
      validate(BigInt.asUintN(64, api.math_low()) | BigInt.asUintN(64, api.math_high()) << 64n, index, `WASM O${optimization}`);
    }
  }
  writeFileSync(source, `export def api :: i64\nfn api =
    let mut order = 0
    let bounded = Math.clamp ({ order = order * 10 + 1; 0.5 }) ({ order = order * 10 + 2; 0.0 }) ({ order = order * 10 + 3; 1.0 })
    let power: f64 -> f64 = Math.pow 2.0
    let sine: f32 -> f32 = Math.sin
    let pi: fn() -> f128 = Math.pi
    assert (order == 123 && bounded == 0.5 && power 3.0 == 8.0 && sine 0.0f32 == 0.0f32)
    assert (Math.floor (pi()) == 3.0f128)
    assert (Math.sqrt 4.0d128 == 2.0d128)
    42
api()`);
  for (const optimization of [0, 3]) {
    assert.equal(cli(["run", source, `-O${optimization}`]).stdout, "42\n");
    const wasm = join(temporary, `math-api-${optimization}.wasm`);
    cli(["build", source, "--target", "wasm32", "--trap-info", `-O${optimization}`, "-o", wasm]);
    const module = new WebAssembly.Module(readFileSync(wasm));
    assert.deepEqual(WebAssembly.Module.imports(module), []);
    assert.equal(new WebAssembly.Instance(module).exports.tz_api(), 42n);
  }
  console.log(`math: ${cases.length} reference cases (${elementaryCases.length} elementary), native/WASM O0/O3, non-NaN bit identity, trap-info and curried API`);
} finally { rmSync(temporary, { recursive: true, force: true }); }