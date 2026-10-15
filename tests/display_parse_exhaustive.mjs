import assert from "node:assert/strict";
import { spawn, spawnSync } from "node:child_process";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { availableParallelism } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
let f32 = false, range = [0, 2 ** 32], f32Samples = 0, f64 = 0, jobs = availableParallelism();
let baseline = join(root, "target/perf/PR04/numeric-before.ll");
const args = process.argv.slice(2);
for (let i = 0; i < args.length; ++i) {
  if (args[i] === "--f32") f32 = true;
  else if (args[i] === "--range") {
    range = args[++i].split(":").map(Number);
    f32 = true;
  } else if (args[i] === "--f32-samples") f32Samples = Number(args[++i]);
  else if (args[i] === "--f64") f64 = Number(args[++i]);
  else if (args[i] === "--jobs") jobs = Number(args[++i]);
  else if (args[i] === "--baseline-runtime") baseline = resolve(args[++i]);
  else throw new Error(`unknown option: ${args[i]}`);
}
assert.ok(f32 || f32Samples || f64, "specify --f32, --range START:END, --f32-samples COUNT or --f64 COUNT");
assert.ok(Number.isSafeInteger(jobs) && jobs > 0 && jobs <= 256, "invalid jobs");
assert.ok(range.length === 2 && range.every(Number.isSafeInteger) && range[0] >= 0 &&
  range[0] < range[1] && range[1] <= 2 ** 32, "invalid f32 range");
assert.ok(Number.isSafeInteger(f64) && f64 >= 0, "invalid f64 count");
assert.ok(Number.isSafeInteger(f32Samples) && f32Samples >= 0, "invalid f32 sample count");
const directory = join(root, "target/display-parse-exhaustive");
mkdirSync(directory, { recursive: true });
const scratch = mkdtempSync(join(directory, "run-"));
try {
  const before = join(scratch, "baseline.ll"), executable = join(scratch, `compare${process.platform === "win32" ? ".exe" : ""}`);
  writeFileSync(before, readFileSync(baseline, "utf8").replaceAll("@tz_soft_", "@tz_base_"));
  const build = spawnSync(process.env.TSUZURI_CLANG ?? "clang", [
    "-O3", "-Wno-override-module", join(root, "tests/display_parse_exhaustive.c"),
    join(root, "src/runtime/numeric.ll"), before,
    ...(process.platform === "win32" ? [join(root, "src/runtime/wasm.ll")] : []), "-o", executable,
  ], { encoding: "utf8", cwd: root });
  if (build.error) throw build.error;
  assert.equal(build.status, 0, build.stderr || build.stdout);
  async function compare(kind, start, count) {
    const tasks = [];
    const children = new Set();
    let failure;
    const fail = (error) => {
      if (!failure) {
        failure = error;
        for (const child of children) child.kill();
      }
    };
    for (let job = 0; job < Math.min(jobs, count); ++job) {
      const low = Math.floor(count * job / Math.min(jobs, count));
      const high = Math.floor(count * (job + 1) / Math.min(jobs, count));
      tasks.push(new Promise((accept, reject) => {
        const child = spawn(executable, [String(kind), String(start + low), String(high - low)], { cwd: root });
        children.add(child);
        let stdout = "", stderr = "";
        child.stdout.on("data", (data) => { stdout += data; });
        child.stderr.on("data", (data) => { stderr += data; });
        child.on("error", (error) => { fail(error); reject(error); });
        child.on("close", (status) => {
          children.delete(child);
          if (status !== 0) {
            const error = new Error(`comparison exited ${status}: ${stderr}${stdout}`);
            fail(error);
            reject(error);
          } else {
            const [checked, mismatches] = stdout.trim().split(" ").map(Number);
            try {
              assert.equal(checked, high - low);
              assert.equal(mismatches, 0);
              accept(checked);
            } catch (error) { fail(error); reject(error); }
          }
        });
      }));
    }
    const totals = await Promise.allSettled(tasks);
    if (failure) throw failure;
    for (const total of totals) if (total.status === "rejected") throw total.reason;
    assert.equal(totals.reduce((sum, total) => sum + total.value, 0), count);
    console.log(`f${kind === 2 ? 64 : 32}${kind === 3 ? " sampled" : ""}: ${count} values, 0 mismatches (display/parse/round-trip/canonical NaN)`);
  }
  if (f32) await compare(1, range[0], range[1] - range[0]);
  if (f32Samples) await compare(3, 0, f32Samples);
  if (f64) await compare(2, 0, f64);
} finally {
  rmSync(scratch, { recursive: true, force: true });
}
