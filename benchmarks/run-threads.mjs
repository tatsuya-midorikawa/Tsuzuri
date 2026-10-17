import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { platform } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { parseTimeOutput, parseTimePasses } from "./metrics.mjs";

// PB04: the thread count against the compiler before PB04, one round of every configuration after another, so that a load that
// drifts during the run (a shared machine) touches them all alike. Prints the median (min-max) of the wall time of the process,
// the compiler's own total and phases (TSUZURI_TIME_PASSES), user+sys CPU time and peak RSS, and writes the raw samples.
//   node benchmarks/run-threads.mjs <compiler> <project dir> <check|emit-llvm> <runs> --out <dir> <configuration>...
// A configuration is a thread count (1..32), `unset` (the compiler's own choice) or `before` (the compiler named by --before).
// --cache cold|warm gives every configuration a cache directory of its own (fresh for every run, or filled by the first run) and
// leaves every cache on, parsed sources included, which --no-cache would turn off; without it every cache is disabled.
const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const args = process.argv.slice(2);
const options = { cache: null, before: null, out: null };
const positional = [];
while (args.length) {
  const argument = args.shift();
  if (argument === "--out") options.out = resolve(args.shift());
  else if (argument === "--before") options.before = resolve(args.shift());
  else if (argument === "--cache") options.cache = args.shift();
  else positional.push(argument);
}
const [compiler, project, variant, runs, ...configurations] = positional;
assert.ok(compiler && project && ["check", "emit-llvm"].includes(variant) && Number(runs) >= 1 && configurations.length && options.out,
  "Usage: node benchmarks/run-threads.mjs <compiler> <project> <check|emit-llvm> <runs> --out <dir> [--before <compiler>] [--cache cold|warm] <configuration>...");
assert.ok([null, "cold", "warm"].includes(options.cache), "--cache is cold or warm");
assert.ok(["darwin", "linux"].includes(platform()), "RSS measurement requires macOS or Linux");
assert.ok(configurations.every((name) => name === "unset" || (name === "before" && options.before) || /^(?:[1-9]|[12]\d|3[0-2])$/.test(name)),
  "a configuration is 1..32, unset or before (with --before)");
mkdirSync(options.out, { recursive: true });
const median = (values) => [...values].sort((a, b) => a - b)[Math.floor(values.length / 2)];
const range = (values, digits = 1) => `${median(values).toFixed(digits)} (${Math.min(...values).toFixed(digits)}-${Math.max(...values).toFixed(digits)})`;
const samples = Object.fromEntries(configurations.map((name) => [name, { wall_ms: [], user_ms: [], sys_ms: [], cpu_ms: [], peak_rss_bytes: [], total_ms: [], phases: {} }]));
for (let round = 0; round <= Number(runs); round++) {
  for (const name of configurations) {
    const env = { ...process.env, LC_ALL: "C", TSUZURI_CACHE_DIR: "", TSUZURI_TIME_PASSES: "1" };
    delete env.TSUZURI_THREADS;
    if (name !== "unset" && name !== "before") env.TSUZURI_THREADS = name;
    if (options.cache) {
      const cache = join(options.out, `cache-${name}`);
      if (options.cache === "cold") rmSync(cache, { recursive: true, force: true });
      env.TSUZURI_CACHE_DIR = cache;
    }
    const output = join(options.out, `${name}.ll`);
    const command = variant === "check" ? ["check", project]
      : ["build", project, "--emit", "llvm", "-O0", "--cpu", "generic", ...(options.cache ? [] : ["--no-cache"]), "-o", output];
    const report = join(options.out, "time.out");
    const started = process.hrtime.bigint();
    const child = spawnSync("/usr/bin/time", [platform() === "darwin" ? "-l" : "-v", "-o", report, name === "before" ? options.before : resolve(compiler), ...command],
      { cwd: root, encoding: "utf8", env, maxBuffer: 256 * 1024 * 1024, timeout: 600_000 });
    const wall = Number(process.hrtime.bigint() - started) / 1e6;
    assert.ifError(child.error);
    assert.equal(child.status, 0, `${name}: ${child.stdout}${child.stderr}`);
    if (round === 0) continue; // the first round warms the file cache (and fills a warm frontend cache)
    const timing = parseTimeOutput(readFileSync(report, "utf8"), platform());
    const passes = parseTimePasses(child.stderr);
    const sample = samples[name];
    sample.wall_ms.push(wall);
    sample.user_ms.push(timing.user_ms);
    sample.sys_ms.push(timing.sys_ms);
    sample.cpu_ms.push(timing.user_ms + timing.sys_ms); // per run: the median of a sum is not the sum of the medians
    sample.peak_rss_bytes.push(timing.peak_rss_bytes);
    sample.total_ms.push(passes.total_ms);
    for (const phase of passes.phases) (sample.phases[phase.name] ??= []).push(phase.ms);
  }
}
console.log(`# ${project} ${variant}${options.cache ? ` (frontend cache ${options.cache})` : ""}, ${runs} samples, compiler ${compiler}`);
for (const [name, sample] of Object.entries(samples)) {
  const phases = Object.entries(sample.phases).map(([phase, values]) => `${phase}=${median(values).toFixed(1)}`).join(" ");
  console.log(`${name.padEnd(6)} wall ${range(sample.wall_ms, 0)} ms, total ${range(sample.total_ms)} ms, cpu ${range(sample.cpu_ms, 0)} ms, rss ${range(sample.peak_rss_bytes.map((bytes) => bytes / 1048576))} MiB | ${phases}`);
}
writeFileSync(join(options.out, `${variant}${options.cache ? `-${options.cache}` : ""}.json`), JSON.stringify({ project, variant, runs: Number(runs), compiler, before: options.before, cache: options.cache, samples }, null, 1) + "\n");
