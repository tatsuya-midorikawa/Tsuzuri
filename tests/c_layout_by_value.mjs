// W36: `@c_layout` records passed and returned by value in `extern def`.
//
// 1. Reference: for every target, the `declare` that Tsuzuri emits for each extern of
//    tests/fixtures/c_layout_by_value is compared with the `declare` that Clang emits for the same C
//    prototype (tests/c_layout_by_value_reference.c). Clang is the independent reference of the C ABI.
// 2. End to end on the host, at -O0 and -O3: the fixture calls the C functions of
//    tests/c_layout_by_value_host.c, which check every field and return values that the fixture checks.
//
// usage: node tests/c_layout_by_value.mjs <tsuzuri>   (TSUZURI_CLANG chooses the C compiler)
import assert from "node:assert/strict";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { spawnSync } from "node:child_process";

const compiler = resolve(process.argv[2] ?? "target/release/tsuzuri");
const clang = process.env.TSUZURI_CLANG ?? "clang";
const windows = process.platform === "win32";
const isBundledClang = /tsuzuri-clang(?:\.exe)?$/i.test(clang);
const fixture = resolve("tests/fixtures/c_layout_by_value");
const reference = resolve("tests/c_layout_by_value_reference.c");
const hostC = resolve("tests/c_layout_by_value_host.c");
const root = mkdtempSync(join(tmpdir(), "tsuzuri-by-value-"));

// `--target` of Tsuzuri and the triple that Clang takes for it.
const TARGETS = [
  ["x86_64-pc-windows-msvc", "x86_64-pc-windows-msvc"],
  ["x86_64-pc-windows-gnu", "x86_64-w64-windows-gnu"],
  ["aarch64-pc-windows-msvc", "aarch64-pc-windows-msvc"],
  ["aarch64-pc-windows-gnu", "aarch64-w64-windows-gnu"],
  ["x86_64-unknown-linux-gnu", "x86_64-unknown-linux-gnu"],
  ["x86_64-unknown-linux-musl", "x86_64-unknown-linux-musl"],
  ["aarch64-unknown-linux-gnu", "aarch64-unknown-linux-gnu"],
  ["aarch64-unknown-linux-musl", "aarch64-unknown-linux-musl"],
  ["aarch64-apple-darwin", "arm64-apple-macosx12.0.0"],
];
// The structs of these functions hold pointers. Clang spells a pointer that passes in an integer
// register `ptr` and Tsuzuri spells it `i64`: the same register, so only here the two are equal.
const POINTER_FIELDS = new Set(["bv_take_hx", "bv_ret_hx", "bv_take_h1", "bv_ret_h1", "bv_take_h2", "bv_ret_h2"]);

function run(program, args, expectSuccess = true) {
  const result = spawnSync(program, args, { cwd: root, encoding: "utf8", timeout: 300_000, maxBuffer: 64 * 1024 * 1024 });
  assert.ifError(result.error);
  if (expectSuccess) assert.equal(result.status, 0, `${program} ${args.join(" ")}\n${result.stdout}\n${result.stderr}`);
  return result;
}

// Splits at the commas that are outside every bracket.
function splitTop(text) {
  const parts = [];
  let depth = 0;
  let start = 0;
  for (let index = 0; index < text.length; index++) {
    const character = text[index];
    if ("([{<".includes(character)) depth++;
    else if (")]}>".includes(character)) depth--;
    else if (character === "," && depth === 0) {
      parts.push(text.slice(start, index));
      start = index + 1;
    }
  }
  parts.push(text.slice(start));
  return parts.map((part) => part.trim()).filter((part) => part !== "");
}

// The scalars of a struct type in order: named structs, nested structs and arrays are opened.
function leaves(type, named) {
  const text = type.trim();
  if (text.startsWith("%")) {
    const definition = named.get(text);
    assert.ok(definition, `no definition of ${text}`);
    return leaves(definition, named);
  }
  const struct = /^<?\{(.*)\}>?$/s.exec(text);
  if (struct) return splitTop(struct[1]).flatMap((part) => leaves(part, named));
  const array = /^\[(\d+) x (.*)\]$/s.exec(text);
  if (array) return Array.from({ length: Number(array[1]) }, () => leaves(array[2], named)).flat();
  return [text];
}

// A struct type is the list of its scalars (Clang returns an HFA as its named struct type).
function canonicalType(type, named) {
  const text = type.trim();
  return text.startsWith("%") || text.startsWith("{") || text.startsWith("<{") ? `{ ${leaves(text, named).join(", ")} }` : text;
}

const ATTRIBUTES = /\b(?:noundef|dead_on_unwind|writable|noalias|nocapture|nonnull|readonly|readnone|nofree|dso_local|dllimport|local_unnamed_addr|hidden)\b/g;

function normalize(text, named, pointers) {
  let type = text
    .replace(/\bdead_on_return(?:\(\d+\))?/g, "")
    .replace(/\bcaptures\([^)]*\)/g, "")
    .replace(/\binitializes\(\([^)]*\)\)/g, "")
    .replace(ATTRIBUTES, "")
    // The pointee of `sret` and `byval` has another name on each side, and the alignment of `sret`
    // is not part of the ABI. `byval` keeps its alignment.
    .replace(/\bsret\([^)]*\)(?:\s+align\s+\d+)?/, "sret")
    .replace(/\bbyval\([^)]*\)/, "byval")
    .replace(/\s+/g, " ")
    .trim();
  if (!/ (?:sret|byval)\b/.test(type) && !/ alignstack\(/.test(type)) type = canonicalType(type, named);
  if (pointers) type = type.replace(/\bptr\b(?! (?:sret|byval))/g, "i64");
  return type;
}

// name -> "result (parameter, ...)" for every `declare` of a `bv_` function.
function declarations(ir) {
  const named = new Map();
  for (const line of ir.split("\n")) {
    const definition = /^(%[\w.]+) = type (.*)$/.exec(line.trim());
    if (definition) named.set(definition[1], definition[2]);
  }
  const found = new Map();
  for (const line of ir.split("\n")) {
    const declare = /^declare\s+(.*?)@(bv_\w+)\((.*)\)[^)]*$/.exec(line.trim());
    if (!declare) continue;
    const pointers = POINTER_FIELDS.has(declare[2]);
    const result = normalize(declare[1], named, pointers);
    const parameters = splitTop(declare[3]).map((parameter) => normalize(parameter, named, pointers));
    found.set(declare[2], `${result} (${parameters.join(", ")})`);
  }
  return found;
}

try {
  // 1. Every declare against Clang, per target.
  let compared = 0;
  const skipped = [];
  for (const [target, triple] of TARGETS) {
    const referenceIr = join(root, `reference-${target}.ll`);
    const clangResult = run(clang, [`--target=${triple}`, "-O1", "-S", "-emit-llvm", "-ffreestanding", reference, "-o", referenceIr], false);
    if (clangResult.status !== 0) {
      // A Clang built without this target cannot be the reference for it, and the bundled launcher
      // (Zig or LLVM-MinGW) only takes the targets that it can link. Anything else is a failure.
      const missingTarget = /unknown target|unable to create target|No available targets|unsupported option '--target/i.test(clangResult.stderr);
      assert.ok(isBundledClang || missingTarget, `${clang} --target=${triple}\n${clangResult.stderr}`);
      skipped.push(target);
      continue;
    }
    const expected = declarations(readFileSync(referenceIr, "utf8"));
    assert.ok(expected.size >= 99, `${target}: the reference has ${expected.size} functions`);
    const ir = join(root, `fixture-${target}.ll`);
    run(compiler, ["build", fixture, "--no-cache", "--emit", "llvm", "--target", target, "-o", ir]);
    const text = readFileSync(ir, "utf8");
    const actual = declarations(text);
    for (const [name, signature] of expected) {
      assert.equal(actual.get(name), signature, `${target}: declare of ${name}`);
    }
    // LLVM accepts the lowered calls for the target (`sret`, `byval`, `alignstack`, vector and array
    // operands): the IR compiles to an object. No sysroot is needed for that.
    const assembled = run(clang, [`--target=${triple}`, "-Wno-override-module", "-c", "-x", "ir", ir, "-o", join(root, `fixture-${target}.o`)], false);
    assert.equal(assembled.status, 0, `${target}: Clang rejects the IR\n${assembled.stderr}`);
    // No attribute that an older Clang cannot read (LLVM 17 and later read the IR).
    assert.ok(!/dead_on_return|dead_on_unwind/.test(text), `${target}: attributes newer than LLVM 17`);
    // Generated IR is deterministic.
    const again = join(root, `fixture-${target}-again.ll`);
    run(compiler, ["build", fixture, "--no-cache", "--emit", "llvm", "--target", target, "-o", again]);
    assert.equal(readFileSync(again, "utf8"), text, `${target}: IR differs between two builds`);
    compared += expected.size;
  }
  assert.ok(isBundledClang || skipped.length < TARGETS.length, `${clang} emitted the reference for no target`);

  // 2. The host target end to end.
  const cFlags = windows && !isBundledClang
    ? [`--target=${process.arch === "arm64" ? "aarch64" : "x86_64"}-pc-windows-msvc`, "-fuse-ld=lld", "-D_CRT_SECURE_NO_WARNINGS"]
    : [];
  for (const optimization of ["-O0", "-O3"]) {
    const object = join(root, `by-value${optimization}.o`);
    const executable = join(root, `by-value${optimization}${windows ? ".exe" : ""}`);
    run(compiler, ["build", fixture, "--no-cache", optimization, "--emit", "object", "-o", object]);
    run(clang, [...cFlags, optimization, "-I", resolve("tests"), hostC, object, ...(windows ? [] : ["-lm"]), "-o", executable]);
    const result = run(executable, [], false);
    assert.equal(result.status, 0, `by-value host ${optimization}: status ${result.status} ${result.signal ?? ""}\n${result.stdout}\n${result.stderr}`);
  }
  const note = skipped.length === 0 ? "" : ` (no reference from this Clang for ${skipped.join(", ")})`;
  console.log(`C layout by value: ${compared} declares match Clang on ${TARGETS.length - skipped.length} targets${note}; ${process.platform}-${process.arch} end to end at -O0 and -O3 passed`);
} finally {
  rmSync(root, { recursive: true, force: true });
}
