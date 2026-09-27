import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, writeFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

const compiler = resolve(process.argv[2] ?? "target/release/tsuzuri");
const quick = process.argv.includes("--quick");
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
    double samples[5];
    for (int sample = 0; sample < 5; ++sample) {
      double start = now();
      for (int run = 0; run < repeats; ++run) assert(functions[implementation](values, count) == expected);
      samples[sample] = (now() - start) * 1000.0;
    }
    for (int outer = 1; outer < 5; ++outer) for (int inner = outer; inner > 0 && samples[inner] < samples[inner - 1]; --inner) { double value = samples[inner]; samples[inner] = samples[inner - 1]; samples[inner - 1] = value; }
    printf("%s: %.6f ms, count=%lld repeats=%d checksum=%lld\\n", names[implementation], samples[2], (long long)count, repeats, (long long)expected);
  }
  free(values); return 0;
}
`);
  const executable = join(root, "benchmark");
  execute(process.env.TSUZURI_CLANG ?? "clang", ["-O3", "-std=c11", "-ffp-contract=off", "-fno-lto", host, object, "-lm", "-o", executable]);
  console.log(execute(executable, []).trim());
  if (quick) console.log("quick validation only; these times are not performance evidence");
} finally { rmSync(root, { recursive: true, force: true }); }
