#!/usr/bin/env node
// Generates src/runtime/unicode.ll, the Unicode tables of std/Unicode.tz and std/Regex.tz, from the
// Unicode Character Database. The UCD files are not part of the repository; download them first:
//
//   mkdir -p target/ucd/17.0.0/extracted
//   for f in extracted/DerivedGeneralCategory.txt DerivedCoreProperties.txt PropList.txt CaseFolding.txt; do
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
};
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

const flatten = (ranges) => ranges.flat();
// Table ids are the first argument of the std-only builtins Unicode.__table_length and
// Unicode.__table_entry; std/Unicode.tz names them.
const tables = [
  ["general category runs: start << 5 | category", runs(category, (code, value) => code * 32 + value)],
  ["Alphabetic ranges: first, last", flatten(property(core, "Alphabetic"))],
  ["White_Space ranges: first, last", flatten(property(list, "White_Space"))],
  ["Join_Control ranges: first, last", flatten(property(list, "Join_Control"))],
  ["simple case folding (CaseFolding.txt C and S) runs: start, count, stride, delta", deltaRuns(folding)],
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
text += `\ndefine internal i64 @tz.unicode.length(i64 %table) nounwind {\nentry:\n  switch i64 %table, label %bad [\n${cases("t")}\n  ]\n`;
for (const [id, [, entries]] of tables.entries()) text += `t${id}:\n  ret i64 ${entries.length}\n`;
text += "bad:\n  call void @llvm.trap()\n  unreachable\n}\n";
text += `\ndefine internal i64 @tz.unicode.entry(i64 %table, i64 %index) nounwind {\nentry:\n  switch i64 %table, label %bad [\n${cases("t")}\n  ]\n`;
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
