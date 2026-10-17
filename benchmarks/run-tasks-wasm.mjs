// Times Task.parallel groups of benchmarks/tasks/Main.tz on the WASM threads pool (Node worker_threads), with the same
// shapes as run-tasks.mjs. The compiler embeds src/runtime/task-wasm-threads.c, so two compilers compare two pools.
// Usage: node benchmarks/run-tasks-wasm.mjs <compiler> [--quick] [--workers n] [--samples n] [--gap-us microseconds]
// Prints one JSON line. Each sample is a new pool: cold_ms is its first call, warm_ms the mean of back-to-back calls,
// p50_ms the median of calls timed one by one (with --gap-us, after an idle gap, and warm_ms is their mean).
import { execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { availableParallelism, tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { createThreadPool } from "../src/runtime/wasm-threads.mjs";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const usage = "Usage: node benchmarks/run-tasks-wasm.mjs <compiler> [--quick] [--workers n] [--samples n] [--gap-us microseconds]";
const options = {}, positional = [];
let quick = false;
const args = process.argv.slice(2);
for (let index = 0; index < args.length; index++) {
  const argument = args[index];
  if (argument === "--quick" && !quick) quick = true;
  else if (["--workers", "--samples", "--gap-us"].includes(argument) && options[argument] === undefined) {
    const value = Number(args[++index]);
    if (!Number.isInteger(value)) throw new Error(usage);
    options[argument] = value;
  } else if (!argument.startsWith("-")) positional.push(argument);
  else throw new Error(usage);
}
if (positional.length !== 1) throw new Error(usage);
const compiler = resolve(positional[0]);
const workers = options["--workers"] ?? Math.min(availableParallelism(), 32) - 1;
const samples = quick ? 3 : options["--samples"] ?? 9;
const gapUs = options["--gap-us"] ?? 0;
if (workers < 1 || workers > 31 || samples < 1 || gapUs < 0 || gapUs > 1_000_000) throw new Error(usage);

const workloads = [
  { name: "tiny", count: 4, rounds: 0, repetitions: quick ? 10 : 1000 },
  { name: "small", count: 32, rounds: 16, repetitions: quick ? 8 : 256 },
  { name: "bulk", count: 1024, rounds: 256, repetitions: quick ? 3 : 64 },
];
function reference(count, rounds) {
  let total = 0n;
  for (let index = 0; index < count; index++) {
    let value = BigInt(index);
    for (let round = 0; round < rounds; round++) value = (value * 1664525n + 1013904223n) & 2147483647n;
    total += value;
  }
  return total;
}
const median = (values) => [...values].sort((left, right) => left - right)[Math.floor(values.length / 2)];
const now = () => Number(process.hrtime.bigint()) / 1e6;
const sleeper = new Int32Array(new SharedArrayBuffer(4));

const directory = mkdtempSync(join(tmpdir(), "tsuzuri-task-wasm-benchmark-"));
try {
  const output = join(directory, "tasks.wasm");
  execFileSync(compiler, ["build", join(root, "benchmarks/tasks/Main.tz"), "--target", "wasm32", "--wasm-feature", "threads", "-O3", "-o", output], { cwd: root, stdio: "inherit" });
  const bytes = readFileSync(output);
  const results = [];
  for (const workload of workloads) {
    const expected = reference(workload.count, workload.rounds);
    const repetitions = gapUs > 0 ? Math.min(workload.repetitions, 300) : workload.repetitions;
    const cold = [], warm = [], p50 = [];
    for (let sample = 0; sample < samples; sample++) {
      const pool = await createThreadPool(bytes, { workers });
      const call = () => {
        const value = pool.call("tz_parallel_batch", BigInt(workload.count), BigInt(workload.rounds));
        if (value !== expected) throw new Error(`${workload.name}: wrong result ${value}, expected ${expected}`);
      };
      let start = now();
      call();
      cold.push(now() - start);
      for (let index = 0; index < 8; index++) call();
      if (gapUs === 0) {
        start = now();
        for (let index = 0; index < repetitions; index++) call();
        warm.push((now() - start) / repetitions);
      }
      const calls = [];
      for (let index = 0; index < repetitions; index++) {
        if (gapUs > 0) Atomics.wait(sleeper, 0, 0, gapUs / 1000);
        start = now();
        call();
        calls.push(now() - start);
      }
      p50.push(median(calls));
      if (gapUs > 0) warm.push(calls.reduce((sum, value) => sum + value, 0) / calls.length);
      await pool.close();
    }
    results.push({ name: workload.name, count: workload.count, rounds: workload.rounds, repetitions, cold_ms: median(cold), warm_ms: median(warm), p50_ms: median(p50), cold_all: cold, p50_all: p50 });
  }
  console.log(JSON.stringify({ compiler, workers, samples, gap_us: gapUs, node: process.version, results }));
} finally {
  rmSync(directory, { recursive: true, force: true });
}
