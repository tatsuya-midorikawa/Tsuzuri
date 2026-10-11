import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { copyFileSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { arch, cpus, platform, release, tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

// D11 Phase 2: CRC-32 over a generated buffer, with the 256-entry table read in place from static data,
// bound once with `let`, or bound in the loop, built by this compiler and by an earlier one (`--baseline`).
const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const args = process.argv.slice(2);
const usage = "Usage: node benchmarks/run-statics.mjs [compiler] [--baseline compiler] [--size bytes] [--runs number] [--artifacts directory] [--quick]";
const take = (flag, fallback) => {
  const index = args.indexOf(flag);
  if (index < 0) return fallback;
  const value = args[index + 1];
  if (value === undefined || value.startsWith("--")) throw new Error(usage);
  args.splice(index, 2);
  return value;
};
const baselineArgument = take("--baseline", null);
const baseline = baselineArgument && resolve(baselineArgument);
const quick = args.includes("--quick");
const size = Number(take("--size", quick ? "65536" : String(1 << 22)));
const runs = Number(take("--runs", quick ? "1" : "9"));
const artifacts = take("--artifacts", null);
const positional = args.filter((arg) => arg !== "--quick");
if (!Number.isInteger(size) || size < 1024 || size > 1 << 30 || !Number.isInteger(runs) || runs < 1 || runs > 64
  || positional.length > 1 || positional.some((arg) => arg.startsWith("-"))) {
  throw new Error(usage);
}
const compiler = resolve(positional[0] ?? join(root, "target/release/tsuzuri"));
const clang = process.env.TSUZURI_CLANG ?? "clang";
const run = (program, arguments_) => execFileSync(program, arguments_, {
  cwd: root, encoding: "utf8", timeout: 300_000, stdio: ["ignore", "pipe", "pipe"],
});
const median = (values) => {
  const sorted = [...values].sort((a, b) => a - b);
  return (sorted[Math.floor((sorted.length - 1) / 2)] + sorted[Math.floor(sorted.length / 2)]) / 2;
};
const table = Array.from({ length: 256 }, (_, index) => {
  let value = index;
  for (let bit = 0; bit < 8; bit++) value = value & 1 ? (value >>> 1) ^ 0xEDB88320 : value >>> 1;
  return value >>> 0;
});
let crc = 0xFFFFFFFF;
for (let i = 0; i < size; i++) crc = table[(crc ^ ((i * 7 + 3) & 255)) & 0xFF] ^ (crc >>> 8);
const expected = (~crc) >>> 0;
const flags = ["-O3", "-fPIC", "-fno-fast-math", "-ffp-contract=off", "-Wno-override-module"];
const temporary = mkdtempSync(join(tmpdir(), "tsuzuri-statics-benchmark-"));

try {
  const fixture = "benchmarks/statics";
  writeFileSync(join(temporary, "table.inc"), `${table.map((value) => `UINT32_C(${value})`).join(",\n")}\n`);
  const objects = [];
  const build = (tool, name, rename) => {
    const ir = join(temporary, `${name}.ll`);
    const header = join(temporary, `${name}.h`);
    run(tool, ["build", fixture, "--emit", "llvm", "-O3", "-o", ir]);
    run(tool, ["build", fixture, "--emit", "header", "-o", header]);
    if (rename) {
      writeFileSync(ir, readFileSync(ir, "utf8").replaceAll("@tz_", `@${rename}tz_`));
      writeFileSync(header, readFileSync(header, "utf8").replaceAll("tz_", `${rename}tz_`));
    }
    run(clang, [...flags, "-c", ir, "-o", join(temporary, `${name}.o`)]);
    run(clang, [...flags, "-S", ir, "-o", join(temporary, `${name}.s`)]);
    run(clang, [...flags, "-S", "-emit-llvm", ir, "-o", join(temporary, `${name}.optimized.ll`)]);
    objects.push(`${name}.o`);
  };
  build(compiler, "statics", "");
  if (baseline) build(baseline, "before", "before_");
  const native = join(temporary, "benchmark");
  run(clang, ["-std=c11", ...flags, "-Wall", "-Wextra", "-Werror", ...(baseline ? ["-DBASELINE"] : []),
    `${fixture}/host.c`, "-I", temporary, ...objects.map((name) => join(temporary, name)), "-lm", "-o", native]);
  const result = JSON.parse(run(native, ["--size", String(size), "--runs", String(runs)]));
  assert.equal(result.checksum, expected);
  const variants = Object.fromEntries(result.variants.map((variant) => [variant.name, variant.ms]));
  const report = result.variants.map((variant) => ({
    name: variant.name,
    median_cpu_ms: median(variant.ms),
    cpu_ns_per_byte: median(variant.ms) * 1e6 / size,
    min_cpu_ms: Math.min(...variant.ms),
    median_wall_ms: median(variant.wall_ms),
    raw_cpu_ms: variant.ms,
    raw_wall_ms: variant.wall_ms,
  }));
  // The optimized IR of an export: whether it still allocates, copies, or reserves the table.
  const body = (file, name) => {
    const optimized = readFileSync(join(temporary, file), "utf8");
    const match = optimized.match(new RegExp(`^define [^\\n]*@${name}\\([^\\n]*\\{[\\s\\S]*?^\\}`, "m"));
    assert.ok(match, `${name} is in ${file}`);
    return match[0];
  };
  const inspect = (file, prefix) => Object.fromEntries(["crc_in_place", "crc_bound", "crc_copied"].map((name) => {
    const text = body(file, `${prefix}tz_${name}`);
    return [name, {
      allocas: (text.match(/\balloca\b/g) ?? []).length,
      memcpy: (text.match(/@llvm\.memcpy/g) ?? []).length,
      allocator_calls: (text.match(/@(?:tz\.alloc|malloc)\(/g) ?? []).length,
      static_table_reads: (text.match(/@tz\.static\.\d+\.data/g) ?? []).length,
    }];
  }));
  const inspection = { current: inspect("statics.optimized.ll", ""), ...(baseline ? { before: inspect("before.optimized.ll", "before_") } : {}) };
  if (artifacts) {
    mkdirSync(resolve(artifacts), { recursive: true });
    for (const name of ["statics.ll", "statics.s", "statics.optimized.ll", ...(baseline ? ["before.ll", "before.s", "before.optimized.ll"] : [])]) {
      copyFileSync(join(temporary, name), join(resolve(artifacts), name));
    }
  }
  console.log(JSON.stringify({
    environment: {
      tsuzuri: run(compiler, ["--version"]).trim(),
      compiler_sha256: createHash("sha256").update(readFileSync(compiler)).digest("hex"),
      baseline: baseline ?? null,
      baseline_sha256: baseline ? createHash("sha256").update(readFileSync(baseline)).digest("hex") : null,
      clang: run(clang, ["--version"]).trim(),
      node: process.version, platform: platform(), os_release: release(),
      architecture: arch(), cpu: cpus()[0]?.model, logical_cpus: cpus().length,
    },
    mode: quick ? "correctness-smoke" : "benchmark",
    size, runs, clang_flags: flags, clock: "CLOCK_THREAD_CPUTIME_ID and CLOCK_MONOTONIC",
    speed_over_before: baseline ? Object.fromEntries(["in_place", "bound", "copied"].map((name) =>
      [name, median(variants[`before_${name}`]) / median(variants[name])])) : null,
    variants: report, inspection,
    note: "Thread CPU time of one call over `size` generated bytes (the wall-clock time is reported too); one warmup call; rotating order; no LTO. The checksum of every variant equals the JavaScript CRC-32. No speed threshold applies.",
  }, null, 2));
} finally {
  rmSync(temporary, { recursive: true, force: true });
}
