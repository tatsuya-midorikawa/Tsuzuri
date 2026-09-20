import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { arch, platform, tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { performance } from "node:perf_hooks";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const compiler = resolve(process.argv[2] ?? join(root, "target/release/tsuzuri"));
const iterations = Number(process.argv[3] ?? 5_000_000);
if (!Number.isSafeInteger(iterations) || iterations <= 0 || iterations > 100_000_000) {
  throw new Error("Iterations must be an integer in 1..100000000.");
}
const clang = process.env.TSUZURI_CLANG ?? "clang";
const run = (program, args) => execFileSync(program, args, {
  cwd: root, encoding: "utf8", timeout: 120_000, stdio: ["ignore", "pipe", "pipe"],
});
const temporary = mkdtempSync(join(tmpdir(), "tsuzuri-benchmark-"));
const median = (values) => [...values].sort((a, b) => a - b)[Math.floor(values.length / 2)];
const hex = (value) => BigInt.asUintN(64, value).toString(16).padStart(16, "0");

try {
  const input = join(root, "benchmarks/Mix.tzr");
  const object = join(temporary, "mix.o");
  const wasm = join(temporary, "mix.wasm");
  const native = join(temporary, platform() === "win32" ? "benchmark.exe" : "benchmark");
  run(compiler, ["build", input, "--emit", "object", "-O3", "-o", object]);
  run(compiler, ["build", input, "--emit", "header", "-o", join(temporary, "mix.h")]);
  run(compiler, ["build", input, "--target", "wasm32", "-O3", "-o", wasm]);
  run(clang, ["-O3", "-std=c11", "-Wall", "-Wextra", "-Werror", "benchmarks/native.c",
    "-I", temporary, object, "-o", native, ...(platform() === "win32" ? [] : ["-lm"])]);
  const nativeResult = JSON.parse(run(native, [String(iterations)]));
  const bytes = readFileSync(wasm);
  const { module, instance } = await WebAssembly.instantiate(bytes);
  assert.deepEqual(WebAssembly.Module.imports(module), []);
  const mix = instance.exports.tz_mix;
  let reference = 42n;
  for (let index = 0; index < 1000; index++) {
    const unsigned = BigInt.asUintN(64, reference);
    reference = BigInt.asIntN(64, (unsigned ^ (unsigned >> 13n)) * 6364136223846793005n + 1442695040888963407n);
  }
  assert.equal(mix(1000n, 42n), reference);
  assert.equal(mix(0n, 42n), 42n);
  assert.throws(() => mix(-1n, 42n), WebAssembly.RuntimeError);
  for (let warmup = 0; warmup < 25; warmup++) mix(10_000n, BigInt(warmup));
  const wasmSamples = [];
  for (let sample = 0; sample < 9; sample++) {
    const count = BigInt(iterations);
    const seed = BigInt(42 + sample);
    const start = performance.now();
    const result = mix(count, seed);
    wasmSamples.push(performance.now() - start);
    assert.equal(hex(result), nativeResult.samples[sample].checksum);
  }
  const cMedian = median(nativeResult.samples.map((sample) => sample.c_ms));
  const tsuzuriMedian = median(nativeResult.samples.map((sample) => sample.tsuzuri_ms));
  console.log(JSON.stringify({
    environment: {
      tsuzuri: run(compiler, ["--version"]).trim(),
      clang: run(clang, ["--version"]).split(/\r?\n/)[0],
      node: process.version,
      platform: platform(),
      architecture: arch(),
    },
    workload: "loop-carried 64-bit integer mixing; identical inputs and checked outputs",
    iterations,
    samples: 9,
    native: {
      c_median_ms: cMedian,
      tsuzuri_median_ms: tsuzuriMedian,
      tsuzuri_over_c: cMedian > 0 ? tsuzuriMedian / cMedian : null,
      raw: nativeResult.samples,
    },
    wasm: { median_ms: median(wasmSamples), bytes: bytes.length, imports: 0, raw_ms: wasmSamples },
    note: "Native timings use CPU time; WASM uses wall time after warmup. This microbenchmark does not establish general C/C++ superiority.",
  }, null, 2));
} finally {
  rmSync(temporary, { recursive: true, force: true });
}
