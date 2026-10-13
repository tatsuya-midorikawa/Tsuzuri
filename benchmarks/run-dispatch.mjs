import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, writeFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { captureRun, makeRecord, writeRecords } from "./metrics.mjs";

const args = process.argv.slice(2);
const usage = "Usage: node benchmarks/run-dispatch.mjs [compiler] [--quick] [--metrics directory]";
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
const root = mkdtempSync(join(tmpdir(), "tsuzuri-dispatch-benchmark-"));
function execute(program, args) {
  const result = spawnSync(program, args, { encoding: "utf8", timeout: 300_000 });
  assert.ifError(result.error);
  assert.equal(result.status, 0, `${program} ${args.join(" ")}\n${result.stdout}\n${result.stderr}`);
  return result.stdout;
}
try {
  const object = join(root, "dispatch.o");
  execute(compiler, ["build", "benchmarks/dispatch", "--emit", "object", "-O3", "-o", object]);
  const host = join(root, "host.c");
  writeFileSync(host, `#define _POSIX_C_SOURCE 200809L
#include <assert.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
extern int64_t tz_dispatched_sum(const int64_t *, int64_t);
extern int64_t tz_scalar_sum(const int64_t *, int64_t);
extern uint64_t tsuzuri_cpu_features(void);
extern int tsuzuri_cpu_variant(void);
static int64_t c_sum(const int64_t *values, int64_t count) { uint64_t total = 0; for (int64_t index = 0; index < count; ++index) total += (uint64_t)values[index]; int64_t result; memcpy(&result, &total, 8); return result; }
static double now(void) { struct timespec time; assert(clock_gettime(CLOCK_MONOTONIC, &time) == 0); return (double)time.tv_sec + (double)time.tv_nsec * 1e-9; }
int main(void) {
  const int64_t count = ${quick ? 4097 : 1048577}; const int repeats = ${quick ? 10 : 200};
  int64_t *values = malloc((size_t)count * 8); assert(values);
  for (int64_t index = 0; index < count; ++index) values[index] = index * 37 % 1009 - 504;
  int64_t (*functions[])(const int64_t *, int64_t) = {tz_dispatched_sum, tz_scalar_sum, c_sum};
  const char *names[] = {"Tsuzuri dispatch", "Tsuzuri scalar", "C"};
  int64_t expected = c_sum(values, count); double samples[3][9];
  for (int index = 0; index < 3; ++index) assert(functions[index](values, count) == expected);
  for (int sample = 0; sample < 9; ++sample) for (int offset = 0; offset < 3; ++offset) {
    int implementation = (sample + offset) % 3; double start = now();
    for (int run = 0; run < repeats; ++run) assert(functions[implementation](values, count) == expected);
    samples[implementation][sample] = (now() - start) * 1000.0;
  }
  for (int implementation = 0; implementation < 3; ++implementation) {
#ifdef METRICS
    double raw[9]; memcpy(raw,samples[implementation],sizeof(raw));
#endif
    for (int outer = 1; outer < 9; ++outer) for (int inner = outer; inner > 0 && samples[implementation][inner] < samples[implementation][inner - 1]; --inner) { double value = samples[implementation][inner]; samples[implementation][inner] = samples[implementation][inner - 1]; samples[implementation][inner - 1] = value; }
    printf("{\\"implementation\\":\\"%s\\",\\"milliseconds\\":%.6f,\\"features\\":%llu,\\"variant\\":%d,\\"count\\":%lld,\\"repeats\\":%d,\\"checksum\\":%lld", names[implementation], samples[implementation][4], (unsigned long long)tsuzuri_cpu_features(), tsuzuri_cpu_variant(), (long long)count, repeats, (long long)expected);
#ifdef METRICS
    fputs(",\\"raw_ms\\":[",stdout); for(int sample=0;sample<9;sample++) printf("%s%.9f",sample?",":"",raw[sample]/repeats); fputs("]",stdout);
#endif
    puts("}");
  }
  free(values); return 0;
}
`);
  const executable = join(root, "benchmark");
  execute(process.env.TSUZURI_CLANG ?? "clang", ["-O3", "-std=c11", "-fno-lto", ...(metrics ? ["-DMETRICS"] : []), host, object, "-lm", "-o", executable]);
  const results = execute(executable, []).trim().split("\n").map(JSON.parse);
  if (metrics) writeRecords(metrics, "dispatch", results.map((result, index) => makeRecord(context, {
    suite: "dispatch", workload: "sum", size: result.count, language: index === 2 ? "c" : "tsuzuri",
    variant: ["dispatch", "scalar", "default"][index], opt: "O3", metric: "wall_time", samples: result.raw_ms,
  })));
  for (const { raw_ms, ...result } of results) console.log(JSON.stringify({ ...result, quick }));
} finally { rmSync(root, { recursive: true, force: true }); }
