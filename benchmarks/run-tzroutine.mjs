// TZ10 (S3, D1): the compute kernels of benchmarks/tzroutine/Main.tz, which run on a tzroutine (so
// that the code has the preemption checks), built with one compiler or with two and run in turn.
// Every run checks the results against values computed here. No speed threshold: it reports.
//
// usage: node benchmarks/run-tzroutine.mjs [compiler] [--baseline compiler] [--samples N] [--quick] [--json file]
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { arch, cpus, platform, release, tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const usage = "usage: node benchmarks/run-tzroutine.mjs [compiler] [--baseline compiler] [--samples N] [--quick] [--json file]";
const options = { samples: 15 };
const positional = [];
const args = process.argv.slice(2);
for (let index = 0; index < args.length; index++) {
  const argument = args[index];
  if (argument === "--quick") options.quick = true;
  else if (["--baseline", "--samples", "--json"].includes(argument)) {
    const value = args[++index];
    if (value === undefined) throw new Error(usage);
    options[argument.slice(2)] = argument === "--samples" ? Number(value) : resolve(value);
  } else if (!argument.startsWith("-")) positional.push(resolve(argument));
  else throw new Error(usage);
}
if (positional.length > 1 || !Number.isInteger(options.samples) || options.samples < 1) throw new Error(usage);
const compiler = positional[0] ?? join(root, "target/release/tsuzuri");

// The kernels' results, computed independently.
function expected() {
  const step = (x) => (x * 31 + 7) % 1000003;
  const fib = (n) => { let a = 0, b = 1; for (let i = 0; i < n; i++) [a, b] = [b, a + b]; return a; };
  const gcd = (a, b) => { while (b !== 0) [a, b] = [b, a % b]; return a; };
  let gcds = 0;
  for (let a = 1; a <= 1500; a++) for (let b = 1; b <= 1500; b++) gcds += gcd(a, b);
  const values = Array.from({ length: 1000000 }, (_, i) => step(i));
  let squares = 0;
  for (let round = 1; round <= 40; round++) for (const v of values) squares += (v % 4096) ** 2;
  let mapFold = 0;
  for (let round = 1; round <= 20; round++) for (const v of values) mapFold += (v * 2 + round) * 3;
  let x = 1;
  for (let i = 0; i < 60000000; i++) x = step(x);
  const sorted = Array.from({ length: 400000 }, (_, i) => step((i * 7919) % 1000003)).sort((left, right) => left - right);
  const sort = sorted[0] + sorted[sorted.length >> 1] * 3 + sorted[sorted.length - 1] * 7;
  return { fib: fib(35), gcd: gcds, squares, map_fold: mapFold, chain: x, sort };
}

const temporary = mkdtempSync(join(tmpdir(), "tsuzuri-tzroutine-bench-"));
// The middle sample, or the mean of the two middle ones for an even count.
const median = (values) => {
  const sorted = [...values].sort((a, b) => a - b);
  const middle = Math.floor(sorted.length / 2);
  return sorted.length % 2 === 1 ? sorted[middle] : (sorted[middle - 1] + sorted[middle]) / 2;
};
try {
  const want = expected();
  const builds = [["new", compiler], ...(options.baseline ? [["baseline", options.baseline]] : [])].map(([name, path]) => {
    const executable = join(temporary, `${name}${process.platform === "win32" ? ".exe" : ""}`);
    execFileSync(path, ["build", join(root, "benchmarks/tzroutine"), "-O3", "-o", executable], { stdio: ["ignore", "ignore", "pipe"] });
    return { name, executable, times: {} };
  });
  const sample = (build) => {
    const output = execFileSync(build.executable, [], { encoding: "utf8", timeout: 300_000 });
    for (const line of output.trim().split(/\r?\n/)) {
      const [kernel, nanoseconds, value] = line.split(" ");
      assert.equal(Number(value), want[kernel], `${build.name} ${kernel}`);
      // a clock that could not be read is no sample (`clock_error`), and neither is a zero
      assert.match(nanoseconds, /^[1-9][0-9]*$/, `${build.name} ${kernel}: no duration`);
      (build.times[kernel] ??= []).push(Number(nanoseconds) / 1e6);
    }
  };
  const samples = options.quick ? 1 : options.samples;
  for (let round = 0; round < samples; round++) {
    // Alternate the order, so that neither build always runs first.
    for (const build of round % 2 === 0 ? builds : [...builds].reverse()) sample(build);
  }
  const kernels = Object.keys(want);
  const rows = kernels.map((kernel) => {
    const [mine, theirs] = builds.map((build) => build.times[kernel]);
    const row = { kernel, new_ms: median(mine), new_min: Math.min(...mine), new_max: Math.max(...mine) };
    if (theirs) Object.assign(row, { baseline_ms: median(theirs), baseline_min: Math.min(...theirs), baseline_max: Math.max(...theirs), change_percent: (median(mine) / median(theirs) - 1) * 100 });
    return row;
  });
  const environment = { platform: platform(), release: release(), arch: arch(), cpu: cpus()[0]?.model, cpus: cpus().length, samples };
  console.log(JSON.stringify(environment));
  for (const row of rows) {
    const range = (low, middle, high) => `${middle.toFixed(2)} (${low.toFixed(2)}-${high.toFixed(2)})`;
    console.log(`${row.kernel.padEnd(9)} new ${range(row.new_min, row.new_ms, row.new_max)} ms` +
      (row.baseline_ms === undefined ? "" : `  baseline ${range(row.baseline_min, row.baseline_ms, row.baseline_max)} ms  ${row.change_percent >= 0 ? "+" : ""}${row.change_percent.toFixed(2)} %`));
  }
  if (options.json) writeFileSync(options.json, JSON.stringify({ environment, rows, raw: builds.map(({ name, times }) => ({ name, times })) }, null, 2));
} finally {
  rmSync(temporary, { recursive: true, force: true });
}
