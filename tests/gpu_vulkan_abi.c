/* Checks the hand-declared Vulkan ABI subset of src/runtime/gpu-vulkan.c against the real headers (F09 Phase 3).
 *
 * The same table is compiled twice and the two outputs are compared (tests/gpu_vulkan.mjs):
 *   clang -DTZ_ABI_REAL -I<vulkan headers> tests/gpu_vulkan_abi.c   prints the layout the headers declare
 *   clang tests/gpu_vulkan_abi.c                                      prints the layout of the runtime's declarations
 * Each line is `<type> size=<bytes> align=<bytes> <field>@<offset> ...` or `<NAME>=<value>`. A change of a field type,
 * order, or constant in the runtime that is not what Vulkan declares makes the outputs differ. */
#include <stddef.h>
#include <stdio.h>

#ifdef TZ_ABI_REAL
#include <vulkan/vulkan.h>
#else
#include "../src/runtime/gpu-vulkan.c"
#endif

#define BEGIN(type) printf("%s size=%zu align=%zu", #type, sizeof(type), _Alignof(type));
#define F(type, field) printf(" %s@%zu", #field, offsetof(type, field));
#define END printf("\n");
#define K(name) printf("%s=%lld\n", #name, (long long)(name));

int main(void) {
    BEGIN(VkApplicationInfo) F(VkApplicationInfo, sType) F(VkApplicationInfo, pNext) F(VkApplicationInfo, pApplicationName)
        F(VkApplicationInfo, applicationVersion) F(VkApplicationInfo, pEngineName) F(VkApplicationInfo, engineVersion)
        F(VkApplicationInfo, apiVersion) END
    BEGIN(VkInstanceCreateInfo) F(VkInstanceCreateInfo, sType) F(VkInstanceCreateInfo, pNext) F(VkInstanceCreateInfo, flags)
        F(VkInstanceCreateInfo, pApplicationInfo) F(VkInstanceCreateInfo, enabledLayerCount)
        F(VkInstanceCreateInfo, ppEnabledLayerNames) F(VkInstanceCreateInfo, enabledExtensionCount)
        F(VkInstanceCreateInfo, ppEnabledExtensionNames) END
    BEGIN(VkExtensionProperties) F(VkExtensionProperties, extensionName) F(VkExtensionProperties, specVersion) END
    BEGIN(VkPhysicalDeviceFeatures) F(VkPhysicalDeviceFeatures, robustBufferAccess) F(VkPhysicalDeviceFeatures, shaderFloat64)
        F(VkPhysicalDeviceFeatures, shaderInt64) F(VkPhysicalDeviceFeatures, shaderInt16)
        F(VkPhysicalDeviceFeatures, shaderResourceResidency) F(VkPhysicalDeviceFeatures, inheritedQueries) END
    BEGIN(VkPhysicalDeviceLimits) F(VkPhysicalDeviceLimits, maxImageDimension1D) F(VkPhysicalDeviceLimits, maxUniformBufferRange)
        F(VkPhysicalDeviceLimits, maxStorageBufferRange) F(VkPhysicalDeviceLimits, maxPushConstantsSize)
        F(VkPhysicalDeviceLimits, maxMemoryAllocationCount) F(VkPhysicalDeviceLimits, bufferImageGranularity)
        F(VkPhysicalDeviceLimits, sparseAddressSpaceSize) F(VkPhysicalDeviceLimits, maxBoundDescriptorSets)
        F(VkPhysicalDeviceLimits, maxPerStageDescriptorStorageBuffers) F(VkPhysicalDeviceLimits, maxDescriptorSetStorageBuffers)
        F(VkPhysicalDeviceLimits, maxVertexInputAttributes) F(VkPhysicalDeviceLimits, maxFragmentCombinedOutputResources)
        F(VkPhysicalDeviceLimits, maxComputeSharedMemorySize) F(VkPhysicalDeviceLimits, maxComputeWorkGroupCount)
        F(VkPhysicalDeviceLimits, maxComputeWorkGroupInvocations) F(VkPhysicalDeviceLimits, maxComputeWorkGroupSize)
        F(VkPhysicalDeviceLimits, subPixelPrecisionBits) F(VkPhysicalDeviceLimits, maxSamplerLodBias)
        F(VkPhysicalDeviceLimits, maxViewportDimensions) F(VkPhysicalDeviceLimits, viewportBoundsRange)
        F(VkPhysicalDeviceLimits, minMemoryMapAlignment) F(VkPhysicalDeviceLimits, minTexelBufferOffsetAlignment)
        F(VkPhysicalDeviceLimits, minStorageBufferOffsetAlignment) F(VkPhysicalDeviceLimits, minTexelOffset)
        F(VkPhysicalDeviceLimits, maxFramebufferWidth) F(VkPhysicalDeviceLimits, framebufferColorSampleCounts)
        F(VkPhysicalDeviceLimits, maxColorAttachments) F(VkPhysicalDeviceLimits, storageImageSampleCounts)
        F(VkPhysicalDeviceLimits, maxSampleMaskWords) F(VkPhysicalDeviceLimits, timestampPeriod)
        F(VkPhysicalDeviceLimits, discreteQueuePriorities) F(VkPhysicalDeviceLimits, pointSizeRange)
        F(VkPhysicalDeviceLimits, strictLines) F(VkPhysicalDeviceLimits, optimalBufferCopyOffsetAlignment)
        F(VkPhysicalDeviceLimits, nonCoherentAtomSize) END
    BEGIN(VkPhysicalDeviceSparseProperties) F(VkPhysicalDeviceSparseProperties, residencyStandard2DBlockShape)
        F(VkPhysicalDeviceSparseProperties, residencyNonResidentStrict) END
    BEGIN(VkPhysicalDeviceProperties) F(VkPhysicalDeviceProperties, apiVersion) F(VkPhysicalDeviceProperties, driverVersion)
        F(VkPhysicalDeviceProperties, vendorID) F(VkPhysicalDeviceProperties, deviceID) F(VkPhysicalDeviceProperties, deviceType)
        F(VkPhysicalDeviceProperties, deviceName) F(VkPhysicalDeviceProperties, pipelineCacheUUID)
        F(VkPhysicalDeviceProperties, limits) F(VkPhysicalDeviceProperties, sparseProperties) END
    BEGIN(VkPhysicalDeviceProperties2) F(VkPhysicalDeviceProperties2, sType) F(VkPhysicalDeviceProperties2, pNext)
        F(VkPhysicalDeviceProperties2, properties) END
    BEGIN(VkPhysicalDeviceMaintenance3Properties) F(VkPhysicalDeviceMaintenance3Properties, sType)
        F(VkPhysicalDeviceMaintenance3Properties, pNext) F(VkPhysicalDeviceMaintenance3Properties, maxPerSetDescriptors)
        F(VkPhysicalDeviceMaintenance3Properties, maxMemoryAllocationSize) END
    BEGIN(VkPhysicalDeviceFloatControlsProperties) F(VkPhysicalDeviceFloatControlsProperties, sType)
        F(VkPhysicalDeviceFloatControlsProperties, pNext) F(VkPhysicalDeviceFloatControlsProperties, denormBehaviorIndependence)
        F(VkPhysicalDeviceFloatControlsProperties, roundingModeIndependence)
        F(VkPhysicalDeviceFloatControlsProperties, shaderSignedZeroInfNanPreserveFloat16)
        F(VkPhysicalDeviceFloatControlsProperties, shaderSignedZeroInfNanPreserveFloat32)
        F(VkPhysicalDeviceFloatControlsProperties, shaderSignedZeroInfNanPreserveFloat64)
        F(VkPhysicalDeviceFloatControlsProperties, shaderDenormPreserveFloat16)
        F(VkPhysicalDeviceFloatControlsProperties, shaderDenormPreserveFloat32)
        F(VkPhysicalDeviceFloatControlsProperties, shaderDenormPreserveFloat64)
        F(VkPhysicalDeviceFloatControlsProperties, shaderDenormFlushToZeroFloat16)
        F(VkPhysicalDeviceFloatControlsProperties, shaderDenormFlushToZeroFloat32)
        F(VkPhysicalDeviceFloatControlsProperties, shaderDenormFlushToZeroFloat64)
        F(VkPhysicalDeviceFloatControlsProperties, shaderRoundingModeRTEFloat16)
        F(VkPhysicalDeviceFloatControlsProperties, shaderRoundingModeRTEFloat32)
        F(VkPhysicalDeviceFloatControlsProperties, shaderRoundingModeRTEFloat64)
        F(VkPhysicalDeviceFloatControlsProperties, shaderRoundingModeRTZFloat16)
        F(VkPhysicalDeviceFloatControlsProperties, shaderRoundingModeRTZFloat32)
        F(VkPhysicalDeviceFloatControlsProperties, shaderRoundingModeRTZFloat64) END
    BEGIN(VkMemoryType) F(VkMemoryType, propertyFlags) F(VkMemoryType, heapIndex) END
    BEGIN(VkMemoryHeap) F(VkMemoryHeap, size) F(VkMemoryHeap, flags) END
    BEGIN(VkPhysicalDeviceMemoryProperties) F(VkPhysicalDeviceMemoryProperties, memoryTypeCount)
        F(VkPhysicalDeviceMemoryProperties, memoryTypes) F(VkPhysicalDeviceMemoryProperties, memoryHeapCount)
        F(VkPhysicalDeviceMemoryProperties, memoryHeaps) END
    BEGIN(VkExtent3D) F(VkExtent3D, width) F(VkExtent3D, height) F(VkExtent3D, depth) END
    BEGIN(VkQueueFamilyProperties) F(VkQueueFamilyProperties, queueFlags) F(VkQueueFamilyProperties, queueCount)
        F(VkQueueFamilyProperties, timestampValidBits) F(VkQueueFamilyProperties, minImageTransferGranularity) END
    BEGIN(VkDeviceQueueCreateInfo) F(VkDeviceQueueCreateInfo, sType) F(VkDeviceQueueCreateInfo, pNext)
        F(VkDeviceQueueCreateInfo, flags) F(VkDeviceQueueCreateInfo, queueFamilyIndex) F(VkDeviceQueueCreateInfo, queueCount)
        F(VkDeviceQueueCreateInfo, pQueuePriorities) END
    BEGIN(VkDeviceCreateInfo) F(VkDeviceCreateInfo, sType) F(VkDeviceCreateInfo, pNext) F(VkDeviceCreateInfo, flags)
        F(VkDeviceCreateInfo, queueCreateInfoCount) F(VkDeviceCreateInfo, pQueueCreateInfos)
        F(VkDeviceCreateInfo, enabledLayerCount) F(VkDeviceCreateInfo, ppEnabledLayerNames)
        F(VkDeviceCreateInfo, enabledExtensionCount) F(VkDeviceCreateInfo, ppEnabledExtensionNames)
        F(VkDeviceCreateInfo, pEnabledFeatures) END
    BEGIN(VkBufferCreateInfo) F(VkBufferCreateInfo, sType) F(VkBufferCreateInfo, pNext) F(VkBufferCreateInfo, flags)
        F(VkBufferCreateInfo, size) F(VkBufferCreateInfo, usage) F(VkBufferCreateInfo, sharingMode)
        F(VkBufferCreateInfo, queueFamilyIndexCount) F(VkBufferCreateInfo, pQueueFamilyIndices) END
    BEGIN(VkMemoryRequirements) F(VkMemoryRequirements, size) F(VkMemoryRequirements, alignment)
        F(VkMemoryRequirements, memoryTypeBits) END
    BEGIN(VkMemoryAllocateInfo) F(VkMemoryAllocateInfo, sType) F(VkMemoryAllocateInfo, pNext)
        F(VkMemoryAllocateInfo, allocationSize) F(VkMemoryAllocateInfo, memoryTypeIndex) END
    BEGIN(VkDescriptorSetLayoutBinding) F(VkDescriptorSetLayoutBinding, binding) F(VkDescriptorSetLayoutBinding, descriptorType)
        F(VkDescriptorSetLayoutBinding, descriptorCount) F(VkDescriptorSetLayoutBinding, stageFlags)
        F(VkDescriptorSetLayoutBinding, pImmutableSamplers) END
    BEGIN(VkDescriptorSetLayoutCreateInfo) F(VkDescriptorSetLayoutCreateInfo, sType) F(VkDescriptorSetLayoutCreateInfo, pNext)
        F(VkDescriptorSetLayoutCreateInfo, flags) F(VkDescriptorSetLayoutCreateInfo, bindingCount)
        F(VkDescriptorSetLayoutCreateInfo, pBindings) END
    BEGIN(VkPushConstantRange) F(VkPushConstantRange, stageFlags) F(VkPushConstantRange, offset) F(VkPushConstantRange, size) END
    BEGIN(VkPipelineLayoutCreateInfo) F(VkPipelineLayoutCreateInfo, sType) F(VkPipelineLayoutCreateInfo, pNext)
        F(VkPipelineLayoutCreateInfo, flags) F(VkPipelineLayoutCreateInfo, setLayoutCount)
        F(VkPipelineLayoutCreateInfo, pSetLayouts) F(VkPipelineLayoutCreateInfo, pushConstantRangeCount)
        F(VkPipelineLayoutCreateInfo, pPushConstantRanges) END
    BEGIN(VkShaderModuleCreateInfo) F(VkShaderModuleCreateInfo, sType) F(VkShaderModuleCreateInfo, pNext)
        F(VkShaderModuleCreateInfo, flags) F(VkShaderModuleCreateInfo, codeSize) F(VkShaderModuleCreateInfo, pCode) END
    BEGIN(VkPipelineShaderStageCreateInfo) F(VkPipelineShaderStageCreateInfo, sType) F(VkPipelineShaderStageCreateInfo, pNext)
        F(VkPipelineShaderStageCreateInfo, flags) F(VkPipelineShaderStageCreateInfo, stage)
        F(VkPipelineShaderStageCreateInfo, module) F(VkPipelineShaderStageCreateInfo, pName)
        F(VkPipelineShaderStageCreateInfo, pSpecializationInfo) END
    BEGIN(VkComputePipelineCreateInfo) F(VkComputePipelineCreateInfo, sType) F(VkComputePipelineCreateInfo, pNext)
        F(VkComputePipelineCreateInfo, flags) F(VkComputePipelineCreateInfo, stage) F(VkComputePipelineCreateInfo, layout)
        F(VkComputePipelineCreateInfo, basePipelineHandle) F(VkComputePipelineCreateInfo, basePipelineIndex) END
    BEGIN(VkDescriptorPoolSize) F(VkDescriptorPoolSize, type) F(VkDescriptorPoolSize, descriptorCount) END
    BEGIN(VkDescriptorPoolCreateInfo) F(VkDescriptorPoolCreateInfo, sType) F(VkDescriptorPoolCreateInfo, pNext)
        F(VkDescriptorPoolCreateInfo, flags) F(VkDescriptorPoolCreateInfo, maxSets) F(VkDescriptorPoolCreateInfo, poolSizeCount)
        F(VkDescriptorPoolCreateInfo, pPoolSizes) END
    BEGIN(VkDescriptorSetAllocateInfo) F(VkDescriptorSetAllocateInfo, sType) F(VkDescriptorSetAllocateInfo, pNext)
        F(VkDescriptorSetAllocateInfo, descriptorPool) F(VkDescriptorSetAllocateInfo, descriptorSetCount)
        F(VkDescriptorSetAllocateInfo, pSetLayouts) END
    BEGIN(VkDescriptorBufferInfo) F(VkDescriptorBufferInfo, buffer) F(VkDescriptorBufferInfo, offset) F(VkDescriptorBufferInfo, range) END
    BEGIN(VkWriteDescriptorSet) F(VkWriteDescriptorSet, sType) F(VkWriteDescriptorSet, pNext) F(VkWriteDescriptorSet, dstSet)
        F(VkWriteDescriptorSet, dstBinding) F(VkWriteDescriptorSet, dstArrayElement) F(VkWriteDescriptorSet, descriptorCount)
        F(VkWriteDescriptorSet, descriptorType) F(VkWriteDescriptorSet, pImageInfo) F(VkWriteDescriptorSet, pBufferInfo)
        F(VkWriteDescriptorSet, pTexelBufferView) END
    BEGIN(VkCommandPoolCreateInfo) F(VkCommandPoolCreateInfo, sType) F(VkCommandPoolCreateInfo, pNext)
        F(VkCommandPoolCreateInfo, flags) F(VkCommandPoolCreateInfo, queueFamilyIndex) END
    BEGIN(VkCommandBufferAllocateInfo) F(VkCommandBufferAllocateInfo, sType) F(VkCommandBufferAllocateInfo, pNext)
        F(VkCommandBufferAllocateInfo, commandPool) F(VkCommandBufferAllocateInfo, level)
        F(VkCommandBufferAllocateInfo, commandBufferCount) END
    BEGIN(VkCommandBufferBeginInfo) F(VkCommandBufferBeginInfo, sType) F(VkCommandBufferBeginInfo, pNext)
        F(VkCommandBufferBeginInfo, flags) F(VkCommandBufferBeginInfo, pInheritanceInfo) END
    BEGIN(VkBufferCopy) F(VkBufferCopy, srcOffset) F(VkBufferCopy, dstOffset) F(VkBufferCopy, size) END
    BEGIN(VkMemoryBarrier) F(VkMemoryBarrier, sType) F(VkMemoryBarrier, pNext) F(VkMemoryBarrier, srcAccessMask)
        F(VkMemoryBarrier, dstAccessMask) END
    BEGIN(VkSubmitInfo) F(VkSubmitInfo, sType) F(VkSubmitInfo, pNext) F(VkSubmitInfo, waitSemaphoreCount)
        F(VkSubmitInfo, pWaitSemaphores) F(VkSubmitInfo, pWaitDstStageMask) F(VkSubmitInfo, commandBufferCount)
        F(VkSubmitInfo, pCommandBuffers) F(VkSubmitInfo, signalSemaphoreCount) F(VkSubmitInfo, pSignalSemaphores) END
    BEGIN(VkFenceCreateInfo) F(VkFenceCreateInfo, sType) F(VkFenceCreateInfo, pNext) F(VkFenceCreateInfo, flags) END

    printf("handle sizes: %zu %zu %zu %zu %zu\n", sizeof(VkInstance), sizeof(VkBuffer), sizeof(VkDeviceMemory),
        sizeof(VkPipeline), sizeof(VkFence));
    K(VK_SUCCESS) K(VK_INCOMPLETE) K(VK_ERROR_OUT_OF_HOST_MEMORY) K(VK_ERROR_OUT_OF_DEVICE_MEMORY)
    K(VK_ERROR_INITIALIZATION_FAILED) K(VK_ERROR_DEVICE_LOST) K(VK_ERROR_INCOMPATIBLE_DRIVER)
    K(VK_STRUCTURE_TYPE_APPLICATION_INFO) K(VK_STRUCTURE_TYPE_INSTANCE_CREATE_INFO)
    K(VK_STRUCTURE_TYPE_DEVICE_QUEUE_CREATE_INFO) K(VK_STRUCTURE_TYPE_DEVICE_CREATE_INFO) K(VK_STRUCTURE_TYPE_SUBMIT_INFO)
    K(VK_STRUCTURE_TYPE_MEMORY_ALLOCATE_INFO) K(VK_STRUCTURE_TYPE_FENCE_CREATE_INFO) K(VK_STRUCTURE_TYPE_BUFFER_CREATE_INFO)
    K(VK_STRUCTURE_TYPE_SHADER_MODULE_CREATE_INFO) K(VK_STRUCTURE_TYPE_PIPELINE_SHADER_STAGE_CREATE_INFO)
    K(VK_STRUCTURE_TYPE_COMPUTE_PIPELINE_CREATE_INFO) K(VK_STRUCTURE_TYPE_PIPELINE_LAYOUT_CREATE_INFO)
    K(VK_STRUCTURE_TYPE_DESCRIPTOR_SET_LAYOUT_CREATE_INFO) K(VK_STRUCTURE_TYPE_DESCRIPTOR_POOL_CREATE_INFO)
    K(VK_STRUCTURE_TYPE_DESCRIPTOR_SET_ALLOCATE_INFO) K(VK_STRUCTURE_TYPE_WRITE_DESCRIPTOR_SET)
    K(VK_STRUCTURE_TYPE_COMMAND_POOL_CREATE_INFO) K(VK_STRUCTURE_TYPE_COMMAND_BUFFER_ALLOCATE_INFO)
    K(VK_STRUCTURE_TYPE_COMMAND_BUFFER_BEGIN_INFO) K(VK_STRUCTURE_TYPE_MEMORY_BARRIER)
    K(VK_STRUCTURE_TYPE_PHYSICAL_DEVICE_FEATURES_2) K(VK_STRUCTURE_TYPE_PHYSICAL_DEVICE_PROPERTIES_2)
    K(VK_STRUCTURE_TYPE_PHYSICAL_DEVICE_MAINTENANCE_3_PROPERTIES)
    K(VK_STRUCTURE_TYPE_PHYSICAL_DEVICE_FLOAT_CONTROLS_PROPERTIES)
    K(VK_INSTANCE_CREATE_ENUMERATE_PORTABILITY_BIT_KHR) K(VK_QUEUE_COMPUTE_BIT) K(VK_PHYSICAL_DEVICE_TYPE_OTHER)
    K(VK_PHYSICAL_DEVICE_TYPE_INTEGRATED_GPU) K(VK_PHYSICAL_DEVICE_TYPE_DISCRETE_GPU) K(VK_PHYSICAL_DEVICE_TYPE_VIRTUAL_GPU)
    K(VK_PHYSICAL_DEVICE_TYPE_CPU) K(VK_MEMORY_PROPERTY_DEVICE_LOCAL_BIT) K(VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT)
    K(VK_MEMORY_PROPERTY_HOST_COHERENT_BIT) K(VK_BUFFER_USAGE_TRANSFER_SRC_BIT) K(VK_BUFFER_USAGE_TRANSFER_DST_BIT)
    K(VK_BUFFER_USAGE_STORAGE_BUFFER_BIT) K(VK_SHARING_MODE_EXCLUSIVE) K(VK_DESCRIPTOR_TYPE_STORAGE_BUFFER)
    K(VK_SHADER_STAGE_COMPUTE_BIT) K(VK_COMMAND_POOL_CREATE_RESET_COMMAND_BUFFER_BIT) K(VK_COMMAND_BUFFER_LEVEL_PRIMARY)
    K(VK_COMMAND_BUFFER_USAGE_ONE_TIME_SUBMIT_BIT) K(VK_PIPELINE_BIND_POINT_COMPUTE) K(VK_PIPELINE_STAGE_TRANSFER_BIT)
    K(VK_PIPELINE_STAGE_COMPUTE_SHADER_BIT) K(VK_PIPELINE_STAGE_HOST_BIT) K(VK_ACCESS_SHADER_READ_BIT)
    K(VK_ACCESS_SHADER_WRITE_BIT) K(VK_ACCESS_TRANSFER_READ_BIT) K(VK_ACCESS_TRANSFER_WRITE_BIT) K(VK_ACCESS_HOST_READ_BIT)
    K(VK_SHADER_FLOAT_CONTROLS_INDEPENDENCE_32_BIT_ONLY) K(VK_SHADER_FLOAT_CONTROLS_INDEPENDENCE_ALL)
    K(VK_SHADER_FLOAT_CONTROLS_INDEPENDENCE_NONE) K(VK_WHOLE_SIZE)
    K(VK_MAKE_API_VERSION(0, 1, 3, 0)) K(VK_API_VERSION_MAJOR(VK_MAKE_API_VERSION(0, 1, 3, 7)))
    K(VK_API_VERSION_MINOR(VK_MAKE_API_VERSION(0, 1, 3, 7))) K(VK_API_VERSION_PATCH(VK_MAKE_API_VERSION(0, 1, 3, 7)))
    return 0;
}
