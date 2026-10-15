import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, statSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { performance } from "node:perf_hooks";
import { captureRun, makeRecord, writeRecords } from "./metrics.mjs";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
assert.ok(Number(process.versions.node.split(".")[0]) >= 24, "numeric benchmarks require Node.js 24+");
const args = process.argv.slice(2);
let runtime = join(root, "src/runtime/numeric.ll"), metrics = join(root, "target/perf/PR04-numeric");
let compiler = join(root, "target/release/tsuzuri"), quick = false;
for (let i = 0; i < args.length; ++i) {
  if (args[i] === "--runtime") runtime = resolve(args[++i]);
  else if (args[i] === "--metrics") metrics = resolve(args[++i]);
  else if (args[i] === "--compiler") compiler = resolve(args[++i]);
  else if (args[i] === "--quick") quick = true;
  else throw new Error(`unknown option: ${args[i]}`);
}
const clang = process.env.TSUZURI_CLANG ?? "clang";
const scratchRoot = join(root, "target/numeric-benchmark");
mkdirSync(scratchRoot, { recursive: true });
mkdirSync(metrics, { recursive: true });
const scratch = mkdtempSync(join(scratchRoot, "run-"));
const context = captureRun(metrics, { compiler, clang, mode: quick ? "quick" : "benchmark" });
const execute = (program, arguments_, input) => {
  const result = spawnSync(program, arguments_, {
    cwd: root, input, encoding: "utf8", timeout: 1_800_000, maxBuffer: 64 * 1024 * 1024,
  });
  if (result.error) throw result.error;
  assert.equal(result.status, 0, `${program} ${arguments_.join(" ")}\n${result.stderr}${result.stdout}`);
  return result.stdout.replaceAll("\r\n", "\n");
};
const work = [];
function format(name, kind, low, high = "0") {
  work.push({ name: `${name}-format`, kind, low: BigInt(`0x${low}`), high: BigInt(`0x${high}`) });
}
function parse(name, kind, text) { work.push({ name: `${name}-parse`, kind, text }); }
for (const [name, hex] of [["tenth", "3dcccccd"], ["pi", "40490fdb"], ["max", "7f7fffff"], ["min", "1"]])
  format(`f32-${name}`, 1, hex);
for (const [name, hex] of [["tenth", "3fb999999999999a"], ["pi", "400921fb54442d18"],
  ["1e23", "44b52d02c7e14af6"], ["min", "1"], ["max", "7fefffffffffffff"], ["integer", "40fe240000000000"]])
  format(`f64-${name}`, 2, hex);
for (const [name, text] of [["tenth", "0.1"], ["pi", "3.1415927"], ["max", "3.4028235e+38"]])
  parse(`f32-${name}`, 1, text);
for (const [name, text] of [["tenth", "0.1"], ["pi", "3.141592653589793"], ["max", "1.7976931348623157e+308"],
  ["min", "5e-324"], ["long", "1.000000000000000000000001"], ["underscore", "1_000.5"]])
  parse(`f64-${name}`, 2, text);
format("f16-tenth", 0, "2e66");
format("f128-tenth", 3, "999999999999999a", "3ffb999999999999");
format("d64-tenth", 5, "31a0000000000001");
parse("f16-tenth", 0, "0.1");
parse("f128-tenth", 3, "0.1");
parse("d64-tenth", 5, "0.1");
for (const [kind, prefix, max] of [[27, "i64u", "ffffffffffffffff"], [26, "i32u", "ffffffff"], [25, "i16u", "ffff"]]) {
  format(`${prefix}-max`, kind, max);
  format(`${prefix}-small`, kind, "2a");
  parse(`${prefix}-max`, kind, BigInt(`0x${max}`).toString());
  parse(`${prefix}-small`, kind, "42");
}
for (const item of work) {
  // Workload names follow the existing performance-ticket convention.
  item.name = item.name.replace(/^(.*)-(.*)-(format|parse)$/, "$1-$3-$2");
  item.repeats = (item.kind >= 16 ? 2_000_000 : 200_000) / (quick ? 100 : 1);
}
const records = [];
const record = (item, target, samples) => records.push(makeRecord(context, {
  suite: "numeric", workload: item.name, size: item.repeats, language: "tsuzuri",
  target, opt: "O3", metric: target === "native" ? "cpu_time" : "wall_time", samples,
}));
try {
  const harness = join(root, "tests/display_parse_runtime.c");
  const native = join(scratch, "runtime");
  execute(clang, ["-O3", "-Wno-override-module", harness, runtime,
    ...(process.platform === "win32" ? [join(root, "src/runtime/wasm.ll")] : []), "-o", native]);
  const protocol = work.map((item) => item.text === undefined
    ? `F ${item.kind} ${item.repeats} ${item.low.toString(16)} ${item.high.toString(16)}`
    : `P ${item.kind} ${item.repeats} ${Buffer.from(item.text).toString("hex")}`).join("\n") + "\n";
  const samples = work.map(() => []);
  let checksums;
  for (let sample = -1; sample < (quick ? 1 : 9); ++sample) {
    const lines = execute(native, [], protocol).trim().split("\n");
    assert.equal(lines.length, work.length);
    const actual = lines.map((line) => line.split(" ")[1]);
    if (checksums) assert.deepEqual(actual, checksums, "native benchmark checksum changed");
    else checksums = actual;
    lines.forEach((line, index) => {
      const time = Number(line.split(" ")[0]);
      assert.ok(Number.isFinite(time) && time >= 0);
      if (sample >= 0) samples[index].push(time);
    });
  }
  work.forEach((item, index) => record(item, "native", samples[index]));
  const wasm = join(scratch, "runtime.wasm");
  execute(clang, ["--target=wasm32-unknown-unknown", "-O3", "-ffreestanding", "-fno-builtin",
    "-Wno-override-module", "-mbulk-memory", "-nostdlib", harness, runtime,
    join(root, "src/runtime/wasm.ll"), "-Wl,--no-entry", "-Wl,--export-memory",
    "-Wl,-z,stack-size=1048576", "-Wl,--max-memory=16777216",
    ...["input_pointer", "output_pointer", "result_low", "result_high", "format_value", "parse_value"]
      .map((name) => `-Wl,--export=${name}`), "-o", wasm]);
  const module = new WebAssembly.Module(readFileSync(wasm));
  assert.deepEqual(WebAssembly.Module.imports(module), []);
  const api = new WebAssembly.Instance(module).exports;
  const memory = new Uint8Array(api.memory.buffer);
  for (const item of work) {
    const times = [];
    const isParse = item.text !== undefined;
    const bytes = isParse ? Buffer.from(item.text) : null;
    if (bytes) memory.set(bytes, api.input_pointer());
    let expected;
    for (let sample = -1; sample < (quick ? 1 : 9); ++sample) {
      let checksum = 0;
      const start = performance.now();
      if (isParse) {
        for (let repeat = 0; repeat < item.repeats; ++repeat) checksum += api.parse_value(item.kind, bytes.length);
      } else {
        for (let repeat = 0; repeat < item.repeats; ++repeat)
          checksum += api.format_value(item.kind, item.low, item.high);
      }
      const duration = (performance.now() - start) / item.repeats;
      if (expected !== undefined) assert.equal(checksum, expected, "WASM benchmark checksum changed");
      else expected = checksum;
      assert.ok(checksum > 0, "benchmark operation failed");
      if (sample >= 0) times.push(duration);
    }
    record(item, "wasm32", times);
  }
  for (const target of ["native", "wasm32"]) {
    for (const opt of ["O0", "O3"]) {
      const object = join(scratch, `${target}-${opt}.o`);
      execute(clang, [...(target === "wasm32" ? ["--target=wasm32-unknown-unknown"] : []),
        `-${opt}`, "-Wno-override-module", "-c", "-x", "ir", runtime, "-o", object]);
      records.push(makeRecord(context, { suite: "numeric", workload: "runtime", language: "tsuzuri",
        target, opt, metric: "object_bytes", samples: [statSync(object).size] }));
    }
  }
  writeRecords(metrics, "numeric", records);
  writeFileSync(join(metrics, "env.txt"), JSON.stringify({
    ...context, runtime, runtime_sha256: createHash("sha256").update(readFileSync(runtime)).digest("hex"),
    native_checksums: checksums,
    timing: "native clock(), WASM performance.now(); ms per operation; stdio outside timed interval",
  }, null, 2) + "\n");
  console.log(`numeric: ${records.length} records (${quick ? 1 : 9} samples), ${metrics}`);
} finally {
  rmSync(scratch, { recursive: true, force: true });
}
