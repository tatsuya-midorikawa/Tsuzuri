import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
if (Number(process.versions.node.split(".")[0]) < 24) {
  throw new Error("This benchmark requires Node.js 24+; run with a current LTS runtime without disabling JIT optimization.");
}
const args = process.argv.slice(2);
const usage = "Usage: node benchmarks/run-managed.mjs [compiler] [--quick] [--scale number] [--cpu generic|native] [--baseline compiler] [--artifacts directory]";
const take = (name, fallback) => {
  const index = args.indexOf(name);
  if (index < 0) return fallback;
  const value = args[index + 1];
  if (!value || value.startsWith("--")) throw new Error(usage);
  args.splice(index, 2);
  return value;
};
const cpu = take("--cpu", "generic");
const scaleArgument = take("--scale", null);
const baseline = take("--baseline", null);
const artifacts = take("--artifacts", null);
const quick = args.includes("--quick");
const positional = args.filter((value) => value !== "--quick");
const scale = Number(scaleArgument ?? (quick ? "1" : "0.01"));
if (positional.length > 1 || positional.some((value) => value.startsWith("-"))
    || args.filter((value) => value === "--quick").length > 1 || !["generic", "native"].includes(cpu)
    || !Number.isFinite(scale) || scale <= 0 || scale > 5 || (quick && scale !== 1)) throw new Error(usage);
const compiler = resolve(positional[0] ?? join(root, "target/release/tsuzuri"));
const dotnet = process.env.TSUZURI_DOTNET ?? "dotnet";
const run = (program, arguments_, environment = {}) => execFileSync(program, arguments_, {
  cwd: root, encoding: "utf8", timeout: 300000, maxBuffer: 16 * 1024 * 1024,
  stdio: ["ignore", "pipe", "pipe"], env: { ...process.env, ...environment },
});
const median = (values) => {
  const sorted = [...values].sort((left, right) => left - right);
  return (sorted[Math.floor((sorted.length - 1) / 2)] + sorted[Math.floor(sorted.length / 2)]) / 2;
};
const temporary = mkdtempSync(join(tmpdir(), "tsuzuri-managed-benchmark-"));
try {
  const native = {};
  for (const family of ["control", "cpp", "computations"]) {
    native[family] = JSON.parse(run(process.execPath, [`benchmarks/run-${family}.mjs`, compiler,
      "--cpu", cpu, ...(quick ? ["--quick"] : ["--scale", String(scale)]),
      ...(baseline && family !== "cpp" ? ["--baseline", resolve(baseline)] : []),
      ...(artifacts ? ["--artifacts", join(resolve(artifacts), family)] : [])]));
  }
  const jobs = Object.entries(native).flatMap(([family, result]) => result.workloads.map((work) => ({
    family, name: work.name, size: work.size,
    checks: work.checks.map((check) => ({ ...check, seed: BigInt.asUintN(64, BigInt(check.seed)).toString() })),
    samples: work.raw.map((sample, index) => ({ seed: BigInt.asUintN(64, BigInt(sample.seed ?? 42 + index)).toString(), checksum: sample.checksum })),
  })));
  const plan = join(temporary, "plan.json");
  writeFileSync(plan, JSON.stringify({ quick, jobs }));
  const dotnetEnvironment = { DOTNET_CLI_TELEMETRY_OPTOUT: "1", DOTNET_NOLOGO: "1", DOTNET_TieredCompilation: "0" };
  const output = join(temporary, "csharp");
  run(dotnet, ["build", "benchmarks/csharp/Tsuzuri.Benchmarks.csproj", "-c", "Release", "--nologo",
    "--artifacts-path", join(temporary, "dotnet-build"), "-o", output], dotnetEnvironment);
  const csharp = JSON.parse(run(dotnet, [join(output, "Tsuzuri.Benchmarks.dll"), plan], dotnetEnvironment));
  const javascript = JSON.parse(run(process.execPath, ["--expose-gc", "benchmarks/javascript.mjs", plan]));
  const workloads = jobs.map((job, index) => {
    const source = native[job.family].workloads.find((work) => work.name === job.name);
    const variants = {};
    for (const name of ["c", "cpp", "rust", "tsuzuri", "before", "direct"]) {
      const field = job.family === "computations" && name === "tsuzuri" ? "computation" : name;
      const times = source.raw.map((sample) => sample[`${field}_wall_ms`]);
      if (times.every((time) => time === undefined)) continue;
      assert.ok(times.every((time) => Number.isFinite(time) && time >= 0));
      variants[name] = { median_wall_ms: median(times), raw_wall_ms: times };
    }
    for (const [name, result] of [["csharp", csharp], ["javascript", javascript]]) {
      const measured = result.workloads[index];
      assert.equal(measured.family, job.family);
      assert.equal(measured.name, job.name);
      assert.equal(measured.checks, job.checks.length);
      assert.equal(measured.raw.length, job.samples.length);
      measured.raw.forEach((sample, sampleIndex) => {
        assert.equal(sample.checksum, job.samples[sampleIndex].checksum);
        assert.ok(Number.isFinite(sample.wall_ms) && sample.wall_ms >= 0);
      });
      variants[name] = { median_wall_ms: median(measured.raw.map((sample) => sample.wall_ms)),
        raw: measured.raw, post_measurement_gc_ms: measured.post_measurement_gc_ms };
    }
    return { family: job.family, name: job.name, size: job.size, checks: job.checks.length, variants,
      tsuzuri_over: Object.fromEntries(Object.entries(variants).filter(([, data]) => data.median_wall_ms > 0)
        .map(([name, data]) => [name, variants.tsuzuri.median_wall_ms / data.median_wall_ms])) };
  });
  const result = { mode: quick ? "correctness-smoke" : "benchmark", cpu, scale,
    native_options: Object.fromEntries(Object.entries(native).map(([family, data]) => [family, {
      c_cpp_flags: data.c_cpp_flags ?? data.cpp_flags ?? data.native_flags,
      rust_flags: data.rust_flags,
      tsuzuri_flags: data.tsuzuri_flags ?? data.native_flags,
    }])),
    environment: { native: Object.fromEntries(Object.entries(native).map(([family, data]) => [family, data.environment])),
      dotnet_sdk: run(dotnet, ["--version"]).trim(), csharp: csharp.environment, javascript: javascript.environment },
    workloads,
    note: "All ratios use monotonic wall time, identical sizes/seeds/checksums. Native variants rotate within one process; C# and JavaScript run sequentially in separate warmed processes. JIT/startup are excluded. In-measurement GC is included; forced post-measurement GC is separate, so managed storage is not an identical explicit-free lifecycle. JavaScript uses BigInt for integer equivalence. No speed thresholds; quick timings are not performance evidence.",
  };
  if (artifacts) {
    mkdirSync(resolve(artifacts), { recursive: true });
    writeFileSync(join(resolve(artifacts), "plan.json"), readFileSync(plan));
    writeFileSync(join(resolve(artifacts), "native.json"), JSON.stringify(native, null, 2));
    writeFileSync(join(resolve(artifacts), "result.json"), JSON.stringify(result, null, 2));
  }
  console.log(JSON.stringify(result, null, 2));
} finally {
  rmSync(temporary, { recursive: true, force: true });
}