#include <errno.h>
#include <inttypes.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include "mix.h"

#if defined(_MSC_VER)
#define NOINLINE __declspec(noinline)
#else
#define NOINLINE __attribute__((noinline))
#endif

typedef int64_t (*Mixer)(int64_t, int64_t);
static volatile int64_t sink;

NOINLINE int64_t c_mix(int64_t iterations, int64_t seed) {
    if (iterations < 0) abort();
    uint64_t state = (uint64_t)seed;
    for (int64_t index = 0; index < iterations; ++index) {
        state = (state ^ (state >> 13)) * UINT64_C(6364136223846793005)
            + UINT64_C(1442695040888963407);
    }
    int64_t result;
    memcpy(&result, &state, sizeof(result));
    return result;
}

static NOINLINE double measure(Mixer function, int64_t iterations, int64_t seed, int64_t *result) {
    volatile int64_t input = seed;
    const clock_t start = clock();
    const int64_t value = function(iterations, input);
    sink = value;
    const clock_t end = clock();
    if (start == (clock_t)-1 || end == (clock_t)-1 || end < start) {
        fputs("CPU clock is unavailable\n", stderr);
        exit(1);
    }
    *result = value;
    return (double)(end - start) * 1000.0 / CLOCKS_PER_SEC;
}

int main(int argc, char **argv) {
    if (argc != 2) { fputs("expected a positive iteration count\n", stderr); return 2; }
    char *end = NULL;
    errno = 0;
    const int64_t iterations = strtoll(argv[1], &end, 10);
    if (errno || *end || iterations <= 0 || iterations > 100000000) {
        fputs("iterations must be in 1..100000000\n", stderr);
        return 2;
    }
    if (c_mix(10000, 42) != tz_mix(10000, 42)) {
        fputs("warmup checksum mismatch\n", stderr);
        return 1;
    }
    printf("{\"iterations\":%" PRId64 ",\"samples\":[", iterations);
    for (int sample = 0; sample < 9; ++sample) {
        int64_t c_result, tz_result;
        double c_ms, tz_ms;
        if (sample % 2) {
            c_ms = measure(c_mix, iterations, 42 + sample, &c_result);
            tz_ms = measure(tz_mix, iterations, 42 + sample, &tz_result);
        } else {
            tz_ms = measure(tz_mix, iterations, 42 + sample, &tz_result);
            c_ms = measure(c_mix, iterations, 42 + sample, &c_result);
        }
        if (c_result != tz_result) { fputs("checksum mismatch\n", stderr); return 1; }
        printf("%s{\"c_ms\":%.6f,\"tsuzuri_ms\":%.6f,\"checksum\":\"%016" PRIx64 "\"}",
               sample ? "," : "", c_ms, tz_ms, (uint64_t)tz_result);
    }
    puts("]}");
    return 0;
}
