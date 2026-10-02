// F12 Phase 2 の判断（docs/benchmarks.md「境界検査の Phase 2 の形」）のための計測。
// 使い方: tsuzuri build benchmarks/bounds_shapes --emit header -o m.h
//         tsuzuri build benchmarks/bounds_shapes --emit object -O3 -o m.o
//         clang -O2 -I. benchmarks/bounds_shapes/bench.c m.o -o bench && ./bench
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <time.h>
#include "m.h"

static double now(void) {
    struct timespec t;
    clock_gettime(CLOCK_MONOTONIC, &t);
    return t.tv_sec * 1e3 + t.tv_nsec / 1e6;
}

#define BENCH(name, call)                                                                    \
    do {                                                                                     \
        double best = 1e30;                                                                  \
        int64_t sink = 0;                                                                    \
        for (int round = 0; round < 7; ++round) {                                            \
            double start = now();                                                            \
            for (int rep = 0; rep < 200; ++rep) sink += (call);                              \
            double elapsed = now() - start;                                                  \
            if (elapsed < best) best = elapsed;                                              \
        }                                                                                    \
        printf("%-14s %8.3f ms per 200 calls (checksum %lld)\n", name, best, (long long)sink); \
    } while (0)

int main(void) {
    int64_t count = 1 << 20;
    int64_t *data = malloc(sizeof(int64_t) * (size_t)count);
    for (int64_t i = 0; i < count; ++i) data[i] = i & 1023;
    for (int pass = 0; pass < 2; ++pass) {
        BENCH("sum_all", tz_sum_all(data, count));
        BENCH("sum_first", tz_sum_first(data, count, count));
        BENCH("sum_checked", tz_sum_checked(data, count, count));
        BENCH("sum_while", tz_sum_while(data, count));
    }
    free(data);
    return 0;
}
