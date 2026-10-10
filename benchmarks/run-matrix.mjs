import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, writeFileSync, rmSync } from "node:fs";
import { arch, cpus, loadavg, platform, tmpdir } from "node:os";
import { join, resolve } from "node:path";

// Matched comparison of Matrix.mul and its siblings with plain C (C11, -O3, -ffp-contract=off). Every
// implementation builds the same n x n inputs, performs the same per-element order of operations (the
// separately rounded k-ascending sum for mul and mul_parallel, the fma(a, b, total) sum for mul_fma and
// mul_fma_parallel) and returns the same checksum; the checksums are compared bit for bit before any timing.
// Times are medians of 9 samples (min and max are printed too). There are no pass/fail speed thresholds.
// The same program is then built for wasm32 (default and simd128) and run in Node, after its checksums are
// compared with the C references.
//
// Math.fma is the hardware instruction only when the compiler itself is built for AArch64 (src/llvm_math.rs);
// elsewhere, and for wasm32 always, it is the software routine tz_soft_fma, about a microsecond per multiply-add,
// so one 512 x 512 product of mul_fma takes minutes. The results are bit-identical either way. When the emitted
// IR calls tz_soft_fma, the Tsuzuri fused kernels are timed only up to n = 64 with one product per sample;
// `--fused-all` times every size (hours in total for the native sizes).
const root = resolve(import.meta.dirname, "..");
const compiler = resolve(process.argv[2] ?? "target/release/tsuzuri");
const quick = process.argv.includes("--quick");
const fusedAll = process.argv.includes("--fused-all");
const clang = process.env.TSUZURI_CLANG ?? "clang";
const sizes = quick ? [16] : [64, 128, 256, 512];
const samples = 9;
const softFusedLimit = 64;
const temporary = mkdtempSync(join(tmpdir(), "tsuzuri-matrix-benchmark-"));

function execute(program, args, timeout = 900_000) {
  const result = spawnSync(program, args, { encoding: "utf8", timeout });
  assert.ifError(result.error);
  assert.equal(result.status, 0, `${program} ${args.join(" ")}\n${result.stdout}\n${result.stderr}`);
  return result.stdout;
}

// Each family has one reference implementation; every other member must return the same bits.
const families = [
  { name: "f64", reference: "c_ijk", members: [
    ["Tsuzuri Matrix.mul", "tz_mul"], ["Tsuzuri i-j-k loop (Phase 1 order)", "tz_textbook"], ["Tsuzuri Matrix.mul_parallel", "tz_mul_parallel"],
    ["C i-j-k", "c_ijk"], ["C i-k-j", "c_ikj"], ["C i-k-j, pthreads", "c_ikj_threads"]] },
  { name: "f64 fused", reference: "c_fma_ikj", members: [
    ["Tsuzuri Matrix.mul_fma", "tz_mul_fma"], ["Tsuzuri Matrix.mul_fma_parallel", "tz_mul_fma_parallel"], ["C fma() i-k-j", "c_fma_ikj"]] },
  { name: "f32", reference: "c_ijk32", members: [
    ["Tsuzuri Matrix.mul", "tz_mul32"], ["Tsuzuri Matrix.mul_parallel", "tz_mul_parallel32"], ["C i-j-k", "c_ijk32"], ["C i-k-j", "c_ikj32"]] },
  { name: "i64", reference: "c_ijk_int", members: [
    ["Tsuzuri Matrix.mul", "tz_mul_int"], ["Tsuzuri Matrix.mul_parallel", "tz_mul_parallel_int"], ["C i-j-k", "c_ijk_int"], ["C i-k-j", "c_ikj_int"]] },
];

// The plain C loops for one element type: the inputs, the checksum, the i-j-k and the i-k-j products.
const plain = ({ suffix, type, left, right, accumulator, start, step, finish }) => `
static ${type} left_${suffix}(int64_t i, int64_t j) { return ${left}; }
static ${type} right_${suffix}(int64_t i, int64_t j) { return ${right}; }
static void inputs_${suffix}(int64_t n, ${type} **a, ${type} **b, ${type} **c) {
  *a = malloc((size_t)(n * n) * sizeof(${type})); *b = malloc((size_t)(n * n) * sizeof(${type})); *c = malloc((size_t)(n * n) * sizeof(${type}));
  assert(*a && *b && *c);
  for (int64_t i = 0; i < n; ++i) for (int64_t j = 0; j < n; ++j) { (*a)[i * n + j] = left_${suffix}(i, j); (*b)[i * n + j] = right_${suffix}(i, j); }
}
static ${accumulator} checksum_${suffix}(const ${type} *values, int64_t count) { ${accumulator} total = ${start}; for (int64_t index = 0; index < count; ++index) total = ${step}; return total; }
static uint64_t c_ijk${suffix}(int64_t n, int64_t repeats) {
  ${type} *a, *b, *c; inputs_${suffix}(n, &a, &b, &c); ${accumulator} sum = 0;
  for (int64_t r = 0; r < repeats; ++r) {
    for (int64_t i = 0; i < n; ++i) for (int64_t j = 0; j < n; ++j) { ${type} total = 0; for (int64_t k = 0; k < n; ++k) total = total + a[i * n + k] * b[k * n + j]; c[i * n + j] = total; }
    sum = sum + checksum_${suffix}(c, n * n);
  }
  free(a); free(b); free(c); return ${finish};
}
static uint64_t c_ikj${suffix}(int64_t n, int64_t repeats) {
  ${type} *a, *b, *c; inputs_${suffix}(n, &a, &b, &c); ${accumulator} sum = 0;
  for (int64_t r = 0; r < repeats; ++r) {
    memset(c, 0, (size_t)(n * n) * sizeof(${type}));
    for (int64_t i = 0; i < n; ++i) for (int64_t k = 0; k < n; ++k) { ${type} x = a[i * n + k]; for (int64_t j = 0; j < n; ++j) c[i * n + j] = c[i * n + j] + x * b[k * n + j]; }
    sum = sum + checksum_${suffix}(c, n * n);
  }
  free(a); free(b); free(c); return ${finish};
}`;

// The kernels that call Math.fma.
const fusedKernels = new Set(["tz_mul_fma", "tz_mul_fma_parallel"]);
const functions = families.flatMap(({ name, members }) => members.map(([label, symbol]) => ({ family: name, label, symbol, fused: fusedKernels.has(symbol) })));

try {
  const object = join(temporary, "matrix.o");
  execute(compiler, ["build", join(root, "benchmarks/matrix"), "--emit", "object", "-O3", "-o", object]);
  // The generated code decides: native IR calls tz_soft_fma wherever this compiler does not lower Math.fma to llvm.fma.
  const ir = join(temporary, "matrix.ll");
  execute(compiler, ["build", join(root, "benchmarks/matrix"), "--emit", "llvm", "-O3", "-o", ir]);
  const softFma = readFileSync(ir, "utf8").includes("call void @tz_soft_fma(");
  // Per function: 1 when it is timed with one product per sample, and the largest size at which it is timed.
  const slow = functions.map(({ fused }) => (fused && softFma ? 1 : 0));
  const limits = functions.map(({ fused }) => (fused && softFma && !fusedAll ? softFusedLimit : "INT64_MAX"));
  const host = join(temporary, "host.c");
  writeFileSync(host, `#define _GNU_SOURCE
#define _DARWIN_C_SOURCE 1
#include <assert.h>
#include <math.h>
#include <pthread.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <unistd.h>
extern double tz_matmul_checksum(int64_t, int64_t, int64_t);
extern double tz_matmul_checksum32(int64_t, int64_t, int64_t);
extern int64_t tz_matmul_checksum_int(int64_t, int64_t, int64_t);
static uint64_t bits(double value) { uint64_t out; memcpy(&out, &value, sizeof out); return out; }
static uint64_t tz_mul(int64_t n, int64_t r) { return bits(tz_matmul_checksum(n, r, 0)); }
static uint64_t tz_mul_parallel(int64_t n, int64_t r) { return bits(tz_matmul_checksum(n, r, 1)); }
static uint64_t tz_mul_fma(int64_t n, int64_t r) { return bits(tz_matmul_checksum(n, r, 2)); }
static uint64_t tz_mul_fma_parallel(int64_t n, int64_t r) { return bits(tz_matmul_checksum(n, r, 3)); }
static uint64_t tz_textbook(int64_t n, int64_t r) { return bits(tz_matmul_checksum(n, r, 4)); }
static uint64_t tz_mul32(int64_t n, int64_t r) { return bits(tz_matmul_checksum32(n, r, 0)); }
static uint64_t tz_mul_parallel32(int64_t n, int64_t r) { return bits(tz_matmul_checksum32(n, r, 1)); }
static uint64_t tz_mul_int(int64_t n, int64_t r) { return (uint64_t)tz_matmul_checksum_int(n, r, 0); }
static uint64_t tz_mul_parallel_int(int64_t n, int64_t r) { return (uint64_t)tz_matmul_checksum_int(n, r, 1); }
${plain({ suffix: "", type: "double", left: "(double)((i * 7 + j * 3) % 13) * 0.25 - 1.5", right: "(double)((i * 5 + j * 11) % 17) * 0.125 - 1.0", accumulator: "double", start: "0.0", step: "total * 0.999 + values[index]", finish: "bits(sum)" })}
${plain({ suffix: "32", type: "float", left: "(float)((double)((i * 7 + j * 3) % 13) * 0.25 - 1.5)", right: "(float)((double)((i * 5 + j * 11) % 17) * 0.125 - 1.0)", accumulator: "double", start: "0.0", step: "total * 0.999 + (double)values[index]", finish: "bits(sum)" })}
${plain({ suffix: "_int", type: "int64_t", left: "(i * 7 + j * 3) % 13 - 6", right: "(i * 5 + j * 11) % 17 - 8", accumulator: "uint64_t", start: "7", step: "total * 31u + (uint64_t)values[index]", finish: "sum" })}
static uint64_t c_fma_ikj(int64_t n, int64_t repeats) {
  double *a, *b, *c; inputs_(n, &a, &b, &c); double sum = 0.0;
  for (int64_t r = 0; r < repeats; ++r) {
    memset(c, 0, (size_t)(n * n) * sizeof(double));
    for (int64_t i = 0; i < n; ++i) for (int64_t k = 0; k < n; ++k) { double x = a[i * n + k]; for (int64_t j = 0; j < n; ++j) c[i * n + j] = fma(x, b[k * n + j], c[i * n + j]); }
    sum = sum + checksum_(c, n * n);
  }
  free(a); free(b); free(c); return bits(sum);
}
struct rows_job { const double *a, *b; double *c; int64_t n, first, last; };
static void *ikj_rows(void *argument) {
  struct rows_job *job = argument; const int64_t n = job->n;
  for (int64_t i = job->first; i < job->last; ++i) for (int64_t k = 0; k < n; ++k) { double x = job->a[i * n + k]; for (int64_t j = 0; j < n; ++j) job->c[i * n + j] = job->c[i * n + j] + x * job->b[k * n + j]; }
  return NULL;
}
static uint64_t c_ikj_threads(int64_t n, int64_t repeats) {
  double *a, *b, *c; inputs_(n, &a, &b, &c); double sum = 0.0;
  long online = sysconf(_SC_NPROCESSORS_ONLN); const int64_t threads = online < 1 ? 1 : online < n ? online : n;
  pthread_t ids[256]; struct rows_job jobs[256]; assert(threads <= 256);
  for (int64_t r = 0; r < repeats; ++r) {
    memset(c, 0, (size_t)(n * n) * sizeof(double));
    for (int64_t t = 0; t < threads; ++t) { jobs[t] = (struct rows_job){ a, b, c, n, n * t / threads, n * (t + 1) / threads }; assert(pthread_create(&ids[t], NULL, ikj_rows, &jobs[t]) == 0); }
    for (int64_t t = 0; t < threads; ++t) assert(pthread_join(ids[t], NULL) == 0);
    sum = sum + checksum_(c, n * n);
  }
  free(a); free(b); free(c); return bits(sum);
}
static double now(void) { struct timespec time; assert(clock_gettime(CLOCK_MONOTONIC, &time) == 0); return (double)time.tv_sec + (double)time.tv_nsec * 1e-9; }
typedef uint64_t (*function_t)(int64_t, int64_t);
static int compare(const void *x, const void *y) { double a = *(const double *)x, b = *(const double *)y; return (a > b) - (a < b); }
int main(void) {
  const int64_t sizes[] = {${sizes.join(", ")}};
  const char *labels[] = {${functions.map(({ label }) => JSON.stringify(label)).join(", ")}};
  const char *family_names[] = {${functions.map(({ family }) => JSON.stringify(family)).join(", ")}};
  function_t functions[] = {${functions.map(({ symbol }) => symbol).join(", ")}};
  const int references[] = {${functions.map(({ family }) => functions.findIndex((candidate) => candidate.family === family && candidate.symbol === families.find(({ name }) => name === family).reference)).join(", ")}};
  const int count = ${functions.length}, samples = ${samples};
  const int slow[] = {${slow.join(", ")}};
  const int64_t limits[] = {${limits.join(", ")}};
  for (size_t s = 0; s < sizeof(sizes) / sizeof(sizes[0]); ++s) {
    const int64_t n = sizes[s];
    const int64_t repeats = ${quick ? "3" : "n <= 64 ? 200 : n <= 128 ? 40 : n <= 256 ? 6 : 2"};
    for (int f = 0; f < count; ++f) {
      if (n > limits[f]) continue;
      uint64_t got = functions[f](n, 1), want = functions[references[f]](n, 1);
      if (got != want) { printf("checksum mismatch: %s %s n=%lld %016llx vs %016llx\\n", family_names[f], labels[f], (long long)n, (unsigned long long)got, (unsigned long long)want); return 1; }
    }
    printf("reference n=%lld f64=%016llx fused=%016llx\\n", (long long)n, (unsigned long long)c_ijk(n, 1), (unsigned long long)c_fma_ikj(n, 1));
    double times[${functions.length}][${samples}];
    for (int sample = 0; sample < samples; ++sample) for (int offset = 0; offset < count; ++offset) {
      int f = (sample + offset) % count;
      if (n > limits[f]) continue;
      const int64_t each = slow[f] ? 1 : repeats;
      double start = now(); volatile uint64_t sink = functions[f](n, each); (void)sink;
      times[f][sample] = (now() - start) * 1000.0 / (double)each;
    }
    for (int f = 0; f < count; ++f) {
      if (n > limits[f]) { printf("n=%-4lld %-10s %-36s not timed: software fma (--fused-all times it)\\n", (long long)n, family_names[f], labels[f]); continue; }
      qsort(times[f], (size_t)samples, sizeof(double), compare);
      double flops = 2.0 * (double)n * (double)n * (double)n;
      printf("n=%-4lld %-10s %-36s median %10.4f ms  min %10.4f  max %10.4f  %7.3f GFLOP/s (median)  repeats=%lld checksum=%016llx\\n", (long long)n, family_names[f], labels[f], times[f][samples / 2], times[f][0], times[f][samples - 1], flops / (times[f][samples / 2] * 1e-3) / 1e9, (long long)(slow[f] ? 1 : repeats), (unsigned long long)functions[references[f]](n, 1));
    }
  }
  return 0;
}
`);
  const executable = join(temporary, "benchmark");
  execute(clang, ["-O3", "-std=c11", "-ffp-contract=off", "-fno-lto", "-Wno-override-module", host, object, join(root, "src/runtime/task.c"), "-pthread", "-lm", "-o", executable]);
  const load = () => loadavg().map((value) => value.toFixed(2)).join(" ");
  console.log(`environment: ${platform()} ${arch()} ${cpus()[0]?.model ?? "unknown cpu"} x${cpus().length}, node ${process.version}, ${execute(clang, ["--version"]).split("\n")[0]}`);
  console.log(softFma
    ? `note: this compiler lowers Math.fma to the software routine tz_soft_fma (the hardware instruction is used only when the compiler is built for AArch64, and never for wasm32): the results are bit-identical, but each multiply-add takes about a microsecond or more. Tsuzuri mul_fma and mul_fma_parallel are timed with one product per sample${fusedAll ? " at every size (--fused-all): expect minutes per product at n=512" : `, and only up to n=${softFusedLimit} (--fused-all times every size)`}.`
    : "note: this compiler lowers Math.fma to llvm.fma for native code (hardware fma on this host); wasm32 below always uses the software routine.");
  console.log(`load average before: ${load()}`);
  const lines = execute(executable, [], fusedAll && softFma ? 6 * 3600_000 : 900_000).trim().split("\n");
  const references = new Map(lines.filter((line) => line.startsWith("reference ")).map((line) => {
    const [, size, f64, fused] = /^reference n=(\d+) f64=([0-9a-f]+) fused=([0-9a-f]+)$/.exec(line);
    return [Number(size), { f64, fused }];
  }));
  console.log(lines.filter((line) => !line.startsWith("reference ")).join("\n"));

  // The same program built for WebAssembly and run in Node: first the checksums against the C references (the
  // results must not depend on the target), then the times. The default build has no simd128 and no threads, so
  // mul_parallel runs its chunks one after another there.
  const bitsOf = (value) => {
    const view = new DataView(new ArrayBuffer(8));
    view.setFloat64(0, value);
    return view.getBigUint64(0).toString(16).padStart(16, "0");
  };
  const median = (values) => [...values].sort((a, b) => a - b)[values.length >> 1];
  const wasmModes = [["Tsuzuri Matrix.mul", 0n, "f64"], ["Tsuzuri i-j-k loop (Phase 1 order)", 4n, "f64"], ["Tsuzuri Matrix.mul_parallel", 1n, "f64"],
    ["Tsuzuri Matrix.mul_fma", 2n, "fused"], ["Tsuzuri Matrix.mul_fma_parallel", 3n, "fused"]];
  console.log(`note: wasm32 always uses the software fma, so Tsuzuri mul_fma and mul_fma_parallel are timed there with one product per sample, ${quick ? "9" : "3"} samples${quick ? " at n=16" : fusedAll ? " at n=64, 128 and 256" : " at n=64 only (--fused-all adds 128 and 256)"}.`);
  for (const [label, extra] of [["wasm32", []], ["wasm32 --wasm-feature simd128", ["--wasm-feature", "simd128"]]]) {
    const file = join(temporary, `matrix-${extra.length ? "simd128" : "default"}.wasm`);
    execute(compiler, ["build", join(root, "benchmarks/matrix"), "--target", "wasm32", ...extra, "-O3", "-o", file]);
    const module = new WebAssembly.Module(readFileSync(file));
    assert.deepEqual(WebAssembly.Module.imports(module), [], "the default WASM build has no imports");
    const run = new WebAssembly.Instance(module).exports.tz_matmul_checksum;
    for (const [n, fusedToo] of quick ? [[16, true]] : [[64, true], [128, fusedAll], [256, fusedAll]]) {
      for (const [mode, product, family] of wasmModes) {
        if (family === "fused" && !fusedToo) continue;
        assert.equal(bitsOf(run(BigInt(n), 1n, product)), references.get(n)[family], `${label} ${mode} n=${n}: checksum equals the C reference`);
        const repeats = family === "fused" ? 1 : quick ? 3 : n <= 64 ? 50 : n <= 128 ? 10 : 2;
        const times = Array.from({ length: family === "fused" && !quick ? 3 : samples }, () => {
          const start = performance.now();
          run(BigInt(n), BigInt(repeats), product);
          return (performance.now() - start) / repeats;
        });
        const middle = median(times);
        console.log(`n=${String(n).padEnd(4)} ${label.padEnd(30)} ${mode.padEnd(36)} median ${middle.toFixed(4).padStart(10)} ms  min ${Math.min(...times).toFixed(4).padStart(10)}  max ${Math.max(...times).toFixed(4).padStart(10)}  ${(2 * n ** 3 / (middle * 1e-3) / 1e9).toFixed(3).padStart(7)} GFLOP/s (median)  samples=${times.length}`);
      }
    }
  }
  console.log(`load average after: ${load()}`);
  if (quick) console.log("quick validation only; these times are not performance evidence");
} finally { rmSync(temporary, { recursive: true, force: true }); }
