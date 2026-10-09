import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, writeFileSync, rmSync } from "node:fs";
import { arch, cpus, platform, tmpdir } from "node:os";
import { join, resolve } from "node:path";

// Matched comparison of Matrix.mul with plain C (C11, -O3, -ffp-contract=off). Every implementation
// builds the same n x n inputs, performs the same separately rounded k-ascending sum per output
// element, and returns the same checksum; the checksums are compared bit for bit before any timing.
// Times are medians of 9 samples (min and max are printed too). There are no pass/fail speed thresholds.
const compiler = resolve(process.argv[2] ?? "target/release/tsuzuri");
const quick = process.argv.includes("--quick");
const clang = process.env.TSUZURI_CLANG ?? "clang";
const sizes = quick ? [16] : [64, 128, 256, 512];
const samples = 9;
const root = mkdtempSync(join(tmpdir(), "tsuzuri-matrix-benchmark-"));

function execute(program, args) {
  const result = spawnSync(program, args, { encoding: "utf8", timeout: 600_000 });
  assert.ifError(result.error);
  assert.equal(result.status, 0, `${program} ${args.join(" ")}\n${result.stdout}\n${result.stderr}`);
  return result.stdout;
}

const implementations = [
  { label: "Tsuzuri Matrix.mul", symbol: "tz_matmul_checksum" },
  { label: "C i-j-k", symbol: "c_ijk" },
  { label: "C i-k-j", symbol: "c_ikj" },
];

try {
  const object = join(root, "matrix.o");
  execute(compiler, ["build", "benchmarks/matrix", "--emit", "object", "-O3", "-o", object]);
  const host = join(root, "host.c");
  writeFileSync(host, `#define _POSIX_C_SOURCE 200809L
#include <assert.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
extern double tz_matmul_checksum(int64_t, int64_t);
static double left_value(int64_t i, int64_t j) { return (double)((i * 7 + j * 3) % 13) * 0.25 - 1.5; }
static double right_value(int64_t i, int64_t j) { return (double)((i * 5 + j * 11) % 17) * 0.125 - 1.0; }
static double checksum(const double *values, int64_t count) { double total = 0.0; for (int64_t index = 0; index < count; ++index) total = total * 0.999 + values[index]; return total; }
static void inputs(int64_t n, double **left, double **right, double **out) {
  *left = malloc((size_t)(n * n) * sizeof(double)); *right = malloc((size_t)(n * n) * sizeof(double)); *out = malloc((size_t)(n * n) * sizeof(double));
  assert(*left && *right && *out);
  for (int64_t i = 0; i < n; ++i) for (int64_t j = 0; j < n; ++j) { (*left)[i * n + j] = left_value(i, j); (*right)[i * n + j] = right_value(i, j); }
}
static double c_ijk(int64_t n, int64_t repeats) {
  double *a, *b, *c; inputs(n, &a, &b, &c); double sum = 0.0;
  for (int64_t r = 0; r < repeats; ++r) {
    for (int64_t i = 0; i < n; ++i) for (int64_t j = 0; j < n; ++j) { double total = 0.0; for (int64_t k = 0; k < n; ++k) total = total + a[i * n + k] * b[k * n + j]; c[i * n + j] = total; }
    sum = sum + checksum(c, n * n);
  }
  free(a); free(b); free(c); return sum;
}
static double c_ikj(int64_t n, int64_t repeats) {
  double *a, *b, *c; inputs(n, &a, &b, &c); double sum = 0.0;
  for (int64_t r = 0; r < repeats; ++r) {
    memset(c, 0, (size_t)(n * n) * sizeof(double));
    for (int64_t i = 0; i < n; ++i) for (int64_t k = 0; k < n; ++k) { double x = a[i * n + k]; for (int64_t j = 0; j < n; ++j) c[i * n + j] = c[i * n + j] + x * b[k * n + j]; }
    sum = sum + checksum(c, n * n);
  }
  free(a); free(b); free(c); return sum;
}
static double now(void) { struct timespec time; assert(clock_gettime(CLOCK_MONOTONIC, &time) == 0); return (double)time.tv_sec + (double)time.tv_nsec * 1e-9; }
typedef double (*function_t)(int64_t, int64_t);
static int compare(const void *x, const void *y) { double a = *(const double *)x, b = *(const double *)y; return (a > b) - (a < b); }
int main(void) {
  const int64_t sizes[] = {${sizes.join(", ")}}; const char *labels[] = {${implementations.map(({ label }) => JSON.stringify(label)).join(", ")}};
  function_t functions[] = {${implementations.map(({ symbol }) => symbol).join(", ")}};
  const int count = ${implementations.length}, samples = ${samples};
  for (size_t s = 0; s < sizeof(sizes) / sizeof(sizes[0]); ++s) {
    const int64_t n = sizes[s];
    const int64_t repeats = ${quick ? "3" : "n <= 64 ? 200 : n <= 128 ? 40 : n <= 256 ? 6 : 2"};
    const double reference = c_ijk(n, 1);
    for (int f = 0; f < count; ++f) { double got = functions[f](n, 1); if (memcmp(&got, &reference, sizeof got) != 0) { printf("checksum mismatch: %s n=%lld %.17g vs %.17g\\n", labels[f], (long long)n, got, reference); return 1; } }
    double times[16][${samples}];
    for (int sample = 0; sample < samples; ++sample) for (int offset = 0; offset < count; ++offset) {
      int f = (sample + offset) % count; double start = now(); volatile double sink = functions[f](n, repeats); (void)sink;
      times[f][sample] = (now() - start) * 1000.0 / (double)repeats;
    }
    for (int f = 0; f < count; ++f) {
      qsort(times[f], (size_t)samples, sizeof(double), compare);
      double flops = 2.0 * (double)n * (double)n * (double)n;
      printf("n=%-4lld %-20s median %10.4f ms  min %10.4f  max %10.4f  %7.3f GFLOP/s (median)  repeats=%lld checksum=%.17g\\n", (long long)n, labels[f], times[f][samples / 2], times[f][0], times[f][samples - 1], flops / (times[f][samples / 2] * 1e-3) / 1e9, (long long)repeats, reference);
    }
  }
  return 0;
}
`);
  const executable = join(root, "benchmark");
  execute(clang, ["-O3", "-std=c11", "-ffp-contract=off", "-fno-lto", host, object, "-lm", "-o", executable]);
  console.log(`environment: ${platform()} ${arch()} ${cpus()[0]?.model ?? "unknown cpu"} x${cpus().length}, node ${process.version}, ${execute(clang, ["--version"]).split("\n")[0]}`);
  console.log(execute(executable, []).trim());
  if (quick) console.log("quick validation only; these times are not performance evidence");
} finally { rmSync(root, { recursive: true, force: true }); }
