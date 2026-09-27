import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { arch, cpus, platform, release, tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const usage = "Usage: node benchmarks/run-tasks.mjs [compiler] [--quick] [--data-parallel] [--baseline-runtime object] [--artifacts directory]";
const options = {}, positional = [];
let quick = false;
let dataParallel = false;
const args = process.argv.slice(2);
for (let index = 0; index < args.length; index++) {
  const argument = args[index];
  if (argument === "--quick" && !quick) quick = true;
  else if (argument === "--data-parallel" && !dataParallel) dataParallel = true;
  else if (["--baseline-runtime", "--artifacts"].includes(argument) && options[argument] === undefined) {
    const value = args[++index];
    if (value === undefined || value.startsWith("--")) throw new Error(usage);
    options[argument] = resolve(value);
  } else if (!argument.startsWith("-")) positional.push(argument);
  else throw new Error(usage);
}
if (positional.length > 1 || dataParallel && options["--baseline-runtime"]) throw new Error(usage);
const compiler = resolve(positional[0] ?? join(root, "target/release/tsuzuri"));
const clang = process.env.TSUZURI_CLANG ?? "clang";
const run = (program, arguments_) => execFileSync(program, arguments_, { cwd: root, encoding: "utf8", timeout: 300_000, stdio: ["ignore", "pipe", "pipe"] });
const median = (values) => { const sorted = [...values].sort((left, right) => left - right); return sorted[Math.floor(sorted.length / 2)]; };
const hash = (path) => createHash("sha256").update(readFileSync(path)).digest("hex");
const temporary = mkdtempSync(join(tmpdir(), "tsuzuri-task-benchmark-"));
const artifacts = options["--artifacts"];
const workloads = dataParallel ? [
  { name: "small", count: 64, rounds: 8, repetitions: quick ? 2 : 128 },
  { name: "chunk-boundary", count: 8193, rounds: 16, repetitions: quick ? 2 : 32 },
  { name: "bulk", count: quick ? 16384 : 262144, rounds: 32, repetitions: quick ? 2 : 8 },
] : [
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
try {
  const ir = join(temporary, "tasks.ll"), host = join(temporary, "host.c");
  run(compiler, ["build", join(root, "benchmarks/tasks/Main.tz"), "--emit", "llvm", "-o", ir]);
  const functions = ["parallel_batch", "data_parallel_batch", "sequential_batch", "data_map_batch", "sequential_map_batch", "data_reduce_batch", "sequential_reduce_batch"];
  writeFileSync(host, `#include <assert.h>\n#include <inttypes.h>\n#include <stdint.h>\n#include <stdio.h>\n#include <stdlib.h>\n#include <time.h>\n${functions.map((name) => `extern int64_t tz_${name}(int64_t,int64_t);`).join("\n")}\nstatic double now(void) { struct timespec time; assert(clock_gettime(CLOCK_MONOTONIC,&time)==0); return time.tv_sec * 1000.0 + time.tv_nsec / 1000000.0; }\nint main(int argc,char **argv) { if(argc!=5) return 2; int64_t count=strtoll(argv[1],0,10),rounds=strtoll(argv[2],0,10),repetitions=strtoll(argv[3],0,10),kind=strtoll(argv[4],0,10); assert(count>=0 && rounds>=0 && repetitions>0 && kind>=0 && kind<7); int64_t (*batches[])(int64_t,int64_t)={${functions.map((name) => `tz_${name}`).join(",")}}; int64_t (*batch)(int64_t,int64_t)=batches[kind]; int64_t expected=tz_sequential_batch(count,rounds); double start=now(); int64_t result=batch(count,rounds); double cold=now()-start; assert(result==expected); start=now(); for(int64_t iteration=0;iteration<repetitions;iteration++) assert(batch(count,rounds)==expected); double warm=(now()-start)/repetitions; printf("{\\\"value\\\":\\\"%" PRId64 "\\\",\\\"cold_ms\\\":%.6f,\\\"warm_ms_per_call\\\":%.6f}\\n",result,cold,warm); return 0; }\n`);
  const backends = dataParallel ? ["parallel-init", "array-init", "parallel-map", "array-map", "parallel-sum", "array-sum"].map((name, index) => [name, join(root, "src/runtime/task.c"), index + 1]) : [["pool", join(root, "src/runtime/task.c"), 0]];
  if (options["--baseline-runtime"]) backends.push(["baseline", options["--baseline-runtime"]]);
  const binaries = new Map();
  for (const [name, runtime] of backends) {
    const binary = join(temporary, name);
    run(clang, ["-std=c11", "-O3", "-fPIC", "-fno-fast-math", "-ffp-contract=off", "-Wno-override-module", host, ir, runtime, "-pthread", "-lm", "-o", binary]);
    binaries.set(name, binary);
  }
  const results = [];
  for (const workload of workloads) {
    const samples = Object.fromEntries(backends.map(([name]) => [name, []]));
    for (let sample = 0; sample < (quick ? 3 : 7); sample++) {
      const order = sample % 2 ? [...backends].reverse() : backends;
      for (const [name, , kind = 0] of order) {
        const result = JSON.parse(run(binaries.get(name), [workload.count, workload.rounds, workload.repetitions, kind].map(String)));
        assert.equal(result.value, reference(workload.count, workload.rounds));
        assert.ok(result.cold_ms >= 0 && result.warm_ms_per_call >= 0);
        samples[name].push(result);
      }
    }
    results.push({ ...workload, backends: Object.fromEntries(Object.entries(samples).map(([name, samples]) => [name, {
      cold_median_ms: median(samples.map((sample) => sample.cold_ms)),
      warm_median_ms_per_call: median(samples.map((sample) => sample.warm_ms_per_call)),
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
    mode: quick ? "correctness-smoke" : "measurement", optimization: "O3 generic, no fast-math, fp-contract=off",
    workload_family: dataParallel ? "Array versus Parallel" : "Task runtime comparison",
    environment: { os: platform(), release: release(), arch: arch(), cpu: cpus()[0]?.model, logical_cpus: cpus().length, clang: run(clang, ["--version"]).split("\n")[0] },
    ir_sha256: hash(ir), runtime_sha256: hash(join(root, "src/runtime/task.c")), baseline_runtime_sha256: options["--baseline-runtime"] ? hash(options["--baseline-runtime"]) : null,
    notes: dataParallel ? "Array/Parallel pairs use the same input and arithmetic; timings include input/output allocation, captures, work, synchronization and cleanup. Reduction is integer addition, so both orders have the same result. Cold includes lazy pool startup, but the common sequential reference is computed first. WASM is correctness-only sequential fallback. No SIMD claims or speed thresholds." : "Same LLVM and allocations across runtime variants. Cold includes lazy pool startup; warm includes allocation, dispatch, join and cleanup. Compilation, process startup and process-exit shutdown are outside the timing. No speed thresholds.", results,
    wasm_sequential_checks: wasmChecks,
  };
  if (artifacts) {
    mkdirSync(artifacts, { recursive: true });
    writeFileSync(join(artifacts, "result.json"), JSON.stringify(report, null, 2) + "\n");
    writeFileSync(join(artifacts, "tasks.ll"), readFileSync(ir));
    if (dataParallel) run(clang, ["-S", "-O3", "-fno-fast-math", "-ffp-contract=off", "-Wno-override-module", ir, "-o", join(artifacts, "parallel.s")]);
  }
  console.log(JSON.stringify(report, null, 2));
} finally { rmSync(temporary, { recursive: true, force: true }); }