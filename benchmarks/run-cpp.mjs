import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdtempSync, rmSync } from "node:fs";
import { arch, cpus, platform, release, tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const args = process.argv.slice(2);
const usage = "Usage: node benchmarks/run-cpp.mjs [compiler] [--quick] [--cpu generic|native]";
const cpuIndex = args.indexOf("--cpu");
const cpu = cpuIndex < 0 ? "generic" : args.splice(cpuIndex, 2)[1];
if (!["generic", "native"].includes(cpu)) throw new Error(usage);
const quick = args.includes("--quick");
const positional = args.filter((arg) => arg !== "--quick");
if (positional.length > 1 || positional.some((arg) => arg.startsWith("-"))
    || args.filter((arg) => arg === "--quick").length > 1) {
  throw new Error(usage);
}
const compiler = resolve(positional[0] ?? join(root, "target/release/tsuzuri"));
const clang = process.env.TSUZURI_CLANG ?? "clang";
const run = (program, arguments_) => execFileSync(program, arguments_, {
  cwd: root, encoding: "utf8", timeout: 180_000, stdio: ["ignore", "pipe", "pipe"],
});
const median = (values) => {
  const sorted = [...values].sort((a, b) => a - b);
  return (sorted[Math.floor((sorted.length - 1) / 2)] + sorted[Math.floor(sorted.length / 2)]) / 2;
};
const hex = (value) => BigInt.asUintN(64, value).toString(16).padStart(16, "0");
const references = {
  integer_mix(size, seed) {
    let state = BigInt.asUintN(64, seed);
    for (let index = 0; index < size; ++index) {
      state = BigInt.asUintN(64, (state ^ (state >> 13n)) * 6364136223846793005n + 1442695040888963407n);
    }
    return state;
  },
  mandelbrot(size, seed) {
    if (size === 0) return 0n;
    const dx = 3 / size, dy = 2 / size, offset = Number(seed) * 0.000001;
    let total = 0;
    for (let y = 0; y < size; ++y) {
      for (let x = 0; x < size; ++x) {
        const cr = x * dx - 2 + offset, ci = y * dy - 1;
        let real = 0, imaginary = 0, count = 0;
        while (count < 256 && real * real + imaginary * imaginary <= 4) {
          const nextReal = real * real - imaginary * imaginary + cr;
          imaginary = 2 * real * imaginary + ci;
          real = nextReal;
          ++count;
        }
        total += count;
      }
    }
    return BigInt(total);
  },
  array_sum(size, seed) {
    let total = 0n;
    for (let index = 0; index < size; ++index) {
      total += (BigInt(index) ^ BigInt.asUintN(64, seed)) * 6364136223846793005n + 1442695040888963407n;
    }
    return total;
  },
};
const flags = ["-O3", "-std=c++20", "-fPIC", "-fno-fast-math", "-ffp-contract=off",
  "-Wall", "-Wextra", "-Werror"];
if (cpu === "native") {
  if (["x64", "ia32"].includes(arch())) flags.push("-march=native");
  else if (["arm64", "arm"].includes(arch())) flags.push("-mcpu=native");
  else throw new Error(`Native CPU tuning is unsupported on ${arch()}`);
}
const tsuzuriFlags = ["-O3", "--cpu", cpu];
const temporary = mkdtempSync(join(tmpdir(), "tsuzuri-cpp-benchmark-"));

try {
  const objects = [];
  for (const [source, name] of [["benchmarks/Mix.tz", "mix"], ["benchmarks/cpp/Kernels.tz", "kernels"]]) {
    const object = join(temporary, `${name}.o`);
    run(compiler, ["build", source, "--emit", "object", ...tsuzuriFlags, "-o", object]);
    run(compiler, ["build", source, "--emit", "header", "-o", join(temporary, `${name}.h`)]);
    objects.push(object);
  }
  const native = join(temporary, platform() === "win32" ? "benchmark.exe" : "benchmark");
  run(clang, ["--driver-mode=g++", ...flags, "benchmarks/cpp/native.cpp",
    "-I", temporary, ...objects, "-o", native, ...(platform() === "win32" ? [] : ["-lm"])]);
  const result = JSON.parse(run(native, quick ? ["--quick"] : []));
  assert.equal(result.samples, 10);
  assert.deepEqual(result.workloads.map((workload) => workload.name), Object.keys(references));
  for (const workload of result.workloads) {
    assert.equal(workload.checks.length, 25);
    for (const check of workload.checks) {
      assert.equal(check.checksum, hex(references[workload.name](check.size, BigInt(check.seed))),
        `${workload.name}: reference mismatch at size=${check.size}, seed=${check.seed}`);
    }
    assert.equal(workload.raw.length, result.samples);
    for (const sample of workload.raw) {
      assert.ok(Number.isFinite(sample.cpp_ms) && sample.cpp_ms >= 0);
      assert.ok(Number.isFinite(sample.tsuzuri_ms) && sample.tsuzuri_ms >= 0);
      if (!quick) assert.ok(sample.cpp_ms > 0 && sample.tsuzuri_ms > 0, "Increase workload size: clock resolution is insufficient");
    }
    workload.cpp_median_ms = median(workload.raw.map((sample) => sample.cpp_ms));
    workload.tsuzuri_median_ms = median(workload.raw.map((sample) => sample.tsuzuri_ms));
    workload.tsuzuri_over_cpp = workload.cpp_median_ms > 0
      ? workload.tsuzuri_median_ms / workload.cpp_median_ms : null;
  }
  console.log(JSON.stringify({
    environment: {
      tsuzuri: run(compiler, ["--version"]).trim(),
      clang: run(clang, ["--version"]).split(/\r?\n/)[0],
      node: process.version,
      platform: platform(),
      os_release: release(),
      architecture: arch(),
      cpu: cpus()[0]?.model,
      logical_cpus: cpus().length,
    },
    mode: quick ? "correctness-smoke" : "benchmark",
    cpp_flags: flags,
    tsuzuri_flags: tsuzuriFlags,
    ...result,
    note: "Native CPU time, two warmup pairs, alternating order, no LTO. Ratio < 1 favors Tsuzuri. Microbenchmarks are not a general language ranking; quick-mode timings are not meaningful.",
  }, null, 2));
} finally {
  rmSync(temporary, { recursive: true, force: true });
}
