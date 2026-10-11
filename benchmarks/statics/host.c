// Times the CRC-32 variants of benchmarks/statics/Main.tz, built by the current compiler and, with
// -DBASELINE, by an earlier one whose exports are renamed `before_tz_*`. Every variant checksums the same
// generated buffer, so a wrong value stops the run. Prints one JSON object of raw times.
#include <math.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

#include "statics.h"
#ifdef BASELINE
#include "before.h"
#endif

typedef int64_t (*variant_function)(int64_t);

// The reference in C: the same table, `static const`, and the same buffer.
static const uint32_t reference_table[256] = {
#include "table.inc"
};

static int64_t c_reference(int64_t count) {
    uint32_t crc = ~UINT32_C(0);
    for (int64_t i = 0; i < count; i++) {
        uint32_t byte = (uint32_t)((i * 7 + 3) & 255);
        crc = reference_table[(crc ^ byte) & 0xFF] ^ (crc >> 8);
    }
    return (int64_t)(uint32_t)~crc;
}

static double now_ms(clockid_t clock) {
    struct timespec now;
    clock_gettime(clock, &now);
    return (double)now.tv_sec * 1e3 + (double)now.tv_nsec / 1e6;
}

static const struct {
    const char *name;
    variant_function run;
} variants[] = {
    {"c_static_table", c_reference},
    {"in_place", tz_crc_in_place},
    {"bound", tz_crc_bound},
    {"copied", tz_crc_copied},
#ifdef BASELINE
    {"before_in_place", before_tz_crc_in_place},
    {"before_bound", before_tz_crc_bound},
    {"before_copied", before_tz_crc_copied},
#endif
};
#define VARIANTS (sizeof variants / sizeof variants[0])

int main(int argc, char **argv) {
    int64_t size = INT64_C(1) << 22;
    int runs = 9;
    for (int i = 1; i + 1 < argc; i += 2) {
        if (strcmp(argv[i], "--size") == 0) size = atoll(argv[i + 1]);
        else if (strcmp(argv[i], "--runs") == 0) runs = atoi(argv[i + 1]);
        else return 2;
    }
    const int64_t expected = c_reference(size);
    static double times[VARIANTS][64];
    static double wall[VARIANTS][64];
    if (runs < 1 || runs > 64) return 2;
    for (size_t v = 0; v < VARIANTS; v++) {
        if (variants[v].run(1024) != c_reference(1024)) {
            fprintf(stderr, "%s: a wrong checksum on the warmup\n", variants[v].name);
            return 1;
        }
    }
    // The order of the variants rotates every run, so that none always follows the same neighbor.
    for (int run = 0; run < runs; run++) {
        for (size_t step = 0; step < VARIANTS; step++) {
            size_t v = (step + (size_t)run) % VARIANTS;
            double start = now_ms(CLOCK_THREAD_CPUTIME_ID);
            double wall_start = now_ms(CLOCK_MONOTONIC);
            int64_t result = variants[v].run(size);
            times[v][run] = now_ms(CLOCK_THREAD_CPUTIME_ID) - start;
            wall[v][run] = now_ms(CLOCK_MONOTONIC) - wall_start;
            if (result != expected) {
                fprintf(stderr, "%s: checksum %lld, expected %lld\n", variants[v].name, (long long)result, (long long)expected);
                return 1;
            }
        }
    }
    printf("{\"size\": %lld, \"runs\": %d, \"checksum\": %lld, \"variants\": [", (long long)size, runs, (long long)expected);
    for (size_t v = 0; v < VARIANTS; v++) {
        printf("%s{\"name\": \"%s\", \"ms\": [", v ? ", " : "", variants[v].name);
        for (int run = 0; run < runs; run++) printf("%s%.4f", run ? ", " : "", times[v][run]);
        printf("], \"wall_ms\": [");
        for (int run = 0; run < runs; run++) printf("%s%.4f", run ? ", " : "", wall[v][run]);
        printf("]}");
    }
    printf("]}\n");
    return 0;
}
