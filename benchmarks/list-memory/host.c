#define _POSIX_C_SOURCE 200809L
#include <inttypes.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

// Host of benchmarks/run-list-memory.mjs: runs one workload at one size and prints one JSON line.
//   host <name> <n> [repeats]        mean wall and CPU time of one call over `repeats` calls (calls after the first free their lists)
//   host built with -DTRACKING       the allocations of one call, counted by benchmarks/tracked_alloc.c
// The size reaches the workload through a volatile so that it is not folded into the call.

#ifdef TRACKING
void tracked_reset(void);
void tracked_read(uint64_t *, uint64_t *, uint64_t *);
#endif

extern int64_t tz_bulk_new(int64_t);
extern int64_t tz_cons_build(int64_t);
extern int64_t tz_tail_consume(int64_t);
extern int64_t tz_clone_list(int64_t);
extern int64_t tz_index_walk(int64_t);
extern int64_t tz_string_list(int64_t);

static const struct { const char *name; int64_t (*kernel)(int64_t); } workloads[] = {
    {"bulk_new", tz_bulk_new}, {"cons_build", tz_cons_build}, {"tail_consume", tz_tail_consume},
    {"clone_list", tz_clone_list}, {"index_walk", tz_index_walk}, {"string_list", tz_string_list},
};

#ifndef TRACKING
static double now_ms(clockid_t clock) {
    struct timespec t;
    clock_gettime(clock, &t);
    return (double)t.tv_sec * 1e3 + (double)t.tv_nsec / 1e6;
}
#endif

int main(int argc, char **argv) {
    if (argc < 3) {
        fprintf(stderr, "usage: %s <name> <n> [repeats]\n", argv[0]);
        return 2;
    }
    char *end = NULL;
    long long n = strtoll(argv[2], &end, 10);
    if (*end || n < 0) return 2;
    long repeats = argc > 3 ? strtol(argv[3], NULL, 10) : 1;
    if (repeats < 1) return 2;
    for (size_t index = 0; index < sizeof workloads / sizeof *workloads; ++index) {
        if (strcmp(workloads[index].name, argv[1])) continue;
        volatile int64_t input = (int64_t)n;
        int64_t result = 0;
#ifdef TRACKING
        tracked_reset();
        result = workloads[index].kernel(input);
        uint64_t calls, bytes, live;
        tracked_read(&calls, &bytes, &live);
        printf("{\"name\":\"%s\",\"n\":%lld,\"result\":%" PRId64 ",\"alloc_calls\":%" PRIu64 ",\"alloc_bytes\":%" PRIu64 ",\"live\":%" PRIu64 "}\n",
            argv[1], n, result, calls, bytes, live);
        (void)repeats;
#else
        double wall = now_ms(CLOCK_MONOTONIC), cpu = now_ms(CLOCK_PROCESS_CPUTIME_ID);
        for (long call = 0; call < repeats; ++call) result = workloads[index].kernel(input);
        wall = (now_ms(CLOCK_MONOTONIC) - wall) / (double)repeats;
        cpu = (now_ms(CLOCK_PROCESS_CPUTIME_ID) - cpu) / (double)repeats;
        printf("{\"name\":\"%s\",\"n\":%lld,\"result\":%" PRId64 ",\"wall_ms\":%.9f,\"cpu_ms\":%.9f}\n", argv[1], n, result, wall, cpu);
#endif
        return 0;
    }
    fprintf(stderr, "unknown workload: %s\n", argv[1]);
    return 3;
}
