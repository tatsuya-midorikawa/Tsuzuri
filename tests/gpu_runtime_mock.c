// Mock "WebGPU libraries" for tests/gpu_runtime.mjs. The runtime of src/runtime/gpu.c loads a library
// by name, reads wgpuGetVersion, and must fail closed (Result.Error Gpu.Unavailable, never a crash
// or a silent CPU run) when the library is not the wgpu-native 29 that it declares the API of.
//   -DMOCK_VERSION=0x1C000000u   a wgpu-native 28 (a different C API)
//   -DMOCK_NO_VERSION            a library without wgpuGetVersion (another WebGPU implementation)
//   -DMOCK_VERSION=0x1D000101u   the right version, but none of the other functions
#include <stdint.h>

#ifndef MOCK_VERSION
#define MOCK_VERSION 0x1C000000u
#endif

#if defined(_WIN32)
#define MOCK_EXPORT __declspec(dllexport)
#else
#define MOCK_EXPORT __attribute__((visibility("default")))
#endif

#ifndef MOCK_NO_VERSION
MOCK_EXPORT uint32_t wgpuGetVersion(void) { return MOCK_VERSION; }
#else
MOCK_EXPORT int mock_without_version(void) { return 1; }
#endif
