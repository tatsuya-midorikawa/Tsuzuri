#if defined(__linux__)
/* glibc declares clock_gettime under -std=c11 only with this; macOS would then hide
   clock_gettime_nsec_np, so it is set only on Linux. */
#define _POSIX_C_SOURCE 200809L
#endif
#if defined(_WIN32)
#include <windows.h>
#endif
#include <errno.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <time.h>

extern uint32_t tsuzuri_bench_count(void);
extern int64_t tsuzuri_bench_sample(uint32_t index, int64_t iterations);

/* Nanoseconds of a monotonic clock from an arbitrary origin (Bench.now). */
int64_t tsuzuri_bench_now(void) {
#if defined(_WIN32)
    LARGE_INTEGER counter, frequency;
    QueryPerformanceCounter(&counter);
    QueryPerformanceFrequency(&frequency);
    int64_t ticks = counter.QuadPart, rate = frequency.QuadPart;
    return ticks / rate * 1000000000 + ticks % rate * 1000000000 / rate;
#elif defined(__APPLE__)
    return (int64_t)clock_gettime_nsec_np(CLOCK_UPTIME_RAW);
#else
    struct timespec now;
    clock_gettime(CLOCK_MONOTONIC, &now);
    return (int64_t)now.tv_sec * 1000000000 + now.tv_nsec;
#endif
}

/* A decimal argument in [minimum, maximum] without sign, spaces, or leading '+'. */
static int parse(const char *text, unsigned long long minimum, unsigned long long maximum,
                 unsigned long long *value) {
    if (text[0] < '0' || text[0] > '9') return 0;
    char *end = NULL;
    errno = 0;
    *value = strtoull(text, &end, 10);
    return errno != ERANGE && end != text && *end == '\0' && *value >= minimum && *value <= maximum;
}

#ifdef TSUZURI_BENCH_ALLOCS
extern void tsuzuri_bench_counts(uint64_t *out);

/* Two iterations minus one cancels the shared input's setup and drop. Warm up separately.
   Exit codes: 2 usage, 3 negative duration, 4 output error, 5 negative allocation delta. */
int main(int argc, char **argv) {
    unsigned long long index;
    if (argc != 2 || !parse(argv[1], 0, UINT32_MAX, &index) || index >= tsuzuri_bench_count()) return 2;
    uint64_t before[3], middle[3], after[3];
    if (tsuzuri_bench_sample((uint32_t)index, 1) < 0) return 3;
    tsuzuri_bench_counts(before);
    if (tsuzuri_bench_sample((uint32_t)index, 1) < 0) return 3;
    tsuzuri_bench_counts(middle);
    if (tsuzuri_bench_sample((uint32_t)index, 2) < 0) return 3;
    tsuzuri_bench_counts(after);
    uint64_t first_calls = (middle[0] - before[0]) + (middle[1] - before[1]);
    uint64_t second_calls = (after[0] - middle[0]) + (after[1] - middle[1]);
    uint64_t first_bytes = middle[2] - before[2], second_bytes = after[2] - middle[2];
    if (second_calls < first_calls || second_bytes < first_bytes) return 5;
    if (printf("alloc_calls %llu\nalloc_bytes %llu\n",
               (unsigned long long)(second_calls - first_calls),
               (unsigned long long)(second_bytes - first_bytes)) < 0) return 4;
    return fflush(stdout) == 0 ? 0 : 4;
}
#else
/* bench-runner INDEX SAMPLES TARGET_NS: doubles the iteration count from 1 until one sample
   takes TARGET_NS (at most 2^30 iterations), discards one warm-up sample, and prints
   "iterations N" and SAMPLES lines "sample NS". A negative sample exits with code 3. */
int main(int argc, char **argv) {
    unsigned long long index, samples, target;
    if (argc != 4 || !parse(argv[1], 0, UINT32_MAX, &index) || index >= tsuzuri_bench_count() ||
        !parse(argv[2], 1, 1000, &samples) || !parse(argv[3], 1, INT64_MAX, &target)) return 2;
    int64_t iterations = 1;
    for (;;) {
        int64_t elapsed = tsuzuri_bench_sample((uint32_t)index, iterations);
        if (elapsed < 0) return 3;
        if ((unsigned long long)elapsed >= target || iterations >= (INT64_C(1) << 30)) break;
        iterations *= 2;
    }
    if (tsuzuri_bench_sample((uint32_t)index, iterations) < 0) return 3;
    if (printf("iterations %lld\n", (long long)iterations) < 0) return 4;
    for (unsigned long long sample = 0; sample < samples; sample++) {
        int64_t elapsed = tsuzuri_bench_sample((uint32_t)index, iterations);
        if (elapsed < 0) return 3;
        if (printf("sample %lld\n", (long long)elapsed) < 0) return 4;
    }
    return fflush(stdout) == 0 ? 0 : 4;
}
#endif
