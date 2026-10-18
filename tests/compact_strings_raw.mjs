// PM03: the string runtime of `TSUZURI_STRINGS=compact`, called directly with raw length words.
//
// The compiler writes the IR of tests/fixtures/compact_strings_raw for wasm32, with and without
// `--wasm-feature simd128` (which selects the 128-bit vector kernels of the Latin-1 runtime); the IR has
// the whole runtime, because the runtime files are appended whole. This test adds bridges that pass a pointer
// and a length word, with bit 62 set for a Latin-1 string, to every function that takes over a
// UTF-16 one, and to the kernels under them. Each result is compared with a JavaScript reference,
// each buffer must be freed (the allocator is counted), and the length guards, which a program could
// only reach with petabytes, run in a module that replaces the allocator.
//
//   node tests/compact_strings_raw.mjs [compiler] [--quick]
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const compiler = resolve(process.argv.find((argument, index) => index > 1 && !argument.startsWith("--")) ?? join(root, "target/debug/tsuzuri"));
const quick = process.argv.includes("--quick");
const clang = process.env.TSUZURI_CLANG ?? "clang";
const fixture = join(root, "tests/fixtures/compact_strings_raw");
const scratch = join(root, "target", `compact-raw-${process.pid}`);
mkdirSync(scratch, { recursive: true });
const environment = { ...process.env, TMPDIR: scratch, TSUZURI_CACHE_DIR: join(scratch, "cache"), TSUZURI_STRINGS: "compact" };

function execute(program, args) {
  const result = spawnSync(program, args, { cwd: root, env: environment, encoding: "utf8", timeout: 300_000, maxBuffer: 64 * 1024 * 1024 });
  if (result.error) throw result.error;
  assert.equal(result.status, 0, `${program} ${args.join(" ")}\n${result.stdout}\n${result.stderr}`);
  return result;
}

const LATIN1 = 1n << 62n;
const MASK = LATIN1 - 1n;
const MAX = 9007199254740991n;

// ---- IR bridges ----------------------------------------------------------------------------
const descriptor = (name) =>
  `  %${name}0 = insertvalue %tz.string zeroinitializer, ptr %${name}, 0\n  %${name}s = insertvalue %tz.string %${name}0, i64 %${name}w, 1\n`;
const stringResult = (call) =>
  `  %r = ${call}\n  %d = extractvalue %tz.string %r, 0\n  %n = extractvalue %tz.string %r, 1\n  store ptr %d, ptr %out\n  ret i64 %n\n}\n`;
const flag = (call) => `  %r = ${call}\n  %z = zext i1 %r to i32\n  ret i32 %z\n}\n`;
const bridges = [
  `define i64 @t_new(ptr %src, i64 %w, ptr %out) {\n${stringResult("call %tz.string @tz.string.new(ptr %src, i64 %w)")}`,
  `define i64 @t_from_ascii(ptr %src, i64 %w, ptr %out) {\n${stringResult("call %tz.string @tz.string.from_ascii(ptr %src, i64 %w)")}`,
  `define i64 @t_from_utf8(ptr %src, i64 %w, ptr %out) {\n${stringResult("call %tz.string @tz.string.from_utf8(ptr %src, i64 %w)")}`,
  `define i64 @t_to_well(ptr %src, i64 %w, ptr %out) {\n${stringResult("call %tz.string @tz.string.to_well_formed(ptr %src, i64 %w)")}`,
  `define i64 @t_from_units(ptr %src, i64 %w, ptr %out) {\n${stringResult("call %tz.string @tz.string.from_units(ptr %src, i64 %w)")}`,
  ...["concat", "append"].map((name) => `define i64 @t_${name}(ptr %a, i64 %aw, ptr %b, i64 %bw, ptr %out) {\n${descriptor("a")}${descriptor("b")}${stringResult(`call %tz.string @tz.string.${name}(%tz.string %as, %tz.string %bs)`)}`),
  `define i64 @t_concat_into(ptr %buffer, i64 %capacity, ptr %a, i64 %aw, ptr %b, i64 %bw, ptr %out) {\n${descriptor("a")}${descriptor("b")}${stringResult("call %tz.string @tz.string.concat_into(ptr %buffer, i64 %capacity, %tz.string %as, %tz.string %bs)")}`,
  ...["to_utf16", "borrow_utf16"].map((name) => `define i64 @t_${name}(ptr %a, i64 %aw, ptr %out) {\n${descriptor("a")}${stringResult(`call %tz.string @tz.string.${name}(%tz.string %as)`)}`),
  `define i32 @t_equal(ptr %a, i64 %aw, ptr %b, i64 %bw) {\n${descriptor("a")}${descriptor("b")}${flag("call i1 @tz.string.equal(%tz.string %as, %tz.string %bs)")}`,
  `define i32 @t_compare(ptr %a, i64 %aw, ptr %b, i64 %bw) {\n${descriptor("a")}${descriptor("b")}  %r = call i32 @tz.string.compare(%tz.string %as, %tz.string %bs)\n  ret i32 %r\n}\n`,
  `define i32 @t_to_ascii(ptr %dst, ptr %src, i64 %w) {\n${flag("call i1 @tz.string.to_ascii(ptr %dst, ptr %src, i64 %w)")}`,
  `define i32 @t_well(ptr %src, i64 %w) {\n${flag("call i1 @tz.string.is_well_formed(ptr %src, i64 %w)")}`,
  `define i64 @t_encode(ptr %src, i64 %w, ptr %out) {\n  %r = call %tz.utf8string @tz.utf8string.from_string(ptr %src, i64 %w)\n  %d = extractvalue %tz.utf8string %r, 0\n  %n = extractvalue %tz.utf8string %r, 1\n  store ptr %d, ptr %out\n  ret i64 %n\n}\n`,
  `define i32 @t_parse(ptr %src, i64 %w, i32 %bits, i32 %signed, ptr %value) {\n  %s = trunc i32 %signed to i1\n${flag("call i1 @tz.integer.parse(ptr %src, i64 %w, i32 %bits, i1 %s, ptr %value)")}`,
  `define i32 @t_parse128(ptr %src, i64 %w, i32 %signed, ptr %value) {\n  %s = trunc i32 %signed to i1\n${flag("call i1 @tz.integer.parse128(ptr %src, i64 %w, i1 %s, ptr %value)")}`,
  `define i64 @t_char_display(i32 %v, ptr %out) {\n${stringResult("call %tz.string @tz.character.display(i32 %v)")}`,
  `define i32 @t_char_parse(ptr %src, i64 %w, i32 %scalar) {\n  %slot = alloca %tz.string, align 8\n  %t0 = insertvalue %tz.string zeroinitializer, ptr %src, 0\n  %ts = insertvalue %tz.string %t0, i64 %w, 1\n  store %tz.string %ts, ptr %slot\n  %s = trunc i32 %scalar to i1\n  %r = call i32 @tz.character.parse(ptr %slot, i1 %s)\n  ret i32 %r\n}\n`,
  `define i64 @t_join(ptr %parts, i64 %count, i32 %kind, ptr %out) {\n  %p0 = insertvalue %tz.array zeroinitializer, ptr %parts, 0\n  %p1 = insertvalue %tz.array %p0, i64 %count, 1\n${stringResult("call %tz.string @tz.display.join(%tz.array %p1, i32 %kind)")}`,
  `define i64 @t_quote(ptr %src, i64 %w, i32 %kind, ptr %out) {\n${stringResult("call %tz.string @tz.display.quote(ptr %src, i64 %w, i32 %kind)")}`,
  `define i64 @t_utf8_scan(ptr %p, i64 %n, ptr %max) {\n  %r = call { i64, i32 } @tz.string.utf8_scan(ptr %p, i64 %n)\n  %u = extractvalue { i64, i32 } %r, 0\n  %m = extractvalue { i64, i32 } %r, 1\n  store i32 %m, ptr %max\n  ret i64 %u\n}\n`,
  `define void @t_utf8_to_latin1(ptr %p, i64 %n, ptr %data) {\n  call void @tz.string.utf8_to_latin1(ptr %p, i64 %n, ptr %data)\n  ret void\n}\n`,
  `define i64 @t_slice(ptr %data, i64 %w, i64 %first, i64 %last, ptr %out) {\n${stringResult("call %tz.string @tz.string.slice(ptr %data, i64 %w, i64 %first, i64 %last)")}`,
  `define i64 @t_string_join(ptr %separator, i64 %sw, ptr %parts, i64 %count, ptr %out) {\n${stringResult("call %tz.string @tz.string.join(ptr %separator, i64 %sw, ptr %parts, i64 %count)")}`,
  `define i32 @t_is_latin1_units(ptr %p, i64 %n) {\n${flag("call i1 @tz.string.is_latin1_units(ptr %p, i64 %n)")}`,
  `define void @t_widen_in_place(ptr %p, i64 %n) {\n  call void @tz.string.widen_in_place(ptr %p, i64 %n)\n  ret void\n}\n`,
  `define void @t_narrow_in_place(ptr %p, i64 %n) {\n  call void @tz.string.narrow_in_place(ptr %p, i64 %n)\n  ret void\n}\n`,
  `define i32 @t_equal_mixed(ptr %a, ptr %b, i64 %n) {\n${flag("call i1 @tz.string.equal_mixed(ptr %a, ptr %b, i64 %n)")}`,
  `define i32 @t_compare_mixed(ptr %a, i64 %an, ptr %b, i64 %bn) {\n  %r = call i32 @tz.string.compare_mixed(ptr %a, i64 %an, ptr %b, i64 %bn)\n  ret i32 %r\n}\n`,
  `define i64 @t_from_latin1(ptr %src, i64 %n, ptr %out) {\n  %r = call %tz.utf8string @tz.utf8string.from_latin1(ptr %src, i64 %n)\n  %d = extractvalue %tz.utf8string %r, 0\n  %m = extractvalue %tz.utf8string %r, 1\n  store ptr %d, ptr %out\n  ret i64 %m\n}\n`,
  `define ptr @t_alloc(i64 %n) {\n  %p = call ptr @tz.alloc(i64 %n)\n  ret ptr %p\n}\n`,
  `define void @t_free(ptr %p) {\n  call void @tz.free(ptr %p)\n  ret void\n}\n`,
  ...["live", "calls", "bytes", "reallocs"].map((name) => `define i64 @t_${name}() {\n  %v = load i64, ptr @test.${name}\n  ret i64 %v\n}\n`),
  `define void @t_reset() {\n  store i64 0, ptr @test.calls\n  store i64 0, ptr @test.bytes\n  store i64 0, ptr @test.reallocs\n  ret void\n}\n`,
].join("");
const exported = [...bridges.matchAll(/^define \S+ @(t_\w+)\(/gm)].map((match) => match[1]);
const counting = `
@test.live = internal global i64 0
@test.calls = internal global i64 0
@test.bytes = internal global i64 0
@test.reallocs = internal global i64 0
define internal ptr @tz.alloc(i64 %size) nounwind {
  %c = load i64, ptr @test.calls
  %c1 = add i64 %c, 1
  store i64 %c1, ptr @test.calls
  %b = load i64, ptr @test.bytes
  %b1 = add i64 %b, %size
  store i64 %b1, ptr @test.bytes
  %l = load i64, ptr @test.live
  %l1 = add i64 %l, 1
  store i64 %l1, ptr @test.live
  %p = call ptr @test.base_alloc(i64 %size)
  ret ptr %p
}
define internal void @tz.free(ptr %p) nounwind {
entry:
  %null = icmp eq ptr %p, null
  br i1 %null, label %done, label %count
count:
  %l = load i64, ptr @test.live
  %l1 = sub i64 %l, 1
  store i64 %l1, ptr @test.live
  call void @test.base_free(ptr %p)
  br label %done
done:
  ret void
}
define internal ptr @tz.realloc(ptr %old, i64 %o, i64 %n) nounwind {
  %r = load i64, ptr @test.reallocs
  %r1 = add i64 %r, 1
  store i64 %r1, ptr @test.reallocs
  %p = call ptr @test.base_realloc(ptr %old, i64 %o, i64 %n)
  ret ptr %p
}
`;

function compile(optimization, simd) {
  const tag = `${simd ? "simd-" : ""}O${optimization}`;
  const ir = join(scratch, `raw-${tag}.ll`);
  execute(compiler, ["build", fixture, "--target", "wasm32", `-O${optimization}`, ...(simd ? ["--wasm-feature", "simd128"] : []), "--emit", "llvm", "-o", ir]);
  let text = readFileSync(ir, "utf8");
  for (const [name, type] of [["alloc", "ptr"], ["free", "void"], ["realloc", "ptr"]]) {
    const head = `define internal ${type} @tz.${name}(`;
    assert.ok(text.includes(head), `the runtime defines ${head}`);
    text = text.replace(head, `define internal ${type} @test.base_${name}(`);
  }
  writeFileSync(ir, text + counting + bridges);
  const wasm = join(scratch, `raw-${tag}.wasm`);
  execute(clang, [
    "--target=wasm32-unknown-unknown", `-O${optimization}`, "-ffreestanding", "-fno-builtin", "-Wno-override-module",
    "-mbulk-memory", simd ? "-msimd128" : "-mno-simd128", "-nostdlib", ir, "-Wl,--no-entry", "-Wl,--export-memory", "-Wl,-z,stack-size=1048576",
    "-Wl,--max-memory=67108864", ...exported.map((name) => `-Wl,--export=${name}`), "-o", wasm,
  ]);
  const module = new WebAssembly.Module(readFileSync(wasm));
  assert.deepEqual(WebAssembly.Module.imports(module), [], "the runtime needs no imports");
  // The vector kernels are in the module exactly when the build asks for simd128.
  const instructions = disassemble(wasm);
  if (instructions) assert.equal(/\bv128\./.test(instructions), simd, `${tag}: vector instructions iff simd128`);
  return { module, text };
}

// The text of the module's instructions, from llvm-objdump (TSUZURI_OBJDUMP, or next to clang); null when there is none.
function disassemble(wasm) {
  const objdump = process.env.TSUZURI_OBJDUMP ?? join(dirname(clang), "llvm-objdump");
  const result = spawnSync(objdump, ["-d", wasm], { encoding: "utf8", maxBuffer: 256 * 1024 * 1024 });
  if (result.error || result.status !== 0) return "";
  return result.stdout;
}

// ---- the JavaScript side -------------------------------------------------------------------
let api;
let checks = 0;
let OUT = 0;
let baseline = 0n;
const bytes = () => new Uint8Array(api.memory.buffer);
const view = () => new DataView(api.memory.buffer);
const hasUnitsAbove = (units, limit) => units.some((unit) => unit > limit);

// A string of `form` "L" (Latin-1), "U" (UTF-16), or "E" (the empty string, which is the word 0).
function make(units, form) {
  if (units.length === 0) return { units, ptr: 0, word: 0n, form: "E" };
  if (form === "L") {
    assert.ok(!hasUnitsAbove(units, 255), "a Latin-1 string has units of at most 0xFF");
    const ptr = api.t_alloc(BigInt(units.length));
    bytes().set(units, ptr);
    return { units, ptr, word: BigInt(units.length) | LATIN1, form };
  }
  const ptr = api.t_alloc(BigInt(units.length * 2));
  new Uint16Array(api.memory.buffer, ptr, units.length).set(units);
  return { units, ptr, word: BigInt(units.length), form: "U" };
}
// The forms that a text can have: any text is UTF-16, and a text of units up to 0xFF is also Latin-1.
const formsOf = (units) => (units.length === 0 ? ["E"] : hasUnitsAbove(units, 255) ? ["U"] : ["U", "L"]);
function read(ptr, word) {
  const length = Number(word & MASK);
  if (word & LATIN1) return { form: "L", units: Array.from(bytes().subarray(ptr, ptr + length)) };
  return { form: word === 0n ? "E" : "U", units: Array.from(new Uint16Array(api.memory.buffer, ptr, length)) };
}
const output = (word) => {
  const ptr = view().getUint32(OUT, true);
  return { ptr, word, ...read(ptr, word) };
};
const drop = (...strings) => strings.forEach((text) => text.ptr && api.t_free(text.ptr));
const same = (actual, expected, label) => {
  checks += 1;
  assert.deepEqual(actual, expected, label);
};
const unitsOf = (text) => Array.from({ length: text.length }, (_, index) => text.charCodeAt(index));
const order = (a, b) => {
  const length = Math.min(a.length, b.length);
  for (let index = 0; index < length; ++index) if (a[index] !== b[index]) return a[index] < b[index] ? -1 : 1;
  return a.length === b.length ? 0 : a.length < b.length ? -1 : 1;
};
const sizeList = quick ? [0, 1, 7, 8, 9, 33] : [0, 1, 2, 3, 7, 8, 9, 15, 16, 17, 31, 32, 33, 63, 64, 65];
const ascii = (length, offset = 0) => Array.from({ length }, (_, index) => 97 + ((index + offset) % 26));
const pattern = [0x41, 0xe9, 0xff, 0x80, 0xa0, 0x7f, 0x00, 0xc2, 0xc3, 0x20];
const latin = (length, offset = 0) => Array.from({ length }, (_, index) => pattern[(index + offset) % pattern.length]);
const wide = (length, offset = 0) => Array.from({ length }, (_, index) => (index + offset) % 5 === 3 ? 0x3042 : pattern[(index + offset) % pattern.length]);
const unitSamples = [
  [], [0], [0x41], [0xff], [0x100], [0xe9], [0xd800], [0xdc00], [0xd83d, 0xde00], [0xdc00, 0xd800],
  ascii(5), ascii(8), ascii(9), latin(1), latin(7), latin(16), latin(17), latin(33), wide(1), wide(9), wide(17), wide(40),
  [0x41, 0xd800, 0x42], [0xff, 0x100], [0x100, 0xff],
];
const tracked = (action) => {
  api.t_reset();
  const value = action();
  return { value, calls: Number(api.t_calls()), bytes: Number(api.t_bytes()) };
};
const noLeaks = (label) => {
  checks += 1;
  assert.equal(api.t_live(), baseline, `${label}: every buffer is freed`);
};

function run(module, label) {
  api = new WebAssembly.Instance(module).exports;
  OUT = api.t_alloc(64n);
  baseline = api.t_live();

  // new, from_ascii, to_well_formed
  for (const length of sizeList) for (const units of [ascii(length), latin(length), wide(length)]) for (const form of formsOf(units)) {
    const input = make(units, form);
    const { value, calls, bytes: size } = tracked(() => output(api.t_new(input.ptr, input.word, OUT)));
    same([value.form, value.units], [form, units], `new ${form} ${units}`);
    same([calls, size], units.length === 0 ? [0, 0] : [1, form === "U" ? units.length * 2 : units.length], `new allocation ${form} ${units.length}`);
    const repaired = output(api.t_to_well(input.ptr, input.word, OUT));
    same([repaired.form, repaired.units], [form, unitsOf(String.fromCharCode(...units).toWellFormed())], `to_well_formed ${form} ${units}`);
    drop(value, repaired, input);
  }
  for (const length of sizeList) {
    const units = ascii(length);
    const source = make(units, "L");
    const { value, calls, bytes: size } = tracked(() => output(api.t_from_ascii(source.ptr, BigInt(length), OUT)));
    same([value.form, value.units, calls, size], [length === 0 ? "E" : "L", units, length === 0 ? 0 : 1, length], `from_ascii ${length}`);
    drop(value, source);
  }
  noLeaks(`${label}: new`);

  // concat, append, concat_into
  for (const left of unitSamples) for (const right of unitSamples) for (const leftForm of formsOf(left)) for (const rightForm of formsOf(right)) {
    const units = [...left, ...right];
    const latinResult = leftForm !== "U" && rightForm !== "U" && units.length > 0;
    const expected = [units.length === 0 ? "E" : latinResult ? "L" : "U", units];
    const name = `${leftForm}${left.length}+${rightForm}${right.length}`;
    const a = make(left, leftForm);
    const b = make(right, rightForm);
    const { value, calls, bytes: size } = tracked(() => output(api.t_concat(a.ptr, a.word, b.ptr, b.word, OUT)));
    same([value.form, value.units], expected, `concat ${name}`);
    same([calls, size], units.length === 0 ? [0, 0] : [1, latinResult ? units.length : units.length * 2], `concat ${name} allocates one buffer of the result's size`);
    drop(value);
    const owned = make(left, leftForm);
    const appended = output(api.t_append(owned.ptr, owned.word, b.ptr, b.word, OUT));
    same([appended.form, appended.units], expected, `append ${name}`);
    drop(appended);
    for (const capacity of [0, 1, 4, 16, 128]) {
      const buffer = api.t_alloc(512n);
      const into = output(api.t_concat_into(buffer, BigInt(capacity), a.ptr, a.word, b.ptr, b.word, OUT));
      same([into.form, into.units], expected, `concat_into ${name} capacity ${capacity}`);
      if (units.length > 0) {
        const fits = latinResult ? units.length <= capacity * 2 : units.length <= capacity;
        same(into.ptr === buffer, fits, `concat_into ${name} capacity ${capacity} uses the buffer iff the result fits`);
      }
      if (into.ptr !== buffer) drop(into);
      api.t_free(buffer);
    }
    drop(a, b);
  }
  noLeaks(`${label}: concat`);

  // equal, compare
  for (const left of unitSamples) for (const right of unitSamples) for (const leftForm of formsOf(left)) for (const rightForm of formsOf(right)) {
    const a = make(left, leftForm);
    const b = make(right, rightForm);
    const name = `${leftForm}${JSON.stringify(left)} ${rightForm}${JSON.stringify(right)}`;
    same(api.t_equal(a.ptr, a.word, b.ptr, b.word), order(left, right) === 0 ? 1 : 0, `equal ${name}`);
    same(api.t_compare(a.ptr, a.word, b.ptr, b.word), order(left, right), `compare ${name}`);
    drop(a, b);
  }
  noLeaks(`${label}: equal`);

  // to_ascii
  for (const units of [...unitSamples, ascii(4096), ascii(4097), [...ascii(4095), 128], latin(4096)]) for (const form of formsOf(units)) {
    const input = make(units, form);
    const target = api.t_alloc(BigInt(Math.max(units.length, 1)));
    const expected = units.length <= 4096 && !hasUnitsAbove(units, 127);
    same(api.t_to_ascii(target, input.ptr, input.word), expected ? 1 : 0, `to_ascii ${form}${units.length}`);
    if (expected) same(Array.from(bytes().subarray(target, target + units.length)), units, "to_ascii copies the bytes");
    api.t_free(target);
    drop(input);
  }
  noLeaks(`${label}: to_ascii`);

  // from_utf8 and utf8string.from_string
  for (const text of ["", "a", "hello world, hello world", "caf\u00e9", "\u00ff\u0100", "\u3042\u3044", "\u{1f600}x", "a\0b", "\u0080", "\u07ff\u0800", "x".repeat(70), `${"y".repeat(40)}\u00e9${"z".repeat(40)}`]) {
    const encoded = Buffer.from(text, "utf8");
    const input = api.t_alloc(BigInt(Math.max(encoded.length, 1)));
    bytes().set(encoded, input);
    const decoded = output(api.t_from_utf8(input, BigInt(encoded.length), OUT));
    const expectLatin = Array.from(text).every((character) => character.codePointAt(0) <= 0xff);
    same([decoded.form, decoded.units], [text.length === 0 ? "E" : expectLatin ? "L" : "U", unitsOf(text)], `from_utf8 ${JSON.stringify(text)}`);
    api.t_free(input);
    drop(decoded);
    for (const form of formsOf(unitsOf(text))) {
      const source = make(unitsOf(text), form);
      const count = api.t_encode(source.ptr, source.word, OUT);
      const pointer = view().getUint32(OUT, true);
      same(Buffer.from(bytes().subarray(pointer, pointer + Number(count))).toString("hex"), encoded.toString("hex"), `from_string ${form} ${JSON.stringify(text)}`);
      if (pointer) api.t_free(pointer);
      drop(source);
    }
  }
  for (const length of sizeList) {
    const units = latin(length);
    if (units.length === 0) continue;
    const source = make(units, "L");
    const count = api.t_from_latin1(source.ptr, BigInt(length), OUT);
    const pointer = view().getUint32(OUT, true);
    same(Buffer.from(bytes().subarray(pointer, pointer + Number(count))).toString("hex"), Buffer.from(String.fromCharCode(...units), "utf8").toString("hex"), `from_latin1 ${length}`);
    api.t_free(pointer);
    drop(source);
  }
  noLeaks(`${label}: utf8`);

  // utf8_scan and utf8_to_latin1: ASCII and the pairs 0xC2/0xC3 + continuation make Latin-1; any other text, valid or not, is { -1, 0 }
  const decoder = new TextDecoder("utf-8", { fatal: true, ignoreBOM: true });
  const scanned = (encoded) => {
    let text;
    try {
      text = decoder.decode(encoded);
    } catch {
      return { units: -1n, max: 0, latin1: null };
    }
    let max = 0;
    for (const character of text) {
      const code = character.codePointAt(0);
      if (code > 0xff) return { units: -1n, max: 0, latin1: null };
      max = Math.max(max, code);
    }
    return { units: BigInt(text.length), max, latin1: Array.from(text, (character) => character.charCodeAt(0)) };
  };
  const scanCase = (encoded) => {
    const input = api.t_alloc(BigInt(encoded.length + 40));
    bytes().fill(0xc2, input, input + encoded.length + 40);
    bytes().set(encoded, input);
    const units = api.t_utf8_scan(input, BigInt(encoded.length), OUT + 16);
    const max = view().getInt32(OUT + 16, true);
    const expected = scanned(encoded);
    same([units, max], [expected.units, expected.max], `utf8_scan ${Array.from(encoded)}`);
    if (expected.latin1) {
      const target = api.t_alloc(BigInt(expected.latin1.length + 8));
      bytes().fill(0x5a, target, target + expected.latin1.length + 8);
      api.t_utf8_to_latin1(input, BigInt(encoded.length), target);
      same(Array.from(bytes().subarray(target, target + expected.latin1.length + 8)), [...expected.latin1, ...Array(8).fill(0x5a)], `utf8_to_latin1 ${Array.from(encoded)}`);
      api.t_free(target);
    }
    api.t_free(input);
  };
  const pairs = [[0xc2, 0x80], [0xc3, 0xa9], [0xc3, 0xbf], [0xc2, 0xbf], [0xc3, 0x80], [0xc2, 0xa0]];
  const invalid = [[0xc2], [0xc3], [0x80], [0xbf], [0xc0, 0x80], [0xc1, 0xbf], [0xc4, 0x80], [0xdf, 0xbf], [0xe2, 0x82, 0xac], [0xf0, 0x9f, 0x98, 0x80],
    [0xed, 0xa0, 0x80], [0xff], [0xc2, 0x41], [0xc3, 0xc3, 0x80], [0xc2, 0x80, 0x80], [0xef, 0xbb, 0xbf], [0xe0, 0x80, 0x80], [0xf4, 0x90, 0x80, 0x80]];
  const step = quick ? 5 : 1;
  for (let length = 0; length <= 70; ++length) {
    scanCase(Uint8Array.from(ascii(length)));
    for (let at = 0; at < length; at += at < 40 ? step : 1) {
      for (const pair of pairs) {
        if (at + 2 > length) continue;
        const text = ascii(length);
        text.splice(at, 2, ...pair);
        scanCase(Uint8Array.from(text));
      }
      for (const bad of invalid) {
        if (at + bad.length > length) continue;
        const text = ascii(length);
        text.splice(at, bad.length, ...bad);
        scanCase(Uint8Array.from(text));
      }
    }
  }
  // text that is all pairs, pairs that touch every block boundary, and random mutations of such text
  let state = 0x2545f491;
  const random = (limit) => {
    state = (Math.imul(state, 1103515245) + 12345) >>> 0;
    return (state >>> 8) % limit;
  };
  for (let round = 0; round < (quick ? 400 : 3000); ++round) {
    const encoded = [];
    const target = random(100);
    while (encoded.length < target) {
      if (random(3) === 0 && encoded.length + 2 <= target) encoded.push(...pairs[random(pairs.length)]);
      else encoded.push(0x20 + random(95));
    }
    scanCase(Uint8Array.from(encoded));
    if (encoded.length) {
      const mutated = [...encoded];
      mutated[random(mutated.length)] = random(256);
      scanCase(Uint8Array.from(mutated));
    }
  }
  noLeaks(`${label}: utf8 scan`);

  // is_well_formed
  for (const units of unitSamples) for (const form of formsOf(units)) {
    const input = make(units, form);
    same(api.t_well(input.ptr, input.word), String.fromCharCode(...units).isWellFormed() ? 1 : 0, `is_well_formed ${form}${units}`);
    drop(input);
  }

  // to_utf16, borrow_utf16, from_units
  for (const units of unitSamples) for (const form of formsOf(units)) {
    const input = make(units, form);
    const borrowed = output(api.t_borrow_utf16(input.ptr, input.word, OUT));
    same([borrowed.form, borrowed.units], [units.length === 0 ? "E" : "U", units], `borrow_utf16 ${form}${units}`);
    same(borrowed.ptr === input.ptr, form !== "L", "borrow_utf16 only makes a copy of a Latin-1 string");
    if (borrowed.ptr !== input.ptr) drop(borrowed);
    const owned = output(api.t_to_utf16(input.ptr, input.word, OUT));
    same([owned.form, owned.units], [units.length === 0 ? "E" : "U", units], `to_utf16 ${form}${units}`);
    drop(owned);
  }
  for (const length of sizeList) for (const units of [latin(length), wide(length), ascii(length)]) {
    const owned = make(units, "U");
    const text = output(api.t_from_units(owned.ptr, BigInt(length), OUT));
    const expectLatin = length > 0 && !hasUnitsAbove(units, 255);
    same([text.form, text.units], [length === 0 ? "E" : expectLatin ? "L" : "U", units], `from_units ${units}`);
    drop(text);
  }
  noLeaks(`${label}: conversions`);

  // the kernels, around every size that a word or a vector would split
  for (let length = 0; length <= 70; ++length) {
    const unitText = make(latin(length), "U");
    if (length) same(api.t_is_latin1_units(unitText.ptr, BigInt(length)), 1, `is_latin1_units ${length}`);
    for (let at = 0; at < length; ++at) {
      const view16 = new Uint16Array(api.memory.buffer, unitText.ptr, length);
      const kept = view16[at];
      for (const high of [0x100, 0x3042, 0xd800, 0xff00]) {
        view16[at] = high;
        same(api.t_is_latin1_units(unitText.ptr, BigInt(length)), 0, `is_latin1_units ${length} with ${high.toString(16)} at ${at}`);
      }
      view16[at] = kept;
    }
    drop(unitText);
    if (length) {
      const buffer = api.t_alloc(BigInt(length * 2 + 16));
      bytes().fill(0x5a, buffer, buffer + length * 2 + 16);
      const bytesUnits = latin(length);
      bytes().set(bytesUnits, buffer);
      api.t_widen_in_place(buffer, BigInt(length));
      same(Array.from(new Uint16Array(api.memory.buffer, buffer, length)), bytesUnits, `widen_in_place ${length}`);
      same(Array.from(bytes().subarray(buffer + length * 2, buffer + length * 2 + 16)), Array(16).fill(0x5a), "widen_in_place stays inside the buffer");
      api.t_narrow_in_place(buffer, BigInt(length));
      same(Array.from(bytes().subarray(buffer, buffer + length)), bytesUnits, `narrow_in_place ${length}`);
      api.t_free(buffer);
    }
    for (const otherLength of new Set([0, 1, length - 1, length, length + 1].filter((value) => value >= 0))) {
      const a = latin(length);
      const b = latin(otherLength);
      const bytesText = make(a, "L");
      const unitsText = make(b, "U");
      if (length === otherLength && length) same(api.t_equal_mixed(bytesText.ptr, unitsText.ptr, BigInt(length)), 1, `equal_mixed ${length}`);
      same(api.t_compare_mixed(bytesText.ptr, BigInt(length), unitsText.ptr, BigInt(otherLength)), order(a, b), `compare_mixed ${length} ${otherLength}`);
      drop(bytesText, unitsText);
    }
    for (let at = 0; at < length; ++at) {
      for (const changed of [0, Math.max(0, latin(length)[at] - 1), latin(length)[at] + 1, 0xff, 0x100, 0xffff]) {
        const a = latin(length);
        const b = [...a];
        b[at] = changed;
        const bytesText = make(a, "L");
        const unitsText = make(b, "U");
        same(api.t_equal_mixed(bytesText.ptr, unitsText.ptr, BigInt(length)), order(a, b) === 0 ? 1 : 0, `equal_mixed ${length} differing at ${at}`);
        same(api.t_compare_mixed(bytesText.ptr, BigInt(length), unitsText.ptr, BigInt(length)), order(a, b), `compare_mixed ${length} differing at ${at} to ${changed}`);
        drop(bytesText, unitsText);
      }
    }
  }
  noLeaks(`${label}: kernels`);

  // integer parsers: the Latin-1 copy gives the result of the UTF-16 code for every text
  const alphabet = "0123456789abcdefABCDEFxXbBoO_+- \t.\u00e9\u00ff\u0660";
  let seed = 12345;
  const next = () => (seed = (Math.imul(seed, 1103515245) + 12345) >>> 0);
  const texts = ["", "0", "-0", "+0", "42", "-42", "0x2a", "0b101", "0o17", "1_000", "_1", "1_", "1__0", "255", "256", "-128", "-129", "127", "128",
    "18446744073709551615", "18446744073709551616", "-9223372036854775808", "-9223372036854775809", "9223372036854775807", "9223372036854775808",
    "340282366920938463463374607431768211455", "340282366920938463463374607431768211456", "-170141183460469231731687303715884105728",
    "0xffffffffffffffff", "0x10000000000000000", "0xffffffffffffffffffffffffffffffff", `0b${"1".repeat(64)}`, `0b${"1".repeat(65)}`, `0b${"1".repeat(128)}`,
    "\u00e9", "1\u00e9", "\u0661", "0".repeat(4096), "0".repeat(4097), "1".repeat(19), "1".repeat(20), "9".repeat(19), "9".repeat(20)];
  for (let count = 0; count < (quick ? 400 : 3000); ++count) {
    const length = next() % 14;
    texts.push(Array.from({ length }, () => alphabet[next() % alphabet.length]).join(""));
  }
  const slot = OUT + 16;
  const readValue = () => [view().getBigUint64(slot, true), view().getBigUint64(slot + 8, true)];
  for (const text of texts) {
    const units = unitsOf(text);
    if (units.length === 0 || hasUnitsAbove(units, 255)) continue;
    const reference = make(units, "U");
    const input = make(units, "L");
    for (const [bits, signed] of [[8, 0], [8, 1], [16, 0], [16, 1], [32, 0], [32, 1], [64, 0], [64, 1]]) {
      bytes().fill(0, slot, slot + 16);
      const expected = api.t_parse(reference.ptr, reference.word, bits, signed, slot);
      const wanted = readValue()[0];
      bytes().fill(0, slot, slot + 16);
      const actual = api.t_parse(input.ptr, input.word, bits, signed, slot);
      same([actual, actual ? readValue()[0] : 0n], [expected, expected ? wanted : 0n], `parse ${JSON.stringify(text)} i${bits}${signed ? "" : "u"}`);
    }
    for (const signed of [0, 1]) {
      bytes().fill(0, slot, slot + 16);
      const expected = api.t_parse128(reference.ptr, reference.word, signed, slot);
      const wanted = readValue();
      bytes().fill(0, slot, slot + 16);
      const actual = api.t_parse128(input.ptr, input.word, signed, slot);
      same([actual, actual ? readValue() : [0n, 0n]], [expected, expected ? wanted : [0n, 0n]], `parse128 ${JSON.stringify(text)}`);
    }
    drop(reference, input);
  }
  for (const [text, bits, signed, ok, value] of [["42", 8, 1, 1, 42n], ["-128", 8, 1, 1, 0xffffffffffffff80n], ["-129", 8, 1, 0, 0n], ["255", 8, 0, 1, 255n], ["256", 8, 0, 0, 0n], ["0x2a", 64, 0, 1, 42n], ["1_0", 16, 0, 1, 10n]]) {
    const input = make(unitsOf(text), "L");
    bytes().fill(0, slot, slot + 16);
    same([api.t_parse(input.ptr, input.word, bits, signed, slot), ok ? readValue()[0] : 0n], [ok, value], `parse of the Latin-1 text ${JSON.stringify(text)}`);
    drop(input);
  }
  noLeaks(`${label}: integers`);

  // character.display and character.parse
  for (const value of [0, 1, 0x41, 0x7f, 0x80, 0xa0, 0xe9, 0xfe, 0xff, 0x100, 0x3042, 0xd7ff, 0xd800, 0xdbff, 0xdc00, 0xdfff, 0xe000, 0xfffd, 0xffff, 0x10000, 0x1f600, 0x10ffff]) {
    const text = output(api.t_char_display(value, OUT));
    const expectedUnits = value > 0xffff ? unitsOf(String.fromCodePoint(value)) : [value];
    same([text.form, text.units], [value <= 0xff ? "L" : "U", expectedUnits], `character.display ${value.toString(16)}`);
    const surrogate = value >= 0xd800 && value <= 0xdfff;
    same(api.t_char_parse(text.ptr, text.word, 1), surrogate ? -1 : value, `character.parse scalar ${value.toString(16)}`);
    same(api.t_char_parse(text.ptr, text.word, 0), value > 0xffff ? -1 : value, `character.parse unit ${value.toString(16)}`);
    drop(text);
  }
  for (const units of [[], [0x41, 0x42], [0xff, 0xff], [0x41, 0x42, 0x43]]) for (const form of formsOf(units)) {
    const input = make(units, form);
    same(api.t_char_parse(input.ptr, input.word, 1), -1, `character.parse of ${form}${units}`);
    drop(input);
  }
  noLeaks(`${label}: characters`);

  // slice and join, which the standard library's String.slice and String.join call
  for (const units of unitSamples) for (const form of formsOf(units)) {
    const text = make(units, form);
    for (let first = 0; first <= units.length; ++first) for (let last = first; last <= units.length; ++last) {
      const { value, calls, bytes: size } = tracked(() => output(api.t_slice(text.ptr, text.word, BigInt(first), BigInt(last), OUT)));
      const wanted = units.slice(first, last);
      same([value.form, value.units], [wanted.length === 0 ? "E" : form, wanted], `slice ${form}${units}[${first}, ${last})`);
      same([calls, size], wanted.length === 0 ? [0, 0] : [1, form === "U" ? wanted.length * 2 : wanted.length], "slice makes one buffer of the slice's size");
      drop(value);
    }
    drop(text);
  }
  for (const [separatorUnits, partList] of [
    [[44], [ascii(3), latin(2), [], ascii(1)]], [[], [ascii(2), ascii(2)]], [[32, 0xe9], [latin(3), latin(1)]],
    [[0x3042], [ascii(2), ascii(2), ascii(1)]], [[44], [ascii(1), wide(3), ascii(1)]], [[44], [wide(2)]], [[0x3042], [ascii(2)]],
    [[44], []], [[44], [[]]], [[44], [[], []]], [ascii(40), [latin(40), latin(33)]], [[1], Array.from({ length: 20 }, (_, index) => ascii(index % 5))],
  ]) for (const separatorForm of formsOf(separatorUnits)) for (const makeUtf16 of [false, true]) {
    const separator = make(separatorUnits, separatorForm);
    const strings = partList.map((units) => make(units, makeUtf16 || hasUnitsAbove(units, 255) ? "U" : "L"));
    const array = api.t_alloc(BigInt(Math.max(strings.length, 1) * 16));
    strings.forEach((text, index) => {
      view().setUint32(array + index * 16, text.ptr, true);
      view().setBigUint64(array + index * 16 + 8, text.word, true);
    });
    const expected = partList.flatMap((units, index) => (index ? [...separatorUnits, ...units] : units));
    const wide = strings.some((text) => text.form === "U") || (separator.form === "U" && strings.length > 1);
    const { value, calls } = tracked(() => output(api.t_string_join(separator.ptr, separator.word, array, BigInt(strings.length), OUT)));
    same([value.form, value.units], [expected.length === 0 ? "E" : wide ? "U" : "L", expected], `join ${separatorForm}${separatorUnits} of ${strings.map((text) => text.form)}`);
    same(calls, expected.length === 0 ? 0 : 1, "join allocates once");
    drop(value, separator, ...strings);
    api.t_free(array);
  }
  noLeaks(`${label}: slice and join`);

  // display.join and display.quote
  const partSamples = [[], ascii(3), latin(5), [0xe9], wide(4), [0x3042]];
  for (const kind of [0, 1, 2]) for (const count of [0, 1, 2, 3]) for (const offset of [0, 1, 2]) for (const mix of [0, 1, 2]) {
    const parts = Array.from({ length: count }, (_, index) => partSamples[(offset + index * (mix + 1)) % partSamples.length]);
    // mix 2 keeps every other part as UTF-16 even when it has only Latin-1 units
    const strings = parts.map((units, index) => make(units, hasUnitsAbove(units, 255) || (mix === 2 && index % 2 === 0) ? "U" : "L"));
    const array = api.t_alloc(BigInt(Math.max(count, 1) * 16));
    strings.forEach((text, index) => {
      view().setUint32(array + index * 16, text.ptr, true);
      view().setBigUint64(array + index * 16 + 8, text.word, true);
    });
    const joined = output(api.t_join(array, BigInt(count), kind, OUT));
    const [open, close] = [["(", ")"], ["[", "]"], ["[|", "|]"]][kind];
    const expected = [...unitsOf(open), ...parts.flatMap((units, index) => (index ? [44, 32, ...units] : units)), ...unitsOf(close)];
    same([joined.form, joined.units], [strings.some((text) => text.form === "U") ? "U" : "L", expected], `join kind ${kind} of ${strings.map((text) => text.form)}`);
    drop(joined);
  }
  for (const units of [[], ascii(4), [0x22, 0x5c, 0x27], [0, 9, 10, 13, 1, 31, 127], latin(12), [0xe9, 0xff, 0x80], wide(6), [0xd800], [0xd83d, 0xde00]]) for (const kind of [0, 1, 2, 3]) {
    const reference = make(units, "U");
    const expected = output(api.t_quote(reference.ptr, reference.word, kind, OUT));
    drop(reference);
    if (!hasUnitsAbove(units, 255)) {
      const input = make(units, "L");
      const quoted = output(api.t_quote(input.ptr, input.word, kind, OUT));
      same([quoted.form, quoted.units], ["L", expected.units], `quote kind ${kind} of a Latin-1 or empty string ${units}`);
      drop(quoted, input);
    }
    drop(expected);
  }
  const newline = make([97, 10], "L");
  const quoted = output(api.t_quote(newline.ptr, newline.word, 0, OUT));
  same(quoted.units, unitsOf('"a\\n"'), "quote writes the escape of a newline");
  drop(quoted, newline);
  noLeaks(`${label}: display`);
  api.t_free(OUT);
  assert.equal(api.t_live(), baseline - 1n);
  return checks;
}

// ---- length guards, which a program reaches only with petabytes ----------------------------
function guards(text, optimization) {
  const definition = (name) => {
    const match = text.match(new RegExp(`^define internal [^\\n]*@${name.replaceAll(".", "\\.")}\\([\\s\\S]*?^}`, "m"));
    assert.ok(match, `missing ${name}`);
    return match[0];
  };
  const names = ["tz.string.allocate", "tz.string.allocate.check", "tz.string.allocate_latin1", "tz.string.concat", "tz.string.concat.utf16",
    "tz.string.append", "tz.string.append.utf16", "tz.string.concat_into", "tz.string.concat_into.utf16", "tz.string.copy_as_utf16"];
  const ir = `%tz.string = type { ptr, i64 }
@recorded = internal global i64 0
declare void @llvm.trap()
define internal ptr @tz.alloc(i64 %bytes) {
  store i64 %bytes, ptr @recorded
  ret ptr null
}
define internal ptr @tz.realloc(ptr %old, i64 %before, i64 %bytes) {
  store i64 %bytes, ptr @recorded
  ret ptr null
}
define internal void @tz.string.copy(ptr %to, ptr %from, i64 %length) {
  ret void
}
define internal void @tz.string.copy_bytes(ptr %to, ptr %from, i64 %length) {
  ret void
}
define internal void @tz.string.widen(ptr %to, ptr %from, i64 %length) {
  ret void
}
define internal void @tz.string.widen_in_place(ptr %data, i64 %length) {
  ret void
}
${names.map(definition).join("\n")}
define i64 @g_allocate(i64 %n) {
  store i64 0, ptr @recorded
  %r = call %tz.string @tz.string.allocate_latin1(i64 %n)
  %w = extractvalue %tz.string %r, 1
  ret i64 %w
}
define i64 @g_concat(i64 %aw, i64 %bw) {
  store i64 0, ptr @recorded
  %a = insertvalue %tz.string zeroinitializer, i64 %aw, 1
  %b = insertvalue %tz.string zeroinitializer, i64 %bw, 1
  %r = call %tz.string @tz.string.concat(%tz.string %a, %tz.string %b)
  %w = extractvalue %tz.string %r, 1
  ret i64 %w
}
define i64 @g_append(i64 %aw, i64 %bw) {
  store i64 0, ptr @recorded
  %a = insertvalue %tz.string zeroinitializer, i64 %aw, 1
  %b = insertvalue %tz.string zeroinitializer, i64 %bw, 1
  %r = call %tz.string @tz.string.append(%tz.string %a, %tz.string %b)
  %w = extractvalue %tz.string %r, 1
  ret i64 %w
}
define i64 @g_concat_into(i64 %capacity, i64 %aw, i64 %bw) {
  store i64 0, ptr @recorded
  %a = insertvalue %tz.string zeroinitializer, i64 %aw, 1
  %b = insertvalue %tz.string zeroinitializer, i64 %bw, 1
  %r = call %tz.string @tz.string.concat_into(ptr null, i64 %capacity, %tz.string %a, %tz.string %b)
  %w = extractvalue %tz.string %r, 1
  ret i64 %w
}
define i64 @g_recorded() {
  %v = load i64, ptr @recorded
  ret i64 %v
}
`;
  const file = join(scratch, `guards-O${optimization}.ll`);
  writeFileSync(file, ir);
  const wasm = join(scratch, `guards-O${optimization}.wasm`);
  execute(clang, [
    "--target=wasm32-unknown-unknown", `-O${optimization}`, "-Wno-override-module", "-nostdlib", "-Wl,--no-entry", file,
    ...["g_allocate", "g_concat", "g_append", "g_concat_into", "g_recorded"].map((name) => `-Wl,--export=${name}`), "-o", wasm,
  ]);
  const module = new WebAssembly.Module(readFileSync(wasm));
  assert.deepEqual(WebAssembly.Module.imports(module), []);
  const fresh = () => new WebAssembly.Instance(module).exports;
  const guard = fresh();
  const latin1 = (length) => length | LATIN1;
  const accepted = [
    ["g_allocate", [MAX], MAX | LATIN1, MAX], ["g_allocate", [1n], 1n | LATIN1, 1n], ["g_allocate", [0n], 0n, 0n],
    ["g_concat", [latin1(MAX), 0n], MAX | LATIN1, MAX], ["g_concat", [0n, latin1(MAX)], MAX | LATIN1, MAX],
    ["g_concat", [latin1(MAX - 1n), latin1(1n)], MAX | LATIN1, MAX], ["g_concat", [latin1(MAX - 1n), 1n], MAX, 2n * MAX],
    ["g_concat", [MAX - 1n, latin1(1n)], MAX, 2n * MAX], ["g_concat", [MAX, 0n], MAX, 2n * MAX], ["g_concat", [0n, 0n], 0n, 0n],
    ["g_append", [latin1(MAX - 1n), latin1(1n)], MAX | LATIN1, MAX], ["g_append", [latin1(MAX - 1n), 1n], MAX, 2n * MAX],
    ["g_append", [MAX - 1n, latin1(1n)], MAX, 2n * MAX], ["g_append", [0n, latin1(MAX)], MAX | LATIN1, MAX],
    ["g_concat_into", [128n, latin1(MAX - 1n), latin1(1n)], MAX | LATIN1, MAX],
  ];
  for (const [name, args, word, recorded] of accepted) {
    checks += 1;
    assert.equal(guard[name](...args), word, `${name}(${args})`);
    assert.equal(guard.g_recorded(), recorded, `${name}(${args}) records the bytes that it asked for`);
  }
  const traps = [
    ["g_allocate", [MAX + 1n]], ["g_allocate", [1n << 63n]], ["g_allocate", [(1n << 64n) - 1n]],
    ["g_concat", [latin1(MAX), latin1(1n)]], ["g_concat", [latin1(MAX), 1n]], ["g_concat", [MAX, latin1(1n)]],
    ["g_concat", [latin1(MAX + 1n), 0n]], ["g_concat", [latin1(1n << 61n), latin1(1n << 61n)]],
    ["g_append", [latin1(MAX), latin1(1n)]], ["g_append", [latin1(MAX), 1n]], ["g_append", [MAX, latin1(1n)]],
    ["g_concat_into", [128n, latin1(MAX), latin1(1n)]], ["g_concat_into", [128n, latin1(MAX), 1n]],
  ];
  for (const [name, args] of traps) {
    checks += 1;
    assert.throws(() => fresh()[name](...args), WebAssembly.RuntimeError, `${name}(${args}) must trap`);
  }
  return { accepted: accepted.length, traps: traps.length };
}

// Invalid input traps, and a trap leaves the module that it happened in unusable but nothing else.
function trapping(module) {
  const isolated = () => {
    api = new WebAssembly.Instance(module).exports;
    OUT = api.t_alloc(64n);
  };
  for (const invalid of [[0xff], [0xc0, 0x80], [0xed, 0xa0, 0x80], [0xe2, 0x82]]) {
    isolated();
    const input = api.t_alloc(16n);
    bytes().set(invalid, input);
    assert.throws(() => api.t_from_utf8(input, BigInt(invalid.length), OUT), WebAssembly.RuntimeError, `from_utf8 of ${invalid} must trap`);
    checks += 1;
  }
  for (const units of [[0xd800], [0xdc00], [0x41, 0xd800, 0x42]]) {
    isolated();
    const text = make(units, "U");
    assert.throws(() => api.t_encode(text.ptr, text.word, OUT), WebAssembly.RuntimeError, `from_string of ${units} must trap`);
    checks += 1;
  }
}

try {
  for (const simd of [false, true]) {
    for (const optimization of [0, 3]) {
      const { module, text } = compile(optimization, simd);
      checks = 0;
      run(module, `${simd ? "simd128 " : ""}O${optimization}`);
      const limits = guards(text, optimization);
      trapping(module);
      console.log(`compact strings raw ${simd ? "wasm simd128" : "wasm scalar"} O${optimization}: ${checks} checks of the Latin-1 kernels and dispatchers against references, ${limits.accepted} length-guard cases accepted and ${limits.traps} trapped`);
    }
  }
} finally {
  rmSync(scratch, { recursive: true, force: true });
}
