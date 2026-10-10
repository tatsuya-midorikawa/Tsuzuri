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

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const compiler = resolve(process.argv[2] ?? join(root, "target/debug/tsuzuri"));
const only = process.argv[3];
const clang = process.env.TSUZURI_CLANG ?? "clang";
const sanitizerKind = process.env.TSUZURI_TSAN === "1" ? "thread" : process.env.TSUZURI_ASAN === "1" ? "address" : null;
const sanitizer = sanitizerKind ? [`-fsanitize=${sanitizerKind}`] : [];
const nativeOptions = process.env.TSUZURI_TEST_CPU === "native" ? [process.arch === "arm64" ? "-mcpu=native" : "-march=native"] : [];
const wasmOptions = process.env.TSUZURI_TEST_WASM_SIMD === "1" ? ["--wasm-feature", "simd128"] : [];
// wasm64 needs Node.js 24 or newer.
const wasmTarget = process.env.TSUZURI_TEST_WASM_TARGET ?? "wasm32";
// F13: `host` builds the native IR and header with --allocator host and tracks tsuzuri_host_*.
const hostAllocator = process.env.TSUZURI_TEST_ALLOCATOR === "host";
const allocatorOptions = hostAllocator ? ["--allocator", "host"] : [];
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

const suites = {
  higher_kinds: {
    cases: [
      ...[min, -1n, 0n, 41n, max].flatMap(seed => [["option_map", [seed], BigInt.asIntN(64, seed + 1n)], ["result_map", [seed], BigInt.asIntN(64, seed * 3n)]]),
      ["result_error", [], 6n], ["all_constructors", [], 210n], ["binary_constructor", [], 42n], ["first_class", [], 42n],
      ...[0n, 1n, 10000n].map(count => ["owned_option", [count], count * 6n]),
    ],
    inspect(ir) { assert.doesNotMatch(ir, /%tz\.hkt|dictionary/); },
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
        assert.match(line, /, align (?:1|16)(?:, !|$)/, line);
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
      const lookups = [...ir.matchAll(/^define internal [^\n]*@tz\.fn\.Map\.(?:lower_bound|found|get|at|contains_key)[^\n]*\{([\s\S]*?)^\}/gm)];
      assert.ok(lookups.length > 0);
      for (const [, body] of lookups) assert.doesNotMatch(body, /@tz\.(?:alloc|realloc)\(/);
    },
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
    ],
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
        assert.equal((body.match(/phi ptr \[[^\n]+\], \[/g) ?? []).length, 2, "both list cursors advance in lockstep");
        assert.ok(!body.includes("@tz.alloc") && !body.includes("@tz.list.index"));
      }
    },
  },
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
      // List.fold_ref with a state aligned more strictly than the elements reads each element with
      // the list's node layout.
      ["fold_ref_union_state", [], (() => {
        const digits = [3n, 4n].reduce((total, value) => total * 10n + value, 0n);
        const lengths = [[1, 2], [3, 4, 5]].reduce((total, row) => total * 10n + BigInt(row.length), 0n);
        const joined = BigInt(["ab", "cde"].join("").length);
        const rendered = BigInt([3, 4].map(String).join(",").length) * 1000n + 2n;
        return digits * 1000000n + lengths * 10000n + joined * 100000000n + rendered;
      })()],
      ["list_iter_aligned", [], [5n, 7n].reduce((total, value) => total * 10n + value, 0n) * 1000n
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
        '%"tz.union.Main.Shape" = type { i32, [1 x i128] }',
        '%"tz.union.Main.Color" = type i32',
        '%"tz.union.Main.Pick" = type { i32, i64 }',
        '%"tz.union.Main.Maybe[i64]" = type { i32, i64 }',
        '%"tz.union.Main.Maybe[string]" = type { i32, %tz.string }',
        '%"tz.union.Shapes.Shape" = type { i32, double }',
      ]) {
        assert.equal(ir.split(`${definition}\n`).length, 2, `${definition} is defined once`);
      }
      assert.doesNotMatch(ir, /tz\.union\.Main\.Maybe = type/);
      assert.match(ir, /switch i32/);
      // First-class constructors become one hidden function per concrete instance.
      const constructors = ir.match(/^define internal \S+ @tz\.fn\.\$case\.Main\.Maybe\.Some\.\$mono\.\d+\(/gm) ?? [];
      assert.equal(constructors.length, 2, "Maybe.Some has an i64 and a string constructor function");
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
      for (const definition of [
        "define internal double @tz.fn.Math.zero(",
        "define internal double @tz.fn.Math.identity_f64(",
        "define internal i64 @tz.builtin.unreachable.i64(i8 %unit) noreturn nounwind {",
      ]) {
        assert.equal(ir.split(definition).length, 2, `${definition} is defined once`);
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
      assert.match(ir, /tz\.union\.Maybe\.Maybe\[string\]/);
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
    inspect(ir) { assert.doesNotMatch(ir, /llvm\.assume|!range| nsw | nuw /); },
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
  const fixture = join(root, "tests/fixtures", name);
  const temporary = mkdtempSync(join(tmpdir(), `tsuzuri-${name}-`));
  try {
    cli(["check", fixture]);
    const ir = join(temporary, `${name}.ll`);
    const again = join(temporary, "again.ll");
    // A `tz-` prefix keeps fixture headers from shadowing system headers.
    const headerPath = join(temporary, `tz-${name}.h`);
    cli(["build", fixture, "--emit", "header", ...allocatorOptions, "-o", headerPath]);
    cli(["build", fixture, "--emit", "llvm", ...allocatorOptions, "-o", ir]);
    cli(["build", fixture, "--emit", "llvm", ...allocatorOptions, "-o", again]);
    const sourceIr = readFileSync(ir, "utf8");
    assert.equal(sourceIr, readFileSync(again, "utf8"), `${name}: IR is deterministic`);
    const declarations = sourceIr.match(/^declare .*$/gm) ?? [];
    assert.equal(new Set(declarations).size, declarations.length, `${name}: declarations are deduplicated`);
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
#include <assert.h>
#include <math.h>
#include <stdint.h>
#include <stdatomic.h>
#include <stdlib.h>
#include "tz-${name}.h"
static _Atomic uint64_t live;
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
            ${bounded.map(([exported, args, expected], index) => `case ${traps.length + index}: return tz_${exported}(${args.map(cValue).join(", ")}) == ${cValue(expected)} && live == 0 ? 0 : 1;`).join("\n")}
        }
        return 0;
    }
    ${[...suite.cases, ...suite.nativeCases ?? []].map(([exported, args, expected]) =>
    `assert(tz_${exported}(${args.map(cValue).join(", ")}) == ${cValue(expected)}); assert(live == 0);`).join("\n    ")}
    return 0;
}
`);
    for (const optimization of ["0", "3"]) {
      const native = join(temporary, `${name}-O${optimization}`);
      execute(clang, [`-O${optimization}`, "-Wno-override-module", "-ffp-contract=off", ...sanitizer, ...nativeOptions,
        `-I${temporary}`, ir, host, ...(sourceIr.includes("declare void @tsuzuri_task_parallel(") ? [join(root, "src/runtime/task.c"), "-pthread"] : []), "-lm", "-o", native]);
      execute(native, []);
      for (const [index, [exported]] of bounded.entries()) {
        const result = executeBounded(native, [String(traps.length + index)]);
        assert.equal(result.status, 0, `${name} native O${optimization}: ${exported} returned a wrong value or leaked`);
      }
      if ((name === "parallel" || name === "matrix") && optimization === "3") {
        for (const processors of [1, 4]) {
          const runtime = join(temporary, `runtime-${processors}.c`);
          const binary = join(temporary, `parallel-cpus-${processors}`);
          writeFileSync(runtime, `#define TZ_TASK_SYSCONF(name) ${processors}\n#include ${JSON.stringify(join(root, "src/runtime/task.c"))}\n`);
          execute(clang, ["-O3", "-Wno-override-module", "-ffp-contract=off", ...sanitizer,
            `-I${temporary}`, ir, host, runtime, "-pthread", "-lm", "-o", binary]);
          execute(binary, []);
        }
      }
      for (let index = 0; index < traps.length; index++) {
        const result = execute(native, [String(index)], false);
        assert.ok(result.status !== 0 || result.signal, `${name} native O${optimization}: ${traps[index][0]} must trap`);
      }
      const wasm = join(temporary, `${name}-O${optimization}.wasm`);
      cli(["build", fixture, "--target", wasmTarget, ...wasmOptions, `-O${optimization}`, "-o", wasm]);
      const module = new WebAssembly.Module(readFileSync(wasm));
      assert.deepEqual(WebAssembly.Module.imports(module), [], `${name}: WASM has no imports`);
      const exports = new WebAssembly.Instance(module).exports;
      for (const [exported, args, expected] of suite.cases) {
        assert.equal(exports[`tz_${exported}`](...args), expected, `${name} WASM O${optimization}: ${exported}(${args})`);
        assert.ok(exports.memory.buffer.byteLength <= 16 * 1024 * 1024, `${name}: WASM stays within 16 MiB`);
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
        cli(["build", fixture, "--target", wasmTarget, ...wasmOptions, "--trap-info", `-O${optimization}`, "-o", traced]);
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
        cli(["build", fixture, "--target", wasmTarget, ...wasmOptions, "--trap-info", `-O${optimization}`, "-o", measured]);
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
    writeFileSync(parallelSource, "let tasks = new [Task<i64>](16, index -> task { return Debug.trace index })\nlet results = Task.run (Task.parallel tasks)\nArray.sum (ref results)");
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
      execute(clang, ["--target=wasm32", `-O${optimization}`, "-Wno-override-module", "-c", input, "-o", object]);
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
  if (name === "matrix") await matrixWasmThreadsChecks();
  if (name === "vec") wasmReallocationChecks();
  if (name === "chars") characterConsoleChecks();
  if (name === "debug_output") debugOutputChecks();
  console.log(`${name}: native/WASM at O0/O3 passed`);
}
assert.ok(total > 0, "no feature suite ran");
console.log(`features: ${total} cases passed`);
