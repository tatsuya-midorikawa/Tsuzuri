import assert from "node:assert/strict";
import { mkdtempSync, readFileSync, writeFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";
import { casesPath as regexCasesPath, casesSource as regexCasesSource, expectedCases as regexCases } from "./regex-cases.mjs";
import { expectedCases as unicodeCases } from "./unicode-cases.mjs";
import * as unicodeData from "./unicode-ucd.mjs";
import { cases as matrixCases, traps as matrixTraps, productEntry } from "./matrix-cases.mjs";
import { cases as matrixViewCases, traps as matrixViewTraps, bounded as matrixViewBounded } from "./matrix-view-cases.mjs";
import { cases as tensorCases, traps as tensorTraps, bounded as tensorBounded } from "./tensor-cases.mjs";
import { createBoundary } from "../src/runtime/trap-boundary.mjs";
import { steadyChurn as steadyChurnReference } from "../benchmarks/heap/reference.mjs";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const compiler = resolve(process.argv[2] ?? join(root, "target/debug/tsuzuri"));
const only = process.argv[3];
const clang = process.env.TSUZURI_CLANG ?? "clang";
const nativeLibraries = process.platform === "win32" ? [] : ["-lm"];
const executableSuffix = process.platform === "win32" ? ".exe" : "";
const sanitizerKind = process.env.TSUZURI_TSAN === "1" ? "thread" : process.env.TSUZURI_ASAN === "1" ? "address" : null;
const sanitizer = sanitizerKind ? [`-fsanitize=${sanitizerKind}`] : [];
const nativeOptions = process.env.TSUZURI_TEST_CPU === "native" ? [process.arch === "arm64" ? "-mcpu=native" : "-march=native"] : [];
const wasmOptions = process.env.TSUZURI_TEST_WASM_SIMD === "1" ? ["--wasm-feature", "simd128"] : [];
// wasm64 needs Node.js 24 or newer.
const wasmTarget = process.env.TSUZURI_TEST_WASM_TARGET ?? "wasm32";
// F13: `host` builds the native IR and header with --allocator host and tracks tsuzuri_host_*.
// PM05: `small` builds it with --allocator small; the C library's calls are still tracked, and after each call every block
// that the size classes cut from a region is back in a cache or a central stack and nothing else is live.
const hostAllocator = process.env.TSUZURI_TEST_ALLOCATOR === "host";
const smallAllocator = process.env.TSUZURI_TEST_ALLOCATOR === "small";
const allocatorOptions = hostAllocator ? ["--allocator", "host"] : smallAllocator ? ["--allocator", "small"] : [];
const min = -(1n << 63n);
const max = (1n << 63n) - 1n;

function execute(program, args, success = true) {
  const result = spawnSync(program, args, {
    cwd: root, encoding: "utf8", timeout: 180_000, maxBuffer: 16 * 1024 * 1024,
  });
  if (result.error) throw result.error;
  if (success) assert.equal(result.status, 0, `${program} ${args.join(" ")}\n${result.stdout}\n${result.stderr}`);
  return result;
}
const cli = (args) => execute(compiler, args);
// Runs a program that must finish quickly. The program itself is the child and is killed with SIGKILL after
// `seconds`, so a loop that never ends cannot outlive the test (a SIGTERM aimed at a wrapper such as
// `tsuzuri run` would leave the program running at full speed).
function executeBounded(program, args, seconds = 10) {
  const result = spawnSync(program, args, {
    cwd: root, encoding: "utf8", timeout: seconds * 1000, killSignal: "SIGKILL", maxBuffer: 1024 * 1024,
  });
  assert.ok(!result.error, `${program} ${args.join(" ")} did not finish within ${seconds} s (${result.error?.code})`);
  return result;
}
// Calls one export of a WebAssembly module with i64 arguments in a child Node process and prints the result,
// so that `executeBounded` can kill a call that never returns.
const boundedWasmCall = `const { readFileSync } = require("node:fs");
const [file, name, ...args] = process.argv.slice(1);
const exports = new WebAssembly.Instance(new WebAssembly.Module(readFileSync(file))).exports;
console.log(String(exports[name](...args.map(BigInt))));`;
const cValue = (n) => typeof n !== "bigint"
  ? (Object.is(n, -0) ? "-0.0" : Number.isNaN(n) ? "NAN" : n === Infinity ? "INFINITY" : n === -Infinity ? "(-INFINITY)" : String(n))
  : n === min ? "INT64_MIN" : n < 0n ? `(-INT64_C(${-n}))`
    : n > max ? `UINT64_C(${n})` : `INT64_C(${n})`;

function orderedReferences(count, seed, bits) {
  const rounded = bits === 32 ? Math.fround : (value) => value;
  const input = (index, salt) => rounded([1e16, 1, -1e16, 3, -3, 0.5, -0.25, 1e-38][(index * 37 + salt) % 8]);
  const values = Array.from({ length: count }, (_, index) => input(index, seed));
  let level = values;
  while (level.length > 1) level = Array.from({ length: Math.ceil(level.length / 2) }, (_, index) => index * 2 + 1 < level.length ? rounded(level[index * 2] + level[index * 2 + 1]) : level[index * 2]);
  let total = 0, correction = 0, dot = 0;
  for (const [index, value] of values.entries()) {
    const next = rounded(total + value);
    correction = rounded(correction + (Math.abs(total) >= Math.abs(value) ? rounded(rounded(total - next) + value) : rounded(rounded(value - next) + total)));
    total = next;
    dot = rounded(dot + rounded(value * input(index, seed + 3)));
  }
  return [total, level[0] ?? 0, rounded(total + correction), dot];
}

// Reference for HashMap/HashSet iteration order: new keys append, replacement keeps
// the position, and removal moves the last entry into the hole (swap-remove).
function insertionTable() {
  const keys = [], values = [], index = new Map();
  return {
    keys, values, get: (key) => index.get(key),
    set(key, value) {
      const at = index.get(key);
      if (at === undefined) { index.set(key, keys.length); keys.push(key); values.push(value); } else values[at] = value;
    },
    remove(key) {
      const at = index.get(key);
      if (at === undefined) return;
      const last = keys.length - 1;
      keys[at] = keys[last]; values[at] = values[last]; index.set(keys[at], at);
      keys.pop(); values.pop(); index.delete(key);
    },
  };
}

function hashOps(count, seed, set) {
  const table = insertionTable(), wrap = (value) => BigInt.asIntN(64, value);
  let state = seed, acc = 0n;
  for (let step = 0n; step < count; step++) {
    state = wrap(state * 6364136223846793005n + 1442695040888963407n);
    const r = BigInt.asUintN(64, state) >> 33n, key = (r >> 3n) % 4099n * 8n - 16000n, op = r % 8n;
    if (op < 4n) table.set(key, step);
    else if (op < 6n) table.remove(key);
    else { const at = table.get(key); acc = wrap(acc * 31n + (at === undefined ? 7n : set ? 1n : table.values[at])); }
  }
  return table.keys.reduce((total, key, position) => wrap(total * 31n + key * 7n + (set ? 0n : table.values[position])),
    wrap(acc * 1000003n + BigInt(table.keys.length)));
}

// SipHash-c-d reference over bytes (BigInt arithmetic, written from the published algorithm).
// Its self-check below uses the official SipHash-2-4 test vectors for the key 00 01 .. 0f.
function siphash(compression, finalization, key0, key1, bytes) {
  const mask = (1n << 64n) - 1n, rotate = (x, bits) => ((x << BigInt(bits)) | (x >> BigInt(64 - bits))) & mask;
  let v0 = key0 ^ 0x736f6d6570736575n, v1 = key1 ^ 0x646f72616e646f6dn, v2 = key0 ^ 0x6c7967656e657261n, v3 = key1 ^ 0x7465646279746573n;
  const round = () => {
    v0 = (v0 + v1) & mask; v1 = rotate(v1, 13); v1 ^= v0; v0 = rotate(v0, 32);
    v2 = (v2 + v3) & mask; v3 = rotate(v3, 16); v3 ^= v2;
    v0 = (v0 + v3) & mask; v3 = rotate(v3, 21); v3 ^= v0;
    v2 = (v2 + v1) & mask; v1 = rotate(v1, 17); v1 ^= v2; v2 = rotate(v2, 32);
  };
  const end = bytes.length - bytes.length % 8;
  const absorb = (word) => { v3 ^= word; for (let step = 0; step < compression; step++) round(); v0 ^= word; };
  for (let index = 0; index < end; index += 8) {
    let word = 0n;
    for (let byte = 7; byte >= 0; byte--) word = (word << 8n) | BigInt(bytes[index + byte]);
    absorb(word);
  }
  let last = BigInt(bytes.length & 0xff) << 56n;
  for (let byte = bytes.length % 8 - 1; byte >= 0; byte--) last |= BigInt(bytes[end + byte]) << BigInt(8 * byte);
  absorb(last);
  v2 ^= 0xffn;
  for (let step = 0; step < finalization; step++) round();
  return (v0 ^ v1 ^ v2 ^ v3) & mask;
}
// JSON references (D08): FNV-1a 64 over UTF-8 bytes, and status codes `kind * 2^40 + offset`.
function fnv64(bytes) {
  let hash = 14695981039346656037n;
  for (const byte of bytes) hash = ((hash ^ BigInt(byte)) * 1099511628211n) & ((1n << 64n) - 1n);
  return BigInt.asIntN(64, hash);
}
const utf8Bytes = (text) => [...new TextEncoder().encode(text)];
const jsonTextHash = (text) => fnv64(utf8Bytes(text));
const jsonStatus = (kind, offset) => BigInt(kind) * (1n << 40n) + BigInt(offset);
// The DuplicateKey message of the first entry whose key repeats an earlier key, scanning in
// document order: the reference for Map and Set decoding, which sorts the entries instead.
// `key` maps the JSON text of an entry's key to the key.
function duplicateMessage(texts, key = (text) => text) {
  const seen = new Set();
  for (const text of texts) {
    if (seen.has(key(text))) return `json: duplicate key ${JSON.stringify(text)}`;
    seen.add(key(text));
  }
  throw new Error("no repeated key");
}
// `scattered count` of the json and cbor fixtures: a permutation with two planted repeats.
const scattered = (count) => Array.from({ length: count }, (_, index) =>
  ((index === count - 1 ? 0 : index === Math.floor(count / 2) ? Math.floor(count / 3) : index) * 7919) % count);
// The nearest f32 to a positive integer, rounded once (ties to even), computed exactly with BigInt.
function integerToF32(value) {
  const bits = value.toString(2).length;
  if (bits <= 24) return Number(value);
  const shift = BigInt(bits - 24), kept = value >> shift, rest = value - (kept << shift), half = 1n << (shift - 1n);
  const rounded = rest > half || (rest === half && (kept & 1n) === 1n) ? kept + 1n : kept;
  return Number(rounded) * 2 ** Number(shift);
}
// CBOR references (D08 Phase 2): RFC 8949 Appendix A vectors as hex, and a small deterministic encoder.
const hexBytes = (hex) => Array.from({ length: hex.length / 2 }, (_, index) => parseInt(hex.slice(index * 2, index * 2 + 2), 16));
const cborHead = (major, value) => {
  const n = BigInt(value);
  if (n < 24n) return [major << 5 | Number(n)];
  const size = n < 256n ? 1 : n < 65536n ? 2 : n < 4294967296n ? 4 : 8;
  return [major << 5 | { 1: 24, 2: 25, 4: 26, 8: 27 }[size], ...Array.from({ length: size }, (_, index) => Number(n >> BigInt(8 * (size - 1 - index)) & 255n))];
};
const cborText = (text) => { const bytes = [...new TextEncoder().encode(text)]; return [...cborHead(3, bytes.length), ...bytes]; };
const cborMap = (entries) => {
  const encoded = entries.map(([key, value]) => [cborText(key), value]);
  encoded.sort(([left], [right]) => { for (let index = 0; index < Math.min(left.length, right.length); index++) if (left[index] !== right[index]) return left[index] - right[index]; return left.length - right.length; });
  return [...cborHead(5, entries.length), ...encoded.flatMap(([key, value]) => [...key, ...value])];
};
const cborArray = (items) => [...cborHead(4, items.length), ...items.flat()];
const sipKey0 = 0x0706050403020100n, sipKey1 = 0x0f0e0d0c0b0a0908n;
assert.equal(siphash(2, 4, sipKey0, sipKey1, []), 0x726fdb47dd0e0e31n);
assert.equal(siphash(2, 4, sipKey0, sipKey1, [0]), 0x74f839c593dc67fdn);
assert.equal(siphash(2, 4, sipKey0, sipKey1, Array.from({ length: 15 }, (_, byte) => byte)), 0xa129ca6149be45e5n);

// SipHash-1-3 of the eight little-endian bytes of a word: the keyed finalizer of HashMap.with_seed.
function sip13Word(key0, key1, message) {
  const word = BigInt.asUintN(64, message);
  const bytes = Array.from({ length: 8 }, (_, byte) => Number((word >> BigInt(8 * byte)) & 0xffn));
  return BigInt.asIntN(64, siphash(1, 3, BigInt.asUintN(64, key0), BigInt.asUintN(64, key1), bytes));
}

function sip13Inputs() {
  const inputs = [[0n, 0n, 0n], [1n, 2n, 3n], [-1n, -1n, -1n], [0n, 0n, -1n], [-1n, 0n, 0n], [0n, -1n, 0n],
    [BigInt.asIntN(64, sipKey0), BigInt.asIntN(64, sipKey1), BigInt.asIntN(64, sipKey0)]];
  let state = 88172645463325252n;
  const next = () => { state = BigInt.asUintN(64, state * 6364136223846793005n + 1442695040888963407n); return BigInt.asIntN(64, state ^ (state >> 29n)); };
  for (let index = 0; index < 32; index++) inputs.push([next(), next(), next()]);
  return inputs;
}

function hashBorrowedKeys(count) {
  const wrap = (value) => BigInt.asIntN(64, value);
  let sum = 0n, found = 0n;
  for (let index = 0n; index < 2n * count; index++) {
    const present = index < count;
    if (present) found++;
    sum = wrap(sum * 31n + (present ? index * 3n : -1n));
    if (present) sum = wrap(sum + index * 3n);
  }
  return wrap(sum * 1000003n + found * 1009n + count / 2n * 7n);
}

const hashSetBorrowedKeys = (count) => count * 1000003n + count / 2n * 1009n + count / 2n;

function hashStrings(count) {
  const table = insertionTable(), wrap = (value) => BigInt.asIntN(64, value);
  for (let index = 0; index < count; index++) table.set(String(index % 9), String(index));
  table.remove("3");
  return BigInt(table.keys.length) * 100000n + table.values.reduce((total, value) => wrap(total * 7n + BigInt(value.length)), 0n);
}

function hashCollisions(count) {
  let total = 0n, length = 0n;
  for (let index = 0n; index < count; index++) if (index % 3n !== 0n) { total += index; length++; }
  return total * 1000n + length;
}

// References for string interpolation. Numbers are formatted from exact BigInt
// rationals with a single round-half-even step, never with host float printing.
const interpolationDigest = (text) => {
  let hash = 0n;
  for (let index = 0; index < text.length; index++) hash = (hash * 131n + BigInt(text.charCodeAt(index))) % 1000000007n;
  return hash;
};
const interpolationPad = (text, width, align, fill = " ") => {
  const missing = Math.max(width - [...text].length, 0);
  const before = align === "right" ? missing : align === "center" ? Math.floor(missing / 2) : 0;
  return fill.repeat(before) + text + fill.repeat(missing - before);
};

// References for the Format module and for Format instances: the spec grammar
// [[fill]align][+][width][.precision][type] read over Unicode scalars. A fill is any scalar but { } " \ CR LF
// (the lexer's rule), so half of a surrogate pair is no fill, and a precision has no leading zero except ".0".
function parseFormatSpec(text) {
  const scalars = [...text], aligns = { "<": "left", "^": "center", ">": "right" };
  const isFill = (scalar) => !["{", "}", "\"", "\\", "\r", "\n"].includes(scalar) && !(scalar.length === 1 && scalar >= "\uD800" && scalar <= "\uDFFF");
  let index = 0, fill = " ", align = "auto";
  if (scalars.length >= 2 && Object.hasOwn(aligns, scalars[1])) {
    if (!isFill(scalars[0])) return null;
    fill = scalars[0]; align = aligns[scalars[1]]; index = 2;
  }
  else if (scalars.length >= 1 && Object.hasOwn(aligns, scalars[0])) { align = aligns[scalars[0]]; index = 1; }
  const plus = scalars[index] === "+";
  if (plus) index++;
  const digits = () => {
    let value = 0n, count = 0;
    while (index < scalars.length && scalars[index].length === 1 && scalars[index] >= "0" && scalars[index] <= "9") {
      value = value * 10n + BigInt(scalars[index]); index++; count++;
    }
    return [value, count];
  };
  const leadingZero = scalars[index] === "0";
  const [width] = digits();
  let precision = -1n, valid = width <= 4096n && !leadingZero;
  if (scalars[index] === ".") {
    index++;
    const start = index;
    const [value, count] = digits();
    valid = valid && count > 0 && value <= 4096n && !(scalars[start] === "0" && count > 1);
    precision = value;
  }
  let kind = "plain";
  if (index < scalars.length && ["x", "X", "o", "b", "e", "f"].includes(scalars[index])) kind = scalars[index++];
  return valid && index === scalars.length ? { fill, align, plus, width, precision, kind } : null;
}
const describeFormatSpec = (text) => {
  const spec = parseFormatSpec(text);
  return spec ? `fill=${spec.fill} align=${spec.align} plus=${spec.plus} width=${spec.width} precision=${spec.precision} kind=${spec.kind}` : "none";
};
const formatParseSpecs = ["", ">", "<5", "^7", "*>+8.2f", "\u{1F600}^4", "\u00e9<3", ">>5", "<<<", "+", "+5", "08", "0", "5", ".3e", ".", ".x",
  "x", "X", "o", "b", "e", "f", "4096", "4097", "99999999999", "+.2f", "-^6.1e", "q", "5x7", " >2", "5>", "\uD800<2", "\u{1F600}>+10.4X", "<+",
  ".5.5", "++", "x ", "^^^^", "\"<2", "\\<2", "{<2", "}<2", "\n<2", "\r<2", "\uD83D<2", "\uDE00>3", "\uD83D\uDE00<2", "'<2", "/>3", "0>5", "+<2",
  "|^6", ".05", ".00", ".0", ".0f"];
const formatPadSpecs = ["", "5", "<5", ">5", "^5", "*^7", "-^6", "\u{1F600}>4", "<3", "^2", "8", ">1", "^8", "*<9"];
const formatPadTexts = ["", "ab", "\u00e9\u{1F600}", "\uD800", "abcdef", "\u{1F600}\u{1F600}", "x"];
const formatPadReference = () => formatPadSpecs.flatMap((specText) => formatPadTexts.map((text) => {
  const spec = parseFormatSpec(specText);
  return spec ? `${interpolationPad(text, Number(spec.width), spec.align, spec.fill)}|` : "invalid|";
})).join("");
// The Point instance of the fixture: sign and precision are shown, the rest is padded by Format.pad.
const formatPoint = (specText, x = 3n, y = -4n) => {
  const spec = parseFormatSpec(specText);
  assert.ok(spec, specText);
  const body = `${spec.plus ? "+" : ""}${x},${y}${spec.precision >= 0n ? `.${spec.precision}` : ""}`;
  return interpolationPad(body, Number(spec.width), spec.align, spec.fill);
};
const formatInstancesReference = () => [formatPoint(">10"), formatPoint("<10"), formatPoint("^10"), formatPoint("+"), formatPoint(".2"),
  formatPoint("*>+12.1f"), "L[x]", "D[>3]", `box(${formatPoint("^9.1e")})`, formatPoint("+", 0n, 1n)].join("|");
const roundHalfEven = (numerator, denominator) => {
  const quotient = numerator / denominator, twice = 2n * (numerator % denominator);
  return twice > denominator || (twice === denominator && quotient % 2n === 1n) ? quotient + 1n : quotient;
};
const pow10 = (exponent) => 10n ** BigInt(exponent);
const floorLog = (numerator, denominator, base) => {
  const atLeast = (power) => power >= 0 ? numerator >= denominator * base ** BigInt(power) : numerator * base ** BigInt(-power) >= denominator;
  let power = numerator.toString(base === 2n ? 2 : 10).length - denominator.toString(base === 2n ? 2 : 10).length;
  while (!atLeast(power)) power--;
  while (atLeast(power + 1)) power++;
  return power;
};
function fixedDigits(numerator, denominator, precision) {
  const scaled = roundHalfEven(numerator * pow10(precision), denominator);
  if (precision === 0) return scaled.toString();
  const digits = scaled.toString().padStart(precision + 1, "0");
  return `${digits.slice(0, -precision)}.${digits.slice(-precision)}`;
}
function exponentDigits(numerator, denominator, precision) {
  if (numerator === 0n) return `0${precision ? `.${"0".repeat(precision)}` : ""}e0`;
  let exponent = floorLog(numerator, denominator, 10n);
  const scale = precision - exponent;
  let scaled = scale >= 0 ? roundHalfEven(numerator * pow10(scale), denominator) : roundHalfEven(numerator, denominator * pow10(-scale));
  if (scaled >= pow10(precision + 1)) { scaled /= 10n; exponent++; }
  const digits = scaled.toString();
  return `${digits[0]}${precision ? `.${digits.slice(1)}` : ""}e${exponent}`;
}
// Nearest binary value with `precision` significant bits and minimum exponent `emin`.
function nearestBinary(numerator, denominator, precision, emin) {
  const quantum = Math.max(floorLog(numerator, denominator, 2n) - (precision - 1), emin);
  const scaled = quantum >= 0 ? roundHalfEven(numerator, denominator << BigInt(quantum)) : roundHalfEven(numerator << BigInt(-quantum), denominator);
  return quantum >= 0 ? { negative: false, numerator: scaled << BigInt(quantum), denominator: 1n } : { negative: false, numerator: scaled, denominator: 1n << BigInt(-quantum) };
}
function doubleValue(value) {
  if (Number.isNaN(value)) return { special: "nan" };
  const negative = value < 0 || Object.is(value, -0);
  if (!Number.isFinite(value)) return { special: "inf", negative };
  const view = new DataView(new ArrayBuffer(8));
  view.setFloat64(0, Math.abs(value));
  const bits = view.getBigUint64(0), biased = Number((bits >> 52n) & 0x7ffn), fraction = bits & ((1n << 52n) - 1n);
  const coefficient = biased ? fraction | (1n << 52n) : fraction, exponent = (biased || 1) - 1075;
  return exponent >= 0 ? { negative, numerator: coefficient << BigInt(exponent), denominator: 1n } : { negative, numerator: coefficient, denominator: 1n << BigInt(-exponent) };
}
function floatSpec(value, { plus = false, precision, style = "fixed" }) {
  if (value.special === "nan") return "nan";
  const sign = value.negative ? "-" : plus ? "+" : "";
  if (value.special === "inf") return `${sign}inf`;
  return sign + (style === "fixed" ? fixedDigits(value.numerator, value.denominator, precision) : exponentDigits(value.numerator, value.denominator, precision));
}
function floatInterpolation(value) {
  return [
    floatSpec(value, { precision: 0 }), floatSpec(value, { precision: 2 }), floatSpec(value, { precision: 17 }),
    floatSpec(value, { plus: true, precision: 3 }), floatSpec(value, { precision: 0, style: "exponent" }),
    floatSpec(value, { precision: 3, style: "exponent" }), floatSpec(value, { precision: 16, style: "exponent" }),
    interpolationPad(floatSpec(value, { precision: 1 }), 12, "right"),
  ].join("|");
}
function integerInterpolation(value) {
  const sign = value < 0n ? "-" : "", magnitude = value < 0n ? -value : value;
  const hex = magnitude.toString(16);
  return [
    sign + hex, sign + hex.toUpperCase(), sign + magnitude.toString(8), sign + magnitude.toString(2), value < 0n ? value.toString() : `+${value}`,
    interpolationPad(value.toString(), 25, "right"), interpolationPad(value.toString(), 25, "center", "*"), interpolationPad(value.toString(), 25, "right", "0"),
  ].join("|");
}
function wideInterpolation(kind) {
  const exact = {
    0: nearestBinary(1n, 10n, 11, -24), 1: nearestBinary(1n, 10n, 113, -16494),
    2: { negative: false, numerator: 125n, denominator: 1000n }, 3: { negative: false, numerator: 25n, denominator: 10n },
    4: { negative: false, numerator: 1n, denominator: pow10(30) },
  }[kind];
  return [floatSpec(exact, { precision: 3 }), floatSpec(exact, { precision: 20 }), floatSpec(exact, { precision: 2, style: "exponent" })].join("|");
}
const interpolationOwnedLoop = (count) => {
  let total = 0n;
  for (let index = 0n; index < count; index++) total += BigInt(`${index}:ab`.length);
  return total;
};
const interpolationDoubles = [0, -0, 0.1, 0.125, 0.375, 2.5, 3.5, 1e21, 1e-7, 5e-324, 1.7976931348623157e308, Infinity, -Infinity, NaN];
const interpolationSingles = [0, -0, 0.1, 0.125, 0.375, 2.5, 3.5, 1e21, 1e-7, 1.401298464324817e-45, 3.4028234663852886e38, Infinity, -Infinity, NaN];
const interpolationIntegers = [min, -255n, -1n, 0n, 1n, 42n, max];

// Each suite is a fixture directory whose exports are called with the listed
// arguments. Native hosts track every allocation, so each call must leave no
// live heap bytes; WASM modules must stay import-free.
// F10: the fixture's `sequence` of atomic operations, written from the instructions' definitions. Every
// operation wraps to `bits`, signed or not; the compare-exchanges see the value before they write.
function atomicSequence(bits, signed, start, delta, mask) {
  const wrap = (value) => signed ? BigInt.asIntN(bits, value) : BigInt.asUintN(bits, value);
  let cell = wrap(start);
  const seen = [];
  const update = (operate) => { seen.push(cell); cell = wrap(operate(cell)); };
  update((value) => value + delta);
  update((value) => value + delta);
  update((value) => value - mask);
  update((value) => value ^ mask);
  update((value) => value | delta);
  update((value) => value & mask);
  update(() => start);
  update((value) => value === wrap(start) ? delta : value);
  update((value) => value === wrap(start) ? mask : value);
  seen.push(cell);
  return BigInt.asIntN(64, seen.reduce((total, value) => total + BigInt.asIntN(64, value), 0n));
}

// PM04: the chunks of a list that List.cons grows (src/llvm_list.rs): the first chunk has `first` slots and each
// next one half as many again, up to `largest`. The lengths of the suite are those at, next to and between them.
function chunkBoundaries(first, largest, limit) {
  const boundaries = [];
  let capacity = first;
  let total = first;
  while (total <= limit) {
    boundaries.push(total);
    capacity = Math.min(capacity + (capacity >> 1) + 1, largest);
    total += capacity;
  }
  return boundaries;
}
function chunkLengths(first, largest, limit) {
  const lengths = new Set([0, 1, 2, 3]);
  for (const boundary of chunkBoundaries(first, largest, limit)) {
    for (const delta of [-2, -1, 0, 1, 2]) if (boundary + delta >= 0) lengths.add(boundary + delta);
  }
  return [...lengths].sort((a, b) => a - b);
}
const weightedSum = (values) => values.reduce((total, value, position) => total + value * BigInt(position + 1), 0n);
const descending = (n) => Array.from({ length: n }, (_, i) => BigInt(n - 1 - i));
const ascending = (n) => Array.from({ length: n }, (_, i) => BigInt(i));

const suites = {
  range_forms: {
    cases: (() => {
      const MAX = (1n << 63n) - 1n;
      const MIN = -(1n << 63n);
      // The values that `for i in start .. step OP finish` visits in a type of `bits`, stopping
      // where the next value would leave the type (A22 appendix F).
      const values = (start, step, finish, exclusive, bits, signed) => {
        const low = signed ? -(1n << (bits - 1n)) : 0n;
        const high = signed ? (1n << (bits - 1n)) - 1n : (1n << bits) - 1n;
        const out = [];
        for (let v = start; step > 0n ? (exclusive ? v < finish : v <= finish) : (exclusive ? v > finish : v >= finish); v += step) {
          out.push(v);
          if (v + step < low || v + step > high) break;
        }
        return out;
      };
      const fold = (list) => list.reduce((t, v) => BigInt.asIntN(64, t * 31n + v), 0n);
      const narrow = (value, bits, signed) => (signed ? BigInt.asIntN(Number(bits), value) : BigInt.asUintN(Number(bits), value));
      const loop = (bits, signed, step = 1n) => (exclusive) => (a, b) =>
        fold(values(narrow(a, bits, signed), step, narrow(b, bits, signed), exclusive, bits, signed));
      const xs = [10n, 20n, 30n, 40n, 50n];
      const slice = (a, end) => {
        const start = BigInt.asUintN(64, a);
        const stop = BigInt.asUintN(64, end);
        if (!(start <= stop && stop <= 5n)) return null;
        const part = xs.slice(Number(start), Number(stop));
        return BigInt(part.length) * 1000n + part.reduce((sum, value) => sum + value, 0n);
      };
      const cases = [];
      const add = (name, args, expected) => { if (expected !== null) cases.push([name, args, expected]); };
      for (const [a, b] of [[0n, 3n], [3n, 3n], [5n, 2n], [-3n, 3n], [2147483646n, 2147483647n]]) {
        add("closed_i32", [a, b], loop(32n, true)(false)(a, b));
        add("open_i32", [a, b], loop(32n, true)(true)(a, b));
      }
      for (const [a, b] of [[3n, 0n], [0n, 0n], [0n, 3n], [-2147483647n, -2147483648n]]) {
        add("down_closed_i32", [a, b], loop(32n, true, -1n)(false)(a, b));
        add("down_open_i32", [a, b], loop(32n, true, -1n)(true)(a, b));
      }
      for (const [a, b] of [[0n, 3n], [0n, 0n], [5n, 2n], [MAX - 1n, MAX], [MIN, MIN + 1n], [MIN, MIN]]) {
        add("closed_i64", [a, b], loop(64n, true)(false)(a, b));
        add("open_i64", [a, b], loop(64n, true)(true)(a, b));
      }
      for (const [a, b] of [[3n, 0n], [MIN + 1n, MIN], [0n, 0n]]) {
        add("down_closed_i64", [a, b], loop(64n, true, -1n)(false)(a, b));
        add("down_open_i64", [a, b], loop(64n, true, -1n)(true)(a, b));
      }
      for (const [a, s, b] of [[0n, 2n, 7n], [0n, 2n, 6n], [10n, -3n, 0n], [10n, -3n, 1n], [5n, 1n, 5n], [0n, 3n, 100n]]) {
        add("stride_closed", [a, s, b], fold(values(a, s, b, false, 64n, true)));
        add("stride_open", [a, s, b], fold(values(a, s, b, true, 64n, true)));
      }
      for (const [a, b] of [[250n, 255n], [0n, 0n], [0n, 255n], [255n, 255n]]) {
        add("u8_closed", [a, b], loop(8n, false)(false)(a, b));
        add("u8_open", [a, b], loop(8n, false)(true)(a, b));
      }
      for (const [a, s, b] of [[125n, 2n, 127n], [-128n, 50n, 127n], [127n, -1n, 126n]]) {
        add("i8_stride_closed", [a, s, b], fold(values(a, s, b, false, 8n, true)));
        add("i8_stride_open", [a, s, b], fold(values(a, s, b, true, 8n, true)));
      }
      for (const [a, b] of [[1n, 3n], [0n, 4n], [2n, 1n], [0n, -1n], [1n, 1n]]) {
        add("slice_closed", [a, b], slice(a, b + 1n));
      }
      for (const [a, b] of [[1n, 3n], [0n, 5n], [2n, 2n], [0n, 0n]]) {
        add("slice_open", [a, b], slice(a, b));
        add("slice_bare", [a, b], slice(a, b));
      }
      for (const n of [0n, 1n, 10n, 1000n]) add("sum_below_length", [n], 3n * (n * (n - 1n) / 2n));
      for (const n of [0n, 1n, 4n]) add("folded", [n], 5050n + n * (n - 1n) / 2n);
      return cases;
    })(),
    traps: [
      ["stride_open", [0n, 0n, 5n], "range step is zero"], ["stride_open", [5n, 0n, 5n], "range step is zero"],
      ["stride_closed", [0n, 0n, 5n], "range step is zero"],
      ["slice_closed", [0n, 5n], "index out of bounds"], ["slice_closed", [2n, 0n], "index out of bounds"],
      ["slice_closed", [0n, -2n], "index out of bounds"], ["slice_closed", [0n, (1n << 63n) - 1n], "index out of bounds"],
      ["slice_open", [0n, 6n], "index out of bounds"],
    ],
    trapKind: "range step is zero",
    inspect(ir) {
      // The closed loops keep their inclusive test; `..=` slices end at `b + 1`.
      assert.match(ir, /icmp eq i64/);
      assert.match(ir, /add i64 %[\w.]+, 1/);
    },
  },
  contracts: {
    options: ["--contracts", "check"],
    cases: (() => {
      const wrap = (value) => BigInt.asIntN(64, value);
      const truncating = (a, b) => a / b;
      return [
        ["clamp", [0n, 10n, -5n], 0n], ["clamp", [0n, 10n, 5n], 5n], ["clamp", [0n, 10n, 50n], 10n],
        ["clamp", [min, max, 0n], 0n],
        ["safe_div", [7n, 2n], truncating(7n, 2n)], ["safe_div", [-7n, 2n], truncating(-7n, 2n)],
        ["sum_to", [10n], 55n], ["sum_to", [1000000n], 1000000n * 1000001n / 2n],
        ["at_least", [3n, 5n], 5n], ["at_least", [-2n, -9n], -2n], ["wrap_add", [max, 1n], wrap(max + 1n)],
        ["abs_diff", [9n, 4n], 5n], ["bump_first", [9n], 10n],
      ];
    })(),
    traps: [
      ["clamp", [5n, 3n, 0n], "contract requires failed"], ["sum_to", [-1n], "contract requires failed"],
      ["abs_diff", [max, -1n], "contract ensures failed"], ["bump_first", [1n], "contract ensures failed"],
    ],
    trapKind: "contract requires failed",
    inspect(ir) {
      assert.match(ir, /@tz\.builtin\.\$requires/);
      assert.match(ir, /@tz\.builtin\.\$ensures/);
    },
  },
  effect_attributes: {
    cases: (() => {
      const wrap = (value) => BigInt.asIntN(64, value);
      const table = [3n, 1n, 4n, 1n, 5n, 9n, 2n, 6n];
      const inputs = [-3n, -1n, 0n, 1n, 2n, 7n, 8n, 100n, 1000n];
      const expectations = {
        sum_squares: (n) => { let total = 0n; for (let i = 0n; i <= n; i++) total = wrap(total + i * i); return total; },
        table_lookup: (n) => (n < 0n || n >= 8n ? -1n : table[Number(n)]),
        clamped: (n) => (n < 0n ? 0n : n > 100n ? 100n : n) + 1n,
        applied: (n) => wrap(wrap(n * 3n + 1n) + 1n),
        divided: (n) => (n === 0n ? 0n : 142n + n / 3n - n % 5n),
        checksum: (n) => [1n, 2n, 3n, 4n, 5n].reduce((sum, byte) => wrap(sum * 31n + byte), n),
      };
      const extremes = [-(1n << 63n), (1n << 63n) - 1n];
      return Object.entries(expectations).flatMap(([name, expected]) =>
        [...inputs, ...(name === "sum_squares" ? [] : extremes)].map((n) => [name, [n], expected(n)]));
    })(),
  },
  pure_parallel: {
    cases: [
      ...[0n, 1n, 4095n, 4096n, 4097n, 8192n].map((n) => ["parallel_sum", [n], n * (n + 1n)]),
      ...[0n, 1n, 99n].flatMap((n) => [["run_task", [n], n + 1n], ["run_pair", [n], 3n * n + 1n]]),
    ],
    traps: [["parallel_trap", [4097n]]],
  },
  pure_functions: {
    cases: (() => {
      const range = (n) => Array.from({ length: Number(n) }, (_, i) => BigInt(i));
      const expectations = {
        square_sum: (n) => range(n).reduce((sum, i) => sum + i * i, 0n),
        greeting: (n) => 2n * n + 12n,
        doubled_sum: (n) => n * (n + 1n),
        local_update: (n) => {
          const xs = [1n, 2n, 3n, 4n];
          for (const i of range(n)) xs[Number(i % 4n)] = i * 3n;
          return xs[0] + 10n * xs[1] + 100n * xs[2] + 1000n * xs[3];
        },
        largest_x: (n) => range(n).reduce((max, i) => { const x = (i * 7n) % 11n; return x > max ? x : max; }, 0n),
        renamed: (n) => 2n * n + 2n,
        first_or: (n) => n % 2n === 0n ? n : 0n,
        pick: (n) => (n + 1n) + ((n % 3n) + 1n) * (n + 1n),
        with_first_own: (n) => 101n * n + 91n,
        build_list: (n) => range(n + 1n).slice(1).reduce((acc, i) => acc * 3n + i, 0n),
        shout: (n) => n + 1n,
        pick_own: (n) => n % 2n === 0n ? n : n + 1n,
      };
      return [
        ...Object.entries(expectations).flatMap(([name, expected]) => range(13n).map((n) => [`t_${name}`, [n], expected(n)])),
        ...["t_factory_owned", "t_factory_borrowed", "t_factory_returned", "t_factory_class", "t_default_factory", "t_prefix_factory", "t_member_distance", "t_annotated_distance", "t_member_factory", "t_annotated_factory", "t_pure_generic", "t_pure_callback", "t_pure_cpu", "t_pure_deferred", "t_pure_std_weaken", "t_qualified_factory", "t_pure_reuse", "t_pure_reuse_mutated", "t_pure_unsigned_reuse"].flatMap(name =>
          [-3n, 0n, 1n, 20n, 1024n, -(1n << 63n), (1n << 63n) - 1n].map(n => [name, [n], BigInt.asIntN(64, 2n * n + 1n)])),
        ...[-(1n << 31n), (1n << 31n) - 1n, -1n, 0n, 1024n, -(1n << 63n), (1n << 63n) - 1n].map(n =>
          ["t_pure_narrow_reuse", [n], BigInt.asIntN(32, 2n * n + 1n)]),
        ...[0n, 1n, 5n, 10n, 20n].map((n) => ["t_fact", [n], range(n + 1n).slice(1).reduce((acc, i) => acc * i, 1n)]),
        ["t_unused_trap", [0n], 0n],
      ];
    })(),
    traps: [["t_unused_trap", [-1n]], ["t_unused_trap", [2n]]],
  },
  conventions: {
    cases: [
      ["borrowed_text", [], 2n * BigInt("hello".length)],
      ["borrowed_array", [], 2n + 3n + 5n + 7n + 4n + 5n],
      ["borrowed_record", [], 2n * BigInt("hello".length) + 7n],
      ["owned_text", [], BigInt("hello!".length + "hello".length)],
      ["generic_read", [], 18n],
      ["generic_scalar", [], 42n],
      ...[-3, 0, 1, 5].map((n) => {
        const wide = BigInt(n), flag = n > 0;
        const a = wide + wide + wide;
        const b = (wide + 1n) + wide * 2n - wide;
        const c = (flag ? wide : 7n) + (flag ? 100n : 200n);
        const d = (n === 1 ? 1000n : 2000n) + wide + 10000n;
        const e = wide * 100000n;
        const f = BigInt((flag ? "long text" : "s").length + (n === 1 ? "one" : "other").length);
        // `n + 2147483647` overflows i32 for a positive n, and the handler's `Error` reads as -1.
        const g = (wide + 1n) + (flag ? -1n : wide + 2147483647n);
        return ["value_temporaries", [n], a + b + c + d + e + f + g];
      }),
      ["read_map", [], 5n],
      ["read_map_alias", [], 5n],
      ["read_mapi", [], 4n],
      ["read_fold", [], 3n],
      ["read_list_map", [], 3n],
      ["read_list_fold", [], 3n],
      ["read_fold_back", [], 23n],
      ["read_list_fold_back", [], 23n],
      ["read_list_mapi", [], 15n],
      ["read_iters", [], 4n],
      ["callback_constructed_once", [], 109n],
      ["mutable_record", [], 10n],
      ["mutable_elements", [0n], 14n],
      ["mutable_elements", [1n], 13n],
      ["rhs_before_index", [], 1299n],
      ["mutable_vector", [0n], 0n],
      ["mutable_vector", [1n], 5n],
      ["mutable_vector", [50n], 290n],
      ["mutable_vector_alias", [], 30n],
      ["mutable_vector_subject_last", [], 3n],
      ["mutable_drop_field", [], 5n],
      ["mutable_local_projections", [0n], 10n + 10n + 8n + 12n],
      ["mutable_local_projections", [1n], 9n + 9n + 7n + 11n],
      ["borrowed_callable_temporaries", [], 3n * 42n],
      ["contextual_own_type_name", [], 42n],
      ["trap", [1n], 10n + 20n + 20n],
    ],
    traps: [
      ["trap", [-1n]], ["trap", [2n]],
      ["mutable_elements", [-1n]], ["mutable_elements", [2n]],
      ["mutable_local_projections", [-1n]], ["mutable_local_projections", [2n]],
      ["read_array_iter_trap", []], ["read_array_iteri_trap", []],
      ["read_list_iter_trap", []], ["read_list_iteri_trap", []],
    ],
  },
  storage: {
    input: "storage/Storage.tz",
    cases: [
      ["literal_concat_stack", [], 115n],
      ["length_only_concat", [1n], 5n], ["length_only_concat", [0n], 6n],
      ["buffer_concat", [1n], 598n], ["buffer_concat", [0n], 698n],
      ["utf8_buffer_concat", [1n], 598n],
      ["buffer_escape", [1n], 5n],
      ["boundary_concat", [0n], 128n], ["boundary_concat", [1n], 129n],
      ["local_closure", [10n], 65n], ["capture_stack", [], 76n],
      ["field_take", [], 4n], ["field_take_nested", [], 4n],
      ["field_take_shared", [], 2n], ["field_update_reuse", [], 42n],
      ["string_append_owned", [], 8n], ["utf8_append_owned", [], 8n],
      ["concat_stack", [], 1206n],
      ["identity_concat_length", [0n], 3n], ["identity_concat_length", [1n], 5n],
      ["identity_concat_local", [0n], 100n], ["identity_concat_local", [1n], 102n],
      ["identity_concat_copy", [0n], 33n], ["identity_concat_copy", [1n], 55n],
    ],
  },
  param_facts: {
    traps: [["trap_step", []], ["trap_assert", []], ["trap_unreachable", []],
      ["trap_checked", []], ["trap_discarded", []], ["trap_loop", []]],
    cases: [
      ...[0n, 1n, 2n, 17n, 1000n].flatMap(count => [min, -1n, 0n, 1n, max].flatMap(seed => [
        ["scalar_loop", [count, seed], BigInt.asIntN(64, 7n + count * seed)],
        ["interior_alias", [count, seed], BigInt.asIntN(64, seed + count)],
        ["self_tail", [count, seed], BigInt.asIntN(64, seed + count * (count + 1n) / 2n)],
      ])),
      ...[min, -1n, 0n, 1n, max].flatMap(seed => [
        ["shared_alias", [seed], BigInt.asIntN(64, seed * 2n)],
        ["returned_reference", [seed], BigInt.asIntN(64, seed * 2n)],
        ["callback_reference", [seed], seed],
        ["stored_reference", [seed], seed],
        ["mutex_alias", [seed], BigInt.asIntN(64, seed + 1n)],
        ["union_overlap", [seed], BigInt.asIntN(64, seed + 3n)],
      ]),
      ...[0n, 1n, 2n, 17n, 1000n].map(count => ["buffer_loop", [count], count + 3n * count * (count - 1n) / 2n]),
      ["overlapping_slices", [], 23n], ["disjoint_siblings", [], 4534n], ["byte_buffer", [], 255n],
    ],
    inspect(ir) {
      assert.match(ir, /!alias\.scope !\d+/);
      assert.match(ir, /!noalias !\d+/);
      assert.match(ir, /!tbaa !\d+/);
      assert.match(ir, /!nonnull !\d+/);
      assert.match(ir, /@tz\.fn\.Main\.accumulate\(ptr noalias nocapture noundef nonnull/);
      const body = name => ir.slice(ir.indexOf(`@tz.fn.Main.${name}(`)).split("\n}\n")[0];
      assert.doesNotMatch(body("atomic_steps"), /!tbaa|!noalias|!alias\.scope/);
      assert.doesNotMatch(body("mutex_steps"), /!tbaa|!noalias|!alias\.scope/);
      assert.doesNotMatch(body("first_pair"), /!noalias|!alias\.scope/);
      assert.doesNotMatch(body("copy_byte"), /!tbaa/);
      assert.doesNotMatch(body("read_mixed"), /!tbaa/);
    },
  },
  higher_kinds: {
    cases: [
      ...[min, -1n, 0n, 41n, max].flatMap(seed => [["option_map", [seed], BigInt.asIntN(64, seed + 1n)], ["result_map", [seed], BigInt.asIntN(64, seed * 3n)]]),
      ["result_error", [], 6n], ["all_constructors", [], 210n], ["binary_constructor", [], 42n], ["first_class", [], 42n],
      ...[0n, 1n, 10000n].map(count => ["owned_option", [count], count * 6n]),
    ],
    inspect(ir) { assert.doesNotMatch(ir, /%tz\.hkt|dictionary/); },
  },
  // PR03 F1': a union result leaves through one return slot only when every exit may, so the cases also cover the
  // functions that keep their plain `ret`s. Expected values are worked out by hand from the source.
  return_slot: {
    cases: [
      ...[[5n, 10n], [1n, 2n], [0n, -1n], [-7n, -1n], [(1n << 62n) - 1n, (1n << 63n) - 2n]].map(([n, expected]) => ["slot_make", [n], expected]),
      ...[[0n, 4n], [21n, 42n], [1n, 2n], [-3n, -6n]].map(([n, expected]) => ["slot_pick", [n], expected]),
      // "zero" is 4 bytes; "negative " is 9 bytes plus the digits of -n.
      ...[[7n, 7n], [1n, 1n], [0n, 4n], [-1n, 10n], [-12n, 11n], [-1000000n, 16n]].map(([n, expected]) => ["slot_explain", [n], expected]),
      // "item " is 5 bytes plus the digits of n.
      ...[[7n, 6n], [12345n, 10n], [0n, -1n], [-3n, -1n]].map(([n, expected]) => ["slot_label", [n], expected]),
      // "scratch " is 8 bytes plus the characters of n; the Ok exit returns that length, the Error exit the text.
      ...[[7n, 9n], [0n, 9n], [-12n, 11n], [12345n, 13n]].map(([n, expected]) => ["slot_scratch", [n], expected]),
      // The first n with n * n > limit, or none up to 1000.
      ["slot_find", [1n, 50n], 8n], ["slot_find", [1n, 2000000n], -1n], ["slot_find", [1000n, 0n], 1000n],
      ["slot_find", [1001n, 0n], -1n], ["slot_find", [0n, 0n], 1n], ["slot_find", [-5n, 3n], -5n],
      ...[[150n, 300n], [101n, 202n], [100n, -1n], [50n, -1n]].flatMap(([n, expected]) => [["slot_wrap", [n], expected], ["slot_piped", [n], expected], ["slot_read_call", [n], expected], ["slot_read_local", [n], expected]]),
      ...[[3n, 3n], [1n, 1n], [0n, -1n], [-8n, -1n]].map(([n, expected]) => ["slot_field", [n], expected]),
      ...[[1n, 101n], [50n, 150n], [0n, -1n], [-4n, -1n]].map(([n, expected]) => ["slot_local", [n], expected]),
      ...[[5n, 45n], [0n, 40n], [-30n, 10n]].map(([n, expected]) => ["slot_let_chain", [n], expected]),
      ...[[0n, 1n], [1n, 0n], [10n, 1n], [7n, 0n], [1000n, 1n], [999n, 0n]].map(([n, expected]) => ["slot_parity", [n], expected]),
      ...[0n, 1n, 5n, 1000n].map(n => ["slot_count_down", [n], 7n]),
      ...[0n, 1n, 1000n].map(n => ["slot_nested", [n], n]),
      ...[[3n, 6n], [0n, -1n], [-2n, -1n]].map(([n, expected]) => ["slot_apply", [n], expected]),
      // `make n` is Some (2 * n) for n > 0; the closure recursion ends in Some 7 at depth 0.
      ...[[3n, 6n], [0n, -1n], [-2n, -1n]].map(([n, expected]) => ["slot_hold", [n], expected]),
      ...[0n, 1n, 500n].map(n => ["slot_descend", [n], 7n]),
      ...[[0n, 10n], [1n, 20n], [2n, 30n], [9n, 30n], [-1n, 30n]].map(([n, expected]) => ["slot_color", [n], expected]),
    ],
    inspect(ir) {
      const body = (name) => {
        const start = ir.search(new RegExp(`^define [^\\n]*@tz\\.fn\\.Main\\.${name}\\(`, "m"));
        assert.ok(start >= 0, `${name} is defined`);
        return ir.slice(start).split("\n}\n")[0];
      };
      const returns = (name) => body(name).split("\n").filter((line) => line.startsWith("  ret ")).length;
      // Every exit builds a value or reads a field or a parameter: one entry-block slot and one `ret`.
      for (const name of ["make", "pick", "explain", "scratch", "find", "choose", "keep"]) {
        assert.equal(body(name).split("%tz.return.slot = alloca").length - 1, 1, `${name}: one return slot`);
        assert.match(body(name), /\ntz\.return:\n/, `${name}: one return block`);
        assert.equal(returns(name), 1, `${name}: one ret`);
      }
      // An exit that may hold the result of a call (a call, a pipeline, a local bound to one, a field of
      // either), a recursion cycle, a continuation, an unknown callee, a callee that gets the closure of its
      // recursion, a bare tag, and a niche union (`label` returns a `Maybe<string>`, which is its payload and
      // has no tag to merge, PM01) keep every plain `ret`.
      for (const [name, count] of [["wrap", 2], ["piped", 2], ["read_call", 2], ["read_local", 2], ["hold", 2], ["guard", 2], ["descend", 2], ["even_m", 2], ["odd_m", 2], ["count_down", 2], ["nest", 2], ["apply_to", 2], ["color", 3], ["label", 2]]) {
        assert.doesNotMatch(body(name), /tz\.return/, `${name}: no return slot`);
        assert.equal(returns(name), count, `${name}: plain rets`);
      }
    },
  },
  fma_reductions: {
    cases: [
      ...[32, 64].flatMap((bits) => [...Array.from({ length: 18 }, (_, index) => index), 31, 32, 33, 63, 64, 65, 127, 128, 129, 255, 256, 257, 511, 512, 513, 1023, 1024, 1025].flatMap((count) => [0, 1, 7].flatMap((seed) => orderedReferences(count, seed, bits).map((expected, operation) => [`ordered${bits}`, [BigInt(count), BigInt(seed), BigInt(operation)], expected])))),
      ...[0n, 1n, 2n, 3n, 17n, 1025n].map((count) => ["fused_order", [count], 1]),
      ["all_formats", [], 1], ["evaluation_order", [], 123n],
    ],
    traps: [["mismatch", [0n]], ["mismatch", [1n]]],
  },
  computation_extensions: {
    cases: [
      ["match_some", [], 42n], ["match_none", [], 1], ["result_error", [], 4n],
      ["bind_return", [], 1042n], ["bind_two", [], 2042n], ["merge_fallback", [], 42n],
      ["mutable_bind_two", [], 2042n],
      ["source_order", [], 6123n], ["strict_none", [], 2n], ["mutable_group", [], 42n],
      ...[0n, 1n, 10000n].map((count) => ["owned_match", [count], count * 10n]),
    ],
    traps: [["strict_trap", []]],
  },
  // B08: the values follow from the virtual-clock rules by hand: time advances only when every
  // computation sleeps, to the earliest wake-up; a round resumes children in index order.
  async: {
    cases: [
      // yield, then sleep 3 from time 0: now = 3.
      ...[0n, 4n, -5n].map((n) => ["basic", [n], n * 10n + 3n]),
      ["sleep_zero", [0n], 0n],
      // Two sleeps of the largest i64: the second wake-up saturates.
      ["saturated", [0n], max],
      // Worker 1 records times 1, 2, 3 and worker 2 times 2, 4.
      ["timeline", [0n], 123n * 1000n + 24n],
      // The spinning child yields three times at time 0 before the sleeper wakes at 1.
      ["starvation", [0n], 0n * 10n + 1n],
      // [[12, 3], [2]]: the inner workers wake at 1 and 2, at 3, and at 2.
      ["nested_all", [0n], 12n * 10000n + 3n * 100n + 2n],
      ["all_empty", [0n], 0n],
      ["results_ok", [0n], 1n * 10n + 2n],
      // Index 0 and 1 fail at time 2; within the round index 0 comes first.
      ["results_first_error", [0n], -1n],
      // Index 1 fails at time 1, before index 0 at time 3.
      ["results_time_order", [0n], -2n],
      // The failure at time 1 drops the child that owns 100 elements; live == 0 proves it.
      ["cancel_frees", [0n], -5n],
      ...[0n, 1n, 1000n].map((count) => ["for_suspending", [count], count]),
      ["for_sync", [100000n], 100000n],
      ["for_sync_many", [1000000n], 1000000n],
      ["tail_loop", [1000000n], 1000000n],
      // Non-tail nesting uses stack per level, like synchronous recursion (docs/language.md).
      ["deep", [200n], 200n],
      // The inner run has its own clock: it sees 2, the outer 5.
      ["nested_run", [0n], 2n * 10n + 5n],
      ["unused", [7n], 7n],
      ["divide_after_yield", [2n], 5n],
      // "7", "42", and "abc".
      ["owned_results", [0n], 1n * 100n + 2n * 10n + 3n],
      // Ok ["12", "5"], and Error "failed".
      ["owned_failure", [5n], 2n * 10n + 1n], ["owned_failure", [-1n], -6n],
      ["borrows_end", [4n], 3n * 100n + 4n],
      ["implicit_body", [21n], 42n],
      ["task_inside", [3n], 4n + 3n],
      // Both spellings sleep n, read now = n, and add 1: 6 * 1000 + 6 for n = 5.
      ["alias_spelling", [5n], 6n * 1000n + 6n], ["alias_spelling", [0n], 1n * 1000n + 1n],
    ],
    nativeCases: [["deep", [1000n], 1000n]],
    traps: [["divide_after_yield", [0n]]],
    inspect(ir) {
      // Resuming a continuation costs no frame of its own, even at -O0.
      assert.match(ir, /^define internal \S+ @tz\.builtin\.Async\.__resume\S*\(.*\) nounwind alwaysinline \{$/m);
      assert.doesNotMatch(ir, /^define internal \S+ @tz\.builtin\.Async\.__resume\S*\(.*\) nounwind \{$/m);
    },
  },
  simd: {
    cases: [
      ...[128, 256].flatMap((width) => [8, 16, 32, 64].flatMap((bits) => [false, true].flatMap((unsigned) => [0n, 1n, -1n, 127n, -129n, 2147483647n, -(1n << 63n)].flatMap((seed) => [0n, 1n, BigInt(bits), BigInt(bits + 1)].map((shift) => {
        const cast = (value) => unsigned ? BigInt.asUintN(bits, value) : BigInt.asIntN(bits, value);
        let expected = 0n;
        for (let lane = 0; lane < width / bits; lane++) {
          const value = cast(seed + (lane === 0 ? 7n : 0n));
          expected = cast(expected + cast((value * value + value) << (shift & BigInt(bits - 1))));
        }
        return [`${width === 128 ? "vector" : "wide"}_i${bits}${unsigned ? "u" : ""}`, [seed, shift], BigInt.asIntN(64, expected)];
      }))))),
      ["float_semantics", [], 1], ["float_order", [], 1], ["simd_owned", [], 42n], ["mask_storage", [], 1],
      ...[0n, 1n, 4n].map((index) => ["simd_load", [index], 10n + index * 4n]),
      ["operator_method", [41n], 42n],
      ["wide_float", [], 1], ["wide_storage", [], 120042n],
      ...[0n, 1n, 4n].map((index) => ["wide_load", [index], 10n + index * 4n]),
      ...[0n, 1n, 2n].map((index) => {
        const data = Array(12).fill(0n);
        for (let lane = 0n; lane < 8n; lane++) data[Number(2n + index + lane)] = lane + 1n;
        return ["simd_store", [index], data.reduce((total, value) => total * 3n + value, 0n)];
      }),
      ...[[0n, 53n], [1n, 530n], [2n, 5300n]].map(([index, expected]) => ["simd_store_narrow", [index], expected]),
    ],
    traps: [
      ["simd_extract_trap", [-1n]], ["simd_extract_trap", [4n]], ["simd_load", [-1n]], ["simd_load", [5n]], ["simd_load", [9223372036854775807n]],
      ["wide_load", [-1n]], ["wide_load", [5n]], ["simd_store", [-1n]], ["simd_store", [3n]], ["simd_store", [9223372036854775807n]],
      ["simd_store_narrow", [3n]], ["simd_store_narrow", [-1n]],
    ],
    inspect(ir) {
      for (const type of ["<16 x i8>", "<8 x i16>", "<4 x i32>", "<2 x i64>", "<4 x float>", "<2 x double>", "<4 x i1>", "<32 x i8>", "<16 x i16>", "<8 x i32>", "<4 x i64>", "<8 x float>", "<4 x double>", "<32 x i1>"]) assert.ok(ir.includes(type), type);
      assert.doesNotMatch(ir, /(?:fadd|fmul) fast|add nsw <|add nuw </);
      // 256-bit vectors live in storage aligned to 16 bytes, so no access may assume 32.
      for (const [line] of ir.matchAll(/^\s*(?:store <(?:32 x i8|16 x i16|8 x i32|4 x i64|8 x float|4 x double)> |%\S+ = load <(?:32 x i8|16 x i16|8 x i32|4 x i64|8 x float|4 x double)>, ).*$/gm)) {
        assert.match(line, /, align (?:1|2|4|8|16)(?:, !|$)/, line);
      }
    },
  },
  borrowed_records: {
    cases: [
      ...[0n, 1n, 42n, 10000n].map((count) => ["view_sum", [count], count * (count + 1n)]),
      ["text_view", [], 16n], ["partial_view", [], 13n],
      ["view_iteration", [10000n], 60000n], ["pair_view", [], 3n],
      ["named_view", [], 10n], ["named_reference", [], 42n],
      ["region_call", [], 7n], ["region_field", [], 7n], ["region_nested", [], 7n], ["region_update", [], 10n],
      ...[0n, 1n, 1000n].map((count) => ["region_loop", [count], count * 9n]),
      ["region_callback", [], 28n],
      ...[0n, 1n, 1000n].map((count) => ["region_callback_loop", [count], count * 9n]),
      ...[0n, 1n, 10n, 10000n].map((count) => ["exclusive_loop", [count], 3n + 3n * count]),
      ...[0n, 1n, 5n, 1000n].map((count) => ["exclusive_vec", [count], 1001n * count]),
      ...[7n, 12n, 15n, 7n, 9n, 17n, 1125n, 3n, 32n, 11n, 8n, 13n, 14n, 3n, 7n, 9n].map((value, index) => [`exclusive_${index + 1}`, [], value]),
      ["target_local", [], 6n], ["target_swap", [], 65n], ["target_editor", [], 62n], ["target_kept", [], 5n],
      // Even passes store "abcdef" (6), odd passes "ab" (2), and each pass adds the stored length.
      ...[0n, 1n, 10n, 10000n].map((count) => ["target_loop", [count], 6n * ((count + 1n) / 2n) + 2n * (count / 2n)]),
      // The last pass stores "beta!!" (6) or "alpha" (5) and the decimal text of its index times 1000.
      ...[0n, 1n, 10n, 1000n].map((count) => {
        if (count === 0n) return ["target_owned", [count], 5n * 100000n + 5n];
        const last = count - 1n;
        return ["target_owned", [count], (last % 2n === 0n ? 6n : 5n) * 100000n + BigInt(String(last * 1000n).length)];
      }),
    ],
  },
  iteration_protocol: {
    cases: [
      ["seq_once", [], 42n], ["seq_empty", [], 0n], ["seq_cold", [], 42n],
      ...[0n, 1n, 10n, 1000n, 10000n].flatMap((count) => {
        let filtered = 0n;
        let textLength = 0n;
        for (let value = 0n; value < count; value++) {
          if (value % 3n === 0n) filtered += value * 2n;
          if (String(value).length % 2 === 0) textLength += BigInt(String(value).length);
        }
        return [["seq_range_sum", [count], count * (count - 1n) / 2n], ["seq_list", [count], count * (count - 1n) / 2n], ["seq_map_filter", [count], filtered], ["seq_owned", [count], textLength]];
      }),
      ["seq_borrowed", [], 25n], ["seq_early_exit", [], 100n], ["seq_task", [], 42n],
    ],
    inspect(ir) {
      const next = [...ir.matchAll(/^define internal [^\n]*@tz\.builtin\.Seq\.next[^\n]*\{([\s\S]*?)^\}/gm)];
      assert.ok(next.length > 0);
      for (const [, body] of next) {
        assert.doesNotMatch(body, /@tz\.closure\.clone/);
        assert.match(body, /i1 (?:false|0)\)/);
      }
    },
  },
  map_set: {
    cases: [
      ...[0n, 1n, 33n, 257n, 1024n].flatMap((count) => {
        const map = new Map();
        const text = new Map();
        const left = new Set();
        const right = new Set();
        for (let index = 0n; index < count; index++) {
          map.set(index * 37n % 101n, index * 3n);
          text.set(String(index % 9n), String(index));
          if (index % 2n === 0n) left.add(index);
          if (index % 3n === 0n) right.add(index);
        }
        const checksum = [...map].sort(([left], [right]) => Number(left - right)).reduce((total, [key, value]) => BigInt.asIntN(64, total * 31n + key * 7n + value), 0n);
        const sum = (values) => [...values].reduce((total, value) => total + value, 0n);
        const union = new Set([...left, ...right]);
        const common = [...left].filter((value) => right.has(value));
        const different = [...left].filter((value) => !right.has(value));
        const oddCount = count / 2n;
        return [
          ["map_insert_lookup", [count], checksum],
          ["map_replace_drop", [count], BigInt([...text].reduce((total, [key, value]) => total + key.length + value.length, 0))],
          ["map_remove", [count], 3n * oddCount * oddCount],
          ["set_algebra", [count], sum(union) * 1000000n + sum(common) * 1000n + sum(different)],
        ];
      }),
      ["set_owned_union", [], 2n], ["map_capture", [], 84n], ["map_borrowed_values", [], 8n],
      ["first_representative", [], 1], ["map_task_drop", [], 0n], ["owned_record_keys", [], 42n],
    ],
    traps: [["map_at_missing", []], ["nan_query", []], ["nan_stored", []]],
    inspect(ir) {
      const lookups = [...ir.matchAll(/^define internal [^\n]*@tz\.fn\.Map\.(?:locate|chunk_of|chunk_total|get|at|contains_key)[^\n]*\{([\s\S]*?)^\}/gm)];
      assert.ok(lookups.length > 0);
      for (const [, body] of lookups) assert.doesNotMatch(body, /@tz\.(?:alloc|realloc)\(/);
    },
  },
  map_set_chunks: {
    cases: [
      // Ascending fills 512 before appending; descending splits the first full chunk in half.
      ["ascending", [0n], 0n], ["ascending", [512n], 1n], ["ascending", [513n], 2n], ["ascending", [1024n], 2n], ["ascending", [1025n], 3n],
      ["descending", [512n], 1n], ["descending", [513n], 2n], ["descending", [768n], 2n], ["descending", [769n], 3n],
      ["middle_split", [], 1002n], ["merge_after_removals", [], 22001256n], ["head_deleted", [], 201256n], ["tail_deleted", [], 2001n],
      ["set_algebra_chunks", [], 40202n],
      ["array_key_selection", [1], 105n],
      ["array_key_selection", [0], 210n],
      ["large_array_key_selection", [1], 2n],
      ["large_array_key_selection", [0], 1200003n],
      ["comparison_bounds", [32768n], 0n], ["set_algebra_bounds", [16384n], 0n], ["borrowed_keys_and_values", [], 33n],
      ...[0n, 1n, 2n, 3n].flatMap((profile) => [["model_map", [profile, 11n], 1n], ["model_set", [profile, 11n], 1n], ["model_algebra", [profile, 11n], 1n]]),
    ],
    traps: [],
    inspect(ir) {
      const lookups = [...ir.matchAll(/^define internal [^\n]*@tz\.fn\.Map\.(?:locate|chunk_of|chunk_total|beyond_last|get|contains_key)[^\n]*\{([\s\S]*?)^\}/gm)];
      assert.ok(lookups.length > 0);
      for (const [, body] of lookups) assert.doesNotMatch(body, /@tz\.(?:alloc|realloc)\(/);
    },
  },
  // The references loop over up to 4,194,305 elements in BigInt: computed only when the suite runs.
  monoid: {
    get cases() {
      return (this.computed ??= (() => {
        // Independent references: plain loops over BigInt, with the wrapping of each element type.
        const mix = (index) => (index * 2654435761n + 12345n) % 1000003n - 500000n;
        const loop = (count, initial, step) => {
          let state = initial;
          for (let index = 0n; index < count; index++) state = step(state, index);
          return state;
        };
        const references = {
          sum_total: (count) => loop(count, 0n, (total, index) => total + mix(index)),
          product_total: (count) => loop(count, 1n, (total, index) => BigInt.asIntN(32, total * (BigInt.asIntN(32, mix(index)) | 1n))),
          max_total: (count) => count === 0n ? -1000000n : loop(count, mix(0n), (best, index) => mix(index) > best ? mix(index) : best),
          min_total: (count) => count === 0n ? 1000n : loop(count, BigInt.asIntN(8, mix(0n)), (best, index) => BigInt.asIntN(8, mix(index)) < best ? BigInt.asIntN(8, mix(index)) : best),
          and_total: (count) => loop(count, 4294967295n, (total, index) => total & ((BigInt.asUintN(32, mix(index)) | 4042322160n))),
          or_total: (count) => loop(count, 0n, (total, index) => total | (mix(index) & 1048575n)),
          xor_total: (count) => loop(count, 0n, (total, index) => total ^ mix(index)),
          all_total: (count) => count <= 1000002n ? 1n : 0n,
          any_total: (count) => count > 4098n ? 1n : 0n,
          affine_total: (count) => {
            const [scale, shift] = loop(count, [1n, 0n], ([scale, shift], index) => {
              const next = [index % 5n + 1n, mix(index) % 17n];
              return [BigInt.asIntN(64, next[0] * scale), BigInt.asIntN(64, next[0] * shift + next[1])];
            });
            return BigInt.asIntN(64, scale * 1000003n + shift);
          },
          stats_total: (count) => count === 0n ? 0n : count * 1000003n + loop(count, 0n, (total, index) => total + mix(index)) + loop(count, mix(0n), (best, index) => mix(index) > best ? mix(index) : best),
        };
        const sizes = [0n, 1n, 4095n, 4096n, 4097n, 8193n, 12288n, 100000n];
        return [
          ...Object.entries(references).flatMap(([name, reference]) => sizes.map((count) => [name, [count], reference(count)])),
          ...[4194304n, 4194305n].map((count) => ["cap_xor", [count], loop(count, 0n, (total, index) => total ^ (index % 251n))]),
        ];
      })());
    },
    traps: [],
  },
  parallel_equivalence: {
    get cases() {
      return (this.computed ??= (() => {
        const sizes = [0n, 1n, 4095n, 4096n, 4097n, 8193n, 12288n, 100000n];
        // The sum of f64 by the chunk rule of docs/language.md and by one running total, in IEEE double arithmetic.
        const decimals = (count) => Array.from({ length: Number(count) }, (_, index) => ((index * 7919 + 13) % 1009) / 10 + 0.1);
        const sequential = (values) => values.reduce((total, value) => total + value, 0);
        const chunked = (values) => {
          const length = values.length;
          if (length === 0) return 0;
          const chunks = Math.min(1024, Math.ceil(length / 4096));
          const start = (chunk) => Math.floor(length / chunks) * chunk + Math.floor(((length % chunks) * chunk) / chunks);
          let total = 0;
          for (let chunk = 0; chunk < chunks; chunk++) total += values.slice(start(chunk), start(chunk + 1)).reduce((part, value) => part + value, 0);
          return total;
        };
        return [
          ...["map_equal", "map_ref_equal", "init_equal", "for_each_chunk_equal", "sum_equal"].flatMap((name) => sizes.map((count) => [name, [count], 1n])),
          ...sizes.map((count) => ["float_sum_equal", [count], sequential(decimals(count)) === chunked(decimals(count)) ? 1n : 0n]),
        ];
      })());
    },
    traps: [],
  },
  arena: {
    cases: [
      ["arena_cycle", [], 143n], ["arena_order", [], 42351n], ["arena_foreign", [], 110n],
      ["arena_capture", [], 84n], ["arena_task", [], 42n], ["arena_borrowed_values", [], 8n],
      // Swap-removing 20 and 10 leaves 40, 50, 30 in the first copy, which then takes 7, 8, 9; the
      // second copy's handle is not found there (flags 100 + 10 + 1).
      ["arena_snapshot", [], [40n, 50n, 30n, 7n, 8n, 9n].reduce((digits, value) => digits * 100n + value, 0n) * 10000n + 111n],
      ...[1n, 2n, 3n, 1000n].map((n) => ["arena_ring", [n], (n - 1n) * n * (n + 1n) / 3n]),
      ...[0n, 1n, 50000n].map((n) => ["arena_chain", [n], n * (n - 1n) / 2n]),
      ...[0n, 1n, 1000n].map((count) => ["arena_parallel", [count], 2n * count * (4n * count - 1n)]),
      ...[0n, 1n, 2n, 3n, 100n, 1000n].map((count) => {
        let kept = 0n, removed = 0n, stale = 0n;
        for (let i = 0n; i < count; i++) if (i % 3n === 0n) { removed += i * 3n; stale++; } else kept += i * 3n;
        let reinserted = 0n;
        for (let k = 0n; k < stale; k++) reinserted += 1000000n + k;
        return ["arena_churn", [count], (kept + reinserted) * 3n + removed * 5n + stale * 7n + count * 11n];
      }),
      ...[0n, 1n, 10n, 10000n].map((count) => {
        let live = 0n, removed = 0n;
        for (let i = 0n; i < count; i++) {
          const length = BigInt(String(i).length);
          if (i % 4n === 1n) removed += length; else live += length + (i % 4n === 2n ? 1n : 0n);
        }
        return ["arena_strings", [count], live * 1000n + removed];
      }),
    ],
    traps: [["arena_at_removed", []], ["arena_at_foreign", []], ["arena_update_stale", []], ["arena_negative_capacity", []]],
    inspect(ir) {
      const lookups = [...ir.matchAll(/^define internal [^\n]*@tz\.fn\.Arena\.(?:position|contains|get|at)[^\n]*\{([\s\S]*?)^\}/gm)];
      assert.ok(lookups.length > 0);
      for (const [, body] of lookups) assert.doesNotMatch(body, /@tz\.(?:alloc|realloc)\(/);
      assert.equal(ir.match(/^@tz\.arena\.next_id = internal global i64 0/gm)?.length, 1);
      assert.equal(ir.match(/^define internal i64 @tz\.builtin\.Arena\.__next_id\(\)/gm)?.length, 1);
      assert.match(ir, /atomicrmw add ptr @tz\.arena\.next_id, i64 1 monotonic/);
    },
  },
  rc: {
    cases: [
      ["rc_counts", [], (3n * 10n + 2n) * 10000n + (1n * 10n + 1n) * 100n + 2n],
      ["rc_try_unwrap", [], 2n * 100n + BigInt("unwrap".length)],
      ["arc_try_unwrap", [], 2n * 100n + BigInt("unwrap".length)],
      ["rc_upgrade_after_drop", [], 2n + 10n], ["arc_upgrade_after_drop", [], 2n + 10n],
      ["rc_shared_tails", [], (100n + 55n) + (200n + 55n) + 3n * 1000n],
      ["rc_dag", [], (1n + (2n + 4n) + (3n + 4n)) * 10n + 3n],
      ["rc_ptr_eq", [], 10n], ["rc_borrowed", [], BigInt("borrowed".length)], ["arc_capture", [], 2n * 10n + 2n],
      ["shared_named_types", [], (2n * 10n + BigInt("four".length)) * 1000000n + (1n * 100n + 3n * 5n) * 1000n + 7n * 11n],
      // Both children reach the root (7) until it drops; then upgrading gives None (-1 + 1).
      ["rc_weak_parent", [], (7n + 7n) * 1000n + 2n * 100n + (2n + 3n) * 10n + 0n],
      ...[0n, 1n, 2n, 100000n].map((n) => ["rc_chain", [n], n * (n - 1n) / 2n]),
      ...[0n, 1n, 100000n].map((n) => ["rc_long_drop", [n], n]),
      ...[0n, 1n, 100000n].map((n) => ["arc_chain", [n], n * (n - 1n) / 2n]),
      ...[0n, 1n, 1000n, 4097n].map((n) => ["arc_parallel", [n], n * (n - 1n) / 2n * 10n + 1n]),
      ...[0n, 1n, 1000n].map((n) => ["arc_weak_parallel", [n], 4n * (n * (n - 1n) / 2n) * 10n]),
      // The last task to finish drops the shared value.
      ...[0n, 1n, 1000n, 100000n].map((n) => ["arc_parallel_last", [n], n * (n - 1n) / 2n]),
      ...[0n, 1n, 1000n, 50000n].map((n) => ["arc_chain_parallel", [n], 4n * (n * (n - 1n) / 2n)]),
    ],
    // A chain of a million shared nodes drops without native recursion; on wasm32 it needs more
    // than the default 16 MiB heap, so it traps there at the allocation instead.
    nativeCases: [["rc_chain", [1000000n], 1000000n * 999999n / 2n], ["rc_long_drop", [1000000n], 1000000n],
      ["arc_chain", [1000000n], 1000000n * 999999n / 2n]],
    wasmTraps: [["rc_chain", [1000000n]], ["arc_chain", [1000000n]]],
    traps: [["rc_upgrade_dead", []]],
    inspect(ir) {
      const bodies = (prefix) => [...ir.matchAll(new RegExp(`^define internal [^\\n]*@tz\\.builtin\\.${prefix}\\.[^\\n]*\\{([\\s\\S]*?)^\\}`, "gm"))].map((match) => match[1]);
      const rc = bodies("Rc"), arc = bodies("Arc");
      assert.ok(rc.length > 0 && arc.length > 0);
      for (const body of rc) assert.doesNotMatch(body, /atomicrmw|cmpxchg|fence|load atomic/);
      assert.ok(arc.some((body) => /atomicrmw add ptr [^\n]*, i64 1 monotonic/.test(body)));
      assert.ok(arc.some((body) => /cmpxchg ptr [^\n]* acquire monotonic/.test(body)));
      assert.match(ir, /atomicrmw sub ptr [^\n]*, i64 1 release/);
      assert.match(ir, /fence acquire/);
      assert.match(ir, /^define internal void @"tz\.shared\.drop\.rc\.Main\.Link"\(ptr %node, ptr %pending\)/m);
      assert.match(ir, /^define internal void @"tz\.shared\.drop\.arc\.Main\.AList"\(ptr %node, ptr %pending\)/m);
    },
  },
  hash_map: {
    cases: [
      ...[0n, 1n, 2n, 7n, 8n, 9n, 100n, 1000n, 100000n].flatMap((count) => [1n, 2n, -3n].flatMap((seed) => [
        ["hash_ops", [count, seed], hashOps(count, seed, false)],
        ["hash_set_ops", [count, seed], hashOps(count, seed, true)],
      ])),
      // A seed changes only the table layout: results and iteration order match the unseeded map.
      ...[0n, 1n, 9n, 100n, 1000n].flatMap((count) => [0n, 5n, -2n].flatMap((tableSeed) => [
        ["hash_seeded_ops", [count, 1n, tableSeed], hashOps(count, 1n, false)],
        ["hash_seeded_capacity_ops", [count, 2n, tableSeed], hashOps(count, 2n, false)],
        ["hash_set_seeded_ops", [count, 1n, tableSeed], hashOps(count, 1n, true)],
        ["hash_set_seeded_capacity_ops", [count, -3n, tableSeed], hashOps(count, -3n, true)],
      ])),
      ...sip13Inputs().map(([key0, key1, message]) => ["hash_sip13", [key0, key1, message], sip13Word(key0, key1, message)]),
      ...[0n, 1n, 7n, 100n, 1000n].flatMap((count) => [
        ["hash_borrowed_keys", [count], hashBorrowedKeys(count)],
        ["hash_set_borrowed_keys", [count], hashSetBorrowedKeys(count)],
      ]),
      ...[0n, 1n, -1n, 123456789n].map((tableSeed) => ["hash_flood", [tableSeed], 1n]),
      ["hash_empty_probe", [], 0n],
      ...[0n, 1n, 9n, 12n, 20n, 257n].map((count) => ["hash_strings", [count], hashStrings(count)]),
      ...[0n, 1n, 3n, 40n, 100n, 1000n].map((count) => ["hash_collisions", [count], hashCollisions(count)]),
      ...[0n, 1n, 7n, 8n, 9n, 1000n].map((count) => ["hash_capacity", [count], count]),
      ["hash_iter_order", [], 58732n], ["first_representative", [], 1], ["owned_record_keys", [], 42n],
      ["hash_capture", [], 84n], ["hash_borrowed_values", [], 8n], ["hash_task_drop", [], 0n],
    ],
    traps: [["hash_at_missing", []], ["hash_nan_insert", []], ["hash_nan_query_empty", []], ["hash_set_nan", []],
      ["hash_capacity_negative", []], ["hash_capacity_huge", []]],
    inspect(ir) {
      const lookups = [...ir.matchAll(/^define internal [^\n]*@tz\.fn\.HashMap\.(?:mix|sip13|sip_round|hash_of|probe|find|contains_key|get|at)[^\n]*\{([\s\S]*?)^\}/gm)];
      assert.ok(lookups.length > 0);
      for (const [, body] of lookups) assert.doesNotMatch(body, /@tz\.(?:alloc|realloc)\(/);
    },
  },
  string_interpolation: {
    cases: [
      ["basic_holes", [], 34n], ["borrowed_holes", [], 15n], ["evaluation_order", [], 123003n], ["utf8_holes", [], 11n],
      ["nested_literals", [], 1], ["display_instance", [], 7n],
      ...[-5n, 0n, 41n].map((n) => ["temporaries", [n], interpolationDigest(`${n + 1n}|abc|42`)]),
      ...[0n, 1n, 1000n].map((count) => ["owned_loop", [count], interpolationOwnedLoop(count)]),
      ...interpolationIntegers.map((v) => ["spec_integer", [v], interpolationDigest(integerInterpolation(v))]),
      ...interpolationIntegers.map((v) => ["spec_unsigned", [v], interpolationDigest(
        [BigInt.asUintN(64, v).toString(16), `+${BigInt.asUintN(64, v)}`, BigInt.asUintN(64, v).toString(2),
          interpolationPad(BigInt.asUintN(64, v).toString(), 22, "center")].join("|"))]),
      ...[min, -255n, -129n, -128n, -1n, 0n, 1n, 127n, 128n, 255n, 65535n, 65536n, 70000n, max].map((v) => {
        const narrow = BigInt.asIntN(8, v), wide = BigInt.asUintN(16, v), sign = narrow < 0n ? "-" : "", magnitude = narrow < 0n ? -narrow : narrow;
        return ["spec_narrow", [v], interpolationDigest(`${sign}${magnitude.toString(16)}|${sign}${magnitude.toString(2)}|${wide.toString(16).toUpperCase()}|${wide.toString(8)}`)];
      }),
      ["spec_i128_min", [], interpolationDigest(`-${(1n << 127n).toString(16)}|-${(1n << 127n)}|-${(1n << 127n).toString(2)}`)],
      ...interpolationDoubles.map((x) => ["spec_f64", [x], interpolationDigest(floatInterpolation(doubleValue(x)))]),
      ...interpolationSingles.map((x) => ["spec_f32", [x], interpolationDigest(floatInterpolation(doubleValue(Math.fround(x))))]),
      ...[0n, 1n, 2n, 3n, 4n].map((k) => ["spec_wide", [k], interpolationDigest(wideInterpolation(Number(k)))]),
      ["spec_padding", [], interpolationDigest([
        interpolationPad("é😀", 5, "right", "*"), interpolationPad("\uD800", 3, "left"), interpolationPad("ab", 6, "center"),
        interpolationPad("lab", 8, "right"), interpolationPad("true", 6, "left"), interpolationPad("x", 3, "center"),
      ].join("|"))],
      ["spec_utf8_padding", [], 31n],
      ["format_instances", [], interpolationDigest(formatInstancesReference())],
      ["format_instances_utf8", [], 17n], ["format_keeps_owners", [], 11n],
      ["display_case_holes", [], interpolationDigest("QuietLoud")],
      ["format_parse", [], interpolationDigest(formatParseSpecs.map((spec) => `${describeFormatSpec(spec)}|`).join(""))],
      ["format_pad", [], interpolationDigest(formatPadReference())],
    ],
    traps: [["trap_utf8_surrogate", []], ["trap_display", []]],
    inspect(ir) {
      assert.match(ir, /@tz_soft_format_spec/);
      assert.match(ir, /@tz\.format\./);
      assert.doesNotMatch(ir, /@printf|@snprintf|@strtod|@strtof/);
    },
  },
  interpolation_temporaries: {
    cases: (() => {
      const ns = [-5n, 0n, 41n], table = [10n, 20n, 30n];
      const digests = (name, text) => ns.map((n) => [name, [n], interpolationDigest(text(n))]);
      return [
        ...digests("length_holes", (n) => `3|3|3|3|2|${n}`),
        ...digests("other_lengths", () => "5|6"),
        ...digests("string_index", (n) => `97|99|${n}`),
        ...digests("field_of_call", (n) => `${n}|seven|5`),
        ...digests("index_of_call", (n) => `${n + 1n}|3|2`),
        ...digests("property_of_place", (n) => `5|${n}`),
        ...digests("property_specs", () => `${interpolationPad("4", 5, "right")}|11|+17|10001|${interpolationPad("4", 7, "center", "*")}`),
        ...[0n, 1n, 2n].map((i) => ["constant_parts", [i], interpolationDigest(`20|3|3|5|origin|${table[Number(i)]}`)]),
        ...digests("snapshot_holes", (n) => `${n}${n + 1n}${n + 1n}|379`),
        ...digests("drop_temporary", (n) => `${n}`),
        ["evaluation_order", [], 123003n], ["utf8_holes", [], 4n],
        ["nested_holes", [], interpolationDigest("<[3]>")],
        ...digests("captured_holes", (n) => `3:${n}`),
        ...[0n, 1n, 1000n].map((count) => ["owned_loop", [count], (() => {
          let total = 0n;
          for (let index = 0n; index < count; index++) total += BigInt(`${index}:2:seven`.length);
          return total;
        })()]),
      ];
    })(),
    inspect(ir) {
      assert.doesNotMatch(ir, /@printf|@snprintf|@strtod|@strtof/);
    },
  },
  hierarchical: {
    cases: [
      ["distance_sum", [3n, 4n, 5n, 12n], 18n],
      ["distance_sum", [0n, 0n, 8n, 15n], 17n],
      ["distance_sum", [-3n, -4n, 20n, 21n], 34n],
    ],
  },
  constants: {
    cases: [
      ["integer_values", [], 42n], ["owned_values", [], 42n],
      ["borrowed_values", [], 8n], ["float_values", [], 1],
      ["repeat_values", [100000n], 600000n], ["captured_values", [], 42n],
      ["wide_values", [], 1], ["float_reference", [1.0009765625, 0.00048828125], 1],
      // The runtime rule for a NaN result of f16 and f128 arithmetic (tz_soft_op in numeric.c): `+` takes the
      // sign of the left operand, `*` and `/` the xor of both signs. A constant must agree with the program.
      ["nan_signs", [], (() => {
        const sum = 1, product = 1 ^ 0, quotient = 1 ^ 0, flipped = sum ^ 1;
        const signs = sum + 2 * product + 4 * quotient + 8 * flipped + 16 * sum + 32 * flipped;
        return BigInt(signs * 100 + signs);
      })()],
    ],
  },
  // D11 Phase 1: every expected value is computed here, not copied from the compiler.
  const_functions: {
    cases: (() => {
      const crcTable = Array.from({ length: 256 }, (_, index) => {
        let value = index;
        for (let bit = 0; bit < 8; bit++) value = value & 1 ? (value >>> 1) ^ 0xEDB88320 : value >>> 1;
        return value >>> 0;
      });
      let crc = 0xFFFFFFFF;
      for (const byte of Buffer.from("123456789")) crc = crcTable[(crc ^ byte) & 0xFF] ^ (crc >>> 8);
      crc = ~crc >>> 0;
      let previous = 0n, current = 1n;
      for (let step = 1; step <= 90; step++) [previous, current] = [current, previous + current];
      let collatz = 0, n = 27;
      while (n !== 1) { n = n % 2 === 0 ? n / 2 : 3 * n + 1; collatz++; }
      const classify = (value) => value === 0 ? 10 : value === 1 ? 20 : value * 2;
      const classified = Array.from({ length: 10 }, (_, value) => classify(value)).reduce((a, b) => a + b, 0);
      let hash = 14695981039346656037n;
      for (const byte of Buffer.from("tsuzuri")) hash = ((hash ^ BigInt(byte)) * 1099511628211n) & ((1n << 64n) - 1n);
      const units = Array.from("日本語ab", (character) => character.charCodeAt(0));
      const unitSum = units.reduce((a, b) => a + b, 0) + units.length * 1000 + units[1];
      let root = 1000000;
      for (;;) { const next = Math.floor((root + Math.floor(1000000 / root)) / 2); if (next >= root) break; root = next; }
      const triangular = (100 * 101) / 2;
      let fibonacci20 = [0, 1];
      for (let step = 2; step <= 20; step++) fibonacci20.push(fibonacci20[step - 1] + fibonacci20[step - 2]);
      const behind = collatz * 2 + fibonacci20[20] + triangular + root;
      return [
        ["crc_first", [], BigInt(crcTable[1])], ["crc_last", [], BigInt(crcTable[255])], ["crc_whole_table", [], 1],
        ["crc_check", [], BigInt(crc)], ["fib_rec", [], BigInt(fibonacci20[20])], ["fib_iter_90", [], previous],
        ["collatz_27", [], BigInt(collatz)], ["wrapping", [], BigInt.asIntN(8, 127n + 1n)],
        ["min_max", [], BigInt(Math.max(5, -3, 9, 0) * 100 + Math.min(5, -3, 9, 0))],
        ["classify_sum", [], BigInt(classified)], ["poly_value", [], 1.7 * 1.7 * 0.1 + 1.7 / 3],
        ["fnv_hash", [], BigInt.asIntN(64, hash)], ["unit_sum", [], BigInt(unitSum)],
        ["moved_point", [], 3n * 100n + (4n + 10n)], ["areas_total", [], 0n * 10000n + 49n * 100n + 15n],
        ["early_exits", [], 2n * 10000n + 9n * 100n + 25n], ["fixed_array", [], 30n],
        ["logic", [], 1n * 100n + 0n * 10n + 3n + 0n], ["behind_const", [], BigInt(behind)],
      ];
    })(),
    inspect(ir) {
      // The constants are literals in the IR: the Fibonacci number is not computed at run time.
      assert.match(ir, /\b6765\b/);
      assert.match(ir, /\b1996959894\b/);
    },
  },
  // D11 Phase 2: every expected value is computed here, not copied from the compiler.
  const_statics: {
    cases: (() => {
      const crcTable = Array.from({ length: 256 }, (_, index) => {
        let value = index;
        for (let bit = 0; bit < 8; bit++) value = value & 1 ? (value >>> 1) ^ 0xEDB88320 : value >>> 1;
        return value >>> 0;
      });
      const crc = (count) => {
        let value = 0xFFFFFFFF;
        for (let i = 0; i < count; i++) value = crcTable[(value ^ ((i * 7 + 3) & 255)) & 0xFF] ^ (value >>> 8);
        return BigInt(~value >>> 0);
      };
      const primes = [2n, 3n, 5n, 7n, 11n, 13n];
      const sum = (values) => values.reduce((a, b) => a + b, 0n);
      const squaresSum = (count) => sum(Array.from({ length: count }, (_, i) => BigInt(i) * BigInt(i)));
      const bytes = [1n, 2n, 255n, 0n];
      const wide = [1n, (1n << 128n) - 1n];
      const wideAt = (index) => BigInt.asIntN(64, (wide[index] >> 100n) + BigInt.asIntN(64, wide[index]));
      const hot = (count) => {
        let total = 0n;
        for (let i = 0; i < count; i++) total += primes[i % 6] + BigInt((i & 4095) ** 2) + bytes[i & 3];
        return total;
      };
      const floatTotal = ((((0.1 + 1 / 3) + 1.5) + Math.fround(0.1)) + -2.5) + 1.5;
      return [
        ...[0n, 1n, 9n, 255n, 1000n].map((count) => ["crc_buffer", [count], crc(Number(count))]),
        ...[0n, 1n, 100n, 2047n, 4095n].map((index) => ["squares_at", [index], index * index]),
        ...[0n, 1n, 10n, 4096n].map((count) => ["squares_sum", [count], squaresSum(Number(count))]),
        ...[0n, 3n, 4096n].map((count) => ["squares_window", [count], squaresSum(Number(count))]),
        ["squares_length", [], 3n * 4096n], ["primes_total", [], 41n * 3n + 2n],
        ...[0n, 1n, 2n, 3n].map((index) => ["grid_at", [index], [10n, 20n, 30n, 40n][Number(index)] + 4n]),
        ...[0n, 1n, 2n, 3n].map((index) => ["bytes_at", [index], bytes[Number(index)]]),
        ...[0n, 1n].map((index) => ["wide_at", [index], wideAt(Number(index))]),
        ["float_total", [], floatTotal], ["float_signs", [], 1111n],
        ...[0n, 1n, 2n].map((index) => ["flags_letters", [index], [111n, 100n, 111n][Number(index)]]),
        ["empty_single", [], 142n], ...[0n, 1n].map((index) => ["points_at", [index], [3n, 1n][Number(index)]]),
        ["corner_total", [], 16n], ...[0n, 1n, 2n].map((index) => ["readings_at", [index], [0n, 7n, 90n][Number(index)]]),
        ["favorite_value", [], 50n], ["escaped_references", [], 13n + 5n + 7n + 4n + 5n],
        ["closures_and_tasks", [], 9n + 41n * 3n + 41n + 103n],
        ["owned_copies", [], squaresSum(4096) + 13n + 2n + 26n + 12n + 3n + 9n + 11n],
        ["values_stay_values", [], 84n], ["strings_stay_values", [], 17n], ["scalar_borrow", [], 45n],
        ...[0n, 1n, 10n, 5000n].map((count) => ["hot_loop", [count], hot(Number(count))]),
      ];
    })(),
    traps: [
      ["squares_at", [4096n]], ["squares_at", [-1n]], ["squares_sum", [4097n]], ["grid_at", [4n]], ["grid_at", [-1n]],
      ["bytes_at", [4n]], ["wide_at", [2n]], ["flags_letters", [3n]], ["points_at", [2n]], ["readings_at", [3n]],
    ],
    trapKind: "index out of bounds",
    inspect(ir) {
      const definitions = [...ir.matchAll(/^(@tz\.static\.\d+(?:\.data)?) = ([^\n]*)$/gm)];
      assert.ok(definitions.length >= 12, "the plain constants read in place are static data");
      assert.equal(new Set(definitions.map(([, name]) => name)).size, definitions.length, "each static is defined once");
      for (const [, name, rest] of definitions) assert.match(rest, /^private unnamed_addr constant /, `${name} is private and constant`);
      assert.match(ir, /^@tz\.static\.\d+\.data = private unnamed_addr constant \[4096 x i64\] \[i64 0, i64 1, i64 4, i64 9,/m);
      assert.match(ir, /^@tz\.static\.\d+\.data = private unnamed_addr constant \[256 x i32\] \[i32 0, i32 1996959894,/m);
      // A reference never stores into the data, and the data of an array is named only by the headers
      // that lend it, so no allocator call can receive it (the harness's tracked_free also rejects a
      // pointer that it did not allocate, and the ASan variant runs this suite too).
      assert.doesNotMatch(ir, /^\s*(?:store|atomicrmw|cmpxchg) [^\n]*, ptr @tz\.static\./m);
      const uses = ir.split("\n").filter((line) => !line.startsWith("@tz.static.")).join("\n");
      assert.equal((uses.match(/@tz\.static\.\d+\.data/g) ?? []).length,
        (uses.match(/\{ ptr @tz\.static\.\d+\.data, i64 \d+ \}/g) ?? []).length, "a static array is named only by its header");
      // A table read in a loop is not rebuilt: the loop of the export holds no allocation.
      const hot = ir.match(/^define internal i64 @tz\.fn\.Main\.hot_loop\([^\n]*\{([\s\S]*?)^\}/m);
      assert.ok(hot, "hot_loop is emitted");
      assert.doesNotMatch(hot[1], /@tz\.(?:alloc|realloc|free)\(|alloca \[/);
      // The strings and arrays of strings are not plain data: they keep their copies.
      assert.match(ir, /@tz\.literal\./);
    },
  },
  exceptions: {
    cases: (() => {
      const wrap = (value, bits = 64) => BigInt.asIntN(bits, value);
      const overflow = "Arithmetic operation resulted in an overflow.".length;
      const bits = (value, shift) => {
        const amount = shift & 63n;
        return wrap((((value & 0xffn) | (value ^ shift)) + ~value + wrap(value << amount) + (value >> amount) +
          wrap(BigInt.asUintN(64, value) >> amount)));
      };
      const pipeline = (count) => {
        const values = Array.from({ length: Number(count) }, (_, index) => BigInt(index));
        const total = values.reduce((sum, value) => sum + value * 2n, 0n);
        const folded = values.reduce((state, value) => state * 3n + value, 0n);
        const backward = values.reduceRight((state, value) => state * 3n + value, 0n);
        return total * 1000000n + total + folded % 1000n + backward % 7n;
      };
      const big = (high, low) => (high * 10n ** 20n + low) % 1000000007n;
      const divide = (left, right) => (left / right) * 1000n + left % right;
      return [
        ["checked_add", [1n, 2n], 3n], ["checked_add", [max, 1n], -1n], ["checked_add", [min, -1n], -1n],
        ["checked_add", [max, 0n], max], ["checked_add", [-5n, 3n], -2n],
        ["checked_mul_i32", [46340n, 46340n], 2147395600n], ["checked_mul_i32", [46341n, 46341n], -1n],
        ["checked_mul_i32", [-2147483648n, -1n], -1n], ["checked_mul_i32", [65536n, -32768n], -2147483648n],
        ["checked_neg", [5n], -5n], ["checked_neg", [min], -1n], ["checked_neg", [max], -max],
        ["checked_power", [3n, 39n], 3n ** 39n], ["checked_power", [3n, 40n], -1n], ["checked_power", [-2n, 63n], min],
        ["checked_power", [2n, 63n], -1n], ["checked_power", [2n, 0n], 1n], ["checked_power", [0n, 0n], 1n],
        ["checked_power", [-1n, 1000000n], 1n],
        ["checked_power_u8", [2n, 7n], 128n], ["checked_power_u8", [2n, 8n], -1n], ["checked_power_u8", [3n, 5n], 243n],
        ["checked_power_u8", [16n, 2n], -1n], ["checked_power_u8", [15n, 2n], 225n],
        ["power_wrap", [3n, 40n], wrap(3n ** 40n)], ["power_wrap", [2n, 64n], 0n], ["power_wrap", [-3n, 3n], -27n],
        ["power_wrap", [7n, 0n], 1n],
        ["power_i16", [2n, 15n], -32768n], ["power_i16", [3n, 11n], wrap(3n ** 11n, 16)], ["power_i16", [-2n, 3n], -8n],
        ["float_power", [2, 10], 1024], ["float_power", [2, 0.5], Math.SQRT2], ["float_power", [9, 0.5], 3],
        ["float_power", [0, 0], 1], ["float_power_f32", [2, 0.5], Math.fround(Math.SQRT2)], ["float_power_f32", [3, 2], 9],
        ["nested_finally", [3n], 911n], ["nested_finally", [4000000000n], -89n], ["nested_finally", [-4000000000n], -189n],
        ["finally_raises", [1n], 7n], ["finally_raises", [2n], -2n], ["finally_raises", [-2n], 7n],
        ["finally_raises", [-3n], -2n], ["finally_raises", [4000000000n], -2n], ["finally_raises", [-4000000000n], -2n],
        ["loop_strings", [5n], 17000n], ["loop_strings", [12n], 28004n],
        ["uncaught", [1n], 2n],
        ["checked_message", [5n], 6n], ["checked_message", [2147483647n], BigInt(overflow)],
        ["bigint_digits", [0n], 1n], ["bigint_digits", [100n], BigInt(String(2n ** 100n).length)],
        ["bigint_digits", [1000n], BigInt(String(2n ** 1000n).length)],
        ["bigint_mod", [123456789n, 987654321n], big(123456789n, 987654321n)], ["bigint_mod", [-5n, 3n], big(-5n, 3n)],
        ["bigint_division", [17n, 5n], divide(17n, 5n)], ["bigint_division", [-17n, 5n], divide(-17n, 5n)],
        ["bigint_division", [17n, -5n], divide(17n, -5n)], ["bigint_division", [-17n, -5n], divide(-17n, -5n)],
        ["bigint_compare", [1n, 2n], -1n], ["bigint_compare", [2n, 2n], 0n], ["bigint_compare", [3n, 2n], 1n],
        ["compose", [100n], 202201n], ["compose", [-3n], -4005n],
        ["logic", [1n, 1n], 0n], ["logic", [1n, 0n], 110n], ["logic", [0n, 1n], 101n], ["logic", [0n, 0n], 111n],
        ["bits", [-1000n, 3n], bits(-1000n, 3n)], ["bits", [123456789n, 70n], bits(123456789n, 70n)],
        ["suffixes", [], 6000000931n], ["bytes", [], 265104105n], ["default_literals", [], -2147483644n],
        ["pipeline", [10n], pipeline(10n)], ["pipeline", [1n], pipeline(1n)],
      ];
    })(),
    traps: [["uncaught", [max]], ["power_wrap", [2n, -1n]], ["power_i16", [2n, -1n]], ["checked_power", [2n, -1n]]],
  },
  deriving: {
    cases: [
      ["record_flags", [0n, 1n], 14n], ["record_flags", [1n, 0n], 50n], ["record_flags", [1n, 1n], 50n],
      ["defaults", [], 42n], ["case_order", [], 141414n],
      ["floating", [0n], 2n], ["floating", [1n], 41n], ["floating", [2n], 2n],
      ["owned_words", [], 42n], ["recursive_compare", [], 42n],
    ],
  },
  typeclasses: {
    cases: [
      ...["array_mask", "list_mask", "tuple_mask"].flatMap((name) => [-1n, 0n, 1n].flatMap((left) => [-1n, 0n, 1n].map((right) => [name, [left, right], left === right ? 41n : left < right ? 14n : 50n]))),
      ["prefixes", [], 501414n], ["defaults", [], 42n], ["noncopy_elements", [], 39n],
      ["nested_collections", [], 1414n], ["floating_masks", [0n], 2n], ["floating_masks", [1n], 41n], ["floating_masks", [2n], 2n],
      ["long_lists", [0n], 0n], ["long_lists", [100000n], 100000n], ["staged_comparison", [], 42n],
      ["optional_values", [], 42n],
      ["short_circuits", [], 42n],
    ],
    inspect(ir) {
      const helpers = [...ir.matchAll(/^define [^\n]*@tz\.fn\.\$intrinsic\.(?:Eq|Ord)\.[^\n]*\{([\s\S]*?)^\}/gm)];
      const lists = helpers.map((match) => match[1]).filter((body) => /phi ptr \[[^\n]+\], \[/.test(body));
      assert.ok(lists.length >= 6, "list comparison helpers are present");
      for (const body of lists) {
        assert.equal((body.match(/phi ptr \[[^\n]+\], \[/g) ?? []).length, 4, "each list cursor is a chunk, left for the next one by a branch, and both advance by runs in lockstep");
        assert.ok(!body.includes("@tz.alloc") && !body.includes("@tz.list.index"));
      }
    },
  },
  list_chunks: (() => {
    const call = (name, n, expected) => [name, [BigInt(n)], expected];
    const digits = (value) => BigInt(String(value).length);
    // i64 elements: the first chunk has 1 slot (32 bytes with the header) and the largest 4093 (32 KiB).
    const numbers = chunkLengths(1, 4093, 15000).flatMap((n) => {
      const down = descending(n);
      const half = Math.floor(n / 2);
      const refilled = down.slice(half);
      for (let i = 1; i <= half; ++i) refilled.unshift(1000000n + BigInt(i));
      const grown = [...Array.from({ length: 30 }, (_, i) => BigInt(30 - i)), ...Array.from({ length: n }, (_, i) => BigInt(i + 100))].slice(10);
      const rest = down.slice(3);
      const head = (index) => down[index] ?? 0n;
      const pattern = n >= 3 ? head(0) + head(1) * 10n + head(2) * 100n + weightedSum(rest) * 1000n
        : n === 2 ? head(0) + head(1) * 10n : n === 1 ? head(0) : 0n;
      return [
        call("cons_walk", n, weightedSum(down)), call("cons_length", n, BigInt(n)), call("index_walk", n, weightedSum(down)),
        call("tail_walk", n, weightedSum(down)), call("refill", n, weightedSum(refilled) * 1000n + BigInt(refilled.length)),
        call("compare_prefix", n, 14n), call("compare_suffix", n, 50n), call("compare_copy", n, 41n),
        call("compare_change", n, n === 0 ? 41n : 14n),
        call("copy_weighted", n, 4n * weightedSum(down)),
        call("map_weighted", n, weightedSum(down.map((value) => value * 2n + 1n))),
        call("reverse_weighted", n, weightedSum(ascending(n))), call("to_array_weighted", n, weightedSum(down)),
        call("from_array_weighted", n, weightedSum([5n, ...ascending(n).map((value) => value + 7n)]) * 10n + weightedSum(ascending(n).map((value) => value + 7n))),
        call("bulk_grow", n, weightedSum(grown) * 100n + BigInt(grown.length)),
        call("pattern_rest", n, pattern),
        call("pattern_exact", n, n === 2 ? 10n : n === 1 ? 5n : n === 0 ? 7n : 99n),
        call("display_length", n, n === 0 ? 4n : 4n + down.reduce((total, value) => total + digits(value), 0n) + 2n * BigInt(n - 1)),
        call("hash_layout", n, n === 0 ? 1n : 11n),
        call("stack_cons", n, BigInt(n - 1) + BigInt(n) * 2n + BigInt(n + 1) * 3n + BigInt(n + 2) * 4n),
        call("stack_tail", n, BigInt(n + 1) + BigInt(n + 2) * 2n),
        call("stack_escape", n, 1n + BigInt(n) * 2n + BigInt(n + 1) * 3n + BigInt(n + 2) * 4n + (BigInt(n + 1) + BigInt(n + 2) * 2n + BigInt(n + 3) * 3n) * 100n),
      ];
    });
    // Strings, lists and 16-byte integers have 16-byte slots (a chunk holds at most 2046 of them); () has no size.
    const owned = chunkLengths(1, 2046, 15000).flatMap((n) => {
      const half = Math.floor(n / 2);
      const lengths = Array.from({ length: n }, (_, position) => BigInt(((n - 1 - position) % 5) + 1));
      const view = n >= 2 ? lengths[0] + lengths[1] * 10n + weightedSum(lengths.slice(2)) * 100n
        : n === 1 ? lengths[0] : 0n;
      const down = descending(n);
      const nested = down.map((value) => (value + (value + 1n) * 2n));
      // The pair at position p (from 1) is (n - p, (n - p) % 100); the second element weighs 1000.
      const pairs = down.map((value) => value + (value % 100n) * 1000n);
      return [
        call("words_walk", n, weightedSum(lengths)), call("words_index", n, weightedSum(lengths)),
        call("words_tail", n, weightedSum(lengths.slice(half)) * 10n + BigInt(n - half)),
        call("words_map", n, 3n * weightedSum(lengths)), call("words_drop", n, BigInt(n)), call("words_view", n, view),
        call("wide_walk", n, weightedSum(down)), call("wide_index_tail", n, BigInt(n) * BigInt(n - 1) / 2n),
        call("nested_walk", n, weightedSum(nested)), call("nested_tail", n, BigInt(n - Math.floor(n / 3))),
        // Copies and conversions: bytes for the elements that own nothing, clones for lists of lists.
        call("wide_copy", n, weightedSum(down) + 1000000n), call("nested_copy", n, weightedSum(nested) * 2n + 100000000000n),
        call("pairs_copy", n, weightedSum(pairs) + 1000000000000n), call("words_escape", n, 2032n + BigInt(n)),
      ];
    });
    const units = chunkLengths(8, 32744, 15000).flatMap((n) => [
      call("units_walk", n, BigInt(n) * 1000003n + BigInt(n - Math.floor(n / 2))), call("units_copy", n, BigInt(n) * 1000011n),
    ]);
    const deep = (n) => call("deep_ops", n, 1000000n + weightedSum(descending(n)) % 1000n);
    // An exception that an element of a heap literal raises frees what the literal held so far (the host checks `live == 0`).
    const raises = [
      ...["raise_list_numbers", "raise_list_words", "raise_array_numbers", "raise_array_words"].flatMap((name) =>
        [-1n, 0n, 1n, 2n].map((at) => [name, [at], at < 0n ? (name.endsWith("words") ? 111n : 321n) : -1n - at])),
      // A literal that is complete when a later element of the enclosing expression raises, literals in literals, a try in a loop.
      ...[-1n, 0n, 1n, 2n].flatMap((at) => [
        ["after_literal_list", [at], at === 0n ? -1n : 21n], ["after_literal_array", [at], at === 0n ? -1n : 21n],
        ["nested_literals", [at], at === 0n ? -1n : at === 1n ? -2n : 2n],
      ]),
      ...[[-1n, 8n], [1n, 106n], [3n, 106n]].map(([at, expected]) => ["loop_in_try", [at], expected]),
    ];
    // Elements with a user Drop that folds its id into a digest: the drops happen in list order across the chunks (the elements are 16
    // bytes, so the chunks grow like those of `owned`), List.tail drops the head at once, and no owner of the shared cell is left.
    const mix = (ids) => ids.reduce((digest, id) => BigInt.asIntN(64, digest * 1000003n + BigInt(id)), 0n);
    const tracked = chunkLengths(1, 2046, 15000).flatMap((n) => {
      const ids = Array.from({ length: n }, (_, position) => n - position);
      return [
        call("tracked_drop", n, mix(ids)), call("tracked_tail_dropped", n, mix(ids.slice(0, Math.floor(n / 3)))),
        call("tracked_tail_all", n, mix(ids)), call("tracked_owners", n, 1n),
      ];
    });
    // Indexing a cons-grown list: the 8-byte elements start a new chunk after 1, 3, 7, 14, 25, 42, 68, 108 and 169 elements, the 1-byte
    // elements (one chunk of 8 slots with free slots in front, then 13, 20, ...) after 8, 21, 41, 72 and 119. One past the end, and
    // before the start, trap wherever the free slots are; so do the same indexes after List.tail pops the head.
    const around = (limits) => [...new Set(limits.flatMap((limit) => [limit - 1, limit, limit + 1]).filter((n) => n >= 1))].sort((a, b) => a - b);
    const wide = around([1, 3, 7, 14, 25, 42, 68, 108, 169]);
    const bytes = around([8, 21, 41, 72, 119]);
    const popped = [[5, 1], [5, 2], [7, 3], [8, 1], [14, 6], [15, 7], [26, 10], [43, 20], [69, 33]];
    const reads = [
      ...wide.flatMap((n) => [["index_at", [BigInt(n), 0n], BigInt(n - 1)], ["index_at", [BigInt(n), BigInt(n - 1)], 0n]]),
      ...bytes.flatMap((n) => [["bytes_at", [BigInt(n), 0n], BigInt.asIntN(8, BigInt(n - 1))], ["bytes_at", [BigInt(n), BigInt(n - 1)], 0n]]),
      ...popped.flatMap(([n, pops]) => [["tail_index_at", [BigInt(n), BigInt(pops), 0n], BigInt(n - 1 - pops)], ["tail_index_at", [BigInt(n), BigInt(pops), BigInt(n - 1 - pops)], 0n]]),
    ];
    const outside = [
      ...wide.flatMap((n) => [n, n + 1, -1].map((index) => ["index_at", [BigInt(n), BigInt(index)]])),
      ...bytes.flatMap((n) => [n, n + 1, -1].map((index) => ["bytes_at", [BigInt(n), BigInt(index)]])),
      ...popped.flatMap(([n, pops]) => [n - pops, n - pops + 1, -1].map((index) => ["tail_index_at", [BigInt(n), BigInt(pops), BigInt(index)]])),
    ];
    return {
      cases: [...numbers, ...owned, ...units, ...raises, ...tracked, ...reads],
      traps: outside,
      trapKind: "index out of bounds",
      nativeCases: [deep(250000), deep(1000000)],
      inspect(ir) {
        assert.ok(ir.includes("{ ptr, i64, i64, [0 x i64] }"), "the lists are chunks");
        assert.ok(!ir.includes("getelementptr inbounds { ptr, i64 },"), "no list node is left");
      },
    };
  })(),
  recursive_types: {
    cases: [
      ["deep_drop", [0n], 0n], ["deep_drop", [100000n], 100000n],
      ["deep_clone", [0n], 3n], ["deep_clone", [50000n], 100003n],
      ["subtree_move", [], 41n], ["consuming_walk", [10000n], 50005000n],
      ["rose_clone", [0n], 3n], ["rose_clone", [25000n], 5n],
      ["list_drop", [25000n], 1n], ["record_clone", [10000n], 13n],
      ["generic_container", [], 42n], ["second_empty", [], 42n],
      ["constructor_value", [], 42n], ["task_payload", [], 42n],
      ["vec_clone", [25000n], 5n], ["mutual_clone", [20000n], 43n],
      ["guard_cleanup", [], 42n], ["task_tree", [10000n], 10000n],
      ["constructor_order", [0n], 42123n], ["constructor_order", [1n], 42123n],
      ["list_clone", [25000n], 5n], ["tail_walk", [100000n], 5000050000n],
      ["alternative_cleanup", [], 42n],
    ],
    nativeCases: [["deep_drop", [1000000n], 1000000n], ["deep_clone", [250000n], 500003n]],
    wasmTraps: [["deep_drop", [1000000n]]],
    traps: [],
  },
  // B09: the values follow from the closed forms: triangle numbers, factorials, Euclid's algorithm,
  // the length of a printed number, and the parity of a count. A call of the lifted function from its
  // own tail is a loop, so the depth of 1,000,000 needs no stack in the native or the WASM build.
  local_recursion: {
    cases: [
      ...[0n, 1n, 100n, 1000000n].map((n) => ["triangle", [n], n * (n + 1n) / 2n]),
      ...[0n, 1n, 5n, 20n].map((n) => ["factorial", [n], n < 2n ? 1n : Array.from({ length: Number(n) }, (_, i) => BigInt(i + 1)).reduce((a, b) => a * b)]),
      ...[[48n, 18n, 6n], [1071n, 462n, 21n], [17n, 0n, 17n], [0n, 5n, 5n]].map(([a, b, expected]) => ["gcd", [a, b], expected]),
      ...[[0n, 0n], [1n, 1n], [10n, 55n], [25n, 75025n]].map(([n, expected]) => ["fib", [n], expected]),
      ...[[-1n, 5n], [0n, 5n], [4n, 3n], [5n, 2n], [9n, 0n]].map(([limit, expected]) => ["count_above", [limit], expected]),
      ...[[1n, 0n], [3n, 1n], [5n, 2n], [7n, 3n], [9n, 4n], [11n, 5n], [0n, -1n], [4n, -1n], [12n, -1n]].map(([target, expected]) => ["binary_search", [target], expected]),
      ...[0n, 1n, 1000n, 500000n].map((n) => ["walk", [n], n * (n - 1n) / 2n]),
      ...[[0n, 1n], [7n, 1n], [12345n, 5n], [-12n, 3n], [min, 20n]].map(([n, expected]) => ["digits", [n], expected]),
      ...[0n, 1n, 10n, -4n].map((n) => ["escaped", [n], n > 0n ? 3n * n : 0n]),
      ...[0n, 10n, -6n].map((n) => ["partial", [n], n + 6n]),
      ...[-1n, 0n, 5n, 1000000n].map((n) => ["piped", [n], 7n]),
      ...[0n, 1n, 10n, 100n].map((n) => ["nested", [n], n * (n + 1n) * (n + 2n) / 6n]),
      ...[0n, 1n, 10n, 7n, 1000n, 1001n].map((n) => ["parity", [n], n % 2n === 0n ? 1n : 0n]),
      ...[[0n, 4n], [12345n, 8n], [-7n, 5n]].map(([n, expected]) => ["group_captures", [n], expected]),
      ...[[0n, 2n], [12345n, 10n], [-7n, 4n]].map(([n, expected]) => ["sibling", [n], expected]),
      ...[0n, 1n, 10n, 7n, 1000n].map((n) => ["pair_even", [n], n % 2n === 0n ? 1n : 0n]),
      ...[0n, 1n, 10n, 7n, 1000n].map((n) => ["pair_odd", [n], n % 2n === 1n ? 1n : 0n]),
      ...[[0n, 4n], [3n, 31n], [-1n, 1n]].map(([n, expected]) => ["mapped", [n], expected]),
      // Where a group is called directly, and where it is a function value that a call goes through.
      ...[0n, 5n, 12345n, -12n].map((n) => ["nested_string", [n], BigInt(n.toString().length)]),
      ...[0n, 5n, 12345n, -12n].map((n) => ["nested_record", [n], n + BigInt(n.toString().length)]),
      ...[0n, 5n, -3n].map((n) => ["nested_closure", [n], 1n + n]),
      ...[0n, 12345n, -7n].map((n) => ["group_in_closure", [n], 3n + 2n * n]),
      ...[0n, 1n, 45n, 1000n].map((n) => ["builder_continuation", [n], 2n * n + 1n]),
      ...[0n, 5n, 12345n].map((n) => ["group_in_loop", [n], 3n * BigInt(n.toString().length) + 6n]),
      ...[-5n, 0n, 5n, 12345n].map((n) => ["group_in_branch", [n], n > 0n ? 2n * (1n + BigInt(n.toString().length)) : 0n]),
      // A closure keeps the number that its capture had when it was made.
      ...[0n, 4n, -9n].map((n) => ["reassigned_direct", [n], n + 1n]),
      ...[0n, 4n, -9n].map((n) => ["reassigned_group", [n], n + 1n]),
      ...[0n, 12345n, 54321n].map((n) => ["match_borrowed", [n], n === 12345n ? 1n : 2n]),
      ...[-1n, 0n, 1n, 12345n].map((n) => ["match_maybe", [n], n > 0n ? 1n : 2n]),
      ...[0n, 7n, -3n].map((n) => ["generic_earlier", [n], n]),
      // The members of a group in the body of a task are bound together.
      ...[0n, 1n, 10n, 7n, 1000n].map((n) => ["task_group", [n], n % 2n === 0n ? 1n : 0n]),
      ...[0n, 1n, 10n, 7n, 1000n].map((n) => ["task_group_after_bind", [n], n % 2n === 0n ? 1n : 0n]),
    ],
    inspect(ir) {
      const lifted = ir.split("\ndefine ").filter((body) => /^internal \S+ @tz\.fn\.\$lambda\.\d+\(/.test(body));
      assert.ok(lifted.length >= 30, `the let rec functions are lifted: ${lifted.length}`);
      // A tail call jumps back: a parameter's phi has an incoming value from the back edge.
      const loops = lifted.filter((body) => /= phi [^\n]*\], \[/.test(body));
      assert.ok(loops.length >= 15, `tail calls of lifted functions are loops: ${loops.length}`);
      // A `let rec` function is called directly: no closure is built for the calls in its body. The
      // one loop that holds a closure is the group in `group_in_closure`, which calls an earlier
      // function through its function value.
      const closing = loops.filter((body) => body.includes("%tz.closure"));
      assert.ok(closing.length <= 1, `only a loop that calls a function value uses a closure: ${closing.length}`);
    },
  },
  parallel: {
    cases: [
      ...[0n, 1n, 4095n, 4096n, 4097n, 8192n].map((count) => ["parallel_init_sum", [count], count * (count + 1n) / 2n]),
      ...[4194303n, 4194304n, 4194305n].map((count) => ["parallel_large_length", [count], count]),
      ["parallel_slice", [], 19n], ["parallel_strings", [4097n], 20485n],
      ["parallel_copy_arrays", [4097n], 4097n * 4098n / 2n], ["parallel_copy_functions", [], 22n],
      ["parallel_callback_snapshots", [4097n], 8194n], ["parallel_float_special", [], 1n], ["parallel_order", [], 1203n],
      ["parallel_capture_order", [], 53n],
      ["parallel_wrapping", [], -16n],
    ],
    traps: [["parallel_large_length", [-1n]], ["parallel_trap", []]],
  },
  debug_output: {
    cases: [["debug_answer", [], 42n], ["debug_owned", [0n], 0n], ["debug_owned", [1000n], 5000n]],
    traps: [["debug_display_trap", []]],
  },
  active_patterns: {
    cases: [
      ["active_option", [0], 43n], ["active_option", [1], -1n],
      ["active_owned", [0n], 0n], ["active_owned", [64n], 320n], ["active_owned", [10000n], 50000n],
      ["active_parity", [-1n], 1n], ["active_parity", [0n], 0n], ["active_parity", [1n], 1n], ["active_parity", [14n], 0n],
      ["active_repeated_case", [0], 0n], ["active_repeated_case", [1], 5n],
      ["active_multi_owned", [0n], 0n], ["active_multi_owned", [64n], 320n], ["active_multi_owned", [10000n], 50000n],
      ["active_order", [], 3231n], ["active_conservative", [], 2n], ["active_borrowed", [], 8n],
    ],
    traps: [["active_trap", []]],
  },
  string_library: {
    cases: [
      ["character_units", [], 55357n + 4n + 128512n + 13n + 5n + 4n],
      ["all_unit_transfer", [], 65536n * 65535n / 2n],
      ...[0n, 1n, -1n, -1n, -1n, 4n, -1n, 3n, -1n, 3n].map((expected, which) => ["validate_bytes", [which], expected]),
    ],
    traps: [["trap_decode_end", []], ["trap_decode_continuation", []], ["trap_repeat_negative", []], ["trap_repeat_overflow", []]],
  },
  // D09: the cases and their V8 or hand-written expectations come from tests/regex-cases.mjs.
  regex: {
    get cases() {
      assert.equal(readFileSync(regexCasesPath, "utf8"), regexCasesSource(), "run node tests/regex-cases.mjs --write");
      return (this.computed ??= regexCases());
    },
    inspect(ir) {
      const matching = [...ir.matchAll(/^define internal [^\n]*@tz\.fn\.Regex\.(?:add_thread|search|copy_slots|in_class|holds|consumes)\([^\n]*\{([\s\S]*?)^\}/gm)];
      assert.equal(matching.length, 6);
      for (const [, body] of matching) assert.doesNotMatch(body, /@tz\.(?:alloc|realloc)\(/);
      assert.match(ir, /^@tz\.unicode\.table\.0 = internal unnamed_addr constant/m);
    },
  },
  // D09 Phase 2: the UCD conformance tests (tests/unicode-ucd.mjs) and V8 references from tests/unicode-cases.mjs.
  unicode: {
    get cases() { return (this.computed ??= unicodeCases(unicodeData)); },
    inspect(ir) {
      assert.match(ir, /^@tz\.unicode\.table\.17 = internal unnamed_addr constant/m);
      assert.equal(ir.match(/^define internal i64 @tz\.unicode\.entry\(/gm).length, 1);
    },
  },
  chars: {
    cases: [
      ["all_code_units", [], 65536n], ["character_patterns", [], 42n], ["character_order", [], 1],
      ["parse_characters", [], 1], ["utf8_ascii", [], 1],
      ...[0, 0x7f, 0x80, 0x7ff, 0x800, 0xd7ff, 0xd800, 0xdfff, 0xe000, 0xffff, 0x10000, 0x10ffff, 0x110000, 0xffffffff].map((value) => ["scalar_roundtrip", [value], 1]),
    ],
    traps: [["trap_surrogate", []], ["trap_outside", []]],
    inspect(ir) { assert.match(ir, /switch i16/); assert.match(ir, /switch i32/); },
  },
  vec: {
    cases: [
      ["vec_push_sum", [0n], 0n], ["vec_push_sum", [1n], 0n], ["vec_push_sum", [4096n], 4096n * 4095n / 2n],
      ["vec_pop_order", [0n], 0n], ["vec_pop_order", [4n], 3210n],
      ["vec_growth", [], 408n], ["vec_set_swap", [], 4312n],
      ["vec_strings", [1024n], 5n], ["vec_clone", [], 16n],
      ["vec_get", [-1n], -1n], ["vec_get", [0n], 42n], ["vec_get", [1n], -1n],
      ["vec_capture", [], 44n], ["vec_empty_transfer", [], 0n], ["vec_nested", [], 42n],
    ],
    traps: [["vec_bounds", []], ["vec_negative_capacity", []], ["vec_negative_reserve", []], ["vec_overflow_reserve", []], ["vec_overflow_capacity", []], ["vec_negative_truncate", []]],
  },
  // PM05: a bounded live set in the WASM heap. A temporary that is freed above kept blocks must leave a hole that later rounds
  // reuse: the heap stays under 8 MiB (the 16 MiB heap was full after 100 rounds and failed after 3,000 when only the first
  // hole was looked at). 3600 rounds: 1800 * 299999 + 1800 * 37499 + 3600. The steady churn (a live set of 3 MB, blocks of up to
  // 64 KB replaced at random) runs after it in the same instance, whose largest request was 2.4 MB: it must not grow the heap to
  // its limit around the free blocks that merging leaves (16 MiB before the bound on the holes); its checksum is the independent
  // reference of benchmarks/heap, wrapped to i64.
  heap_churn: {
    wasmMemory: 8 * 1024 * 1024,
    cases: [
      ["churn", [3600n], 1800n * 299999n + 1800n * 37499n + 3600n],
      ["steady_churn", [100n, 10000n, 8000n, 42n], BigInt.asIntN(64, steadyChurnReference(100, 10000, 8000, 42n))],
    ],
  },
  array_bulk: {
    cases: [
      ["array_basics", [], 44n],
      ["borrowed_search", [], 16n],
      ["bulk_callbacks", [], 495n],
      ["bulk_empty", [], 1n],
      ["bulk_short_circuit", [], 1],
      ["bulk_list", [], 448n],
      ["sort_stable_owned", [], [0n, 2n, 4n, 1n, 3n, 5n].reduce((total, ordinal) => total * 10n + ordinal + 6n, 0n)],
      ["sort_float", [], 1],
      ["reductions", [], 1],
      ["first_duplicate", [], 1n],
      ["filtered", [], 135n],
      // List.foldr with a state aligned more strictly than the elements reads each element with
      // the list's node layout.
      ["foldr_union_state", [], (() => {
        const digits = [3n, 4n].reduce((total, value) => total * 10n + value, 0n);
        const lengths = [[1, 2], [3, 4, 5]].reduce((total, row) => total * 10n + BigInt(row.length), 0n);
        const joined = BigInt(["ab", "cde"].join("").length);
        const rendered = BigInt([3, 4].map(String).join(",").length) * 1000n + 2n;
        return digits * 1000000n + lengths * 10000n + joined * 100000000n + rendered;
      })()],
      ["list_to_seqr_aligned", [], [5n, 7n].reduce((total, value) => total * 10n + value, 0n) * 1000n
        + [5n, BigInt("x".length), 7n].reduce((total, value) => total * 10n + value, 0n)],
      ...[0, 1, 2, 3, 17, 128, 1024].map((count) => ["sort_numbers", [BigInt(count)],
        Array.from({ length: count }, (_, index) => BigInt((index * 13 + 7) % 19)).sort((left, right) => left < right ? -1 : left > right ? 1 : 0)
          .reduce((total, value, index) => total + BigInt(index + 1) * value, 0n)]),
    ],
    traps: [["trap_sub", []], ["trap_zip", []]],
  },
  consuming_update: {
    cases: [
      ["update_numbers", [4n, 2n, 42n], 46n],
      ["update_numbers", [1n, 0n, -1n], -1n],
      ["update_snapshot", [], 1131n],
      ["update_callback", [], 13n],
      ["update_owned", [0n], 11n],
      ["update_owned", [4096n], 10n],
      ["update_partial", [], 45n],
      ["update_captured", [], 45n],
      ["update_list", [], 13n],
      ["update_list_snapshot", [], 123n],
      ["swap_same", [], 13n],
    ],
    traps: [["trap_update_index", []], ["trap_swap_index", []], ["trap_empty_tail", []]],
  },
  explicit_copy: {
    cases: [
      ["explicit_copy", [], 14n],
      ...[0n, 1n, 10000n].flatMap((count) => [
        ["copy_sum", [count], count * (count - 1n)],
        ["list_copy", [count], 2n * count * (count - 1n) + count],
      ]),
      ...[0n, 1n, 100n].map((count) => ["nested_copy", [count], count * (count - 1n) / 2n + count]),
    ],
  },
  last_use: {
    // Independent JavaScript references for tests/fixtures/last_use/Main.tz (A17). A result does not
    // depend on whether a read copies or moves; A17 changes the allocation counts, which
    // primitives.mjs checks, and these cases keep the results fixed.
    cases: (() => {
      const checksum = (xs) => xs[0] + xs[1] * 10n + xs[2] * 100n + xs[3] * 1000n;
      const range = (n) => Array.from({ length: Number(n) }, (_, i) => BigInt(i));
      const selfUpdate = (n) => {
        const xs = [1n, 1n, 1n, 1n];
        for (const i of range(n)) xs[Number(i % 4n)] = i * 3n;
        return checksum(xs);
      };
      const recursive = (n) => {
        const xs = [2n, 2n, 2n, 2n];
        for (const i of range(n)) xs[Number(i % 4n)] = i * 5n;
        return checksum(xs);
      };
      const later = (n) => {
        const xs = [1n, 2n, 3n, 4n];
        for (const i of range(n)) xs[Number(i % 4n)] = xs[Number((i + 1n) % 4n)] + i;
        return checksum(xs);
      };
      const consed = (n) => range(n).reverse().reduce((acc, x) => acc * 3n + x, 0n);
      const loops = [1n, 3n, 4n, 7n, 10n, 25n];
      const values = [0n, 1n, 4n, 5n, 150n, -1n];
      return [
        ...loops.flatMap((n) => [
          ["self_update_loop", [n], selfUpdate(n)],
          ["recursive_accumulate", [n], recursive(n)],
          ["loop_invariant", [n], n * (n - 1n) / 2n + 2n * n],
          ["later_argument_reads_it", [n], later(n)],
          ["earlier_argument_reads_it", [n], later(n)],
          ["break_keeps_live", [n], 2n + range(n < 3n ? n : 3n).reduce((acc, i) => acc + i + 10n, 0n)],
          ["list_cons_loop", [n], consed(n)],
        ]),
        ...values.flatMap((n) => [
          ["branch_last_reads", [n], n % 2n === 0n ? 700n + (n + 1n) * 10n + n + 2n : n * 100n + 90n + n + 2n],
          ["build_from_locals", [n], n + (n + 1n) * 10n + n * 1000n + 40000n],
          ["read_after_read", [n], n * 1000n + 99n],
          ["borrow_alias", [n], n * 100n + 9n],
          ["closure_then_use", [n], n + 1000n],
          ["scrutinee_temp", [n], n >= 0n ? 321n * n : 8n],
          ["scrutinee_local_last", [n], n >= 0n ? 5n * n + 2n : 0n],
          ["scrutinee_local_used_after", [n], n >= 0n ? (2n * n + 1n) * 100n + 3n * n + 1n : 0n],
          ["guard_falls_through", [n], n < 0n ? 8n : n > 100n ? 1000n + n : 321n * n],
          ["as_pattern_overlap", [n], n >= 0n ? 31n * n : 8n],
          ["tuple_scrutinee", [n], n >= 0n ? 22n * n + 20n : 0n],
          ["stack_scrutinee", [n], n + 4n],
          ["recursive_scrutinee", [n], 2n * n + 2n],
          ["guard_reads_subject", [n], n >= 0n ? 4n * n + 2n : 0n],
        ]),
      ];
    })(),
    inspect(ir) {
      for (const name of ["self_update_loop", "recursive_accumulate", "branch_last_reads"]) {
        const body = ir.split("\n\n").find(block => block.startsWith("define ") && block.split("\n")[0].includes(`@tz.fn.Main.${name}(`));
        assert.ok(body, name);
        assert.equal((body.match(/call (?:nonnull )?ptr @tz\.alloc/g) ?? []).length, 1, name);
      }
    },
  },
  slices: {
    cases: [
      ["slice_sum", [0n, 0n, 0n], 0n],
      ["slice_sum", [10n, 0n, 10n], 45n],
      ["slice_sum", [10n, 3n, 7n], 18n],
      ["slice_sum", [10n, 10n, 10n], 0n],
      ["slice_nested", [], 230n],
      ["slice_string_refs", [], 13n],
      ["slice_copy_owned", [], 23n],
      ["slice_closures", [], 35n],
      ["slice_aggregates", [], 6n],
      ["slice_reborrow", [], 4n],
      ["slice_borrowed_match", [], 8n],
    ],
    traps: [["trap_slice_start", []], ["trap_slice_end", []], ["trap_slice_order", []], ["trap_slice_index", []]],
    inspect(ir) {
      assert.match(ir, /@tz\.fn\.Main\.sum\(%tz\.array/);
      assert.match(ir, /icmp ule i64/);
    },
  },
  mutable_slices: {
    cases: [
      // [1, 2, 3, 4]: the tail becomes [20, 40, 3] (sum 63) and values [1, 20, 40, 3].
      ["write_swap", [], 63n * 10000n + 1000n + 20n + 40n + 3n],
      ["negate", [], -[3n, -1n, 4n, 1n, 5n].reduce((sum, value) => sum + value, 0n)],
      ...[0n, 1n, 2n, 7n, 1000n].map((count) => ["negate_sized", [count], -(count * (count + 1n) / 2n)]),
      ["alternating", [], 10n + 2n * 20n + 3n * 30n + 4n * 40n],
      ["copy_snapshot", [], 1n * 10n + 9n],
      // ["xyz", "c", "b"]: lengths 3 + 1 + 1, and the owner sees the swap.
      ["strings_drop", [], 5n + 100n],
      ...[0n, 1n, 12n, 1000n].map((count) => ["heap_strings", [count],
        Array.from({ length: Number(count) }, (_, index) => BigInt(String(index * 10).length)).reduce((sum, length) => sum + length, 0n)]),
      // [[7, 8, 9], [3]] swapped to [[3], [7, 8, 9]].
      ["nested_frames", [], 1n * 100n + 3n * 10n + 9n],
      ["convert", [], 5n * 7n],
      ...[0n, 5n, 100n].map((count) => ["convert_owner", [count], 3n * count]),
      // [1, 2, 3, 4, 5, 6] becomes [1, 20, 30, 40, 5, 6]; outer[2..] starts at 40.
      ["views_of_views", [], 40n * 1000n + 20n * 10n + 40n],
      ["sort_sizes", [], 9n],
      // A stable sort keeps +0, -0, -0, +0 in input order: positive at bits 0, 3, and 4.
      ["sort_signed_zero", [], 1n + 8n + 16n],
      ["sort_strings", [], ["pear", "fig", "apple", "kiwi", "banana", "fig"].sort()
        .reduce((sum, word) => sum * 10n + BigInt(word.length), 0n)],
      // An exclusive borrow field holds texts[1..]; position 1 replaces "c" with "longer".
      ["record_cursor", [], 1n * 100n + 1n * 10n + 6n],
      // Each element i becomes 2i + i % 3, whatever chunk it lands in.
      ...[[0n, 1n], [1n, 1n], [10n, 3n], [10000n, 4096n], [10000n, 1n], [5n, 100n]].map(([count, size]) => ["parallel_double", [count, size],
        Array.from({ length: Number(count) }, (_, index) => 2n * BigInt(index) + BigInt(index % 3)).reduce((sum, value) => sum + value, 0n)]),
      ...[[0n, 2n], [9n, 4n], [5000n, 333n]].map(([count, size]) => ["parallel_captured", [count, size], count * (count - 1n) / 2n + 10n * count * count]),
      ...[0n, 1n, 7n, 20n, 100n].map((count) => {
        const words = Array.from({ length: Number(count) }, (_, index) => String(count - BigInt(index)));
        const sorted = [];
        for (let start = 0; start < words.length; start += 7) sorted.push(...words.slice(start, start + 7).sort());
        return ["parallel_strings", [count], sorted.reduce((sum, word) => BigInt.asIntN(64, sum * 31n + BigInt(word.length)), 0n)];
      }),
      // Element i becomes i + 10 * count (+ 1 for an odd size). More than 1,024 chunks share the
      // 1,024 jobs, unevenly for 2,049 and 5,000 chunks.
      ...[[0n, 1n], [1n, 1n], [10n, 4n], [2049n, 1n], [4098n, 2n], [5000n, 1n], [5000n, 3n], [100n, 1000n]].map(([count, size]) =>
        ["parallel_dynamic", [count, size], count * (count - 1n) / 2n + count * (10n * count + size % 2n)]),
    ],
    traps: [["trap_write_index", []], ["trap_split_past", []], ["trap_split_negative", []], ["trap_swap_index", []], ["trap_slice_order", []], ["trap_chunk_size", []]],
    inspect(ir) {
      assert.match(ir, /@tz\.fn\.Main\.fill\(%tz\.array/);
      assert.match(ir, /icmp ult i64/);
      // parallel_dynamic takes the path that copies the callback for each job.
      assert.match(ir, /getelementptr inbounds %tz\.closure, ptr %[\w.]+, i64 %chunk/);
    },
  },
  fixed_arrays: {
    cases: [
      ...[0n, 5n, min, max].map((x) => ["fixed_sum", [x], BigInt.asIntN(64, 4n * x + 6n)]),
      ["fixed_index", [0n], 0n],
      ["fixed_index", [3n], 9n],
      // `b` keeps [1, 2, 3] when `a` is replaced by [7, 8, 9].
      ["fixed_copy", [], 1n * 100n + 7n],
      ["fixed_strings", [], 1n + 2n + 3n],
      // The copied row is ["123", "4567"].
      ["fixed_clone_nested", [], 3n * 10n + 4n + 4n * 100n],
      ["fixed_nested", [], 2n * 10n + 3n],
      // probe [1, 2, 3, 4, 5] = 1 * 100 + 5, and probe [2, 3, 4] = 2 * 100 + 3.
      ["fixed_slice", [], 105n + 203n],
      ["fixed_record", [], 3n * 10n + 7n],
      ["fixed_generic", [1n], 12n],
      ["fixed_generic", [0n], 34n],
      ...[3n, max].map((n) => ["fixed_init_closure", [n], BigInt.asIntN(64, 7n * n)]),
      ["fixed_simd", [], 4n],
      ["fixed_length", [], 1024n + 1023n],
      ...[0n, 7n, 1000n].map((n) => ["fixed_strings_init", [n], [0n, 1n, 2n, 3n].reduce((sum, i) => sum + BigInt(String(i * n).length), 0n)]),
      // total [1, 2, 3] = 6, total [4, 5] = 9, and the five zeros.
      ["fixed_generic_length", [], 6n * 100n + 9n + 5n * 1000n],
      // first [12, 3456] = 12; Poly [1, 2, 3, 4] has 4 points ending in 4; Row [5, 6, 7] ends in 7.
      ["fixed_records_generic", [], 12n * 10000n + (4n * 100n + 4n) * 10n + 7n],
    ],
    traps: [["trap_index", [-1n]], ["trap_index", [4n]], ["trap_index", [max]], ["trap_constant", []], ["trap_slice", []]],
    inspect(ir) {
      assert.match(ir, /getelementptr inbounds \[4 x i64\], ptr %[\w.]+, i64 0, i64/);
      assert.match(ir, /insertvalue \[3 x i64\]/);
      const copy = ir.slice(ir.indexOf("@tz.fn.Main.fixed_copy("));
      const body = copy.slice(0, copy.indexOf("\n}"));
      assert.doesNotMatch(body, /@tz\.alloc/);
      assert.doesNotMatch(body, /icmp ult i64 \d+, \d+/);
    },
  },
  dyn_dispatch: {
    cases: [
      ...[0n, 3n, -2n, 10n].map((x) => ["total_area", [x], 2n * x * x + 11n * x]),
      ...[0n, 3n, -2n].map((x) => ["describe_total", [x], 2000n * x * x + 11000n * x + 6n]),
      ...[0n, 3n, -2n].map((x) => ["generic_total", [x], 2n * x * x + 11n * x]),
      ...[0n, 3n].map((x) => ["names", [x], 1n + 2n + 3n]),
      ...[0n, 5n, -3n].map((x) => ["bag_area", [x], x * x + x]),
      ...[7n, 0n, -1n].map((x) => ["into_area", [x], x]),
      ...[3n, 0n, -5n].map((x) => ["grown_area", [x], 4n * x * x]),
      ["maybe_area", [4n], 16n],
      ["maybe_area", [0n], -1n],
      ["maybe_area", [-2n], -1n],
      ...[1n, 0n, -1n].map((x) => ["array_area", [x], 3n * x + 3n]),
      ...[9n, -4n].map((x) => ["wide_area", [x], x]),
      ["faulty_area", [4n], 25n],
      ["faulty_area", [5n], 20n],
      ...[0n, 42n, -7n].map((x) => ["display_agrees", [x], 1n]),
      ...[0n, 42n].map((x) => ["hash_agrees", [x], 1n]),
      // Phase 2: Send, Copy, a region, several classes, upcasts, and builtin instances.
      ...[3n, -4n].map((x) => ["send_area", [x], x * x]),
      ...[2n, -3n].map((x) => ["copy_area", [x], 6n * x * x]),
      ...[5n, 0n].map((x) => ["copy_array", [x], 2n * (x + 1n)]),
      ...[3n, -1n].map((x) => ["borrowed_area", [x], 4n * x * x]),
      ...[2n, 0n].map((x) => ["multi_area", [x], x * x + x + 100n]),
      ...[3n, 0n].map((x) => ["upcast_area", [x], 10n * x * x + 1n]),
      ["marker_upcast", [4n], 16n],
      ["wrapped_tag", [3n], 16n],
      // The digits of x, then the four letters of "text".
      ["builtin_display", [42n], 2n * 10n + 4n],
      ["builtin_display", [-7n], 2n * 10n + 4n],
      ["builtin_display", [0n], 1n * 10n + 4n],
      ...[0n, 42n].map((x) => ["builtin_hash", [x], 1n]),
    ],
    traps: [["faulty_area", [0n]]],
    inspect(ir) {
      assert.match(ir, /^%tz\.dyn = type \{ ptr, ptr \}$/m);
      // Named.id, then Shape's area, grow, into_area, and describe; `upcast_area` upcasts to Named.
      assert.match(ir, /@"tz\.vtable\.Shapes\.Shape\[Main\.Square\]" = internal unnamed_addr constant \{ ptr, ptr, i64, i64, \[5 x ptr\], \[1 x ptr\] \}/);
      // The copy vtable has a clone slot; the vtable of two classes has an upcast table.
      assert.match(ir, /@"tz\.vtable\.Shapes\.Shape\+copy\[Main\.Square\]" = internal unnamed_addr constant \{ ptr, ptr, i64, i64, \[5 x ptr\] \} \{ ptr @"tz\.dyn\.drop\[Main\.Square\]", ptr @"tz\.dyn\.clone\[Main\.Square\]"/);
      assert.match(ir, /@"tz\.vtable\.Shapes\.Shape,Shapes\.Tagged\[Main\.Square\]" = internal unnamed_addr constant \{ ptr, ptr, i64, i64, \[6 x ptr\], \[1 x ptr\] \}/);
      assert.match(ir, /getelementptr inbounds \{ ptr, ptr, i64, i64, \[5 x ptr\] \}, ptr %v\d+, i32 0, i32 4, i64 4/);
    },
  },
  borrowed_comparisons: {
    cases: [
      ["compare_loop", [0n], 0n],
      ["compare_loop", [1n], 5n],
      ["compare_loop", [20000n], 100000n],
      ["method_scalar", [42n, 42n], 1],
      ["method_scalar", [42n, 43n], 0],
      ["method_float", [0, -0], 1],
      ["method_float", [1, 2], 1],
      ["method_record", [], 4n],
      ["comparison_order", [], 112n],
      ["comparison_jump", [], 1n],
    ],
    inspect(ir) {
      assert.match(ir, /icmp eq i64/);
      assert.match(ir, /fcmp olt double/);
      assert.match(ir, /@tz\.string\.equal/);
      assert.doesNotMatch(ir, /fcmp fast/);
    },
  },
  visibility: {
    cases: [
      ["answer", [], 42n],
      ["reveal", [2n], 8n],
      ["reveal", [12n], 15n],
      ["reveal", [41n], 41n],
      ["reveal", [100n], 100n],
      ["owned_total", [1n], 3n],
      ["owned_total", [64n], 66n],
    ],
    inspect(ir, header) {
      assert.match(header, /tz_answer/);
      assert.doesNotMatch(header, /owned\b|sum|weight|Token|Pair/);
      assert.match(ir, /@tz\.fn\.Secret\.weight/);
    },
  },
  generic_records: {
    cases: [
      ["numbers", [], 4203n],
      ["strings", [], 5n],
      ["nested", [39n], 42n],
      ["nested", [-3n], 0n],
      ["closures", [20n], 41n],
      ["closures", [0n], 1n],
      ["matched", [2n], 402n],
      ["collections", [5n], 3022n],
      ["nested_syntax", [39n], 42n],
      ["nested_syntax", [-3n], 0n],
    ],
    inspect(ir, header) {
      assert.doesNotMatch(header, /Pair|Box|Wrap/);
      for (const definition of [
        '%"tz.record.Main.Pair[i64,string]" = type { i64, %tz.string }',
        '%"tz.record.Main.Pair[i64,i64]" = type { i64, i64 }',
        '%"tz.record.Main.Box[fn[i64->i64]]" = type { %tz.closure }',
        '%"tz.record.Main.Wrap[Main.Pair[i64,string]]" = type { %"tz.record.Main.Pair[i64,string]" }',
      ]) {
        assert.equal(ir.split(`${definition}\n`).length, 2, `${definition} is defined once`);
      }
      assert.doesNotMatch(ir, /tz\.record\.Main\.(Pair|Box|Wrap) = type/);
    },
  },
  unions: {
    cases: [
      ["shape_area_scaled", [], 24566n],
      ["maybe_number", [], 42n],
      ["maybe_string_length", [], 5n],
      ["color_sum", [], 123n],
      ["constructors", [5n], 3655n],
      ["constructors", [-2n], 2878n],
      ["either", [4n], 8n],
      ["either", [-3n], 6n],
      ["either", [0n], 7n],
      ["guarded", [3n], 7n],
      ["guarded", [10n], 107n],
      ["nested", [5n], 5n],
      ["nested", [0n], 100n],
      ["nested", [-1n], 200n],
      ["containers", [5n], 461513n],
      ["containers", [0n], 460003n],
      ["owned_collections", [4n], 443206n],
      ["owned_collections", [0n], 93206n],
      ["copy_reuse", [1n], 6066n],
      ["copy_reuse", [10n], 33336n],
      ["captured", [3n], 21n],
      ["captured", [5n], 29n],
      ["qualified", [4n], 45n],
      ["qualified", [7n], 75n],
      ["churn", [0n], 0n],
      ["churn", [4n], 34n],
      ["churn", [40000n], 1600226662n],
    ],
    inspect(ir, header) {
      assert.doesNotMatch(header, /Shape|Maybe|Color|Pick|Tagged/);
      for (const definition of [
        '%"tz.union.Main.Shape" = type { i8, [2 x i64] }',
        '%"tz.union.Main.Color" = type i8',
        '%"tz.union.Main.Pick" = type { i8, i64 }',
        '%"tz.union.Main.Maybe[i64]" = type { i8, i64 }',
        '%"tz.union.Shapes.Shape" = type { i8, double }',
      ]) {
        assert.equal(ir.split(`${definition}\n`).length, 2, `${definition} is defined once`);
      }
      // A union of a nullary case and a string payload is its payload (PM01).
      assert.ok(!ir.includes("tz.union.Main.Maybe[string]"));
      assert.doesNotMatch(ir, /tz\.union\.Main\.Maybe = type/);
      assert.match(ir, /switch i32/);
      // First-class constructors become one hidden function per concrete instance.
      const constructors = ir.match(/^define internal \S+ @tz\.fn\.\$case\.Main\.Maybe\.Some\.\$mono\.\d+\(/gm) ?? [];
      assert.equal(constructors.length, 2, "Maybe.Some has an i64 and a string constructor function");
    },
  },
  // PM01: union tags take the fewest bytes, and the payload area, niches and field order never change a result.
  value_layout: {
    cases: [],
    inspect(ir, header) {
      assert.doesNotMatch(header, /\b(struct|union)\b/);
      for (const definition of [
        '%"tz.union.Main.Narrow" = type i8',
        '%"tz.union.Main.Mid" = type i8',
        '%"tz.union.Main.Wide" = type i16',
        '%"tz.union.Main.Big" = type { i16, [2 x i64] }',
        '%"tz.union.Main.Shape" = type { i8, [2 x i64] }',
        '%"tz.union.Main.Small" = type { i8, [1 x i8] }',
        '%"tz.union.Main.Wideness" = type { i8, [1 x i128] }',
        '%"tz.union.Main.Rich" = type { i8, [2 x i64] }',
        '%"tz.union.Main.Nested" = type { i8, [3 x i64] }',
        '%"tz.union.Main.Lanes" = type { i8, [2 x i128] }',
      ]) {
        assert.equal(ir.split(`${definition}\n`).length, 2, `${definition} is defined once`);
      }
      // PM01 Phase 2: an option-like union with a string or array payload is its payload and has no type of its own.
      for (const name of ["Main.Alias", "Main.Label", "Maybe.Maybe[string]"]) {
        assert.ok(!ir.includes(`tz.union.${name}"`), `${name} is its payload`);
      }
      assert.match(ir, /\{ ptr null, i64 -1 \}/);
      assert.match(ir, /icmp slt i64 %v\d+, 0/);
      // The native IR: an option of a reference is its pointer. (WebAssembly keeps the tag.)
      assert.ok(!ir.includes("tz.union.Maybe.Maybe[ref[i64]]"));
      assert.match(ir, /icmp eq ptr %v\d+, null/);
      // Not an option-like union of one payload case: two payload cases, three cases, and a nested option.
      assert.ok(ir.includes('%"tz.union.Maybe.Maybe[Maybe.Maybe[string]]" = type { i8, %tz.string }\n'));
    },
  },
  // PM01 Phase 3a: a record that is smaller with its fields sorted by alignment stores them that way. The
  // cases come from the generation block below, which works the results out from the source of the fixture.
  record_layout: {
    cases: [],
    inspect(ir) {
      for (const definition of [
        "%tz.record.Main.Gauge = type { i64, double, i1, i8 }",
        "%tz.record.Main.Keyed = type { i64, i8, i8 }",
        "%tz.record.Main.Spread = type { i64, i32, i8, i8, i1 }",
        "%tz.record.Main.Crate = type { %tz.record.Main.Gauge, i16, i1 }",
        '%"tz.record.Main.Cargo[i64]" = type { i64, i1, i1 }',
        '%"tz.record.Main.Cargo[string]" = type { %tz.string, i1, i1 }',
        // Equal alignments keep their declared order.
        '%"tz.record.Main.Cargo[bool]" = type { i1, i1, i1 }',
        "%tz.record.Main.Ledger = type { %tz.string, %tz.array, i1, i8 }",
        "%tz.record.Main.Rows = type { %tz.array, i1, i8 }",
        "%tz.record.Main.Wide = type { <8 x float>, i8, i8 }",
      ]) {
        assert.equal(ir.split(`${definition}\n`).length, 2, `${definition} is defined once`);
      }
      // The static data of a constant stores each field where the record keeps it: count, ratio, flag, tag.
      assert.match(ir, /constant %tz\.record\.Main\.Gauge \{ i64 42, double 0x4004000000000000, i1 1, i8 7 \}/);
      assert.match(ir, /%tz\.record\.Main\.Spread \{ i64 2, i32 4, i8 1, i8 3, i1 1 \}/);
    },
  },
  stdlib: {
    cases: [
      ["answer", [], 42n],
      ["checked", [5n], 5n],
      ["checked", [0n], 0n],
    ],
    traps: [["checked", [-1n]]],
    inspect(ir, header) {
      assert.match(header, /tz_answer/);
      assert.doesNotMatch(header, /Math|zero|identity/);
      for (const [pattern, definition] of [
        [/^define internal (?:noundef )?double @tz\.fn\.Math\.zero\(/gm, "Math.zero"],
        [/^define internal (?:noundef )?double @tz\.fn\.Math\.identity_f64\(/gm, "Math.identity_f64"],
        [/^define internal i64 @tz\.builtin\.unreachable\.i64\(i8 %unit\) noreturn nounwind \{/gm, "unreachable"],
      ]) {
        assert.equal([...ir.matchAll(pattern)].length, 1, `${definition} is defined once`);
      }
    },
  },
  maybe_result: {
    cases: [
      ["maybe_some", [], 42n],
      ["maybe_none_short_circuit", [], 42n],
      ["result_ok", [], 42n],
      ["result_error_short_circuit", [], 42n],
      ["propagation_first_owned_error", [], 11n],
      ["propagation_explicit_conversion", [], 42n],
      ["propagation_selected_branch", [1], 42n],
      ["propagation_eager_let", [1n], 42n],
      ["maybe_loop", [0n], 0n],
      ["maybe_loop", [257n], 257n],
      ["result_loop", [0n], 0n],
      ["result_loop", [257n], 257n],
      ["maybe_loop", [40000n], 40000n],
      ["result_loop", [40000n], 40000n],
      ["stopped_loops", [], 42n],
      ["while_and_zero", [], 42n],
      ["while_failure", [], 42n],
      ["owned_loop_failures", [], 12n],
      ["remaining_functions", [], 42n],
      ["defaults", [], 42n],
      ["conversions", [], 42n],
      ["owned", [0n], 0n],
      ["owned", [1n], 20n],
      ["owned", [40000n], 800000n],
      ["copy_snapshot", [], 42n],
      ["borrowed_patterns", [], 24n],
    ],
    traps: [
      ["trap_maybe_get", []],
      ["trap_result_get", []],
      ["trap_result_get_error", []],
      ["trap_string", []],
      ["trap_eager_default", []],
      ["trap_propagation_eager_let", []],
      ["trap_propagation_statement", []],
    ],
    inspect(ir, header) {
      assert.doesNotMatch(header, /Maybe|Result/);
      assert.match(ir, /@tz\.specialized\./);
      assert.match(ir, /tz\.union\.Maybe\.Maybe\[i64\]/);
      assert.doesNotMatch(ir, /tz\.union\.Maybe\.Maybe\[string\]/);
      assert.match(ir, /tz\.union\.Result\.Result\[string,i64\]/);
    },
  },
  json: {
    cases: [
      ["scene_hash", [], jsonTextHash(JSON.stringify({ name: "a", points: [{ x: 1, y: 0.5 }], shapes: [{ Circle: 2 }, { Rect: [1, 2.5] }, "Empty"], note: null }))],
      ["scene_round_trip", [], 1],
      ["nesting", [1n], -1n], ["nesting", [128n], -1n], ["nesting", [129n], jsonStatus(6, 128)],
      ["nesting", [1000000n], jsonStatus(6, 128)],
      ["object_nesting", [128n], -1n], ["object_nesting", [129n], jsonStatus(6, 128 * 5)],
      ...[0.1, -0, 5e-324, 1.7976931348623157e308, -0, Number("1.000000059604644775390625000000001"),
        Number("123456789012345678901234567890"), undefined, -0, Number("2.2250738585072011e-308")]
        .flatMap((value, index) => value === undefined ? [] : [
          ["number_value", [BigInt(index)], value],
          ["number_sign", [BigInt(index)], Object.is(value, -0) ? -1 : 1],
        ]),
      ["number_status", [0n], -1n], ["number_status", [7n], jsonStatus(9, -1)],
      ["float32_value", [0n], Math.fround(0.1)],
      // 1 + 2^-24 + 1e-33 is above the tie, so f32 rounds up; rounding through f64 would give 1.
      ["float32_value", [5n], 1 + 2 ** -23],
      ["float32_value", [6n], integerToF32(123456789012345678901234567890n)],
      ...[-1n, jsonStatus(9, -1), jsonStatus(10, -1), -2n, -1n, -1n, jsonStatus(9, -1), jsonStatus(10, -1),
        BigInt.asIntN(64, ((1n << 128n) - 1n) % 1000000007n), jsonStatus(10, -1), jsonStatus(9, -1), -128n, jsonStatus(1, 3)]
        .map((expected, index) => ["integer_status", [BigInt(index)], expected]),
      ...[
        [jsonStatus(11, -1), 'json: missing field "y"'], [-1n, null], [jsonStatus(10, -1), "json: expected integer"],
        [jsonStatus(10, -1), "json: expected number"], [jsonStatus(10, -1), "json: expected object"],
        [jsonStatus(12, -1), 'json: unknown case "Square"'], [jsonStatus(10, -1), "json: expected object"], [-1n, null],
        [jsonStatus(10, -1), "json: expected string"], [jsonStatus(10, -1), "json: expected object with one member"],
        [jsonStatus(10, -1), "json: expected string or object"], [jsonStatus(10, -1), "json: expected array of 2"], [-1n, null],
        [jsonStatus(1, 5), "json: syntax error at byte 5"], [jsonStatus(5, 7), 'json: duplicate key "a" at byte 7'],
        [jsonStatus(1, 1), "json: syntax error at byte 1"], [jsonStatus(5, 13), 'json: duplicate key "x" at byte 13'],
      ].flatMap(([expected, text], index) => [
        ["field_rules", [BigInt(index)], expected],
        ["field_messages", [BigInt(index)], text === null ? 0n : jsonTextHash(text)],
      ]),
      ["text_rules", [0n], jsonTextHash(JSON.stringify("\ud800"))],
      ["text_rules", [1n], jsonStatus(13, -1)], ["text_rules", [2n], jsonStatus(8, -1)], ["text_rules", [3n], jsonStatus(8, -1)],
      ["text_rules", [4n], fnv64([0xf0, 0x9f, 0x98, 0x80])],
      ["text_rules", [5n], jsonTextHash(JSON.stringify("\u2028\u007f/\"\\\ud83d\ude00\udc00"))],
      ["text_rules", [6n], jsonTextHash(JSON.stringify(String.fromCharCode(...Array.from({ length: 32 }, (_, index) => index))))],
      // Numbers keep the shortest round-trip text of `to_string`, unlike JSON.stringify ("0", "10000000").
      ["text_rules", [7n], jsonTextHash("-0")], ["text_rules", [8n], jsonTextHash("1e+7")], ["text_rules", [9n], jsonTextHash("0.1")],
      ["text_rules", [10n], jsonTextHash("[1,null]")], ["text_rules", [11n], jsonTextHash("[1,null,[2,3]]")],
      ["text_rules", [12n], jsonTextHash(`[${-(1n << 127n)},${(1n << 128n) - 1n},true,"é"]`)],
      // Phase 2: maps (objects for string keys, `[key, value]` pairs otherwise), sets, f16, f128, decimals.
      ...['{"a":1,"b":2}', '[[2,"y"],[10,"x"]]', "{}", '{"z":3,"a":2}', '{"Blue":1.5,"Red":65500}', "[1,2.5]", "[3,1]",
        "[0.5,0.1,1e+300,[1.1,-0]]", jsonStatus(5, -1), '{"a":1}', jsonStatus(10, -1), jsonStatus(10, -1), '[[1,"a"],[2,"b"]]',
        jsonStatus(9, -1), "1.234568", jsonStatus(8, -1), '{"Blue":1,"Red":2}', jsonStatus(10, -1)]
        .map((expected, index) => ["container_hash", [BigInt(index)], typeof expected === "string" ? jsonTextHash(expected) : expected]),
      // Map and Set decoding sorts the entries by key: inserting descending keys one by one would be
      // quadratic. WASM has 16 MiB, so the larger inputs run natively (`nativeCases`).
      ["descending", [0n, 50000n], 50000n], ["descending", [1n, 25000n], 25000n], ["descending", [2n, 5000n], 5000n],
      // A repeated key is reported where inserting in document order would find it, in its text there.
      ...[
        ["9", "1", "9", "5", "1"], ["2.0", "1", "2", "1.0"], ["3", "1", "2", "1", "3"], ["10", "2", "1e1", "2.0"], ["3", "1", "03", "001"],
      ].map((texts) => [jsonStatus(5, -1), duplicateMessage(texts, Number)])
        .concat([[jsonStatus(5, -1), duplicateMessage(['"Red"', '"Blue"', '"Green"', '"Blue"', '"Red"'])],
          [jsonStatus(5, -1), duplicateMessage(["4", "8", "8", "4"], Number)], [jsonStatus(10, -1), "json: expected integer"], [-1n, null], [-1n, null], [-1n, null]])
        .flatMap(([expected, text], index) => [
          ["duplicate_status", [BigInt(index)], expected],
          ["duplicate_message", [BigInt(index)], text === null ? 0n : jsonTextHash(text)],
        ]),
      ["scattered_message", [50000n], jsonTextHash(duplicateMessage(scattered(50000).map(String), Number))],
      // `@json` names of fields and cases.
      ["renamed", [0n], jsonTextHash('[{"created":{"user_id":7,"display name":"Ann","email":null}},{"deleted":7},"Reset"]')],
      ["renamed", [1n], jsonTextHash('[{"created":{"user_id":1,"display name":"B","email":null}},"Reset"]')],
      ["renamed", [2n], jsonStatus(12, -1)], ["renamed", [3n], jsonStatus(11, -1)],
      ["renamed_message", [], jsonTextHash('json: missing field "user_id"')],
      // Pretty printing is JSON.stringify(value, null, indent), with the indent clamped to 0..10.
      ...[-1, 0, 1, 2, 4, 10, 11, 100].map((indent) => ["pretty", [BigInt(indent)],
        jsonTextHash(JSON.stringify(JSON.parse('{"a":[1,{"b":[]},{}],"c":"d","e":{"f":null}}'), null, indent))]),
      // The incremental writer and its misuses.
      ["writer_check", [-1n], jsonTextHash(`{"a":[null,true,1.50,{"x":[]}],${JSON.stringify("b\n")}:${JSON.stringify('x"y\ud800')}}`)],
      ...[jsonStatus(1, 1), jsonStatus(1, 1), jsonStatus(2, 5), jsonStatus(1, 4), jsonStatus(1, 1), jsonStatus(2, 0), jsonStatus(1, 5), jsonStatus(1, 0)]
        .map((expected, index) => ["writer_check", [BigInt(index)], expected]),
      // The pull parser: events, then `|` and the status of the error (the same kind and offset as `parse`).
      ...[" { Ka=a [ N1@8 SxA! T Z { } ] Kb { Kc N-2.5e3@50 } } $", ` { Ka=a N1@5 Ka=a [ N1@12|${jsonStatus(5, 7)}`,
        ` [ N1@1 N2@4|${jsonStatus(2, 5)}`, ` { Kk N1@5 Kk N2@12|${jsonStatus(5, 8)}`, ` [ ]|${jsonStatus(1, 3)}`, " Sété! $",
        ` { Ka=a { Kb N1@10 Kb N2@16|${jsonStatus(5, 12)}`, `${" [".repeat(128)}|${jsonStatus(6, 128)}`]
        .map((expected, index) => ["stream_events", [BigInt(index)], jsonTextHash(expected)]),
    ],
    nativeCases: [
      ["too_large", [], jsonStatus(7, 0)],
      ["descending", [0n, 200000n], 200000n], ["descending", [1n, 200000n], 200000n], ["descending", [2n, 100000n], 100000n],
      ["scattered_message", [200000n], jsonTextHash(duplicateMessage(scattered(200000).map(String), Number))],
    ],
    inspect(ir) {
      assert.match(ir, /define internal [^\n]*@tz\.fn\.Json\.parse\(/);
      assert.doesNotMatch(ir, /@printf|@strtod|@strtof|@snprintf/);
    },
  },
  cbor: {
    cases: [
      ...["00", "17", "1818", "1b000000e8d4a51000", "1bffffffffffffffff", "c249010000000000000000", "3bffffffffffffffff",
        "c349010000000000000000", "3903e7", "f90000", "f98000", "fb3ff199999999999a", "f93e00", "f97bff", "fa47c35000", "fa7f7fffff",
        "fb7e37e43c8800759c", "f90001", "f90400", "fbc010666666666666", "83f4f5f6", "69c3bce6b0b4f0908591", "8301820203820405",
        "98190102030405060708090a0b0c0d0e0f101112131415161718181819", "a26161016162820203", "a56161614161626142616361436164614461656145",
        "a361610361620162616102", "f98000", "f95640", jsonStatus(9, -1), jsonStatus(13, -1), `c25101${"00".repeat(16)}`, "fb0000000000000001"]
        .map((expected, index) => ["encode_hash", [BigInt(index)], typeof expected === "string" ? fnv64(hexBytes(expected)) : expected]),
      ...["18446744073709551615", "-18446744073709551616", "18446744073709551616", "-18446744073709551617", "5.960464477539063e-8", "100000",
        "1.1", "-0", '{"a":1,"b":[2,3]}', '["a",{"b":"c"}]', JSON.stringify("\ud800\udd51"), "0", '{"b":1,"a":1}',
        jsonStatus(8, 0), jsonStatus(8, 0), jsonStatus(15, 0), jsonStatus(15, 0), jsonStatus(15, 0), jsonStatus(15, 0), jsonStatus(15, 0),
        jsonStatus(5, 4), jsonStatus(2, 2), jsonStatus(2, 1), jsonStatus(1, 0), jsonStatus(1, 0), jsonStatus(1, 0), jsonStatus(15, 0),
        jsonStatus(14, 0), jsonStatus(15, 1), jsonStatus(1, 1), jsonStatus(2, 9), jsonStatus(2, 5), "0", "5e-324", "1"]
        .map((expected, index) => ["decode_hash", [BigInt(index)], typeof expected === "string" ? jsonTextHash(expected) : expected]),
      ["nesting", [127n], -1n], ["nesting", [128n], jsonStatus(6, 128)], ["nesting", [1000000n], jsonStatus(6, 128)],
      ["scene_round_trip", [], 1],
      ["scene_hash", [], fnv64(cborMap([
        ["name", cborText("a")],
        ["points", cborArray([cborMap([["x", [0x01]], ["y", hexBytes("f93800")]]), cborMap([["x", [0x21]], ["y", hexBytes("f98000")]])])],
        ["shapes", cborArray([cborMap([["Circle", [0x02]]]), cborMap([["Rect", cborArray([[0x01], hexBytes("f94100")])]]), cborText("Empty")])],
        ["note", cborText("é")],
        ["big", [0xc2, 0x50, ...Array(16).fill(0xff)]],
      ]))],
      // Map and Set decoding from CBOR sorts the entries by key (see the json suite).
      ["descending", [0n, 50000n], 50000n], ["descending", [1n, 25000n], 25000n],
      ["duplicate_message", [0n], jsonTextHash(duplicateMessage(["9", "1", "9", "5", "1"], Number))],
      ["duplicate_message", [50000n], jsonTextHash(duplicateMessage(scattered(50000).map(String), Number))],
      // Every integer of at most 4096 digits round-trips; a bignum decodes only within that limit
      // (PR #17 review). The digit counts come from BigInt, not from the compiler.
      ...[[1n, 0n], [21n, 0n], [21n, 1n], [4096n, 0n], [4096n, 1n]].map(([digits, sign]) => ["bignum_round_trip", [digits, sign], 1n]),
      ["bignum_round_trip", [4097n, 0n], 2n],
      ...[1700n, 1701n, 1702n].map((length) => ["bignum_bytes", [length],
        length <= 1701n && ((1n << (8n * length)) - 1n).toString().length <= 4096 ? 1n : jsonStatus(9, 0)]),
    ],
    nativeCases: [
      ["descending", [0n, 200000n], 200000n], ["descending", [1n, 200000n], 200000n],
      ["duplicate_message", [200000n], jsonTextHash(duplicateMessage(scattered(200000).map(String), Number))],
    ],
  },
  display_parse: {
    cases: [
      ["displays", [], 1],
      ["text_copy", [], 20n],
      ["parse_numbers", [], 42n],
      ["parse_bool", [], 1],
      ["malformed", [], 1],
      ["round_f64", [0.1], 0.1],
      ["round_f32", [Math.fround(0.1)], Math.fround(0.1)],
      ["custom", [0n], 0n],
      ["custom", [1n], 10n],
      ["custom", [40000n], 400000n],
    ],
    inspect(ir) {
      assert.match(ir, /@tz_soft_format/);
      assert.match(ir, /@tz_soft_parse/);
      assert.match(ir, /@tz\.integer\.parse/);
      assert.doesNotMatch(ir, /@printf|@strtod|@strtof|@snprintf|@memcmp/);
    },
  },
  bounds_checks: {
    cases: [
      // `3 * i + 1` for i < n sums to 3n(n - 1)/2 + n.
      ...[0n, 1n, 5n, 1000n].flatMap((count) => ["forward", "backward", "builtin_length"]
        .map((name) => [name, [count], 3n * count * (count - 1n) / 2n + count])),
      ...[-1n, 0n, 2n, 3n, min, max].map((index) => ["pick", [index], 30n + (index >= 0n && index < 3n ? [10n, 20n, 30n][Number(index)] : 0n)]),
      ["replaced", [0n], 0n], ["replaced", [1n], 0n],
      ...[0n, 5n].map((count) => ["while_sum", [count], 3n * count * (count - 1n) / 2n + count]),
      ...[-1n, 0n, 1n, 2n, 3n, 4n].map((start) => ["tail_walk", [start], start >= 0n && start < 3n ? [10n, 20n, 30n].slice(Number(start)).reduce((sum, value) => sum + value, 0n) : 0n]),
    ],
    traps: [["replaced", [2n]], ["other", [1n]], ["past_end", [0n]], ["past_end", [3n]], ["literal_past", [0n]]],
    // Range proofs remove checks and add no facts; overflow flags (PR02) are written only in proven mode.
    inspect(ir) { assert.doesNotMatch(ir, process.env.TSUZURI_RANGE_FLAGS === "proven" ? /llvm\.assume|!range/ : /llvm\.assume|!range| nsw | nuw /); },
  },
  // C11: every expected value is computed by tests/matrix-cases.mjs from JavaScript numbers and BigInt.
  matrix: {
    cases: matrixCases,
    traps: matrixTraps,
    trapKind: "assertion failed",
    inspect(ir) {
      assert.match(ir, /@tz\.fn\.Matrix\./);
      assert.doesNotMatch(ir, /fmuladd|\bfast\b|\bcontract\b|\breassoc\b/);
      assert.doesNotMatch(ir, /%tz\.matrix/);
      // Only the explicit fused kernels may call Math.fma; the separately rounded product never does.
      for (const chunk of ir.split("\ndefine ")) {
        if (/^[^\n]*@tz\.fn\.Matrix\.multiply_rows\./.test(chunk)) assert.doesNotMatch(chunk.split("\n}\n")[0], /llvm\.fma|tz_soft_fma|Math\.fma/);
      }
      // Math.fma is llvm.fma only in native code of a compiler built for AArch64 (src/llvm_math.rs); elsewhere it is the soft routine.
      assert.match(ir, /@llvm\.fma\.f64|@tz_soft_fma/);
    },
  },
  // C11 Phase 2: windows are modelled in tests/matrix-view-cases.mjs as index mappings, not as strides.
  matrix_view: {
    cases: matrixViewCases,
    bounded: matrixViewBounded,
    traps: matrixViewTraps,
    trapKind: "assertion failed",
    inspect(ir) {
      assert.match(ir, /@tz\.fn\.MatrixView\./);
      assert.doesNotMatch(ir, /fmuladd|llvm\.fma|\bfast\b|\bcontract\b|\breassoc\b/);
    },
  },
  // C11 Phase 2: tensor windows are modelled in tests/tensor-cases.mjs as index mappings, not as strides.
  tensor: {
    cases: tensorCases,
    bounded: tensorBounded,
    traps: tensorTraps,
    trapKind: "assertion failed",
    inspect(ir) {
      assert.match(ir, /@tz\.fn\.Tensor\./);
      assert.doesNotMatch(ir, /fmuladd|llvm\.fma|\bfast\b|\bcontract\b|\breassoc\b/);
    },
  },
  // F10: Atomic, Mutex and Task.scope. Every case is a closed expression that does not depend on the
  // schedule: sums of commutative updates and results in index order.
  concurrency: {
    cases: [
      ["atomic_i8", [], atomicSequence(8, true, 100n, 100n, 90n)],
      ["atomic_i16", [], atomicSequence(16, true, 30000n, 20000n, 12345n)],
      ["atomic_i32", [], atomicSequence(32, true, 2000000000n, 1500000000n, 123456789n)],
      ["atomic_i64", [], atomicSequence(64, true, 9000000000000000000n, 4000000000000000000n, 1234567890123456789n)],
      ["atomic_u8", [], atomicSequence(8, false, 100n, 100n, 90n)],
      ["atomic_u16", [], atomicSequence(16, false, 60000n, 20000n, 12345n)],
      ["atomic_u32", [], atomicSequence(32, false, 4000000000n, 1500000000n, 123456789n)],
      ["atomic_u64", [], atomicSequence(64, false, 18000000000000000000n, 4000000000000000000n, 1234567890123456789n)],
      ...[0n, 1n, 4n, 64n, 257n].map((n) => ["atomic_counter", [n], n * (n + 1n) / 2n]),
      ["atomic_compare_exchange", [], 5n * 10000n + 9n * 100n + 9n],
      ["atomic_wrapping_i32", [], -2147483648n],
      ["atomic_bool_flag", [], 1n],
      ["atomic_into_inner", [], 42n],
      ...[0n, 1n, 10n, 100n, 1000n].map((n) => {
        const misses = (n + 2n) / 3n;
        return ["counters_record", [n], 2n * (n - misses) * 1000000n + misses];
      }),
      ...[0n, 1n, 2n, 1000n].map((n) => {
        let checksum = 0n;
        for (let position = 0n; position < n; position++) checksum = BigInt.asIntN(64, checksum * 31n + (3n * position + 1n) + position);
        return ["scope_results_order", [n], checksum];
      }),
      ...[0n, 1n, 4n, 4097n].map((n) => ["scope_shared_array", [n], n * (n + 1n)]),
      ...[0n, 1n, 9n, 300n].map((n) => ["scope_shared_function", [n], n * (n - 1n) / 2n + 7n * n]),
      ["scope_nested_parallel", [], 4n * 4096n * 4097n / 2n],
      ["scope_empty", [], 0n],
      // Four children each see the array 0, 2, ... 2(n-1) of sum n(n-1), then two see "<n>ab".
      ...[0n, 1n, 5n, 100n, 1000n].map((n) => ["scope_temporary", [n], (4n * n * (n - 1n) + 6n) * 1000n + 2n * BigInt(String(n).length + 2) + 1n]),
      // The total n(n+1)/2 of the locked additions, and the indices n(n-1)/2 that the children return.
      ...[0n, 1n, 4n, 64n, 257n].map((n) => ["mutex_total", [n], n * n]),
      ...[0n, 1n, 4n, 257n].map((n) => ["mutex_owned_array", [n], n * n]),
      ...[0n, 1n, 257n].map((n) => ["mutex_record", [n], n * 1000000n + n * (n - 1n) / 2n]),
      ["mutex_text", [], 6n * 100n + 6n],
      // The tasks drop their Arcs before the call returns, so one owner is left.
      ...[0n, 1n, 4n, 64n].map((n) => ["arc_mutex_tasks", [n], n * (n + 1n) / 2n * 1000n + 1n]),
      ...[0n, 1n, 4n, 64n].map((n) => ["arc_atomic_tasks", [n], n * (n + 1n) / 2n * 1000n + 1n]),
      ...[0n, 1n, 4n, 64n, 257n].map((n) => ["scope_arc_mutex", [n], n * (n + 1n) / 2n]),
      // The task takes `head` with it, and dropping it releases the Arc that it holds of `tail`.
      ["linked_mutex", [], 2n * 10n + 1n],
      // The builtins are asked for Send and Sync of `'f<i64>`, and a holder of plain values passes.
      ["hkt_parallel", [], 6n],
      ["hkt_mutex", [], 42n],
      ["hkt_scope", [], 6n],
    ],
    traps: [["mutex_nested", []], ["mutex_parallel_inside", []], ["scope_negative", []]],
    inspect(ir) {
      // One sequentially consistent instruction per operation, and no runtime call for it.
      assert.match(ir, /atomicrmw add ptr %[\w.]+, i64 %[\w.]+ seq_cst, align 8/);
      assert.match(ir, /atomicrmw xchg ptr/);
      assert.match(ir, /cmpxchg ptr %[\w.]+, i8 %[\w.]+, i8 %[\w.]+ seq_cst seq_cst, align 1/);
      assert.match(ir, /load atomic i8, ptr %[\w.]+ seq_cst, align 1/);
      assert.match(ir, /load atomic i64, ptr %[\w.]+ seq_cst, align 8/);
      assert.doesNotMatch(ir, /call [^\n]*@tsuzuri_atomic/);
      // The lock and the parallel entries come from the runtime that the driver links.
      for (const declaration of ["declare i32 @tsuzuri_mutex_lock(ptr)", "declare void @tsuzuri_mutex_unlock(ptr)", "declare i32 @tsuzuri_mutex_parallel_ok()",
        "declare void @tsuzuri_task_parallel(ptr, ptr, i64)"]) {
        assert.equal(ir.split("\n").filter((line) => line === declaration).length, 1, declaration);
      }
      // Parallel work starts through the wrappers that refuse to start inside a lock.
      assert.match(ir, /^define internal void @tz\.mutex\.parallel\(ptr %run, ptr %context, i64 %length\)/m);
      assert.doesNotMatch(ir, /call void @tsuzuri_task_parallel\(ptr @tz\.parallel/);
      assert.match(ir, /call void @tz\.mutex\.parallel\(ptr @tz\.parallel\.chunk\.\d+/);
    },
  },
  // F10 Phase 2: Channel. The exports need no second thread: up to the capacity a channel is a ring,
  // and everything that can wait traps the same way on every target.
  channel: {
    cases: [
      ...[1n, 2n, 3n, 10n, 257n, 4096n].map((n) => ["roundtrip", [n], n * (n + 1n) / 2n]),
      ["fifo", [], 123456n],
      ["closed", [], 123000n],
      ["refused", [], 41n],
      ["cloned", [], 50n],
      ["text", [], 54n],
      ["returned", [], 8n],
      // n tokens are dropped once each: one when it is received, the others with the channel. The
      // counter then has no other owner.
      ...[1n, 2n, 7n, 100n].map((n) => ["tokens", [n], n * 1000n + 10n + 1n]),
      ["scope_end", [], 4n * 100n + 3n * 10n + 1n],
      ["signals", [], 110n],
      ["record_ends", [], 10n],
      // The item that is left in the outer channel is a Sender: dropping it closes the inner channel.
      ["nested", [], 0n],
      // A function is an item: it carries copies, and a received one goes on to another channel.
      ["functions", [], 4112n],
      ["forwarded", [], 42n * 10n + 1n],
      ["function_drop", [], 31n],
      // The channel is made through a constructor variable, and its items are plain values.
      ["hkt_channel", [], 42n],
    ],
    traps: [["bounded_zero", []], ["bounded_negative", []], ["full_deadlock", []], ["empty_deadlock", []],
      ["send_inside_lock", []], ["recv_inside_lock", []]],
    inspect(ir) {
      // The channel runtime comes from the runtime that the driver links; the lowering only calls it.
      for (const declaration of ["declare i32 @tsuzuri_channel_send(ptr, ptr)", "declare i32 @tsuzuri_channel_recv(ptr, ptr)",
        "declare void @tsuzuri_channel_clone_sender(ptr)", "declare i32 @tsuzuri_channel_close(ptr, i32)",
        "declare i32 @tsuzuri_mutex_wait_ok()", "declare void @tsuzuri_task_parallel(ptr, ptr, i64)"]) {
        assert.equal(ir.split("\n").filter((line) => line === declaration).length, 1, declaration);
      }
    },
  },
};

const stringSamples = ["", "hello hello", "l", "a\0b", "\u{1f600}", "\ue000", " \tAbC\r\n", "\ud800", "\ude00", "aa"];
for (const count of [0, 1, 4095, 4096, 4097, 8192]) {
  const chunks = Math.min(1024, Math.ceil(count / 4096));
  const fold = (identity, initial, value, combine) => {
    if (!chunks) return identity;
    const partials = [];
    for (let chunk = 0; chunk < chunks; chunk++) {
      const first = Math.floor(count / chunks) * chunk + Math.floor((count % chunks) * chunk / chunks);
      const last = Math.floor(count / chunks) * (chunk + 1) + Math.floor((count % chunks) * (chunk + 1) / chunks);
      let total = initial;
      for (let index = first; index < last; index++) total = combine(total, value(index));
      partials.push(total);
    }
    return partials.reduce(combine, identity);
  };
  suites.parallel.cases.push(["parallel_subtract", [BigInt(count)], fold(10n, 10n, (index) => BigInt(index % 7), (left, right) => BigInt.asIntN(64, left - right))]);
  suites.parallel.cases.push(["parallel_copy_reduce", [BigInt(count)], fold(1n, 1n, BigInt, (left, right) => left + right)]);
  suites.parallel.cases.push(["parallel_float", [BigInt(count)], BigInt(fold(0, 0, (index) => index % 3 === 0 ? 1e16 : index % 3 === 1 ? 1 : -1e16, (left, right) => left + right))]);
}
const wrapHash = (value) => BigInt.asIntN(64, value);
function textHash(text, utf8) {
  const units = utf8 ? [...Buffer.from(text, "utf8")] : text.split("").map((character) => character.charCodeAt(0));
  return units.reduce((result, value) => wrapHash(result * 31n + BigInt(value)), BigInt(units.length));
}
for (const utf8 of [false, true]) {
  const indices = stringSamples.map((_, index) => index).filter((index) => !utf8 || ![7, 8].includes(index));
  for (const first of indices) for (const second of indices) {
    const text = stringSamples[first], needle = stringSamples[second];
    const textBytes = Buffer.from(text, "utf8"), needleBytes = Buffer.from(needle, "utf8");
    const parts = needle === "" && utf8 ? Array.from(text) : text.split(needle);
    const emptyReplaced = utf8 ? (text === "" ? "-" : `-${Array.from(text).join("-")}-`) : text.replaceAll("", "-");
    const values = [
      utf8 ? textBytes.indexOf(needleBytes) : text.indexOf(needle),
      utf8 ? textBytes.lastIndexOf(needleBytes) : text.lastIndexOf(needle),
      Number(text.includes(needle)), Number(text.startsWith(needle)), Number(text.endsWith(needle)),
      textHash(needle === "" ? emptyReplaced : text.replaceAll(needle, "-"), utf8),
      parts.reduce((total, part) => wrapHash(total * 31n + textHash(part, utf8)), BigInt(parts.length)),
      textHash(text + needle, utf8), textHash(`${text}-${needle}`, utf8),
      textHash(text.replace(/^[\t\n\v\f\r ]+|[\t\n\v\f\r ]+$/g, ""), utf8),
      textHash(text.replace(/[A-Z]/g, (letter) => letter.toLowerCase()), utf8),
      textHash(text.replace(/[a-z]/g, (letter) => letter.toUpperCase()), utf8),
      utf8 ? Math.sign(Buffer.compare(textBytes, needleBytes)) : text < needle ? -1 : text > needle ? 1 : 0,
      textHash(text.repeat(2), utf8),
    ];
    for (const [operation, expected] of values.entries()) suites.string_library.cases.push([utf8 ? "utf8_operation" : "text_operation", [operation, BigInt(first), BigInt(second)], BigInt(expected)]);
  }
  for (const which of indices) {
    const text = stringSamples[which], bytes = Buffer.from(text, "utf8"), length = utf8 ? bytes.length : text.length;
    for (let first = -1; first <= length + 1; first++) for (let last = -1; last <= length + 1; last++) {
      const boundary = (offset) => offset === 0 || offset === length || (bytes[offset] & 0xc0) !== 0x80;
      const valid = first >= 0 && first <= last && last <= length && (!utf8 || (boundary(first) && boundary(last)));
      const expected = valid ? textHash(utf8 ? bytes.subarray(first, last).toString("utf8") : text.slice(first, last), utf8) : -1n;
      suites.string_library.cases.push([utf8 ? "utf8_slice" : "text_slice", [BigInt(which), BigInt(first), BigInt(last)], expected]);
    }
  }
}

function hashBytes(bytes) {
  return bytes.reduce((hash, byte) => BigInt.asUintN(64, (hash ^ BigInt(byte)) * 1099511628211n), 14695981039346656037n);
}
function littleEndian(value, bytes = 8) {
  return Array.from({ length: bytes }, (_, index) => Number((BigInt(value) >> BigInt(index * 8)) & 255n));
}
function scalarHash(tag, value, bytes) {
  return BigInt.asIntN(64, hashBytes([...littleEndian(tag), ...littleEndian(value, bytes)]));
}
for (let kind = 0; kind < 10; kind++) {
  const bytes = 1 << (kind % 5);
  for (const seed of [min, -1n, 0n, 1n, 127n, 256n, max]) {
    suites.deriving.cases.push(["hash_integer", [BigInt(kind), seed], scalarHash(0x10 + kind % 5, bytes === 16 ? seed << 72n : seed, bytes)]);
  }
}
for (const [kind, bits, fraction, bias] of [[0, 16, 10, 15], [1, 32, 23, 127], [2, 64, 52, 1023], [3, 128, 112, 16383]]) {
  const sign = 1n << BigInt(bits - 1);
  const infinity = BigInt(bias * 2 + 1) << BigInt(fraction);
  const values = [0n, 0n, BigInt(bias) << BigInt(fraction) | 1n << BigInt(fraction - 1), sign | BigInt(bias + 1) << BigInt(fraction) | 1n << BigInt(fraction - 2), infinity, sign | infinity, infinity | 1n << BigInt(fraction - 1)];
  values.forEach((value, which) => suites.deriving.cases.push(["hash_real", [BigInt(kind), BigInt(which)], scalarHash(0x21 + kind, value, bits / 8)]));
}
for (const [kind, bits, fraction, bias] of [[0, 32, 23, 101], [1, 64, 53, 398], [2, 128, 113, 6176]]) {
  const sign = 1n << BigInt(bits - 1);
  const values = [BigInt(bias - 1) << BigInt(fraction) | 12n, BigInt(bias - 1) << BigInt(fraction) | 12n, BigInt(bias) << BigInt(fraction), BigInt(bias) << BigInt(fraction), sign | BigInt(bias) << BigInt(fraction) | 12n, 31n << BigInt(bits - 6), 30n << BigInt(bits - 6), sign | 30n << BigInt(bits - 6)];
  values.forEach((value, which) => suites.deriving.cases.push(["hash_decimal", [BigInt(kind), BigInt(which)], scalarHash(0x32 + kind, value, bits / 8)]));
}
for (const [which, text] of ["hello", "A\n\"\\\0", "\uD800", "\u{1F600}"].entries()) {
  const units = Array.from({ length: text.length }, (_, index) => text.charCodeAt(index));
  suites.deriving.cases.push(["hash_text", [BigInt(which)], BigInt.asIntN(64, hashBytes([...littleEndian(0x53), ...littleEndian(units.length), ...units.flatMap((unit) => littleEndian(unit, 2))]))]);
}
for (const [which, text] of ["hello", "\u{1F600}"].entries()) {
  const bytes = [...Buffer.from(text)];
  suites.deriving.cases.push(["hash_utf8", [BigInt(which)], BigInt.asIntN(64, hashBytes([...littleEndian(0x73), ...littleEndian(bytes.length), ...bytes]))]);
}
for (const [which, value] of [0, 39, 0xD800, 0x3042, 0x1F600].entries()) {
  suites.deriving.cases.push(["hash_character", [BigInt(which)], scalarHash(which < 3 ? 0x43 : 0x63, value, which < 3 ? 2 : 4)]);
}
for (const value of [0, 1]) suites.deriving.cases.push(["hash_boolean", [value], scalarHash(1, value, 8)]);
suites.deriving.cases.push(["hash_unit", [], BigInt.asIntN(64, hashBytes(littleEndian(2)))]);
function compositeHash(tag, components) {
  return hashBytes([...littleEndian(tag), ...littleEndian(components.length), ...components.flatMap((hash, index) => [...littleEndian(index), ...littleEndian(hash)])]);
}
function unionHash(index, payload = -1n) {
  return hashBytes([...littleEndian(0x55), ...littleEndian(index), ...littleEndian(payload)]);
}
const integerHashes = [1n, 2n, 3n].map((value) => scalarHash(0x13, value, 8));
const textHashAbc = hashBytes([...littleEndian(0x53), ...littleEndian(3), ...[97, 98, 99].flatMap((unit) => littleEndian(unit, 2))]);
const structuralHashes = [
  compositeHash(0x52, integerHashes.slice(0, 2)), unionHash(0), unionHash(1, scalarHash(0x13, 42n, 8)),
  unionHash(2, compositeHash(0x54, [scalarHash(0x23, 0x3ff8000000000000n, 8), scalarHash(0x23, 0xc004000000000000n, 8)])),
  unionHash(1, compositeHash(0x54, [unionHash(0), scalarHash(0x13, 42n, 8), unionHash(0)])),
  compositeHash(0x41, integerHashes), compositeHash(0x4c, integerHashes),
  compositeHash(0x54, [scalarHash(0x13, 42n, 8), textHashAbc]), compositeHash(0x52, []),
];
structuralHashes.forEach((hash, which) => suites.deriving.cases.push(["hash_structure", [BigInt(which)], BigInt.asIntN(64, hash)]));
suites.deriving.cases.push(["hash_default_decimal", [], scalarHash(0x33, 398n << 53n, 8)]);
// PM01: independent references for tests/fixtures/value_layout/Main.tz. A case index, never the width
// of the stored tag, is what a program sees, orders by, and hashes.
{
  const layout = suites.value_layout.cases;
  const edge = (edges, fallback) => (k) => (edges.includes(k) ? k : fallback);
  const index = { narrow: edge([255, 254, 128, 127, 1], 0), mid: edge([159, 128, 127, 1], 0), wide: edge([256, 255, 128, 1], 0), big: edge([0, 1, 2, 255, 256], 3) };
  const bigValue = (k) => ({ 0: 41, 1: 5, 2: 5, 255: 255, 256: 256 })[k] ?? 3;
  const bigHash = (k) => {
    if (index.big(k) === 0) return unionHash(0, scalarHash(0x13, 41n, 8));
    if (index.big(k) === 1) return unionHash(1, scalarHash(0x23, 0x4004000000000000n, 8));
    if (index.big(k) === 2) return unionHash(2, canonicalTextHash("owned"));
    return unionHash(index.big(k));
  };
  const order = (x, y) => BigInt((x < y ? 1 : 0) + (x === y ? 2 : 0) + (x > y ? 4 : 0));
  const picks = [0, 1, 2, 3, 126, 127, 128, 129, 159, 254, 255, 256, 257];
  for (const k of picks) {
    layout.push(["narrow", [BigInt(k)], BigInt(index.narrow(k))], ["wide", [BigInt(k)], BigInt(index.wide(k))], ["big", [BigInt(k)], BigInt(bigValue(k))]);
    layout.push(["narrow_hash", [BigInt(k)], BigInt.asIntN(64, unionHash(index.narrow(k)))], ["wide_hash", [BigInt(k)], BigInt.asIntN(64, unionHash(index.wide(k)))], ["big_hash", [BigInt(k)], BigInt.asIntN(64, bigHash(k))]);
    for (const other of picks) {
      layout.push(["mid_order", [BigInt(k), BigInt(other)], order(index.mid(k), index.mid(other))]);
      layout.push(["narrow_equal", [BigInt(k), BigInt(other)], BigInt(+(index.narrow(k) === index.narrow(other)))]);
      layout.push(["wide_equal", [BigInt(k), BigInt(other)], BigInt(+(index.wide(k) === index.wide(other)))]);
      layout.push(["big_equal", [BigInt(k), BigInt(other)], BigInt(+(index.big(k) === index.big(other)))]);
    }
  }
  for (const count of [0, 1, 300, 5000]) {
    const sum = (modulus, value) => BigInt(Array.from({ length: count }, (_, i) => value(i % modulus)).reduce((total, item) => total + item, 0));
    layout.push(["narrow_array", [BigInt(count)], sum(256, index.narrow)], ["wide_array", [BigInt(count)], sum(257, index.wide)], ["big_array", [BigInt(count)], sum(257, bigValue)]);
  }
  // The payload area: each case reads back what it was built with, in arrays whose element size is not a power of two.
  const byte = (n) => Number(BigInt.asIntN(8, BigInt(n)));
  const shapeValue = (i) => (i % 3 === 0 ? (i + 1) ** 2 : i % 3 === 1 ? 2 * i : -1);
  const smallValue = (i) => (i % 3 === 0 ? +(i % 2 === 0) : i % 3 === 1 ? byte(i) : 7);
  const references = {
    shapes: shapeValue,
    smalls: smallValue,
    wideness: (i) => (i % 3 === 0 ? i + 1000 : i % 3 === 1 ? byte(i) : -5),
    riches: (i) => (i % 4 === 0 ? 1 + String(i).length : i % 4 === 1 ? i : i % 4 === 2 ? i + 1 : -3),
    nesteds: (i) => (i % 3 === 0 ? shapeValue(i) : i % 3 === 1 ? smallValue(i) : 11),
    lanes: (i) => (i % 4 === 0 ? 6 : i % 4 === 1 ? 4 : i % 4 === 2 ? byte(i) : -1),
  };
  for (const [name, value] of Object.entries(references)) {
    for (const count of [0, 1, 2, 3, 7, 100, 300]) {
      layout.push([name, [BigInt(count)], BigInt(Array.from({ length: count }, (_, i) => value(i)).reduce((total, item) => total + item, 0))]);
    }
  }
  // The niche: a negative length is the nullary case, so every option reads back as it was built, and
  // dropping, moving out, and assigning over it free each string and array exactly once (`live == 0`).
  const width = (n) => String(n).length;
  const present = (i) => 1 + width(i);
  const options = {
    names: (i) => (i % 3 === 0 ? present(i) : -1),
    labels: (i) => (i % 2 === 0 ? present(i) : -1),
    text_options: (i) => (i % 4 === 0 ? present(i) : -2),
    number_options: (i) => (i % 3 === 1 ? (i % 5) * 1000 + (i % 5 > 0 ? i : 0) : -3),
    word_options: (i) => (i % 2 === 0 ? i % 4 : -4),
    consume: (i) => (i % 4 === 0 ? present(i) : 4),
    reassign: (i) => (i % 2 === 0 ? present(4 * i) : -2),
    holders: (i) => i + (i % 4 === 0 ? present(i) : -2) + (i % 5 === 0 ? 2 : -4),
    nested_options: (i) => (i % 3 === 0 ? -1 : i % 3 === 1 ? -2 : present(i)),
  };
  for (const [name, value] of Object.entries(options)) {
    for (const count of [0, 1, 2, 3, 7, 100, 300]) {
      layout.push([name, [BigInt(count)], BigInt(Array.from({ length: count }, (_, i) => value(i)).reduce((total, item) => total + item, 0))]);
    }
  }
  const combinators = (i) => {
    const first = i % 4 === 0 ? `s${i}!` : null;
    const second = first !== null && first.length > 3 ? `${first}?` : null;
    const third = second !== null && second.length > 4 ? second : null;
    return (third ?? "fallback").length;
  };
  for (const i of [0, 1, 2, 4, 5, 8, 12, 99, 100, 1000]) {
    layout.push(["combinators", [BigInt(i)], BigInt(combinators(i))]);
    layout.push(["sequences", [BigInt(i)], BigInt(100 + 3 + width(i) + 1)]);
    layout.push(["captured", [BigInt(i)], BigInt(2 * (i % 4 === 0 ? present(i) : -2))]);
    layout.push(["tasked", [BigInt(i)], BigInt(i % 4 === 0 ? present(i) : -2)]);
  }
  // An option of a reference: the first value above 5 and the smallest one, or the numbers that stand for none.
  for (const count of [0, 1, 2, 3, 7, 12, 100, 300]) {
    const values = Array.from({ length: count }, (_, i) => (i * 7) % 11);
    const smallest = count === 0 ? -1 : Math.min(...values);
    const first = values.find((value) => value > 5);
    layout.push(["references", [BigInt(count)], BigInt(smallest + (first === undefined ? -2 : first * 100) - 3)]);
    layout.push(["word_references", [BigInt(count)], BigInt(count > 100 ? 4000 + count : -1)]);
  }
  // Derived Eq, Ord, Hash and Display of `Tagline = Hushed | Words of string`, a union without a tag: a case is its
  // index, then its text, and a text compares by UTF-16 code units.
  const tagline = (i) => (i % 3 === 0 ? { index: 0, text: "" } : { index: 1, text: `t${i}` });
  const lineOrder = (x, y) => (x.index !== y.index ? Math.sign(x.index - y.index) : x.text < y.text ? -1 : x.text > y.text ? 1 : 0);
  for (const left of [0, 1, 2, 3, 10]) {
    for (const right of [0, 1, 2, 3, 10, 20]) {
      const order = lineOrder(tagline(left), tagline(right));
      layout.push(["tagline_flags", [BigInt(left), BigInt(right)], BigInt((order === 0 ? 1 : 2) + (order < 0 ? 4 : 0) + (order <= 0 ? 8 : 0) + (order > 0 ? 16 : 0) + (order >= 0 ? 32 : 0))]);
    }
  }
  for (const i of [0, 1, 2, 3, 10, 99]) {
    const line = tagline(i);
    layout.push(["tagline_hash", [BigInt(i)], BigInt.asIntN(64, line.index === 0 ? unionHash(0) : unionHash(1, canonicalTextHash(line.text)))]);
    layout.push(["tagline_display", [BigInt(i)], canonicalTextHash(line.index === 0 ? "Hushed" : `Words ${quotedLiteral(line.text, 0)}`)]);
  }
}
// PM01 Phase 3a: independent references for tests/fixtures/record_layout/Main.tz. Each one repeats the arithmetic
// of the source on BigInt, so that a field that is read or written at the wrong place gives another number.
{
  const records = suites.record_layout.cases;
  const gauge = (n) => ({ flag: n % 2n === 0n, count: n * 1000003n, tag: n % 100n, ratio8: 2n * n });
  const gaugeSum = (g) => (g.flag ? 1n : 0n) + g.count * 3n + g.tag * 5n + g.ratio8 * 7n;
  const spread = (i) => ({ a: i % 7n, b: i * 1000n + 17n, c: i % 5n, d: i * 3n, e: i % 3n === 0n });
  const spreadSum = (s) => s.a + s.b * 2n + s.c * 3n + s.d * 5n + (s.e ? 7n : 0n);
  const digits = (n) => BigInt(n.toString().length);
  const ledgerSum = (n, { open = n % 2n === 0n, level = n % 50n } = {}) => (open ? 1n : 0n) + (6n + digits(n)) * 10n + level * 100n + (3n * n + 3n) * 1000n;
  const rowSum = (n) => (n % 2n === 0n ? 1n : 0n) + (n % 50n) * 100n + (3n * n + 3n) * 1000n;
  const slotSum = (i) => (i % 3n === 0n ? 1n : i % 3n === 1n ? 2n + gaugeSum(gauge(i)) : 3n + gaugeSum(gauge(i)) + spreadSum(spread(i)));
  const keyedCode = ([a, b, c]) => a * 1000000n + b * 1000n + c;
  const compare = (x, y) => {
    for (let i = 0; i < 3; i++) if (x[i] !== y[i]) return x[i] < y[i] ? -1 : 1;
    return 0;
  };
  const sumOver = (count, f) => {
    let total = 0n;
    for (let i = 0n; i < count; i++) total += f(i);
    return total;
  };
  for (const n of [0n, 1n, 2n, 3n, 7n, 12n, 50n, 99n, 100n, 101n, 1000n]) {
    const g = gauge(n);
    records.push(["gauges", [n], gaugeSum(g)]);
    records.push(["updates", [n], gaugeSum({ ...g, count: g.count + 1n, tag: 9n }) * 10007n + gaugeSum(g)]);
    records.push(["captured", [n], 2n * gaugeSum(g)]);
    records.push(["tasked", [n], gaugeSum(g)]);
    records.push(["borrowed", [n], g.tag * 1000n + (g.flag ? 1n : 0n)]);
    records.push(["crates", [n], (g.flag ? 1n : 0n) + gaugeSum(g) * 3n + n * 11n * 5n + n * 13n * 7n + g.tag]);
    // The flags of the three cargos: wide first and last, the text's first, the flag's last.
    const marks = (g.flag ? 1n : 0n) + (n % 3n === 0n ? 2n : 0n) + 4n + 32n;
    records.push(["cargos", [n], n * 3n * 1000n + (5n + digits(n)) * 100n + (n % 4n === 0n ? 64n : 0n) + marks]);
    records.push(["ledgers", [n], ledgerSum(n)]);
    records.push(["moved", [n], (6n + digits(n)) * 1000n + (n % 50n) + (3n * n + 3n) * 100000n]);
    records.push(["projected", [n], (6n + digits(n)) * 10n + (n % 50n)]);
    records.push(["replaced", [n], ledgerSum(n, { open: true, level: 9n })]);
    records.push(["options", [n], g.flag ? gaugeSum(g) : -1n]);
    records.push(["lanes", [n], 8n * n * 100n + 35n]);
    // Origin: 1 + 42 * 3 + 7 * 5 + 20 * 7 = 302; Pins[0] sums to 41 and Pins[1] to 78.
    records.push(["statics", [n], 302n + (n % 2n === 0n ? 41n : 78n) * 1000n + 42n * n]);
  }
  for (const count of [0n, 1n, 2n, 5n, 33n, 100n]) {
    const keys = Array.from({ length: Number(count) }, (_, i) => [BigInt((i * 41) % 5), BigInt((i * 7919) % 101), BigInt(i % 3)]);
    const sorted = [...keys].sort(compare);
    records.push(["ordered", [count], count === 0n ? 0n : keyedCode(sorted[0]) + keyedCode(sorted[sorted.length - 1]) * 100000000n]);
    const spreads = Array.from({ length: Number(count) }, (_, i) => spread(BigInt(i)));
    const spreadTotal = spreads.reduce((total, s) => total + spreadSum(s), 0n);
    records.push(["spreads", [count], count === 0n ? spreadTotal : spreadTotal * 100n + spreads[spreads.length - 1].d]);
    records.push(["ledger_array", [count], sumOver(count, ledgerSum) + count]);
    records.push(["row_copies", [count], sumOver(count, rowSum) * 4n]);
    records.push(["slots", [count], sumOver(count, slotSum)]);
    const map = new Map();
    for (let step = 0n; step < count; step++) map.set(`${step % 2n === 0n ? 1 : 0},${step % 13n},${step % 7n}`, step);
    records.push(["keyed_map", [count], BigInt(map.size) * 1000n + (map.has("1,4,4") ? map.get("1,4,4") : -1n)]);
  }
  // Records that differ in one field, and in every field, in each position of the declared order.
  const inputs = [[0n, 5n, 3n], [1n, 5n, 3n], [1n, 4n, 3n], [1n, 5n, 2n], [2n, 1n, 0n]];
  for (const x of inputs) {
    for (const y of inputs) {
      const order = compare(x, y);
      records.push(["keyed_flags", [...x, ...y], BigInt((order === 0 ? 1 : 2) + (order < 0 ? 4 : 0) + (order <= 0 ? 8 : 0) + (order > 0 ? 16 : 0) + (order >= 0 ? 32 : 0))]);
    }
    records.push(["keyed_hash", x, BigInt.asIntN(64, compositeHash(0x52, [scalarHash(0x10, x[0], 1), scalarHash(0x13, x[1], 8), scalarHash(0x10, x[2], 1)]))]);
    records.push(["keyed_display", x, canonicalTextHash(`Keyed { a: ${x[0]}, b: ${x[1]}, c: ${x[2]} }`)]);
  }
}
function canonicalTextHash(text) {
  return BigInt.asIntN(64, hashBytes([...littleEndian(0x53), ...littleEndian(text.length), ...Array.from({ length: text.length }, (_, index) => text.charCodeAt(index)).flatMap((unit) => littleEndian(unit, 2))]));
}
function quotedLiteral(text, kind) {
  const delimiter = kind >= 2 ? "'" : '"';
  const prefix = kind % 2 ? "u8" : "";
  const escapes = new Map([[0, "0"], [9, "t"], [10, "n"], [13, "r"]]);
  const content = [...text].map((character) => {
    const scalar = character.codePointAt(0);
    if (character === delimiter || character === "\\") return `\\${character}`;
    if (escapes.has(scalar)) return `\\${escapes.get(scalar)}`;
    if (scalar < 32 || scalar === 127 || (scalar >= 0xd800 && scalar <= 0xdfff)) {
      const hex = scalar.toString(16).toUpperCase().padStart(4, "0");
      return prefix ? `\\u{${hex}}` : `\\u${hex}`;
    }
    return character;
  }).join("");
  return `${prefix}${delimiter}${content}${delimiter}`;
}
for (const value of [0, 1, 9, 10, 13, 31, 34, 39, 92, 127, 128, 0x3042, 0xd7ff, 0xd800, 0xdbff, 0xdc00, 0xdfff, 0xe000, 0xffff]) {
  suites.deriving.cases.push(["display_character", [BigInt(value)], canonicalTextHash(`Quote { value: ${quotedLiteral(String.fromCharCode(value), 2)} }`)]);
}
for (const value of [0, 1, 9, 10, 13, 39, 92, 127, 128, 0x3042, 0xffff, 0x1f600, 0x10ffff]) {
  suites.deriving.cases.push(["display_utf8_character", [BigInt(value)], canonicalTextHash(`Quote { value: ${quotedLiteral(String.fromCodePoint(value), 3)} }`)]);
}
for (const [which, text] of ["", "A\n\r\t\"\\\0\u0001\u001F\u007F", "\uD800x\uDC00", "\uD800\uD801\uDC01\uDC02x", "\u{1F600}", "'plain'"].entries()) {
  suites.deriving.cases.push(["display_text", [BigInt(which)], canonicalTextHash(`Quote { value: ${quotedLiteral(text, 0)} }`)]);
}
for (const [which, text] of ["A\n\r\t\"\\\0\u0001\u001F\u007F", "\u{1F600}"].entries()) {
  suites.deriving.cases.push(["display_utf8_text", [BigInt(which)], canonicalTextHash(`Quote { value: ${quotedLiteral(text, 1)} }`)]);
}
suites.deriving.cases.push(["display_point", [], canonicalTextHash("Point { x: 1, y: 2 }")]);
for (const [which, text] of ["First", "Number 42", "Pair (1.5, -2.5)", "Node (Leaf, 42, Leaf)", "Empty { }", '[("a\\n", 1), ("b", 2)]', '[|"a", "b"|]', "[||]", "Quote { value: Point { x: 1, y: 2 } }"].entries()) {
  suites.deriving.cases.push(["display_structure", [BigInt(which)], canonicalTextHash(text)]);
}
for (const count of [0, 1, 10000]) {
  suites.deriving.cases.push(["display_large", [BigInt(count)], canonicalTextHash(`[${Array.from({ length: count }, (_, index) => index).join(", ")}]`)]);
}

function run(name, suite) {
  const fixture = join(root, "tests/fixtures", suite.input ?? name);
  const temporary = mkdtempSync(join(tmpdir(), `tsuzuri-${name}-`));
  try {
    cli(["check", fixture]);
    const ir = join(temporary, `${name}.ll`);
    const again = join(temporary, "again.ll");
    // A `tz-` prefix keeps fixture headers from shadowing system headers.
    const headerPath = join(temporary, `tz-${name}.h`);
    cli(["build", fixture, ...(suite.options ?? []), "--emit", "header", ...allocatorOptions, "-o", headerPath]);
    cli(["build", fixture, ...(suite.options ?? []), "--emit", "llvm", ...allocatorOptions, "-o", ir]);
    cli(["build", fixture, ...(suite.options ?? []), "--emit", "llvm", ...allocatorOptions, "-o", again]);
    const sourceIr = readFileSync(ir, "utf8");
    assert.equal(sourceIr, readFileSync(again, "utf8"), `${name}: IR is deterministic`);
    const declarations = sourceIr.match(/^declare .*$/gm) ?? [];
    const declaredNames = declarations.map((line) => line.match(/@([^\s(]+)\(/)[1]);
    assert.equal(new Set(declaredNames).size, declaredNames.length, `${name}: declarations are deduplicated`);
    suite.inspect?.(sourceIr, readFileSync(headerPath, "utf8"));
    // The host allocator's IR calls only tsuzuri_host_*, so nothing is renamed.
    if (hostAllocator) assert.ok(!/@(malloc|free|realloc)\(/.test(sourceIr), `${name}: the host allocator replaces the C library`);
    let trackedIr = hostAllocator ? sourceIr : sourceIr.replaceAll("@malloc", "@tracked_alloc").replaceAll("@free", "@tracked_free").replaceAll("@realloc", "@tracked_realloc");
    if (sanitizerKind) trackedIr = trackedIr.replace(/ nounwind(?=[^{}\n]* \{)/g, ` nounwind sanitize_${sanitizerKind}`);
    writeFileSync(ir, trackedIr);
    const traps = suite.traps ?? [];
    // Entries that must finish at once (a loop over an empty window). They are not in `cases`: native, each one runs in
    // its own process and WASM in a child Node process, both under a SIGKILL timer, never in this process.
    const bounded = suite.bounded ?? [];
    const host = join(temporary, "host.c");
    writeFileSync(host, `
#undef NDEBUG
#include <assert.h>
#include <math.h>
#include <stdint.h>
#include <stdatomic.h>
#include <stdlib.h>
#include "tz-${name}.h"
static _Atomic uint64_t live;
${smallAllocator && sourceIr.includes("@tz_small_stats(") ? `void tz_small_stats(uint64_t *out);
/* The size classes' own malloc blocks (regions, leaves, caches) stay live; out[4] is their size. */
static int balanced(void) {
    uint64_t stats[5];
    tz_small_stats(stats);
    return stats[0] == stats[1] && live == stats[4];
}
#define NO_LEAK balanced()` : "#define NO_LEAK (live == 0)"}
void *tracked_alloc(uint64_t size) {
    uint64_t *p = malloc((size_t)size + 16);
    assert(p);
    p[0] = size;
    p[1] = UINT64_C(0x51a110ca7e);
    live += size;
    return p + 2;
}
void tracked_free(void *value) {
    if (!value) return;
    uint64_t *p = (uint64_t *)value - 2;
    assert(p[1] == UINT64_C(0x51a110ca7e));
    p[1] = 0;
    assert(live >= p[0]);
    live -= p[0];
    free(p);
}
  void *tracked_realloc(void *value, uint64_t size) {
    if (!value) return tracked_alloc(size);
    if (!size) { tracked_free(value); return NULL; }
    uint64_t *previous = (uint64_t *)value - 2;
    assert(previous[1] == UINT64_C(0x51a110ca7e));
    uint64_t old_size = previous[0];
    uint64_t *next = realloc(previous, (size_t)size + 16);
    assert(next);
    next[0] = size;
    next[1] = UINT64_C(0x51a110ca7e);
    if (size >= old_size) live += size - old_size; else live -= old_size - size;
    return next + 2;
  }
${hostAllocator ? `void *tsuzuri_host_alloc(uint64_t size, uint64_t align) {
    assert(align == 16 && size >= 16);
    return tracked_alloc(size);
}
void tsuzuri_host_free(void *value, uint64_t size, uint64_t align) {
    assert(value && align == 16 && ((uint64_t *)value)[-2] == size);
    tracked_free(value);
}
void *tsuzuri_host_realloc(void *value, uint64_t old_size, uint64_t new_size, uint64_t align) {
    assert(value && align == 16 && new_size >= 16 && ((uint64_t *)value)[-2] == old_size);
    return tracked_realloc(value, new_size);
}` : ""}
int main(int argc, char **argv) {
    if (argc == 2) {
        switch (atoi(argv[1])) {
            ${traps.map(([trap, args], index) => `case ${index}: (void)tz_${trap}(${args.map(cValue).join(", ")}); break;`).join("\n")}
            ${bounded.map(([exported, args, expected], index) => `case ${traps.length + index}: return tz_${exported}(${args.map(cValue).join(", ")}) == ${cValue(expected)} && NO_LEAK ? 0 : 1;`).join("\n")}
        }
        return 0;
    }
    ${[...suite.cases, ...suite.nativeCases ?? []].map(([exported, args, expected]) =>
    `assert(tz_${exported}(${args.map(cValue).join(", ")}) == ${cValue(expected)}); assert(NO_LEAK);`).join("\n    ")}
    return 0;
}
`);
    for (const optimization of ["0", "3"]) {
      const native = join(temporary, `${name}-O${optimization}${executableSuffix}`);
      execute(clang, [`-O${optimization}`, "-Wno-override-module", "-ffp-contract=off", ...sanitizer, ...nativeOptions,
        `-I${temporary}`, ir, host, ...(sourceIr.includes("declare void @tsuzuri_task_parallel(") ? [join(root, "src/runtime/task.c"), "-pthread"] : []), ...nativeLibraries, "-o", native]);
      execute(native, []);
      for (const [index, [exported]] of bounded.entries()) {
        const result = executeBounded(native, [String(traps.length + index)]);
        assert.equal(result.status, 0, `${name} native O${optimization}: ${exported} returned a wrong value or leaked`);
      }
      if ((name === "parallel" || name === "matrix") && optimization === "3") {
        for (const processors of [1, 4]) {
          const runtime = join(temporary, `runtime-${processors}.c`);
          const binary = join(temporary, `parallel-cpus-${processors}${executableSuffix}`);
          writeFileSync(runtime, `#define TZ_TASK_SYSCONF(name) ${processors}\n#include ${JSON.stringify(join(root, "src/runtime/task.c"))}\n`);
          execute(clang, ["-O3", "-Wno-override-module", "-ffp-contract=off", ...sanitizer,
            `-I${temporary}`, ir, host, runtime, "-pthread", ...nativeLibraries, "-o", binary]);
          execute(binary, []);
        }
      }
      for (let index = 0; index < traps.length; index++) {
        const result = execute(native, [String(index)], false);
        assert.ok(result.status !== 0 || result.signal, `${name} native O${optimization}: ${traps[index][0]} must trap`);
      }
      const wasm = join(temporary, `${name}-O${optimization}.wasm`);
      cli(["build", fixture, ...(suite.options ?? []), "--target", wasmTarget, ...wasmOptions, `-O${optimization}`, "-o", wasm]);
      const module = new WebAssembly.Module(readFileSync(wasm));
      assert.deepEqual(WebAssembly.Module.imports(module), [], `${name}: WASM has no imports`);
      const exports = new WebAssembly.Instance(module).exports;
      for (const [exported, args, expected] of suite.cases) {
        assert.equal(exports[`tz_${exported}`](...args), expected, `${name} WASM O${optimization}: ${exported}(${args})`);
        const memoryLimit = suite.wasmMemory ?? 16 * 1024 * 1024;
        assert.ok(exports.memory.buffer.byteLength <= memoryLimit, `${name}: WASM stays within ${memoryLimit / 1048576} MiB`);
      }
      for (const [exported, args, expected] of bounded) {
        const result = executeBounded(process.execPath, ["-e", boundedWasmCall, wasm, `tz_${exported}`, ...args.map(String)]);
        assert.equal(result.status, 0, `${name} WASM O${optimization}: ${exported}\n${result.stderr}`);
        assert.equal(result.stdout.trim(), String(expected), `${name} WASM O${optimization}: ${exported}`);
      }
      for (const [trap, args] of [...traps, ...suite.wasmTraps ?? []]) {
        const fresh = new WebAssembly.Instance(module).exports;
        assert.throws(() => fresh[`tz_${trap}`](...args), WebAssembly.RuntimeError, trap);
      }
      if (suite.trapKind && wasmTarget === "wasm32") {
        // The kind of each trap: a violated precondition must trap as an assertion, before any allocation,
        // element access or callback. An entry may name its own kind as a third element.
        const traced = join(temporary, `${name}-trap-kinds-O${optimization}.wasm`);
        cli(["build", fixture, ...(suite.options ?? []), "--target", wasmTarget, ...wasmOptions, "--trap-info", `-O${optimization}`, "-o", traced]);
        const { sites } = JSON.parse(readFileSync(`${traced}.trap.json`, "utf8"));
        const boundary = createBoundary(new WebAssembly.Module(readFileSync(traced)), { sites });
        for (const [trap, args, kind = suite.trapKind] of traps) {
          const result = boundary.call(`tz_${trap}`, ...args);
          assert.equal(result.ok, false, `${name} O${optimization}: ${trap}(${args}) must trap`);
          assert.equal(result.trap.kind, kind, `${name} O${optimization}: ${trap}(${args}) trapped with the wrong kind`);
        }
      }
      if (name === "recursive_types") {
        const measured = join(temporary, `recursive-traps-${optimization}.wasm`);
        cli(["build", fixture, ...(suite.options ?? []), "--target", wasmTarget, ...wasmOptions, "--trap-info", `-O${optimization}`, "-o", measured]);
        const checked = new WebAssembly.Instance(new WebAssembly.Module(readFileSync(measured))).exports;
        assert.equal(checked.tz_deep_clone(50000n), 100003n);
        assert.equal(checked.tz_mutual_clone(20000n), 43n);
        assert.throws(() => checked.tz_deep_drop(1000000n), WebAssembly.RuntimeError);
        assert.ok(checked.tsuzuri_trap_site() > 0);
      }
    }
    return suite.cases.length + bounded.length;
  } finally {
    rmSync(temporary, { recursive: true, force: true });
  }
}

function debugOutputChecks() {
  const directory = mkdtempSync(join(tmpdir(), "tsuzuri-debug-output-"));
  const fixture = join(root, "tests/fixtures/debug_output");
  try {
    for (const optimization of [0, 3]) {
      const native = join(directory, `debug-${optimization}`);
      cli(["build", fixture, `-O${optimization}`, "-o", native]);
      const result = execute(native, []);
      assert.equal(result.stdout, "42\n");
      assert.equal(result.stderr, "hello\0\u{1f600}\n42\n");
      const wasm = join(directory, `debug-${optimization}.wasm`);
      cli(["build", fixture, "--target", "wasm32", "--debug-output", `-O${optimization}`, "-o", wasm]);
      const module = new WebAssembly.Module(readFileSync(wasm));
      assert.deepEqual(WebAssembly.Module.imports(module), [{ module: "tsuzuri_debug", name: "write", kind: "function" }]);
      const lines = [];
      let instance;
      instance = new WebAssembly.Instance(module, { tsuzuri_debug: { write(pointer, length) {
        lines.push(Buffer.from(new Uint8Array(instance.exports.memory.buffer, pointer, Number(length))).toString("utf8"));
      } } });
      assert.equal(instance.exports.tz_debug_answer(), 42n);
      assert.deepEqual(lines, ["hello\0\u{1f600}", "42"]);
      assert.throws(() => instance.exports.tz_debug_display_trap(), WebAssembly.RuntimeError);
    }
    const unused = join(directory, "Unused.tz"), base = join(directory, "unused.wasm"), opted = join(directory, "unused-opted.wasm");
    writeFileSync(unused, "export def value :: i64\nfn value = 1");
    cli(["build", unused, "--target", "wasm32", "-o", base]);
    cli(["build", unused, "--target", "wasm32", "--debug-output", "-o", opted]);
    assert.deepEqual(readFileSync(base), readFileSync(opted));
    const parallel = mkdtempSync(join(directory, "parallel-"));
    const parallelSource = join(parallel, "Main.tz");
    writeFileSync(parallelSource, "let tasks = new [Task<i64>](16, \\index -> task { return Debug.trace index })\nlet results = Task.run (Task.parallel tasks)\nArray.sum (ref results)");
    for (const optimization of [0, 3]) {
      const native = join(directory, `parallel-${optimization}`);
      cli(["build", parallelSource, `-O${optimization}`, "-o", native]);
      const result = execute(native, []);
      assert.equal(result.stdout, "120\n");
      assert.equal(result.stderr.split("\n").length - 1, 16);
      assert.equal(result.stderr.replaceAll("\n", "").split("").sort().join(""), Array.from({ length: 16 }, (_, index) => String(index)).join("").split("").sort().join(""));
    }
    const ir = join(directory, "short-write.ll"), host = join(directory, "short-write.c");
    const runtime = readFileSync(join(root, "src/runtime/debug.ll"), "utf8").replaceAll("@write(", "@mock_write(").replaceAll("@tz.free(", "@mock_free(");
    writeFileSync(ir, `%tz.utf8string = type { ptr, i64 }\ndeclare void @llvm.trap()\ndeclare void @mock_free(ptr)\n@input = private constant [6 x i8] c"ab\\00cde"\n${runtime}\ndefine void @probe() { call void @tz.debug.write(%tz.utf8string { ptr @input, i64 6 }) ret void }\n`);
    writeFileSync(host, `#include <assert.h>\n#include <stdint.h>\n#include <string.h>\nstatic char output[7]; static uint64_t length; static int mode, freed;\nextern void probe(void);\nint64_t mock_write(int fd, const char *text, uint64_t size) { assert(fd == 2); if(mode == 1) return 0; if(mode == 2) return -1; uint64_t count = size > 2 ? 2 : size; assert(length + count <= 7); memcpy(output + length, text, count); length += count; return count; }\nvoid mock_free(void *pointer) { assert(pointer); ++freed; }\nint main(int argc, char **argv) { mode = argc > 1 ? argv[1][0] - '0' : 0; probe(); assert(length == 7 && memcmp(output, "ab\\0cde\\n", 7) == 0 && freed == 1); return 0; }\n`);
    for (const optimization of [0, 3]) {
      const native = join(directory, `short-${optimization}`);
      execute(clang, [`-O${optimization}`, "-Wno-override-module", ir, host, "-o", native]);
      execute(native, []);
      for (const mode of ["1", "2"]) {
        const result = execute(native, [mode], false);
        assert.notEqual(result.status, 0);
        assert.notEqual(result.error?.code, "ETIMEDOUT");
      }
    }
  } finally { rmSync(directory, { recursive: true, force: true }); }
}

function characterConsoleChecks() {
  const directory = mkdtempSync(join(tmpdir(), "tsuzuri-character-console-"));
  const source = join(directory, "Main.tz");
  try {
    for (const optimization of [0, 3]) {
      for (const [literal, expected] of [["'\\0'", "\0"], ["'\\u3042'", "\u3042"], ["u8'\\u{3042}'", "\u3042"], ["u8'\\u{1F600}'", "\u{1f600}"], ["u8'\\u{10FFFF}'", "\u{10ffff}"]]) {
        writeFileSync(source, literal);
        assert.equal(cli(["run", source, `-O${optimization}`]).stdout, `${expected}\n`);
      }
      for (const literal of ["'\\uD800'", "'\\uDFFF'"]) {
        writeFileSync(source, literal);
        const result = execute(compiler, ["run", source, `-O${optimization}`], false);
        assert.notEqual(result.status, 0);
        assert.equal(result.stdout, "");
      }
    }
  } finally { rmSync(directory, { recursive: true, force: true }); }
}

function wasmReallocationChecks() {
  const directory = mkdtempSync(join(tmpdir(), "tsuzuri-realloc-"));
  try {
    const input = join(directory, "realloc.ll");
    writeFileSync(input, `declare void @llvm.trap()\n${readFileSync(join(root, "src/runtime/heap-wasm.ll"), "utf8")}
define ptr @allocate(i64 %size) { %value = call ptr @tz.alloc(i64 %size) ret ptr %value }
define ptr @resize(ptr %old, i64 %previous, i64 %size) { %value = call ptr @tz.realloc(ptr %old, i64 %previous, i64 %size) ret ptr %value }
define void @release(ptr %value) { call void @tz.free(ptr %value) ret void }
`);
    for (const optimization of [0, 3]) {
      const object = join(directory, `realloc-${optimization}.o`), output = join(directory, `realloc-${optimization}.wasm`);
      execute(clang, ["--target=wasm32", "-mbulk-memory", `-O${optimization}`, "-Wno-override-module", "-c", input, "-o", object]);
      execute(process.env.TSUZURI_WASM_LD ?? "wasm-ld", [object, "--no-entry", "--export=allocate", "--export=resize", "--export=release", "--export-memory", "--max-memory=16777216", "-o", output]);
      const module = new WebAssembly.Module(readFileSync(output));
      assert.deepEqual(WebAssembly.Module.imports(module), []);
      const fresh = () => new WebAssembly.Instance(module).exports;
      for (const neighborSize of [32n, 128n]) {
        const api = fresh();
        const original = api.allocate(32n), neighbor = api.allocate(neighborSize), guard = api.allocate(16n);
        new Uint8Array(api.memory.buffer, original, 32).fill(0x5a);
        api.release(neighbor);
        const grown = api.resize(original, 32n, neighborSize === 32n ? 70n : 80n);
        assert.equal(grown, original);
        assert.ok(new Uint8Array(api.memory.buffer, grown, 32).every((value) => value === 0x5a));
        if (neighborSize === 128n) {
          const remainder = api.allocate(64n);
          assert.equal(remainder, original + 96);
          api.release(remainder);
        }
        api.release(grown);
        api.release(guard);
        const combined = api.allocate(neighborSize === 32n ? 100n : 200n);
        assert.equal(combined, original);
        api.release(combined);
      }
      {
        const api = fresh();
        const original = api.allocate(32n), blocker = api.allocate(32n);
        new Uint8Array(api.memory.buffer, original, 32).fill(0x6b);
        const moved = api.resize(original, 32n, 96n);
        assert.notEqual(moved, original);
        assert.ok(new Uint8Array(api.memory.buffer, moved, 32).every((value) => value === 0x6b));
        api.release(blocker);
        api.release(moved);
      }
      {
        const api = fresh();
        assert.equal(api.resize(0, 0n, 0n), 0);
        const allocated = api.resize(0, 0n, 32n);
        assert.notEqual(allocated, 0);
        assert.equal(api.resize(allocated, 32n, 16n), allocated);
        assert.equal(api.resize(allocated, 16n, 0n), 0);
        assert.equal(api.allocate(32n), allocated);
        assert.throws(() => api.resize(0, 0n, 16777185n), WebAssembly.RuntimeError);
      }
    }
  } finally { rmSync(directory, { recursive: true, force: true }); }
  console.log("WASM realloc: split, absorb, fallback, null, zero and coalescing passed at O0/O3");
}

// The heap alone, against the design's own arithmetic: tests/wasm_heap.mjs (wasm64 too when the suites run on it).
async function wasmHeapChecks() {
  const { memory64Supported, runHeapTests } = await import("./wasm_heap.mjs");
  runHeapTests({ targets: wasmTarget === "wasm64" && memory64Supported() ? ["wasm32", "wasm64"] : ["wasm32"] });
}

// Static data on WASM threads (D11 Phase 2): the workers read the one read-only table in the shared
// memory, and a table read in a worker loop returns the same values as in the single-threaded build.
async function constStaticsWasmThreadsChecks() {
  if (wasmTarget !== "wasm32") return;
  const { createThreadPool } = await import("../src/runtime/wasm-threads.mjs");
  const directory = mkdtempSync(join(tmpdir(), "tsuzuri-statics-threads-"));
  try {
    for (const optimization of [0, 3]) {
      const wasm = join(directory, `statics-threads-${optimization}.wasm`);
      cli(["build", join(root, "tests/fixtures/const_statics"), "--target", "wasm32", "--wasm-feature", "threads", `-O${optimization}`, "-o", wasm]);
      const pool = await createThreadPool(readFileSync(wasm), { workers: 3 });
      try {
        assert.equal(pool.call("tz_closures_and_tasks"), 9n + 41n * 3n + 41n + 103n);
        assert.equal(pool.call("tz_squares_sum", 4096n), 4095n * 4096n * 8191n / 6n);
        assert.equal(pool.call("tz_primes_total"), 41n * 3n + 2n);
        assert.equal(pool.call("tz_bytes_at", 2n), 255n);
        assert.equal(pool.workerCount, 3);
      } finally {
        await pool.close();
      }
    }
  } finally { rmSync(directory, { recursive: true, force: true }); }
  console.log("const_statics: static data on WASM threads at O0/O3 passed");
}

// Arc counts with atomic instructions on WASM threads: the tasks share and drop the counts on
// worker threads, and the heap returns to the worker stacks alone.
async function rcWasmThreadsChecks() {
  if (wasmTarget !== "wasm32") return;
  const { createThreadPool } = await import("../src/runtime/wasm-threads.mjs");
  const directory = mkdtempSync(join(tmpdir(), "tsuzuri-rc-threads-"));
  try {
    for (const optimization of [0, 3]) {
      const wasm = join(directory, `rc-threads-${optimization}.wasm`);
      cli(["build", join(root, "tests/fixtures/rc"), "--target", "wasm32", "--wasm-feature", "threads", `-O${optimization}`, "-o", wasm]);
      const pool = await createThreadPool(readFileSync(wasm), { workers: 3 });
      try {
        assert.equal(pool.call("tz_arc_parallel", 4097n), 4097n * 4096n / 2n * 10n + 1n);
        assert.equal(pool.workerCount, 3);
        assert.equal(pool.call("tz_arc_parallel_last", 100000n), 100000n * 99999n / 2n);
        assert.equal(pool.call("tz_arc_chain_parallel", 20000n), 4n * (20000n * 19999n / 2n));
        assert.equal(pool.call("tz_arc_weak_parallel", 1000n), 4n * (1000n * 999n / 2n) * 10n);
        assert.equal(pool.call("tsuzuri_thread_heap_live_bytes"), 3n * (262144n + 16n));
      } finally {
        await pool.close();
      }
    }
  } finally { rmSync(directory, { recursive: true, force: true }); }
  console.log("rc: Arc on WASM threads at O0/O3 passed");
}

// Matrix.mul_parallel on WASM threads: the workers share the copied operands and write disjoint row
// chunks, and every result equals the sequential product bit for bit (the fixture counts mismatches).
async function matrixWasmThreadsChecks() {
  if (wasmTarget !== "wasm32") return;
  const { createThreadPool } = await import("../src/runtime/wasm-threads.mjs");
  const directory = mkdtempSync(join(tmpdir(), "tsuzuri-matrix-threads-"));
  try {
    for (const optimization of [0, 3]) {
      const wasm = join(directory, `matrix-threads-${optimization}.wasm`);
      cli(["build", join(root, "tests/fixtures/matrix"), "--target", "wasm32", "--wasm-feature", "threads", `-O${optimization}`, "-o", wasm]);
      const pool = await createThreadPool(readFileSync(wasm), { workers: 3 });
      try {
        for (const [rows, inner, cols] of [[129n, 128n, 64n], [3n, 500n, 1500n], [64n, 64n, 64n]]) {
          assert.equal(pool.call("tz_parallel_mismatches64", rows, inner, cols, 1n, 10n), 0n);
          assert.equal(pool.call("tz_parallel_mismatches32", rows, inner, cols, 1n, 13n), 0n);
          assert.equal(pool.call("tz_integer_parallel", rows, inner, cols, 5n), 0n);
        }
        assert.equal(pool.call("tz_kernel_entry64", 129n, 128n, 64n, 0n, 1n, 128n * 64n), productEntry(128, 0, 64, false, 128, 0));
        assert.equal(pool.call("tz_kernel_entry64", 129n, 128n, 64n, 0n, 3n, 128n * 64n), productEntry(128, 0, 64, true, 128, 0));
        assert.equal(pool.workerCount, 3);
      } finally {
        await pool.close();
      }
    }
  } finally { rmSync(directory, { recursive: true, force: true }); }
  console.log("matrix: mul_parallel on WASM threads at O0/O3 passed");
}

let total = 0;
for (const [name, suite] of Object.entries(suites)) {
  if (only && only !== name) continue;
  total += run(name, suite);
  if (name === "rc") await rcWasmThreadsChecks();
  if (name === "const_statics") await constStaticsWasmThreadsChecks();
  if (name === "matrix") await matrixWasmThreadsChecks();
  if (name === "vec") { wasmReallocationChecks(); await wasmHeapChecks(); }
  if (name === "chars") characterConsoleChecks();
  if (name === "debug_output") debugOutputChecks();
  console.log(`${name}: native/WASM at O0/O3 passed`);
}
// The expensive references are computed only for the suites that run (PR #75 review).
for (const name of ["monoid", "parallel_equivalence"]) {
  if (only && only !== name) assert.equal(suites[name].computed, undefined, `${name} computed its cases for ${only}`);
}
assert.ok(total > 0, "no feature suite ran");
console.log(`features: ${total} cases passed`);
