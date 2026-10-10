// A fake wgpu-native 29 for tests/gpu_runtime.mjs (F09 Phase 2 review fixes). It implements the part of webgpu.h that
// src/runtime/gpu.c loads, with real memory for the buffers, a tiny kernel interpreter (the kernels of
// tests/gpu_runtime_host.c), and the behaviors that make wgpu-native dangerous for a caller that does not check:
//   - an object that fails to be created is still returned, marked invalid, and the error comes through the
//     uncaptured-error callback during the call;
//   - a write to an invalid buffer, or a submit of an invalid command buffer, aborts the process (wgpu-native panics
//     inside an extern "C" function there), and so does a dispatch of more workgroups than the device's limit;
//   - a blocking wgpuDevicePoll waits for the device, so it never returns when the device does not answer.
// FAKE_WGPU holds a comma separated list of what to inject: stall-map (the first buffer map never completes),
// stall-adapter (no adapter ever arrives), fail-buffer=N (the Nth buffer fails), fail-bindgroup, fail-pipeline,
// fail-finish, lose-device (the device is lost after the first submit), max-buffer=N, max-groups=N (the device's limits;
// the adapter reports much larger ones, as a real adapter does). FAKE_WGPU_REPORT makes the library print the number of
// objects that are still alive when the process ends.
//
// The library includes gpu.c for its ABI types, so the fake cannot disagree with the runtime about a struct layout.
#include "../src/runtime/gpu.c"

#undef NDEBUG
#include <assert.h>

#define FAKE_EXPORT __attribute__((visibility("default")))

enum { K_INSTANCE, K_ADAPTER, K_DEVICE, K_QUEUE, K_BUFFER, K_MODULE, K_PIPELINE, K_LAYOUT, K_GROUP, K_ENCODER, K_PASS, K_COMMANDS, K_KINDS };
static const char *const kind_names[K_KINDS] = {"instances", "adapters", "devices", "queues", "buffers", "modules", "pipelines", "layouts", "groups", "encoders", "passes", "commands"};

typedef struct Fake Fake;
struct Fake {
    int kind;
    int invalid;
    int refs;
    // buffer
    uint64_t size;
    unsigned char *data;
    int mapped;
    // shader module
    int has_map, has_init, doubles, copies16, two_bindings;
    // pipeline (its module and mode) and layout (the bindings it has: bit n is binding n)
    Fake *module;
    int mode;
    unsigned bindings;
    // bind group: the buffer of each binding
    Fake *bound[3];
    // encoder (and a command buffer made from it): the pass that records, what it recorded, and the copy
    Fake *owner;
    Fake *pipeline, *group, *copy_from, *copy_to;
    uint64_t groups, copy_size;
};

static int live[K_KINDS];
static int buffers_created;
static int submits;
static int device_lost_pending;
static int lost_reported;
static WGPUUncapturedErrorCallbackInfo uncaptured;
static WGPUDeviceLostCallbackInfo lost_info;
static Fake *the_device;

// ---- Injection -------------------------------------------------------------------------------------------------

static const char *token(const char *name) {
    const char *list = getenv("FAKE_WGPU");
    size_t length = strlen(name);
    while (list && *list) {
        const char *end = strchr(list, ',');
        size_t span = end ? (size_t)(end - list) : strlen(list);
        if (span >= length && strncmp(list, name, length) == 0 && (span == length || list[length] == '=')) return list + length;
        list = end ? end + 1 : NULL;
    }
    return NULL;
}
static int flag(const char *name) { return token(name) != NULL; }
static long number(const char *name, long fallback) {
    const char *value = token(name);
    return value && value[0] == '=' ? atol(value + 1) : fallback;
}

static void error(const char *message) {
    if (uncaptured.callback) {
        WGPUDevice device = (WGPUDevice)the_device;
        WGPUStringView text = {message, strlen(message)};
        uncaptured.callback(&device, 1, text, uncaptured.userdata1, uncaptured.userdata2);
    }
}

// wgpu-native panics inside an extern "C" function there, which aborts the process.
__attribute__((noreturn)) static void panic(const char *message) {
    fprintf(stderr, "fake wgpu: panic: %s\n", message);
    abort();
}

static void report(void) {
    if (!getenv("FAKE_WGPU_REPORT")) return;
    fprintf(stderr, "fake wgpu: alive");
    for (int kind = 0; kind < K_KINDS; kind++) fprintf(stderr, " %s=%d", kind_names[kind], live[kind]);
    fprintf(stderr, "\n");
}

static Fake *make(int kind) {
    Fake *object = calloc(1, sizeof *object);
    assert(object);
    object->kind = kind;
    object->refs = 1;
    live[kind]++;
    return object;
}

static void release(void *handle) {
    Fake *object = handle;
    if (!object) return;
    assert(object->refs > 0 && "an object was released more often than it was created");
    if (--object->refs > 0) return;
    live[object->kind]--;
    if (object->kind == K_PIPELINE) release(object->module);
    free(object->data);
    free(object);
}

// ---- Events ------------------------------------------------------------------------------------------------------

struct Pending {
    int type; // 0 adapter, 1 device, 2 map
    int held;
    WGPURequestAdapterCallbackInfo adapter;
    WGPURequestDeviceCallbackInfo device;
    WGPUBufferMapCallbackInfo map;
    Fake *object;
};
static struct Pending pending[16];
static int pending_count;

static struct Pending *enqueue(int type, int held) {
    assert(pending_count < 16);
    struct Pending *entry = &pending[pending_count++];
    memset(entry, 0, sizeof *entry);
    entry->type = type;
    entry->held = held;
    return entry;
}

static void deliver(void) {
    if (device_lost_pending && !lost_reported) {
        lost_reported = 1;
        WGPUDevice device = (WGPUDevice)the_device;
        WGPUStringView text = {"fake: the device was lost", 25};
        if (lost_info.callback) lost_info.callback(&device, 1, text, lost_info.userdata1, lost_info.userdata2);
    }
    while (pending_count > 0 && !pending[0].held) {
        struct Pending entry = pending[0];
        memmove(&pending[0], &pending[1], (size_t)(pending_count - 1) * sizeof pending[0]);
        pending_count--;
        WGPUStringView none = {NULL, 0};
        if (entry.type == 0) {
            entry.adapter.callback(WGPURequestAdapterStatus_Success, (WGPUAdapter)entry.object, none, entry.adapter.userdata1, entry.adapter.userdata2);
        } else if (entry.type == 1) {
            entry.device.callback(WGPURequestDeviceStatus_Success, (WGPUDevice)entry.object, none, entry.device.userdata1, entry.device.userdata2);
        } else {
            entry.object->mapped = 1;
            entry.map.callback(WGPUMapAsyncStatus_Success, none, entry.map.userdata1, entry.map.userdata2);
            release(entry.object);
        }
    }
}

// A request that no one waits for any more: wgpu-native completes a pending request with a status other than success when the
// instance is released, and a request that came late and succeeded owns an adapter or a device that nothing else will release.
static void deliver_cancelled(struct Pending *entry) {
    WGPUStringView none = {NULL, 0};
    if (entry->type == 0) {
        entry->adapter.callback(2 /* InstanceDropped */, NULL, none, entry->adapter.userdata1, entry->adapter.userdata2);
        release(entry->object);
    } else if (entry->type == 1) {
        entry->device.callback(2 /* InstanceDropped */, NULL, none, entry->device.userdata1, entry->device.userdata2);
        release(entry->object);
    } else {
        entry->map.callback(2 /* InstanceDropped */, none, entry->map.userdata1, entry->map.userdata2);
        release(entry->object);
    }
}

static void wait_while_held(void) {
    while (pending_count > 0 && pending[0].held) {
        struct timespec pause = {1, 0};
        nanosleep(&pause, NULL);
    }
}

// ---- The API -----------------------------------------------------------------------------------------------------

FAKE_EXPORT uint32_t wgpuGetVersion(void) { return 0x1D000101u; }

FAKE_EXPORT WGPUInstance wgpuCreateInstance(const WGPUInstanceDescriptor *description) {
    (void)description;
    static int registered;
    if (!registered++) atexit(report);
    return (WGPUInstance)make(K_INSTANCE);
}
FAKE_EXPORT void wgpuInstanceRelease(WGPUInstance instance) {
    // The requests that are still pending are completed (cancelled) when the instance is released, which is when the
    // callback of a request that was given up on runs.
    while (pending_count > 0) {
        struct Pending entry = pending[0];
        memmove(&pending[0], &pending[1], (size_t)(pending_count - 1) * sizeof pending[0]);
        pending_count--;
        deliver_cancelled(&entry);
    }
    release(instance);
}
FAKE_EXPORT void wgpuInstanceProcessEvents(WGPUInstance instance) { (void)instance; deliver(); }

FAKE_EXPORT WGPUFuture wgpuInstanceRequestAdapter(WGPUInstance instance, const WGPURequestAdapterOptions *options, WGPURequestAdapterCallbackInfo info) {
    (void)instance; (void)options;
    struct Pending *entry = enqueue(0, flag("stall-adapter"));
    entry->adapter = info;
    entry->object = make(K_ADAPTER);
    WGPUFuture future = {1};
    return future;
}

static void limits_of(int device, WGPULimits *limits) {
    memset(limits, 0, sizeof *limits);
    limits->maxComputeInvocationsPerWorkgroup = 256;
    limits->maxComputeWorkgroupSizeX = 256;
    if (device) {
        limits->maxStorageBufferBindingSize = (uint64_t)number("max-buffer", 134217728);
        limits->maxBufferSize = (uint64_t)number("max-buffer", 268435456);
        limits->maxComputeWorkgroupsPerDimension = (uint32_t)number("max-groups", 65535);
    } else {
        // An adapter reports what the hardware can do, which is more than the default limits of a device.
        limits->maxStorageBufferBindingSize = 4294967292u;
        limits->maxBufferSize = 4294967292u;
        limits->maxComputeWorkgroupsPerDimension = 4000000u;
    }
}

FAKE_EXPORT uint32_t wgpuAdapterGetLimits(WGPUAdapter adapter, WGPULimits *limits) { (void)adapter; limits_of(0, limits); return WGPUStatus_Success; }
FAKE_EXPORT WGPUBool wgpuAdapterHasFeature(WGPUAdapter adapter, uint32_t feature) { (void)adapter; return feature == WGPUFeatureName_ShaderF16 && !flag("no-f16"); }
FAKE_EXPORT void wgpuAdapterRelease(WGPUAdapter adapter) { release(adapter); }

FAKE_EXPORT WGPUFuture wgpuAdapterRequestDevice(WGPUAdapter adapter, const WGPUDeviceDescriptor *description, WGPURequestDeviceCallbackInfo info) {
    (void)adapter;
    uncaptured = description->uncapturedErrorCallbackInfo;
    lost_info = description->deviceLostCallbackInfo;
    struct Pending *entry = enqueue(1, 0);
    entry->device = info;
    the_device = entry->object = make(K_DEVICE);
    WGPUFuture future = {2};
    return future;
}

FAKE_EXPORT WGPUQueue wgpuDeviceGetQueue(WGPUDevice device) { (void)device; return (WGPUQueue)make(K_QUEUE); }
FAKE_EXPORT uint32_t wgpuDeviceGetLimits(WGPUDevice device, WGPULimits *limits) { (void)device; limits_of(1, limits); return WGPUStatus_Success; }
FAKE_EXPORT void wgpuQueueRelease(WGPUQueue queue) { release(queue); }

FAKE_EXPORT WGPUBool wgpuDevicePoll(WGPUDevice device, WGPUBool wait, const uint64_t *submission) {
    (void)device; (void)submission;
    deliver();
    // A blocking poll waits for the device: with a map that never completes it never returns.
    if (wait) wait_while_held();
    deliver();
    return pending_count == 0;
}

FAKE_EXPORT void wgpuDeviceRelease(WGPUDevice device) {
    // Releasing the device waits for the work that it still has, so it does not return when a map never completes.
    wait_while_held();
    release(device);
}

FAKE_EXPORT WGPUShaderModule wgpuDeviceCreateShaderModule(WGPUDevice device, const WGPUShaderModuleDescriptor *description) {
    (void)device;
    Fake *module = make(K_MODULE);
    const WGPUShaderSourceWGSL *wgsl = (const WGPUShaderSourceWGSL *)description->nextInChain;
    assert(wgsl && wgsl->chain.sType == WGPUSType_ShaderSourceWGSL);
    // The source is not NUL terminated by contract: copy it before looking into it.
    char *text = malloc(wgsl->code.length + 1);
    assert(text);
    memcpy(text, wgsl->code.data, wgsl->code.length);
    text[wgsl->code.length] = '\0';
    module->has_map = strstr(text, "fn map_main(") != NULL;
    module->has_init = strstr(text, "fn init_main(") != NULL;
    module->doubles = strstr(text, "// fake: double") != NULL;
    module->copies16 = strstr(text, "// fake: copy16") != NULL;
    module->two_bindings = strstr(text, "// fake: two bindings") != NULL;
    if (strstr(text, "// fake: invalid shader")) {
        module->invalid = 1;
        error("fake: the shader module is invalid");
    }
    free(text);
    return (WGPUShaderModule)module;
}
FAKE_EXPORT void wgpuShaderModuleRelease(WGPUShaderModule module) { release(module); }

FAKE_EXPORT WGPUComputePipeline wgpuDeviceCreateComputePipeline(WGPUDevice device, const WGPUComputePipelineDescriptor *description) {
    (void)device;
    Fake *pipeline = make(K_PIPELINE);
    Fake *module = (Fake *)description->compute.module;
    pipeline->mode = strcmp(description->compute.entryPoint.data, "init_main") == 0;
    pipeline->module = module;
    module->refs++;
    if (module->invalid) {
        pipeline->invalid = 1;
        error("fake: the pipeline uses an invalid shader module");
    } else if (pipeline->mode ? !module->has_init : !module->has_map) {
        pipeline->invalid = 1;
        error("fake: the shader has no such entry point");
    } else if (flag("fail-pipeline")) {
        pipeline->invalid = 1;
        error("fake: the pipeline failed");
    }
    return (WGPUComputePipeline)pipeline;
}
FAKE_EXPORT void wgpuComputePipelineRelease(WGPUComputePipeline pipeline) { release(pipeline); }

FAKE_EXPORT WGPUBindGroupLayout wgpuComputePipelineGetBindGroupLayout(WGPUComputePipeline handle, uint32_t index) {
    (void)index;
    Fake *pipeline = (Fake *)handle;
    Fake *layout = make(K_LAYOUT);
    layout->invalid = pipeline->invalid;
    layout->bindings = pipeline->mode || pipeline->module->two_bindings ? 6u : 7u;
    return (WGPUBindGroupLayout)layout;
}
FAKE_EXPORT void wgpuBindGroupLayoutRelease(WGPUBindGroupLayout layout) { release(layout); }

FAKE_EXPORT WGPUBuffer wgpuDeviceCreateBuffer(WGPUDevice device, const WGPUBufferDescriptor *description) {
    (void)device;
    Fake *buffer = make(K_BUFFER);
    buffer->size = description->size;
    int failing = ++buffers_created == number("fail-buffer", -1);
    if (failing || description->size > (uint64_t)number("max-buffer", 268435456)) {
        buffer->invalid = 1;
        error("fake: the buffer could not be created");
    } else {
        buffer->data = calloc(1, description->size ? description->size : 1);
        assert(buffer->data);
    }
    return (WGPUBuffer)buffer;
}

FAKE_EXPORT WGPUBindGroup wgpuDeviceCreateBindGroup(WGPUDevice device, const WGPUBindGroupDescriptor *description) {
    (void)device;
    Fake *group = make(K_GROUP);
    Fake *layout = (Fake *)description->layout;
    if (layout->invalid || flag("fail-bindgroup")) group->invalid = 1;
    for (size_t index = 0; index < description->entryCount; index++) {
        const WGPUBindGroupEntry *entry = &description->entries[index];
        Fake *buffer = (Fake *)entry->buffer;
        if (entry->binding > 2 || !(layout->bindings & (1u << entry->binding))) group->invalid = 1;
        else group->bound[entry->binding] = buffer;
        if (buffer->invalid || entry->size > (uint64_t)number("max-buffer", 134217728)) group->invalid = 1;
    }
    if (group->invalid) error("fake: the bind group is invalid");
    return (WGPUBindGroup)group;
}
FAKE_EXPORT void wgpuBindGroupRelease(WGPUBindGroup group) { release(group); }

FAKE_EXPORT WGPUCommandEncoder wgpuDeviceCreateCommandEncoder(WGPUDevice device, const WGPUCommandEncoderDescriptor *description) {
    (void)device; (void)description;
    return (WGPUCommandEncoder)make(K_ENCODER);
}
FAKE_EXPORT WGPUComputePassEncoder wgpuCommandEncoderBeginComputePass(WGPUCommandEncoder encoder, const WGPUComputePassDescriptor *description) {
    (void)description;
    Fake *pass = make(K_PASS);
    pass->owner = (Fake *)encoder;
    return (WGPUComputePassEncoder)pass;
}
FAKE_EXPORT void wgpuComputePassEncoderSetPipeline(WGPUComputePassEncoder pass, WGPUComputePipeline pipeline) { ((Fake *)pass)->owner->pipeline = (Fake *)pipeline; }
FAKE_EXPORT void wgpuComputePassEncoderSetBindGroup(WGPUComputePassEncoder pass, uint32_t index, WGPUBindGroup group, size_t count, const uint32_t *offsets) {
    (void)index; (void)count; (void)offsets;
    ((Fake *)pass)->owner->group = (Fake *)group;
}
FAKE_EXPORT void wgpuComputePassEncoderDispatchWorkgroups(WGPUComputePassEncoder pass, uint32_t x, uint32_t y, uint32_t z) {
    (void)y; (void)z;
    ((Fake *)pass)->owner->groups = x;
}
FAKE_EXPORT void wgpuComputePassEncoderEnd(WGPUComputePassEncoder pass) { (void)pass; }
FAKE_EXPORT void wgpuComputePassEncoderRelease(WGPUComputePassEncoder pass) { release(pass); }

FAKE_EXPORT void wgpuCommandEncoderCopyBufferToBuffer(WGPUCommandEncoder handle, WGPUBuffer from, uint64_t from_offset, WGPUBuffer to, uint64_t to_offset, uint64_t size) {
    Fake *encoder = (Fake *)handle;
    (void)from_offset; (void)to_offset;
    encoder->copy_from = (Fake *)from;
    encoder->copy_to = (Fake *)to;
    encoder->copy_size = size;
}

FAKE_EXPORT WGPUCommandBuffer wgpuCommandEncoderFinish(WGPUCommandEncoder handle, const WGPUCommandBufferDescriptor *description) {
    (void)description;
    Fake *encoder = (Fake *)handle;
    Fake *commands = make(K_COMMANDS);
    commands->pipeline = encoder->pipeline;
    commands->group = encoder->group;
    commands->groups = encoder->groups;
    commands->copy_from = encoder->copy_from;
    commands->copy_to = encoder->copy_to;
    commands->copy_size = encoder->copy_size;
    if (!encoder->pipeline || encoder->pipeline->invalid || !encoder->group || encoder->group->invalid || flag("fail-finish")) {
        commands->invalid = 1;
        error("fake: the command buffer is invalid");
    }
    return (WGPUCommandBuffer)commands;
}
FAKE_EXPORT void wgpuCommandEncoderRelease(WGPUCommandEncoder encoder) { release(encoder); }
FAKE_EXPORT void wgpuCommandBufferRelease(WGPUCommandBuffer commands) { release(commands); }

FAKE_EXPORT void wgpuQueueWriteBuffer(WGPUQueue queue, WGPUBuffer handle, uint64_t offset, const void *data, size_t size) {
    (void)queue;
    Fake *buffer = (Fake *)handle;
    if (buffer->invalid) panic("wgpuQueueWriteBuffer to an invalid buffer");
    if (offset + size > buffer->size) panic("wgpuQueueWriteBuffer beyond the end of the buffer");
    memcpy(buffer->data + offset, data, size);
}

static void execute(Fake *commands) {
    Fake *pipeline = commands->pipeline, *group = commands->group;
    if (commands->groups > (uint64_t)number("max-groups", 65535)) panic("dispatch group size dimension must be less or equal to the device limit");
    uint32_t count;
    memcpy(&count, group->bound[2]->data, sizeof count);
    if (pipeline->module->copies16) {
        // 16-bit lanes: map copies them, init writes the low byte of the index (the bytes, not f16 values)
        uint16_t *output16 = (uint16_t *)group->bound[1]->data;
        const uint16_t *input16 = group->bound[0] ? (const uint16_t *)group->bound[0]->data : NULL;
        uint64_t room16 = group->bound[1]->size / sizeof *output16;
        for (uint32_t index = 0; index < count && index < room16; index++) output16[index] = pipeline->mode ? (uint16_t)(index & 255) : input16[index];
        if (commands->copy_from) memcpy(commands->copy_to->data, commands->copy_from->data, commands->copy_size);
        return;
    }
    int32_t *output = (int32_t *)group->bound[1]->data;
    const int32_t *input = group->bound[0] ? (const int32_t *)group->bound[0]->data : NULL;
    // A kernel of 16-bit lanes writes fewer bytes than the 32-bit lanes that this interpreter computes: stop at the end.
    uint64_t room = group->bound[1]->size / sizeof *output;
    for (uint32_t index = 0; index < count && index < room; index++) {
        if (pipeline->mode) output[index] = (int32_t)index * 3 + 1;
        else if (pipeline->module->doubles) output[index] = input[index] * 2;
        else output[index] = (int32_t)index;
    }
    if (commands->copy_from) memcpy(commands->copy_to->data, commands->copy_from->data, commands->copy_size);
}

FAKE_EXPORT void wgpuQueueSubmit(WGPUQueue queue, size_t count, const WGPUCommandBuffer *commands) {
    (void)queue;
    if (lost_reported) panic("wgpuQueueSubmit on a lost device");
    for (size_t index = 0; index < count; index++) {
        Fake *buffer = (Fake *)commands[index];
        if (buffer->invalid) panic("wgpuQueueSubmit of an invalid command buffer");
        execute(buffer);
    }
    if (++submits == 1 && flag("lose-device")) device_lost_pending = 1;
}

FAKE_EXPORT WGPUFuture wgpuBufferMapAsync(WGPUBuffer handle, WGPUMapMode mode, size_t offset, size_t size, WGPUBufferMapCallbackInfo info) {
    (void)mode; (void)offset; (void)size;
    static int maps;
    struct Pending *entry = enqueue(2, ++maps == 1 && flag("stall-map"));
    entry->map = info;
    entry->object = (Fake *)handle;
    entry->object->refs++; // the pending map keeps the buffer, so a late completion finds it
    WGPUFuture future = {3};
    return future;
}
FAKE_EXPORT const void *wgpuBufferGetConstMappedRange(WGPUBuffer handle, size_t offset, size_t size) {
    Fake *buffer = (Fake *)handle;
    (void)size;
    return buffer->mapped && buffer->data ? buffer->data + offset : NULL;
}
FAKE_EXPORT void wgpuBufferUnmap(WGPUBuffer handle) { ((Fake *)handle)->mapped = 0; }
FAKE_EXPORT void wgpuBufferDestroy(WGPUBuffer handle) { (void)handle; }
FAKE_EXPORT void wgpuBufferRelease(WGPUBuffer handle) { release(handle); }
