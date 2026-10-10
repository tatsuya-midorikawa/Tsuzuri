/* Harness of the Vulkan runtime (src/runtime/gpu-vulkan.c, F09 Phase 3). It includes the runtime source, so the tests
   run the code that the compiler links, with the allocation of the runtime counted and the internal state reachable
   (tests/gpu_vulkan.mjs builds it with and without -fsanitize=address,undefined; tests/gpu_spirv.rs runs it on the SPIR-V
   that the compiler emits). Exit code of every mode: the status of the last call (0 ok, 1 unavailable, 2 unsupported,
   3 limit exceeded, 4 failed), or 64 for a harness error. A line `status=<n>` is printed first. */
#undef NDEBUG
#if defined(__APPLE__) && !defined(_DARWIN_C_SOURCE)
#define _DARWIN_C_SOURCE
#endif
#if defined(__linux__) && !defined(_GNU_SOURCE)
#define _GNU_SOURCE
#endif
#define TZ_VK_TEST_HOOKS 1
#include <assert.h>
#include <inttypes.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

static long tz_live_allocations;
static long tz_total_allocations;

static void *tz_tracked_alloc(size_t bytes) {
    void *block = malloc(bytes + 16);
    if (block == NULL) return NULL;
    *(size_t *)block = bytes;
    tz_live_allocations++;
    tz_total_allocations++;
    return (char *)block + 16;
}

static void tz_tracked_free(void *pointer) {
    if (pointer == NULL) return;
    tz_live_allocations--;
    free((char *)pointer - 16);
}

#define TZ_VK_ALLOC(bytes) tz_tracked_alloc(bytes)
#define TZ_VK_FREE(pointer) tz_tracked_free(pointer)
#include "../src/runtime/gpu-vulkan.c"

#define HARNESS_ERROR 64

static void *read_file(const char *path, size_t *size) {
    FILE *file = fopen(path, "rb");
    if (file == NULL) return NULL;
    fseek(file, 0, SEEK_END);
    long length = ftell(file);
    fseek(file, 0, SEEK_SET);
    void *data = malloc(length > 0 ? (size_t)length : 1);
    if (data != NULL && length > 0 && fread(data, 1, (size_t)length, file) != (size_t)length) {
        free(data);
        data = NULL;
    }
    fclose(file);
    *size = length > 0 ? (size_t)length : 0;
    return data;
}

static int write_file(const char *path, const void *data, size_t size) {
    FILE *file = fopen(path, "wb");
    if (file == NULL) return 0;
    int ok = size == 0 || fwrite(data, 1, size, file) == size;
    return fclose(file) == 0 && ok;
}

static double now_ms(void) {
    struct timespec time;
    clock_gettime(CLOCK_MONOTONIC, &time);
    return (double)time.tv_sec * 1000.0 + (double)time.tv_nsec / 1e6;
}

struct job {
    const char *spirv_path;
    const char *input_path;
    const char *output_path;
    int mode;
    int32_t lanes;
    int32_t features;
    int32_t weight;
    int64_t count;
    int repeat;
    int threads;
    int staged;
    int assume_strict;
    const char *then_spirv_path; /* auto: a second kernel that is asked about after the first one's rounds */
    int64_t then_count;
    int32_t then_weight;
    int then_repeat;
    int programs; /* programs: how many distinct kernels to run */
    int open_first; /* auto: open the backend with these features (an explicit request) before the questions; -1 for no */
    int open_after; /* auto: and after them; -1 for no */
};

static int parse_lanes(const char *text, int32_t *lanes) {
    int input = 0, output = 0;
    if (sscanf(text, "%d,%d", &input, &output) != 2) return 0;
    *lanes = input | (output << 8);
    return 1;
}

static int parse_job(int argc, char **argv, struct job *job) {
    memset(job, 0, sizeof *job);
    job->repeat = 1;
    job->threads = 1;
    job->weight = 1;
    job->open_first = job->open_after = -1;
    for (int index = 2; index < argc; index++) {
        const char *flag = argv[index];
        const char *value = index + 1 < argc ? argv[index + 1] : NULL;
        if (strcmp(flag, "--staged") == 0) {
            job->staged = 1;
        } else if (strcmp(flag, "--assume-strict") == 0) {
            /* UNCERTIFIED: runs a strict float kernel on a device that does not report the float controls. */
            job->assume_strict = 1;
        } else if (value == NULL) {
            fprintf(stderr, "missing value for %s\n", flag);
            return 0;
        } else if (strcmp(flag, "--spirv") == 0) {
            job->spirv_path = value;
            index++;
        } else if (strcmp(flag, "--input") == 0) {
            job->input_path = value;
            index++;
        } else if (strcmp(flag, "--output") == 0) {
            job->output_path = value;
            index++;
        } else if (strcmp(flag, "--mode") == 0) {
            job->mode = strcmp(value, "init") == 0;
            index++;
        } else if (strcmp(flag, "--lanes") == 0) {
            if (!parse_lanes(value, &job->lanes)) return 0;
            index++;
        } else if (strcmp(flag, "--features") == 0) {
            job->features = atoi(value);
            index++;
        } else if (strcmp(flag, "--weight") == 0) {
            job->weight = atoi(value);
            index++;
        } else if (strcmp(flag, "--count") == 0) {
            job->count = atoll(value);
            index++;
        } else if (strcmp(flag, "--repeat") == 0) {
            job->repeat = atoi(value);
            index++;
        } else if (strcmp(flag, "--threads") == 0) {
            job->threads = atoi(value);
            index++;
        } else if (strcmp(flag, "--then-spirv") == 0) {
            job->then_spirv_path = value;
            index++;
        } else if (strcmp(flag, "--then-count") == 0) {
            job->then_count = atoll(value);
            index++;
        } else if (strcmp(flag, "--then-weight") == 0) {
            job->then_weight = atoi(value);
            index++;
        } else if (strcmp(flag, "--then-repeat") == 0) {
            job->then_repeat = atoi(value);
            index++;
        } else if (strcmp(flag, "--programs") == 0) {
            job->programs = atoi(value);
            index++;
        } else if (strcmp(flag, "--open-first") == 0) {
            job->open_first = atoi(value);
            index++;
        } else if (strcmp(flag, "--open-after") == 0) {
            job->open_after = atoi(value);
            index++;
        } else {
            fprintf(stderr, "unknown flag %s\n", flag);
            return 0;
        }
    }
    return 1;
}

static int output_lane_bytes(int32_t lanes) {
    return tz_vk_lane_size((lanes >> 8) & 0xFF);
}

/* The output buffer of a job; a count outside the range of the ABI gets a token buffer (the runtime refuses it). */
static size_t output_bytes_for(const struct job *job) {
    if (job->count < 0 || job->count > TZ_VK_MAX_LANES) return 1;
    return (size_t)job->count * (size_t)output_lane_bytes(job->lanes);
}

struct worker {
    const struct job *job;
    const void *spirv;
    size_t spirv_size;
    const void *input;
    int32_t status;
    unsigned char *output;
    size_t output_size;
};

static void *worker_main(void *argument) {
    struct worker *worker = (struct worker *)argument;
    for (int pass = 0; pass < worker->job->repeat; pass++) {
        memset(worker->output, 0xA5, worker->output_size);
        worker->status = tz_vulkan_run(worker->job->mode, 0, worker->job->lanes, NULL, 0, worker->spirv,
            (int32_t)worker->spirv_size, worker->input, worker->job->count, worker->output);
        if (worker->status != 0) break;
    }
    return NULL;
}

static int run_job(const struct job *job) {
    size_t spirv_size = 0, input_size = 0;
    void *spirv = read_file(job->spirv_path, &spirv_size);
    void *input = job->input_path != NULL ? read_file(job->input_path, &input_size) : NULL;
    if (spirv == NULL || (job->input_path != NULL && input == NULL)) {
        fprintf(stderr, "cannot read the input files\n");
        return HARNESS_ERROR;
    }
    int32_t status = tz_vulkan_open(job->assume_strict ? (job->features & ~TZ_VK_FEATURE_STRICT_F32) : job->features);
    printf("open=%d\n", (int)status);
    if (status != 0) {
        free(spirv);
        free(input);
        printf("status=%d\n", (int)status);
        return (int)status;
    }
    if (job->staged) tz_vk.caps.direct_transfer = 0;
    if (job->assume_strict) {
        tz_vk.caps.strict_f32 = 1;
        tz_vk.caps.strict_probe = 1; /* as if the device had passed the conformance probe */
    }
    size_t output_size = output_bytes_for(job);
    int threads = job->threads < 1 ? 1 : job->threads;
    struct worker *workers = (struct worker *)calloc((size_t)threads, sizeof *workers);
    for (int index = 0; index < threads; index++) {
        workers[index].job = job;
        workers[index].spirv = spirv;
        workers[index].spirv_size = spirv_size;
        workers[index].input = input;
        workers[index].output = (unsigned char *)malloc(output_size > 0 ? output_size : 1);
        workers[index].output_size = output_size;
    }
    double start = now_ms();
    if (threads == 1) {
        worker_main(&workers[0]);
    } else {
        pthread_t *handles = (pthread_t *)calloc((size_t)threads, sizeof *handles);
        for (int index = 0; index < threads; index++) pthread_create(&handles[index], NULL, worker_main, &workers[index]);
        for (int index = 0; index < threads; index++) pthread_join(handles[index], NULL);
        free(handles);
    }
    double elapsed = now_ms() - start;
    int result = 0;
    for (int index = 0; index < threads; index++) {
        if (workers[index].status != 0) result = workers[index].status;
    }
    if (result == 0) {
        for (int index = 1; index < threads; index++) {
            if (memcmp(workers[0].output, workers[index].output, output_size) != 0) {
                fprintf(stderr, "thread %d produced different output\n", index);
                result = HARNESS_ERROR;
            }
        }
        if (job->output_path != NULL && !write_file(job->output_path, workers[0].output, output_size)) result = HARNESS_ERROR;
    }
    printf("status=%d time_ms=%.3f live_allocations=%ld\n", result, elapsed, tz_live_allocations);
    for (int index = 0; index < threads; index++) free(workers[index].output);
    free(workers);
    free(spirv);
    free(input);
    return result;
}

/* Prints the median, minimum and maximum of `repeat` timed runs (the first run, which creates the pipeline, is
   reported separately). */
static int compare_double(const void *left, const void *right) {
    double a = *(const double *)left, b = *(const double *)right;
    return a < b ? -1 : a > b;
}

static int bench_job(const struct job *job) {
    size_t spirv_size = 0, input_size = 0;
    void *spirv = read_file(job->spirv_path, &spirv_size);
    void *input = job->input_path != NULL ? read_file(job->input_path, &input_size) : NULL;
    if (spirv == NULL || (job->input_path != NULL && input == NULL)) return HARNESS_ERROR;
    double open_start = now_ms();
    int32_t status = tz_vulkan_open(job->features);
    double open_ms = now_ms() - open_start;
    if (status != 0) {
        printf("status=%d\n", (int)status);
        return (int)status;
    }
    if (job->staged) tz_vk.caps.direct_transfer = 0;
    size_t output_size = output_bytes_for(job);
    unsigned char *output = (unsigned char *)malloc(output_size > 0 ? output_size : 1);
    int repeat = job->repeat < 1 ? 1 : job->repeat;
    double *times = (double *)malloc(sizeof(double) * (size_t)repeat);
    double first_ms = 0;
    for (int pass = 0; pass < repeat + 1; pass++) {
        double start = now_ms();
        status = tz_vulkan_run(job->mode, 0, job->lanes, NULL, 0, spirv, (int32_t)spirv_size, input, job->count, output);
        double elapsed = now_ms() - start;
        if (status != 0) break;
        if (pass == 0) first_ms = elapsed;
        else times[pass - 1] = elapsed;
    }
    if (status == 0) {
        qsort(times, (size_t)repeat, sizeof(double), compare_double);
        char text[1024];
        tz_vulkan_describe(text, sizeof text);
        printf("status=0 open_ms=%.3f first_run_ms=%.3f median_ms=%.4f min_ms=%.4f max_ms=%.4f count=%" PRId64 " %s\n",
            open_ms, first_ms, times[repeat / 2], times[0], times[repeat - 1], job->count, text);
    } else {
        printf("status=%d\n", (int)status);
    }
    free(times);
    free(output);
    free(spirv);
    free(input);
    return (int)status;
}

/* --- the mock library (tests/gpu_vulkan_mock.c): configuration, fault sweep --- */

typedef int (*mock_configure_function)(const char *);
typedef int (*mock_query_function)(void);
typedef void (*mock_action_function)(void);
typedef const char *(*mock_text_function)(void);

struct mock_api {
    void *handle;
    mock_configure_function configure;
    mock_query_function injected;
    mock_query_function leaked;
    mock_query_function probe_dispatches;
    mock_query_function calls;
    mock_query_function created;
    mock_query_function opened;
    mock_query_function waits_entered;
    mock_query_function live_pipelines;
    mock_action_function release_wait;
    mock_text_function leak_report;
};

static int load_mock(struct mock_api *mock) {
    const char *path = getenv("TSUZURI_VULKAN_LIBRARY");
    if (path == NULL) return 0;
    mock->handle = dlopen(path, RTLD_NOW | RTLD_LOCAL);
    if (mock->handle == NULL) return 0;
    mock->configure = (mock_configure_function)dlsym(mock->handle, "tz_vk_mock_configure");
    mock->injected = (mock_query_function)dlsym(mock->handle, "tz_vk_mock_injected");
    mock->leaked = (mock_query_function)dlsym(mock->handle, "tz_vk_mock_leaked");
    mock->probe_dispatches = (mock_query_function)dlsym(mock->handle, "tz_vk_mock_probe_dispatches");
    mock->calls = (mock_query_function)dlsym(mock->handle, "tz_vk_mock_calls");
    mock->created = (mock_query_function)dlsym(mock->handle, "tz_vk_mock_created");
    mock->opened = (mock_query_function)dlsym(mock->handle, "tz_vk_mock_opened");
    mock->waits_entered = (mock_query_function)dlsym(mock->handle, "tz_vk_mock_waits_entered");
    mock->live_pipelines = (mock_query_function)dlsym(mock->handle, "tz_vk_mock_live_pipelines");
    mock->release_wait = (mock_action_function)dlsym(mock->handle, "tz_vk_mock_release_wait");
    mock->leak_report = (mock_text_function)dlsym(mock->handle, "tz_vk_mock_leak_report");
    return mock->configure != NULL && mock->injected != NULL && mock->leaked != NULL && mock->probe_dispatches != NULL
        && mock->calls != NULL && mock->created != NULL && mock->opened != NULL && mock->waits_entered != NULL
        && mock->live_pipelines != NULL && mock->release_wait != NULL && mock->leak_report != NULL;
}

/* Runs open + run once per injected failure: the Nth fallible Vulkan call fails, for N = 1, 2, ... until a pass meets no
   fault. Every pass must end with a documented status, no object left in the mock, no runtime allocation left, and
   (when the run succeeded) the output the mock computes. */
static int sweep_job(const struct job *job, const char *base_config) {
    struct mock_api mock;
    memset(&mock, 0, sizeof mock);
    if (!load_mock(&mock)) {
        fprintf(stderr, "TSUZURI_VULKAN_LIBRARY must name the mock library\n");
        return HARNESS_ERROR;
    }
    size_t spirv_size = 0, input_size = 0;
    void *spirv = read_file(job->spirv_path, &spirv_size);
    void *input = job->input_path != NULL ? read_file(job->input_path, &input_size) : NULL;
    if (spirv == NULL) return HARNESS_ERROR;
    size_t output_size = output_bytes_for(job);
    unsigned char *output = (unsigned char *)malloc(output_size > 0 ? output_size : 1);
    unsigned char *expected = NULL;
    int failures = 0, passes = 0;
    int counts[5] = {0, 0, 0, 0, 0};
    for (int fault = 1; fault < 4096; fault++) {
        char config[512];
        snprintf(config, sizeof config, "%s,fail=%d", base_config, fault);
        tz_vk_test_reset();
        if (!mock.configure(config)) {
            fprintf(stderr, "the mock rejects %s\n", config);
            failures++;
            break;
        }
        int32_t open_status = tz_vulkan_open(job->features);
        int32_t run_status = -1;
        if (open_status == 0) {
            memset(output, 0xA5, output_size);
            run_status = tz_vulkan_run(job->mode, 0, job->lanes, NULL, 0, spirv, (int32_t)spirv_size, input, job->count, output);
        }
        int injected = mock.injected();
        if (open_status == 0 && run_status != 0 && !tz_vk.poisoned) {
            /* A failed run leaves the backend usable: the next run, which meets no fault, succeeds. */
            memset(output, 0xA5, output_size);
            int32_t again = tz_vulkan_run(job->mode, 0, job->lanes, NULL, 0, spirv, (int32_t)spirv_size, input, job->count, output);
            if (again != 0) {
                fprintf(stderr, "fault %d: the run after a failed run returned %d\n", fault, (int)again);
                failures++;
            }
        }
        tz_vk_test_reset();
        passes++;
        int32_t status = open_status != 0 ? open_status : run_status;
        if (status < 0 || status > 4) {
            fprintf(stderr, "fault %d: status %d is not documented\n", fault, (int)status);
            failures++;
        } else {
            counts[status]++;
        }
        if (mock.leaked() != 0) {
            fprintf(stderr, "fault %d (open=%d run=%d): the runtime leaked Vulkan objects:\n%s\n", fault, (int)open_status,
                (int)run_status, mock.leak_report());
            failures++;
        }
        if (tz_live_allocations != 0) {
            fprintf(stderr, "fault %d: %ld runtime allocations are still live\n", fault, tz_live_allocations);
            failures++;
            tz_live_allocations = 0;
        }
        if (injected == 0) {
            if (status != 0) {
                fprintf(stderr, "fault %d: nothing was injected but the status is %d\n", fault, (int)status);
                failures++;
            } else {
                if (expected == NULL) {
                    expected = (unsigned char *)malloc(output_size > 0 ? output_size : 1);
                    memcpy(expected, output, output_size);
                }
            }
            break;
        }
        if (status == 0) {
            fprintf(stderr, "fault %d was injected and the run still succeeded\n", fault);
            failures++;
        }
    }
    printf("sweep passes=%d ok=%d unavailable=%d unsupported=%d limit=%d failed=%d failures=%d\n", passes, counts[0], counts[1],
        counts[2], counts[3], counts[4], failures);
    if (job->output_path != NULL && expected != NULL) write_file(job->output_path, expected, output_size);
    free(expected);
    free(output);
    free(spirv);
    free(input);
    return failures == 0 ? 0 : HARNESS_ERROR;
}

/* Opens the backend with `features` (`repeat` times) and prints the status and the capability line. With --spirv it then
   runs that kernel in the same process (a refused feature does not stop the kernels that do not need it) and prints
   `run=<status>`; on the mock it prints how many times the conformance probe was dispatched. */
static int probe_job(const struct job *job) {
    int32_t status = 0;
    for (int round = 0; round < (job->repeat < 1 ? 1 : job->repeat); round++) status = tz_vulkan_open(job->features);
    int32_t run_status = -1;
    if (job->spirv_path != NULL) {
        size_t spirv_size = 0, input_size = 0;
        void *spirv = read_file(job->spirv_path, &spirv_size);
        void *input = job->input_path != NULL ? read_file(job->input_path, &input_size) : NULL;
        size_t output_size = output_bytes_for(job);
        unsigned char *output = (unsigned char *)malloc(output_size > 0 ? output_size : 1);
        run_status = spirv != NULL && output != NULL
            ? tz_vulkan_run(job->mode, 0, job->lanes, NULL, 0, spirv, (int32_t)spirv_size, input, job->count, output)
            : HARNESS_ERROR;
        free(output);
        free(input);
        free(spirv);
    }
    char text[1024];
    tz_vulkan_describe(text, sizeof text);
    printf("status=%d %s", (int)status, text);
    if (run_status >= 0) printf(" run=%d", (int)run_status);
    struct mock_api mock;
    memset(&mock, 0, sizeof mock);
    if (getenv("TZ_VK_MOCK") != NULL && load_mock(&mock)) {
        printf(" probe_dispatches=%d created=%d opened=%d", mock.probe_dispatches(), mock.created(), mock.opened());
    }
    printf("\n");
    return (int)status;
}

/* Opens the backend and runs the module of the conformance probe whatever the device reports (the runtime itself runs
   it only where the properties report the controls and a strict kernel is wanted), then prints every lane against the
   bits that the CPU reference computes. The exit code is the status of the run; the last line counts the lanes that differ. */
static int conform_job(const struct job *job) {
    int32_t status = tz_vulkan_open(job->features & ~TZ_VK_FEATURE_STRICT_F32);
    char text[1024];
    tz_vulkan_describe(text, sizeof text);
    printf("status=%d %s\n", (int)status, text);
    if (status != 0) return (int)status;
    uint32_t results[TZ_VK_PROBE_LANES];
    memset(results, 0, sizeof results);
    status = tz_vk_test_conform(results);
    if (status != 0) {
        printf("conform status=%d\n", (int)status);
        return (int)status;
    }
    int different = 0;
    for (uint32_t lane = 0; lane < TZ_VK_PROBE_LANES; lane++) {
        int same = results[lane] == tz_vk_probe_expected[lane];
        different += !same;
        printf("lane %2u %-40s a=0x%08X b=0x%08X reference=0x%08X device=0x%08X %s\n", (unsigned)lane,
            tz_vk_probe_operations[lane / 3], (unsigned)tz_vk_probe_input[2 * lane], (unsigned)tz_vk_probe_input[2 * lane + 1],
            (unsigned)tz_vk_probe_expected[lane], (unsigned)results[lane], same ? "ok" : "DIFFERENT");
    }
    printf("conform lanes=%d different=%d\n", TZ_VK_PROBE_LANES, different);
    return 0;
}

/* Asks Gpu.Auto `repeat` times whether it would run the kernel on this backend, and prints the answers (1: here, 0: the
   CPU reference) after the capability line of the device that the first question opened, if it did. With --then-spirv the
   rounds of a second kernel follow (its own count, weight, and --then-repeat rounds), in the same process: the credit that
   the first kernel's calls built up and spent is what the second one finds. `credit` is the credit left at the end. */
static int ask_rounds(const struct job *job, const void *module, size_t size, int64_t count, int32_t weight, int rounds,
    char *answers, size_t capacity) {
    size_t used = 0;
    answers[0] = '\0';
    for (int round = 0; round < rounds && used + 3 < capacity; round++) {
        int chosen = tz_vulkan_auto(job->mode, job->lanes, job->features, module, (int32_t)size, weight, count);
        used += (size_t)snprintf(answers + used, capacity - used, "%s%d", round == 0 ? "" : ",", chosen);
    }
    return 0;
}

static int auto_job(const struct job *job) {
    size_t size = 0;
    void *module = job->spirv_path != NULL ? read_file(job->spirv_path, &size) : NULL;
    if (module == NULL) {
        fprintf(stderr, "cannot read the module %s\n", job->spirv_path != NULL ? job->spirv_path : "(none)");
        return HARNESS_ERROR;
    }
    char answers[2048], then_answers[2048];
    int first_status = job->open_first >= 0 ? (int)tz_vulkan_open(job->open_first) : -1;
    ask_rounds(job, module, size, job->count, job->weight, job->repeat, answers, sizeof answers);
    then_answers[0] = '\0';
    if (job->then_spirv_path != NULL) {
        size_t then_size = 0;
        void *then_module = read_file(job->then_spirv_path, &then_size);
        if (then_module == NULL) {
            fprintf(stderr, "cannot read the module %s\n", job->then_spirv_path);
            free(module);
            return HARNESS_ERROR;
        }
        ask_rounds(job, then_module, then_size, job->then_count, job->then_weight, job->then_repeat, then_answers, sizeof then_answers);
        free(then_module);
    }
    int after_status = job->open_after >= 0 ? (int)tz_vulkan_open(job->open_after) : -1;
    char text[1024];
    tz_vulkan_describe(text, sizeof text);
    printf("status=0 chosen=%s opened=%d", answers, text[0] != '\0');
    if (job->then_spirv_path != NULL) printf(" then=%s", then_answers);
    if (first_status >= 0) printf(" first=%d", first_status);
    if (after_status >= 0) printf(" after=%d", after_status);
    printf(" credit=%.0f", tz_vk_auto_credit);
    struct mock_api mock;
    memset(&mock, 0, sizeof mock);
    if (getenv("TZ_VK_MOCK") != NULL && load_mock(&mock)) printf(" created=%d mask=%d", mock.created(), mock.opened());
    const char *name = strstr(text, "device=\"");
    if (name != NULL) {
        char device[256];
        if (sscanf(name, "device=\"%255[^\"]\"", device) == 1) printf(" device=%s", device);
    }
    printf("\n");
    free(module);
    return 0;
}

/* The fault sweep of the Auto question: the Nth fallible Vulkan call fails, for N = 1, 2, ... until a pass meets no fault.
   A fault never makes the answer 1 (the CPU reference runs the call), and no Vulkan object or runtime allocation is left. */
static int autosweep_job(const struct job *job, const char *base_config) {
    struct mock_api mock;
    memset(&mock, 0, sizeof mock);
    if (!load_mock(&mock)) {
        fprintf(stderr, "TSUZURI_VULKAN_LIBRARY must name the mock library\n");
        return HARNESS_ERROR;
    }
    size_t size = 0;
    void *module = job->spirv_path != NULL ? read_file(job->spirv_path, &size) : NULL;
    if (module == NULL) return HARNESS_ERROR;
    int failures = 0, passes = 0, chosen_passes = 0;
    for (int fault = 1; fault < 4096; fault++) {
        char config[512];
        snprintf(config, sizeof config, "%s,fail=%d", base_config, fault);
        tz_vk_test_reset();
        if (!mock.configure(config)) {
            fprintf(stderr, "the mock rejects %s\n", config);
            failures++;
            break;
        }
        int chosen = tz_vulkan_auto(job->mode, job->lanes, job->features, module, (int32_t)size, job->weight, job->count);
        int injected = mock.injected();
        tz_vk_test_reset();
        passes++;
        if (mock.leaked() != 0) {
            fprintf(stderr, "fault %d: the runtime leaked Vulkan objects:\n%s\n", fault, mock.leak_report());
            failures++;
        }
        if (tz_live_allocations != 0) {
            fprintf(stderr, "fault %d: %ld runtime allocations are still live\n", fault, tz_live_allocations);
            failures++;
            tz_live_allocations = 0;
        }
        if (injected != 0 && chosen != 0) {
            fprintf(stderr, "fault %d was injected and Gpu.Auto still chose Vulkan\n", fault);
            failures++;
        }
        if (injected == 0) {
            chosen_passes += chosen;
            break;
        }
    }
    printf("autosweep passes=%d chosen=%d failures=%d\n", passes, chosen_passes, failures);
    free(module);
    return failures == 0 ? 0 : HARNESS_ERROR;
}

/* A kernel is running (the mock blocks its wait for the fence) while another thread asks Gpu.Auto about three calls: one
   the cost rule keeps on the CPU reference (10 lanes), one above the rule whose kernel (--then-spirv) has no pipeline yet
   and whose saving does not pay the first use, and a big call of the running kernel, which the rule sends to the device.
   None of them may wait for the kernel, so all three are answered while the kernel is still running, and the last one,
   which needs the state of the backend, is answered 1: only a state that is free while a kernel runs can say so. */
struct blocked_state {
    const struct job *job;
    const void *spirv;
    size_t spirv_size;
    const void *input;
    unsigned char *output;
    const void *then_module;
    size_t then_size;
    int32_t status;
    int answers[3];
    atomic_int run_done;
    atomic_int decisions_done;
};

static void sleep_ms(long milliseconds) {
    struct timespec pause = {milliseconds / 1000, (milliseconds % 1000) * 1000000L};
    nanosleep(&pause, NULL);
}

static void *blocked_run_main(void *argument) {
    struct blocked_state *state = (struct blocked_state *)argument;
    state->status = tz_vulkan_run(state->job->mode, 0, state->job->lanes, NULL, 0, state->spirv, (int32_t)state->spirv_size,
        state->input, state->job->count, state->output);
    atomic_store(&state->run_done, 1);
    return NULL;
}

static void *blocked_decide_main(void *argument) {
    struct blocked_state *state = (struct blocked_state *)argument;
    const struct job *job = state->job;
    state->answers[0] = tz_vulkan_auto(job->mode, job->lanes, job->features, state->spirv, (int32_t)state->spirv_size, 1, 10);
    state->answers[1] = tz_vulkan_auto(job->mode, job->lanes, job->features, state->then_module, (int32_t)state->then_size,
        job->then_weight, job->then_count);
    state->answers[2] = tz_vulkan_auto(job->mode, job->lanes, job->features, state->spirv, (int32_t)state->spirv_size, 1280, 4000000);
    atomic_store(&state->decisions_done, 1);
    return NULL;
}

static int blocked_job(const struct job *job) {
    struct mock_api mock;
    memset(&mock, 0, sizeof mock);
    if (!load_mock(&mock)) {
        fprintf(stderr, "TSUZURI_VULKAN_LIBRARY must name the mock library\n");
        return HARNESS_ERROR;
    }
    static struct blocked_state state;
    size_t input_size = 0;
    state.job = job;
    state.spirv = read_file(job->spirv_path, &state.spirv_size);
    state.then_module = read_file(job->then_spirv_path, &state.then_size);
    state.input = read_file(job->input_path, &input_size);
    state.output = (unsigned char *)malloc(output_bytes_for(job));
    if (state.spirv == NULL || state.then_module == NULL || state.input == NULL || state.output == NULL) return HARNESS_ERROR;
    int32_t open_status = tz_vulkan_open(job->features);
    if (open_status != 0) {
        printf("blocked open=%d\n", (int)open_status);
        return HARNESS_ERROR;
    }
    pthread_t runner, decider;
    pthread_create(&runner, NULL, blocked_run_main, &state);
    int entered = 0;
    for (int wait = 0; wait < 30000 && !(entered = mock.waits_entered() > 0); wait++) sleep_ms(1);
    int decided = 0;
    if (entered) {
        pthread_create(&decider, NULL, blocked_decide_main, &state);
        for (int wait = 0; wait < 30000 && !(decided = atomic_load(&state.decisions_done)); wait++) sleep_ms(1);
    }
    int still_running = !atomic_load(&state.run_done);
    mock.release_wait();
    if (entered) pthread_join(decider, NULL);
    pthread_join(runner, NULL);
    printf("blocked entered=%d decisions=%d,%d,%d decided_while_running=%d still_running=%d run=%d\n", entered,
        state.answers[0], state.answers[1], state.answers[2], decided, still_running, (int)state.status);
    free(state.output);
    free((void *)state.spirv);
    free((void *)state.then_module);
    free((void *)state.input);
    return entered && decided && still_running && state.status == 0 ? 0 : HARNESS_ERROR;
}

/* The strict float32 probe: what a running kernel does to the callers that need it. The mock blocks the wait for the fence of
   the kernel that thread A runs (--spirv, a plain integer kernel), so A holds the execution lock, and the backend was opened
   for integer kernels only, so no thread has run the probe yet. Then:
   blockedprobe: another thread asks Gpu.Auto (--then-repeat questions) about a strict float32 kernel (--then-spirv, with
     --features 4, --then-count and --then-weight) that the cost rule sends to the device. The probe needs the execution
     lock, so the question must not wait for the kernel: it is answered 0 while the kernel runs, no probe is dispatched, no
     verdict exists, and the credit that the compile (which did not happen) took out is back. After the kernel is done the
     next question runs the probe once, records the verdict, and is answered 1.
   blockedopen: another thread asks for the backend with --features (an explicit request that needs the probe). A request
     may wait: it stays inside (holding the state lock) until the kernel is done, and then it runs the probe once and records
     the verdict, so a device that fails the probe refuses the request. Meanwhile Gpu.Auto, asked about a call of the plain
     kernel (--then-count, --then-weight), finds the state lock taken and answers 0 without waiting. */
#define BLOCKED_WAIT_MS 30000

struct probe_wait_state {
    const struct job *job;
    const void *spirv;
    size_t spirv_size;
    const void *input;
    unsigned char *output;
    const void *strict_module;
    size_t strict_size;
    int32_t status;
    int32_t open_status;
    int answers[8];
    int answer_count;
    atomic_int run_done;
    atomic_int decisions_done;
    atomic_int open_done;
};

static int strict_probe_state(void) {
    TZ_VK_LOCK();
    int state = tz_vk.caps.strict_probe;
    TZ_VK_UNLOCK();
    return state;
}

static double auto_credit(void) {
    TZ_VK_LOCK();
    double credit = tz_vk_auto_credit;
    TZ_VK_UNLOCK();
    return credit;
}

static void *probe_wait_run_main(void *argument) {
    struct probe_wait_state *state = (struct probe_wait_state *)argument;
    state->status = tz_vulkan_run(state->job->mode, 0, state->job->lanes, NULL, 0, state->spirv, (int32_t)state->spirv_size,
        state->input, state->job->count, state->output);
    atomic_store(&state->run_done, 1);
    return NULL;
}

static void *probe_wait_decide_main(void *argument) {
    struct probe_wait_state *state = (struct probe_wait_state *)argument;
    const struct job *job = state->job;
    int questions = job->then_repeat > 0 && job->then_repeat <= 8 ? job->then_repeat : 1;
    for (int index = 0; index < questions; index++) {
        state->answers[index] = tz_vulkan_auto(job->mode, job->lanes, job->features, state->strict_module,
            (int32_t)state->strict_size, job->then_weight, job->then_count);
    }
    state->answer_count = questions;
    atomic_store(&state->decisions_done, 1);
    return NULL;
}

static void *probe_wait_open_main(void *argument) {
    struct probe_wait_state *state = (struct probe_wait_state *)argument;
    state->open_status = tz_vulkan_open(state->job->features);
    atomic_store(&state->open_done, 1);
    return NULL;
}

/* The files and the first open that both modes share; returns 0 when the state is ready. */
static int probe_wait_setup(const struct job *job, struct probe_wait_state *state, struct mock_api *mock) {
    memset(mock, 0, sizeof *mock);
    if (!load_mock(mock)) {
        fprintf(stderr, "TSUZURI_VULKAN_LIBRARY must name the mock library\n");
        return HARNESS_ERROR;
    }
    size_t input_size = 0;
    state->job = job;
    state->spirv = read_file(job->spirv_path, &state->spirv_size);
    state->strict_module = job->then_spirv_path != NULL ? read_file(job->then_spirv_path, &state->strict_size) : NULL;
    state->input = read_file(job->input_path, &input_size);
    state->output = (unsigned char *)malloc(output_bytes_for(job));
    if (state->spirv == NULL || state->input == NULL || state->output == NULL) return HARNESS_ERROR;
    return 0;
}

static void probe_wait_release(struct probe_wait_state *state) {
    free(state->output);
    free((void *)state->spirv);
    free((void *)state->strict_module);
    free((void *)state->input);
}

static int blockedprobe_job(const struct job *job) {
    static struct probe_wait_state state;
    struct mock_api mock;
    if (probe_wait_setup(job, &state, &mock) != 0 || state.strict_module == NULL) return HARNESS_ERROR;
    int32_t open_status = tz_vulkan_open(0);
    if (open_status != 0) {
        printf("blockedprobe open=%d\n", (int)open_status);
        return HARNESS_ERROR;
    }
    pthread_t runner, decider;
    pthread_create(&runner, NULL, probe_wait_run_main, &state);
    int entered = 0;
    for (int wait = 0; wait < BLOCKED_WAIT_MS && !(entered = mock.waits_entered() > 0); wait++) sleep_ms(1);
    int decided = 0;
    if (entered) {
        pthread_create(&decider, NULL, probe_wait_decide_main, &state);
        for (int wait = 0; wait < BLOCKED_WAIT_MS && !(decided = atomic_load(&state.decisions_done)); wait++) sleep_ms(1);
    }
    int still_running = !atomic_load(&state.run_done);
    int dispatches_during = mock.probe_dispatches();
    int probe_during = strict_probe_state();
    double credit_during = auto_credit();
    mock.release_wait();
    if (entered) pthread_join(decider, NULL);
    pthread_join(runner, NULL);
    int after[2];
    for (int index = 0; index < 2; index++) {
        after[index] = tz_vulkan_auto(job->mode, job->lanes, job->features, state.strict_module, (int32_t)state.strict_size,
            job->then_weight, job->then_count);
    }
    printf("blockedprobe entered=%d during=", entered);
    for (int index = 0; index < state.answer_count; index++) printf("%s%d", index == 0 ? "" : ",", state.answers[index]);
    printf(" decided_while_running=%d still_running=%d run=%d dispatches_during=%d probe_during=%d credit_during=%.0f after=%d,%d "
           "dispatches_after=%d probe_after=%d credit_after=%.0f\n",
        decided, still_running, (int)state.status, dispatches_during, probe_during, credit_during, after[0], after[1],
        mock.probe_dispatches(), strict_probe_state(), auto_credit());
    probe_wait_release(&state);
    return entered && decided && still_running && state.status == 0 ? 0 : HARNESS_ERROR;
}

static int blockedopen_job(const struct job *job) {
    static struct probe_wait_state state;
    struct mock_api mock;
    if (probe_wait_setup(job, &state, &mock) != 0) return HARNESS_ERROR;
    int32_t open_status = tz_vulkan_open(0);
    if (open_status != 0) {
        printf("blockedopen open=%d\n", (int)open_status);
        return HARNESS_ERROR;
    }
    pthread_t runner, requester;
    pthread_create(&runner, NULL, probe_wait_run_main, &state);
    int entered = 0;
    for (int wait = 0; wait < BLOCKED_WAIT_MS && !(entered = mock.waits_entered() > 0); wait++) sleep_ms(1);
    int inside = 0, returned_early = 0, auto_while_waiting = -1;
    if (entered) {
        pthread_create(&requester, NULL, probe_wait_open_main, &state);
        /* The request is inside when it holds the state lock (it keeps it while it waits for the kernel), or it is done. */
        for (int wait = 0; wait < BLOCKED_WAIT_MS && !inside && !atomic_load(&state.open_done); wait++) {
            if (TZ_VK_TRY_LOCK()) {
                TZ_VK_UNLOCK();
                sleep_ms(1);
            } else {
                inside = 1;
            }
        }
        /* A request that wrongly gives up the wait returns within this time, which only makes the failure visible: the
           runtime that waits is not timing-sensitive, it simply does not return before the kernel is done. */
        for (int grace = 0; grace < 20 && !atomic_load(&state.open_done); grace++) sleep_ms(10);
        returned_early = atomic_load(&state.open_done);
        auto_while_waiting = tz_vulkan_auto(job->mode, job->lanes, 0, state.spirv, (int32_t)state.spirv_size, job->then_weight,
            job->then_count);
    }
    int still_running = !atomic_load(&state.run_done);
    int dispatches_during = mock.probe_dispatches();
    mock.release_wait();
    if (entered) pthread_join(requester, NULL);
    pthread_join(runner, NULL);
    printf("blockedopen entered=%d inside=%d returned_early=%d auto_while_waiting=%d still_running=%d run=%d open=%d "
           "dispatches_during=%d dispatches=%d probe=%d\n",
        entered, inside, returned_early, auto_while_waiting, still_running, (int)state.status, (int)state.open_status,
        dispatches_during, mock.probe_dispatches(), strict_probe_state());
    probe_wait_release(&state);
    return entered && inside && still_running && state.status == 0 ? 0 : HARNESS_ERROR;
}

/* The device is lost during the first wait for a fence (the mock's lose=wait). That run fails, and nothing after it touches
   the device: another run, another open, and a Gpu.Auto question are all answered without one more Vulkan call. */
static int lost_job(const struct job *job) {
    struct mock_api mock;
    memset(&mock, 0, sizeof mock);
    if (!load_mock(&mock)) {
        fprintf(stderr, "TSUZURI_VULKAN_LIBRARY must name the mock library\n");
        return HARNESS_ERROR;
    }
    size_t spirv_size = 0, input_size = 0;
    void *spirv = read_file(job->spirv_path, &spirv_size);
    void *input = read_file(job->input_path, &input_size);
    unsigned char *output = (unsigned char *)malloc(output_bytes_for(job));
    if (spirv == NULL || input == NULL || output == NULL) return HARNESS_ERROR;
    int32_t open_status = tz_vulkan_open(job->features);
    int32_t first = tz_vulkan_run(job->mode, 0, job->lanes, NULL, 0, spirv, (int32_t)spirv_size, input, job->count, output);
    int poisoned = atomic_load(&tz_vk.poisoned);
    int calls_before = mock.calls();
    int32_t second = tz_vulkan_run(job->mode, 0, job->lanes, NULL, 0, spirv, (int32_t)spirv_size, input, job->count, output);
    int32_t reopen = tz_vulkan_open(job->features);
    int chosen = tz_vulkan_auto(job->mode, job->lanes, job->features, spirv, (int32_t)spirv_size, job->weight, job->count);
    int calls_after = mock.calls();
    tz_vk_test_reset();
    printf("lost open=%d first=%d poisoned=%d second=%d reopen=%d auto=%d calls_before=%d calls_after=%d leaked=%d\n",
        (int)open_status, (int)first, poisoned, (int)second, (int)reopen, chosen, calls_before, calls_after, mock.leaked());
    free(output);
    free(input);
    free(spirv);
    return 0;
}

/* More distinct kernels than the program cache holds (the id bound of the base module is changed, so each is a module of
   its own): every run succeeds with the right output, the kernels beyond the cache get a pipeline of their own that is
   released after the run, and the cache keeps exactly its 256 pipelines. */
static int programs_job(const struct job *job) {
    struct mock_api mock;
    memset(&mock, 0, sizeof mock);
    if (!load_mock(&mock)) {
        fprintf(stderr, "TSUZURI_VULKAN_LIBRARY must name the mock library\n");
        return HARNESS_ERROR;
    }
    size_t size = 0, input_size = 0;
    void *base = read_file(job->spirv_path, &size);
    uint32_t *input = (uint32_t *)read_file(job->input_path, &input_size);
    uint32_t *output = (uint32_t *)malloc((size_t)job->count * 4);
    if (base == NULL || input == NULL || output == NULL || tz_vulkan_open(job->features) != 0) return HARNESS_ERROR;
    int ok = 0, wrong = 0;
    for (int pass = 0; pass < 2; pass++) {
        for (int index = 0; index < job->programs; index++) {
            uint32_t *module = (uint32_t *)malloc(size);
            memcpy(module, base, size);
            module[3] = 64 + (uint32_t)index;
            memset(output, 0xA5, (size_t)job->count * 4);
            int32_t status = tz_vulkan_run(0, 0, job->lanes, NULL, 0, module, (int32_t)size, input, job->count, output);
            free(module);
            if (status != 0) continue;
            ok++;
            for (int64_t lane = 0; lane < job->count; lane++) {
                if (output[lane] != input[lane] * 3U + 7U) wrong++;
            }
        }
    }
    printf("programs runs=%d ok=%d wrong=%d pipelines=%d\n", 2 * job->programs, ok, wrong, mock.live_pipelines());
    free(output);
    free(input);
    free(base);
    return 0;
}

int main(int argc, char **argv) {
    if (argc < 2) {
        fprintf(stderr, "usage: harness probe|conform|run|bench|sweep|auto|autosweep|blocked|blockedprobe|blockedopen|lost|programs [--spirv F --mode map|init --lanes IN,OUT --count N ...]\n");
        return HARNESS_ERROR;
    }
    struct job job;
    if (!parse_job(argc, argv, &job)) return HARNESS_ERROR;
    const char *mock_config = getenv("TZ_VK_MOCK");
    int sweeping = strcmp(argv[1], "sweep") == 0 || strcmp(argv[1], "autosweep") == 0;
    if (mock_config != NULL && !sweeping) {
        struct mock_api mock;
        memset(&mock, 0, sizeof mock);
        if (!load_mock(&mock) || !mock.configure(mock_config)) {
            fprintf(stderr, "TZ_VK_MOCK needs the mock library in TSUZURI_VULKAN_LIBRARY and a valid configuration\n");
            return HARNESS_ERROR;
        }
    }
    int code;
    if (strcmp(argv[1], "probe") == 0) {
        code = probe_job(&job);
    } else if (strcmp(argv[1], "conform") == 0) {
        code = conform_job(&job);
    } else if (strcmp(argv[1], "run") == 0) {
        code = run_job(&job);
    } else if (strcmp(argv[1], "bench") == 0) {
        code = bench_job(&job);
    } else if (strcmp(argv[1], "sweep") == 0) {
        code = sweep_job(&job, mock_config != NULL ? mock_config : "");
    } else if (strcmp(argv[1], "autosweep") == 0) {
        code = autosweep_job(&job, mock_config != NULL ? mock_config : "");
    } else if (strcmp(argv[1], "auto") == 0) {
        code = auto_job(&job);
    } else if (strcmp(argv[1], "blocked") == 0) {
        code = blocked_job(&job);
    } else if (strcmp(argv[1], "blockedprobe") == 0) {
        code = blockedprobe_job(&job);
    } else if (strcmp(argv[1], "blockedopen") == 0) {
        code = blockedopen_job(&job);
    } else if (strcmp(argv[1], "lost") == 0) {
        code = lost_job(&job);
    } else if (strcmp(argv[1], "programs") == 0) {
        code = programs_job(&job);
    } else {
        fprintf(stderr, "unknown mode %s\n", argv[1]);
        return HARNESS_ERROR;
    }
    tz_vk_test_reset();
    if (tz_live_allocations != 0) {
        fprintf(stderr, "%ld runtime allocations are still live after teardown\n", tz_live_allocations);
        return HARNESS_ERROR;
    }
    return code;
}
