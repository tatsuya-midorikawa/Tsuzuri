#define _POSIX_C_SOURCE 200809L
#undef NDEBUG
#include <inttypes.h>
#include <math.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include "apps.h"

#define APPS(X) \
    X(tokenize, 200000, 1024, 10000000) \
    X(format_join, 100000, 1024, 10000000) \
    X(csv_aggregate, 100000, 1024, 10000000) \
    X(map_count, 200000, 1024, 10000000) \
    X(record_sort, 200000, 1024, 10000000) \
    X(tree_transform, 256, 2, 100000) \
    X(binary_trees, 2000, 4, 1000000) \
    X(closure_pipeline, 1000000, 1024, 10000000) \
    X(graph_bfs, 100000, 1024, 1000000) \
    X(spectral_norm, 500, 16, 5000)

typedef uint64_t (*Kernel)(int64_t, uint64_t);
#define JOIN_INNER(prefix, name) prefix##name
#define JOIN(prefix, name) JOIN_INNER(prefix, name)

// RSS binaries link only their own implementation, avoiding Go startup in other languages.
#ifdef TRACKING
#define VARIANTS 2
#define DECLARE(name, normal, quick, maximum) extern uint64_t c_##name(int64_t, uint64_t);
#define FUNCTIONS(name) {c_##name, tz_##name}
#elif defined(PROCESS_PREFIX)
#define VARIANTS 1
#define DECLARE(name, normal, quick, maximum) extern uint64_t JOIN(PROCESS_PREFIX, name)(int64_t, uint64_t);
#define FUNCTIONS(name) {JOIN(PROCESS_PREFIX, name)}
static const char *const variants[] = {PROCESS_LABEL};
#else
#ifdef HAVE_ZIG
#define ZIG_COUNT 1
#define ZIG_FUNCTION(name) , zig_##name
#define DECLARE_ZIG(name) extern uint64_t zig_##name(int64_t, uint64_t);
#else
#define ZIG_COUNT 0
#define ZIG_FUNCTION(name)
#define DECLARE_ZIG(name)
#endif
#ifdef HAVE_GO
#define GO_COUNT 1
#define GO_FUNCTION(name) , go_##name
#define DECLARE_GO(name) extern uint64_t go_##name(int64_t, uint64_t);
extern int go_runtime_gomaxprocs(void);
extern int go_runtime_numcpu(void);
#else
#define GO_COUNT 0
#define GO_FUNCTION(name)
#define DECLARE_GO(name)
#endif
#define VARIANTS (4 + ZIG_COUNT + GO_COUNT)
#define DECLARE(name, normal, quick, maximum) \
    extern uint64_t c_##name(int64_t, uint64_t); \
    extern uint64_t cpp_##name(int64_t, uint64_t); \
    extern uint64_t rust_##name(int64_t, uint64_t); \
    DECLARE_ZIG(name) DECLARE_GO(name)
#define FUNCTIONS(name) {c_##name, cpp_##name, rust_##name, tz_##name ZIG_FUNCTION(name) GO_FUNCTION(name)}
static const char *const variants[] = {"c", "cpp", "rust", "tsuzuri",
#ifdef HAVE_ZIG
    "zig",
#endif
#ifdef HAVE_GO
    "go",
#endif
};
#endif
APPS(DECLARE)

struct Workload {
    const char *name;
    Kernel kernels[VARIANTS];
    int64_t normal_size, quick_size, maximum;
};
#define WORKLOAD(name, normal, quick, maximum) {#name, FUNCTIONS(name), normal, quick, maximum},
static const struct Workload workloads[] = {APPS(WORKLOAD)};

struct Timing { double cpu_ms, wall_ms; };

#ifndef TRACKING
static volatile uint64_t sink;
static struct Timing measure(Kernel kernel, int64_t size, uint64_t seed, uint64_t expected, int repeats) {
    volatile int64_t input_size = size;
    volatile uint64_t input_seed = seed;
    struct timespec wall_start, wall_end;
    if (clock_gettime(CLOCK_MONOTONIC, &wall_start) != 0) abort();
    clock_t start = clock();
    for (int index = 0; index < repeats; ++index) {
        uint64_t actual = kernel(input_size, input_seed);
        sink = actual;
        if (actual != expected) {
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
    return (struct Timing){
        (double)(end - start) * 1000.0 / CLOCKS_PER_SEC / repeats,
        ((double)(wall_end.tv_sec - wall_start.tv_sec) * 1000.0
            + (double)(wall_end.tv_nsec - wall_start.tv_nsec) / 1000000.0) / repeats,
    };
}
#else
void tracked_reset(void);
void tracked_read(uint64_t *, uint64_t *, uint64_t *);
#endif

static int cli_error(const char *message, const char *argument) {
    fprintf(stderr, "%s%s%s\n", message, argument ? ": " : "", argument ? argument : "");
    fputs("Usage: host [--only name language | --checks-only] [--quick | --scale number]\n", stderr);
    return 2;
}

static int64_t workload_size(const struct Workload *work, int quick, double scale) {
    int64_t size = quick ? work->quick_size : (int64_t)fmax(1.0, ceil((double)work->normal_size * scale));
    if (size < 0 || size > work->maximum) abort();
    return size;
}

int main(int argc, char **argv) {
    int quick = 0, scaled = 0, checks_only = 0;
    double scale = 1.0;
    const char *only_name = NULL, *only_language = NULL;
    for (int index = 1; index < argc; ++index) {
        const char *argument = argv[index];
        if (strcmp(argument, "--quick") == 0) {
            if (quick) return cli_error("duplicate flag", argument);
            quick = 1;
        } else if (strcmp(argument, "--scale") == 0) {
            if (scaled) return cli_error("duplicate flag", argument);
            scaled = 1;
            if (++index >= argc || strncmp(argv[index], "--", 2) == 0) return cli_error("missing value", argument);
            char *end;
            scale = strtod(argv[index], &end);
            if (end == argv[index] || *end || !isfinite(scale) || scale <= 0.0 || scale > 10.0)
                return cli_error("invalid scale", argv[index]);
        } else if (strcmp(argument, "--only") == 0) {
            if (only_name) return cli_error("duplicate flag", argument);
            if (index + 2 >= argc || argv[index + 1][0] == '-' || argv[index + 2][0] == '-')
                return cli_error("missing name or language", argument);
            only_name = argv[++index];
            only_language = argv[++index];
        } else if (strcmp(argument, "--checks-only") == 0) {
            if (checks_only) return cli_error("duplicate flag", argument);
            checks_only = 1;
        } else return cli_error("unknown argument", argument);
    }
    if (quick && scaled) return cli_error("--quick and --scale are mutually exclusive", NULL);
    if (only_name && checks_only) return cli_error("--only and --checks-only are mutually exclusive", NULL);
#ifdef TRACKING
    if (only_name || only_language || checks_only) return cli_error("tracking requires all workloads", NULL);
    fputs("{\"tracking\":1,\"workloads\":[", stdout);
    for (size_t index = 0; index < sizeof(workloads) / sizeof(workloads[0]); ++index) {
        const struct Workload *work = &workloads[index];
        int64_t size = workload_size(work, quick, scale);
        uint64_t expected = work->kernels[0](size, 42);
        tracked_reset();
        uint64_t actual = work->kernels[1](size, 42);
        uint64_t calls, bytes, live;
        tracked_read(&calls, &bytes, &live);
        if (actual != expected || live != 0) {
            fprintf(stderr, "%s: tracked checksum mismatch or retained %" PRIu64 " bytes\n", work->name, live);
            return 1;
        }
        printf("%s{\"name\":\"%s\",\"size\":%" PRId64 ",\"seed\":\"42\",\"checksum\":\"%016" PRIx64
               "\",\"calls\":%" PRIu64 ",\"bytes\":%" PRIu64 ",\"live\":%" PRIu64 "}",
               index ? "," : "", work->name, size, actual, calls, bytes, live);
    }
    fputs("]}\n", stdout);
    return 0;
#else
    if (only_name) {
        const struct Workload *work = NULL;
        for (size_t index = 0; index < sizeof(workloads) / sizeof(workloads[0]); ++index)
            if (strcmp(only_name, workloads[index].name) == 0) work = &workloads[index];
        if (!work) return cli_error("unknown workload", only_name);
        int variant = -1;
        for (int index = 0; index < VARIANTS; ++index)
            if (strcmp(only_language, variants[index]) == 0) variant = index;
        if (variant < 0) return cli_error("language not compiled into this host", only_language);
        uint64_t actual = work->kernels[variant](workload_size(work, quick, scale), 42);
        sink = actual;
        printf("%016" PRIx64 "\n", actual);
        return 0;
    }
    int samples = checks_only ? 0 : quick ? VARIANTS : VARIANTS * 3;
    printf("{\"samples\":%d,\"clock_ticks_per_second\":%ld,\"variants\":[", samples, (long)CLOCKS_PER_SEC);
    for (int variant = 0; variant < VARIANTS; ++variant) printf("%s\"%s\"", variant ? "," : "", variants[variant]);
    fputs("]", stdout);
#if defined(HAVE_GO) && !defined(PROCESS_PREFIX)
    printf(",\"go_runtime\":{\"gomaxprocs\":%d,\"num_cpu\":%d}", go_runtime_gomaxprocs(), go_runtime_numcpu());
#endif
    fputs(",\"workloads\":[", stdout);
    const int64_t sizes[] = {0, 1, 2, 17, 257};
    const uint64_t seeds[] = {0, 42, UINT64_MAX, UINT64_C(9223372036854775808), UINT64_C(9223372036854775807)};
    for (size_t index = 0; index < sizeof(workloads) / sizeof(workloads[0]); ++index) {
        const struct Workload *work = &workloads[index];
        int64_t size = workload_size(work, quick, scale);
        printf("%s{\"name\":\"%s\",\"size\":%" PRId64 ",\"checks\":[", index ? "," : "", work->name, size);
        int comma = 0;
        for (size_t count = 0; count < sizeof(sizes) / sizeof(sizes[0]); ++count) {
            for (size_t seed = 0; seed < sizeof(seeds) / sizeof(seeds[0]); ++seed) {
                uint64_t expected = work->kernels[0](sizes[count], seeds[seed]);
                for (int variant = 0; variant < VARIANTS; ++variant) {
                    uint64_t actual = work->kernels[variant](sizes[count], seeds[seed]);
                    if (actual != expected) {
                        fprintf(stderr, "%s/%s at size %" PRId64 ", seed %" PRIu64
                                ": %016" PRIx64 " != %016" PRIx64 "\n",
                                work->name, variants[variant], sizes[count], seeds[seed], actual, expected);
                        return 1;
                    }
                }
                printf("%s{\"size\":%" PRId64 ",\"seed\":\"%" PRIu64 "\",\"checksum\":\"%016" PRIx64 "\"}",
                       comma++ ? "," : "", sizes[count], seeds[seed], expected);
            }
        }
        fputs("],\"warmups\":[", stdout);
        if (!checks_only) {
            for (int warmup = 0; warmup < 2; ++warmup) {
                uint64_t seed = 42 + (uint64_t)warmup;
                uint64_t expected = work->kernels[0](size, seed);
                for (int variant = 0; variant < VARIANTS; ++variant) {
                    sink = work->kernels[variant](size, seed);
                    if (sink != expected) {
                        fprintf(stderr, "%s/%s warmup checksum mismatch\n", work->name, variants[variant]);
                        return 1;
                    }
                }
                printf("%s{\"seed\":\"%" PRIu64 "\",\"checksum\":\"%016" PRIx64 "\"}",
                       warmup ? "," : "", seed, expected);
            }
        }
        fputs("],\"raw\":[", stdout);
        int repeats[VARIANTS] = {0};
        if (!checks_only) {
            uint64_t expected = work->kernels[0](size, 42);
            for (int variant = 0; variant < VARIANTS; ++variant) {
                struct Timing calibration = measure(work->kernels[variant], size, 42, expected, 1);
                repeats[variant] = quick ? 1 : (int)fmin(10000.0, ceil(20.0 / fmax(0.001, calibration.wall_ms)));
            }
        }
        for (int sample = 0; sample < samples; ++sample) {
            uint64_t seed = UINT64_C(18446744073709551557) + (uint64_t)sample * 71;
            uint64_t expected = work->kernels[0](size, seed);
            struct Timing times[VARIANTS];
            for (int order = 0; order < VARIANTS; ++order) {
                int variant = (sample + ((sample / VARIANTS) % 2 ? VARIANTS - 1 - order : order)) % VARIANTS;
                times[variant] = measure(work->kernels[variant], size, seed, expected, repeats[variant]);
            }
            printf("%s{\"seed\":\"%" PRIu64 "\",\"checksum\":\"%016" PRIx64 "\"", sample ? "," : "", seed, expected);
            for (int variant = 0; variant < VARIANTS; ++variant) {
                printf(",\"%s_ms\":%.9f,\"%s_wall_ms\":%.9f,\"%s_repeats\":%d",
                       variants[variant], times[variant].cpu_ms, variants[variant], times[variant].wall_ms,
                       variants[variant], repeats[variant]);
            }
            fputs("}", stdout);
        }
        fputs("]}", stdout);
    }
    fputs("]}\n", stdout);
    return 0;
#endif
}
