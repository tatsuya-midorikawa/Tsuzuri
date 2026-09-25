import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, mkdirSync, readFileSync, writeFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const compiler = resolve(process.argv[2] ?? join(root, "target/debug/tsuzuri"));
const clang = process.env.TSUZURI_CLANG ?? "clang";
const temporary = mkdtempSync(join(tmpdir(), "tsuzuri-display-parse-"));
function execute(program, args, input) {
  const result = spawnSync(program, args, {
    cwd: root, input, encoding: "utf8", timeout: 300_000, maxBuffer: 64 * 1024 * 1024,
  });
  if (result.error) throw result.error;
  assert.equal(result.status, 0, `${program} ${args.join(" ")}\n${result.stderr}`);
  return result.stdout;
}
const cli = (args) => execute(compiler, args);
const mask = (bits) => (1n << BigInt(bits)) - 1n;
const decimalFormats = [[32, 7, 23, 101], [64, 16, 53, 398], [128, 34, 113, 6176]];
function canonical(raw, kind) {
  if (kind < 4 || kind >= 16) return raw;
  const [width, precision, fraction, bias] = decimalFormats[kind - 4];
  const sign = raw >> BigInt(width - 1) ? "-" : "";
  raw &= mask(width - 1);
  const combination = Number(raw >> BigInt(width - 6));
  if (combination >= 30) return combination === 30 ? `${sign}inf` : "nan";
  let coefficient, exponent;
  if (width !== 128 && raw >> BigInt(width - 3) === 3n) {
    exponent = Number(raw >> BigInt(fraction - 2) & mask(width - fraction - 1)) - bias;
    coefficient = (raw & mask(fraction - 2)) | 1n << BigInt(fraction);
  } else {
    exponent = Number(raw >> BigInt(fraction)) - bias;
    coefficient = raw & mask(fraction);
  }
  if (coefficient >= 10n ** BigInt(precision) || (width === 128 && raw >> 125n === 3n)) coefficient = 0n;
  if (coefficient === 0n) return `${sign}0`;
  while (coefficient % 10n === 0n) { coefficient /= 10n; ++exponent; }
  return `${sign}${coefficient}e${exponent}`;
}
function checkParse(kind, text, expected, ok, raw) {
  assert.equal(ok, expected === null ? 0 : 1, `parse ${kind} ${JSON.stringify(text).slice(0, 120)}`);
  if (ok) assert.equal(canonical(raw, kind), canonical(BigInt(`0x${expected}`), kind),
    `parse ${kind} ${JSON.stringify(text).slice(0, 120)}`);
}

try {
  const reference = JSON.parse(execute("python3", ["tests/display_parse_reference.py"]));
  const protocol = reference.formats.map(([kind, hex]) => {
    const raw = BigInt(`0x${hex}`);
    return `f ${kind} ${(raw & mask(64)).toString(16)} ${(raw >> 64n).toString(16)}\n`;
  }).join("") + reference.parses.map(([kind, text]) =>
    `p ${kind} ${Buffer.from(text).toString("hex") || "-"}\n`).join("");
  const runtime = join(root, "src/runtime/numeric.ll");
  const harness = join(root, "tests/display_parse_runtime.c");
  for (const optimization of [0, 3]) {
    const native = join(temporary, `runtime-O${optimization}`);
    execute(clang, [`-O${optimization}`, "-Wno-override-module", harness, runtime, "-o", native]);
    const lines = execute(native, [], protocol).trimEnd().split("\n");
    assert.equal(lines.length, reference.formats.length + reference.parses.length);
    let index = 0;
    for (const [kind, hex, expected] of reference.formats) {
      assert.equal(lines[index++], expected, `native O${optimization}: format ${kind} ${hex}`);
    }
    for (const [kind, text, expected] of reference.parses) {
      const [ok, raw] = lines[index++].split(" ");
      checkParse(kind, text, expected, Number(ok), BigInt(`0x${raw}`));
    }

    const wasm = join(temporary, `runtime-O${optimization}.wasm`);
    execute(clang, [
      "--target=wasm32-unknown-unknown", `-O${optimization}`, "-ffreestanding", "-fno-builtin",
      "-Wno-override-module", "-mbulk-memory", "-nostdlib", harness, runtime,
      join(root, "src/runtime/wasm.ll"), "-Wl,--no-entry", "-Wl,--export-memory",
      "-Wl,-z,stack-size=1048576", "-Wl,--max-memory=16777216",
      ...["input_pointer", "output_pointer", "result_low", "result_high", "format_value", "parse_value"]
        .map((name) => `-Wl,--export=${name}`),
      "-o", wasm,
    ]);
    const module = new WebAssembly.Module(readFileSync(wasm));
    assert.deepEqual(WebAssembly.Module.imports(module), []);
    const api = new WebAssembly.Instance(module).exports;
    const memory = new Uint8Array(api.memory.buffer);
    const decoder = new TextDecoder();
    for (const [kind, hex, expected] of reference.formats) {
      const raw = BigInt(`0x${hex}`);
      const length = api.format_value(kind, raw & mask(64), raw >> 64n);
      assert.ok(length > 0 && length <= 128);
      const pointer = api.output_pointer();
      assert.equal(decoder.decode(memory.subarray(pointer, pointer + length)), expected,
        `WASM O${optimization}: format ${kind} ${hex}`);
    }
    for (const [kind, text, expected] of reference.parses) {
      const bytes = Buffer.from(text);
      memory.set(bytes, api.input_pointer());
      const ok = api.parse_value(kind, bytes.length);
      const raw = BigInt.asUintN(64, api.result_low()) | BigInt.asUintN(64, api.result_high()) << 64n;
      checkParse(kind, text, expected, ok, raw);
    }
    assert.ok(api.memory.buffer.byteLength <= 16 * 1024 * 1024);
    console.log(`display/parse O${optimization}: ${reference.formats.length} exact displays, ${reference.parses.length} parse/round-trip cases on native/WASM`);
  }

  execute(process.execPath, [join(root, "tests/features.mjs"), compiler, "display_parse"]);
  for (const [name, source, expected, symbol] of [
    ["parse-only", 'export def value :: i64\nfn value = { let text = "42"; let n: Option<i64> = Parse.parse (&text); Option.get n }', 42, "parse"],
    ["display-only", "export def value :: i64\nfn value = (to_string 42).length", 2, "format"],
  ]) {
    const directory = join(temporary, name);
    mkdirSync(directory);
    writeFileSync(join(directory, "Main.tz"), source);
    const ir = join(directory, "program.ll");
    cli(["build", directory, "--emit", "llvm", "-o", ir]);
    assert.ok(readFileSync(ir, "utf8").includes(`call i32 @tz_soft_${symbol}`));
    const host = join(directory, "host.c");
    writeFileSync(host, `#include <assert.h>\nextern long long tz_value(void);\nint main(void) { assert(tz_value() == ${expected}); }\n`);
    for (const optimization of [0, 3]) {
      const native = join(directory, `host-O${optimization}`);
      execute(clang, [`-O${optimization}`, "-Wno-override-module", host, ir, "-o", native]);
      execute(native, []);
      const wasm = join(directory, `program-O${optimization}.wasm`);
      cli(["build", directory, "--target", "wasm32", `-O${optimization}`, "-o", wasm]);
      const module = new WebAssembly.Module(readFileSync(wasm));
      assert.deepEqual(WebAssembly.Module.imports(module), []);
      assert.equal(new WebAssembly.Instance(module).exports.tz_value(), BigInt(expected));
    }
  }
  const consoleDirectory = join(temporary, "console");
  mkdirSync(consoleDirectory);
  const source = join(consoleDirectory, "Main.tz");
  for (const [expression, expected] of reference.console) {
    for (const value of [expression, `to_string (${expression})`]) {
      writeFileSync(source, value);
      for (const optimization of [0, 3]) {
        assert.equal(cli(["run", source, `-O${optimization}`]), `${expected}\n`, value);
      }
    }
  }
  for (const [expression, expected] of [["()", ""], ["to_string ()", "()\n"]]) {
    writeFileSync(source, expression);
    assert.equal(cli(["run", source]), expected);
  }
  console.log("Display/Parse: shared console format, custom instances, owned cleanup, and standalone links passed");
} finally {
  rmSync(temporary, { recursive: true, force: true });
}
