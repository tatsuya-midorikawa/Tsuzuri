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
    if (job->assume_strict) tz_vk.caps.strict_f32 = 1;
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
typedef const char *(*mock_text_function)(void);

struct mock_api {
    void *handle;
    mock_configure_function configure;
    mock_query_function injected;
    mock_query_function leaked;
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
    mock->leak_report = (mock_text_function)dlsym(mock->handle, "tz_vk_mock_leak_report");
    return mock->configure != NULL && mock->injected != NULL && mock->leaked != NULL && mock->leak_report != NULL;
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

/* Opens the backend with `features` once and prints the status and the capability line. */
static int probe_job(const struct job *job) {
    int32_t status = tz_vulkan_open(job->features);
    char text[1024];
    tz_vulkan_describe(text, sizeof text);
    printf("status=%d %s\n", (int)status, text);
    return (int)status;
}

/* Asks Gpu.Auto `repeat` times whether it would run the kernel on this backend, and prints the answers (1: here, 0: the
   CPU reference) after the capability line of the device that the first question opened, if it did. */
static int auto_job(const struct job *job) {
    size_t size = 0;
    void *module = job->spirv_path != NULL ? read_file(job->spirv_path, &size) : NULL;
    if (module == NULL) {
        fprintf(stderr, "cannot read the module %s\n", job->spirv_path != NULL ? job->spirv_path : "(none)");
        return HARNESS_ERROR;
    }
    char answers[256] = "";
    size_t used = 0;
    for (int round = 0; round < job->repeat && used + 3 < sizeof answers; round++) {
        int chosen = tz_vulkan_auto(job->mode, job->lanes, job->features, module, (int32_t)size, job->weight, job->count);
        used += (size_t)snprintf(answers + used, sizeof answers - used, "%s%d", round == 0 ? "" : ",", chosen);
    }
    char text[1024];
    tz_vulkan_describe(text, sizeof text);
    printf("status=0 chosen=%s opened=%d\n", answers, text[0] != '\0');
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

int main(int argc, char **argv) {
    if (argc < 2) {
        fprintf(stderr, "usage: harness probe|run|bench|sweep|auto|autosweep [--spirv F --mode map|init --lanes IN,OUT --count N ...]\n");
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
