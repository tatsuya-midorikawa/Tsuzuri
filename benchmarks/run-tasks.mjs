import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { arch, cpus, platform, release, tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { captureRun, makeRecord, writeRecords } from "./metrics.mjs";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const usage = "Usage: node benchmarks/run-tasks.mjs [compiler] [--quick] [--data-parallel | --sweep] [--gap-us microseconds] [--runtime file] [--baseline-runtime file] [--artifacts directory] [--metrics directory]";
const options = {}, positional = [];
let quick = false;
let dataParallel = false;
let sweep = false;
const args = process.argv.slice(2);
for (let index = 0; index < args.length; index++) {
  const argument = args[index];
  if (argument === "--quick" && !quick) quick = true;
  else if (argument === "--data-parallel" && !dataParallel) dataParallel = true;
  else if (argument === "--sweep" && !sweep) sweep = true;
  else if (["--baseline-runtime", "--runtime", "--artifacts", "--metrics", "--gap-us"].includes(argument) && options[argument] === undefined) {
    const value = args[++index];
    if (value === undefined || value.startsWith("--")) throw new Error(usage);
    options[argument] = argument === "--gap-us" ? value : resolve(value);
  } else if (!argument.startsWith("-")) positional.push(argument);
  else throw new Error(usage);
}
// Idle time before every measured call, so that the workers have parked: it measures the wake-up, not a hot pool.
const gap = options["--gap-us"] === undefined ? 0 : Number(options["--gap-us"]);
if (!Number.isInteger(gap) || gap < 0 || gap > 1_000_000) throw new Error(usage);
if (positional.length > 1 || dataParallel && (sweep || options["--baseline-runtime"])) throw new Error(usage);
const compiler = resolve(positional[0] ?? join(root, "target/release/tsuzuri"));
const clang = process.env.TSUZURI_CLANG ?? "clang";
const run = (program, arguments_) => execFileSync(program, arguments_, { cwd: root, encoding: "utf8", timeout: 300_000, stdio: ["ignore", "pipe", "pipe"] });
const median = (values) => { const sorted = [...values].sort((left, right) => left - right); return sorted[Math.floor(sorted.length / 2)]; };
const hash = (path) => createHash("sha256").update(readFileSync(path)).digest("hex");
const context = options["--metrics"] ? captureRun(options["--metrics"], { compiler, clang, mode: quick ? "quick" : "benchmark" }) : null;
const temporary = mkdtempSync(join(tmpdir(), "tsuzuri-task-benchmark-"));
const artifacts = options["--artifacts"];
const poolRuntime = options["--runtime"] ?? join(root, "src/runtime/task.c");
const baselineRuntime = options["--baseline-runtime"];
// The suite of a PX01 run that the records belong to: a run directory keeps one file per suite and refuses to overwrite it.
const suite = sweep ? "tasks-sweep" : dataParallel ? "tasks-data" : gap > 0 ? "tasks-gap" : "tasks";
const workloads = dataParallel ? [
  { name: "small", count: 64, rounds: 8, repetitions: quick ? 2 : 128 },
  { name: "chunk-boundary", count: 8193, rounds: 16, repetitions: quick ? 2 : 32 },
  { name: "bulk", count: quick ? 16384 : 262144, rounds: 32, repetitions: quick ? 2 : 8 },
] : sweep ? [16, 64, 256, 1024, 4096, 16384].map((rounds) => (
  // 32 tasks whose sequential time grows with `rounds`: where does the pool stop costing more than it saves?
  { name: `rounds-${rounds}`, count: 32, rounds, repetitions: quick ? 2 : rounds <= 256 ? 512 : rounds <= 1024 ? 256 : 64 }
)) : [
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
  return total.toString();
}
const functions = ["parallel_batch", "data_parallel_batch", "sequential_batch", "data_map_batch", "sequential_map_batch", "data_reduce_batch", "sequential_reduce_batch"];
// One process measures one batch function: the cold first call, the mean of back-to-back warm calls (without a
// clock read per call), then every call timed on its own for the median, with an optional idle gap before each.
// It prints: result cold_ms warm_ms_per_call warm_p50_ms cpu_ms_per_call.
const hostSource = `#undef NDEBUG
#if defined(__linux__) && !defined(_GNU_SOURCE)
#define _GNU_SOURCE
#endif
#include <assert.h>
#include <inttypes.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/resource.h>
#include <time.h>
${functions.map((name) => `extern int64_t tz_${name}(int64_t, int64_t);`).join("\n")}
static double now(void) {
#ifdef __APPLE__
    return (double)clock_gettime_nsec_np(CLOCK_UPTIME_RAW) / 1000000.0;
#else
    struct timespec time;
    assert(clock_gettime(CLOCK_MONOTONIC, &time) == 0);
    return time.tv_sec * 1000.0 + time.tv_nsec / 1000000.0;
#endif
}
static double cpu(void) {
    struct rusage usage;
    assert(getrusage(RUSAGE_SELF, &usage) == 0);
    return (usage.ru_utime.tv_sec + usage.ru_stime.tv_sec) * 1000.0 + (usage.ru_utime.tv_usec + usage.ru_stime.tv_usec) / 1000.0;
}
static int ascending(const void *left, const void *right) {
    double a = *(const double *)left, b = *(const double *)right;
    return (a > b) - (a < b);
}
int main(int argc, char **argv) {
    if (argc != 6) return 2;
    int64_t count = strtoll(argv[1], 0, 10), rounds = strtoll(argv[2], 0, 10), repetitions = strtoll(argv[3], 0, 10);
    int64_t kind = strtoll(argv[4], 0, 10), gap = strtoll(argv[5], 0, 10);
    assert(count >= 0 && rounds >= 0 && repetitions > 0 && kind >= 0 && kind < 7 && gap >= 0);
    int64_t (*batches[])(int64_t, int64_t) = {${functions.map((name) => `tz_${name}`).join(", ")}};
    int64_t (*batch)(int64_t, int64_t) = batches[kind];
    int64_t expected = tz_sequential_batch(count, rounds);
    double start = now();
    int64_t result = batch(count, rounds);
    double cold = now() - start;
    assert(result == expected);
    double warm = 0, used = 0;
    if (gap == 0) {
        double cpu_start = cpu();
        start = now();
        for (int64_t iteration = 0; iteration < repetitions; iteration++) assert(batch(count, rounds) == expected);
        warm = (now() - start) / (double)repetitions;
        used = (cpu() - cpu_start) / (double)repetitions;
    }
    double *calls = malloc(sizeof(double) * (size_t)repetitions);
    assert(calls);
    double total = 0, timed_cpu = cpu();
    struct timespec idle = { (time_t)(gap / 1000000), (long)(gap % 1000000) * 1000 };
    for (int64_t iteration = 0; iteration < repetitions; iteration++) {
        if (gap) nanosleep(&idle, 0);
        double begin = now();
        assert(batch(count, rounds) == expected);
        calls[iteration] = now() - begin;
        total += calls[iteration];
    }
    if (gap) {
        warm = total / (double)repetitions;
        used = (cpu() - timed_cpu) / (double)repetitions;
    }
    qsort(calls, (size_t)repetitions, sizeof(double), ascending);
    printf("%" PRId64 " %.6f %.6f %.6f %.6f\\n", result, cold, warm, calls[repetitions / 2], used);
    return 0;
}
`;
// [name, runtime, function index]. The sweep compares the pool with the sequential loop of the same IR.
const backends = dataParallel
  ? ["parallel-init", "array-init", "parallel-map", "array-map", "parallel-sum", "array-sum"].map((name, index) => [name, poolRuntime, index + 1])
  : sweep ? [["parallel", poolRuntime, 0], ["sequential", poolRuntime, 2]] : [["pool", poolRuntime, 0]];
if (baselineRuntime) backends.push(["baseline", baselineRuntime, 0]);
try {
  const ir = join(temporary, "tasks.ll"), host = join(temporary, "host.c");
  // The allocator is named so that a different default cannot change what is compared.
  run(compiler, ["build", join(root, "benchmarks/tasks/Main.tz"), "--emit", "llvm", "--allocator", "system", "-o", ir]);
  writeFileSync(host, hostSource);
  const binaries = new Map();
  for (const [, runtime] of backends) {
    if (binaries.has(runtime)) continue;
    const binary = join(temporary, `host-${binaries.size}`);
    run(clang, ["-std=c11", "-O3", "-fPIC", "-fno-fast-math", "-ffp-contract=off", "-Wno-override-module", host, ir, runtime, "-pthread", "-lm", "-o", binary]);
    binaries.set(runtime, binary);
  }
  const results = [];
  for (const workload of workloads) {
    // An idle gap before every call makes a run long: fewer calls are enough to see the wake-up.
    const repetitions = gap > 0 ? Math.min(workload.repetitions, 300) : workload.repetitions;
    const expected = reference(workload.count, workload.rounds);
    const samples = Object.fromEntries(backends.map(([name]) => [name, []]));
    for (let sample = 0; sample < (quick ? 3 : 9); sample++) {
      const order = sample % 2 ? [...backends].reverse() : backends;
      for (const [name, runtime, kind] of order) {
        const [value, cold, warm, p50, used] = run(binaries.get(runtime), [workload.count, workload.rounds, repetitions, kind, gap].map(String)).trim().split(" ");
        const result = { value, cold_ms: Number(cold), warm_ms_per_call: Number(warm), warm_p50_ms: Number(p50), cpu_ms_per_call: Number(used) };
        assert.equal(result.value, expected);
        assert.ok([result.cold_ms, result.warm_ms_per_call, result.warm_p50_ms, result.cpu_ms_per_call].every((number) => number >= 0));
        samples[name].push(result);
      }
    }
    results.push({ ...workload, repetitions, gap_us: gap, backends: Object.fromEntries(Object.entries(samples).map(([name, samples]) => [name, {
      cold_median_ms: median(samples.map((sample) => sample.cold_ms)),
      warm_median_ms_per_call: median(samples.map((sample) => sample.warm_ms_per_call)),
      warm_p50_median_ms: median(samples.map((sample) => sample.warm_p50_ms)),
      cpu_median_ms_per_call: median(samples.map((sample) => sample.cpu_ms_per_call)),
      samples,
    }])) });
  }
  const wasmChecks = [];
  if (dataParallel) {
    const wasm = join(temporary, "parallel.wasm");
    run(compiler, ["build", join(root, "benchmarks/tasks/Main.tz"), "--target", "wasm32", "-O3", "-o", wasm]);
    const module = new WebAssembly.Module(readFileSync(wasm));
    assert.deepEqual(WebAssembly.Module.imports(module), []);
    const api = new WebAssembly.Instance(module).exports;
    for (const workload of workloads) {
      const expected = reference(workload.count, workload.rounds);
      for (const [name, , kind] of backends) {
        assert.equal(String(api[`tz_${functions[kind]}`](BigInt(workload.count), BigInt(workload.rounds))), expected);
        wasmChecks.push({ workload: workload.name, api: name, result: expected });
      }
    }
  }
  const report = {
    mode: quick ? "correctness-smoke" : "measurement", optimization: "O3 generic, no fast-math, fp-contract=off, allocator system",
    workload_family: dataParallel ? "Array versus Parallel" : sweep ? "Task granularity sweep" : "Task runtime comparison",
    gap_us: gap,
    environment: { os: platform(), release: release(), arch: arch(), cpu: cpus()[0]?.model, logical_cpus: cpus().length, clang: run(clang, ["--version"]).split("\n")[0] },
    ir_sha256: hash(ir), runtime_sha256: hash(poolRuntime), baseline_runtime_sha256: baselineRuntime ? hash(baselineRuntime) : null,
    notes: (dataParallel ? "Array/Parallel pairs use the same input and arithmetic; timings include input/output allocation, captures, work, synchronization and cleanup. Reduction is integer addition, so both orders have the same result. Cold includes lazy pool startup, but the common sequential reference is computed first. WASM is correctness-only sequential fallback. No SIMD claims or speed thresholds."
      : sweep ? "32 tasks of growing length: the pool against the sequential loop of the same IR. Warm includes allocation, dispatch, join and cleanup. No speed thresholds."
        : "Same LLVM and allocations across runtime variants. Cold includes lazy pool startup; warm includes allocation, dispatch, join and cleanup. Compilation, process startup and process-exit shutdown are outside the timing. No speed thresholds.")
      + " warm_ms_per_call is the mean of back-to-back calls (the idle gap, if any, is excluded), warm_p50_ms the median of calls timed one by one, cpu_ms_per_call the process CPU time per call.",
    results,
    wasm_sequential_checks: wasmChecks,
  };
  if (options["--metrics"]) {
    const records = results.flatMap((work) => Object.entries(work.backends).flatMap(([backend, data]) =>
      [["cold", "wall_time", "cold_ms"], ["warm", "wall_time", "warm_ms_per_call"], ["warm-p50", "wall_time", "warm_p50_ms"], ["warm", "cpu_time", "cpu_ms_per_call"]]
        .map(([kind, metric, column]) => makeRecord(context, {
          suite, workload: work.name, size: work.count, language: "tsuzuri",
          variant: `${backend}-${kind}`, opt: "O3", metric, samples: data.samples.map((sample) => sample[column]),
        }))));
    writeRecords(options["--metrics"], suite, records);
  }
  if (artifacts) {
    mkdirSync(artifacts, { recursive: true });
    writeFileSync(join(artifacts, "result.json"), JSON.stringify(report, null, 2) + "\n");
    writeFileSync(join(artifacts, "tasks.ll"), readFileSync(ir));
    if (dataParallel) run(clang, ["-S", "-O3", "-fno-fast-math", "-ffp-contract=off", "-Wno-override-module", ir, "-o", join(artifacts, "parallel.s")]);
  }
  console.log(JSON.stringify(report, null, 2));
} finally { rmSync(temporary, { recursive: true, force: true }); }
