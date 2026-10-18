// The driver of benchmarks/run-backend-kernels.mjs (PB05): it times the kernels of benchmarks/control as one build of Tsuzuri
// compiled them, with no C, C++ or Rust reference. `kernels [repeats]` prints, for each kernel, its name, the checksum of the last
// call, and the best and median wall milliseconds of the repeats.
#define _POSIX_C_SOURCE 200809L
#include <inttypes.h>
#include <stdio.h>
#include <stdlib.h>
#include <time.h>

uint64_t tz_while_mix(int64_t, uint64_t);
uint64_t tz_for_mix(int64_t, uint64_t);
uint64_t tz_tail_mix(int64_t, uint64_t);
uint64_t tz_tail_if_mix(int64_t, uint64_t);
uint64_t tz_tail_builtin_mix(int64_t, uint64_t);
uint64_t tz_match_dispatch(int64_t, uint64_t);
uint64_t tz_array_sum(int64_t, uint64_t);
uint64_t tz_array_index_sum(int64_t, uint64_t);
uint64_t tz_array_copy(int64_t, uint64_t);
uint64_t tz_list_sum(int64_t, uint64_t);
uint64_t tz_closure_capture(int64_t, uint64_t);
uint64_t tz_closure_churn(int64_t, uint64_t);
uint64_t tz_record_pipeline(int64_t, uint64_t);
uint64_t tz_integer128_mix(int64_t, uint64_t);
uint64_t tz_float32_mix(int64_t, uint64_t);
uint64_t tz_float64_mix(int64_t, uint64_t);

struct Kernel {
    const char *name;
    uint64_t (*run)(int64_t, uint64_t);
    int64_t size;
};

static double now_ms(void) {
    struct timespec time;
    clock_gettime(CLOCK_MONOTONIC, &time);
    return (double)time.tv_sec * 1e3 + (double)time.tv_nsec / 1e6;
}

static int compare(const void *a, const void *b) {
    double x = *(const double *)a, y = *(const double *)b;
    return x < y ? -1 : x > y;
}

int main(int argc, char **argv) {
    int repeats = argc > 1 ? atoi(argv[1]) : 5;
    if (repeats < 1) return 2;
    // The sizes are those of benchmarks/control/host.c; run-backend-kernels.mjs reads this table for wasm32 too.
    const struct Kernel kernels[] = {
        {"while_mix", tz_while_mix, 8000000},
        {"for_mix", tz_for_mix, 8000000},
        {"tail_mix", tz_tail_mix, 8000000},
        {"tail_if_mix", tz_tail_if_mix, 8000000},
        {"tail_builtin_mix", tz_tail_builtin_mix, 8000000},
        {"match_dispatch", tz_match_dispatch, 8000000},
        {"array_sum", tz_array_sum, 4000000},
        {"array_index_sum", tz_array_index_sum, 4000000},
        {"array_copy", tz_array_copy, 2000000},
        {"list_sum", tz_list_sum, 100000},
        {"closure_capture", tz_closure_capture, 1000000},
        {"closure_churn", tz_closure_churn, 1000000},
        {"record_pipeline", tz_record_pipeline, 8000000},
        {"integer128_mix", tz_integer128_mix, 2000000},
        {"float32_mix", tz_float32_mix, 4000000},
        {"float64_mix", tz_float64_mix, 4000000},
    };
    for (size_t k = 0; k < sizeof kernels / sizeof kernels[0]; k++) {
        double *times = malloc(sizeof(double) * (size_t)repeats);
        uint64_t checksum = 0;
        for (int r = 0; r < repeats; r++) {
            volatile int64_t size = kernels[k].size;
            double start = now_ms();
            checksum = kernels[k].run(size, 42);
            times[r] = now_ms() - start;
        }
        qsort(times, (size_t)repeats, sizeof(double), compare);
        printf("%s %016" PRIx64 " %.3f %.3f\n", kernels[k].name, checksum, times[0], times[repeats / 2]);
        free(times);
    }
    return 0;
}
