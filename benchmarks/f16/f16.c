/* Hosts the Tsuzuri kernels of benchmarks/f16/Main.tz next to the same kernels in C `_Float16` (D10).
   usage: f16 COUNT SAMPLES
   Each workload runs both implementations in rotating order, SAMPLES times after a warm-up, and prints one JSON line
   per implementation: the median, minimum, and maximum of a pass in milliseconds and the checksum (the result's bits).
   The checksums must be equal: every implementation computes the same binary16 operations in the same order, and this
   host exits with an error otherwise. Its checks are not `assert`s, which NDEBUG removes together with their calls. */
#define _POSIX_C_SOURCE 200809L
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

extern double tz_half_chain(const double *, int64_t);
extern double tz_half_roundtrip(const double *, int64_t);
extern double tz_half_scale(const double *, int64_t);

static void require(int condition, const char *what) {
  if (condition) return;
  fprintf(stderr, "f16 benchmark host: %s\n", what);
  exit(1);
}

/* `-ffp-contract=off` keeps the product and the sum apart: a fused multiply-add would round once. */
static double c_half_chain(const double *values, int64_t count) {
  _Float16 total = 0;
  const _Float16 factor = (_Float16)0.9990234375;
  for (int64_t index = 0; index < count; ++index) total = total * factor + (_Float16)values[index];
  return (double)total;
}

static double c_half_roundtrip(const double *values, int64_t count) {
  double total = 0.0;
  for (int64_t index = 0; index < count; ++index) total = total + (double)(_Float16)values[index];
  return total;
}

static double c_half_scale(const double *values, int64_t count) {
  double total = 0.0;
  const _Float16 factor = (_Float16)1.5;
  for (int64_t index = 0; index < count; ++index) total = total + (double)((_Float16)values[index] * factor);
  return total;
}

typedef double (*kernel)(const double *, int64_t);
static double now(void) {
  struct timespec time;
  require(clock_gettime(CLOCK_MONOTONIC, &time) == 0, "clock_gettime failed");
  return (double)time.tv_sec + (double)time.tv_nsec * 1e-9;
}
static uint64_t bits(double value) {
  uint64_t result;
  memcpy(&result, &value, 8);
  return result;
}
static int compare(const void *left, const void *right) {
  double a = *(const double *)left, b = *(const double *)right;
  return (a > b) - (a < b);
}

int main(int argc, char **argv) {
  if (argc != 3) return 2;
  const int64_t count = atoll(argv[1]);
  const int samples = atoi(argv[2]);
  require(count > 0 && samples > 0 && samples <= 99, "COUNT must be positive and SAMPLES between 1 and 99");
  double *values = malloc((size_t)count * 8);
  require(values != NULL, "out of memory");
  /* Multiples of 1/64 in about [-16, 16]: binary16 values that a chain of products and sums keeps finite. */
  for (int64_t index = 0; index < count; ++index) values[index] = (double)((index * 2654435761u) % 2001 - 1000) / 64.0;
  const char *workloads[] = {"half_chain", "half_roundtrip", "half_scale"};
  kernel tsuzuri[] = {tz_half_chain, tz_half_roundtrip, tz_half_scale}, c[] = {c_half_chain, c_half_roundtrip, c_half_scale};
  for (int workload = 0; workload < 3; ++workload) {
    kernel implementations[2] = {tsuzuri[workload], c[workload]};
    const char *names[2] = {"tsuzuri", "c"};
    uint64_t checksums[2];
    for (int implementation = 0; implementation < 2; ++implementation) checksums[implementation] = bits(implementations[implementation](values, count));
    require(checksums[0] == checksums[1], "the Tsuzuri and the C kernel return different bits");
    /* Passes per sample, so that a sample of the slower implementation is about 60 ms. */
    double start = now();
    implementations[0](values, count);
    double once = now() - start;
    int repeats = once >= 0.06 ? 1 : (int)(0.06 / (once > 1e-6 ? once : 1e-6)) + 1;
    if (repeats > 2000) repeats = 2000;
    double times[2][100];
    volatile double sink = 0.0;
    for (int sample = -1; sample < samples; ++sample) {
      for (int offset = 0; offset < 2; ++offset) {
        int implementation = (sample + 1 + offset) % 2;
        start = now();
        for (int run = 0; run < repeats; ++run) sink = implementations[implementation](values, count);
        double elapsed = (now() - start) * 1000.0 / repeats;
        if (sample >= 0) times[implementation][sample] = elapsed;
        require(bits(sink) == checksums[implementation], "a pass returned different bits");
      }
    }
    for (int implementation = 0; implementation < 2; ++implementation) {
      qsort(times[implementation], (size_t)samples, sizeof(double), compare);
      printf("{\"workload\":\"%s\",\"implementation\":\"%s\",\"milliseconds\":%.6f,\"min\":%.6f,\"max\":%.6f,\"count\":%lld,\"repeats\":%d,\"samples\":%d,\"checksum\":\"0x%016llx\"}\n",
             workloads[workload], names[implementation], times[implementation][samples / 2], times[implementation][0],
             times[implementation][samples - 1], (long long)count, repeats, samples, (unsigned long long)checksums[implementation]);
    }
  }
  free(values);
  return 0;
}
