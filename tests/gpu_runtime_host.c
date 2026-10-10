// A C host for the WebGPU backend of src/runtime/gpu.c (F09 Phase 2 review fixes). It calls tsuzuri_gpu_open and
// tsuzuri_gpu_run directly, so the failures that a language program only sees as a trap (status 3 and 4, a broken
// kernel, a device that does not answer) can be checked as statuses, and a process that wgpu-native aborts shows as
// a failed run. tests/gpu_runtime.mjs builds it with gpu.c, against a fake library (tests/gpu_runtime_fake.c) and,
// with TSUZURI_WEBGPU=1, against the real wgpu-native:
//   cc tests/gpu_runtime_host.c src/runtime/gpu.c -o host && host <scenario>
// Scenarios: open, basic, half, failures, limits, maximum, stall, run. A failed check aborts the host; a good run prints one line.
#undef NDEBUG
#include <assert.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#if !defined(_WIN32)
#include <time.h>
#endif

int32_t tsuzuri_gpu_open(int32_t backend, int32_t features);
int32_t tsuzuri_gpu_run(int32_t backend, int32_t mode, int32_t flags, int32_t lanes, const void *wgsl, int32_t wgsl_length, const void *spirv, int32_t spirv_length, const void *input, int64_t count, void *output);

enum { OK = 0, UNAVAILABLE = 1, UNSUPPORTED = 2, LIMIT = 3, FAILED = 4 };
#define LANES_I32 0x0101

// The header of every kernel: the fake library recognizes the kernel by the first line and computes what the real
// kernel does. `map_main` doubles its input, `init_main` writes 3 * index + 1.
#define HEADER \
    "struct Params { count: u32, a: u32, b: u32, c: u32 };\n" \
    "@group(0) @binding(1) var<storage, read_write> output_values: array<i32>;\n" \
    "@group(0) @binding(2) var<uniform> params: Params;\n"
#define INPUT "@group(0) @binding(0) var<storage, read> input_values: array<i32>;\n"
#define MAP(expression) \
    "@compute @workgroup_size(256)\nfn map_main(@builtin(global_invocation_id) invocation: vec3<u32>) {\n" \
    "  if (invocation.x < params.count) { output_values[invocation.x] = " expression "; }\n}\n"
#define INIT \
    "@compute @workgroup_size(256)\nfn init_main(@builtin(global_invocation_id) invocation: vec3<u32>) {\n" \
    "  if (invocation.x < params.count) { output_values[invocation.x] = i32(invocation.x) * 3 + 1; }\n}\n"

// The runtime keeps a program by the address of its source, so each kernel is one static text.
static const char kernel[] = "// fake: double\n" HEADER INPUT MAP("input_values[invocation.x] * 2") INIT;
// Valid WGSL without the entry point of init mode: the module is fine, the init pipeline is not.
static const char no_init[] = "// fake: double no-init\n" HEADER INPUT MAP("input_values[invocation.x] * 2");
// WGSL that no implementation accepts.
static const char broken[] = "// fake: invalid shader\n" HEADER "fn map_main( {\n";
// Valid WGSL whose map entry does not use the input binding: the bind group that the runtime makes for it (three
// bindings) has an entry that the pipeline's layout does not have.
static const char two_bindings[] = "// fake: two bindings\n" HEADER MAP("i32(invocation.x)") INIT;
// 16-bit lanes (kind 3): map copies its input. It needs the shader-f16 feature of the device.
static const char copy16[] =
    "// fake: copy16\nenable f16;\n"
    "struct Params { count: u32, a: u32, b: u32, c: u32 };\n"
    "@group(0) @binding(0) var<storage, read> input_values: array<f16>;\n"
    "@group(0) @binding(1) var<storage, read_write> output_values: array<f16>;\n"
    "@group(0) @binding(2) var<uniform> params: Params;\n"
    "@compute @workgroup_size(256)\nfn map_main(@builtin(global_invocation_id) invocation: vec3<u32>) {\n"
    "  if (invocation.x < params.count) { output_values[invocation.x] = input_values[invocation.x]; }\n}\n"
    "@compute @workgroup_size(256)\nfn init_main(@builtin(global_invocation_id) invocation: vec3<u32>) {\n"
    "  if (invocation.x < params.count) { output_values[invocation.x] = f16(invocation.x & 255u); }\n}\n";

static int32_t run(int mode, const char *source, int64_t count, const int32_t *input, int32_t *output) {
    return tsuzuri_gpu_run(1, mode, 0, LANES_I32, source, source ? (int32_t)strlen(source) : 0, NULL, 0, input, count, output);
}

static int32_t *lanes_of(int64_t count) {
    int32_t *values = malloc((size_t)count * sizeof *values);
    assert(values);
    for (int64_t index = 0; index < count; index++) values[index] = (int32_t)(index * 7 - 3);
    return values;
}

// One call of `count` lanes: map (0) or init (1) of `kernel`. It must end with the status `expected`, and when that is
// OK, with the right lanes. A call that is refused for its size (LIMIT) touches no lane, so it gets small buffers; every
// other call, a failing one too, may read its input and write its output before it fails.
static void call(int mode, int64_t count, int expected) {
    int64_t room = expected == LIMIT ? 4 : count;
    int32_t *input = mode ? NULL : lanes_of(room);
    int32_t *output = calloc((size_t)room, sizeof *output);
    assert(output);
    int32_t status = run(mode, kernel, count, input, output);
    assert(status == expected);
    if (status == OK) {
        for (int64_t index = 0; index < count; index++) assert(output[index] == (mode ? (int32_t)index * 3 + 1 : input[index] * 2));
    }
    free(input);
    free(output);
}
static void expect_map(int64_t count) { call(0, count, OK); }
static void expect_init(int64_t count) { call(1, count, OK); }

// Every size around a workgroup, twice each (the second call finds the pipeline in the program cache).
static void basic(void) {
    assert(tsuzuri_gpu_open(1, 0) == OK);
    static const int64_t counts[] = {1, 3, 255, 256, 257, 1000, 4097, 70000};
    for (int repeat = 0; repeat < 2; repeat++)
        for (size_t index = 0; index < sizeof counts / sizeof *counts; index++) {
            expect_map(counts[index]);
            expect_init(counts[index]);
        }
    printf("basic: ok\n");
}

// Broken kernels and over-limit calls end in a status, never in an abort of the process, and the device stays usable.
static void failures(void) {
    assert(tsuzuri_gpu_open(1, 0) == OK);
    int32_t input[300], output[300];
    for (int index = 0; index < 300; index++) input[index] = index;
    // Each is tried twice: a module or a pipeline that failed is not kept, so the second call fails like the first
    // and does not submit an object that the library marked invalid.
    for (int repeat = 0; repeat < 2; repeat++) {
        assert(run(1, no_init, 300, NULL, output) == FAILED);
        assert(run(0, broken, 300, input, output) == FAILED);
        assert(run(0, two_bindings, 300, input, output) == FAILED);
    }
    // The map entry of the kernel without init_main works, and so does everything after the failures.
    assert(run(0, no_init, 300, input, output) == OK);
    for (int index = 0; index < 300; index++) assert(output[index] == index * 2);
    expect_map(1000);
    expect_init(1000);
    // A call without a kernel is unsupported.
    assert(run(0, NULL, 300, input, output) == UNSUPPORTED);
    printf("failures: ok\n");
}

// The device's limits, not the adapter's: a dispatch of 65,536 workgroups (the limit is 65,535) and a buffer over
// maxStorageBufferBindingSize are refused with status 3 before anything is created or submitted.
static void limits(void) {
    assert(tsuzuri_gpu_open(1, 0) == OK);
    int32_t input[4] = {1, 2, 3, 4}, output[4];
    assert(run(0, kernel, 65535 * 256 + 1, input, output) == LIMIT);
    assert(run(1, kernel, 65535 * 256 + 1, NULL, output) == LIMIT);
    assert(run(0, kernel, 80000000, input, output) == LIMIT);
    assert(run(1, kernel, 2147483647, NULL, output) == LIMIT);
    // 16-bit lanes (kind 3) need half the bytes but the same workgroups.
    assert(tsuzuri_gpu_run(1, 1, 0, 0x0301, kernel, (int32_t)strlen(kernel), NULL, 0, NULL, 65535 * 256 + 1, output) == LIMIT);
    expect_map(5);
    printf("limits: ok\n");
}

// The largest call that a device with the default limits takes: 65,535 workgroups of 256 invocations.
static void maximum(void) {
    assert(tsuzuri_gpu_open(1, 0) == OK);
    expect_map(65535 * 256);
    expect_init(65535 * 256);
    printf("maximum: ok\n");
}

#if !defined(_WIN32)
static double seconds(void) {
    struct timespec now;
    clock_gettime(CLOCK_MONOTONIC, &now);
    return (double)now.tv_sec + (double)now.tv_nsec / 1e9;
}

// With the fake library, the first readback never completes (FAKE_WGPU=stall-map) and the host is built with a timeout
// of 400 ms (-DTZ_WGPU_TIMEOUT_MS=400): the run gives up with status 4, and the device is not asked again.
static void stall(void) {
    assert(tsuzuri_gpu_open(1, 0) == OK);
    int32_t input[300], output[300];
    for (int index = 0; index < 300; index++) input[index] = index;
    double start = seconds();
    assert(run(0, kernel, 300, input, output) == FAILED);
    double first = seconds() - start;
    assert(first >= 0.35 && first < 5.0);
    // Two more calls fail at once: if each waited again, they would take 0.8 s.
    start = seconds();
    assert(run(0, kernel, 300, input, output) == FAILED);
    assert(run(1, kernel, 300, NULL, output) == FAILED);
    assert(seconds() - start < 0.6);
    printf("stall: ok\n");
}
#endif

// 16-bit lanes (kind 3) in counts that are not a multiple of two: the upload and the readback end in half a word, which the
// runtime pads on the way in and trims on the way out. The output has guard lanes, which nothing may write.
static void half(void) {
    int32_t status = tsuzuri_gpu_open(1, 1);
    if (status == UNSUPPORTED) {
        printf("half: skipped (the adapter has no shader-f16)\n");
        return;
    }
    assert(status == OK);
    static const int64_t counts[] = {1, 3, 255, 257, 1001, 70001};
    for (size_t index = 0; index < sizeof counts / sizeof *counts; index++) {
        int64_t count = counts[index];
        uint16_t *input = malloc((size_t)count * sizeof *input);
        uint16_t *output = malloc((size_t)(count + 4) * sizeof *output);
        assert(input && output);
        // Finite f16 values, so that a copy keeps every bit.
        for (int64_t lane = 0; lane < count; lane++) input[lane] = (uint16_t)(0x3c00 | (lane & 0x3ff));
        for (int64_t lane = 0; lane < count + 4; lane++) output[lane] = 0xa5a5;
        assert(tsuzuri_gpu_run(1, 0, 0, 0x0303, copy16, (int32_t)strlen(copy16), NULL, 0, input, count, output) == OK);
        for (int64_t lane = 0; lane < count; lane++) assert(output[lane] == input[lane]);
        for (int64_t lane = count; lane < count + 4; lane++) assert(output[lane] == 0xa5a5);
        free(input);
        free(output);
    }
    printf("half: ok\n");
}

// The calls that the arguments name, each `<map|init> <lanes> <status>`: the injection tests run a failing call and then
// check that the next one fails or works as the device allows.
static void sequence(int argc, char **argv) {
    assert(tsuzuri_gpu_open(1, 0) == OK);
    assert(argc % 3 == 0);
    for (int index = 0; index < argc; index += 3) call(strcmp(argv[index], "init") == 0, atoll(argv[index + 1]), atoi(argv[index + 2]));
    printf("sequence: ok\n");
}

// The status of the open alone: the loader tests run it with a library in the working directory, with candidates of their own.
static int open_only(void) {
    int32_t status = tsuzuri_gpu_open(1, 0);
    printf("open: %d\n", (int)status);
    return status;
}

int main(int argc, char **argv) {
    const char *scenario = argc > 1 ? argv[1] : "basic";
    if (strcmp(scenario, "open") == 0) return open_only();
    if (strcmp(scenario, "basic") == 0) basic();
    else if (strcmp(scenario, "failures") == 0) failures();
    else if (strcmp(scenario, "limits") == 0) limits();
    else if (strcmp(scenario, "maximum") == 0) maximum();
    else if (strcmp(scenario, "half") == 0) half();
    else if (strcmp(scenario, "run") == 0) sequence(argc - 2, argv + 2);
#if !defined(_WIN32)
    else if (strcmp(scenario, "stall") == 0) stall();
#endif
    else {
        fprintf(stderr, "unknown scenario %s\n", scenario);
        return 2;
    }
    return 0;
}
