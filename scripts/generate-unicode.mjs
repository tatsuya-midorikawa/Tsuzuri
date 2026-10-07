#!/usr/bin/env node
// Generates src/runtime/unicode.ll, the Unicode tables of std/Unicode.tz and std/Regex.tz, from the
// Unicode Character Database. The UCD files are not part of the repository; download them first:
//
//   mkdir -p target/ucd/17.0.0/extracted target/ucd/17.0.0/auxiliary target/ucd/17.0.0/emoji
//   for f in extracted/DerivedGeneralCategory.txt DerivedCoreProperties.txt PropList.txt CaseFolding.txt \
//       UnicodeData.txt SpecialCasing.txt DerivedNormalizationProps.txt auxiliary/GraphemeBreakProperty.txt \
//       auxiliary/WordBreakProperty.txt emoji/emoji-data.txt; do
//     curl -fsSL -o "target/ucd/17.0.0/$f" "https://www.unicode.org/Public/17.0.0/ucd/$f"
//   done
//   node scripts/generate-unicode.mjs target/ucd/17.0.0
//
// `--check` compares the generated text with the committed file instead of writing it. The output
// depends only on the pinned inputs, so two runs produce the same bytes.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const VERSION = "17.0.0";
const EXPECTED_SHA256 = {
  "extracted/DerivedGeneralCategory.txt": "d62e5bab70ca74f099343f71224fa051cb1fdd61a1ab45c0488c44cfc0b6102e",
  "DerivedCoreProperties.txt": "24c7fed1195c482faaefd5c1e7eb821c5ee1fb6de07ecdbaa64b56a99da22c08",
  "PropList.txt": "130dcddcaadaf071008bdfce1e7743e04fdfbc910886f017d9f9ac931d8c64dd",
  "CaseFolding.txt": "ff8d8fefbf123574205085d6714c36149eb946d717a0c585c27f0f4ef58c4183",
  "UnicodeData.txt": "2e1efc1dcb59c575eedf5ccae60f95229f706ee6d031835247d843c11d96470c",
  "SpecialCasing.txt": "efc25faf19de21b92c1194c111c932e03d2a5eaf18194e33f1156e96de4c9588",
  "DerivedNormalizationProps.txt": "71fd6a206a2c0cdd41feb6b7f656aa31091db45e9cedc926985d718397f9e488",
  "auxiliary/GraphemeBreakProperty.txt": "d6b51d1d2ae5c33b451b7ed994b48f1f4dc62b2272a5831e7fd418514a6bae89",
  "auxiliary/WordBreakProperty.txt": "72274cac1e6b919507db35655c3e175aa27274668a1ece95c28d2069f2ad9852",
  "emoji/emoji-data.txt": "2cb2bb9455cda83e8481541ecf5b6dfda66a3bb89efa3fa7c5297eccf607b72b",
};
// Property value numberings that std/Unicode.tz decodes.
const GRAPHEME = ["Other", "CR", "LF", "Control", "Extend", "ZWJ", "Regional_Indicator", "Prepend", "SpacingMark",
  "L", "V", "T", "LV", "LVT"];
const WORD = ["Other", "CR", "LF", "Newline", "Extend", "ZWJ", "Regional_Indicator", "Format", "Katakana",
  "Hebrew_Letter", "ALetter", "Single_Quote", "Double_Quote", "MidNumLet", "MidLetter", "MidNum", "Numeric",
  "ExtendNumLet", "WSegSpace"];
const CONJUNCT = ["None", "Linker", "Consonant", "Extend"];
// The order of the general categories is the numbering that std/Unicode.tz decodes.
const CATEGORIES = ["Lu", "Ll", "Lt", "Lm", "Lo", "Mn", "Mc", "Me", "Nd", "Nl", "No", "Pc", "Pd", "Ps", "Pe", "Pi",
  "Pf", "Po", "Sm", "Sc", "Sk", "So", "Zs", "Zl", "Zp", "Cc", "Cf", "Cs", "Co", "Cn"];
const LAST = 0x10ffff;

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const output = join(root, "src/runtime/unicode.ll");
const args = process.argv.slice(2);
const check = args.includes("--check");
const directory = args.find((arg) => !arg.startsWith("--"));
if (!directory) {
  console.error("usage: node scripts/generate-unicode.mjs [--check] <UCD 17.0.0 directory>");
  process.exit(2);
}

function input(name) {
  const bytes = readFileSync(join(directory, name));
  const digest = createHash("sha256").update(bytes).digest("hex");
  assert.equal(digest, EXPECTED_SHA256[name], `${name}: SHA-256 differs from the pinned Unicode ${VERSION} file`);
  return bytes.toString("utf8").split("\n").map((line) => line.replace(/#.*/, "").trim()).filter(Boolean)
    .map((line) => line.split(";").map((field) => field.trim()));
}

function codeRange(field) {
  const [first, last] = field.split("..").map((digits) => parseInt(digits, 16));
  return [first, last ?? first];
}

// Sorted, merged closed ranges of the code points that have `property` in a binary-property file.
function property(rows, name) {
  const ranges = rows.filter((row) => row[1] === name).map((row) => codeRange(row[0])).sort((a, b) => a[0] - b[0]);
  const merged = [];
  for (const [first, last] of ranges) {
    const previous = merged.at(-1);
    if (previous && previous[1] + 1 >= first) previous[1] = Math.max(previous[1], last);
    else merged.push([first, last]);
  }
  assert.ok(merged.length > 0, name);
  return merged;
}

// Runs of a per-code-point value: the start of every run whose value differs from the previous one.
function runs(values, encode) {
  const entries = [];
  for (let code = 0; code <= LAST; code++) {
    if (code === 0 || values[code] !== values[code - 1]) entries.push(encode(code, values[code]));
  }
  return entries;
}

// A sorted mapping as runs (start, count, stride, delta): every member start + k * stride maps to itself
// plus delta. A run covers consecutive keys of the sorted mapping, so expanding the runs in order lists
// the keys in ascending order.
function deltaRuns(mapping) {
  const keys = [...mapping.keys()].sort((a, b) => a - b);
  const entries = [];
  let index = 0;
  while (index < keys.length) {
    const start = keys[index];
    const delta = mapping.get(start) - start;
    let count = 1;
    let stride = 1;
    if (index + 1 < keys.length && mapping.get(keys[index + 1]) - keys[index + 1] === delta && keys[index + 1] - start <= 2) {
      stride = keys[index + 1] - start;
      while (index + count < keys.length && keys[index + count] - keys[index + count - 1] === stride
        && mapping.get(keys[index + count]) - keys[index + count] === delta) count++;
    }
    entries.push(start, count, stride, delta);
    index += count;
  }
  return entries;
}

const category = new Int8Array(LAST + 1).fill(-1);
for (const [field, name] of input("extracted/DerivedGeneralCategory.txt")) {
  const value = CATEGORIES.indexOf(name);
  assert.ok(value >= 0, `unknown general category ${name}`);
  const [first, last] = codeRange(field);
  for (let code = first; code <= last; code++) {
    assert.equal(category[code], -1, `U+${code.toString(16)} has two general categories`);
    category[code] = value;
  }
}
assert.ok(category.every((value) => value >= 0), "the general categories cover U+0000..U+10FFFF");

const core = input("DerivedCoreProperties.txt");
const list = input("PropList.txt");
const folding = new Map();
for (const [field, status, target] of input("CaseFolding.txt")) {
  if (status !== "C" && status !== "S") continue;
  const code = parseInt(field, 16);
  const folded = parseInt(target, 16);
  assert.ok(!folding.has(code) && folded !== code, `U+${field}`);
  folding.set(code, folded);
}
for (const [code, folded] of folding) assert.equal(folding.get(folded) ?? folded, folded, `scf(scf(U+${code.toString(16)}))`);

// Per-code-point values of a property file whose rows are `range; value`.
function values(rows, names, filter = () => true) {
  const result = new Int8Array(LAST + 1);
  for (const row of rows) {
    if (!filter(row)) continue;
    const value = names.indexOf(row.at(-1));
    assert.ok(value > 0, `unknown value ${row.at(-1)}`);
    const [first, last] = codeRange(row[0]);
    for (let code = first; code <= last; code++) {
      assert.equal(result[code], 0, `U+${code.toString(16)} has two values`);
      result[code] = value;
    }
  }
  return result;
}

const data = input("UnicodeData.txt");
const combining = new Uint8Array(LAST + 1);
const decompositions = new Map();
const lower = new Map();
const upper = new Map();
const title = new Map();
for (const fields of data) {
  const code = parseInt(fields[0], 16);
  combining[code] = Number(fields[3]);
  if (fields[5]) {
    const parts = fields[5].split(" ");
    const compat = parts[0].startsWith("<");
    decompositions.set(code, [compat, parts.filter((part) => !part.startsWith("<")).map((part) => parseInt(part, 16))]);
  }
  if (fields[12]) upper.set(code, parseInt(fields[12], 16));
  if (fields[13]) lower.set(code, parseInt(fields[13], 16));
  // An empty Simple_Titlecase_Mapping is the Simple_Uppercase_Mapping (UAX #44).
  if (fields[14] && fields[14] !== (fields[12] || fields[0])) title.set(code, parseInt(fields[14], 16));
}
for (const [code, [, mapping]] of decompositions) {
  assert.ok(code < 0xac00 || code > 0xd7a3, "Hangul syllables decompose algorithmically");
  assert.ok(mapping.length >= 1 && mapping.length <= 18, `U+${code.toString(16)}`);
}
const excluded = new Set();
for (const row of input("DerivedNormalizationProps.txt")) {
  if (row[1] !== "Full_Composition_Exclusion") continue;
  const [first, last] = codeRange(row[0]);
  for (let code = first; code <= last; code++) excluded.add(code);
}
const decompositionKeys = [...decompositions.keys()].sort((a, b) => a - b);
const decompositionValues = [];
const decompositionPool = [];
for (const code of decompositionKeys) {
  const [compat, mapping] = decompositions.get(code);
  const payload = mapping.length === 1 ? mapping[0] : decompositionPool.length;
  if (mapping.length > 1) decompositionPool.push(...mapping);
  decompositionValues.push(payload * 64 + mapping.length * 2 + (compat ? 1 : 0));
}
const compositions = [];
for (const code of decompositionKeys) {
  const [compat, mapping] = decompositions.get(code);
  if (!compat && mapping.length === 2 && !excluded.has(code)) compositions.push([mapping[0], mapping[1], code]);
}
compositions.sort((a, b) => a[0] - b[0] || a[1] - b[1]);

const pictographic = values(input("emoji/emoji-data.txt"), ["", "Extended_Pictographic"], (row) => row[1] === "Extended_Pictographic");
const grapheme = values(input("auxiliary/GraphemeBreakProperty.txt"), GRAPHEME);
const conjunct = values(core.filter((row) => row[1] === "InCB").map((row) => [row[0], row[2]]), CONJUNCT);
// Hangul syllables are LV or LVT by (code - AC00) % 28; the table marks them all LV.
for (let code = 0xac00; code <= 0xd7a3; code++) {
  assert.equal(grapheme[code], (code - 0xac00) % 28 === 0 ? 12 : 13);
  grapheme[code] = 12;
}
const word = values(input("auxiliary/WordBreakProperty.txt"), WORD);

const special = new Map();
for (const row of input("SpecialCasing.txt")) {
  if (row.length > 5 && row[4] !== "") continue;
  const code = parseInt(row[0], 16);
  // Fields 1, 2 and 3 are the full lowercase, titlecase and uppercase mappings: the kinds 0, 1 and 2.
  const mappings = [row[1], row[2], row[3]].map((field) => field.split(" ").filter(Boolean).map((digits) => parseInt(digits, 16)));
  const simple = [lower.get(code) ?? code, title.get(code) ?? upper.get(code) ?? code, upper.get(code) ?? code];
  mappings.forEach((mapping, kind) => {
    if (mapping.length !== 1 || mapping[0] !== simple[kind]) special.set(code * 4 + kind, mapping);
  });
}
const fullFolding = new Set();
for (const [field, status, target] of input("CaseFolding.txt")) {
  const code = parseInt(field, 16);
  if (status === "F") {
    special.set(code * 4 + 3, target.split(" ").map((digits) => parseInt(digits, 16)));
    fullFolding.add(code);
  }
}
for (const [field, status] of input("CaseFolding.txt")) {
  if (status === "S") assert.ok(fullFolding.has(parseInt(field, 16)), `S without F for ${field}`);
}
const specialKeys = [...special.keys()].sort((a, b) => a - b);
const specialEntries = [];
const specialPool = [];
for (const key of specialKeys) {
  const mapping = special.get(key);
  specialEntries.push(key, specialPool.length * 32 + mapping.length);
  specialPool.push(...mapping);
}

const flatten = (ranges) => ranges.flat();
// Table ids are the first argument of the std-only builtins Unicode.__table_length and
// Unicode.__table_entry; std/Unicode.tz names them.
const tables = [
  ["general category runs: start << 5 | category", runs(category, (code, value) => code * 32 + value)],
  ["Alphabetic ranges: first, last", flatten(property(core, "Alphabetic"))],
  ["White_Space ranges: first, last", flatten(property(list, "White_Space"))],
  ["Join_Control ranges: first, last", flatten(property(list, "Join_Control"))],
  ["simple case folding (CaseFolding.txt C and S) runs: start, count, stride, delta", deltaRuns(folding)],
  ["canonical combining class runs: start << 8 | class", runs(combining, (code, value) => code * 256 + value)],
  ["decomposition keys (UnicodeData.txt, Hangul syllables excluded)", decompositionKeys],
  ["decomposition values: (code point or pool offset) << 6 | length << 1 | compatibility", decompositionValues],
  ["decomposition pool", decompositionPool],
  ["primary composites: first, second, composite (Full_Composition_Exclusion excluded)", compositions.flat()],
  ["grapheme runs: start << 7 | Grapheme_Cluster_Break | Extended_Pictographic << 4 | InCB << 5",
    runs(Int32Array.from(grapheme, (value, code) => value + pictographic[code] * 16 + conjunct[code] * 32), (code, value) => code * 128 + value)],
  ["word runs: start << 6 | Word_Break | Extended_Pictographic << 5",
    runs(Int32Array.from(word, (value, code) => value + pictographic[code] * 32), (code, value) => code * 64 + value)],
  ["Cased ranges: first, last", flatten(property(core, "Cased"))],
  ["Case_Ignorable ranges: first, last", flatten(property(core, "Case_Ignorable"))],
  ["simple lowercase runs: start, count, stride, delta", deltaRuns(lower)],
  ["simple uppercase runs: start, count, stride, delta", deltaRuns(upper)],
  ["simple titlecase runs where they differ from uppercase: start, count, stride, delta", deltaRuns(title)],
  ["full mappings that differ from the simple ones: code << 2 | kind (0 lower, 1 title, 2 upper, 3 fold), pool offset << 5 | length",
    specialEntries],
  ["full mapping pool", specialPool],
];

for (const [name, entries] of tables) {
  assert.ok(entries.every((value) => Number.isInteger(value) && value >= -(2 ** 31) && value < 2 ** 31), name);
}

let text = `; Generated by scripts/generate-unicode.mjs from Unicode ${VERSION} UCD. Do not edit.\n`;
text += "; Inputs (SHA-256):\n";
for (const [name, digest] of Object.entries(EXPECTED_SHA256)) text += `;   ${name} ${digest}\n`;
for (const [id, [name, entries]] of tables.entries()) {
  text += `\n; ${id}: ${name}\n@tz.unicode.table.${id} = internal unnamed_addr constant [${entries.length} x i32] [\n`;
  for (let index = 0; index < entries.length; index += 16) {
    const line = entries.slice(index, index + 16).map((value) => `i32 ${value}`).join(", ");
    text += `  ${line}${index + 16 < entries.length ? "," : ""}\n`;
  }
  text += "]\n";
}
const cases = (label) => tables.map((_, id) => `    i64 ${id}, label %${label}${id}`).join("\n");
// The accessors inline into every caller: a constant table number then selects one table, and LLVM
// drops the tables that a program never reads.
text += `\ndefine internal i64 @tz.unicode.length(i64 %table) nounwind alwaysinline {\nentry:\n  switch i64 %table, label %bad [\n${cases("t")}\n  ]\n`;
for (const [id, [, entries]] of tables.entries()) text += `t${id}:\n  ret i64 ${entries.length}\n`;
text += "bad:\n  call void @llvm.trap()\n  unreachable\n}\n";
text += `\ndefine internal i64 @tz.unicode.entry(i64 %table, i64 %index) nounwind alwaysinline {\nentry:\n  switch i64 %table, label %bad [\n${cases("t")}\n  ]\n`;
for (const [id, [, entries]] of tables.entries()) {
  text += `t${id}:\n  %in${id} = icmp ult i64 %index, ${entries.length}\n  br i1 %in${id}, label %load${id}, label %bad\n`;
  text += `load${id}:\n  %at${id} = getelementptr inbounds [${entries.length} x i32], ptr @tz.unicode.table.${id}, i64 0, i64 %index\n`;
  text += `  %value${id} = load i32, ptr %at${id}\n  %wide${id} = sext i32 %value${id} to i64\n  ret i64 %wide${id}\n`;
}
text += "bad:\n  call void @llvm.trap()\n  unreachable\n}\n";

if (check) {
  assert.equal(readFileSync(output, "utf8"), text, "src/runtime/unicode.ll is not the generator output; rerun the generator");
  console.log("src/runtime/unicode.ll matches the generator output");
} else {
  writeFileSync(output, text);
  for (const [id, [name, entries]] of tables.entries()) console.log(`table ${id}: ${entries.length} entries (${name})`);
  console.log(`wrote src/runtime/unicode.ll: ${text.length} bytes, SHA-256 ${createHash("sha256").update(text).digest("hex")}`);
}
