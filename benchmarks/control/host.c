#include <inttypes.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include "control.h"

typedef uint64_t (*Kernel)(int64_t, uint64_t);
#define DECLARE(name) \
    extern uint64_t c_##name(int64_t, uint64_t); \
    extern uint64_t cpp_##name(int64_t, uint64_t); \
    extern uint64_t rust_##name(int64_t, uint64_t);
DECLARE(while_mix)
DECLARE(for_mix)
DECLARE(tail_mix)
DECLARE(match_dispatch)
DECLARE(array_sum)

static volatile uint64_t sink;

struct Workload {
    const char *name;
    Kernel kernels[4];
    int64_t size;
};
#define WORKLOAD(name, size) {#name, {c_##name, cpp_##name, rust_##name, tz_##name}, size}

static double measure(Kernel kernel, int64_t size, uint64_t seed, uint64_t expected, int repeats) {
    volatile int64_t input_size = size;
    volatile uint64_t input_seed = seed;
    clock_t start = clock();
    for (int n = 0; n < repeats; ++n) {
        uint64_t result = kernel(input_size, input_seed);
        sink = result;
        if (result != expected) {
            fputs("checksum mismatch during measurement\n", stderr);
            exit(1);
        }
    }
    clock_t end = clock();
    if (start == (clock_t)-1 || end < start) {
        fputs("CPU clock unavailable\n", stderr);
        exit(1);
    }
    return (double)(end - start) * 1000.0 / CLOCKS_PER_SEC / repeats;
}

int main(int argc, char **argv) {
    if (argc > 2 || (argc == 2 && strcmp(argv[1], "--quick") != 0)) return 2;
    int quick = argc == 2;
    const struct Workload workloads[] = {
        WORKLOAD(while_mix, quick ? 1024 : 8000000),
        WORKLOAD(for_mix, quick ? 1024 : 8000000),
        WORKLOAD(tail_mix, quick ? 1024 : 8000000),
        WORKLOAD(match_dispatch, quick ? 1024 : 8000000),
        WORKLOAD(array_sum, quick ? 1024 : 4000000),
    };
    const char *variants[] = {"c", "cpp", "rust", "tsuzuri"};
    int samples = quick ? 4 : 12;
    printf("{\"samples\":%d,\"clock_ticks_per_second\":%ld,\"workloads\":[", samples, (long)CLOCKS_PER_SEC);
    for (size_t w = 0; w < sizeof(workloads) / sizeof(workloads[0]); ++w) {
        const struct Workload *work = &workloads[w];
        printf("%s{\"name\":\"%s\",\"size\":%" PRId64 ",\"checks\":[", w ? "," : "", work->name, work->size);
        const int64_t sizes[] = {0, 1, 2, 17, 257};
        const uint64_t seeds[] = {0, 42, UINT64_MAX, UINT64_C(9223372036854775808), UINT64_C(9223372036854775807)};
        int comma = 0;
        for (size_t i = 0; i < sizeof(sizes) / sizeof(sizes[0]); ++i) {
            for (size_t s = 0; s < sizeof(seeds) / sizeof(seeds[0]); ++s) {
                uint64_t expected = work->kernels[0](sizes[i], seeds[s]);
                for (int v = 0; v < 4; ++v) {
                    if (work->kernels[v](sizes[i], seeds[s]) != expected) {
                        fprintf(stderr, "%s/%s reference mismatch\n", work->name, variants[v]);
                        return 1;
                    }
                }
                printf("%s{\"size\":%" PRId64 ",\"seed\":\"%" PRIu64 "\",\"checksum\":\"%016" PRIx64 "\"}",
                       comma++ ? "," : "", sizes[i], seeds[s], expected);
            }
        }
        for (int warm = 0; warm < 2; ++warm) {
            for (int v = 0; v < 4; ++v) sink = work->kernels[v](work->size, (uint64_t)warm + 42);
        }
        fputs("],\"raw\":[", stdout);
        for (int sample = 0; sample < samples; ++sample) {
            uint64_t seed = UINT64_C(18446744073709551557) + (uint64_t)sample * 71;
            uint64_t expected = work->kernels[0](work->size, seed);
            double times[4];
            for (int order = 0; order < 4; ++order) {
                int variant = (sample + ((sample / 4) % 2 ? 3 - order : order)) % 4;
                times[variant] = measure(work->kernels[variant], work->size, seed, expected, quick ? 1 : 2);
            }
            printf("%s{\"seed\":\"%" PRIu64 "\",\"checksum\":\"%016" PRIx64 "\"",
                   sample ? "," : "", seed, expected);
            for (int v = 0; v < 4; ++v) printf(",\"%s_ms\":%.6f", variants[v], times[v]);
            fputs("}", stdout);
        }
        fputs("]}", stdout);
    }
    fputs("]}\n", stdout);
    return 0;
}
