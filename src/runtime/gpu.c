// The GPU runtime of programs that run kernels on a non-CPU backend (F09 Phase 2). `Gpu.request
// Gpu.WebGpu` calls tsuzuri_gpu_open, and `Gpu.map`/`Gpu.init` on that device call
// tsuzuri_gpu_run, which runs the kernel that the compiler embedded for the call site. The
// driver links this file when the program declares them. The program never links a WebGPU
// library: the library is loaded here at run time, so a machine without one only sees status 1
// ("unavailable") from tsuzuri_gpu_open, and `Gpu.request` returns `Result.Error Gpu.Unavailable`.
//
// The ABI (the same for the WASM imports `tsuzuri_gpu.open` and `tsuzuri_gpu.run`):
//   int32_t tsuzuri_gpu_open(int32_t backend, int32_t features);
//   int32_t tsuzuri_gpu_run(int32_t backend, int32_t mode, int32_t flags, int32_t lanes,
//                           const void *wgsl, int32_t wgsl_length,
//                           const void *spirv, int32_t spirv_length,
//                           const void *input, int64_t count, void *output);
//   int32_t tsuzuri_gpu_select(int32_t mode, int32_t lanes, int32_t features,
//                              const void *spirv, int32_t spirv_length,
//                              int32_t weight, int64_t count);      // native only (F09 Phase 3)
// `backend` is the tag of `Gpu.Backend` (1 WebGpu, 2 Vulkan, ...). `features` and the kernel's
// `lanes` are described below. Every function returns a status: 0 success, 1 unavailable (no
// library, host, adapter), 2 unsupported (library version, missing feature, no kernel source for
// the backend), 3 limit exceeded, 4 failed (compilation, validation, out of memory, device lost).
// The buffers belong to the caller and are not kept; a run returns after the result is in
// `output`. The runtime prints one line to stderr before it returns a status of 2 or more from a
// run, and from `open` only when TSUZURI_GPU_DEBUG is set.
//
// Backends: this file holds the WebGPU backend, which loads wgpu-native (a C library that
// implements webgpu.h) with dlopen or LoadLibrary. TSUZURI_WEBGPU_LIBRARY names the library
// file; if it is set, only that file is tried, and an empty value disables the backend. The
// runtime declares the part of the C API it needs itself and checks `wgpuGetVersion`: only
// wgpu-native 29.x is accepted, whose headers this declaration copies (verified with 29.0.1.1).
// The Vulkan backend (backend 2, F09 Phase 3) is the file gpu-vulkan.c, which comes before this one
// in the translation unit of a program that names Vulkan or `Gpu.Auto`.
#if defined(__APPLE__) && !defined(_DARWIN_C_SOURCE)
#define _DARWIN_C_SOURCE
#endif
#if defined(__linux__) && !defined(_GNU_SOURCE)
#define _GNU_SOURCE
#endif

#if defined(_WIN32)
#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#include <windows.h>
#else
#include <dlfcn.h>
#include <pthread.h>
#include <time.h>
#endif
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#if defined(_WIN32)
#define TZ_GPU_API
#else
#define TZ_GPU_API __attribute__((weak, visibility("hidden")))
#endif

enum {
    TZ_GPU_OK = 0,
    TZ_GPU_UNAVAILABLE = 1,
    TZ_GPU_UNSUPPORTED = 2,
    TZ_GPU_LIMIT = 3,
    TZ_GPU_FAILED = 4,
};

// Device features that a program's kernels need (the `features` of open and of a kernel).
#define TZ_GPU_FEATURE_F16 1

// Lane kinds of `lanes`: the input kind in bits 0 to 7, the output kind in bits 8 to 15.
#define TZ_GPU_LANE_32 1
#define TZ_GPU_LANE_F32 2
#define TZ_GPU_LANE_F16 3

#define TZ_GPU_RELAXED 1

// ---- Platform: the library, the lock, and the clock -------------------------------------------

// A library that the user names with TSUZURI_WEBGPU_LIBRARY is loaded as named. The libraries that the runtime tries
// by itself are never looked up by a bare name: on macOS dlopen searches the working directory for one, and the Windows
// search order has the application and the current directory, so a library planted there would run its initializers in
// every program that asks for a WebGPU device. A default is an absolute path in a system location (on Windows, a name
// that is looked up in the system directory only).
#if defined(_WIN32)
#ifndef LOAD_LIBRARY_SEARCH_SYSTEM32
#define LOAD_LIBRARY_SEARCH_SYSTEM32 0x00000800
#endif
typedef HMODULE tz_gpu_library;
static SRWLOCK tz_gpu_lock = SRWLOCK_INIT;
static void tz_gpu_acquire(void) { AcquireSRWLockExclusive(&tz_gpu_lock); }
static void tz_gpu_release(void) { ReleaseSRWLockExclusive(&tz_gpu_lock); }
static tz_gpu_library tz_gpu_open_library(const char *path) { return LoadLibraryA(path); }
static tz_gpu_library tz_gpu_open_default(const char *path) { return LoadLibraryExA(path, NULL, LOAD_LIBRARY_SEARCH_SYSTEM32); }
static void *tz_gpu_symbol(tz_gpu_library library, const char *name) {
    return (void *)GetProcAddress(library, name);
}
static uint64_t tz_gpu_microseconds(void) {
    LARGE_INTEGER counter, frequency;
    QueryPerformanceFrequency(&frequency);
    QueryPerformanceCounter(&counter);
    return (uint64_t)(counter.QuadPart / frequency.QuadPart) * 1000000 + (uint64_t)(counter.QuadPart % frequency.QuadPart) * 1000000 / (uint64_t)frequency.QuadPart;
}
static void tz_gpu_sleep(uint64_t microseconds) { Sleep((DWORD)((microseconds + 999) / 1000)); }
#define TZ_GPU_DEFAULT_LIBRARIES "wgpu_native.dll"
#else
typedef void *tz_gpu_library;
static pthread_mutex_t tz_gpu_lock = PTHREAD_MUTEX_INITIALIZER;
static void tz_gpu_acquire(void) { pthread_mutex_lock(&tz_gpu_lock); }
static void tz_gpu_release(void) { pthread_mutex_unlock(&tz_gpu_lock); }
static tz_gpu_library tz_gpu_open_library(const char *path) { return dlopen(path, RTLD_NOW | RTLD_LOCAL); }
static tz_gpu_library tz_gpu_open_default(const char *path) { return path[0] == '/' ? tz_gpu_open_library(path) : NULL; }
static void *tz_gpu_symbol(tz_gpu_library library, const char *name) { return dlsym(library, name); }
static uint64_t tz_gpu_microseconds(void) {
    struct timespec now;
    clock_gettime(CLOCK_MONOTONIC, &now);
    return (uint64_t)now.tv_sec * 1000000 + (uint64_t)now.tv_nsec / 1000;
}
static void tz_gpu_sleep(uint64_t microseconds) {
    struct timespec pause = {(time_t)(microseconds / 1000000), (long)(microseconds % 1000000) * 1000};
    nanosleep(&pause, NULL);
}
#if defined(__APPLE__)
#define TZ_GPU_DEFAULT_LIBRARIES "/opt/homebrew/lib/libwgpu_native.dylib", "/usr/local/lib/libwgpu_native.dylib"
#else
#define TZ_GPU_DEFAULT_LIBRARIES "/usr/local/lib/libwgpu_native.so", "/usr/lib/libwgpu_native.so", "/usr/lib64/libwgpu_native.so", "/usr/lib/x86_64-linux-gnu/libwgpu_native.so", "/usr/lib/aarch64-linux-gnu/libwgpu_native.so"
#endif
#endif
// A build can name its own candidates with TZ_GPU_WGPU_DEFAULTS, a list of string literals (the tests do).
#ifdef TZ_GPU_WGPU_DEFAULTS
static const char *const tz_gpu_default_libraries[] = {TZ_GPU_WGPU_DEFAULTS, NULL};
#else
static const char *const tz_gpu_default_libraries[] = {TZ_GPU_DEFAULT_LIBRARIES, NULL};
#endif

static int tz_gpu_debug(void) {
    const char *value = getenv("TSUZURI_GPU_DEBUG");
    return value != NULL && value[0] != '\0';
}

static void tz_gpu_report(const char *action, const char *detail) {
    fprintf(stderr, "tsuzuri gpu: %s: %s\n", action, detail);
}

// ---- The part of webgpu.h that the runtime uses (wgpu-native 29) --------------------------------

typedef uint32_t WGPUBool;
typedef uint64_t WGPUFlags;
typedef WGPUFlags WGPUBufferUsage;
typedef WGPUFlags WGPUMapMode;
typedef struct WGPUInstanceImpl *WGPUInstance;
typedef struct WGPUAdapterImpl *WGPUAdapter;
typedef struct WGPUDeviceImpl *WGPUDevice;
typedef struct WGPUQueueImpl *WGPUQueue;
typedef struct WGPUBufferImpl *WGPUBuffer;
typedef struct WGPUShaderModuleImpl *WGPUShaderModule;
typedef struct WGPUComputePipelineImpl *WGPUComputePipeline;
typedef struct WGPUBindGroupLayoutImpl *WGPUBindGroupLayout;
typedef struct WGPUBindGroupImpl *WGPUBindGroup;
typedef struct WGPUCommandEncoderImpl *WGPUCommandEncoder;
typedef struct WGPUCommandBufferImpl *WGPUCommandBuffer;
typedef struct WGPUComputePassEncoderImpl *WGPUComputePassEncoder;
typedef struct WGPUPipelineLayoutImpl *WGPUPipelineLayout;
typedef struct WGPUSurfaceImpl *WGPUSurface;
typedef struct WGPUSamplerImpl *WGPUSampler;
typedef struct WGPUTextureViewImpl *WGPUTextureView;
typedef struct WGPUPassTimestampWrites WGPUPassTimestampWrites;
typedef struct WGPUConstantEntry WGPUConstantEntry;
typedef struct WGPUInstanceLimits WGPUInstanceLimits;

typedef struct WGPUStringView {
    const char *data;
    size_t length;
} WGPUStringView;
typedef struct WGPUChainedStruct {
    struct WGPUChainedStruct *next;
    uint32_t sType;
} WGPUChainedStruct;
typedef struct WGPUFuture {
    uint64_t id;
} WGPUFuture;

#define WGPU_STRLEN SIZE_MAX
#define WGPU_WHOLE_SIZE UINT64_MAX
#define WGPUSType_ShaderSourceWGSL 2u
#define WGPUCallbackMode_AllowProcessEvents 2u
#define WGPUFeatureName_ShaderF16 0x0000000Bu
#define WGPUPowerPreference_HighPerformance 2u
#define WGPURequestAdapterStatus_Success 1u
#define WGPURequestDeviceStatus_Success 1u
#define WGPUMapAsyncStatus_Success 1u
#define WGPUStatus_Success 1u
#define WGPUBufferUsage_MapRead 0x1u
#define WGPUBufferUsage_CopySrc 0x4u
#define WGPUBufferUsage_CopyDst 0x8u
#define WGPUBufferUsage_Uniform 0x40u
#define WGPUBufferUsage_Storage 0x80u
#define WGPUMapMode_Read 0x1u

typedef void (*WGPURequestAdapterCallback)(uint32_t status, WGPUAdapter adapter, WGPUStringView message, void *userdata1, void *userdata2);
typedef void (*WGPURequestDeviceCallback)(uint32_t status, WGPUDevice device, WGPUStringView message, void *userdata1, void *userdata2);
typedef void (*WGPUBufferMapCallback)(uint32_t status, WGPUStringView message, void *userdata1, void *userdata2);
typedef void (*WGPUDeviceLostCallback)(const WGPUDevice *device, uint32_t reason, WGPUStringView message, void *userdata1, void *userdata2);
typedef void (*WGPUUncapturedErrorCallback)(const WGPUDevice *device, uint32_t type, WGPUStringView message, void *userdata1, void *userdata2);

typedef struct WGPURequestAdapterCallbackInfo {
    WGPUChainedStruct *nextInChain;
    uint32_t mode;
    WGPURequestAdapterCallback callback;
    void *userdata1;
    void *userdata2;
} WGPURequestAdapterCallbackInfo;
typedef struct WGPURequestDeviceCallbackInfo {
    WGPUChainedStruct *nextInChain;
    uint32_t mode;
    WGPURequestDeviceCallback callback;
    void *userdata1;
    void *userdata2;
} WGPURequestDeviceCallbackInfo;
typedef struct WGPUBufferMapCallbackInfo {
    WGPUChainedStruct *nextInChain;
    uint32_t mode;
    WGPUBufferMapCallback callback;
    void *userdata1;
    void *userdata2;
} WGPUBufferMapCallbackInfo;
typedef struct WGPUDeviceLostCallbackInfo {
    WGPUChainedStruct *nextInChain;
    uint32_t mode;
    WGPUDeviceLostCallback callback;
    void *userdata1;
    void *userdata2;
} WGPUDeviceLostCallbackInfo;
typedef struct WGPUUncapturedErrorCallbackInfo {
    WGPUChainedStruct *nextInChain;
    WGPUUncapturedErrorCallback callback;
    void *userdata1;
    void *userdata2;
} WGPUUncapturedErrorCallbackInfo;

typedef struct WGPUInstanceDescriptor {
    WGPUChainedStruct *nextInChain;
    size_t requiredFeatureCount;
    const uint32_t *requiredFeatures;
    const WGPUInstanceLimits *requiredLimits;
} WGPUInstanceDescriptor;
typedef struct WGPURequestAdapterOptions {
    WGPUChainedStruct *nextInChain;
    uint32_t featureLevel;
    uint32_t powerPreference;
    WGPUBool forceFallbackAdapter;
    uint32_t backendType;
    WGPUSurface compatibleSurface;
} WGPURequestAdapterOptions;
typedef struct WGPUQueueDescriptor {
    WGPUChainedStruct *nextInChain;
    WGPUStringView label;
} WGPUQueueDescriptor;
typedef struct WGPULimits {
    WGPUChainedStruct *nextInChain;
    uint32_t maxTextureDimension1D;
    uint32_t maxTextureDimension2D;
    uint32_t maxTextureDimension3D;
    uint32_t maxTextureArrayLayers;
    uint32_t maxBindGroups;
    uint32_t maxBindGroupsPlusVertexBuffers;
    uint32_t maxBindingsPerBindGroup;
    uint32_t maxDynamicUniformBuffersPerPipelineLayout;
    uint32_t maxDynamicStorageBuffersPerPipelineLayout;
    uint32_t maxSampledTexturesPerShaderStage;
    uint32_t maxSamplersPerShaderStage;
    uint32_t maxStorageBuffersPerShaderStage;
    uint32_t maxStorageTexturesPerShaderStage;
    uint32_t maxUniformBuffersPerShaderStage;
    uint64_t maxUniformBufferBindingSize;
    uint64_t maxStorageBufferBindingSize;
    uint32_t minUniformBufferOffsetAlignment;
    uint32_t minStorageBufferOffsetAlignment;
    uint32_t maxVertexBuffers;
    uint64_t maxBufferSize;
    uint32_t maxVertexAttributes;
    uint32_t maxVertexBufferArrayStride;
    uint32_t maxInterStageShaderVariables;
    uint32_t maxColorAttachments;
    uint32_t maxColorAttachmentBytesPerSample;
    uint32_t maxComputeWorkgroupStorageSize;
    uint32_t maxComputeInvocationsPerWorkgroup;
    uint32_t maxComputeWorkgroupSizeX;
    uint32_t maxComputeWorkgroupSizeY;
    uint32_t maxComputeWorkgroupSizeZ;
    uint32_t maxComputeWorkgroupsPerDimension;
    uint32_t maxImmediateSize;
} WGPULimits;
typedef struct WGPUDeviceDescriptor {
    WGPUChainedStruct *nextInChain;
    WGPUStringView label;
    size_t requiredFeatureCount;
    const uint32_t *requiredFeatures;
    const WGPULimits *requiredLimits;
    WGPUQueueDescriptor defaultQueue;
    WGPUDeviceLostCallbackInfo deviceLostCallbackInfo;
    WGPUUncapturedErrorCallbackInfo uncapturedErrorCallbackInfo;
} WGPUDeviceDescriptor;
typedef struct WGPUShaderModuleDescriptor {
    WGPUChainedStruct *nextInChain;
    WGPUStringView label;
} WGPUShaderModuleDescriptor;
typedef struct WGPUShaderSourceWGSL {
    WGPUChainedStruct chain;
    WGPUStringView code;
} WGPUShaderSourceWGSL;
typedef struct WGPUComputeState {
    WGPUChainedStruct *nextInChain;
    WGPUShaderModule module;
    WGPUStringView entryPoint;
    size_t constantCount;
    const WGPUConstantEntry *constants;
} WGPUComputeState;
typedef struct WGPUComputePipelineDescriptor {
    WGPUChainedStruct *nextInChain;
    WGPUStringView label;
    WGPUPipelineLayout layout;
    WGPUComputeState compute;
} WGPUComputePipelineDescriptor;
typedef struct WGPUBufferDescriptor {
    WGPUChainedStruct *nextInChain;
    WGPUStringView label;
    WGPUBufferUsage usage;
    uint64_t size;
    WGPUBool mappedAtCreation;
} WGPUBufferDescriptor;
typedef struct WGPUBindGroupEntry {
    WGPUChainedStruct *nextInChain;
    uint32_t binding;
    WGPUBuffer buffer;
    uint64_t offset;
    uint64_t size;
    WGPUSampler sampler;
    WGPUTextureView textureView;
} WGPUBindGroupEntry;
typedef struct WGPUBindGroupDescriptor {
    WGPUChainedStruct *nextInChain;
    WGPUStringView label;
    WGPUBindGroupLayout layout;
    size_t entryCount;
    const WGPUBindGroupEntry *entries;
} WGPUBindGroupDescriptor;
typedef struct WGPUCommandEncoderDescriptor {
    WGPUChainedStruct *nextInChain;
    WGPUStringView label;
} WGPUCommandEncoderDescriptor;
typedef struct WGPUComputePassDescriptor {
    WGPUChainedStruct *nextInChain;
    WGPUStringView label;
    const WGPUPassTimestampWrites *timestampWrites;
} WGPUComputePassDescriptor;
typedef struct WGPUCommandBufferDescriptor {
    WGPUChainedStruct *nextInChain;
    WGPUStringView label;
} WGPUCommandBufferDescriptor;

// The functions, loaded by name. The names and signatures are those of webgpu.h and wgpu.h.
struct tz_wgpu_api {
    uint32_t (*get_version)(void);
    WGPUInstance (*create_instance)(const WGPUInstanceDescriptor *);
    void (*instance_release)(WGPUInstance);
    void (*instance_process_events)(WGPUInstance);
    WGPUFuture (*instance_request_adapter)(WGPUInstance, const WGPURequestAdapterOptions *, WGPURequestAdapterCallbackInfo);
    uint32_t (*adapter_get_limits)(WGPUAdapter, WGPULimits *);
    WGPUBool (*adapter_has_feature)(WGPUAdapter, uint32_t);
    WGPUFuture (*adapter_request_device)(WGPUAdapter, const WGPUDeviceDescriptor *, WGPURequestDeviceCallbackInfo);
    void (*adapter_release)(WGPUAdapter);
    WGPUQueue (*device_get_queue)(WGPUDevice);
    uint32_t (*device_get_limits)(WGPUDevice, WGPULimits *);
    WGPUShaderModule (*device_create_shader_module)(WGPUDevice, const WGPUShaderModuleDescriptor *);
    WGPUComputePipeline (*device_create_compute_pipeline)(WGPUDevice, const WGPUComputePipelineDescriptor *);
    WGPUBuffer (*device_create_buffer)(WGPUDevice, const WGPUBufferDescriptor *);
    WGPUBindGroup (*device_create_bind_group)(WGPUDevice, const WGPUBindGroupDescriptor *);
    WGPUCommandEncoder (*device_create_command_encoder)(WGPUDevice, const WGPUCommandEncoderDescriptor *);
    WGPUBool (*device_poll)(WGPUDevice, WGPUBool, const uint64_t *);
    void (*device_release)(WGPUDevice);
    WGPUBindGroupLayout (*pipeline_get_bind_group_layout)(WGPUComputePipeline, uint32_t);
    void (*pipeline_release)(WGPUComputePipeline);
    void (*shader_module_release)(WGPUShaderModule);
    void (*bind_group_layout_release)(WGPUBindGroupLayout);
    void (*bind_group_release)(WGPUBindGroup);
    WGPUComputePassEncoder (*encoder_begin_compute_pass)(WGPUCommandEncoder, const WGPUComputePassDescriptor *);
    void (*encoder_copy_buffer_to_buffer)(WGPUCommandEncoder, WGPUBuffer, uint64_t, WGPUBuffer, uint64_t, uint64_t);
    WGPUCommandBuffer (*encoder_finish)(WGPUCommandEncoder, const WGPUCommandBufferDescriptor *);
    void (*encoder_release)(WGPUCommandEncoder);
    void (*pass_set_pipeline)(WGPUComputePassEncoder, WGPUComputePipeline);
    void (*pass_set_bind_group)(WGPUComputePassEncoder, uint32_t, WGPUBindGroup, size_t, const uint32_t *);
    void (*pass_dispatch)(WGPUComputePassEncoder, uint32_t, uint32_t, uint32_t);
    void (*pass_end)(WGPUComputePassEncoder);
    void (*pass_release)(WGPUComputePassEncoder);
    void (*command_buffer_release)(WGPUCommandBuffer);
    void (*queue_submit)(WGPUQueue, size_t, const WGPUCommandBuffer *);
    void (*queue_write_buffer)(WGPUQueue, WGPUBuffer, uint64_t, const void *, size_t);
    void (*queue_release)(WGPUQueue);
    WGPUFuture (*buffer_map_async)(WGPUBuffer, WGPUMapMode, size_t, size_t, WGPUBufferMapCallbackInfo);
    const void *(*buffer_get_const_mapped_range)(WGPUBuffer, size_t, size_t);
    void (*buffer_unmap)(WGPUBuffer);
    void (*buffer_destroy)(WGPUBuffer);
    void (*buffer_release)(WGPUBuffer);
};

#define TZ_GPU_SYMBOLS(X) \
    X(get_version, "wgpuGetVersion") \
    X(create_instance, "wgpuCreateInstance") \
    X(instance_release, "wgpuInstanceRelease") \
    X(instance_process_events, "wgpuInstanceProcessEvents") \
    X(instance_request_adapter, "wgpuInstanceRequestAdapter") \
    X(adapter_get_limits, "wgpuAdapterGetLimits") \
    X(adapter_has_feature, "wgpuAdapterHasFeature") \
    X(adapter_request_device, "wgpuAdapterRequestDevice") \
    X(adapter_release, "wgpuAdapterRelease") \
    X(device_get_queue, "wgpuDeviceGetQueue") \
    X(device_get_limits, "wgpuDeviceGetLimits") \
    X(device_create_shader_module, "wgpuDeviceCreateShaderModule") \
    X(device_create_compute_pipeline, "wgpuDeviceCreateComputePipeline") \
    X(device_create_buffer, "wgpuDeviceCreateBuffer") \
    X(device_create_bind_group, "wgpuDeviceCreateBindGroup") \
    X(device_create_command_encoder, "wgpuDeviceCreateCommandEncoder") \
    X(device_poll, "wgpuDevicePoll") \
    X(device_release, "wgpuDeviceRelease") \
    X(pipeline_get_bind_group_layout, "wgpuComputePipelineGetBindGroupLayout") \
    X(pipeline_release, "wgpuComputePipelineRelease") \
    X(shader_module_release, "wgpuShaderModuleRelease") \
    X(bind_group_layout_release, "wgpuBindGroupLayoutRelease") \
    X(bind_group_release, "wgpuBindGroupRelease") \
    X(encoder_begin_compute_pass, "wgpuCommandEncoderBeginComputePass") \
    X(encoder_copy_buffer_to_buffer, "wgpuCommandEncoderCopyBufferToBuffer") \
    X(encoder_finish, "wgpuCommandEncoderFinish") \
    X(encoder_release, "wgpuCommandEncoderRelease") \
    X(pass_set_pipeline, "wgpuComputePassEncoderSetPipeline") \
    X(pass_set_bind_group, "wgpuComputePassEncoderSetBindGroup") \
    X(pass_dispatch, "wgpuComputePassEncoderDispatchWorkgroups") \
    X(pass_end, "wgpuComputePassEncoderEnd") \
    X(pass_release, "wgpuComputePassEncoderRelease") \
    X(command_buffer_release, "wgpuCommandBufferRelease") \
    X(queue_submit, "wgpuQueueSubmit") \
    X(queue_write_buffer, "wgpuQueueWriteBuffer") \
    X(queue_release, "wgpuQueueRelease") \
    X(buffer_map_async, "wgpuBufferMapAsync") \
    X(buffer_get_const_mapped_range, "wgpuBufferGetConstMappedRange") \
    X(buffer_unmap, "wgpuBufferUnmap") \
    X(buffer_destroy, "wgpuBufferDestroy") \
    X(buffer_release, "wgpuBufferRelease")

// ---- The WebGPU backend --------------------------------------------------------------------------

#define TZ_WGPU_MAJOR 29
#define TZ_WGPU_GROUP 256u
// A device that does not answer within this time is given up on, so a lost device never hangs the program. The tests
// build the runtime with a shorter time.
#ifndef TZ_WGPU_TIMEOUT_MS
#define TZ_WGPU_TIMEOUT_MS 60000
#endif
// A wait polls without sleeping for this long (a small kernel is done sooner), then sleeps between polls.
#ifndef TZ_WGPU_SPIN_US
#define TZ_WGPU_SPIN_US 5000
#endif

struct tz_wgpu_program {
    const void *source;
    int32_t length;
    WGPUShaderModule module;
    WGPUComputePipeline pipeline[2];
    int ready;
};

struct tz_wgpu_state {
    int attempted;
    int status;
    tz_gpu_library library;
    struct tz_wgpu_api api;
    WGPUInstance instance;
    WGPUAdapter adapter;
    WGPUDevice device;
    WGPUQueue queue;
    int32_t features;
    // The limits of the opened device, which are the default limits (the device is created without required limits),
    // not those of the adapter: a dispatch or a binding over them is a validation error that wgpu-native turns into
    // a panic.
    uint64_t max_buffer;
    uint32_t max_groups;
    char message[512];
    // An error was reported since the call began (a validation error, an out-of-memory, a lost device).
    int failed;
    // The device was lost, or a wait ran out of time and the device is not trusted to answer again: later runs fail at once.
    int lost;
    int hung;
    struct tz_wgpu_program *programs;
    size_t program_count, program_capacity;
};
static struct tz_wgpu_state tz_wgpu;

static void tz_wgpu_note(const char *message, size_t length) {
    if (tz_wgpu.message[0] != '\0') return;
    if (length >= sizeof tz_wgpu.message) length = sizeof tz_wgpu.message - 1;
    memcpy(tz_wgpu.message, message, length);
    tz_wgpu.message[length] = '\0';
    tz_wgpu.failed = 1;
}

static void tz_wgpu_uncaptured(const WGPUDevice *device, uint32_t type, WGPUStringView message, void *userdata1, void *userdata2) {
    (void)device; (void)type; (void)userdata1; (void)userdata2;
    tz_wgpu_note(message.data ? message.data : "unknown error", message.data ? message.length : 13);
}

static void tz_wgpu_lost(const WGPUDevice *device, uint32_t reason, WGPUStringView message, void *userdata1, void *userdata2) {
    (void)device; (void)userdata1; (void)userdata2;
    // Destroying the device at exit reports reason 2 (destroyed); a lost device is the failure.
    if (reason == 2) return;
    tz_wgpu.lost = 1;
    tz_wgpu_note(message.data ? message.data : "device lost", message.data ? message.length : 11);
}

// The one wait that is outstanding (the lock allows one call at a time). The callback of a request or a map that was
// given up on after the timeout stays registered and can run during a later call, so its state is static, not a
// local of the call that is gone, and each wait has a generation: a callback of an older generation touches nothing.
struct tz_wgpu_wait {
    uint64_t generation;
    int done;
    uint32_t status;
    void *object;
};
static struct tz_wgpu_wait tz_wgpu_slot;
static uint64_t tz_wgpu_generation;

// Starts a wait and returns its generation, which goes to the callback as `userdata2`.
static void *tz_wgpu_begin(void) {
    memset(&tz_wgpu_slot, 0, sizeof tz_wgpu_slot);
    tz_wgpu_slot.generation = ++tz_wgpu_generation;
    return (void *)(uintptr_t)tz_wgpu_slot.generation;
}

static int tz_wgpu_current(void *userdata2) {
    return (uintptr_t)userdata2 == (uintptr_t)tz_wgpu_slot.generation;
}

static void tz_wgpu_adapter_done(uint32_t status, WGPUAdapter adapter, WGPUStringView message, void *userdata1, void *userdata2) {
    (void)message; (void)userdata1;
    if (!tz_wgpu_current(userdata2)) {
        if (adapter) tz_wgpu.api.adapter_release(adapter);
        return;
    }
    tz_wgpu_slot.status = status;
    tz_wgpu_slot.object = adapter;
    tz_wgpu_slot.done = 1;
}

static void tz_wgpu_device_done(uint32_t status, WGPUDevice device, WGPUStringView message, void *userdata1, void *userdata2) {
    (void)userdata1;
    if (!tz_wgpu_current(userdata2)) {
        if (device) tz_wgpu.api.device_release(device);
        return;
    }
    tz_wgpu_slot.status = status;
    tz_wgpu_slot.object = device;
    tz_wgpu_slot.done = 1;
    if (status != WGPURequestDeviceStatus_Success && message.data) tz_wgpu_note(message.data, message.length);
}

static void tz_wgpu_map_done(uint32_t status, WGPUStringView message, void *userdata1, void *userdata2) {
    (void)userdata1;
    if (!tz_wgpu_current(userdata2)) return;
    tz_wgpu_slot.status = status;
    tz_wgpu_slot.done = 1;
    if (status != WGPUMapAsyncStatus_Success && message.data) tz_wgpu_note(message.data, message.length);
}

// Processes events until the callback of the current wait ran. `poll_device` also drives the queue, which completes
// buffer maps. The poll never blocks, because a blocking poll cannot be given up on: for the first TZ_WGPU_SPIN_US the
// wait polls back to back (on wgpu-native 29 with Metal, a blocking poll added about 1.2 ms to a call of 0.35 ms, which
// is measured), then it sleeps between polls, so a long kernel does not keep a core busy. Gives up after a minute, so a
// lost device never hangs the program.
static int tz_wgpu_await(int poll_device) {
    uint64_t start = tz_gpu_microseconds();
    while (!tz_wgpu_slot.done) {
        tz_wgpu.api.instance_process_events(tz_wgpu.instance);
        if (tz_wgpu_slot.done) break;
        uint64_t elapsed = tz_gpu_microseconds() - start;
        if (poll_device && tz_wgpu.device) tz_wgpu.api.device_poll(tz_wgpu.device, 0, NULL);
        if (elapsed > (uint64_t)TZ_WGPU_TIMEOUT_MS * 1000) {
            // The request or the map is given up on, but its callback stays registered: from now on it is a late one.
            tz_wgpu_slot.generation = 0;
            // A readback that never completes means that the device does not answer.
            if (poll_device) tz_wgpu.hung = 1;
            return 0;
        }
        if (elapsed >= TZ_WGPU_SPIN_US) tz_gpu_sleep(elapsed < 50000 ? 100 : 1000);
    }
    return 1;
}

static void tz_wgpu_close(void) {
    struct tz_wgpu_api *api = &tz_wgpu.api;
    // Releasing a device that stopped answering can wait for it: leave it to the end of the process.
    if (tz_wgpu.hung) return;
    for (size_t index = 0; index < tz_wgpu.program_count; index++) {
        struct tz_wgpu_program *program = &tz_wgpu.programs[index];
        for (int mode = 0; mode < 2; mode++)
            if (program->pipeline[mode]) api->pipeline_release(program->pipeline[mode]);
        if (program->module) api->shader_module_release(program->module);
    }
    free(tz_wgpu.programs);
    tz_wgpu.programs = NULL;
    tz_wgpu.program_count = tz_wgpu.program_capacity = 0;
    if (tz_wgpu.queue) api->queue_release(tz_wgpu.queue);
    if (tz_wgpu.device) api->device_release(tz_wgpu.device);
    if (tz_wgpu.adapter) api->adapter_release(tz_wgpu.adapter);
    if (tz_wgpu.instance) api->instance_release(tz_wgpu.instance);
    tz_wgpu.queue = NULL;
    tz_wgpu.device = NULL;
    tz_wgpu.adapter = NULL;
    tz_wgpu.instance = NULL;
}

static int tz_wgpu_load(void) {
    const char *requested = getenv("TSUZURI_WEBGPU_LIBRARY");
    tz_gpu_library library = NULL;
    if (requested != NULL) {
        if (requested[0] == '\0') {
            if (tz_gpu_debug()) tz_gpu_report("webgpu", "TSUZURI_WEBGPU_LIBRARY is empty; the backend is disabled");
            return TZ_GPU_UNAVAILABLE;
        }
        library = tz_gpu_open_library(requested);
    } else {
        for (const char *const *name = tz_gpu_default_libraries; *name && !library; name++) library = tz_gpu_open_default(*name);
    }
    if (!library) {
        if (tz_gpu_debug()) tz_gpu_report("webgpu", "no WebGPU library (wgpu-native) could be loaded; set TSUZURI_WEBGPU_LIBRARY");
        return TZ_GPU_UNAVAILABLE;
    }
    tz_wgpu.library = library;
    void *version = tz_gpu_symbol(library, "wgpuGetVersion");
    if (!version) {
        if (tz_gpu_debug()) tz_gpu_report("webgpu", "the library has no wgpuGetVersion; only wgpu-native 29 is supported");
        return TZ_GPU_UNSUPPORTED;
    }
    tz_wgpu.api.get_version = (uint32_t (*)(void))version;
    // A release reports major<<24 | minor<<16 | patch<<8 | build. A build from source (Homebrew's, for
    // one) reports 0, which the runtime accepts only for a library that TSUZURI_WEBGPU_LIBRARY names:
    // the user then vouches that it is wgpu-native 29.
    uint32_t number = tz_wgpu.api.get_version();
    if (number == 0 ? requested == NULL : (number >> 24) != TZ_WGPU_MAJOR) {
        if (tz_gpu_debug()) {
            if (number == 0) tz_gpu_report("webgpu", "the library reports no version; set TSUZURI_WEBGPU_LIBRARY to use a build of wgpu-native 29");
            else fprintf(stderr, "tsuzuri gpu: webgpu: wgpu-native %u.%u.%u.%u is not supported; the runtime needs major version %d\n", number >> 24, (number >> 16) & 255, (number >> 8) & 255, number & 255, TZ_WGPU_MAJOR);
        }
        return TZ_GPU_UNSUPPORTED;
    }
#define TZ_LOAD(field, name) \
    if (strcmp(#field, "get_version") != 0) { \
        void *symbol = tz_gpu_symbol(library, name); \
        if (!symbol) { \
            if (tz_gpu_debug()) tz_gpu_report("webgpu", "the library lacks " name); \
            return TZ_GPU_UNSUPPORTED; \
        } \
        memcpy(&tz_wgpu.api.field, &symbol, sizeof symbol); \
    }
    TZ_GPU_SYMBOLS(TZ_LOAD)
#undef TZ_LOAD
    return TZ_GPU_OK;
}

static int tz_wgpu_create(int32_t features) {
    struct tz_wgpu_api *api = &tz_wgpu.api;
    tz_wgpu.instance = api->create_instance(NULL);
    if (!tz_wgpu.instance) return TZ_GPU_UNAVAILABLE;
    WGPURequestAdapterOptions options;
    memset(&options, 0, sizeof options);
    options.powerPreference = WGPUPowerPreference_HighPerformance;
    WGPURequestAdapterCallbackInfo adapter_info = {NULL, WGPUCallbackMode_AllowProcessEvents, tz_wgpu_adapter_done, NULL, tz_wgpu_begin()};
    api->instance_request_adapter(tz_wgpu.instance, &options, adapter_info);
    if (!tz_wgpu_await(0) || tz_wgpu_slot.status != WGPURequestAdapterStatus_Success || !tz_wgpu_slot.object) {
        if (tz_gpu_debug()) tz_gpu_report("webgpu", "no adapter");
        return TZ_GPU_UNAVAILABLE;
    }
    tz_wgpu.adapter = tz_wgpu_slot.object;
    WGPULimits limits;
    memset(&limits, 0, sizeof limits);
    if (api->adapter_get_limits(tz_wgpu.adapter, &limits) != WGPUStatus_Success || limits.maxComputeInvocationsPerWorkgroup < TZ_WGPU_GROUP || limits.maxComputeWorkgroupSizeX < TZ_WGPU_GROUP) {
        if (tz_gpu_debug()) tz_gpu_report("webgpu", "the adapter does not support workgroups of 256 invocations");
        return TZ_GPU_UNSUPPORTED;
    }
    uint32_t required[1];
    size_t required_count = 0;
    if (features & TZ_GPU_FEATURE_F16) {
        if (!api->adapter_has_feature(tz_wgpu.adapter, WGPUFeatureName_ShaderF16)) {
            if (tz_gpu_debug()) tz_gpu_report("webgpu", "the adapter lacks shader-f16, which the program's kernels need");
            return TZ_GPU_UNSUPPORTED;
        }
        required[required_count++] = WGPUFeatureName_ShaderF16;
    }
    WGPUDeviceDescriptor description;
    memset(&description, 0, sizeof description);
    description.requiredFeatureCount = required_count;
    description.requiredFeatures = required_count ? required : NULL;
    description.deviceLostCallbackInfo.mode = WGPUCallbackMode_AllowProcessEvents;
    description.deviceLostCallbackInfo.callback = tz_wgpu_lost;
    description.uncapturedErrorCallbackInfo.callback = tz_wgpu_uncaptured;
    WGPURequestDeviceCallbackInfo device_info = {NULL, WGPUCallbackMode_AllowProcessEvents, tz_wgpu_device_done, NULL, tz_wgpu_begin()};
    api->adapter_request_device(tz_wgpu.adapter, &description, device_info);
    if (!tz_wgpu_await(0) || tz_wgpu_slot.status != WGPURequestDeviceStatus_Success || !tz_wgpu_slot.object) {
        if (tz_gpu_debug()) tz_gpu_report("webgpu", tz_wgpu.message[0] ? tz_wgpu.message : "no device");
        return TZ_GPU_UNAVAILABLE;
    }
    tz_wgpu.device = tz_wgpu_slot.object;
    tz_wgpu.queue = api->device_get_queue(tz_wgpu.device);
    // The device has the default limits, whatever the adapter offers, so the limits of a run come from the device.
    WGPULimits device_limits;
    memset(&device_limits, 0, sizeof device_limits);
    if (!tz_wgpu.queue || api->device_get_limits(tz_wgpu.device, &device_limits) != WGPUStatus_Success || device_limits.maxComputeInvocationsPerWorkgroup < TZ_WGPU_GROUP || device_limits.maxComputeWorkgroupSizeX < TZ_WGPU_GROUP) {
        if (tz_gpu_debug()) tz_gpu_report("webgpu", "the device has no queue or does not report workgroups of 256 invocations");
        return TZ_GPU_UNSUPPORTED;
    }
    tz_wgpu.features = features;
    tz_wgpu.max_buffer = device_limits.maxStorageBufferBindingSize < device_limits.maxBufferSize ? device_limits.maxStorageBufferBindingSize : device_limits.maxBufferSize;
    tz_wgpu.max_groups = device_limits.maxComputeWorkgroupsPerDimension;
    return TZ_GPU_OK;
}

static int tz_wgpu_open(int32_t features) {
    if (tz_wgpu.attempted) {
        if (tz_wgpu.status != TZ_GPU_OK) return tz_wgpu.status;
        if ((features & ~tz_wgpu.features) != 0) {
            if (tz_gpu_debug()) tz_gpu_report("webgpu", "the device was opened without a feature that the program's kernels need");
            return TZ_GPU_UNSUPPORTED;
        }
        return TZ_GPU_OK;
    }
    tz_wgpu.attempted = 1;
    tz_wgpu.status = tz_wgpu_load();
    if (tz_wgpu.status == TZ_GPU_OK) tz_wgpu.status = tz_wgpu_create(features);
    if (tz_wgpu.status != TZ_GPU_OK) {
        tz_wgpu_close();
    } else {
        atexit(tz_wgpu_close);
    }
    return tz_wgpu.status;
}

static size_t tz_gpu_lane_bytes(int kind) {
    return kind == TZ_GPU_LANE_F16 ? 2 : 4;
}

// A program is a shader module with its two entry points; one per distinct kernel source.
static struct tz_wgpu_program *tz_wgpu_program(const void *source, int32_t length) {
    for (size_t index = 0; index < tz_wgpu.program_count; index++) {
        struct tz_wgpu_program *program = &tz_wgpu.programs[index];
        if (program->source == source && program->length == length) return program;
    }
    if (tz_wgpu.program_count == tz_wgpu.program_capacity) {
        size_t capacity = tz_wgpu.program_capacity ? tz_wgpu.program_capacity * 2 : 8;
        struct tz_wgpu_program *grown = realloc(tz_wgpu.programs, capacity * sizeof *grown);
        if (!grown) return NULL;
        tz_wgpu.programs = grown;
        tz_wgpu.program_capacity = capacity;
    }
    struct tz_wgpu_program *program = &tz_wgpu.programs[tz_wgpu.program_count++];
    memset(program, 0, sizeof *program);
    program->source = source;
    program->length = length;
    return program;
}

// A shader module or a pipeline that failed is released and never kept: wgpu-native hands out an invalid object after an
// error, and a later call that found it in the program would submit it, which panics inside the library.
static WGPUComputePipeline tz_wgpu_pipeline(struct tz_wgpu_program *program, int mode) {
    struct tz_wgpu_api *api = &tz_wgpu.api;
    if (!program->module) {
        WGPUShaderSourceWGSL wgsl;
        memset(&wgsl, 0, sizeof wgsl);
        wgsl.chain.sType = WGPUSType_ShaderSourceWGSL;
        wgsl.code.data = program->source;
        wgsl.code.length = (size_t)program->length;
        WGPUShaderModuleDescriptor description;
        memset(&description, 0, sizeof description);
        description.nextInChain = &wgsl.chain;
        WGPUShaderModule module = api->device_create_shader_module(tz_wgpu.device, &description);
        if (!module || tz_wgpu.failed) {
            if (module) api->shader_module_release(module);
            return NULL;
        }
        program->module = module;
    }
    if (!program->pipeline[mode]) {
        WGPUComputePipelineDescriptor description;
        memset(&description, 0, sizeof description);
        description.compute.module = program->module;
        description.compute.entryPoint.data = mode ? "init_main" : "map_main";
        description.compute.entryPoint.length = WGPU_STRLEN;
        WGPUComputePipeline pipeline = api->device_create_compute_pipeline(tz_wgpu.device, &description);
        if (!pipeline || tz_wgpu.failed) {
            if (pipeline) api->pipeline_release(pipeline);
            return NULL;
        }
        program->pipeline[mode] = pipeline;
    }
    return program->pipeline[mode];
}

static WGPUBuffer tz_wgpu_buffer(uint64_t size, WGPUBufferUsage usage) {
    WGPUBufferDescriptor description;
    memset(&description, 0, sizeof description);
    description.usage = usage;
    description.size = size;
    return tz_wgpu.api.device_create_buffer(tz_wgpu.device, &description);
}

// Writes bytes to a buffer whose size is rounded up to a multiple of 4, as WebGPU requires.
static void tz_wgpu_write(WGPUBuffer buffer, const void *data, size_t bytes) {
    size_t whole = bytes & ~(size_t)3;
    if (whole) tz_wgpu.api.queue_write_buffer(tz_wgpu.queue, buffer, 0, data, whole);
    if (bytes != whole) {
        unsigned char tail[4] = {0, 0, 0, 0};
        memcpy(tail, (const unsigned char *)data + whole, bytes - whole);
        tz_wgpu.api.queue_write_buffer(tz_wgpu.queue, buffer, whole, tail, 4);
    }
}

static int tz_wgpu_run(int32_t mode, int32_t flags, int32_t lanes, const void *wgsl, int32_t wgsl_length, const void *input, int64_t count, void *output) {
    struct tz_wgpu_api *api = &tz_wgpu.api;
    (void)flags;
    if (!tz_wgpu.device) {
        tz_gpu_report("webgpu", "no device is open; a WebGPU device comes from Gpu.request Gpu.WebGpu");
        return TZ_GPU_UNAVAILABLE;
    }
    if (!wgsl || wgsl_length <= 0) {
        tz_gpu_report("webgpu", "this call has no WGSL kernel: a strict Gpu.map or Gpu.init runs on a WebGPU device only with i32 or i32u lanes; use Gpu.map_relaxed or Gpu.init_relaxed for f32 or f16");
        return TZ_GPU_UNSUPPORTED;
    }
    if (count == 0) return TZ_GPU_OK;
    if (tz_gpu_debug()) fprintf(stderr, "tsuzuri gpu: webgpu: %s %lld lanes, kinds 0x%04x\n", mode ? "init" : "map", (long long)count, (unsigned)lanes);
    // A loss that happened since the last call arrives with the events. A device that was lost, or that did not answer
    // in time, is not asked again: every later run fails at once with the reason.
    api->instance_process_events(tz_wgpu.instance);
    if (tz_wgpu.lost || tz_wgpu.hung) {
        tz_gpu_report("webgpu: run", tz_wgpu.lost && tz_wgpu.message[0] ? tz_wgpu.message : "the device did not answer in time; it is not used again");
        return TZ_GPU_FAILED;
    }
    // The sizes are computed in 64 bits, so no count wraps a 32-bit size_t.
    uint64_t input_bytes = (uint64_t)count * tz_gpu_lane_bytes(lanes & 255);
    uint64_t output_bytes = (uint64_t)count * tz_gpu_lane_bytes((lanes >> 8) & 255);
    uint64_t input_size = (input_bytes + 3) & ~(uint64_t)3;
    uint64_t output_size = (output_bytes + 3) & ~(uint64_t)3;
    uint64_t groups = ((uint64_t)count + TZ_WGPU_GROUP - 1) / TZ_WGPU_GROUP;
    // The limits are those of the opened device. A call over them is refused here, before anything is created or
    // submitted: a binding or a dispatch over a limit is a validation error that wgpu-native turns into a panic.
    if (input_size > tz_wgpu.max_buffer || output_size > tz_wgpu.max_buffer || groups > tz_wgpu.max_groups) {
        tz_gpu_report("webgpu", "the number of lanes exceeds the device limit");
        return TZ_GPU_LIMIT;
    }
    tz_wgpu.message[0] = '\0';
    tz_wgpu.failed = 0;
    struct tz_wgpu_program *program = tz_wgpu_program(wgsl, wgsl_length);
    WGPUComputePipeline pipeline = program ? tz_wgpu_pipeline(program, mode ? 1 : 0) : NULL;
    if (!pipeline) {
        tz_gpu_report("webgpu: shader", tz_wgpu.message[0] ? tz_wgpu.message : "the kernel could not be compiled");
        return TZ_GPU_FAILED;
    }
    WGPUBuffer input_buffer = NULL, output_buffer = NULL, uniform = NULL, readback = NULL;
    WGPUBindGroupLayout layout = NULL;
    WGPUBindGroup group = NULL;
    WGPUCommandEncoder encoder = NULL;
    WGPUComputePassEncoder pass = NULL;
    WGPUCommandBuffer commands = NULL;
    int status = TZ_GPU_FAILED;
    // After an error wgpu-native still hands out an object, an invalid one, and a call that names it (a write to an
    // invalid buffer, a submit of a command that uses an invalid bind group) panics inside the library, which aborts the
    // process. So every object is checked, with the error state, before the next call uses it, and a failure ends the
    // call with a status, never with a submit.
#define TZ_WGPU_USABLE(object) ((object) != NULL && !tz_wgpu.failed)
    if (mode == 0) {
        input_buffer = tz_wgpu_buffer(input_size, WGPUBufferUsage_Storage | WGPUBufferUsage_CopyDst);
        if (!TZ_WGPU_USABLE(input_buffer)) goto finish;
        tz_wgpu_write(input_buffer, input, (size_t)input_bytes);
    }
    output_buffer = tz_wgpu_buffer(output_size, WGPUBufferUsage_Storage | WGPUBufferUsage_CopySrc);
    if (!TZ_WGPU_USABLE(output_buffer)) goto finish;
    uniform = tz_wgpu_buffer(16, WGPUBufferUsage_Uniform | WGPUBufferUsage_CopyDst);
    if (!TZ_WGPU_USABLE(uniform)) goto finish;
    readback = tz_wgpu_buffer(output_size, WGPUBufferUsage_MapRead | WGPUBufferUsage_CopyDst);
    if (!TZ_WGPU_USABLE(readback)) goto finish;
    uint32_t parameters[4] = {(uint32_t)count, 0, 0, 0};
    api->queue_write_buffer(tz_wgpu.queue, uniform, 0, parameters, sizeof parameters);
    layout = api->pipeline_get_bind_group_layout(pipeline, 0);
    if (!TZ_WGPU_USABLE(layout)) goto finish;
    WGPUBindGroupEntry entries[3];
    memset(entries, 0, sizeof entries);
    size_t entry_count = 0;
    if (mode == 0) {
        entries[entry_count].binding = 0;
        entries[entry_count].buffer = input_buffer;
        entries[entry_count].size = input_size;
        entry_count++;
    }
    entries[entry_count].binding = 1;
    entries[entry_count].buffer = output_buffer;
    entries[entry_count].size = output_size;
    entry_count++;
    entries[entry_count].binding = 2;
    entries[entry_count].buffer = uniform;
    entries[entry_count].size = 16;
    entry_count++;
    WGPUBindGroupDescriptor group_description;
    memset(&group_description, 0, sizeof group_description);
    group_description.layout = layout;
    group_description.entryCount = entry_count;
    group_description.entries = entries;
    group = api->device_create_bind_group(tz_wgpu.device, &group_description);
    if (!TZ_WGPU_USABLE(group)) goto finish;
    encoder = api->device_create_command_encoder(tz_wgpu.device, NULL);
    if (!TZ_WGPU_USABLE(encoder)) goto finish;
    pass = api->encoder_begin_compute_pass(encoder, NULL);
    if (!TZ_WGPU_USABLE(pass)) goto finish;
    api->pass_set_pipeline(pass, pipeline);
    api->pass_set_bind_group(pass, 0, group, 0, NULL);
    api->pass_dispatch(pass, (uint32_t)groups, 1, 1);
    api->pass_end(pass);
    api->encoder_copy_buffer_to_buffer(encoder, output_buffer, 0, readback, 0, output_size);
    commands = api->encoder_finish(encoder, NULL);
    if (!TZ_WGPU_USABLE(commands)) goto finish;
    api->queue_submit(tz_wgpu.queue, 1, &commands);
    WGPUBufferMapCallbackInfo map_info = {NULL, WGPUCallbackMode_AllowProcessEvents, tz_wgpu_map_done, NULL, tz_wgpu_begin()};
    api->buffer_map_async(readback, WGPUMapMode_Read, 0, (size_t)output_size, map_info);
    if (!tz_wgpu_await(1)) {
        static const char timeout[] = "the device did not finish the kernel in time";
        tz_wgpu_note(timeout, sizeof timeout - 1);
        goto finish;
    }
    if (tz_wgpu_slot.status != WGPUMapAsyncStatus_Success || tz_wgpu.failed) goto finish;
    const void *mapped = api->buffer_get_const_mapped_range(readback, 0, (size_t)output_size);
    if (!mapped) goto finish;
    memcpy(output, mapped, (size_t)output_bytes);
    api->buffer_unmap(readback);
    status = TZ_GPU_OK;
#undef TZ_WGPU_USABLE
finish:
    if (pass) api->pass_release(pass);
    if (commands) api->command_buffer_release(commands);
    if (encoder) api->encoder_release(encoder);
    if (group) api->bind_group_release(group);
    if (layout) api->bind_group_layout_release(layout);
    if (readback) { api->buffer_destroy(readback); api->buffer_release(readback); }
    if (uniform) { api->buffer_destroy(uniform); api->buffer_release(uniform); }
    if (output_buffer) { api->buffer_destroy(output_buffer); api->buffer_release(output_buffer); }
    if (input_buffer) { api->buffer_destroy(input_buffer); api->buffer_release(input_buffer); }
    if (status != TZ_GPU_OK) tz_gpu_report("webgpu: run", tz_wgpu.message[0] ? tz_wgpu.message : "the dispatch failed");
    return status;
}

// ---- The entry points -----------------------------------------------------------------------------

// The Vulkan backend (F09 Phase 3) is in src/runtime/gpu-vulkan.c, which the driver puts before this file
// in the same translation unit when the program names `Gpu.Vulkan` or `Gpu.Auto` and defines TZ_GPU_VULKAN. It
// has its own lock, so a Vulkan call does not wait for a WebGPU call.

TZ_GPU_API int32_t tsuzuri_gpu_open(int32_t backend, int32_t features) {
    int status = TZ_GPU_UNAVAILABLE;
#ifdef TZ_GPU_VULKAN
    if (backend == 2) return tz_vulkan_open(features);
#endif
    tz_gpu_acquire();
    if (backend == 1) status = tz_wgpu_open(features);
    tz_gpu_release();
    return status;
}

TZ_GPU_API int32_t tsuzuri_gpu_run(int32_t backend, int32_t mode, int32_t flags, int32_t lanes, const void *wgsl, int32_t wgsl_length, const void *spirv, int32_t spirv_length, const void *input, int64_t count, void *output) {
    int status = TZ_GPU_UNAVAILABLE;
#ifdef TZ_GPU_VULKAN
    if (backend == 2) return tz_vulkan_run(mode, flags, lanes, wgsl, wgsl_length, spirv, spirv_length, input, count, output);
#endif
    (void)spirv;
    (void)spirv_length;
    tz_gpu_acquire();
    if (backend == 1) status = tz_wgpu_run(mode, flags, lanes, wgsl, wgsl_length, input, count, output);
    tz_gpu_release();
    return status;
}

// The backend that serves one call of a `Gpu.Auto` device (F09 Phase 3): 0 is the CPU reference and 2 is Vulkan.
// Only a backend with a measured cost rule is a candidate, and that is Vulkan alone: WebGPU has no such rule yet,
// so it never serves an `Auto` call. The arguments are the call's mode and its kernel descriptor.
TZ_GPU_API int32_t tsuzuri_gpu_select(int32_t mode, int32_t lanes, int32_t features, const void *spirv, int32_t spirv_length, int32_t weight, int64_t count) {
#ifdef TZ_GPU_VULKAN
    if (tz_vulkan_auto(mode, lanes, features, spirv, spirv_length, weight, count)) return 2;
#else
    (void)mode;
    (void)lanes;
    (void)features;
    (void)spirv;
    (void)spirv_length;
    (void)weight;
    (void)count;
#endif
    return 0;
}
