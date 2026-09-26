#define _POSIX_C_SOURCE 200809L
#include <inttypes.h>
#include <math.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include "control.h"

#ifdef BASELINE
#include "before.h"
#define VARIANTS 5
#define BEFORE(name) , before_tz_##name
#else
#define VARIANTS 4
#define BEFORE(name)
#endif

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
DECLARE(array_copy)
DECLARE(list_sum)
DECLARE(closure_capture)
DECLARE(record_pipeline)
DECLARE(integer128_mix)
DECLARE(float32_mix)
DECLARE(float64_mix)

static volatile uint64_t sink;

struct Workload {
    const char *name;
    Kernel kernels[VARIANTS];
    int64_t size;
};
#define WORKLOAD_AS(reference, name, size) {#name, {c_##reference, cpp_##reference, rust_##reference, tz_##name BEFORE(name)}, size}
#define WORKLOAD(name, size) WORKLOAD_AS(name, name, size)

struct Timing { double cpu_ms, wall_ms; };

static struct Timing measure(Kernel kernel, int64_t size, uint64_t seed, uint64_t expected, int repeats) {
    volatile int64_t input_size = size;
    volatile uint64_t input_seed = seed;
    struct timespec wall_start, wall_end;
    if (clock_gettime(CLOCK_MONOTONIC, &wall_start) != 0) abort();
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
    if (clock_gettime(CLOCK_MONOTONIC, &wall_end) != 0) abort();
    if (start == (clock_t)-1 || end < start) {
        fputs("CPU clock unavailable\n", stderr);
        exit(1);
    }
    struct Timing timing = {
        (double)(end - start) * 1000.0 / CLOCKS_PER_SEC / repeats,
        ((double)(wall_end.tv_sec - wall_start.tv_sec) * 1000.0
            + (double)(wall_end.tv_nsec - wall_start.tv_nsec) / 1000000.0) / repeats,
    };
    return timing;
}

int main(int argc, char **argv) {
    int quick = argc == 2 && strcmp(argv[1], "--quick") == 0;
    double scale = 1.0;
    if (argc == 3 && strcmp(argv[1], "--scale") == 0) {
        char *end;
        scale = strtod(argv[2], &end);
        if (end == argv[2] || *end || !(scale > 0.0 && scale <= 10.0)) return 2;
    } else if (argc != 1 && !quick) return 2;
    const struct Workload workloads[] = {
        WORKLOAD(while_mix, quick ? 1024 : 8000000),
        WORKLOAD(for_mix, quick ? 1024 : 8000000),
        WORKLOAD(tail_mix, quick ? 1024 : 8000000),
        WORKLOAD_AS(tail_mix, tail_if_mix, quick ? 1024 : 8000000),
        WORKLOAD_AS(tail_mix, tail_builtin_mix, quick ? 1024 : 8000000),
        WORKLOAD(match_dispatch, quick ? 1024 : 8000000),
        WORKLOAD(array_sum, quick ? 1024 : 4000000),
        WORKLOAD(array_copy, quick ? 1024 : 2000000),
        WORKLOAD(list_sum, quick ? 1024 : 100000),
        WORKLOAD(closure_capture, quick ? 1024 : 1000000),
        WORKLOAD(record_pipeline, quick ? 1024 : 8000000),
        WORKLOAD(integer128_mix, quick ? 1024 : 2000000),
        WORKLOAD(float32_mix, quick ? 1024 : 4000000),
        WORKLOAD(float64_mix, quick ? 1024 : 4000000),
    };
    const char *variants[] = {"c", "cpp", "rust", "tsuzuri",
#ifdef BASELINE
        "before",
#endif
    };
    int samples = quick ? VARIANTS : VARIANTS * 3;
    printf("{\"samples\":%d,\"clock_ticks_per_second\":%ld,\"workloads\":[", samples, (long)CLOCKS_PER_SEC);
    for (size_t w = 0; w < sizeof(workloads) / sizeof(workloads[0]); ++w) {
        struct Workload scaled = workloads[w];
        if (!quick) scaled.size = (int64_t)fmax(1.0, ceil((double)scaled.size * scale));
        const struct Workload *work = &scaled;
        printf("%s{\"name\":\"%s\",\"size\":%" PRId64 ",\"checks\":[", w ? "," : "", work->name, work->size);
        const int64_t sizes[] = {0, 1, 2, 17, 257};
        const uint64_t seeds[] = {0, 42, UINT64_MAX, UINT64_C(9223372036854775808), UINT64_C(9223372036854775807)};
        int comma = 0;
        for (size_t i = 0; i < sizeof(sizes) / sizeof(sizes[0]); ++i) {
            for (size_t s = 0; s < sizeof(seeds) / sizeof(seeds[0]); ++s) {
                uint64_t expected = work->kernels[0](sizes[i], seeds[s]);
                for (int v = 0; v < VARIANTS; ++v) {
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
            for (int v = 0; v < VARIANTS; ++v) sink = work->kernels[v](work->size, (uint64_t)warm + 42);
        }
        int repeats[VARIANTS];
        uint64_t calibration_result = work->kernels[0](work->size, 42);
        for (int variant = 0; variant < VARIANTS; ++variant) {
            struct Timing calibration = measure(work->kernels[variant], work->size, 42, calibration_result, 1);
            repeats[variant] = quick ? 1 : (int)fmin(10000.0, ceil(20.0 / fmax(0.001, calibration.wall_ms)));
        }
        fputs("],\"raw\":[", stdout);
        for (int sample = 0; sample < samples; ++sample) {
            uint64_t seed = UINT64_C(18446744073709551557) + (uint64_t)sample * 71;
            uint64_t expected = work->kernels[0](work->size, seed);
            struct Timing times[VARIANTS];
            for (int order = 0; order < VARIANTS; ++order) {
                int variant = (sample + ((sample / VARIANTS) % 2 ? VARIANTS - 1 - order : order)) % VARIANTS;
                times[variant] = measure(work->kernels[variant], work->size, seed, expected, repeats[variant]);
            }
            printf("%s{\"seed\":\"%" PRIu64 "\",\"checksum\":\"%016" PRIx64 "\"",
                   sample ? "," : "", seed, expected);
            for (int v = 0; v < VARIANTS; ++v) {
                  printf(",\"%s_ms\":%.6f,\"%s_wall_ms\":%.9f,\"%s_repeats\":%d", variants[v], times[v].cpu_ms,
                      variants[v], times[v].wall_ms, variants[v], repeats[v]);
            }
            fputs("}", stdout);
        }
        fputs("]}", stdout);
    }
    fputs("]}\n", stdout);
    return 0;
}
