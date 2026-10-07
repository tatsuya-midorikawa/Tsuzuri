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

// Inputs are JavaScript strings of Unicode scalars; the program sees their UTF-8 bytes.
const cases = [];
// Rejected inputs with the error kind and the byte offset that RFC 8259 and D08 define.
const rejected = (text, kind, offset) => cases.push({ text, status: status(kind, offset), output: 0n });
// Accepted inputs whose numbers JavaScript writes back unchanged and whose keys are not array
// indices (JSON.stringify moves those first): the output equals JSON.stringify(JSON.parse(text)).
const javascript = (text) => {
  const value = JSON.parse(text);
  cases.push({ text, status: -1n, output: hashText(JSON.stringify(value)) });
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

/// The FNV-1a hash of the JSON text that \`Json.to_utf8string\` writes, or 0 when parsing fails.
export def output_hash :: i64 -> i64
fn output_hash index =
    let text = input index
    match Json.parse (ref text) with
    | Ok value -> fnv (ref (Json.to_utf8string (ref value)))
    | Error _ -> 0
`;

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
${cases.map(({ status, output }, index) => `    assert(tz_status(INT64_C(${index})) == ${cValue(status)}); assert(live == 0);
    assert(tz_output_hash(INT64_C(${index})) == ${cValue(output)}); assert(live == 0);`).join("\n")}
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
    for (const [index, { text, status, output }] of cases.entries()) {
      assert.equal(exports.tz_status(BigInt(index)), status, `WASM O${optimization} status of ${JSON.stringify(text).slice(0, 80)}`);
      assert.equal(exports.tz_output_hash(BigInt(index)), output, `WASM O${optimization} output of ${JSON.stringify(text).slice(0, 80)}`);
    }
    assert.ok(exports.memory.buffer.byteLength <= 16 * 1024 * 1024, "WASM stays within 16 MiB");
  }
  const accepted = cases.filter((entry) => entry.status === -1n).length;
  console.log(`json conformance: ${cases.length} inputs (${accepted} accepted, ${cases.length - accepted} rejected), native and WASM at -O0 and -O3`);
} finally {
  rmSync(temporary, { recursive: true, force: true });
}
