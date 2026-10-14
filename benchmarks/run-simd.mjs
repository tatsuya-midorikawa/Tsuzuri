import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, writeFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { captureRun, makeRecord, writeRecords } from "./metrics.mjs";

const args = process.argv.slice(2);
const usage = "Usage: node benchmarks/run-simd.mjs [compiler] [--quick] [--metrics directory]";
const index = args.indexOf("--metrics");
let metrics = null;
if (index >= 0) {
  metrics = args[index + 1];
  assert.ok(metrics && !metrics.startsWith("-"), usage);
  args.splice(index, 2);
}
const quick = args.includes("--quick");
const positional = args.filter((argument) => argument !== "--quick");
assert.ok(positional.length <= 1 && positional.every((argument) => !argument.startsWith("-")) && args.filter((argument) => argument === "--quick").length <= 1, usage);
const compiler = resolve(positional[0] ?? "target/release/tsuzuri");
const context = metrics ? captureRun(metrics, { compiler, mode: quick ? "quick" : "benchmark" }) : null;
const root = mkdtempSync(join(tmpdir(), "tsuzuri-simd-benchmark-"));
function execute(program, args) {
  const result = spawnSync(program, args, { encoding: "utf8", timeout: 300_000 });
  assert.ifError(result.error);
  assert.equal(result.status, 0, `${program} ${args.join(" ")}\n${result.stdout}\n${result.stderr}`);
  return result.stdout;
}
try {
  const object = join(root, "simd.o");
  execute(compiler, ["build", "benchmarks/simd/Main.tz", "--emit", "object", "-O3", "-o", object]);
  const host = join(root, "host.c");
  writeFileSync(host, `#define _POSIX_C_SOURCE 200809L
#include <assert.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <time.h>
extern int64_t tz_scalar_sum(const int64_t *, int64_t);
extern int64_t tz_vector_sum(const int64_t *, int64_t);
static int64_t c_sum(const int64_t *values, int64_t count) { uint64_t total = 0; for (int64_t index = 0; index < count; ++index) total += (uint64_t)values[index]; return (int64_t)total; }
static double now(void) { struct timespec time; assert(clock_gettime(CLOCK_MONOTONIC, &time) == 0); return (double)time.tv_sec + (double)time.tv_nsec * 1e-9; }
int main(void) {
  const int64_t count = ${quick ? 4097 : 1048577};
  const int repeats = ${quick ? 10 : 200};
  int64_t *values = malloc((size_t)count * sizeof(int64_t)); assert(values);
  for (int64_t index = 0; index < count; ++index) values[index] = (index * 37) % 1009 - 504;
  int64_t (*functions[])(const int64_t *, int64_t) = {tz_scalar_sum, tz_vector_sum, c_sum};
  const char *names[] = {"Tsuzuri scalar", "Tsuzuri SIMD", "C"};
  int64_t expected = c_sum(values, count);
  for (int implementation = 0; implementation < 3; ++implementation) {
    assert(functions[implementation](values, count) == expected);
    double samples[${metrics ? 9 : 5}];
    for (int sample = 0; sample < ${metrics ? 9 : 5}; ++sample) {
      double start = now();
      for (int run = 0; run < repeats; ++run) assert(functions[implementation](values, count) == expected);
      samples[sample] = (now() - start) * 1000.0;
    }
#ifdef METRICS
    printf("metrics:{\\"implementation\\":%d,\\"count\\":%lld,\\"raw_ms\\":[",implementation,(long long)count);
    for(int sample=0;sample<9;sample++) printf("%s%.9f",sample?",":"",samples[sample]/repeats);
    puts("]}");
#endif
    for (int outer = 1; outer < ${metrics ? 9 : 5}; ++outer) for (int inner = outer; inner > 0 && samples[inner] < samples[inner - 1]; --inner) { double value = samples[inner]; samples[inner] = samples[inner - 1]; samples[inner - 1] = value; }
    printf("%s: %.6f ms, count=%lld repeats=%d checksum=%lld\\n", names[implementation], samples[${metrics ? 4 : 2}], (long long)count, repeats, (long long)expected);
  }
  free(values); return 0;
}
`);
  const executable = join(root, "benchmark");
  execute(process.env.TSUZURI_CLANG ?? "clang", ["-O3", "-std=c11", "-ffp-contract=off", "-fno-lto", ...(metrics ? ["-DMETRICS"] : []), host, object, "-lm", "-o", executable]);
  const lines = execute(executable, []).trim().split("\n");
  if (metrics) {
    const measured = lines.filter((line) => line.startsWith("metrics:")).map((line) => JSON.parse(line.slice(8)));
    assert.equal(measured.length, 3);
    writeRecords(metrics, "simd", measured.map((result) => makeRecord(context, { suite: "simd", workload: "sum", size: result.count,
      language: result.implementation === 2 ? "c" : "tsuzuri", variant: ["scalar", "vector", "default"][result.implementation],
      opt: "O3", metric: "wall_time", samples: result.raw_ms })));
  }
  console.log(lines.filter((line) => !line.startsWith("metrics:")).join("\n"));
  if (quick) console.log("quick validation only; these times are not performance evidence");
} finally { rmSync(root, { recursive: true, force: true }); }
