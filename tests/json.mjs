// Conformance of `Json.parse` and `Json.to_utf8string` with RFC 8259, checked against
// JavaScript's JSON.parse/JSON.stringify and hand-computed expectations (D08).
// Usage: node tests/json.mjs target/release/tsuzuri
import assert from "node:assert/strict";
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const compiler = resolve(process.argv[2] ?? join(root, "target/debug/tsuzuri"));
const clang = process.env.TSUZURI_CLANG ?? "clang";

function execute(program, args) {
  const result = spawnSync(program, args, { cwd: root, encoding: "utf8", timeout: 600_000, maxBuffer: 64 * 1024 * 1024 });
  if (result.error) throw result.error;
  assert.equal(result.status, 0, `${program} ${args.join(" ")}\n${result.stdout}\n${result.stderr}`);
  return result;
}

function fnv64(bytes) {
  let hash = 14695981039346656037n;
  for (const byte of bytes) hash = ((hash ^ BigInt(byte)) * 1099511628211n) & ((1n << 64n) - 1n);
  return BigInt.asIntN(64, hash);
}
const encoder = new TextEncoder();
const hashText = (text) => fnv64(encoder.encode(text));
const status = (kind, offset) => BigInt(kind) * (1n << 40n) + BigInt(offset);
const [Syntax, UnexpectedEnd, InvalidEscape, ControlCharacter, DuplicateKey, TooDeep] = [1, 2, 3, 4, 5, 6];

// CBOR (RFC 8949 §4.2.1) of a parsed value whose numbers are JavaScript's canonical text: an integer
// text is an integer (a bignum beyond 64 bits), any other number the shortest float that is exact.
const LoneSurrogate = 13;
function cborHead(major, value) {
  const n = BigInt(value);
  if (n < 24n) return [major << 5 | Number(n)];
  const size = n < 256n ? 1 : n < 65536n ? 2 : n < 4294967296n ? 4 : 8;
  return [major << 5 | { 1: 24, 2: 25, 4: 26, 8: 27 }[size], ...Array.from({ length: size }, (_, index) => Number(n >> BigInt(8 * (size - 1 - index)) & 255n))];
}
function cborHalf(x) {
  if (x === 0) return Object.is(x, -0) ? 0x8000 : 0;
  const sign = x < 0 ? 0x8000 : 0, magnitude = Math.abs(x);
  for (let exponent = -14; exponent <= 15; exponent++) {
    if (magnitude >= 2 ** exponent && magnitude < 2 ** (exponent + 1)) {
      const fraction = (magnitude / 2 ** exponent - 1) * 1024;
      return Number.isInteger(fraction) ? sign | (exponent + 15) << 10 | fraction : null;
    }
  }
  const units = magnitude / 2 ** -24;
  return Number.isInteger(units) && units < 1024 ? sign | units : null;
}
function cborNumber(text) {
  if (!/[.eE]/.test(text)) {
    const n = BigInt(text), magnitude = n < 0n ? -1n - n : n;
    if (magnitude < 1n << 64n) return cborHead(n < 0n ? 1 : 0, magnitude);
    let hex = magnitude.toString(16);
    if (hex.length % 2) hex = `0${hex}`;
    const bytes = Array.from({ length: hex.length / 2 }, (_, index) => parseInt(hex.slice(index * 2, index * 2 + 2), 16));
    return [...cborHead(6, n < 0n ? 3 : 2), ...cborHead(2, bytes.length), ...bytes];
  }
  const x = Number(text), view = new DataView(new ArrayBuffer(8)), half = cborHalf(x);
  if (half !== null) return [0xf9, half >> 8, half & 255];
  if (Math.fround(x) === x) { view.setFloat32(0, x); return [0xfa, ...new Uint8Array(view.buffer, 0, 4)]; }
  view.setFloat64(0, x);
  return [0xfb, ...new Uint8Array(view.buffer)];
}
function cborText(text) {
  if (!text.isWellFormed()) throw LoneSurrogate;
  const bytes = [...new TextEncoder().encode(text)];
  return [...cborHead(3, bytes.length), ...bytes];
}
function cborEncode(value) {
  if (value === null) return [0xf6];
  if (value === true) return [0xf5];
  if (value === false) return [0xf4];
  if (typeof value === "number") return cborNumber(String(value));
  if (typeof value === "string") return cborText(value);
  if (Array.isArray(value)) return [...cborHead(4, value.length), ...value.flatMap(cborEncode)];
  const entries = Object.entries(value).map(([key, item]) => [cborText(key), cborEncode(item)]);
  entries.sort(([left], [right]) => {
    for (let index = 0; index < Math.min(left.length, right.length); index++) if (left[index] !== right[index]) return left[index] - right[index];
    return left.length - right.length;
  });
  return [...cborHead(5, entries.length), ...entries.flatMap(([key, item]) => [...key, ...item])];
}
function cborReference(value) {
  try {
    return fnv64(cborEncode(value));
  } catch (error) {
    if (error === LoneSurrogate) return status(LoneSurrogate, -1);
    throw error;
  }
}
// RFC 8949 Appendix A, as a check of the reference encoder itself.
const hexOf = (bytes) => bytes.map((byte) => byte.toString(16).padStart(2, "0")).join("");
for (const [value, hex] of [[0, "00"], [24, "1818"], [1000000000000, "1b000000e8d4a51000"], [-1000, "3903e7"], [1.1, "fb3ff199999999999a"],
  [1.5, "f93e00"], [3.4028234663852886e+38, "fa7f7fffff"], [5.960464477539063e-8, "f90001"], [-4.1, "fbc010666666666666"],
  [[1, [2, 3], [4, 5]], "8301820203820405"], [{ a: 1, b: [2, 3] }, "a26161016162820203"], ["\u00fc", "62c3bc"]]) {
  assert.equal(hexOf(cborEncode(value)), hex);
}
assert.equal(hexOf(cborNumber("18446744073709551616")), "c249010000000000000000");
assert.equal(hexOf(cborNumber("-18446744073709551617")), "c349010000000000000000");

// Inputs are JavaScript strings of Unicode scalars; the program sees their UTF-8 bytes.
const cases = [];
// Rejected inputs with the error kind and the byte offset that RFC 8259 and D08 define.
const rejected = (text, kind, offset) => cases.push({ text, status: status(kind, offset), output: 0n, cbor: status(kind, offset) });
// Accepted inputs whose numbers JavaScript writes back unchanged and whose keys are not array
// indices (JSON.stringify moves those first): the output equals JSON.stringify(JSON.parse(text)).
const javascript = (text) => {
  const value = JSON.parse(text);
  cases.push({ text, status: -1n, output: hashText(JSON.stringify(value)), value, cbor: cborReference(value) });
};
// Accepted inputs whose expected output is written by hand (numbers keep their text, order is kept).
const literal = (text, output) => {
  JSON.parse(text);
  cases.push({ text, status: -1n, output: hashText(output) });
};

rejected("", UnexpectedEnd, 0);
rejected("   ", UnexpectedEnd, 3);
rejected("\uFEFF1", Syntax, 0);
rejected("[1,2,]", Syntax, 5);
rejected("{\"a\":1,}", Syntax, 7);
rejected("[1 2]", Syntax, 3);
rejected("01", Syntax, 1);
rejected("-01", Syntax, 2);
rejected("+1", Syntax, 0);
rejected(".5", Syntax, 0);
rejected("1.", UnexpectedEnd, 2);
rejected("1.e5", Syntax, 2);
rejected("1e", UnexpectedEnd, 2);
rejected("1e+", UnexpectedEnd, 3);
rejected("1ea", Syntax, 2);
rejected("-", UnexpectedEnd, 1);
rejected("-a", Syntax, 1);
rejected("NaN", Syntax, 0);
rejected("Infinity", Syntax, 0);
rejected("-Infinity", Syntax, 1);
rejected("TRUE", Syntax, 0);
rejected("tru", UnexpectedEnd, 3);
rejected("trUe", Syntax, 2);
rejected("nul", UnexpectedEnd, 3);
rejected("[1] x", Syntax, 4);
rejected("1 2", Syntax, 2);
rejected("[1,2]]", Syntax, 5);
rejected("// c\n1", Syntax, 0);
rejected("/* c */1", Syntax, 0);
rejected("[", UnexpectedEnd, 1);
rejected("[1", UnexpectedEnd, 2);
rejected("[1,", UnexpectedEnd, 3);
rejected("{", UnexpectedEnd, 1);
rejected("{\"a\"", UnexpectedEnd, 4);
rejected("{\"a\":", UnexpectedEnd, 5);
rejected("{\"a\" 1}", Syntax, 5);
rejected("{a:1}", Syntax, 1);
rejected("{'a':1}", Syntax, 1);
rejected("['a']", Syntax, 1);
rejected("\"abc", UnexpectedEnd, 4);
rejected("\"\\x\"", InvalidEscape, 2);
rejected("\"é\\x\"", InvalidEscape, 4);
rejected("\"\\u12G4\"", InvalidEscape, 5);
rejected("\"\\u12\"", InvalidEscape, 5);
rejected("\"\\u", UnexpectedEnd, 3);
rejected("\"\\", UnexpectedEnd, 2);
rejected("\"\\\n\"", InvalidEscape, 2);
rejected("\"\u0001\"", ControlCharacter, 1);
rejected("\"a\tb\"", ControlCharacter, 2);
rejected("\"😀\nb\"", ControlCharacter, 5);
rejected("\"\u001f\"", ControlCharacter, 1);
rejected("\"\u0000\"", ControlCharacter, 1);
rejected("[\"\\ud800\" x", Syntax, 10);
rejected("{\"a\":1,\"a\":2}", DuplicateKey, 7);
rejected("{\"a\":1,\"\\u0061\":2}", DuplicateKey, 7);
rejected("{\"a\":{\"b\":1,\"b\":2},\"a\":3}", DuplicateKey, 12);
rejected("{\"a\":1,\"a\":[1,]}", DuplicateKey, 7);
rejected("{\"a\":1,\"a\":2", DuplicateKey, 7);
rejected("[".repeat(129) + "]".repeat(129), TooDeep, 128);
rejected("[".repeat(100000), TooDeep, 128);
rejected("{\"a\":".repeat(129) + "1" + "}".repeat(129), TooDeep, 640);
// More than 32 members take the sorted duplicate check; it reports the same first duplicate.
const wide = (count) => Array.from({ length: count }, (_, index) => `"k${index}":${index}`);
{
  const members = wide(40);
  members.splice(35, 0, "\"k3\":0", "\"k1\":0");
  const text = `{${members.join(",")}}`;
  rejected(text, DuplicateKey, text.indexOf("\"k3\":0"));
  const late = `{${wide(40).join(",")},"k7":[1,],"k1":1}`;
  rejected(late, DuplicateKey, late.indexOf("\"k7\":["));
  const syntax = `{${wide(40).join(",")},"z":[1,]}`;
  rejected(syntax, Syntax, syntax.indexOf("]}"));
}

// RFC 8259 §13.
javascript(`{
      "Image": {
          "Width":  800,
          "Height": 600,
          "Title":  "View from 15th Floor",
          "Thumbnail": {
              "Url":    "http://www.example.com/image/481989943",
              "Height": 125,
              "Width":  100
          },
          "Animated" : false,
          "IDs": [116, 943, 234, 38793]
        }
      }`);
javascript(`[
        {
           "precision": "zip",
           "Latitude":  37.7668,
           "Longitude": -122.3959895,
           "Address":   "",
           "City":      "SAN FRANCISCO",
           "State":     "CA",
           "Zip":       "94107",
           "Country":   "US"
        },
        {
           "precision": "zip",
           "Latitude":  37.371991,
           "Longitude": -122.026020,
           "Address":   "",
           "City":      "SUNNYVALE",
           "State":     "CA",
           "Zip":       "94085",
           "Country":   "US"
        }
      ]`.replace("-122.026020", "-122.02602"));
javascript("\"\\\"\\\\\\/\\b\\f\\n\\r\\t\\u0041\\u00e9\\u20AC\\ud83d\\ude00\"");
javascript("\"\\ud800\"");
javascript("\"\\udc00x\"");
javascript("\"\\ud800\\ud800\"");
javascript("\"\\ude00\\ud83d\"");
javascript("\"\\u0000\\u001F\\u007f\\u2028\\u2029/\"");
javascript("\"é😀\u007f\u2028\"");
for (const scalar of ["true", "false", "null", "0", "1.5", "-2", "\"\"", "[]", "{}", "[[]]", "{\"a\":{}}"]) javascript(scalar);
javascript(" \t\n\r[ 1 , 2 ,{ \"a\" : [ true , null ] } ]\r\n ");
javascript("[".repeat(128) + "]".repeat(128));
javascript(`{${wide(40).join(",")}}`);
javascript("{\"\":0,\" \":1,\"a\":2,\"A\":3}");

literal("1.0E+2", "1.0E+2");
literal("-0", "-0");
literal("1e400", "1e400");
literal("18446744073709551615", "18446744073709551615");
literal("[1.50, 2e-0, -0.0, 0.1e1, 1E5]", "[1.50,2e-0,-0.0,0.1e1,1E5]");
literal("{\"n\": 123456789012345678901234567890}", "{\"n\":123456789012345678901234567890}");
literal("{\"b\":1,\"1\":2,\"0\":3}", "{\"b\":1,\"1\":2,\"0\":3}");
literal("[-122.026020, 37.371991]", "[-122.026020,37.371991]");
literal("[ \"\\u0041\" , \"\\/\" ]", "[\"A\",\"/\"]");

// Values that a seed-fixed LCG builds in JavaScript; each is written compact and indented.
let seed = 0x2545f4914f6cdd1dn;
const next = (bound) => {
  seed = BigInt.asUintN(64, seed * 6364136223846793005n + 1442695040888963407n);
  return Number((seed >> 33n) % BigInt(bound));
};
const pieces = ["a", "Z", "é", "😀", "\"", "\\", "/", "\n", "\u0000", "\u001f", "\u007f", "\u2028", "\ud800", "\udfff", "日本", " "];
const randomString = () => Array.from({ length: next(6) }, () => pieces[next(pieces.length)]).join("");
function randomNumber() {
  switch (next(4)) {
    case 0: return next(2000) - 1000;
    case 1: return (next(1 << 30) * 2 ** 23 + next(1 << 23)) * (next(2) ? 1 : -1);
    case 2: return (next(1 << 30) / 2 ** 30) * 10 ** (next(61) - 30);
    default: return Number(`${next(1000)}.${next(1000)}e${next(40) - 20}`);
  }
}
function randomValue(depth) {
  const choice = next(depth > 3 ? 5 : 7);
  if (choice === 0) return null;
  if (choice === 1) return next(2) === 0;
  if (choice === 2 || choice === 3) return randomNumber();
  if (choice === 4) return randomString();
  if (choice === 5) return Array.from({ length: next(5) }, () => randomValue(depth + 1));
  const object = {};
  for (let index = next(5); index > 0; index--) object[`k${randomString()}`] = randomValue(depth + 1);
  return object;
}
for (let index = 0; index < 200; index++) {
  const value = randomValue(0);
  const compact = JSON.stringify(value);
  assert.equal(JSON.stringify(JSON.parse(compact)), compact);
  javascript(index % 2 === 0 ? compact : JSON.stringify(value, null, 1 + index % 3));
}

// The inputs written as `u8"..."` literals: printable ASCII stays, `"` and `\` are escaped, and
// every other scalar is `\u{...}`, so control characters reach the parser as raw bytes.
function literalOf(text) {
  let out = "u8\"";
  for (const character of text) {
    const code = character.codePointAt(0);
    if (character === "\"" || character === "\\") out += `\\${character}`;
    else if (code >= 0x20 && code < 0x7f) out += character;
    else out += `\\u{${code.toString(16)}}`;
  }
  return `${out}"`;
}
const program = `def input :: i64 -> utf8string
fn input index =
    match index with
${cases.map(({ text }, index) => `    | ${index} -> ${literalOf(text)}`).join("\n")}
    | _ -> u8""

def kind_number :: ref Json.ErrorKind -> i64
fn kind_number kind =
    match kind with
    | Json.Syntax -> 1
    | Json.UnexpectedEnd -> 2
    | Json.InvalidEscape -> 3
    | Json.ControlCharacter -> 4
    | Json.DuplicateKey _ -> 5
    | Json.TooDeep -> 6
    | Json.TooLarge -> 7
    | Json.NumberRange -> 9
    | Json.LoneSurrogate -> 13
    | _ -> 99

def fnv :: ref utf8string -> i64
fn fnv text =
    let mut hash = 14695981039346656037i64u
    for byte in text do hash = (hash ^^^ (byte as i64u)) * 1099511628211i64u
    hash as i64

/// -1 when the input parses, or the error kind times 2^40 plus its byte offset.
export def status :: i64 -> i64
fn status index =
    let text = input index
    match Json.parse (ref text) with
    | Ok _ -> -1
    | Error error -> kind_number (ref error.kind) * 1099511627776 + error.offset

def fnv_bytes :: ref [ubyte] -> i64
fn fnv_bytes bytes =
    let mut hash = 14695981039346656037i64u
    for byte in bytes do hash = (hash ^^^ (byte as i64u)) * 1099511628211i64u
    hash as i64

def error_status :: ref Json.Error -> i64
fn error_status error = kind_number (ref error.kind) * 1099511627776 + error.offset

/// The FNV-1a hash of the JSON text that \`Json.to_utf8string_pretty\` writes, or 0 when parsing fails.
export def pretty_hash :: i64 -> i64 -> i64
fn pretty_hash index indent =
    let text = input index
    match Json.parse (ref text) with
    | Ok value -> fnv (ref (Json.to_utf8string_pretty (ref value) indent))
    | Error _ -> 0

def replay :: ref utf8string -> ref Json.Event -> Json.Writer -> Result<Json.Writer, Json.Error>
fn replay text event writer =
    match event with
    | Json.ObjectStart -> Json.open_object writer
    | Json.ObjectEnd -> Json.close_object writer
    | Json.ArrayStart -> Json.open_array writer
    | Json.ArrayEnd -> Json.close_array writer
    | Json.KeyToken token -> Json.write_key writer (ref (Result.get (Json.token_text text token)))
    | Json.TextToken token -> Json.write_text writer (ref (Result.get (Json.token_text text token)))
    | Json.NumberToken token -> Json.write_number writer (ref (Result.get (Json.token_numeral text token)))
    | Json.BoolToken flag -> Json.write_bool writer flag
    | Json.NullToken -> Json.write_null writer
    | Json.EndOfInput -> Ok writer

/// The pull parser's events written again with the incremental writer: the FNV-1a hash of the
/// output, or the status of the reader's error (7777 if the writer refuses an event).
export def stream_hash :: i64 -> i64
fn stream_hash index =
    let text = input index
    let mut reader = Json.reader (ref text)
    let mut writer = Ok (Json.writer ())
    let mut result = 0
    let mut going = true
    while going do
        match reader with
        | Error error ->
            result = error_status (ref error)
            going = false
            reader = Error error
        | Ok current ->
            match Json.next (ref text) current with
            | Error error ->
                result = error_status (ref error)
                going = false
                reader = Error error
            | Ok (event, after) ->
                writer = match writer with
                    | Ok open -> replay (ref text) (ref event) open
                    | Error error -> Error error
                if event == Json.EndOfInput then
                    result = match writer with
                        | Ok done -> match Json.finish done with
                            | Ok output -> fnv (ref output)
                            | Error _ -> 7777
                        | Error _ -> 7777
                    writer = Ok (Json.writer ())
                    going = false
                reader = Ok after
    result

/// CBOR of the parsed value (FNV-1a hash), checked to decode and encode again to the same bytes;
/// the status of the first error otherwise.
export def cbor_hash :: i64 -> i64
fn cbor_hash index =
    let text = input index
    match Json.parse (ref text) with
    | Error error -> error_status (ref error)
    | Ok value ->
        match Cbor.encode (ref value) with
        | Error error -> error_status (ref error)
        | Ok bytes ->
            match Cbor.decode (ref bytes) with
            | Error _ -> 1
            | Ok back ->
                match Cbor.encode (ref back) with
                | Ok again -> if again == bytes then fnv_bytes (ref bytes) else 2
                | Error _ -> 3

/// The FNV-1a hash of the JSON text that \`Json.to_utf8string\` writes, or 0 when parsing fails.
export def output_hash :: i64 -> i64
fn output_hash index =
    let text = input index
    match Json.parse (ref text) with
    | Ok value -> fnv (ref (Json.to_utf8string (ref value)))
    | Error _ -> 0
`;

// Pretty printing is compared with JSON.stringify(value, null, indent) at these indents.
const indents = [0, 1, 2, 4, 10, 12];
const cValue = (n) => n === -(1n << 63n) ? "INT64_MIN" : n < 0n ? `(-INT64_C(${-n}))` : `INT64_C(${n})`;
const temporary = mkdtempSync(join(tmpdir(), "tsuzuri-json-"));
try {
  const project = join(temporary, "cases");
  mkdirSync(project);
  writeFileSync(join(project, "Main.tz"), program);
  execute(compiler, ["check", project]);
  const ir = join(temporary, "cases.ll");
  const header = join(temporary, "tz-cases.h");
  execute(compiler, ["build", project, "--emit", "header", "-o", header]);
  execute(compiler, ["build", project, "--emit", "llvm", "-o", ir]);
  const source = readFileSync(ir, "utf8");
  execute(compiler, ["build", project, "--emit", "llvm", "-o", `${ir}.again`]);
  assert.equal(source, readFileSync(`${ir}.again`, "utf8"), "the IR is deterministic");
  writeFileSync(ir, source.replaceAll("@malloc", "@tracked_alloc").replaceAll("@free", "@tracked_free").replaceAll("@realloc", "@tracked_realloc"));
  const host = join(temporary, "host.c");
  writeFileSync(host, `#include <assert.h>
#include <stdint.h>
#include <stdlib.h>
#include "tz-cases.h"
static uint64_t live;
void *tracked_alloc(uint64_t size) {
    uint64_t *p = malloc((size_t)size + 16);
    assert(p);
    p[0] = size;
    live += size;
    return p + 2;
}
void tracked_free(void *value) {
    if (!value) return;
    uint64_t *p = (uint64_t *)value - 2;
    assert(live >= p[0]);
    live -= p[0];
    free(p);
}
void *tracked_realloc(void *value, uint64_t size) {
    if (!value) return tracked_alloc(size);
    if (!size) { tracked_free(value); return NULL; }
    uint64_t *previous = (uint64_t *)value - 2;
    uint64_t old_size = previous[0];
    uint64_t *next = realloc(previous, (size_t)size + 16);
    assert(next);
    next[0] = size;
    live = live - old_size + size;
    return next + 2;
}
int main(void) {
${cases.map(({ status, output, value, cbor }, index) => [
    `    assert(tz_status(INT64_C(${index})) == ${cValue(status)}); assert(live == 0);`,
    `    assert(tz_output_hash(INT64_C(${index})) == ${cValue(output)}); assert(live == 0);`,
    `    assert(tz_stream_hash(INT64_C(${index})) == ${cValue(status === -1n ? output : status)}); assert(live == 0);`,
    ...cbor === undefined ? [] : [`    assert(tz_cbor_hash(INT64_C(${index})) == ${cValue(cbor)}); assert(live == 0);`],
    ...value === undefined ? [] : indents.map((indent) => `    assert(tz_pretty_hash(INT64_C(${index}), INT64_C(${indent})) == ${cValue(hashText(JSON.stringify(value, null, indent)))}); assert(live == 0);`),
  ].join("\n")).join("\n")}
    return 0;
}
`);
  for (const optimization of ["0", "3"]) {
    const native = join(temporary, `cases-O${optimization}`);
    execute(clang, [`-O${optimization}`, "-Wno-override-module", "-ffp-contract=off", `-I${temporary}`, ir, host, "-lm", "-o", native]);
    execute(native, []);
    const wasm = join(temporary, `cases-O${optimization}.wasm`);
    execute(compiler, ["build", project, "--target", "wasm32", `-O${optimization}`, "-o", wasm]);
    const module = new WebAssembly.Module(readFileSync(wasm));
    assert.deepEqual(WebAssembly.Module.imports(module), [], "WASM has no imports");
    const exports = new WebAssembly.Instance(module).exports;
    for (const [index, { text, status, output, value, cbor }] of cases.entries()) {
      const label = JSON.stringify(text).slice(0, 80);
      assert.equal(exports.tz_status(BigInt(index)), status, `WASM O${optimization} status of ${label}`);
      assert.equal(exports.tz_output_hash(BigInt(index)), output, `WASM O${optimization} output of ${label}`);
      assert.equal(exports.tz_stream_hash(BigInt(index)), status === -1n ? output : status, `WASM O${optimization} stream of ${label}`);
      if (cbor !== undefined) assert.equal(exports.tz_cbor_hash(BigInt(index)), cbor, `WASM O${optimization} CBOR of ${label}`);
      if (value !== undefined) {
        for (const indent of indents) {
          assert.equal(exports.tz_pretty_hash(BigInt(index), BigInt(indent)), hashText(JSON.stringify(value, null, indent)), `WASM O${optimization} pretty ${indent} of ${label}`);
        }
      }
    }
    assert.ok(exports.memory.buffer.byteLength <= 16 * 1024 * 1024, "WASM stays within 16 MiB");
  }
  const accepted = cases.filter((entry) => entry.status === -1n).length;
  const checks = cases.reduce((total, { value, cbor }) => total + 3 + (cbor === undefined ? 0 : 1) + (value === undefined ? 0 : indents.length), 0);
  console.log(`json conformance: ${cases.length} inputs (${accepted} accepted, ${cases.length - accepted} rejected), ${checks} checks of parse, output, pull parser and writer, pretty printing, and CBOR; native and WASM at -O0 and -O3`);
} finally {
  rmSync(temporary, { recursive: true, force: true });
}
