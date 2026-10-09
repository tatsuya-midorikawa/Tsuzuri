/* The Vulkan backend of the GPU runtime (F09 Phase 3, backend code 2).
 *
 * The library is loaded at run time (dlopen / LoadLibrary), so a program that never asks for Vulkan has no
 * link-time or load-time dependency on it. TSUZURI_VULKAN_LIBRARY names the library exactly; when it is set
 * and cannot be loaded the backend is unavailable, and no other library is tried. The small part of the Vulkan
 * ABI that is needed is declared here by hand and checked against the real headers by tests/gpu_vulkan_abi.c.
 *
 * Entry points (called by the backend table of gpu.c, with the status codes of that table):
 *   tz_vulkan_open(features)  0 ok, 1 unavailable, 2 unsupported
 *   tz_vulkan_run(...)        0 ok, 1 unavailable, 2 unsupported, 3 limit exceeded, 4 failed
 * `features` carries the device features the program's kernels need (bit 1 = shaderInt64, bit 2 = the strict
 * float32 controls); the bits of other backends are ignored. Initialization is lazy, serialized by one mutex, and
 * idempotent: the outcome of the first attempt is kept for the life of the process. Every run submits one command
 * buffer, waits for its fence, and releases its buffers before it returns. */
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
#endif
#include <stdarg.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#ifndef TZ_VK_ALLOC
#define TZ_VK_ALLOC(bytes) malloc(bytes)
#define TZ_VK_FREE(pointer) free(pointer)
#endif

enum {
    TZ_VK_OK = 0,
    TZ_VK_UNAVAILABLE = 1,
    TZ_VK_UNSUPPORTED = 2,
    TZ_VK_LIMIT = 3,
    TZ_VK_FAILED = 4,
};

enum {
    TZ_VK_FEATURE_INT64 = 1 << 1,
    TZ_VK_FEATURE_STRICT_F32 = 1 << 2,
    TZ_VK_FEATURE_MASK = TZ_VK_FEATURE_INT64 | TZ_VK_FEATURE_STRICT_F32,
};

/* ---- the Vulkan ABI subset ---- */

#if defined(_WIN32) && !defined(_WIN64)
#define TZ_VKAPI __stdcall
#else
#define TZ_VKAPI
#endif

#if defined(__LP64__) || defined(_WIN64) || (defined(__x86_64__) && !defined(__ILP32__)) \
    || defined(_M_X64) || defined(__ia64) || defined(_M_IA64) || defined(__aarch64__) \
    || defined(__powerpc64__) || (defined(__riscv) && __riscv_xlen == 64)
#define TZ_VK_NON_DISPATCHABLE(name) typedef struct name##_T *name
#else
#define TZ_VK_NON_DISPATCHABLE(name) typedef uint64_t name
#endif

typedef uint32_t VkFlags;
typedef uint32_t VkBool32;
typedef uint64_t VkDeviceSize;
typedef int32_t VkResult;
typedef int32_t VkStructureType;
typedef struct VkInstance_T *VkInstance;
typedef struct VkPhysicalDevice_T *VkPhysicalDevice;
typedef struct VkDevice_T *VkDevice;
typedef struct VkQueue_T *VkQueue;
typedef struct VkCommandBuffer_T *VkCommandBuffer;
TZ_VK_NON_DISPATCHABLE(VkBuffer);
TZ_VK_NON_DISPATCHABLE(VkDeviceMemory);
TZ_VK_NON_DISPATCHABLE(VkShaderModule);
TZ_VK_NON_DISPATCHABLE(VkPipeline);
TZ_VK_NON_DISPATCHABLE(VkPipelineLayout);
TZ_VK_NON_DISPATCHABLE(VkDescriptorSetLayout);
TZ_VK_NON_DISPATCHABLE(VkDescriptorPool);
TZ_VK_NON_DISPATCHABLE(VkDescriptorSet);
TZ_VK_NON_DISPATCHABLE(VkCommandPool);
TZ_VK_NON_DISPATCHABLE(VkFence);
TZ_VK_NON_DISPATCHABLE(VkSampler);
TZ_VK_NON_DISPATCHABLE(VkPipelineCache);
typedef struct VkAllocationCallbacks VkAllocationCallbacks;
typedef struct VkSpecializationInfo VkSpecializationInfo;
typedef struct VkCommandBufferInheritanceInfo VkCommandBufferInheritanceInfo;
typedef struct VkDescriptorImageInfo VkDescriptorImageInfo;
typedef struct VkImageMemoryBarrier VkImageMemoryBarrier;
typedef struct VkBufferMemoryBarrier VkBufferMemoryBarrier;
TZ_VK_NON_DISPATCHABLE(VkSemaphore);
TZ_VK_NON_DISPATCHABLE(VkBufferView);

enum {
    VK_SUCCESS = 0,
    VK_INCOMPLETE = 5,
    VK_ERROR_OUT_OF_HOST_MEMORY = -1,
    VK_ERROR_OUT_OF_DEVICE_MEMORY = -2,
    VK_ERROR_INITIALIZATION_FAILED = -3,
    VK_ERROR_DEVICE_LOST = -4,
    VK_ERROR_INCOMPATIBLE_DRIVER = -9,
};

enum {
    VK_STRUCTURE_TYPE_APPLICATION_INFO = 0,
    VK_STRUCTURE_TYPE_INSTANCE_CREATE_INFO = 1,
    VK_STRUCTURE_TYPE_DEVICE_QUEUE_CREATE_INFO = 2,
    VK_STRUCTURE_TYPE_DEVICE_CREATE_INFO = 3,
    VK_STRUCTURE_TYPE_SUBMIT_INFO = 4,
    VK_STRUCTURE_TYPE_MEMORY_ALLOCATE_INFO = 5,
    VK_STRUCTURE_TYPE_FENCE_CREATE_INFO = 8,
    VK_STRUCTURE_TYPE_BUFFER_CREATE_INFO = 12,
    VK_STRUCTURE_TYPE_SHADER_MODULE_CREATE_INFO = 16,
    VK_STRUCTURE_TYPE_PIPELINE_SHADER_STAGE_CREATE_INFO = 18,
    VK_STRUCTURE_TYPE_COMPUTE_PIPELINE_CREATE_INFO = 29,
    VK_STRUCTURE_TYPE_PIPELINE_LAYOUT_CREATE_INFO = 30,
    VK_STRUCTURE_TYPE_DESCRIPTOR_SET_LAYOUT_CREATE_INFO = 32,
    VK_STRUCTURE_TYPE_DESCRIPTOR_POOL_CREATE_INFO = 33,
    VK_STRUCTURE_TYPE_DESCRIPTOR_SET_ALLOCATE_INFO = 34,
    VK_STRUCTURE_TYPE_WRITE_DESCRIPTOR_SET = 35,
    VK_STRUCTURE_TYPE_COMMAND_POOL_CREATE_INFO = 39,
    VK_STRUCTURE_TYPE_COMMAND_BUFFER_ALLOCATE_INFO = 40,
    VK_STRUCTURE_TYPE_COMMAND_BUFFER_BEGIN_INFO = 42,
    VK_STRUCTURE_TYPE_MEMORY_BARRIER = 46,
    VK_STRUCTURE_TYPE_PHYSICAL_DEVICE_FEATURES_2 = 1000059000,
    VK_STRUCTURE_TYPE_PHYSICAL_DEVICE_PROPERTIES_2 = 1000059001,
    VK_STRUCTURE_TYPE_PHYSICAL_DEVICE_MAINTENANCE_3_PROPERTIES = 1000168000,
    VK_STRUCTURE_TYPE_PHYSICAL_DEVICE_FLOAT_CONTROLS_PROPERTIES = 1000197000,
};

enum {
    VK_INSTANCE_CREATE_ENUMERATE_PORTABILITY_BIT_KHR = 0x00000001,
    VK_QUEUE_COMPUTE_BIT = 0x00000002,
    VK_PHYSICAL_DEVICE_TYPE_OTHER = 0,
    VK_PHYSICAL_DEVICE_TYPE_INTEGRATED_GPU = 1,
    VK_PHYSICAL_DEVICE_TYPE_DISCRETE_GPU = 2,
    VK_PHYSICAL_DEVICE_TYPE_VIRTUAL_GPU = 3,
    VK_PHYSICAL_DEVICE_TYPE_CPU = 4,
    VK_MEMORY_PROPERTY_DEVICE_LOCAL_BIT = 0x00000001,
    VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT = 0x00000002,
    VK_MEMORY_PROPERTY_HOST_COHERENT_BIT = 0x00000004,
    VK_BUFFER_USAGE_TRANSFER_SRC_BIT = 0x00000001,
    VK_BUFFER_USAGE_TRANSFER_DST_BIT = 0x00000002,
    VK_BUFFER_USAGE_STORAGE_BUFFER_BIT = 0x00000020,
    VK_SHARING_MODE_EXCLUSIVE = 0,
    VK_DESCRIPTOR_TYPE_STORAGE_BUFFER = 7,
    VK_SHADER_STAGE_COMPUTE_BIT = 0x00000020,
    VK_COMMAND_POOL_CREATE_RESET_COMMAND_BUFFER_BIT = 0x00000002,
    VK_COMMAND_BUFFER_LEVEL_PRIMARY = 0,
    VK_COMMAND_BUFFER_USAGE_ONE_TIME_SUBMIT_BIT = 0x00000001,
    VK_PIPELINE_BIND_POINT_COMPUTE = 1,
    VK_PIPELINE_STAGE_TRANSFER_BIT = 0x00001000,
    VK_PIPELINE_STAGE_COMPUTE_SHADER_BIT = 0x00000800,
    VK_PIPELINE_STAGE_HOST_BIT = 0x00004000,
    VK_ACCESS_SHADER_READ_BIT = 0x00000020,
    VK_ACCESS_SHADER_WRITE_BIT = 0x00000040,
    VK_ACCESS_TRANSFER_READ_BIT = 0x00000800,
    VK_ACCESS_TRANSFER_WRITE_BIT = 0x00001000,
    VK_ACCESS_HOST_READ_BIT = 0x00002000,
    VK_SHADER_FLOAT_CONTROLS_INDEPENDENCE_32_BIT_ONLY = 0,
    VK_SHADER_FLOAT_CONTROLS_INDEPENDENCE_ALL = 1,
    VK_SHADER_FLOAT_CONTROLS_INDEPENDENCE_NONE = 2,
};

#define VK_WHOLE_SIZE (~0ULL)
#define VK_MAKE_API_VERSION(variant, major, minor, patch) \
    ((((uint32_t)(variant)) << 29) | (((uint32_t)(major)) << 22) | (((uint32_t)(minor)) << 12) | ((uint32_t)(patch)))
#define VK_API_VERSION_MAJOR(version) (((uint32_t)(version) >> 22) & 0x7FU)
#define VK_API_VERSION_MINOR(version) (((uint32_t)(version) >> 12) & 0x3FFU)
#define VK_API_VERSION_PATCH(version) ((uint32_t)(version) & 0xFFFU)

typedef struct VkApplicationInfo {
    VkStructureType sType;
    const void *pNext;
    const char *pApplicationName;
    uint32_t applicationVersion;
    const char *pEngineName;
    uint32_t engineVersion;
    uint32_t apiVersion;
} VkApplicationInfo;

typedef struct VkInstanceCreateInfo {
    VkStructureType sType;
    const void *pNext;
    VkFlags flags;
    const VkApplicationInfo *pApplicationInfo;
    uint32_t enabledLayerCount;
    const char *const *ppEnabledLayerNames;
    uint32_t enabledExtensionCount;
    const char *const *ppEnabledExtensionNames;
} VkInstanceCreateInfo;

typedef struct VkExtensionProperties {
    char extensionName[256];
    uint32_t specVersion;
} VkExtensionProperties;

typedef struct VkPhysicalDeviceFeatures {
    VkBool32 robustBufferAccess;
    VkBool32 fullDrawIndexUint32;
    VkBool32 imageCubeArray;
    VkBool32 independentBlend;
    VkBool32 geometryShader;
    VkBool32 tessellationShader;
    VkBool32 sampleRateShading;
    VkBool32 dualSrcBlend;
    VkBool32 logicOp;
    VkBool32 multiDrawIndirect;
    VkBool32 drawIndirectFirstInstance;
    VkBool32 depthClamp;
    VkBool32 depthBiasClamp;
    VkBool32 fillModeNonSolid;
    VkBool32 depthBounds;
    VkBool32 wideLines;
    VkBool32 largePoints;
    VkBool32 alphaToOne;
    VkBool32 multiViewport;
    VkBool32 samplerAnisotropy;
    VkBool32 textureCompressionETC2;
    VkBool32 textureCompressionASTC_LDR;
    VkBool32 textureCompressionBC;
    VkBool32 occlusionQueryPrecise;
    VkBool32 pipelineStatisticsQuery;
    VkBool32 vertexPipelineStoresAndAtomics;
    VkBool32 fragmentStoresAndAtomics;
    VkBool32 shaderTessellationAndGeometryPointSize;
    VkBool32 shaderImageGatherExtended;
    VkBool32 shaderStorageImageExtendedFormats;
    VkBool32 shaderStorageImageMultisample;
    VkBool32 shaderStorageImageReadWithoutFormat;
    VkBool32 shaderStorageImageWriteWithoutFormat;
    VkBool32 shaderUniformBufferArrayDynamicIndexing;
    VkBool32 shaderSampledImageArrayDynamicIndexing;
    VkBool32 shaderStorageBufferArrayDynamicIndexing;
    VkBool32 shaderStorageImageArrayDynamicIndexing;
    VkBool32 shaderClipDistance;
    VkBool32 shaderCullDistance;
    VkBool32 shaderFloat64;
    VkBool32 shaderInt64;
    VkBool32 shaderInt16;
    VkBool32 shaderResourceResidency;
    VkBool32 shaderResourceMinLod;
    VkBool32 sparseBinding;
    VkBool32 sparseResidencyBuffer;
    VkBool32 sparseResidencyImage2D;
    VkBool32 sparseResidencyImage3D;
    VkBool32 sparseResidency2Samples;
    VkBool32 sparseResidency4Samples;
    VkBool32 sparseResidency8Samples;
    VkBool32 sparseResidency16Samples;
    VkBool32 sparseResidencyAliased;
    VkBool32 variableMultisampleRate;
    VkBool32 inheritedQueries;
} VkPhysicalDeviceFeatures;

typedef struct VkPhysicalDeviceLimits {
    uint32_t maxImageDimension1D;
    uint32_t maxImageDimension2D;
    uint32_t maxImageDimension3D;
    uint32_t maxImageDimensionCube;
    uint32_t maxImageArrayLayers;
    uint32_t maxTexelBufferElements;
    uint32_t maxUniformBufferRange;
    uint32_t maxStorageBufferRange;
    uint32_t maxPushConstantsSize;
    uint32_t maxMemoryAllocationCount;
    uint32_t maxSamplerAllocationCount;
    VkDeviceSize bufferImageGranularity;
    VkDeviceSize sparseAddressSpaceSize;
    uint32_t maxBoundDescriptorSets;
    uint32_t maxPerStageDescriptorSamplers;
    uint32_t maxPerStageDescriptorUniformBuffers;
    uint32_t maxPerStageDescriptorStorageBuffers;
    uint32_t maxPerStageDescriptorSampledImages;
    uint32_t maxPerStageDescriptorStorageImages;
    uint32_t maxPerStageDescriptorInputAttachments;
    uint32_t maxPerStageResources;
    uint32_t maxDescriptorSetSamplers;
    uint32_t maxDescriptorSetUniformBuffers;
    uint32_t maxDescriptorSetUniformBuffersDynamic;
    uint32_t maxDescriptorSetStorageBuffers;
    uint32_t maxDescriptorSetStorageBuffersDynamic;
    uint32_t maxDescriptorSetSampledImages;
    uint32_t maxDescriptorSetStorageImages;
    uint32_t maxDescriptorSetInputAttachments;
    uint32_t maxVertexInputAttributes;
    uint32_t maxVertexInputBindings;
    uint32_t maxVertexInputAttributeOffset;
    uint32_t maxVertexInputBindingStride;
    uint32_t maxVertexOutputComponents;
    uint32_t maxTessellationGenerationLevel;
    uint32_t maxTessellationPatchSize;
    uint32_t maxTessellationControlPerVertexInputComponents;
    uint32_t maxTessellationControlPerVertexOutputComponents;
    uint32_t maxTessellationControlPerPatchOutputComponents;
    uint32_t maxTessellationControlTotalOutputComponents;
    uint32_t maxTessellationEvaluationInputComponents;
    uint32_t maxTessellationEvaluationOutputComponents;
    uint32_t maxGeometryShaderInvocations;
    uint32_t maxGeometryInputComponents;
    uint32_t maxGeometryOutputComponents;
    uint32_t maxGeometryOutputVertices;
    uint32_t maxGeometryTotalOutputComponents;
    uint32_t maxFragmentInputComponents;
    uint32_t maxFragmentOutputAttachments;
    uint32_t maxFragmentDualSrcAttachments;
    uint32_t maxFragmentCombinedOutputResources;
    uint32_t maxComputeSharedMemorySize;
    uint32_t maxComputeWorkGroupCount[3];
    uint32_t maxComputeWorkGroupInvocations;
    uint32_t maxComputeWorkGroupSize[3];
    uint32_t subPixelPrecisionBits;
    uint32_t subTexelPrecisionBits;
    uint32_t mipmapPrecisionBits;
    uint32_t maxDrawIndexedIndexValue;
    uint32_t maxDrawIndirectCount;
    float maxSamplerLodBias;
    float maxSamplerAnisotropy;
    uint32_t maxViewports;
    uint32_t maxViewportDimensions[2];
    float viewportBoundsRange[2];
    uint32_t viewportSubPixelBits;
    size_t minMemoryMapAlignment;
    VkDeviceSize minTexelBufferOffsetAlignment;
    VkDeviceSize minUniformBufferOffsetAlignment;
    VkDeviceSize minStorageBufferOffsetAlignment;
    int32_t minTexelOffset;
    uint32_t maxTexelOffset;
    int32_t minTexelGatherOffset;
    uint32_t maxTexelGatherOffset;
    float minInterpolationOffset;
    float maxInterpolationOffset;
    uint32_t subPixelInterpolationOffsetBits;
    uint32_t maxFramebufferWidth;
    uint32_t maxFramebufferHeight;
    uint32_t maxFramebufferLayers;
    VkFlags framebufferColorSampleCounts;
    VkFlags framebufferDepthSampleCounts;
    VkFlags framebufferStencilSampleCounts;
    VkFlags framebufferNoAttachmentsSampleCounts;
    uint32_t maxColorAttachments;
    VkFlags sampledImageColorSampleCounts;
    VkFlags sampledImageIntegerSampleCounts;
    VkFlags sampledImageDepthSampleCounts;
    VkFlags sampledImageStencilSampleCounts;
    VkFlags storageImageSampleCounts;
    uint32_t maxSampleMaskWords;
    VkBool32 timestampComputeAndGraphics;
    float timestampPeriod;
    uint32_t maxClipDistances;
    uint32_t maxCullDistances;
    uint32_t maxCombinedClipAndCullDistances;
    uint32_t discreteQueuePriorities;
    float pointSizeRange[2];
    float lineWidthRange[2];
    float pointSizeGranularity;
    float lineWidthGranularity;
    VkBool32 strictLines;
    VkBool32 standardSampleLocations;
    VkDeviceSize optimalBufferCopyOffsetAlignment;
    VkDeviceSize optimalBufferCopyRowPitchAlignment;
    VkDeviceSize nonCoherentAtomSize;
} VkPhysicalDeviceLimits;

typedef struct VkPhysicalDeviceSparseProperties {
    VkBool32 residencyStandard2DBlockShape;
    VkBool32 residencyStandard2DMultisampleBlockShape;
    VkBool32 residencyStandard3DBlockShape;
    VkBool32 residencyAlignedMipSize;
    VkBool32 residencyNonResidentStrict;
} VkPhysicalDeviceSparseProperties;

typedef struct VkPhysicalDeviceProperties {
    uint32_t apiVersion;
    uint32_t driverVersion;
    uint32_t vendorID;
    uint32_t deviceID;
    int32_t deviceType;
    char deviceName[256];
    uint8_t pipelineCacheUUID[16];
    VkPhysicalDeviceLimits limits;
    VkPhysicalDeviceSparseProperties sparseProperties;
} VkPhysicalDeviceProperties;

typedef struct VkPhysicalDeviceProperties2 {
    VkStructureType sType;
    void *pNext;
    VkPhysicalDeviceProperties properties;
} VkPhysicalDeviceProperties2;

typedef struct VkPhysicalDeviceMaintenance3Properties {
    VkStructureType sType;
    void *pNext;
    uint32_t maxPerSetDescriptors;
    VkDeviceSize maxMemoryAllocationSize;
} VkPhysicalDeviceMaintenance3Properties;

typedef struct VkPhysicalDeviceFloatControlsProperties {
    VkStructureType sType;
    void *pNext;
    int32_t denormBehaviorIndependence;
    int32_t roundingModeIndependence;
    VkBool32 shaderSignedZeroInfNanPreserveFloat16;
    VkBool32 shaderSignedZeroInfNanPreserveFloat32;
    VkBool32 shaderSignedZeroInfNanPreserveFloat64;
    VkBool32 shaderDenormPreserveFloat16;
    VkBool32 shaderDenormPreserveFloat32;
    VkBool32 shaderDenormPreserveFloat64;
    VkBool32 shaderDenormFlushToZeroFloat16;
    VkBool32 shaderDenormFlushToZeroFloat32;
    VkBool32 shaderDenormFlushToZeroFloat64;
    VkBool32 shaderRoundingModeRTEFloat16;
    VkBool32 shaderRoundingModeRTEFloat32;
    VkBool32 shaderRoundingModeRTEFloat64;
    VkBool32 shaderRoundingModeRTZFloat16;
    VkBool32 shaderRoundingModeRTZFloat32;
    VkBool32 shaderRoundingModeRTZFloat64;
} VkPhysicalDeviceFloatControlsProperties;

typedef struct VkMemoryType {
    VkFlags propertyFlags;
    uint32_t heapIndex;
} VkMemoryType;

typedef struct VkMemoryHeap {
    VkDeviceSize size;
    VkFlags flags;
} VkMemoryHeap;

typedef struct VkPhysicalDeviceMemoryProperties {
    uint32_t memoryTypeCount;
    VkMemoryType memoryTypes[32];
    uint32_t memoryHeapCount;
    VkMemoryHeap memoryHeaps[16];
} VkPhysicalDeviceMemoryProperties;

typedef struct VkExtent3D {
    uint32_t width;
    uint32_t height;
    uint32_t depth;
} VkExtent3D;

typedef struct VkQueueFamilyProperties {
    VkFlags queueFlags;
    uint32_t queueCount;
    uint32_t timestampValidBits;
    VkExtent3D minImageTransferGranularity;
} VkQueueFamilyProperties;

typedef struct VkDeviceQueueCreateInfo {
    VkStructureType sType;
    const void *pNext;
    VkFlags flags;
    uint32_t queueFamilyIndex;
    uint32_t queueCount;
    const float *pQueuePriorities;
} VkDeviceQueueCreateInfo;

typedef struct VkDeviceCreateInfo {
    VkStructureType sType;
    const void *pNext;
    VkFlags flags;
    uint32_t queueCreateInfoCount;
    const VkDeviceQueueCreateInfo *pQueueCreateInfos;
    uint32_t enabledLayerCount;
    const char *const *ppEnabledLayerNames;
    uint32_t enabledExtensionCount;
    const char *const *ppEnabledExtensionNames;
    const VkPhysicalDeviceFeatures *pEnabledFeatures;
} VkDeviceCreateInfo;

typedef struct VkBufferCreateInfo {
    VkStructureType sType;
    const void *pNext;
    VkFlags flags;
    VkDeviceSize size;
    VkFlags usage;
    int32_t sharingMode;
    uint32_t queueFamilyIndexCount;
    const uint32_t *pQueueFamilyIndices;
} VkBufferCreateInfo;

typedef struct VkMemoryRequirements {
    VkDeviceSize size;
    VkDeviceSize alignment;
    uint32_t memoryTypeBits;
} VkMemoryRequirements;

typedef struct VkMemoryAllocateInfo {
    VkStructureType sType;
    const void *pNext;
    VkDeviceSize allocationSize;
    uint32_t memoryTypeIndex;
} VkMemoryAllocateInfo;

typedef struct VkDescriptorSetLayoutBinding {
    uint32_t binding;
    int32_t descriptorType;
    uint32_t descriptorCount;
    VkFlags stageFlags;
    const VkSampler *pImmutableSamplers;
} VkDescriptorSetLayoutBinding;

typedef struct VkDescriptorSetLayoutCreateInfo {
    VkStructureType sType;
    const void *pNext;
    VkFlags flags;
    uint32_t bindingCount;
    const VkDescriptorSetLayoutBinding *pBindings;
} VkDescriptorSetLayoutCreateInfo;

typedef struct VkPushConstantRange {
    VkFlags stageFlags;
    uint32_t offset;
    uint32_t size;
} VkPushConstantRange;

typedef struct VkPipelineLayoutCreateInfo {
    VkStructureType sType;
    const void *pNext;
    VkFlags flags;
    uint32_t setLayoutCount;
    const VkDescriptorSetLayout *pSetLayouts;
    uint32_t pushConstantRangeCount;
    const VkPushConstantRange *pPushConstantRanges;
} VkPipelineLayoutCreateInfo;

typedef struct VkShaderModuleCreateInfo {
    VkStructureType sType;
    const void *pNext;
    VkFlags flags;
    size_t codeSize;
    const uint32_t *pCode;
} VkShaderModuleCreateInfo;

typedef struct VkPipelineShaderStageCreateInfo {
    VkStructureType sType;
    const void *pNext;
    VkFlags flags;
    VkFlags stage;
    VkShaderModule module;
    const char *pName;
    const VkSpecializationInfo *pSpecializationInfo;
} VkPipelineShaderStageCreateInfo;

typedef struct VkComputePipelineCreateInfo {
    VkStructureType sType;
    const void *pNext;
    VkFlags flags;
    VkPipelineShaderStageCreateInfo stage;
    VkPipelineLayout layout;
    VkPipeline basePipelineHandle;
    int32_t basePipelineIndex;
} VkComputePipelineCreateInfo;

typedef struct VkDescriptorPoolSize {
    int32_t type;
    uint32_t descriptorCount;
} VkDescriptorPoolSize;

typedef struct VkDescriptorPoolCreateInfo {
    VkStructureType sType;
    const void *pNext;
    VkFlags flags;
    uint32_t maxSets;
    uint32_t poolSizeCount;
    const VkDescriptorPoolSize *pPoolSizes;
} VkDescriptorPoolCreateInfo;

typedef struct VkDescriptorSetAllocateInfo {
    VkStructureType sType;
    const void *pNext;
    VkDescriptorPool descriptorPool;
    uint32_t descriptorSetCount;
    const VkDescriptorSetLayout *pSetLayouts;
} VkDescriptorSetAllocateInfo;

typedef struct VkDescriptorBufferInfo {
    VkBuffer buffer;
    VkDeviceSize offset;
    VkDeviceSize range;
} VkDescriptorBufferInfo;

typedef struct VkWriteDescriptorSet {
    VkStructureType sType;
    const void *pNext;
    VkDescriptorSet dstSet;
    uint32_t dstBinding;
    uint32_t dstArrayElement;
    uint32_t descriptorCount;
    int32_t descriptorType;
    const VkDescriptorImageInfo *pImageInfo;
    const VkDescriptorBufferInfo *pBufferInfo;
    const VkBufferView *pTexelBufferView;
} VkWriteDescriptorSet;

typedef struct VkCommandPoolCreateInfo {
    VkStructureType sType;
    const void *pNext;
    VkFlags flags;
    uint32_t queueFamilyIndex;
} VkCommandPoolCreateInfo;

typedef struct VkCommandBufferAllocateInfo {
    VkStructureType sType;
    const void *pNext;
    VkCommandPool commandPool;
    int32_t level;
    uint32_t commandBufferCount;
} VkCommandBufferAllocateInfo;

typedef struct VkCommandBufferBeginInfo {
    VkStructureType sType;
    const void *pNext;
    VkFlags flags;
    const VkCommandBufferInheritanceInfo *pInheritanceInfo;
} VkCommandBufferBeginInfo;

typedef struct VkBufferCopy {
    VkDeviceSize srcOffset;
    VkDeviceSize dstOffset;
    VkDeviceSize size;
} VkBufferCopy;

typedef struct VkMemoryBarrier {
    VkStructureType sType;
    const void *pNext;
    VkFlags srcAccessMask;
    VkFlags dstAccessMask;
} VkMemoryBarrier;

typedef struct VkSubmitInfo {
    VkStructureType sType;
    const void *pNext;
    uint32_t waitSemaphoreCount;
    const VkSemaphore *pWaitSemaphores;
    const VkFlags *pWaitDstStageMask;
    uint32_t commandBufferCount;
    const VkCommandBuffer *pCommandBuffers;
    uint32_t signalSemaphoreCount;
    const VkSemaphore *pSignalSemaphores;
} VkSubmitInfo;

typedef struct VkFenceCreateInfo {
    VkStructureType sType;
    const void *pNext;
    VkFlags flags;
} VkFenceCreateInfo;

typedef void(TZ_VKAPI *PFN_vkVoidFunction)(void);
PFN_vkVoidFunction TZ_VKAPI vkGetInstanceProcAddr(VkInstance instance, const char *pName);
PFN_vkVoidFunction TZ_VKAPI vkGetDeviceProcAddr(VkDevice device, const char *pName);
VkResult TZ_VKAPI vkEnumerateInstanceVersion(uint32_t *pApiVersion);
VkResult TZ_VKAPI vkEnumerateInstanceExtensionProperties(const char *pLayerName, uint32_t *pPropertyCount, VkExtensionProperties *pProperties);
VkResult TZ_VKAPI vkCreateInstance(const VkInstanceCreateInfo *pCreateInfo, const VkAllocationCallbacks *pAllocator, VkInstance *pInstance);
void TZ_VKAPI vkDestroyInstance(VkInstance instance, const VkAllocationCallbacks *pAllocator);
VkResult TZ_VKAPI vkEnumeratePhysicalDevices(VkInstance instance, uint32_t *pPhysicalDeviceCount, VkPhysicalDevice *pPhysicalDevices);
void TZ_VKAPI vkGetPhysicalDeviceFeatures(VkPhysicalDevice physicalDevice, VkPhysicalDeviceFeatures *pFeatures);
void TZ_VKAPI vkGetPhysicalDeviceProperties2(VkPhysicalDevice physicalDevice, VkPhysicalDeviceProperties2 *pProperties);
void TZ_VKAPI vkGetPhysicalDeviceQueueFamilyProperties(VkPhysicalDevice physicalDevice, uint32_t *pQueueFamilyPropertyCount, VkQueueFamilyProperties *pQueueFamilyProperties);
void TZ_VKAPI vkGetPhysicalDeviceMemoryProperties(VkPhysicalDevice physicalDevice, VkPhysicalDeviceMemoryProperties *pMemoryProperties);
VkResult TZ_VKAPI vkEnumerateDeviceExtensionProperties(VkPhysicalDevice physicalDevice, const char *pLayerName, uint32_t *pPropertyCount, VkExtensionProperties *pProperties);
VkResult TZ_VKAPI vkCreateDevice(VkPhysicalDevice physicalDevice, const VkDeviceCreateInfo *pCreateInfo, const VkAllocationCallbacks *pAllocator, VkDevice *pDevice);
void TZ_VKAPI vkDestroyDevice(VkDevice device, const VkAllocationCallbacks *pAllocator);
void TZ_VKAPI vkGetDeviceQueue(VkDevice device, uint32_t queueFamilyIndex, uint32_t queueIndex, VkQueue *pQueue);
VkResult TZ_VKAPI vkDeviceWaitIdle(VkDevice device);
VkResult TZ_VKAPI vkCreateBuffer(VkDevice device, const VkBufferCreateInfo *pCreateInfo, const VkAllocationCallbacks *pAllocator, VkBuffer *pBuffer);
void TZ_VKAPI vkDestroyBuffer(VkDevice device, VkBuffer buffer, const VkAllocationCallbacks *pAllocator);
void TZ_VKAPI vkGetBufferMemoryRequirements(VkDevice device, VkBuffer buffer, VkMemoryRequirements *pMemoryRequirements);
VkResult TZ_VKAPI vkAllocateMemory(VkDevice device, const VkMemoryAllocateInfo *pAllocateInfo, const VkAllocationCallbacks *pAllocator, VkDeviceMemory *pMemory);
void TZ_VKAPI vkFreeMemory(VkDevice device, VkDeviceMemory memory, const VkAllocationCallbacks *pAllocator);
VkResult TZ_VKAPI vkBindBufferMemory(VkDevice device, VkBuffer buffer, VkDeviceMemory memory, VkDeviceSize memoryOffset);
VkResult TZ_VKAPI vkMapMemory(VkDevice device, VkDeviceMemory memory, VkDeviceSize offset, VkDeviceSize size, VkFlags flags, void **ppData);
void TZ_VKAPI vkUnmapMemory(VkDevice device, VkDeviceMemory memory);
VkResult TZ_VKAPI vkCreateDescriptorSetLayout(VkDevice device, const VkDescriptorSetLayoutCreateInfo *pCreateInfo, const VkAllocationCallbacks *pAllocator, VkDescriptorSetLayout *pSetLayout);
void TZ_VKAPI vkDestroyDescriptorSetLayout(VkDevice device, VkDescriptorSetLayout descriptorSetLayout, const VkAllocationCallbacks *pAllocator);
VkResult TZ_VKAPI vkCreatePipelineLayout(VkDevice device, const VkPipelineLayoutCreateInfo *pCreateInfo, const VkAllocationCallbacks *pAllocator, VkPipelineLayout *pPipelineLayout);
void TZ_VKAPI vkDestroyPipelineLayout(VkDevice device, VkPipelineLayout pipelineLayout, const VkAllocationCallbacks *pAllocator);
VkResult TZ_VKAPI vkCreateShaderModule(VkDevice device, const VkShaderModuleCreateInfo *pCreateInfo, const VkAllocationCallbacks *pAllocator, VkShaderModule *pShaderModule);
void TZ_VKAPI vkDestroyShaderModule(VkDevice device, VkShaderModule shaderModule, const VkAllocationCallbacks *pAllocator);
VkResult TZ_VKAPI vkCreateComputePipelines(VkDevice device, VkPipelineCache pipelineCache, uint32_t createInfoCount, const VkComputePipelineCreateInfo *pCreateInfos, const VkAllocationCallbacks *pAllocator, VkPipeline *pPipelines);
void TZ_VKAPI vkDestroyPipeline(VkDevice device, VkPipeline pipeline, const VkAllocationCallbacks *pAllocator);
VkResult TZ_VKAPI vkCreateDescriptorPool(VkDevice device, const VkDescriptorPoolCreateInfo *pCreateInfo, const VkAllocationCallbacks *pAllocator, VkDescriptorPool *pDescriptorPool);
void TZ_VKAPI vkDestroyDescriptorPool(VkDevice device, VkDescriptorPool descriptorPool, const VkAllocationCallbacks *pAllocator);
VkResult TZ_VKAPI vkAllocateDescriptorSets(VkDevice device, const VkDescriptorSetAllocateInfo *pAllocateInfo, VkDescriptorSet *pDescriptorSets);
void TZ_VKAPI vkUpdateDescriptorSets(VkDevice device, uint32_t descriptorWriteCount, const VkWriteDescriptorSet *pDescriptorWrites, uint32_t descriptorCopyCount, const void *pDescriptorCopies);
VkResult TZ_VKAPI vkCreateCommandPool(VkDevice device, const VkCommandPoolCreateInfo *pCreateInfo, const VkAllocationCallbacks *pAllocator, VkCommandPool *pCommandPool);
void TZ_VKAPI vkDestroyCommandPool(VkDevice device, VkCommandPool commandPool, const VkAllocationCallbacks *pAllocator);
VkResult TZ_VKAPI vkAllocateCommandBuffers(VkDevice device, const VkCommandBufferAllocateInfo *pAllocateInfo, VkCommandBuffer *pCommandBuffers);
VkResult TZ_VKAPI vkBeginCommandBuffer(VkCommandBuffer commandBuffer, const VkCommandBufferBeginInfo *pBeginInfo);
VkResult TZ_VKAPI vkEndCommandBuffer(VkCommandBuffer commandBuffer);
VkResult TZ_VKAPI vkResetCommandBuffer(VkCommandBuffer commandBuffer, VkFlags flags);
void TZ_VKAPI vkCmdBindPipeline(VkCommandBuffer commandBuffer, int32_t pipelineBindPoint, VkPipeline pipeline);
void TZ_VKAPI vkCmdBindDescriptorSets(VkCommandBuffer commandBuffer, int32_t pipelineBindPoint, VkPipelineLayout layout, uint32_t firstSet, uint32_t descriptorSetCount, const VkDescriptorSet *pDescriptorSets, uint32_t dynamicOffsetCount, const uint32_t *pDynamicOffsets);
void TZ_VKAPI vkCmdPushConstants(VkCommandBuffer commandBuffer, VkPipelineLayout layout, VkFlags stageFlags, uint32_t offset, uint32_t size, const void *pValues);
void TZ_VKAPI vkCmdDispatch(VkCommandBuffer commandBuffer, uint32_t groupCountX, uint32_t groupCountY, uint32_t groupCountZ);
void TZ_VKAPI vkCmdCopyBuffer(VkCommandBuffer commandBuffer, VkBuffer srcBuffer, VkBuffer dstBuffer, uint32_t regionCount, const VkBufferCopy *pRegions);
void TZ_VKAPI vkCmdPipelineBarrier(VkCommandBuffer commandBuffer, VkFlags srcStageMask, VkFlags dstStageMask, VkFlags dependencyFlags, uint32_t memoryBarrierCount, const VkMemoryBarrier *pMemoryBarriers, uint32_t bufferMemoryBarrierCount, const VkBufferMemoryBarrier *pBufferMemoryBarriers, uint32_t imageMemoryBarrierCount, const VkImageMemoryBarrier *pImageMemoryBarriers);
VkResult TZ_VKAPI vkQueueSubmit(VkQueue queue, uint32_t submitCount, const VkSubmitInfo *pSubmits, VkFence fence);
VkResult TZ_VKAPI vkCreateFence(VkDevice device, const VkFenceCreateInfo *pCreateInfo, const VkAllocationCallbacks *pAllocator, VkFence *pFence);
void TZ_VKAPI vkDestroyFence(VkDevice device, VkFence fence, const VkAllocationCallbacks *pAllocator);
VkResult TZ_VKAPI vkResetFences(VkDevice device, uint32_t fenceCount, const VkFence *pFences);
VkResult TZ_VKAPI vkWaitForFences(VkDevice device, uint32_t fenceCount, const VkFence *pFences, VkBool32 waitAll, uint64_t timeout);

/* The functions are loaded by name from the library, so the tables below are the only link between the
   declarations above and the library (a missing function makes the backend unavailable). */
#define TZ_VK_GLOBAL_FUNCTIONS(X) \
    X(vkEnumerateInstanceExtensionProperties) X(vkCreateInstance)
#define TZ_VK_INSTANCE_FUNCTIONS(X) \
    X(vkGetDeviceProcAddr) X(vkDestroyInstance) X(vkEnumeratePhysicalDevices) X(vkGetPhysicalDeviceFeatures) \
    X(vkGetPhysicalDeviceProperties2) X(vkGetPhysicalDeviceQueueFamilyProperties) \
    X(vkGetPhysicalDeviceMemoryProperties) X(vkEnumerateDeviceExtensionProperties) X(vkCreateDevice)
#define TZ_VK_DEVICE_FUNCTIONS(X) \
    X(vkDestroyDevice) X(vkGetDeviceQueue) X(vkDeviceWaitIdle) X(vkCreateBuffer) X(vkDestroyBuffer) \
    X(vkGetBufferMemoryRequirements) X(vkAllocateMemory) X(vkFreeMemory) X(vkBindBufferMemory) X(vkMapMemory) \
    X(vkUnmapMemory) X(vkCreateDescriptorSetLayout) X(vkDestroyDescriptorSetLayout) X(vkCreatePipelineLayout) \
    X(vkDestroyPipelineLayout) X(vkCreateShaderModule) X(vkDestroyShaderModule) X(vkCreateComputePipelines) \
    X(vkDestroyPipeline) X(vkCreateDescriptorPool) X(vkDestroyDescriptorPool) X(vkAllocateDescriptorSets) \
    X(vkUpdateDescriptorSets) X(vkCreateCommandPool) X(vkDestroyCommandPool) X(vkAllocateCommandBuffers) \
    X(vkBeginCommandBuffer) X(vkEndCommandBuffer) X(vkResetCommandBuffer) X(vkCmdBindPipeline) \
    X(vkCmdBindDescriptorSets) X(vkCmdPushConstants) X(vkCmdDispatch) X(vkCmdCopyBuffer) \
    X(vkCmdPipelineBarrier) X(vkQueueSubmit) X(vkCreateFence) X(vkDestroyFence) X(vkResetFences) X(vkWaitForFences)

struct tz_vk_functions {
#define TZ_VK_MEMBER(name) __typeof__(&name) name;
    TZ_VK_GLOBAL_FUNCTIONS(TZ_VK_MEMBER)
    TZ_VK_INSTANCE_FUNCTIONS(TZ_VK_MEMBER)
    TZ_VK_DEVICE_FUNCTIONS(TZ_VK_MEMBER)
#undef TZ_VK_MEMBER
    __typeof__(&vkGetInstanceProcAddr) vkGetInstanceProcAddr;
    __typeof__(&vkEnumerateInstanceVersion) vkEnumerateInstanceVersion;
};

/* ---- the state of the backend ---- */

#define TZ_VK_MAX_DEVICES 16
#define TZ_VK_PROGRAMS 256
#define TZ_VK_GROUP_SIZE 256
#define TZ_VK_MAX_LANES 2147483647LL

struct tz_vk_caps {
    char name[256];
    uint32_t api_version;
    uint32_t vendor;
    uint32_t device_id;
    int32_t device_type;
    uint32_t max_workgroup_invocations;
    uint32_t max_workgroup_size_x;
    uint32_t max_group_count_x;
    uint32_t max_storage_range;
    uint64_t max_allocation;
    int int64;
    int float_controls;
    int zero_inf_nan_preserve32;
    int denorm_preserve32;
    int rounding_rte32;
    int denorm_independence;
    int rounding_independence;
    int strict_f32;
    int direct_transfer;
};

struct tz_vk_program {
    uint32_t *words;
    uint32_t length;
    VkShaderModule module;
    VkPipeline pipelines[2];
};

struct tz_vk_state {
    int state; /* 0 not tried, 1 ready, 2 failed */
    int32_t status;
    int poisoned;
    int exit_registered;
    void *library;
    struct tz_vk_functions fn;
    VkInstance instance;
    VkPhysicalDevice physical;
    VkDevice device;
    VkQueue queue;
    uint32_t queue_family;
    VkCommandPool pool;
    VkCommandBuffer commands;
    VkFence fence;
    VkDescriptorSetLayout set_layout;
    VkPipelineLayout pipeline_layout;
    VkDescriptorPool descriptors;
    VkDescriptorSet set;
    VkPhysicalDeviceMemoryProperties memory;
    struct tz_vk_caps caps;
    struct tz_vk_program programs[TZ_VK_PROGRAMS];
    uint32_t program_count;
};

static struct tz_vk_state tz_vk;

#if defined(_WIN32)
static SRWLOCK tz_vk_mutex = SRWLOCK_INIT;
#define TZ_VK_LOCK() AcquireSRWLockExclusive(&tz_vk_mutex)
#define TZ_VK_UNLOCK() ReleaseSRWLockExclusive(&tz_vk_mutex)
#else
static pthread_mutex_t tz_vk_mutex = PTHREAD_MUTEX_INITIALIZER;
#define TZ_VK_LOCK() pthread_mutex_lock(&tz_vk_mutex)
#define TZ_VK_UNLOCK() pthread_mutex_unlock(&tz_vk_mutex)
#endif

static int tz_vk_debug_enabled(void) {
    const char *value = getenv("TSUZURI_GPU_DEBUG");
    return value != NULL && value[0] != '\0' && strcmp(value, "0") != 0;
}

static void tz_vk_debug(const char *format, ...) {
    if (!tz_vk_debug_enabled()) return;
    va_list arguments;
    va_start(arguments, format);
    fputs("tsuzuri: Vulkan: ", stderr);
    vfprintf(stderr, format, arguments);
    fputc('\n', stderr);
    va_end(arguments);
}

/* Reports why a run failed; the status is returned by the caller. */
static int32_t tz_vk_report(int32_t status, const char *format, ...) {
    va_list arguments;
    va_start(arguments, format);
    fputs("tsuzuri: Vulkan: ", stderr);
    vfprintf(stderr, format, arguments);
    fputc('\n', stderr);
    va_end(arguments);
    return status;
}

/* ---- loading ---- */

static void *tz_vk_open_library(const char *name) {
#if defined(_WIN32)
    return (void *)LoadLibraryA(name);
#else
    return dlopen(name, RTLD_NOW | RTLD_LOCAL);
#endif
}

static void *tz_vk_symbol(void *library, const char *name) {
#if defined(_WIN32)
    return (void *)GetProcAddress((HMODULE)library, name);
#else
    return dlsym(library, name);
#endif
}

static void *tz_vk_find_library(void) {
    const char *override = getenv("TSUZURI_VULKAN_LIBRARY");
    if (override != NULL && override[0] != '\0') {
        void *library = tz_vk_open_library(override);
        if (library == NULL) tz_vk_debug("TSUZURI_VULKAN_LIBRARY=%s cannot be loaded", override);
        return library;
    }
    static const char *const names[] = {
#if defined(_WIN32)
        "vulkan-1.dll",
#elif defined(__APPLE__)
        "libvulkan.1.dylib", "libvulkan.dylib", "/opt/homebrew/lib/libvulkan.1.dylib",
        "/usr/local/lib/libvulkan.1.dylib", "libMoltenVK.dylib", "/opt/homebrew/lib/libMoltenVK.dylib",
        "/usr/local/lib/libMoltenVK.dylib",
#else
        "libvulkan.so.1", "libvulkan.so",
#endif
        NULL};
    for (size_t index = 0; names[index] != NULL; index++) {
        void *library = tz_vk_open_library(names[index]);
        if (library != NULL) return library;
    }
    tz_vk_debug("no Vulkan loader library was found");
    return NULL;
}

/* Every function of the tables must exist; a library that lacks one is not a usable Vulkan 1.1 loader. All
   functions are looked up even after a miss, so that the teardown of a partial load sees what exists. */
static int tz_vk_load_global_functions(void) {
    struct tz_vk_functions *fn = &tz_vk.fn;
    fn->vkGetInstanceProcAddr = (__typeof__(&vkGetInstanceProcAddr))tz_vk_symbol(tz_vk.library, "vkGetInstanceProcAddr");
    if (fn->vkGetInstanceProcAddr == NULL) return 0;
    fn->vkEnumerateInstanceVersion =
        (__typeof__(&vkEnumerateInstanceVersion))fn->vkGetInstanceProcAddr(NULL, "vkEnumerateInstanceVersion");
    int complete = 1;
#define TZ_VK_LOAD(name) \
    fn->name = (__typeof__(&name))fn->vkGetInstanceProcAddr(NULL, #name); \
    complete &= fn->name != NULL;
    TZ_VK_GLOBAL_FUNCTIONS(TZ_VK_LOAD)
#undef TZ_VK_LOAD
    return complete;
}

static int tz_vk_load_instance_functions(void) {
    struct tz_vk_functions *fn = &tz_vk.fn;
    int complete = 1;
#define TZ_VK_LOAD(name) \
    fn->name = (__typeof__(&name))fn->vkGetInstanceProcAddr(tz_vk.instance, #name); \
    complete &= fn->name != NULL;
    TZ_VK_INSTANCE_FUNCTIONS(TZ_VK_LOAD)
#undef TZ_VK_LOAD
    return complete;
}

static int tz_vk_load_device_functions(void) {
    struct tz_vk_functions *fn = &tz_vk.fn;
    int complete = 1;
#define TZ_VK_LOAD(name) \
    fn->name = (__typeof__(&name))fn->vkGetDeviceProcAddr(tz_vk.device, #name); \
    complete &= fn->name != NULL;
    TZ_VK_DEVICE_FUNCTIONS(TZ_VK_LOAD)
#undef TZ_VK_LOAD
    return complete;
}

static int tz_vk_has_extension(const VkExtensionProperties *extensions, uint32_t count, const char *name) {
    for (uint32_t index = 0; index < count; index++) {
        if (strcmp(extensions[index].extensionName, name) == 0) return 1;
    }
    return 0;
}

/* ---- device selection and capabilities ---- */

struct tz_vk_candidate {
    VkPhysicalDevice device;
    struct tz_vk_caps caps;
    VkPhysicalDeviceMemoryProperties memory;
    uint32_t queue_family;
    int has_queue;
    int portability_subset;
    int float_controls_extension;
    int rank;
};

static int tz_vk_type_rank(int32_t type) {
    switch (type) {
    case VK_PHYSICAL_DEVICE_TYPE_DISCRETE_GPU: return 4;
    case VK_PHYSICAL_DEVICE_TYPE_INTEGRATED_GPU: return 3;
    case VK_PHYSICAL_DEVICE_TYPE_VIRTUAL_GPU: return 2;
    case VK_PHYSICAL_DEVICE_TYPE_OTHER: return 1;
    default: return 0;
    }
}

static int32_t tz_vk_describe_candidate(VkPhysicalDevice device, struct tz_vk_candidate *candidate) {
    struct tz_vk_functions *fn = &tz_vk.fn;
    memset(candidate, 0, sizeof *candidate);
    candidate->device = device;
    uint32_t count = 0;
    VkResult result = fn->vkEnumerateDeviceExtensionProperties(device, NULL, &count, NULL);
    if (result != VK_SUCCESS) return TZ_VK_UNAVAILABLE;
    VkExtensionProperties *extensions = NULL;
    if (count > 0) {
        extensions = (VkExtensionProperties *)TZ_VK_ALLOC(sizeof *extensions * count);
        if (extensions == NULL) return TZ_VK_FAILED;
        result = fn->vkEnumerateDeviceExtensionProperties(device, NULL, &count, extensions);
        if (result != VK_SUCCESS && result != VK_INCOMPLETE) {
            TZ_VK_FREE(extensions);
            return TZ_VK_UNAVAILABLE;
        }
    }
    candidate->portability_subset = extensions != NULL && tz_vk_has_extension(extensions, count, "VK_KHR_portability_subset");
    candidate->float_controls_extension =
        extensions != NULL && tz_vk_has_extension(extensions, count, "VK_KHR_shader_float_controls");
    TZ_VK_FREE(extensions);

    VkPhysicalDeviceProperties2 properties;
    VkPhysicalDeviceMaintenance3Properties maintenance;
    VkPhysicalDeviceFloatControlsProperties controls;
    memset(&properties, 0, sizeof properties);
    memset(&maintenance, 0, sizeof maintenance);
    memset(&controls, 0, sizeof controls);
    properties.sType = VK_STRUCTURE_TYPE_PHYSICAL_DEVICE_PROPERTIES_2;
    maintenance.sType = VK_STRUCTURE_TYPE_PHYSICAL_DEVICE_MAINTENANCE_3_PROPERTIES;
    controls.sType = VK_STRUCTURE_TYPE_PHYSICAL_DEVICE_FLOAT_CONTROLS_PROPERTIES;
    /* A structure is chained only when the device implements it: Maintenance3 is core 1.1; the float
       controls are core 1.2 or the extension. The api version is read first, from a plain query. */
    fn->vkGetPhysicalDeviceProperties2(device, &properties);
    uint32_t api = properties.properties.apiVersion;
    int controls_known = api >= VK_MAKE_API_VERSION(0, 1, 2, 0) || candidate->float_controls_extension;
    if (api >= VK_MAKE_API_VERSION(0, 1, 1, 0)) {
        if (controls_known) maintenance.pNext = &controls;
        properties.pNext = &maintenance;
        fn->vkGetPhysicalDeviceProperties2(device, &properties);
    }
    struct tz_vk_caps *caps = &candidate->caps;
    memcpy(caps->name, properties.properties.deviceName, sizeof caps->name);
    caps->name[sizeof caps->name - 1] = '\0';
    caps->api_version = properties.properties.apiVersion;
    caps->vendor = properties.properties.vendorID;
    caps->device_id = properties.properties.deviceID;
    caps->device_type = properties.properties.deviceType;
    caps->max_workgroup_invocations = properties.properties.limits.maxComputeWorkGroupInvocations;
    caps->max_workgroup_size_x = properties.properties.limits.maxComputeWorkGroupSize[0];
    caps->max_group_count_x = properties.properties.limits.maxComputeWorkGroupCount[0];
    caps->max_storage_range = properties.properties.limits.maxStorageBufferRange;
    caps->max_allocation = api >= VK_MAKE_API_VERSION(0, 1, 1, 0) && maintenance.maxMemoryAllocationSize != 0
        ? maintenance.maxMemoryAllocationSize
        : (uint64_t)1 << 30;
    VkPhysicalDeviceFeatures features;
    memset(&features, 0, sizeof features);
    fn->vkGetPhysicalDeviceFeatures(device, &features);
    caps->int64 = features.shaderInt64 != 0;
    if (controls_known) {
        caps->float_controls = 1;
        caps->zero_inf_nan_preserve32 = controls.shaderSignedZeroInfNanPreserveFloat32 != 0;
        caps->denorm_preserve32 = controls.shaderDenormPreserveFloat32 != 0;
        caps->rounding_rte32 = controls.shaderRoundingModeRTEFloat32 != 0;
        caps->denorm_independence = controls.denormBehaviorIndependence;
        caps->rounding_independence = controls.roundingModeIndependence;
        /* The strict kernels set the float controls of width 32 only, and with an independence of NONE the
           entry point must use the same modes for all widths (VUID-RuntimeSpirv-denormBehaviorIndependence-06290
           and -roundingModeIndependence-06292). */
        caps->strict_f32 = caps->zero_inf_nan_preserve32 && caps->denorm_preserve32 && caps->rounding_rte32
            && caps->denorm_independence != VK_SHADER_FLOAT_CONTROLS_INDEPENDENCE_NONE
            && caps->rounding_independence != VK_SHADER_FLOAT_CONTROLS_INDEPENDENCE_NONE;
    }

    uint32_t families = 0;
    fn->vkGetPhysicalDeviceQueueFamilyProperties(device, &families, NULL);
    if (families > 0) {
        VkQueueFamilyProperties *properties_list =
            (VkQueueFamilyProperties *)TZ_VK_ALLOC(sizeof *properties_list * families);
        if (properties_list == NULL) return TZ_VK_FAILED;
        fn->vkGetPhysicalDeviceQueueFamilyProperties(device, &families, properties_list);
        for (uint32_t index = 0; index < families; index++) {
            if ((properties_list[index].queueFlags & VK_QUEUE_COMPUTE_BIT) != 0 && properties_list[index].queueCount > 0) {
                candidate->queue_family = index;
                candidate->has_queue = 1;
                break;
            }
        }
        TZ_VK_FREE(properties_list);
    }
    memset(&candidate->memory, 0, sizeof candidate->memory);
    fn->vkGetPhysicalDeviceMemoryProperties(device, &candidate->memory);
    /* Unified memory: a type that the host maps and the device uses without a copy. A discrete GPU keeps the
       staged path even when such a type exists (a small BAR window, or reads over the bus). */
    const VkFlags unified = VK_MEMORY_PROPERTY_DEVICE_LOCAL_BIT | VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT
        | VK_MEMORY_PROPERTY_HOST_COHERENT_BIT;
    for (uint32_t index = 0; index < candidate->memory.memoryTypeCount && index < 32; index++) {
        if ((candidate->memory.memoryTypes[index].propertyFlags & unified) == unified
            && caps->device_type != VK_PHYSICAL_DEVICE_TYPE_DISCRETE_GPU) {
            caps->direct_transfer = 1;
        }
    }
    candidate->rank = tz_vk_type_rank(caps->device_type);
    return TZ_VK_OK;
}

static int tz_vk_satisfies(const struct tz_vk_caps *caps, int32_t features) {
    if ((features & TZ_VK_FEATURE_INT64) != 0 && !caps->int64) return 0;
    if ((features & TZ_VK_FEATURE_STRICT_F32) != 0 && !caps->strict_f32) return 0;
    return 1;
}

static const char *tz_vk_independence(int value) {
    switch (value) {
    case VK_SHADER_FLOAT_CONTROLS_INDEPENDENCE_32_BIT_ONLY: return "32-bit-only";
    case VK_SHADER_FLOAT_CONTROLS_INDEPENDENCE_ALL: return "all";
    default: return "none";
    }
}

static const char *tz_vk_type_name(int32_t type) {
    switch (type) {
    case VK_PHYSICAL_DEVICE_TYPE_DISCRETE_GPU: return "discrete";
    case VK_PHYSICAL_DEVICE_TYPE_INTEGRATED_GPU: return "integrated";
    case VK_PHYSICAL_DEVICE_TYPE_VIRTUAL_GPU: return "virtual";
    case VK_PHYSICAL_DEVICE_TYPE_CPU: return "cpu";
    default: return "other";
    }
}

static void tz_vk_format_caps(const struct tz_vk_caps *caps, char *text, size_t size) {
    snprintf(text, size,
        "device=\"%s\" type=%s api=%u.%u.%u int64=%d strict_f32=%d float_controls=%d signed_zero_inf_nan_preserve32=%d "
        "denorm_preserve32=%d rounding_rte32=%d denorm_independence=%s rounding_independence=%s workgroup_invocations=%u "
        "group_count_x=%u max_storage_range=%u max_allocation=%llu transfer=%s",
        caps->name, tz_vk_type_name(caps->device_type), VK_API_VERSION_MAJOR(caps->api_version),
        VK_API_VERSION_MINOR(caps->api_version), VK_API_VERSION_PATCH(caps->api_version), caps->int64, caps->strict_f32,
        caps->float_controls, caps->zero_inf_nan_preserve32, caps->denorm_preserve32, caps->rounding_rte32,
        tz_vk_independence(caps->denorm_independence), tz_vk_independence(caps->rounding_independence),
        caps->max_workgroup_invocations, caps->max_group_count_x, caps->max_storage_range,
        (unsigned long long)caps->max_allocation, caps->direct_transfer ? "direct" : "staged");
}

/* ---- teardown (also the cleanup of a failed initialization) ---- */

static void tz_vk_release_programs(void) {
    struct tz_vk_state *state = &tz_vk;
    for (uint32_t index = 0; index < state->program_count; index++) {
        struct tz_vk_program *program = &state->programs[index];
        for (int entry = 0; entry < 2; entry++) {
            if (program->pipelines[entry] != 0) state->fn.vkDestroyPipeline(state->device, program->pipelines[entry], NULL);
        }
        if (program->module != 0) state->fn.vkDestroyShaderModule(state->device, program->module, NULL);
        TZ_VK_FREE(program->words);
        memset(program, 0, sizeof *program);
    }
    state->program_count = 0;
}

static void tz_vk_teardown(void) {
    struct tz_vk_state *state = &tz_vk;
    if (state->device != NULL && state->fn.vkDestroyDevice != NULL) {
        if (state->fn.vkDeviceWaitIdle != NULL) state->fn.vkDeviceWaitIdle(state->device);
        tz_vk_release_programs();
        if (state->descriptors != 0) state->fn.vkDestroyDescriptorPool(state->device, state->descriptors, NULL);
        if (state->pipeline_layout != 0) state->fn.vkDestroyPipelineLayout(state->device, state->pipeline_layout, NULL);
        if (state->set_layout != 0) state->fn.vkDestroyDescriptorSetLayout(state->device, state->set_layout, NULL);
        if (state->fence != 0) state->fn.vkDestroyFence(state->device, state->fence, NULL);
        if (state->pool != 0) state->fn.vkDestroyCommandPool(state->device, state->pool, NULL);
        state->fn.vkDestroyDevice(state->device, NULL);
    }
    if (state->instance != NULL && state->fn.vkDestroyInstance != NULL) state->fn.vkDestroyInstance(state->instance, NULL);
    state->device = NULL;
    state->instance = NULL;
    state->queue = NULL;
    state->commands = NULL;
    state->pool = 0;
    state->fence = 0;
    state->set_layout = 0;
    state->pipeline_layout = 0;
    state->descriptors = 0;
    state->set = 0;
    state->physical = NULL;
    /* The library stays mapped: unloading a Vulkan loader while driver threads may still run is unsafe. */
}

static void tz_vk_at_exit(void) {
    TZ_VK_LOCK();
    if (tz_vk.state == 1) tz_vk_teardown();
    tz_vk.state = 2;
    tz_vk.status = TZ_VK_UNAVAILABLE;
    TZ_VK_UNLOCK();
}

/* ---- initialization ---- */

static int32_t tz_vk_fail_init(int32_t status, const char *what, VkResult result) {
    if (result != VK_SUCCESS) tz_vk_debug("%s failed (VkResult %d)", what, (int)result);
    else tz_vk_debug("%s", what);
    tz_vk_teardown();
    tz_vk.state = 2;
    tz_vk.status = status;
    return status;
}

static int32_t tz_vk_init(int32_t features) {
    struct tz_vk_state *state = &tz_vk;
    memset(&state->fn, 0, sizeof state->fn);
    state->library = tz_vk_find_library();
    if (state->library == NULL) return tz_vk_fail_init(TZ_VK_UNAVAILABLE, "the Vulkan library cannot be loaded", VK_SUCCESS);
    if (!tz_vk_load_global_functions()) {
        return tz_vk_fail_init(TZ_VK_UNAVAILABLE, "the library lacks vkGetInstanceProcAddr or a global function", VK_SUCCESS);
    }
    uint32_t instance_version = VK_MAKE_API_VERSION(0, 1, 0, 0);
    if (state->fn.vkEnumerateInstanceVersion != NULL) {
        VkResult result = state->fn.vkEnumerateInstanceVersion(&instance_version);
        if (result != VK_SUCCESS) return tz_vk_fail_init(TZ_VK_UNAVAILABLE, "vkEnumerateInstanceVersion", result);
    }
    if (instance_version < VK_MAKE_API_VERSION(0, 1, 1, 0)) {
        tz_vk_debug("the Vulkan loader reports version %u.%u", VK_API_VERSION_MAJOR(instance_version),
            VK_API_VERSION_MINOR(instance_version));
        return tz_vk_fail_init(TZ_VK_UNSUPPORTED, "the Vulkan loader implements Vulkan 1.0 only; 1.1 is required", VK_SUCCESS);
    }
    uint32_t api = instance_version < VK_MAKE_API_VERSION(0, 1, 3, 0) ? instance_version : VK_MAKE_API_VERSION(0, 1, 3, 0);
    api &= ~0xFFFU;

    const char *extensions[1];
    uint32_t extension_count = 0;
    VkFlags instance_flags = 0;
    uint32_t count = 0;
    VkResult result = state->fn.vkEnumerateInstanceExtensionProperties(NULL, &count, NULL);
    if (result != VK_SUCCESS) return tz_vk_fail_init(TZ_VK_UNAVAILABLE, "vkEnumerateInstanceExtensionProperties", result);
    if (count > 0) {
        VkExtensionProperties *available = (VkExtensionProperties *)TZ_VK_ALLOC(sizeof *available * count);
        if (available == NULL) return tz_vk_fail_init(TZ_VK_FAILED, "out of host memory", VK_SUCCESS);
        result = state->fn.vkEnumerateInstanceExtensionProperties(NULL, &count, available);
        if ((result == VK_SUCCESS || result == VK_INCOMPLETE)
            && tz_vk_has_extension(available, count, "VK_KHR_portability_enumeration")) {
            extensions[extension_count++] = "VK_KHR_portability_enumeration";
            instance_flags |= VK_INSTANCE_CREATE_ENUMERATE_PORTABILITY_BIT_KHR;
        }
        TZ_VK_FREE(available);
        if (result != VK_SUCCESS && result != VK_INCOMPLETE) {
            return tz_vk_fail_init(TZ_VK_UNAVAILABLE, "vkEnumerateInstanceExtensionProperties", result);
        }
    }
    VkApplicationInfo application;
    memset(&application, 0, sizeof application);
    application.sType = VK_STRUCTURE_TYPE_APPLICATION_INFO;
    application.pApplicationName = "tsuzuri";
    application.pEngineName = "tsuzuri";
    application.apiVersion = api;
    VkInstanceCreateInfo instance_info;
    memset(&instance_info, 0, sizeof instance_info);
    instance_info.sType = VK_STRUCTURE_TYPE_INSTANCE_CREATE_INFO;
    instance_info.flags = instance_flags;
    instance_info.pApplicationInfo = &application;
    instance_info.enabledExtensionCount = extension_count;
    instance_info.ppEnabledExtensionNames = extensions;
    result = state->fn.vkCreateInstance(&instance_info, NULL, &state->instance);
    if (result != VK_SUCCESS) {
        state->instance = NULL;
        return tz_vk_fail_init(TZ_VK_UNAVAILABLE, "vkCreateInstance", result);
    }
    if (!tz_vk_load_instance_functions()) {
        return tz_vk_fail_init(TZ_VK_UNAVAILABLE, "the library lacks an instance function", VK_SUCCESS);
    }

    uint32_t device_count = 0;
    result = state->fn.vkEnumeratePhysicalDevices(state->instance, &device_count, NULL);
    if (result != VK_SUCCESS) return tz_vk_fail_init(TZ_VK_UNAVAILABLE, "vkEnumeratePhysicalDevices", result);
    if (device_count == 0) return tz_vk_fail_init(TZ_VK_UNAVAILABLE, "no Vulkan physical device", VK_SUCCESS);
    if (device_count > TZ_VK_MAX_DEVICES) device_count = TZ_VK_MAX_DEVICES;
    VkPhysicalDevice devices[TZ_VK_MAX_DEVICES];
    result = state->fn.vkEnumeratePhysicalDevices(state->instance, &device_count, devices);
    if (result != VK_SUCCESS && result != VK_INCOMPLETE) {
        return tz_vk_fail_init(TZ_VK_UNAVAILABLE, "vkEnumeratePhysicalDevices", result);
    }
    /* The best device that has a compute queue, Vulkan 1.1, and every feature the first caller needs; when no
       device has all the features, the best usable device is kept and open reports the missing feature. */
    struct tz_vk_candidate chosen;
    int have_chosen = 0, chosen_satisfies = 0;
    for (uint32_t index = 0; index < device_count; index++) {
        struct tz_vk_candidate candidate;
        int32_t status = tz_vk_describe_candidate(devices[index], &candidate);
        if (status != TZ_VK_OK) {
            if (status == TZ_VK_FAILED) return tz_vk_fail_init(TZ_VK_FAILED, "out of host memory", VK_SUCCESS);
            continue;
        }
        tz_vk_debug("device %u: %s", index, candidate.caps.name);
        if (!candidate.has_queue || candidate.caps.api_version < VK_MAKE_API_VERSION(0, 1, 1, 0)) continue;
        int satisfies = tz_vk_satisfies(&candidate.caps, features);
        if (!have_chosen || (satisfies && !chosen_satisfies)
            || (satisfies == chosen_satisfies && candidate.rank > chosen.rank)) {
            chosen = candidate;
            have_chosen = 1;
            chosen_satisfies = satisfies;
        }
    }
    if (!have_chosen) {
        return tz_vk_fail_init(TZ_VK_UNAVAILABLE, "no Vulkan device with a compute queue and Vulkan 1.1", VK_SUCCESS);
    }
    state->physical = chosen.device;
    state->caps = chosen.caps;
    state->memory = chosen.memory;
    state->queue_family = chosen.queue_family;
    if (state->caps.max_workgroup_invocations < TZ_VK_GROUP_SIZE || state->caps.max_workgroup_size_x < TZ_VK_GROUP_SIZE) {
        return tz_vk_fail_init(TZ_VK_UNSUPPORTED, "the device cannot run workgroups of 256 invocations", VK_SUCCESS);
    }

    const char *device_extensions[2];
    uint32_t device_extension_count = 0;
    if (chosen.portability_subset) device_extensions[device_extension_count++] = "VK_KHR_portability_subset";
    if (chosen.float_controls_extension && state->caps.api_version < VK_MAKE_API_VERSION(0, 1, 2, 0)) {
        device_extensions[device_extension_count++] = "VK_KHR_shader_float_controls";
    }
    float priority = 1.0f;
    VkDeviceQueueCreateInfo queue_info;
    memset(&queue_info, 0, sizeof queue_info);
    queue_info.sType = VK_STRUCTURE_TYPE_DEVICE_QUEUE_CREATE_INFO;
    queue_info.queueFamilyIndex = state->queue_family;
    queue_info.queueCount = 1;
    queue_info.pQueuePriorities = &priority;
    VkPhysicalDeviceFeatures enabled;
    memset(&enabled, 0, sizeof enabled);
    enabled.shaderInt64 = state->caps.int64;
    VkDeviceCreateInfo device_info;
    memset(&device_info, 0, sizeof device_info);
    device_info.sType = VK_STRUCTURE_TYPE_DEVICE_CREATE_INFO;
    device_info.queueCreateInfoCount = 1;
    device_info.pQueueCreateInfos = &queue_info;
    device_info.enabledExtensionCount = device_extension_count;
    device_info.ppEnabledExtensionNames = device_extensions;
    device_info.pEnabledFeatures = &enabled;
    result = state->fn.vkCreateDevice(state->physical, &device_info, NULL, &state->device);
    if (result != VK_SUCCESS) {
        state->device = NULL;
        return tz_vk_fail_init(TZ_VK_UNAVAILABLE, "vkCreateDevice", result);
    }
    if (!tz_vk_load_device_functions()) {
        return tz_vk_fail_init(TZ_VK_UNAVAILABLE, "the library lacks a device function", VK_SUCCESS);
    }
    state->fn.vkGetDeviceQueue(state->device, state->queue_family, 0, &state->queue);

    VkCommandPoolCreateInfo pool_info;
    memset(&pool_info, 0, sizeof pool_info);
    pool_info.sType = VK_STRUCTURE_TYPE_COMMAND_POOL_CREATE_INFO;
    pool_info.flags = VK_COMMAND_POOL_CREATE_RESET_COMMAND_BUFFER_BIT;
    pool_info.queueFamilyIndex = state->queue_family;
    result = state->fn.vkCreateCommandPool(state->device, &pool_info, NULL, &state->pool);
    if (result != VK_SUCCESS) {
        state->pool = 0;
        return tz_vk_fail_init(TZ_VK_FAILED, "vkCreateCommandPool", result);
    }
    VkCommandBufferAllocateInfo command_info;
    memset(&command_info, 0, sizeof command_info);
    command_info.sType = VK_STRUCTURE_TYPE_COMMAND_BUFFER_ALLOCATE_INFO;
    command_info.commandPool = state->pool;
    command_info.level = VK_COMMAND_BUFFER_LEVEL_PRIMARY;
    command_info.commandBufferCount = 1;
    result = state->fn.vkAllocateCommandBuffers(state->device, &command_info, &state->commands);
    if (result != VK_SUCCESS) {
        state->commands = NULL;
        return tz_vk_fail_init(TZ_VK_FAILED, "vkAllocateCommandBuffers", result);
    }
    VkFenceCreateInfo fence_info;
    memset(&fence_info, 0, sizeof fence_info);
    fence_info.sType = VK_STRUCTURE_TYPE_FENCE_CREATE_INFO;
    result = state->fn.vkCreateFence(state->device, &fence_info, NULL, &state->fence);
    if (result != VK_SUCCESS) {
        state->fence = 0;
        return tz_vk_fail_init(TZ_VK_FAILED, "vkCreateFence", result);
    }
    VkDescriptorSetLayoutBinding bindings[2];
    memset(bindings, 0, sizeof bindings);
    for (uint32_t index = 0; index < 2; index++) {
        bindings[index].binding = index;
        bindings[index].descriptorType = VK_DESCRIPTOR_TYPE_STORAGE_BUFFER;
        bindings[index].descriptorCount = 1;
        bindings[index].stageFlags = VK_SHADER_STAGE_COMPUTE_BIT;
    }
    VkDescriptorSetLayoutCreateInfo layout_info;
    memset(&layout_info, 0, sizeof layout_info);
    layout_info.sType = VK_STRUCTURE_TYPE_DESCRIPTOR_SET_LAYOUT_CREATE_INFO;
    layout_info.bindingCount = 2;
    layout_info.pBindings = bindings;
    result = state->fn.vkCreateDescriptorSetLayout(state->device, &layout_info, NULL, &state->set_layout);
    if (result != VK_SUCCESS) {
        state->set_layout = 0;
        return tz_vk_fail_init(TZ_VK_FAILED, "vkCreateDescriptorSetLayout", result);
    }
    VkPushConstantRange push;
    push.stageFlags = VK_SHADER_STAGE_COMPUTE_BIT;
    push.offset = 0;
    push.size = 8;
    VkPipelineLayoutCreateInfo pipeline_layout_info;
    memset(&pipeline_layout_info, 0, sizeof pipeline_layout_info);
    pipeline_layout_info.sType = VK_STRUCTURE_TYPE_PIPELINE_LAYOUT_CREATE_INFO;
    pipeline_layout_info.setLayoutCount = 1;
    pipeline_layout_info.pSetLayouts = &state->set_layout;
    pipeline_layout_info.pushConstantRangeCount = 1;
    pipeline_layout_info.pPushConstantRanges = &push;
    result = state->fn.vkCreatePipelineLayout(state->device, &pipeline_layout_info, NULL, &state->pipeline_layout);
    if (result != VK_SUCCESS) {
        state->pipeline_layout = 0;
        return tz_vk_fail_init(TZ_VK_FAILED, "vkCreatePipelineLayout", result);
    }
    VkDescriptorPoolSize pool_size;
    pool_size.type = VK_DESCRIPTOR_TYPE_STORAGE_BUFFER;
    pool_size.descriptorCount = 2;
    VkDescriptorPoolCreateInfo descriptor_pool_info;
    memset(&descriptor_pool_info, 0, sizeof descriptor_pool_info);
    descriptor_pool_info.sType = VK_STRUCTURE_TYPE_DESCRIPTOR_POOL_CREATE_INFO;
    descriptor_pool_info.maxSets = 1;
    descriptor_pool_info.poolSizeCount = 1;
    descriptor_pool_info.pPoolSizes = &pool_size;
    result = state->fn.vkCreateDescriptorPool(state->device, &descriptor_pool_info, NULL, &state->descriptors);
    if (result != VK_SUCCESS) {
        state->descriptors = 0;
        return tz_vk_fail_init(TZ_VK_FAILED, "vkCreateDescriptorPool", result);
    }
    VkDescriptorSetAllocateInfo set_info;
    memset(&set_info, 0, sizeof set_info);
    set_info.sType = VK_STRUCTURE_TYPE_DESCRIPTOR_SET_ALLOCATE_INFO;
    set_info.descriptorPool = state->descriptors;
    set_info.descriptorSetCount = 1;
    set_info.pSetLayouts = &state->set_layout;
    result = state->fn.vkAllocateDescriptorSets(state->device, &set_info, &state->set);
    if (result != VK_SUCCESS) {
        state->set = 0;
        return tz_vk_fail_init(TZ_VK_FAILED, "vkAllocateDescriptorSets", result);
    }
    state->state = 1;
    state->status = TZ_VK_OK;
    if (!state->exit_registered) {
        state->exit_registered = 1;
        atexit(tz_vk_at_exit);
    }
    if (tz_vk_debug_enabled()) {
        char text[1024];
        tz_vk_format_caps(&state->caps, text, sizeof text);
        tz_vk_debug("%s", text);
    }
    return TZ_VK_OK;
}

/* Initializes once, then checks the features; the lock is held by the caller. */
static int32_t tz_vk_ensure(int32_t features) {
    if (tz_vk.state == 0) {
        int32_t status = tz_vk_init(features);
        if (status != TZ_VK_OK) return status;
    }
    if (tz_vk.state != 1) return tz_vk.status != TZ_VK_OK ? tz_vk.status : TZ_VK_UNAVAILABLE;
    if (tz_vk.poisoned) return TZ_VK_FAILED;
    if ((features & TZ_VK_FEATURE_INT64) != 0 && !tz_vk.caps.int64) {
        tz_vk_debug("the device does not support 64-bit integers in shaders (shaderInt64)");
        return TZ_VK_UNSUPPORTED;
    }
    if ((features & TZ_VK_FEATURE_STRICT_F32) != 0 && !tz_vk.caps.strict_f32) {
        tz_vk_debug("the device lacks the strict float32 controls (signed zero/inf/nan preserve, denorm preserve, "
                    "round to nearest even, independent 32-bit modes)");
        return TZ_VK_UNSUPPORTED;
    }
    return TZ_VK_OK;
}

static int32_t tz_vulkan_open(int32_t features) {
    TZ_VK_LOCK();
    int32_t status = tz_vk_ensure(features & TZ_VK_FEATURE_MASK);
    TZ_VK_UNLOCK();
    return status;
}

/* ---- running a kernel ---- */

struct tz_vk_buffer {
    VkBuffer buffer;
    VkDeviceMemory memory;
    void *mapped;
};

static int tz_vk_find_memory(uint32_t type_bits, VkFlags required, VkFlags avoided, uint32_t *index) {
    const struct tz_vk_state *state = &tz_vk;
    for (int pass = 0; pass < 2; pass++) {
        for (uint32_t candidate = 0; candidate < state->memory.memoryTypeCount && candidate < 32; candidate++) {
            VkFlags flags = state->memory.memoryTypes[candidate].propertyFlags;
            if ((type_bits & (1U << candidate)) == 0 || (flags & required) != required) continue;
            if (pass == 0 && (flags & avoided) != 0) continue;
            *index = candidate;
            return 1;
        }
    }
    return 0;
}

static void tz_vk_destroy_buffer(struct tz_vk_buffer *buffer) {
    struct tz_vk_state *state = &tz_vk;
    if (buffer->mapped != NULL) state->fn.vkUnmapMemory(state->device, buffer->memory);
    if (buffer->buffer != 0) state->fn.vkDestroyBuffer(state->device, buffer->buffer, NULL);
    if (buffer->memory != 0) state->fn.vkFreeMemory(state->device, buffer->memory, NULL);
    memset(buffer, 0, sizeof *buffer);
}

/* Creates a buffer with its own memory; `mapped` is non-NULL afterwards when the memory is host visible. */
static VkResult tz_vk_create_buffer(struct tz_vk_buffer *buffer, VkDeviceSize size, VkFlags usage, VkFlags required,
    VkFlags avoided, int map, int *no_memory_type) {
    struct tz_vk_state *state = &tz_vk;
    memset(buffer, 0, sizeof *buffer);
    VkBufferCreateInfo info;
    memset(&info, 0, sizeof info);
    info.sType = VK_STRUCTURE_TYPE_BUFFER_CREATE_INFO;
    info.size = size;
    info.usage = usage;
    info.sharingMode = VK_SHARING_MODE_EXCLUSIVE;
    VkResult result = state->fn.vkCreateBuffer(state->device, &info, NULL, &buffer->buffer);
    if (result != VK_SUCCESS) {
        buffer->buffer = 0;
        return result;
    }
    VkMemoryRequirements requirements;
    memset(&requirements, 0, sizeof requirements);
    state->fn.vkGetBufferMemoryRequirements(state->device, buffer->buffer, &requirements);
    uint32_t type = 0;
    if (!tz_vk_find_memory(requirements.memoryTypeBits, required, avoided, &type)) {
        *no_memory_type = 1;
        tz_vk_destroy_buffer(buffer);
        return VK_ERROR_OUT_OF_DEVICE_MEMORY;
    }
    VkMemoryAllocateInfo allocation;
    memset(&allocation, 0, sizeof allocation);
    allocation.sType = VK_STRUCTURE_TYPE_MEMORY_ALLOCATE_INFO;
    allocation.allocationSize = requirements.size;
    allocation.memoryTypeIndex = type;
    result = state->fn.vkAllocateMemory(state->device, &allocation, NULL, &buffer->memory);
    if (result != VK_SUCCESS) {
        buffer->memory = 0;
        tz_vk_destroy_buffer(buffer);
        return result;
    }
    result = state->fn.vkBindBufferMemory(state->device, buffer->buffer, buffer->memory, 0);
    if (result != VK_SUCCESS) {
        tz_vk_destroy_buffer(buffer);
        return result;
    }
    if (map) {
        result = state->fn.vkMapMemory(state->device, buffer->memory, 0, VK_WHOLE_SIZE, 0, &buffer->mapped);
        if (result != VK_SUCCESS) {
            buffer->mapped = NULL;
            tz_vk_destroy_buffer(buffer);
            return result;
        }
    }
    return VK_SUCCESS;
}

/* The pipeline of a SPIR-V module, found by content or created; the lock is held. */
static int32_t tz_vk_pipeline(const uint32_t *words, uint32_t length, int mode, VkPipeline *pipeline) {
    struct tz_vk_state *state = &tz_vk;
    struct tz_vk_program *program = NULL;
    for (uint32_t index = 0; index < state->program_count; index++) {
        struct tz_vk_program *candidate = &state->programs[index];
        if (candidate->length == length && memcmp(candidate->words, words, (size_t)length * 4) == 0) {
            program = candidate;
            break;
        }
    }
    if (program == NULL) {
        if (state->program_count == TZ_VK_PROGRAMS) {
            tz_vk_release_programs();
        }
        program = &state->programs[state->program_count];
        memset(program, 0, sizeof *program);
        program->words = (uint32_t *)TZ_VK_ALLOC((size_t)length * 4);
        if (program->words == NULL) return tz_vk_report(TZ_VK_FAILED, "out of host memory");
        memcpy(program->words, words, (size_t)length * 4);
        program->length = length;
        VkShaderModuleCreateInfo info;
        memset(&info, 0, sizeof info);
        info.sType = VK_STRUCTURE_TYPE_SHADER_MODULE_CREATE_INFO;
        info.codeSize = (size_t)length * 4;
        info.pCode = program->words;
        VkResult result = state->fn.vkCreateShaderModule(state->device, &info, NULL, &program->module);
        if (result != VK_SUCCESS) {
            TZ_VK_FREE(program->words);
            memset(program, 0, sizeof *program);
            return tz_vk_report(TZ_VK_FAILED, "vkCreateShaderModule failed (VkResult %d)", (int)result);
        }
        state->program_count++;
    }
    if (program->pipelines[mode] == 0) {
        VkComputePipelineCreateInfo info;
        memset(&info, 0, sizeof info);
        info.sType = VK_STRUCTURE_TYPE_COMPUTE_PIPELINE_CREATE_INFO;
        info.stage.sType = VK_STRUCTURE_TYPE_PIPELINE_SHADER_STAGE_CREATE_INFO;
        info.stage.stage = VK_SHADER_STAGE_COMPUTE_BIT;
        info.stage.module = program->module;
        info.stage.pName = mode == 0 ? "map_main" : "init_main";
        info.layout = state->pipeline_layout;
        VkResult result = state->fn.vkCreateComputePipelines(state->device, 0, 1, &info, NULL, &program->pipelines[mode]);
        if (result != VK_SUCCESS) {
            program->pipelines[mode] = 0;
            return tz_vk_report(TZ_VK_FAILED, "vkCreateComputePipelines failed (VkResult %d)", (int)result);
        }
    }
    *pipeline = program->pipelines[mode];
    return TZ_VK_OK;
}

/* The capabilities of a module that the device must have been verified for; the walk stops at the memory model. */
static int32_t tz_vk_check_module(const uint32_t *words, uint32_t length) {
    const struct tz_vk_caps *caps = &tz_vk.caps;
    if (length < 5 || words[0] != 0x07230203U) return tz_vk_report(TZ_VK_UNSUPPORTED, "the kernel is not a SPIR-V module");
    uint32_t minor = (words[1] >> 8) & 0xFF;
    uint32_t major = (words[1] >> 16) & 0xFF;
    uint32_t needed = minor <= 3 ? VK_MAKE_API_VERSION(0, 1, 1, 0)
        : minor <= 5 ? VK_MAKE_API_VERSION(0, 1, 2, 0) : VK_MAKE_API_VERSION(0, 1, 3, 0);
    if (major != 1 || minor > 6 || caps->api_version < needed) {
        return tz_vk_report(TZ_VK_UNSUPPORTED, "the device does not support SPIR-V %u.%u", major, minor);
    }
    uint32_t position = 5;
    while (position < length) {
        uint32_t opcode = words[position] & 0xFFFF, size = words[position] >> 16;
        if (size == 0 || position + size > length) return tz_vk_report(TZ_VK_UNSUPPORTED, "the SPIR-V module is truncated");
        if (opcode == 14) break; /* OpMemoryModel: the capabilities precede it */
        if (opcode == 17 && size >= 2) { /* OpCapability */
            uint32_t capability = words[position + 1];
            if (capability == 11 && !caps->int64) {
                return tz_vk_report(TZ_VK_UNSUPPORTED, "the kernel uses Int64 and the device lacks shaderInt64");
            }
            if ((capability == 4464 || capability == 4466 || capability == 4467) && !caps->strict_f32) {
                return tz_vk_report(TZ_VK_UNSUPPORTED, "the kernel needs the strict float32 controls and the device lacks them");
            }
            if (capability != 1 && capability != 11 && capability != 4464 && capability != 4466 && capability != 4467) {
                return tz_vk_report(TZ_VK_UNSUPPORTED, "the kernel uses SPIR-V capability %u", capability);
            }
        }
        position += size;
    }
    return TZ_VK_OK;
}

static int tz_vk_lane_size(int32_t kind) {
    switch (kind) {
    case 1: case 2: return 4;
    case 4: return 8;
    default: return 0; /* 3 = f16 has no Vulkan kernel */
    }
}

static void tz_vk_barrier(VkCommandBuffer commands, VkFlags source_stage, VkFlags destination_stage, VkFlags source_access,
    VkFlags destination_access) {
    VkMemoryBarrier barrier;
    memset(&barrier, 0, sizeof barrier);
    barrier.sType = VK_STRUCTURE_TYPE_MEMORY_BARRIER;
    barrier.srcAccessMask = source_access;
    barrier.dstAccessMask = destination_access;
    tz_vk.fn.vkCmdPipelineBarrier(commands, source_stage, destination_stage, 0, 1, &barrier, 0, NULL, 0, NULL);
}

/* mode 0 = map over `count` input lanes, 1 = init (no input); the buffers belong to the caller. */
static int32_t tz_vulkan_run(int32_t mode, int32_t flags, int32_t lanes, const void *wgsl, int32_t wgsl_length,
    const void *spirv, int32_t spirv_length, const void *input, int64_t count, void *output) {
    (void)flags;
    (void)wgsl;
    (void)wgsl_length;
    if (mode != 0 && mode != 1) return tz_vk_report(TZ_VK_UNSUPPORTED, "unknown run mode %d", (int)mode);
    if (spirv == NULL || spirv_length < 20 || (spirv_length & 3) != 0) {
        return tz_vk_report(TZ_VK_UNSUPPORTED, "the kernel has no SPIR-V module for Vulkan");
    }
    int input_size = tz_vk_lane_size(lanes & 0xFF), output_size = tz_vk_lane_size((lanes >> 8) & 0xFF);
    if ((mode == 0 && input_size == 0) || output_size == 0) {
        return tz_vk_report(TZ_VK_UNSUPPORTED, "the lane type of the kernel is not available on Vulkan");
    }
    if (count < 0 || count > TZ_VK_MAX_LANES) return tz_vk_report(TZ_VK_LIMIT, "%lld lanes exceed 2147483647", (long long)count);
    if (count == 0) return TZ_VK_OK;
    if (output == NULL || (mode == 0 && input == NULL)) return tz_vk_report(TZ_VK_FAILED, "a buffer is missing");

    TZ_VK_LOCK();
    int32_t status = tz_vk_ensure(0);
    if (status != TZ_VK_OK) {
        TZ_VK_UNLOCK();
        return status == TZ_VK_FAILED && tz_vk.poisoned ? tz_vk_report(status, "the device was lost earlier") : status;
    }
    struct tz_vk_state *state = &tz_vk;
    const struct tz_vk_caps *caps = &state->caps;
    const uint32_t *words = (const uint32_t *)spirv;
    uint32_t length_words = (uint32_t)spirv_length / 4;
    uint32_t *aligned = NULL;
    if (((uintptr_t)spirv & 3U) != 0) {
        aligned = (uint32_t *)TZ_VK_ALLOC((size_t)spirv_length);
        if (aligned == NULL) {
            TZ_VK_UNLOCK();
            return tz_vk_report(TZ_VK_FAILED, "out of host memory");
        }
        memcpy(aligned, spirv, (size_t)spirv_length);
        words = aligned;
    }
    VkDeviceSize input_bytes = mode == 0 ? (VkDeviceSize)count * (VkDeviceSize)input_size : 0;
    VkDeviceSize output_bytes = (VkDeviceSize)count * (VkDeviceSize)output_size;
    struct tz_vk_buffer device_in, device_out, host_in, host_out;
    memset(&device_in, 0, sizeof device_in);
    memset(&device_out, 0, sizeof device_out);
    memset(&host_in, 0, sizeof host_in);
    memset(&host_out, 0, sizeof host_out);
    int recording = 0, submitted = 0, no_memory_type = 0;
    VkPipeline pipeline = 0;
    VkResult result = VK_SUCCESS;
    const char *what = "";

    status = tz_vk_check_module(words, length_words);
    if (status != TZ_VK_OK) goto done;
    if ((VkDeviceSize)input_bytes > caps->max_storage_range || output_bytes > caps->max_storage_range
        || input_bytes > caps->max_allocation || output_bytes > caps->max_allocation) {
        status = tz_vk_report(TZ_VK_LIMIT, "%lld lanes need %llu bytes; the device allows %u bytes per storage buffer and %llu per allocation",
            (long long)count, (unsigned long long)(input_bytes > output_bytes ? input_bytes : output_bytes),
            caps->max_storage_range, (unsigned long long)caps->max_allocation);
        goto done;
    }
    status = tz_vk_pipeline(words, length_words, mode, &pipeline);
    if (status != TZ_VK_OK) goto done;

    {
        const VkFlags host_flags = VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT | VK_MEMORY_PROPERTY_HOST_COHERENT_BIT;
        const VkFlags storage = VK_BUFFER_USAGE_STORAGE_BUFFER_BIT;
        if (caps->direct_transfer) {
            const VkFlags unified = host_flags | VK_MEMORY_PROPERTY_DEVICE_LOCAL_BIT;
            if (mode == 0) {
                what = "creating the input buffer";
                result = tz_vk_create_buffer(&device_in, input_bytes, storage, unified, 0, 1, &no_memory_type);
                if (result != VK_SUCCESS) goto failed;
                memcpy(device_in.mapped, input, (size_t)input_bytes);
            }
            what = "creating the output buffer";
            result = tz_vk_create_buffer(&device_out, output_bytes, storage, unified, 0, 1, &no_memory_type);
            if (result != VK_SUCCESS) goto failed;
        } else {
            if (mode == 0) {
                what = "creating the input buffers";
                result = tz_vk_create_buffer(&device_in, input_bytes, storage | VK_BUFFER_USAGE_TRANSFER_DST_BIT,
                    VK_MEMORY_PROPERTY_DEVICE_LOCAL_BIT, VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT, 0, &no_memory_type);
                if (result != VK_SUCCESS) goto failed;
                result = tz_vk_create_buffer(&host_in, input_bytes, VK_BUFFER_USAGE_TRANSFER_SRC_BIT, host_flags,
                    VK_MEMORY_PROPERTY_DEVICE_LOCAL_BIT, 1, &no_memory_type);
                if (result != VK_SUCCESS) goto failed;
                memcpy(host_in.mapped, input, (size_t)input_bytes);
            }
            what = "creating the output buffers";
            result = tz_vk_create_buffer(&device_out, output_bytes, storage | VK_BUFFER_USAGE_TRANSFER_SRC_BIT,
                VK_MEMORY_PROPERTY_DEVICE_LOCAL_BIT, VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT, 0, &no_memory_type);
            if (result != VK_SUCCESS) goto failed;
            result = tz_vk_create_buffer(&host_out, output_bytes, VK_BUFFER_USAGE_TRANSFER_DST_BIT, host_flags,
                VK_MEMORY_PROPERTY_DEVICE_LOCAL_BIT, 1, &no_memory_type);
            if (result != VK_SUCCESS) goto failed;
        }
    }

    {
        VkDescriptorBufferInfo infos[2];
        infos[0].buffer = mode == 0 ? device_in.buffer : device_out.buffer;
        infos[0].offset = 0;
        infos[0].range = mode == 0 ? input_bytes : output_bytes;
        infos[1].buffer = device_out.buffer;
        infos[1].offset = 0;
        infos[1].range = output_bytes;
        VkWriteDescriptorSet writes[2];
        memset(writes, 0, sizeof writes);
        for (uint32_t index = 0; index < 2; index++) {
            writes[index].sType = VK_STRUCTURE_TYPE_WRITE_DESCRIPTOR_SET;
            writes[index].dstSet = state->set;
            writes[index].dstBinding = index;
            writes[index].descriptorCount = 1;
            writes[index].descriptorType = VK_DESCRIPTOR_TYPE_STORAGE_BUFFER;
            writes[index].pBufferInfo = &infos[index];
        }
        state->fn.vkUpdateDescriptorSets(state->device, 2, writes, 0, NULL);
    }

    {
        VkCommandBuffer commands = state->commands;
        VkCommandBufferBeginInfo begin;
        memset(&begin, 0, sizeof begin);
        begin.sType = VK_STRUCTURE_TYPE_COMMAND_BUFFER_BEGIN_INFO;
        begin.flags = VK_COMMAND_BUFFER_USAGE_ONE_TIME_SUBMIT_BIT;
        what = "vkBeginCommandBuffer";
        recording = 1;
        result = state->fn.vkBeginCommandBuffer(commands, &begin);
        if (result != VK_SUCCESS) goto failed;
        if (!caps->direct_transfer && mode == 0) {
            VkBufferCopy region;
            region.srcOffset = 0;
            region.dstOffset = 0;
            region.size = input_bytes;
            state->fn.vkCmdCopyBuffer(commands, host_in.buffer, device_in.buffer, 1, &region);
            tz_vk_barrier(commands, VK_PIPELINE_STAGE_TRANSFER_BIT, VK_PIPELINE_STAGE_COMPUTE_SHADER_BIT,
                VK_ACCESS_TRANSFER_WRITE_BIT, VK_ACCESS_SHADER_READ_BIT);
        }
        state->fn.vkCmdBindPipeline(commands, VK_PIPELINE_BIND_POINT_COMPUTE, pipeline);
        state->fn.vkCmdBindDescriptorSets(commands, VK_PIPELINE_BIND_POINT_COMPUTE, state->pipeline_layout, 0, 1, &state->set, 0, NULL);
        uint64_t groups_total = ((uint64_t)count + TZ_VK_GROUP_SIZE - 1) / TZ_VK_GROUP_SIZE;
        uint64_t groups_per_dispatch = caps->max_group_count_x != 0 ? caps->max_group_count_x : 65535;
        for (uint64_t first = 0; first < groups_total; first += groups_per_dispatch) {
            uint64_t groups = groups_total - first < groups_per_dispatch ? groups_total - first : groups_per_dispatch;
            uint32_t constants[2] = {(uint32_t)count, (uint32_t)(first * TZ_VK_GROUP_SIZE)};
            state->fn.vkCmdPushConstants(commands, state->pipeline_layout, VK_SHADER_STAGE_COMPUTE_BIT, 0, 8, constants);
            state->fn.vkCmdDispatch(commands, (uint32_t)groups, 1, 1);
        }
        if (caps->direct_transfer) {
            tz_vk_barrier(commands, VK_PIPELINE_STAGE_COMPUTE_SHADER_BIT, VK_PIPELINE_STAGE_HOST_BIT,
                VK_ACCESS_SHADER_WRITE_BIT, VK_ACCESS_HOST_READ_BIT);
        } else {
            tz_vk_barrier(commands, VK_PIPELINE_STAGE_COMPUTE_SHADER_BIT, VK_PIPELINE_STAGE_TRANSFER_BIT,
                VK_ACCESS_SHADER_WRITE_BIT, VK_ACCESS_TRANSFER_READ_BIT);
            VkBufferCopy region;
            region.srcOffset = 0;
            region.dstOffset = 0;
            region.size = output_bytes;
            state->fn.vkCmdCopyBuffer(commands, device_out.buffer, host_out.buffer, 1, &region);
            tz_vk_barrier(commands, VK_PIPELINE_STAGE_TRANSFER_BIT, VK_PIPELINE_STAGE_HOST_BIT,
                VK_ACCESS_TRANSFER_WRITE_BIT, VK_ACCESS_HOST_READ_BIT);
        }
        what = "vkEndCommandBuffer";
        result = state->fn.vkEndCommandBuffer(commands);
        if (result != VK_SUCCESS) goto failed;
        recording = 0;
        what = "vkResetFences";
        result = state->fn.vkResetFences(state->device, 1, &state->fence);
        if (result != VK_SUCCESS) goto failed;
        VkSubmitInfo submit;
        memset(&submit, 0, sizeof submit);
        submit.sType = VK_STRUCTURE_TYPE_SUBMIT_INFO;
        submit.commandBufferCount = 1;
        submit.pCommandBuffers = &commands;
        what = "vkQueueSubmit";
        result = state->fn.vkQueueSubmit(state->queue, 1, &submit, state->fence);
        if (result != VK_SUCCESS) goto failed;
        submitted = 1;
        what = "vkWaitForFences";
        result = state->fn.vkWaitForFences(state->device, 1, &state->fence, 1, UINT64_MAX);
        if (result != VK_SUCCESS) goto failed;
        submitted = 0;
    }
    memcpy(output, caps->direct_transfer ? device_out.mapped : host_out.mapped, (size_t)output_bytes);
    status = TZ_VK_OK;
    goto done;

failed:
    if (result == VK_ERROR_DEVICE_LOST) state->poisoned = 1;
    status = no_memory_type ? tz_vk_report(TZ_VK_FAILED, "%s failed: the device has no suitable memory type", what)
                            : tz_vk_report(TZ_VK_FAILED, "%s failed (VkResult %d)", what, (int)result);
    if (recording) state->fn.vkResetCommandBuffer(state->commands, 0);
done:
    if (submitted) state->fn.vkDeviceWaitIdle(state->device);
    tz_vk_destroy_buffer(&host_out);
    tz_vk_destroy_buffer(&host_in);
    tz_vk_destroy_buffer(&device_out);
    tz_vk_destroy_buffer(&device_in);
    TZ_VK_FREE(aligned);
    TZ_VK_UNLOCK();
    return status;
}

/* The capability line of the selected device, for diagnostics and tests; empty when the backend is not open. */
static void tz_vulkan_describe(char *text, size_t size) {
    if (size == 0) return;
    text[0] = '\0';
    TZ_VK_LOCK();
    if (tz_vk.state == 1) tz_vk_format_caps(&tz_vk.caps, text, size);
    TZ_VK_UNLOCK();
}

#ifdef TZ_VK_TEST_HOOKS
/* Test hooks: the harness includes this file and reaches the state directly. */
static void tz_vk_test_reset(void) {
    TZ_VK_LOCK();
    if (tz_vk.state == 1) tz_vk_teardown();
    tz_vk.state = 0;
    tz_vk.status = 0;
    tz_vk.poisoned = 0;
    TZ_VK_UNLOCK();
}
#endif
