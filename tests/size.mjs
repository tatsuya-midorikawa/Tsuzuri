// PM08: the size optimization levels, --strip, the integer formatter split, and the size of the artifacts.
//   node tests/size.mjs [path/to/tsuzuri]
// Sizes are deterministic, so the checks compare bytes exactly where a fixed relation is promised and
// say "not larger" where it is not; no size or speed threshold is a pass or fail condition of the suite.
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, statSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { artifactFingerprint } from "../benchmarks/metrics.mjs";

const root = resolve(fileURLToPath(new URL("..", import.meta.url)));
const compiler = resolve(process.argv[2] ?? join(root, "target/release/tsuzuri"));
const scratch = join(root, "target/size-tests");
mkdirSync(scratch, { recursive: true });
const directory = mkdtempSync(join(scratch, "run-"));
const suffix = process.platform === "win32" ? ".exe" : "";
const env = { ...process.env, TSUZURI_CACHE_DIR: join(directory, "cache"), TMPDIR: directory, TMP: directory, TEMP: directory };
const example = (name) => join(root, "examples", name);

function tsuzuri(args, { environment = env, status = 0 } = {}) {
  const result = spawnSync(compiler, args, { env: environment, encoding: "utf8", timeout: 600000, maxBuffer: 64 * 1024 * 1024 });
  assert.ifError(result.error);
  assert.equal(result.status, status, `tsuzuri ${args.join(" ")}\n${result.stdout}\n${result.stderr}`);
  return result;
}
function execute(file, args = []) {
  const result = spawnSync(file, args, { encoding: "utf8", timeout: 120000 });
  assert.ifError(result.error);
  return { status: result.status, stdout: result.stdout, stderr: result.stderr };
}
let counter = 0;
function build(project, args, extension = suffix, environment = env) {
  const output = join(directory, `a${counter++}${extension}`);
  tsuzuri(["build", project, ...args, "-o", output], { environment });
  return output;
}
const sections = [];
function section(name, body) {
  sections.push([name, body]);
}
const wasmInstance = (file) => {
  const module = new WebAssembly.Module(readFileSync(file));
  assert.deepEqual(WebAssembly.Module.imports(module), [], `${file} imports nothing`);
  return new WebAssembly.Instance(module).exports;
};

const LEVELS = ["-O3", "-Os", "-Oz"];
const PROJECTS = ["hello", "point", "tasks"];

section("-Os and -Oz executables and run print what -O3 prints", () => {
  for (const name of PROJECTS) {
    const expected = execute(build(example(name), ["-O3"]));
    assert.equal(expected.status, 0, expected.stderr);
    assert.ok(expected.stdout.length > 0);
    for (const level of ["-Os", "-Oz"]) {
      const actual = execute(build(example(name), [level]));
      assert.deepEqual(actual, expected, `${name} ${level} executable`);
    }
    // `run` instruments traps and links the runtime into the module: the other link path.
    const ran = tsuzuri(["run", example(name), "-Oz"]);
    assert.equal(ran.stdout, expected.stdout, `${name} run -Oz`);
  }
});

section("-O3, -Os, and -Oz emit the same LLVM IR (the level selects the optimizer, not the program)", () => {
  const emit = (level) => {
    const output = join(directory, `ir${counter++}.ll`);
    tsuzuri(["build", example("point"), level, "--emit", "llvm", "-o", output]);
    return readFileSync(output, "utf8");
  };
  const reference = emit("-O3");
  assert.equal(emit("-Os"), reference);
  assert.equal(emit("-Oz"), reference);
  assert.ok(!/\boptsize\b|\bminsize\b/.test(reference), "no optsize or minsize attributes are emitted");
});

section("size builds are deterministic and the cache keeps the levels apart", () => {
  const fingerprint = artifactFingerprint;
  // link.exe stamps the build time into a PE header, so only the other linkers promise equal bytes.
  if (process.platform !== "win32") {
    const cold = () => build(example("hello"), ["-Oz"], suffix, { ...env, TSUZURI_CACHE_DIR: join(directory, `cold${counter++}`) });
    assert.equal(fingerprint(cold()), fingerprint(cold()), "two cold -Oz builds are identical");
  }
  const first = {};
  for (const level of LEVELS) first[level] = build(example("hello"), [level]);
  // Had the cache key ignored the level, the -Oz build would have returned the -O3 artifact.
  assert.notEqual(fingerprint(first["-O3"]), fingerprint(first["-Oz"]), "-O3 and -Oz artifacts differ");
  for (const level of LEVELS) {
    assert.equal(fingerprint(build(example("hello"), [level])), fingerprint(first[level]), `a cache hit returns the ${level} artifact`);
  }
});

section("WASM modules at -Os and -Oz behave like -O3 and are smaller", () => {
  const project = join(directory, "wasm-project");
  mkdirSync(project, { recursive: true });
  writeFileSync(join(project, "Main.tz"), [
    "def rec sum :: i64 -> i64 -> i64 = \\n total ->",
    "    if n <= 0 {",
    "        total",
    "    } else {",
    "        sum (n - 1) (total + n * n)",
    "    }",
    "",
    "export def total :: i64 -> i64 = \\n -> sum n 0",
    "",
    "export def label :: i64 -> i64 = \\n -> (to_string n).length",
    "",
    // A float's text is the numeric runtime, the largest part of a module that shows numbers.
    "export def shown :: f64 -> i64 = \\x -> (to_string x).length",
    "",
  ].join("\n"));
  const modules = Object.fromEntries(LEVELS.map((level) => [level, build(project, ["--target", "wasm32", level], ".wasm")]));
  for (const level of LEVELS) {
    const api = wasmInstance(modules[level]);
    assert.equal(api.tz_total(1000n), 333833500n, `${level} total`);
    assert.equal(api.tz_label(-123456n), 7n, `${level} label`);
    assert.equal(api.tz_shown(-1.25), 5n, `${level} shown`);
  }
  const bytes = (level) => statSync(modules[level]).size;
  assert.ok(bytes("-Os") < bytes("-O3"), "-Os is smaller than -O3");
  assert.ok(bytes("-Oz") <= bytes("-Os"), "-Oz is not larger than -Os");
});

const nm = process.env.TSUZURI_NM ?? "nm";
// The local symbols of an object file (lowercase `nm` types; weak and unique symbols are not local), or null without `nm`.
function localSymbols(file) {
  const result = spawnSync(nm, ["-a", file], { encoding: "utf8", maxBuffer: 64 * 1024 * 1024 });
  if (result.error || result.status !== 0) return null;
  return result.stdout.split("\n").filter((line) => /^\s*[0-9a-fA-F]*\s+[a-tx-z]\s/.test(line)).length;
}
// The names in the symbol table of an executable, without the underscore Mach-O puts before them, or null without `nm`.
function symbolNames(file) {
  const result = spawnSync(nm, ["-a", file], { encoding: "utf8", maxBuffer: 64 * 1024 * 1024 });
  if (result.error || result.status !== 0) return null;
  return new Set(result.stdout.split("\n").map((line) => line.trim().split(/\s+/).at(-1)?.replace(/^_/, "")));
}

section("an executable with the runtime embedded is collected at link time like the default build", () => {
  // `point` shows floats, so it needs the numeric runtime's formatter but not its parser.
  const point = example("point");
  const plain = build(point, []);
  for (const extra of [["--no-cache"], ["--trap-info"]]) {
    const embedded = build(point, extra);
    assert.deepEqual(execute(embedded), execute(plain), `point ${extra.join(" ")} runs like the default build`);
    if (process.platform === "win32") continue; // no symbol table to read
    const symbols = symbolNames(embedded);
    if (symbols === null) continue;
    assert.ok(symbols.has("tz_soft_format"), "the formatter point needs is linked");
    assert.ok(!symbols.has("tz_soft_parse"), `the parser nothing calls is dropped from the ${extra.join(" ")} build`);
  }
});

section("--strip drops the local symbols and leaves the program as it is", () => {
  const project = example("hello");
  for (const extra of [[], ["--no-cache"], ["-Oz"]]) {
    const plain = build(project, extra);
    const stripped = build(project, [...extra, "--strip"]);
    assert.deepEqual(execute(stripped), execute(plain), `hello ${extra.join(" ")} --strip runs like the unstripped build`);
    if (process.platform === "win32") continue; // a Windows link is left as it is
    assert.ok(statSync(stripped).size <= statSync(plain).size, "the stripped executable is not larger");
    const before = localSymbols(plain), after = localSymbols(stripped);
    if (before === null) continue;
    assert.ok(after <= before, `local symbols: ${before} before, ${after} after`);
    if (process.platform === "darwin") {
      assert.equal(after, 0, "no local symbol is left in a Mach-O executable");
      if (extra.includes("--no-cache")) assert.ok(before > 0, "the unstripped embedded build has local symbols to drop");
      const signature = spawnSync("codesign", ["-v", stripped], { encoding: "utf8" });
      if (!signature.error) assert.equal(signature.status, 0, `the stripped executable keeps a valid signature: ${signature.stderr}`);
    }
  }
  // The trap table comes from the source map, not from symbols.
  const traps = [];
  for (const extra of [[], ["--strip"]]) {
    const output = join(directory, `trap${counter++}${suffix}`);
    tsuzuri(["build", example("point"), "--trap-info", ...extra, "-o", output]);
    traps.push(readFileSync(`${output}.trap.json`, "utf8"));
    assert.equal(execute(output).status, 0);
  }
  assert.equal(traps[0], traps[1], "--strip leaves the trap table unchanged");
});

// The integer types, their widths, and the values whose text is hard to get right: the extremes, the
// neighbors of powers of ten and of two, and, for the wide types, the parts of the 128-bit split.
const INTEGER_TYPES = [8, 16, 32, 64, 128].flatMap((bits) => [false, true].map((signed) => ({ bits, signed,
  name: `i${bits}${signed ? "" : "u"}`,
  low: signed ? -(1n << BigInt(bits - 1)) : 0n, high: (1n << BigInt(bits - (signed ? 1 : 0))) - 1n })));
function integerValues({ bits, signed, low, high }) {
  const values = new Set([low, low + 1n, high - 1n, high, 0n, 1n, 9n, 10n, 99n, 100n, 127n, 128n, 255n, 256n]);
  for (const exponent of [3n, 4n, 5n, 9n, 10n, 18n, 19n, 20n, 37n, 38n]) {
    for (const delta of [-1n, 0n, 1n]) values.add(10n ** exponent + delta);
  }
  for (const exponent of [7n, 8n, 15n, 16n, 31n, 32n, 63n, 64n, 65n, 127n]) values.add(2n ** exponent - 1n);
  for (const value of [...values]) if (signed) values.add(-value);
  return [...values].filter((value) => value >= low && value <= high).sort((left, right) => (left < right ? -1 : left > right ? 1 : 0));
}
const literal = (value, { name }) => `${value}${name}`;

section("integers print the same text in to_string and interpolation at every level", () => {
  const lines = [], expected = [];
  for (const [index, type] of INTEGER_TYPES.entries()) {
    const values = integerValues(type);
    lines.push(`    let values${index} = [${values.map((value) => literal(value, type)).join(", ")}]`,
      `    for value in values${index} do`, "        do! IO.write_line (to_string value)", '        do! IO.write_line $"{value}"');
    for (const value of values) expected.push(value.toString(), value.toString());
  }
  const project = join(directory, "integer-project");
  mkdirSync(project, { recursive: true });
  writeFileSync(join(project, "Main.tz"), ["def main :: unit -> i32 = \\() ->", ...lines, "    0", ""].join("\n"));
  const text = expected.join("\n") + "\n";
  for (const level of ["-O0", "-O3", "-Os", "-Oz"]) {
    const result = execute(build(project, [level]));
    assert.equal(result.status, 0, result.stderr);
    assert.equal(result.stdout.replaceAll("\r\n", "\n"), text, `native ${level}`);
  }
  // `run` links the formatter into the module with the trap table: the other path.
  assert.equal(tsuzuri(["run", project, "-O0"]).stdout.replaceAll("\r\n", "\n"), text);
});

section("the top-level result of an integer expression is written by the integer formatter", () => {
  for (const type of INTEGER_TYPES) {
    for (const value of [type.low, type.high, 0n]) {
      const project = join(directory, `result-${counter++}`);
      mkdirSync(project, { recursive: true });
      writeFileSync(join(project, "Main.tz"), `${literal(value, type)}\n`);
      const ir = join(directory, `result${counter++}.ll`);
      tsuzuri(["build", project, "--emit", "llvm", "-o", ir]);
      assert.ok(!readFileSync(ir, "utf8").includes("tz_soft_"), `${type.name}: no numeric runtime`);
      for (const level of ["-O0", "-Oz"]) {
        assert.equal(execute(build(project, [level])).stdout.trim(), value.toString(), `${type.name} ${value} ${level}`);
      }
    }
  }
});

section("an integer-only WASM module carries no numeric tables and its integer text has the right length", () => {
  const exports = INTEGER_TYPES.filter(({ bits }) => bits <= 64).map(({ name }) =>
    `export def length_${name} :: ${name} -> i64 = \\x -> (to_string x).length`);
  const project = join(directory, "wasm-integers");
  mkdirSync(project, { recursive: true });
  writeFileSync(join(project, "Main.tz"), exports.join("\n") + "\n");
  const withFloat = join(directory, "wasm-float");
  mkdirSync(withFloat, { recursive: true });
  writeFileSync(join(withFloat, "Main.tz"), exports.join("\n") + "\nexport def length_f64 :: f64 -> i64 = \\x -> (to_string x).length\n");
  const size = (file) => statSync(file).size;
  for (const level of ["-O0", "-O3", "-Oz"]) {
    const integers = build(project, ["--target", "wasm32", level], ".wasm");
    const floats = build(withFloat, ["--target", "wasm32", level], ".wasm");
    assert.ok(size(integers) * 2 < size(floats), `${level}: ${size(integers)} bytes without the numeric runtime, ${size(floats)} with it`);
    const api = wasmInstance(integers);
    for (const type of INTEGER_TYPES.filter(({ bits }) => bits <= 64)) {
      for (const value of integerValues(type)) {
        // WebAssembly wraps the argument to the width of the parameter: a Number to 32 bits, a BigInt to 64.
        const argument = type.bits === 64 ? value : Number(value);
        assert.equal(api[`tz_length_${type.name}`](argument), BigInt(value.toString().length), `${level} ${type.name} ${value}`);
      }
    }
  }
});

section("the option errors of --strip", () => {
  const fails = (args, message) => {
    const result = tsuzuri(args, { status: 2 });
    assert.ok(result.stderr.includes(`error[E2000]: ${message}`), `${args.join(" ")}\n${result.stderr}`);
  };
  const hello = example("hello");
  fails(["build", hello, "--strip", "--strip"], "strip specified more than once");
  fails(["run", hello, "--strip"], "--strip is only valid with build");
  fails(["test", hello, "--strip"], "--strip is only valid with build");
  fails(["build", hello, "--strip", "-g"], "--strip cannot be combined with --debug-info: a debugger needs the symbols");
  fails(["build", hello, "--strip", "--emit", "object"], "--strip requires native executable output (--emit exe)");
  fails(["build", hello, "--strip", "--target", "wasm32"],
    "--strip requires a native executable; WASM output is already stripped unless --debug-info is used");
  assert.match(tsuzuri(["--help"]).stdout, /--strip\s+Link a native executable without its local symbols/);
});

section("the option errors of -Os and -Oz", () => {
  const fails = (args, message) => {
    const result = tsuzuri(args, { status: 2 });
    assert.ok(result.stderr.includes(`error[E2000]: ${message}`), `${args.join(" ")}\n${result.stderr}`);
  };
  for (const level of ["-Os", "-Oz"]) {
    fails(["test", example("hello"), level], "test supports -O0 to -O3; use -Os or -Oz with build or run");
    fails(["bench", example("hello"), level], "bench supports -O0 to -O3; use -Os or -Oz with build or run");
    fails(["check", example("hello"), level], "check does not use an optimization level");
    fails(["build", example("hello"), "-O3", level], "optimization specified more than once");
    fails(["repl", level], "repl takes no paths; supported options are -O0 to -O3");
    fails(["script", level, example("hello")], "script accepts -O0 to -O3");
  }
  fails(["build", example("hello"), "-Ot"], "unknown option '-Ot'; use --help");
  const help = tsuzuri(["--help"]).stdout;
  assert.match(help, /-Os, -Oz\s+Optimize for size/);
});

let failed = 0;
try {
  for (const [name, body] of sections) {
    try {
      body();
      console.log(`Size: ${name} passed`);
    } catch (error) {
      failed += 1;
      console.error(`Size: ${name} FAILED`);
      console.error(error);
    }
  }
} finally {
  rmSync(directory, { recursive: true, force: true });
}
if (failed) process.exit(1);
