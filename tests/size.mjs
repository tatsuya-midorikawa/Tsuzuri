// PM08: the size optimization levels, --strip, the integer runtime split (text, format specs, Parse), and the size of the artifacts.
//   node tests/size.mjs [path/to/tsuzuri]
// Sizes are deterministic, so the checks compare bytes exactly where a fixed relation is promised and
// say "not larger" where it is not; no size or speed threshold is a pass or fail condition of the suite.
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { chmodSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, statSync, writeFileSync } from "node:fs";
import { delimiter, join, resolve } from "node:path";
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

section("-O3, -Os, and -Oz emit the same LLVM IR but for the inline attribute of closure functions (the level selects the optimizer, not the program)", () => {
  const emit = (level) => {
    const output = join(directory, `ir${counter++}.ll`);
    tsuzuri(["build", example("point"), level, "--emit", "llvm", "-o", output]);
    return readFileSync(output, "utf8");
  };
  const reference = emit("-O3");
  // PM02: a native build forces known closures inline at -O1 to -O3 only (the next section).
  const plain = reference.replace(/^(define internal [^\n]*@tz\.(?:apply\.|fn\.\$lambda\.)[^\n]*?) alwaysinline/gm, "$1");
  assert.equal(emit("-Os"), plain);
  assert.equal(emit("-Oz"), plain);
  assert.ok(!/\boptsize\b|\bminsize\b/.test(reference), "no optsize or minsize attributes are emitted");
});

section("the level changes only the inline attribute of the closure functions of a native build (PM02)", () => {
  const emit = (level, extra = []) => {
    const output = join(directory, `ir${counter++}.ll`);
    tsuzuri(["build", example("hello"), level, ...extra, "--emit", "llvm", "-o", output]);
    return readFileSync(output, "utf8");
  };
  const closureDefine = /^define internal [^\n]*@tz\.(?:apply\.|fn\.\$lambda\.)[^\n]*$/gm;
  const forced = (ir) => (ir.match(closureDefine) ?? []).filter((line) => line.includes(" alwaysinline"));
  const speed = emit("-O3");
  assert.ok(forced(speed).length > 0, "-O3 forces the adapters of closures inline");
  for (const level of ["-O1", "-O2"]) assert.equal(emit(level), speed, `${level} emits what -O3 emits`);
  // -O0 and the size levels leave the inlining of closures to the compiler.
  const plain = speed.replace(closureDefine, (line) => line.replace(" alwaysinline", ""));
  for (const level of ["-O0", "-Os", "-Oz"]) {
    const ir = emit(level);
    assert.equal(forced(ir).length, 0, `${level} leaves the inlining of closures to the compiler`);
    assert.equal(ir, plain, `${level}: nothing else differs from -O3`);
  }
  // WebAssembly never has the attribute, so its IR is the same at every level.
  const wasm = ["--target", "wasm32"];
  const reference = emit("-O3", wasm);
  assert.equal(forced(reference).length, 0, "WebAssembly never forces closures inline");
  for (const level of ["-O0", "-Os", "-Oz"]) assert.equal(emit(level, wasm), reference, `wasm32 ${level}`);
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

// A small library with integer and float text: its WASM module holds both formatters.
function wasmProject() {
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
  return project;
}
const wasmResults = (file, level) => {
  const api = wasmInstance(file);
  assert.equal(api.tz_total(1000n), 333833500n, `${level} total`);
  assert.equal(api.tz_label(-123456n), 7n, `${level} label`);
  assert.equal(api.tz_shown(-1.25), 5n, `${level} shown`);
};

section("WASM modules at -Os and -Oz behave like -O3 and are smaller", () => {
  const project = wasmProject();
  const modules = Object.fromEntries(LEVELS.map((level) => [level, build(project, ["--target", "wasm32", level], ".wasm")]));
  for (const level of LEVELS) wasmResults(modules[level], level);
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
      // Only a linker that signs its output (Apple silicon) leaves a signature for --strip to keep valid.
      const original = spawnSync("codesign", ["-v", plain], { encoding: "utf8" });
      if (!original.error && original.status === 0) {
        const signature = spawnSync("codesign", ["-v", stripped], { encoding: "utf8" });
        assert.equal(signature.status, 0, `the stripped executable keeps a valid signature: ${signature.stderr}`);
      }
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

// What Parse.parse makes of a text, from the grammar of the language reference: an optional sign, then decimal
// digits, "0x" and hexadecimal digits, or "0b" and binary digits, with a single "_" allowed between two digits,
// within the type's range and 4096 units. Written here again with BigInt for the programs the compiler builds.
const INTEGER_TEXT = /^[+-]?(?:[0-9](?:_?[0-9])*|0x[0-9a-fA-F](?:_?[0-9a-fA-F])*|0b[01](?:_?[01])*)$/;
function parseReference(text, { signed, low, high }) {
  if (text.length === 0 || text.length > 4096 || !INTEGER_TEXT.test(text)) return null;
  const negative = text.startsWith("-");
  if (negative && !signed) return null;
  const magnitude = BigInt(text.replace(/^[+-]/, "").replaceAll("_", ""));
  const value = negative ? -magnitude : magnitude;
  return value >= low && value <= high ? value : null;
}
// What a format hole shows: the sign ("+" asked for or "-"), then the digits of the magnitude in the base.
const specReference = (value, base, plus = false, upper = false) => {
  const digits = (value < 0n ? -value : value).toString(base);
  return (value < 0n ? "-" : plus ? "+" : "") + (upper ? digits.toUpperCase() : digits);
};
const INVALID_TEXTS = ["", "+", "-", "_1", "1_", "1__0", "+-1", "-+1", "++1", "0x", "0b", "-0x", "0x_1", "0xg", "0b2", "0X1",
  "0B1", " 1", "1 ", "00x1", "0x0x1", "1e3", "1.5", "０", "１２", "٣", "Ā", "\u0130", "\u0178"];

section("integer Parse and format specs print the same at every level and match the reference", () => {
  const lines = [], expected = [];
  for (const [index, type] of INTEGER_TYPES.entries()) {
    const values = integerValues(type);
    const texts = [...INVALID_TEXTS, "0".repeat(4095) + "1", "0".repeat(4097), "0x" + "0".repeat(4094), "0x" + "0".repeat(4095)];
    for (const value of [...values, type.low - 1n, type.high + 1n, type.high * 2n + 1n]) {
      const magnitude = value < 0n ? -value : value;
      const sign = value < 0n ? "-" : "";
      const digits = magnitude.toString();
      texts.push(value.toString(), sign + "0x" + magnitude.toString(16), sign + "0X" + magnitude.toString(16).toUpperCase(),
        sign + "0b" + magnitude.toString(2), (value < 0n ? "-" : "+") + "00" + digits,
        sign + "0x" + magnitude.toString(16).toUpperCase().split("").join("_"), digits.split("").join("_"));
    }
    lines.push(`    for text in [${texts.map((text) => JSON.stringify(text)).join(", ")}] do`,
      `        let parsed${index}: Maybe<${type.name}> = Parse.parse (&text)`,
      `        do! IO.write_line (if Maybe.is_none (&parsed${index}) then "none" else to_string (Maybe.get parsed${index}))`);
    for (const text of texts) expected.push(String(parseReference(text, type) ?? "none"));
    lines.push(`    let values${index} = [${values.map((value) => literal(value, type)).join(", ")}]`,
      `    for value in values${index} do`,
      '        do! IO.write_line $"{value:+}|{value:x}|{value:X}|{value:o}|{value:b}|{value:5}|{value:+6}"');
    for (const value of values) {
      expected.push([specReference(value, 10, true), specReference(value, 16), specReference(value, 16, false, true),
        specReference(value, 8), specReference(value, 2), value.toString().padStart(5),
        specReference(value, 10, true).padStart(6)].join("|"));
    }
  }
  const project = join(directory, "integer-parse-project");
  mkdirSync(project, { recursive: true });
  writeFileSync(join(project, "Main.tz"), ["def main :: unit -> i32 = \\() ->", ...lines, "    0", ""].join("\n"));
  const text = expected.join("\n") + "\n";
  for (const level of ["-O0", "-O3", "-Os", "-Oz"]) {
    const result = execute(build(project, [level]));
    assert.equal(result.status, 0, result.stderr);
    const lines = result.stdout.replaceAll("\r\n", "\n").split("\n");
    const reference = text.split("\n");
    assert.equal(lines.length, reference.length, `native ${level}: lines`);
    for (const [number, line] of reference.entries()) assert.equal(lines[number], line, `native ${level}: line ${number + 1}`);
  }
  // `run` links the runtime into the module with the trap table: the other path.
  assert.equal(tsuzuri(["run", project, "-O0"]).stdout.replaceAll("\r\n", "\n"), text);
  // None of it needs the numeric runtime, nor the ASCII copy and 4 KiB buffer that parser used to take.
  const ir = join(directory, `parse${counter++}.ll`);
  tsuzuri(["build", project, "--emit", "llvm", "-o", ir]);
  const emitted = readFileSync(ir, "utf8");
  assert.ok(!emitted.includes("tz_soft_"), "no numeric runtime");
  assert.ok(!emitted.includes("call i1 @tz.string.to_ascii("), "no ASCII copy of the text");
});

section("a float still uses the numeric runtime to parse, with the ASCII copy", () => {
  const project = join(directory, "float-parse");
  mkdirSync(project, { recursive: true });
  writeFileSync(join(project, "Main.tz"), [
    "def main :: unit -> i32 = \\() ->",
    '    let parsed: Maybe<f64> = Parse.parse (&"2.5")',
    "    do! IO.write_line (to_string (Maybe.get parsed))",
    '    let spec = $"{Maybe.get parsed:.3}"',
    "    do! IO.write_line spec",
    "    0",
    "",
  ].join("\n"));
  const ir = join(directory, `float-parse${counter++}.ll`);
  tsuzuri(["build", project, "--emit", "llvm", "-o", ir]);
  const emitted = readFileSync(ir, "utf8");
  assert.ok(emitted.includes("call i32 @tz_soft_parse(") && emitted.includes("call i32 @tz_soft_format_spec("));
  assert.ok(emitted.includes("call i1 @tz.string.to_ascii("));
  assert.equal(execute(build(project, ["-Oz"])).stdout.replaceAll("\r\n", "\n"), "2.5\n2.500\n");
});

section("an integer-only WASM module parses and formats specs without the numeric runtime", () => {
  const types = INTEGER_TYPES.filter(({ bits }) => bits <= 64);
  // Each export parses the text of its own argument in decimal, hexadecimal, and binary and returns the sum of the
  // three parses, so a wrong digit, base, sign, or range anywhere changes the answer.
  const body = types.flatMap(({ name }) => [
    `export def round_${name} :: ${name} -> i64 = \\x ->`,
    "    let decimal = to_string x",
    '    let hexadecimal = $"0x{x:x}"',
    '    let binary = $"0b{x:b}"',
    `    let first: Maybe<${name}> = Parse.parse (&decimal)`,
    `    let second: Maybe<${name}> = Parse.parse (&hexadecimal)`,
    `    let third: Maybe<${name}> = Parse.parse (&binary)`,
    "    if Maybe.is_none (&first) || Maybe.is_none (&second) || Maybe.is_none (&third) then 0 else",
    "        (Maybe.get first) as i64 + (Maybe.get second) as i64 + (Maybe.get third) as i64",
  ]);
  const project = join(directory, "wasm-parse");
  mkdirSync(project, { recursive: true });
  writeFileSync(join(project, "Main.tz"), body.join("\n") + "\n");
  const withFloat = join(directory, "wasm-parse-float");
  mkdirSync(withFloat, { recursive: true });
  writeFileSync(join(withFloat, "Main.tz"), body.join("\n") +
    '\nexport def parse_f64 :: i64 -> i64 = \\n ->\n    let parsed: Maybe<f64> = Parse.parse (&(to_string n))\n    to_int (Maybe.get parsed)\n');
  const size = (file) => statSync(file).size;
  for (const level of ["-O0", "-O3", "-Oz"]) {
    const integers = build(project, ["--target", "wasm32", level], ".wasm");
    const floats = build(withFloat, ["--target", "wasm32", level], ".wasm");
    assert.ok(size(integers) + 16384 < size(floats), `${level}: ${size(integers)} bytes without the numeric runtime, ${size(floats)} with it`);
    const api = wasmInstance(integers);
    for (const type of types) {
      for (const value of integerValues(type)) {
        // A negative value has no "0x" text that parses back to it, so its binary and hexadecimal parses fail.
        if (value < 0n) continue;
        const argument = type.bits === 64 ? value : Number(value);
        assert.equal(BigInt.asIntN(64, api[`tz_round_${type.name}`](argument)), BigInt.asIntN(64, value * 3n), `${level} ${type.name} ${value}`);
      }
    }
  }
});

// A stand-in for Binaryen's wasm-opt: it logs its arguments, answers --version with a version taken from the
// environment, and writes its input back with one extra custom section, so a test sees what ran and what came out.
function fakeWasmOpt(name) {
  const file = join(directory, name);
  writeFileSync(file, `#!${process.execPath}
import { appendFileSync, readFileSync, writeFileSync } from "node:fs";
const args = process.argv.slice(2);
appendFileSync(process.env.FAKE_WASM_OPT_LOG, JSON.stringify(args) + "\\n");
if (args[0] === "--version") { console.log("wasm-opt version fake-" + (process.env.FAKE_WASM_OPT_VERSION ?? "1")); process.exit(0); }
if (process.env.FAKE_WASM_OPT_FAIL) { console.error("fake wasm-opt failed"); process.exit(3); }
const output = args[args.indexOf("-o") + 1];
const name = Buffer.from("fake-opt");
writeFileSync(output, Buffer.concat([readFileSync(args[0]), Buffer.from([0, name.length + 1, name.length]), name]));
`);
  chmodSync(file, 0o755);
  return file;
}
const customSections = (file, name) => WebAssembly.Module.customSections(new WebAssembly.Module(readFileSync(file)), name).length;

section("--wasm-opt runs the named wasm-opt on request only, and the cache knows the tool", () => {
  if (process.platform === "win32") return; // the stand-in is a script
  const project = wasmProject();
  const log = join(directory, "wasm-opt.log");
  const fake = fakeWasmOpt("fake-wasm-opt.mjs");
  const environment = (extra = {}) => ({ ...env, TSUZURI_WASM_OPT: fake, FAKE_WASM_OPT_LOG: log, ...extra });
  const calls = () => (existsSync(log) ? readFileSync(log, "utf8").trim().split("\n").filter(Boolean).map((line) => JSON.parse(line)) : []);
  const runs = () => calls().filter((args) => args[0] !== "--version");
  const wasm = ["--target", "wasm32", "-Oz"];
  // Without the flag the tool is never looked at, whatever the environment says.
  const plain = build(project, wasm, ".wasm", environment());
  assert.deepEqual(calls(), []);
  assert.equal(customSections(plain, "fake-opt"), 0);
  // With it, the linked module goes through -Oz with the features the module uses, and the result is the artifact.
  const optimized = build(project, [...wasm, "--wasm-opt"], ".wasm", environment());
  assert.equal(runs().length, 1);
  const [input, flag, produced, level, ...features] = runs()[0];
  assert.ok(input.length > 0 && flag === "-o" && produced.endsWith("optimized.wasm") && level === "-Oz", JSON.stringify(runs()[0]));
  assert.deepEqual(features, ["--enable-bulk-memory", "--enable-sign-ext", "--enable-nontrapping-float-to-int",
    "--enable-mutable-globals", "--enable-multivalue", "--enable-reference-types"]);
  assert.equal(customSections(optimized, "fake-opt"), 1);
  wasmResults(optimized, "-Oz --wasm-opt");
  // SIMD adds its feature.
  const before = runs().length;
  build(project, [...wasm, "--wasm-feature", "simd128", "--wasm-opt"], ".wasm", environment());
  assert.equal(runs().length, before + 1);
  assert.ok(runs().at(-1).includes("--enable-simd"));
  // The whole-build cache keys on the tool: the same build is a hit, another version of the tool is a miss.
  // (Where the linker's identity cannot be read, the cache is off and both builds run the tool.)
  const count = runs().length;
  build(project, [...wasm, "--wasm-opt"], ".wasm", environment());
  if (runs().length === count) {
    build(project, [...wasm, "--wasm-opt"], ".wasm", environment({ FAKE_WASM_OPT_VERSION: "2" }));
    assert.equal(runs().length, count + 1, "another version of wasm-opt is another cache entry");
  }
});

section("--wasm-opt reports a missing or failing wasm-opt as E2002 with a hint, and never looks for one on its own", () => {
  const project = wasmProject();
  const hint = "install Binaryen or set TSUZURI_WASM_OPT to the wasm-opt executable";
  const missing = join(directory, "no-such-wasm-opt");
  const output = join(directory, `missing${counter++}.wasm`);
  const result = tsuzuri(["build", project, "--target", "wasm32", "--wasm-opt", "-o", output],
    { environment: { ...env, TSUZURI_WASM_OPT: missing }, status: 1 });
  assert.ok(result.stderr.includes(`error[E2002]: cannot execute '${missing}'`), result.stderr);
  assert.ok(result.stderr.includes(hint), result.stderr);
  assert.ok(!existsSync(output), "no module is published when wasm-opt cannot run");
  if (process.platform === "win32") return; // the rest needs a script
  const fake = fakeWasmOpt("failing-wasm-opt.mjs");
  const log = join(directory, "failing.log");
  const failing = join(directory, `failing${counter++}.wasm`);
  const failure = tsuzuri(["build", project, "--target", "wasm32", "--wasm-opt", "-o", failing],
    { environment: { ...env, TSUZURI_WASM_OPT: fake, FAKE_WASM_OPT_LOG: log, FAKE_WASM_OPT_FAIL: "1" }, status: 1 });
  assert.ok(failure.stderr.includes(`error[E2002]: '${fake}' failed`), failure.stderr);
  assert.ok(failure.stderr.includes("fake wasm-opt failed") && failure.stderr.includes(hint), failure.stderr);
  assert.ok(!existsSync(failing), "no module is published when wasm-opt fails");
  // A wasm-opt on PATH is used when asked for and is not found when not.
  const directoryOnPath = join(directory, "on-path");
  mkdirSync(directoryOnPath, { recursive: true });
  writeFileSync(join(directoryOnPath, "wasm-opt"), readFileSync(fake));
  chmodSync(join(directoryOnPath, "wasm-opt"), 0o755);
  const pathLog = join(directory, "on-path.log");
  const onPath = { ...env, PATH: `${directoryOnPath}${delimiter}${process.env.PATH}`, FAKE_WASM_OPT_LOG: pathLog };
  delete onPath.TSUZURI_WASM_OPT;
  build(project, ["--target", "wasm32"], ".wasm", onPath);
  assert.ok(!existsSync(pathLog), "an installed wasm-opt is not run without --wasm-opt");
  assert.equal(customSections(build(project, ["--target", "wasm32", "--wasm-opt"], ".wasm", onPath), "fake-opt"), 1);
});

section("a real wasm-opt makes the module smaller and keeps its imports, exports, and results", () => {
  // Only where Binaryen is installed (the stand-in above does not need it): TSUZURI_TEST_WASM_OPT or PATH.
  const tool = process.env.TSUZURI_TEST_WASM_OPT ?? "wasm-opt";
  const version = spawnSync(tool, ["--version"], { encoding: "utf8" });
  if (version.error || version.status !== 0) {
    console.log("  (no wasm-opt: set TSUZURI_TEST_WASM_OPT to run the real-tool check)");
    return;
  }
  const project = wasmProject();
  const real = { ...env, TSUZURI_WASM_OPT: tool };
  for (const level of ["-O3", "-Oz"]) {
    const plain = build(project, ["--target", "wasm32", level], ".wasm", real);
    const optimized = build(project, ["--target", "wasm32", level, "--wasm-opt"], ".wasm", real);
    assert.ok(statSync(optimized).size < statSync(plain).size, `${level}: ${statSync(optimized).size} < ${statSync(plain).size}`);
    wasmResults(optimized, `${level} --wasm-opt`);
    const [left, right] = [plain, optimized].map((file) => new WebAssembly.Module(readFileSync(file)));
    assert.deepEqual(WebAssembly.Module.imports(right), WebAssembly.Module.imports(left));
    assert.deepEqual(WebAssembly.Module.exports(right).map(({ name, kind }) => `${name}:${kind}`).sort(),
      WebAssembly.Module.exports(left).map(({ name, kind }) => `${name}:${kind}`).sort());
    assert.equal(new WebAssembly.Instance(right).exports.memory.buffer.byteLength,
      new WebAssembly.Instance(left).exports.memory.buffer.byteLength);
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

section("the option errors of --wasm-opt", () => {
  const fails = (args, message) => {
    const result = tsuzuri(args, { status: 2 });
    assert.ok(result.stderr.includes(`error[E2000]: ${message}`), `${args.join(" ")}\n${result.stderr}`);
  };
  const hello = example("hello");
  fails(["build", hello, "--target", "wasm32", "--wasm-opt", "--wasm-opt"], "wasm-opt specified more than once");
  fails(["run", hello, "--wasm-opt"], "--wasm-opt is only valid with build");
  fails(["build", hello, "--wasm-opt"], "--wasm-opt requires WASM output (--target wasm32, --emit wasm)");
  fails(["build", hello, "--target", "wasm32", "--emit", "object", "--wasm-opt"], "--wasm-opt requires WASM output (--target wasm32, --emit wasm)");
  fails(["build", hello, "--target", "wasm32", "--wasm-feature", "threads", "--wasm-opt"],
    "--wasm-opt cannot be combined with --wasm-feature threads");
  assert.match(tsuzuri(["--help"]).stdout, /--wasm-opt\s+Run Binaryen's wasm-opt -Oz/);
  assert.match(tsuzuri(["--help"]).stdout, /TSUZURI_WASM_OPT\s+Binaryen optimizer/);
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
// TSUZURI_TEST_SECTION=text runs only the sections whose name contains the text.
const only = process.env.TSUZURI_TEST_SECTION ?? "";
try {
  for (const [name, body] of sections) {
    if (!name.includes(only)) continue;
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
