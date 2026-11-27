// M02: typed message sends, end to end on macOS.
//
// 1. The fixture is linked with an ARC host (tests/objc_send_host.m) and every case's output is
//    compared with values computed here, in JavaScript, from the host's own arithmetic and from
//    its count of live objects.
// 2. The type of every `objc_msgSend` call that the compiler emits for `reference_calls` is
//    compared with the call that clang emits for the same message (tests/objc_send_reference.m):
//    the stop condition S4 of the ticket. Clang is the reference; the compiler's output is never
//    copied into an expectation.
//
// Usage: node tests/objc_send.mjs [path/to/tsuzuri]
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { existsSync, mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const compiler = resolve(process.argv[2] ?? join(root, "target", "debug", "tsuzuri"));
const clang = process.env.TSUZURI_CLANG ?? "clang";
// Not under tests/fixtures/: the repository-wide tests build every fixture for their host, and
// these programs reach Objective-C, which only an Apple target has.
const fixture = join(root, "tests", "objc", "send");
const foundation = join(root, "tests", "objc", "foundation");
const host = join(root, "tests", "objc_send_host.m");
const reference = join(root, "tests", "objc_send_reference.m");
const bundledClang =
  process.env.TSUZURI_BUNDLED_CLANG ?? join(root, "target", "toolchain", "bin", "tsuzuri-clang");

if (process.platform !== "darwin" || process.arch !== "arm64") {
  console.log("objc_send: skipped (typed message sends need an arm64 Mac)");
  process.exit(0);
}

function run(command, args, options = {}) {
  const result = spawnSync(command, args, { encoding: "utf8", timeout: 180_000, ...options });
  if (result.error) throw result.error;
  return result;
}

function must(command, args, options = {}) {
  const result = run(command, args, options);
  assert.equal(
    result.status,
    0,
    `${command} ${args.join(" ")}\nstdout:\n${result.stdout}\nstderr:\n${result.stderr}`,
  );
  return result;
}

const sdk = run("xcrun", ["--show-sdk-path"]);
const sysroot = sdk.status === 0 ? ["-isysroot", sdk.stdout.trim()] : [];

// The host's arithmetic, in JavaScript.
const cents = (value) => Math.trunc(value * 100);
const wrap = (value, bits, signed) => {
  const mask = (1n << BigInt(bits)) - 1n;
  const low = BigInt(value) & mask;
  return Number(signed && low >> BigInt(bits - 1) ? low - (1n << BigInt(bits)) : low);
};
const weighted = (values) => values.reduce((total, value, index) => total + (index + 1) * value, 0);

const expected = {
  // made (+0, retained) and created (+1): 2 live; a copy (+1) and a twin (+0): 4. The values of
  // made and twin are 1 + 2; the copy holds 2. Everything is released when the function ends,
  // before the pool is popped.
  families: [2, 4, 1 + 2, 2, 0, 0].join(" "),
  // init keeps the value 7 with one object live; a refused init returns nil and leaves nothing;
  // the plain init has the value 0 with one object live. Nothing is left.
  inits: [7 + 100 * 1, 0, 0 + 100 * 1, 0].join(" "),
  scalars: [
    5 > 0, // isPositive
    !true, // flip:
    !false,
    wrap(-128 - 1, 8, true), // narrow:
    wrap(255 + 1, 8, false), // ubyte:
    wrap(-32768 - 1, 16, true), // shorter:
    wrap(65535 + 1, 16, false), // ushorter:
    -2147483647 - 1, // int32:
    wrap(4294967295 + 1, 32, false), // uint32:
    cents(2.5 / 2), // half:
    cents(1.5 * 2), // twice:
  ].join(" "),
  many: [
    -1 + -2 + 200 + 60000 + 1000000 + Math.trunc(1.5 * 2), // takeChar:short:uchar:ushort:flag:f:
    weighted([1, 2, 3, 4, 5, 6, 7, 8, 9, 10]), // sum:b:c:d:e:f:g:h:i:j:
    cents(1 + 2.5 + 3 + 0.25 + 0.5), // mixInt:d:i:f:flag:
  ].join(" "),
  buffers: [1 + 2 + 3 + 250, 5 * 3, !false, !!false].join(" "),
  objects: [true, false, 9, -1, 4 * 3 + 5, 11, 11, 0].join(" "),
  // The class objects by name; a child answers its own `tag` also through a base's member; the
  // class methods and the category.
  classes: [1, 1, 0, true, false, 2, 2, 1, 6, 42 + 42, 5 * 2 + 7].join(" "),
  struct_results: [
    cents(5 + 2 * 5.5 + 1 + 2 * 2 + 3 * 5 + 4 * 4), // point and frame
    5, // range
    7,
    cents(weighted([1, 2, 3, 4, 5, 5])), // big
    5, // pair
    -5,
    cents(0.25 + 2 * 0.5 + 4 * 5), // color
  ].join(" "),
  struct_arguments: [
    cents(3 * 4.5 + 1.5 + 10 * 2.5), // areaOf:
    cents(1 + 0.25 + 10 * (2 - 0.25)), // offset:by:
    3 + 5, // grow:by:
    4 * 5,
    cents((1 + 2 + 3 + 4 + 5 + 6) * 1.5), // scale:by:
    7 * 100 + -3, // sumPair:
    cents(0.5 + 2 * 0.25 + 4 * 2), // luma:
    cents(weighted([1, 2, 3, 4, 5, 6, 7, 8, 9, 10])), // mix:with:at:
    1 + 2 + 3 + 4 + 5 + 6 * 100 + 7 * 10000, // spill:b:c:d:e:range:
  ].join(" "),
  routine: "5",
  // extra (7) + fill (5 * 3); no object is left.
  reference: [7 + 15, 0].join(" "),
};

// One `objc_msgSend` call as its type: the result and the parameters, without value names,
// constants and the attributes that say nothing about the calling convention. A named struct
// is replaced by its fields, so that clang's `%struct.TzRect` reads like the literal struct.
function callTypes(ir, functionName) {
  const structs = new Map();
  for (const [, name, body] of ir.matchAll(/^(%[\w.]+) = type \{(.*)\}$/gm)) {
    structs.set(name, body);
  }
  const expand = (text) => {
    let current = text;
    for (let round = 0; round < 8; round += 1) {
      const next = current.replace(/%[\w.]+/g, (name) =>
        structs.has(name) ? `{${structs.get(name)}}` : name,
      );
      if (next === current) break;
      current = next;
    }
    // Nested literal structs flatten to their leaves, as the calling convention sees them: one
    // type is one struct at most, so its braces become one pair.
    const flat = current.includes("{") ? `{${current.replace(/[{}]/g, "")}}` : current;
    return flat.replace(/\s+/g, "");
  };
  const start = ir.indexOf(`@${functionName}(`);
  assert.notEqual(start, -1, `${functionName} is in the IR`);
  const body = ir.slice(start, ir.indexOf("\n}\n", start));
  const calls = [];
  for (const line of body.split("\n")) {
    const match = /\bcall (.*?) @objc_msgSend\((.*)\)(?: \[ .* \])?$/.exec(line.trimEnd());
    if (!match) continue;
    const strip = (text) =>
      text
        .replace(/\b(noundef|nonnull|dead_on_unwind|dead_on_return|writable|captures\(none\))\b/g, "")
        .replace(/\balign \d+\b/g, "")
        .replace(/sret\([^()]*(?:\([^()]*\))?[^()]*\)/, "sret")
        .trim();
    const parameters = [];
    let depth = 0;
    let current = "";
    for (const character of match[2]) {
      if ("([{".includes(character)) depth += 1;
      if (")]}".includes(character)) depth -= 1;
      if (character === "," && depth === 0) {
        parameters.push(current);
        current = "";
      } else {
        current += character;
      }
    }
    if (current.trim() !== "") parameters.push(current);
    // The value is the last word of a parameter.
    const types = parameters.map((parameter) =>
      expand(strip(parameter.trim().replace(/\s+\S+$/, ""))),
    );
    calls.push(`${expand(strip(match[1]))}(${types.join(",")})`);
  }
  return calls;
}

const scratch = mkdtempSync(join(tmpdir(), "tsuzuri-m02-"));
const env = { ...process.env, TSUZURI_CACHE_DIR: "" };
let ran = 0;
try {
  // 2. The call types against clang, first: a wrong type would make the runs below meaningless.
  const referenceIr = join(scratch, "reference.ll");
  must(clang, [
    "-target",
    "arm64-apple-macosx12.0",
    "-fobjc-arc",
    "-fno-objc-msgsend-selector-stubs",
    "-fno-objc-convert-messages-to-runtime-calls",
    "-O0",
    ...sysroot,
    "-S",
    "-emit-llvm",
    reference,
    "-o",
    referenceIr,
  ]);
  const fixtureIr = join(scratch, "fixture.ll");
  must(
    compiler,
    ["build", fixture, "--emit", "llvm", "--target", "aarch64-apple-darwin", "-o", fixtureIr],
    { env },
  );
  const ir = readFileSync(fixtureIr, "utf8");
  const want = callTypes(readFileSync(referenceIr, "utf8"), "reference_calls");
  const got = callTypes(ir, "tz.fn.Main.reference_calls");
  assert.ok(want.length >= 40, `clang's reference has its sends (${want.length})`);
  assert.deepEqual(got, want, "the type of every objc_msgSend call is clang's");
  ran += want.length;
  assert.equal(
    ir.split("\n").filter((line) => /^declare .*@objc_msgSend\(/.test(line)).join("\n"),
    "declare ptr @objc_msgSend(ptr, ptr, ...)",
    "objc_msgSend has one variadic declaration",
  );

  // 1. The runs.
  const hostObject = join(scratch, "host.o");
  must(clang, [
    "-fobjc-arc",
    "-O2",
    "-mmacosx-version-min=12.0",
    ...sysroot,
    "-c",
    host,
    "-o",
    hostObject,
  ]);
  const clangs = [["system", undefined]];
  if (existsSync(bundledClang)) {
    clangs.push(["bundled", bundledClang]);
  } else {
    console.log(`objc_send: the bundled toolchain is skipped (${bundledClang} is missing)`);
  }
  for (const [label, path] of clangs) {
    const buildEnv = path ? { ...env, TSUZURI_CLANG: path } : env;
    for (const optimization of ["-O0", "-O3"]) {
      const executable = join(scratch, `send-${label}${optimization}`);
      must(
        compiler,
        ["build", fixture, optimization, "--trap-info", "--link", hostObject, "-o", executable],
        { env: buildEnv },
      );
      for (const [name, line] of Object.entries(expected)) {
        const result = must(executable, [name], { env });
        assert.equal(result.stdout, `${line}\n`, `${name} at ${optimization} (${label})`);
        assert.equal(result.stderr, "", `${name} at ${optimization} (${label})`);
        ran += 1;
      }
    }
  }
  // The counting allocator: the program's own heap is balanced too.
  const counted = join(scratch, "counted");
  must(
    compiler,
    ["build", fixture, "-O3", "--allocator", "counting", "--link", hostObject, "-o", counted],
    { env },
  );
  for (const name of ["families", "inits", "struct_arguments"]) {
    assert.equal(must(counted, [name], { env }).stdout, `${expected[name]}\n`, `${name} counted`);
    ran += 1;
  }

  // 3. Foundation and AppKit: the example of the ticket, a constant NSString, and a window that
  //    is never shown (no activation, deferred, no run loop), within a time limit.
  for (const optimization of ["-O0", "-O3"]) {
    const executable = join(scratch, `foundation${optimization}`);
    must(
      compiler,
      ["build", foundation, optimization, "--trap-info", "--framework", "AppKit", "-o", executable],
      { env },
    );
    const result = must(executable, [], { env, timeout: 60_000 });
    // 2.5 as double and float, its BOOL, the UTF-16 length of the text, the comparison with the
    // constant string, the lengths of an ASCII and of an empty constant, and the window: its
    // title's length, its content rectangle (width 320, height 200), and that it is not visible.
    assert.equal(
      result.stdout,
      `2.5 2.5 true 5 true\n${"Tsuzuri".length} 0 ${"窓 window".length} 320 200 false\n`,
      `foundation at ${optimization}`,
    );
    assert.equal(result.stderr, "");
    ran += 1;
  }
} finally {
  rmSync(scratch, { recursive: true, force: true });
}
console.log(`objc_send: ok (${ran} checks)`);
