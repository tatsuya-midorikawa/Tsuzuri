/* A synthetic Vulkan implementation for the tests of src/runtime/gpu-vulkan.c (F09 Phase 3).
 *
 * The runtime loads it through TSUZURI_VULKAN_LIBRARY, so the paths that a real machine does not reach run here: no
 * device, a device without shaderInt64 or without the float controls, a Vulkan 1.0 loader, a missing function, small
 * limits, and a failure of the Nth fallible call. It keeps every object it creates, reports the ones that are not
 * destroyed, checks the order and validity of the calls the runtime makes (handles alive, recording state, bindings),
 * and "executes" a submission: copies are performed, and a dispatch computes out[i] = in[i] * 3 + 7 in the lane size of
 * the buffers (out[i] = i * 3 + 7 for the init entry point) while it checks that every lane is written exactly once.
 *
 * Configuration (tz_vk_mock_configure): comma separated key=value, see parse_configuration. */
#define _GNU_SOURCE
#define _DARWIN_C_SOURCE
#include <pthread.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "../src/runtime/gpu-vulkan.c"

#define MOCK_LIVE 0x4C495645U
#define MOCK_DEAD 0x44454144U
#define MOCK_MAX_COMMANDS 16384

enum object_type {
    O_INSTANCE, O_PHYSICAL, O_DEVICE, O_QUEUE, O_BUFFER, O_MEMORY, O_MODULE, O_PIPELINE, O_PIPELINE_LAYOUT,
    O_SET_LAYOUT, O_POOL, O_SET, O_COMMAND_POOL, O_COMMAND_BUFFER, O_FENCE, O_TYPE_COUNT
};

static const char *const object_names[O_TYPE_COUNT] = {
    "VkInstance", "VkPhysicalDevice", "VkDevice", "VkQueue", "VkBuffer", "VkDeviceMemory", "VkShaderModule",
    "VkPipeline", "VkPipelineLayout", "VkDescriptorSetLayout", "VkDescriptorPool", "VkDescriptorSet", "VkCommandPool",
    "VkCommandBuffer", "VkFence"};

enum command_kind { C_COPY, C_PUSH, C_DISPATCH, C_BIND_PIPELINE, C_BIND_SET, C_BARRIER };

struct command {
    enum command_kind kind;
    struct object *a, *b;
    uint64_t size;
    uint32_t words[2];
};

struct object {
    uint32_t magic;
    enum object_type type;
    struct object *parent;
    uint64_t size;
    uint32_t usage;
    uint32_t flags;
    struct object *memory;
    unsigned char *data;
    int mapped;
    int type_index;
    uint32_t *words;
    uint32_t word_count;
    char entries[4][32];
    int entry_count;
    struct object *module;
    int entry;
    struct object *bound_buffers[2];
    uint64_t bound_ranges[2];
    int written[2];
    int state; /* command buffer: 0 initial, 1 recording, 2 executable; fence: 1 signaled */
    int has_pipeline_layout;
    struct command *commands;
    int command_count;
    uint32_t *hits;
    uint64_t hit_count;
};

struct configuration {
    int devices, device_type, int64, szinp, denorm, rte, denorm_independence, rounding_independence, float_controls_extension;
    int queue, portability, unified, no_version, features_chained;
    uint32_t api, instance_api, max_range, max_groups, invocations, group_size;
    uint64_t max_allocation;
    int fail, fail_code;
    int probe_fault; /* the lane of the conformance probe whose result is corrupted, plus one; 0 for none */
    char missing[64];
};

static pthread_mutex_t mock_lock = PTHREAD_MUTEX_INITIALIZER;
static struct configuration config;
static int live_objects[O_TYPE_COUNT];
static int misuse_count, injected_faults, fallible_calls, total_submits, probe_dispatches;
static char report[4096];
static int instance_flags_seen;

static void note(const char *format, ...) {
    va_list arguments;
    va_start(arguments, format);
    size_t used = strlen(report);
    if (used < sizeof report - 1) vsnprintf(report + used, sizeof report - used, format, arguments);
    va_end(arguments);
}

static void misuse(const char *what) {
    misuse_count++;
    note("misuse: %s\n", what);
}

static uint32_t parse_version(const char *text) {
    unsigned major = 1, minor = 0;
    sscanf(text, "%u.%u", &major, &minor);
    return VK_MAKE_API_VERSION(0, major, minor, 0);
}

static int parse_independence(const char *text) {
    if (strcmp(text, "all") == 0) return VK_SHADER_FLOAT_CONTROLS_INDEPENDENCE_ALL;
    if (strcmp(text, "32") == 0) return VK_SHADER_FLOAT_CONTROLS_INDEPENDENCE_32_BIT_ONLY;
    return VK_SHADER_FLOAT_CONTROLS_INDEPENDENCE_NONE;
}

static int parse_device_type(const char *text) {
    if (strcmp(text, "discrete") == 0) return VK_PHYSICAL_DEVICE_TYPE_DISCRETE_GPU;
    if (strcmp(text, "cpu") == 0) return VK_PHYSICAL_DEVICE_TYPE_CPU;
    if (strcmp(text, "virtual") == 0) return VK_PHYSICAL_DEVICE_TYPE_VIRTUAL_GPU;
    if (strcmp(text, "other") == 0) return VK_PHYSICAL_DEVICE_TYPE_OTHER;
    return VK_PHYSICAL_DEVICE_TYPE_INTEGRATED_GPU;
}

/* Keys: devices, type=integrated|discrete|cpu|virtual|other, api=1.x (device), instance=1.x, int64, szinp, denorm, rte,
   denorm_independence=none|32|all, rounding_independence, strict=1 (all float controls with independence all), fc_ext,
   queue, portability, unified, max_range, max_groups, invocations, group_size, max_allocation, no_version, missing=<fn>,
   fail=N (the Nth fallible call fails), code=<VkResult> (default -1). Returns 0 for an unknown key. */
int tz_vk_mock_configure(const char *text) {
    pthread_mutex_lock(&mock_lock);
    memset(&config, 0, sizeof config);
    config.devices = 1;
    config.device_type = VK_PHYSICAL_DEVICE_TYPE_INTEGRATED_GPU;
    instance_flags_seen = 0;
    config.api = VK_MAKE_API_VERSION(0, 1, 3, 0);
    config.instance_api = VK_MAKE_API_VERSION(0, 1, 3, 0);
    config.int64 = 1;
    config.queue = 1;
    config.unified = 1;
    config.max_range = 0xFFFFFFFFU;
    config.max_groups = 65535;
    config.invocations = 1024;
    config.group_size = 1024;
    config.max_allocation = (uint64_t)1 << 32;
    config.denorm_independence = config.rounding_independence = VK_SHADER_FLOAT_CONTROLS_INDEPENDENCE_NONE;
    config.fail_code = VK_ERROR_OUT_OF_HOST_MEMORY;
    memset(live_objects, 0, sizeof live_objects);
    misuse_count = injected_faults = fallible_calls = total_submits = probe_dispatches = 0;
    report[0] = '\0';
    int ok = 1;
    char buffer[512];
    snprintf(buffer, sizeof buffer, "%s", text != NULL ? text : "");
    int api_set = 0;
    for (char *token = strtok(buffer, ","); token != NULL; token = strtok(NULL, ",")) {
        char *equals = strchr(token, '=');
        if (equals == NULL) {
            ok = 0;
            continue;
        }
        *equals = '\0';
        const char *key = token, *value = equals + 1;
        if (strcmp(key, "devices") == 0) config.devices = atoi(value);
        else if (strcmp(key, "type") == 0) config.device_type = parse_device_type(value);
        else if (strcmp(key, "api") == 0) { config.api = parse_version(value); api_set = 1; }
        else if (strcmp(key, "instance") == 0) config.instance_api = parse_version(value);
        else if (strcmp(key, "int64") == 0) config.int64 = atoi(value);
        else if (strcmp(key, "szinp") == 0) config.szinp = atoi(value);
        else if (strcmp(key, "denorm") == 0) config.denorm = atoi(value);
        else if (strcmp(key, "rte") == 0) config.rte = atoi(value);
        else if (strcmp(key, "denorm_independence") == 0) config.denorm_independence = parse_independence(value);
        else if (strcmp(key, "rounding_independence") == 0) config.rounding_independence = parse_independence(value);
        else if (strcmp(key, "strict") == 0 && atoi(value)) {
            config.szinp = config.denorm = config.rte = 1;
            config.denorm_independence = config.rounding_independence = VK_SHADER_FLOAT_CONTROLS_INDEPENDENCE_ALL;
        }
        else if (strcmp(key, "fc_ext") == 0) config.float_controls_extension = atoi(value);
        else if (strcmp(key, "queue") == 0) config.queue = atoi(value);
        else if (strcmp(key, "portability") == 0) config.portability = atoi(value);
        else if (strcmp(key, "unified") == 0) config.unified = atoi(value);
        else if (strcmp(key, "max_range") == 0) config.max_range = (uint32_t)strtoul(value, NULL, 10);
        else if (strcmp(key, "max_groups") == 0) config.max_groups = (uint32_t)strtoul(value, NULL, 10);
        else if (strcmp(key, "invocations") == 0) config.invocations = (uint32_t)strtoul(value, NULL, 10);
        else if (strcmp(key, "group_size") == 0) config.group_size = (uint32_t)strtoul(value, NULL, 10);
        else if (strcmp(key, "max_allocation") == 0) config.max_allocation = strtoull(value, NULL, 10);
        else if (strcmp(key, "no_version") == 0) config.no_version = atoi(value);
        else if (strcmp(key, "missing") == 0) snprintf(config.missing, sizeof config.missing, "%s", value);
        else if (strcmp(key, "fail") == 0) config.fail = atoi(value);
        else if (strcmp(key, "probe") == 0) config.probe_fault = atoi(value) + 1;
        else if (strcmp(key, "code") == 0) config.fail_code = atoi(value);
        else ok = 0;
    }
    if (!api_set) config.api = config.instance_api;
    pthread_mutex_unlock(&mock_lock);
    return ok;
}

int tz_vk_mock_injected(void) { return injected_faults; }
int tz_vk_mock_calls(void) { return fallible_calls; }
int tz_vk_mock_submits(void) { return total_submits; }
int tz_vk_mock_probe_dispatches(void) { return probe_dispatches; }

/* A program that is not the harness (a compiled Tsuzuri program) configures the mock through the environment;
   TZ_VK_MOCK_SELF configures a copy that the runtime found by itself, which the harness does not know about. */
__attribute__((constructor)) static void configure_from_environment(void) {
    const char *text = getenv("TZ_VK_MOCK");
    if (text == NULL) text = getenv("TZ_VK_MOCK_SELF");
    if (text != NULL) tz_vk_mock_configure(text);
}

#ifdef TZ_VK_MOCK_MARKER
/* A planted copy of the mock: loading it leaves a trace in the named file, so that a test can tell that a library of the
   working directory (or any other the runtime must not pick) was loaded and its initializers ran. */
__attribute__((constructor)) static void leave_marker(void) {
    FILE *file = fopen(TZ_VK_MOCK_MARKER, "a");
    if (file != NULL) {
        fputs("loaded\n", file);
        fclose(file);
    }
}
#endif

int tz_vk_mock_leaked(void) {
    int total = misuse_count;
    for (int type = 0; type < O_TYPE_COUNT; type++) {
        if (type == O_PHYSICAL || type == O_QUEUE) continue;
        total += live_objects[type];
    }
    return total;
}

const char *tz_vk_mock_leak_report(void) {
    static char text[8192];
    text[0] = '\0';
    size_t used = 0;
    for (int type = 0; type < O_TYPE_COUNT; type++) {
        if (type == O_PHYSICAL || type == O_QUEUE || live_objects[type] == 0) continue;
        used += (size_t)snprintf(text + used, sizeof text - used, "  %d live %s\n", live_objects[type], object_names[type]);
    }
    snprintf(text + used, sizeof text - used, "%s", report);
    return text;
}

/* ---- objects ---- */

static struct object **registry;
static size_t registry_count, registry_capacity;
static struct object *graveyard;

static struct object *create(enum object_type type, struct object *parent) {
    struct object *object = (struct object *)calloc(1, sizeof *object);
    object->magic = MOCK_LIVE;
    object->type = type;
    object->parent = parent;
    live_objects[type]++;
    if (registry_count == registry_capacity) {
        registry_capacity = registry_capacity == 0 ? 256 : registry_capacity * 2;
        registry = (struct object **)realloc(registry, registry_capacity * sizeof *registry);
    }
    registry[registry_count++] = object;
    return object;
}

static void destroy(struct object *object) {
    /* Destroying a pool releases the descriptor sets or command buffers allocated from it. */
    if (object->type == O_POOL || object->type == O_COMMAND_POOL) {
        for (size_t index = 0; index < registry_count;) {
            struct object *child = registry[index];
            if (child->parent == object && (child->type == O_SET || child->type == O_COMMAND_BUFFER)) {
                destroy(child);
                index = 0;
            } else {
                index++;
            }
        }
    }
    for (size_t index = 0; index < registry_count; index++) {
        if (registry[index] == object) {
            registry[index] = registry[--registry_count];
            break;
        }
    }
    live_objects[object->type]--;
    object->magic = MOCK_DEAD;
    free(object->data);
    free(object->words);
    free(object->commands);
    free(object->hits);
    object->data = NULL;
    object->words = NULL;
    object->commands = NULL;
    object->hits = NULL;
    /* A destroyed object stays allocated so that a later use is reported instead of being undefined behavior. */
    object->parent = graveyard;
    graveyard = object;
}

static struct object *check(const void *handle, enum object_type type, const char *where) {
    struct object *object = (struct object *)handle;
    if (object == NULL) {
        char text[128];
        snprintf(text, sizeof text, "%s: null %s", where, object_names[type]);
        misuse(text);
        return NULL;
    }
    if (object->magic != MOCK_LIVE || object->type != type) {
        char text[128];
        snprintf(text, sizeof text, "%s: %s is not a live object of that type", where, object_names[type]);
        misuse(text);
        return NULL;
    }
    return object;
}

/* The Nth fallible call fails; returns the error to return or VK_SUCCESS. */
static VkResult fault(const char *name) {
    (void)name;
    fallible_calls++;
    if (config.fail != 0 && fallible_calls == config.fail) {
        injected_faults++;
        return config.fail_code;
    }
    return VK_SUCCESS;
}

#define ENTER(name) \
    pthread_mutex_lock(&mock_lock); \
    VkResult injected = fault(name); \
    if (injected != VK_SUCCESS) { \
        pthread_mutex_unlock(&mock_lock); \
        return injected; \
    }
#define LEAVE() pthread_mutex_unlock(&mock_lock)

/* ---- the instance ---- */

VkResult TZ_VKAPI vkEnumerateInstanceVersion(uint32_t *pApiVersion) {
    ENTER("vkEnumerateInstanceVersion");
    *pApiVersion = config.instance_api;
    LEAVE();
    return VK_SUCCESS;
}

VkResult TZ_VKAPI vkEnumerateInstanceExtensionProperties(const char *pLayerName, uint32_t *pPropertyCount, VkExtensionProperties *pProperties) {
    (void)pLayerName;
    ENTER("vkEnumerateInstanceExtensionProperties");
    uint32_t count = config.portability ? 1 : 0;
    if (pProperties == NULL) {
        *pPropertyCount = count;
    } else {
        if (*pPropertyCount > count) *pPropertyCount = count;
        if (count > 0 && *pPropertyCount > 0) {
            memset(&pProperties[0], 0, sizeof pProperties[0]);
            snprintf(pProperties[0].extensionName, sizeof pProperties[0].extensionName, "VK_KHR_portability_enumeration");
            pProperties[0].specVersion = 1;
        }
    }
    LEAVE();
    return VK_SUCCESS;
}

VkResult TZ_VKAPI vkCreateInstance(const VkInstanceCreateInfo *pCreateInfo, const VkAllocationCallbacks *pAllocator, VkInstance *pInstance) {
    ENTER("vkCreateInstance");
    if (pAllocator != NULL) misuse("vkCreateInstance: allocation callbacks");
    instance_flags_seen = (int)pCreateInfo->flags;
    if (config.portability && (pCreateInfo->flags & VK_INSTANCE_CREATE_ENUMERATE_PORTABILITY_BIT_KHR) == 0) {
        /* A portability driver stays hidden without the flag; the runtime must set it. */
    }
    if (pCreateInfo->pApplicationInfo == NULL || pCreateInfo->pApplicationInfo->apiVersion > config.instance_api) {
        misuse("vkCreateInstance: apiVersion above what the loader reported");
    }
    *pInstance = (VkInstance)create(O_INSTANCE, NULL);
    LEAVE();
    return VK_SUCCESS;
}

void TZ_VKAPI vkDestroyInstance(VkInstance instance, const VkAllocationCallbacks *pAllocator) {
    (void)pAllocator;
    pthread_mutex_lock(&mock_lock);
    struct object *object = check(instance, O_INSTANCE, "vkDestroyInstance");
    if (object != NULL) {
        if (live_objects[O_DEVICE] != 0) misuse("vkDestroyInstance: a device is still alive");
        destroy(object);
    }
    pthread_mutex_unlock(&mock_lock);
}

static struct object *physical_devices[16];

VkResult TZ_VKAPI vkEnumeratePhysicalDevices(VkInstance instance, uint32_t *pPhysicalDeviceCount, VkPhysicalDevice *pPhysicalDevices) {
    ENTER("vkEnumeratePhysicalDevices");
    struct object *owner = check(instance, O_INSTANCE, "vkEnumeratePhysicalDevices");
    uint32_t count = (uint32_t)config.devices;
    if (config.portability && (instance_flags_seen & VK_INSTANCE_CREATE_ENUMERATE_PORTABILITY_BIT_KHR) == 0) count = 0;
    if (owner == NULL) {
        LEAVE();
        return VK_ERROR_INITIALIZATION_FAILED;
    }
    if (pPhysicalDevices == NULL) {
        *pPhysicalDeviceCount = count;
    } else {
        if (*pPhysicalDeviceCount > count) *pPhysicalDeviceCount = count;
        for (uint32_t index = 0; index < *pPhysicalDeviceCount && index < 16; index++) {
            if (physical_devices[index] == NULL || physical_devices[index]->magic != MOCK_LIVE) {
                physical_devices[index] = create(O_PHYSICAL, owner);
                physical_devices[index]->type_index = (int)index;
            }
            pPhysicalDevices[index] = (VkPhysicalDevice)physical_devices[index];
        }
    }
    LEAVE();
    return VK_SUCCESS;
}

void TZ_VKAPI vkGetPhysicalDeviceFeatures(VkPhysicalDevice physicalDevice, VkPhysicalDeviceFeatures *pFeatures) {
    pthread_mutex_lock(&mock_lock);
    check(physicalDevice, O_PHYSICAL, "vkGetPhysicalDeviceFeatures");
    memset(pFeatures, 0, sizeof *pFeatures);
    pFeatures->shaderInt64 = (VkBool32)config.int64;
    pthread_mutex_unlock(&mock_lock);
}

void TZ_VKAPI vkGetPhysicalDeviceProperties2(VkPhysicalDevice physicalDevice, VkPhysicalDeviceProperties2 *pProperties) {
    pthread_mutex_lock(&mock_lock);
    check(physicalDevice, O_PHYSICAL, "vkGetPhysicalDeviceProperties2");
    VkPhysicalDeviceProperties *p = &pProperties->properties;
    memset(p, 0, sizeof *p);
    p->apiVersion = config.api;
    p->deviceType = config.device_type;
    snprintf(p->deviceName, sizeof p->deviceName, "Mock Vulkan device");
    p->limits.maxStorageBufferRange = config.max_range;
    p->limits.maxComputeWorkGroupCount[0] = config.max_groups;
    p->limits.maxComputeWorkGroupInvocations = config.invocations;
    p->limits.maxComputeWorkGroupSize[0] = config.group_size;
    p->limits.maxPushConstantsSize = 128;
    int controls_allowed = config.api >= VK_MAKE_API_VERSION(0, 1, 2, 0) || config.float_controls_extension;
    for (VkStructureType *chain = (VkStructureType *)pProperties->pNext; chain != NULL;) {
        struct header { VkStructureType type; void *next; } *header = (struct header *)chain;
        if (header->type == VK_STRUCTURE_TYPE_PHYSICAL_DEVICE_MAINTENANCE_3_PROPERTIES) {
            VkPhysicalDeviceMaintenance3Properties *m = (VkPhysicalDeviceMaintenance3Properties *)header;
            m->maxMemoryAllocationSize = config.max_allocation;
        } else if (header->type == VK_STRUCTURE_TYPE_PHYSICAL_DEVICE_FLOAT_CONTROLS_PROPERTIES) {
            VkPhysicalDeviceFloatControlsProperties *c = (VkPhysicalDeviceFloatControlsProperties *)header;
            if (!controls_allowed) misuse("the float controls were chained for a device that does not implement them");
            c->denormBehaviorIndependence = config.denorm_independence;
            c->roundingModeIndependence = config.rounding_independence;
            c->shaderSignedZeroInfNanPreserveFloat32 = (VkBool32)config.szinp;
            c->shaderDenormPreserveFloat32 = (VkBool32)config.denorm;
            c->shaderRoundingModeRTEFloat32 = (VkBool32)config.rte;
        } else {
            misuse("an unexpected structure was chained to the properties");
        }
        chain = (VkStructureType *)header->next;
    }
    pthread_mutex_unlock(&mock_lock);
}

void TZ_VKAPI vkGetPhysicalDeviceQueueFamilyProperties(VkPhysicalDevice physicalDevice, uint32_t *pQueueFamilyPropertyCount, VkQueueFamilyProperties *pQueueFamilyProperties) {
    pthread_mutex_lock(&mock_lock);
    check(physicalDevice, O_PHYSICAL, "vkGetPhysicalDeviceQueueFamilyProperties");
    /* family 0: transfer only; family 1: compute (when configured) */
    uint32_t count = config.queue ? 2 : 1;
    if (pQueueFamilyProperties == NULL) {
        *pQueueFamilyPropertyCount = count;
    } else {
        if (*pQueueFamilyPropertyCount > count) *pQueueFamilyPropertyCount = count;
        for (uint32_t index = 0; index < *pQueueFamilyPropertyCount; index++) {
            memset(&pQueueFamilyProperties[index], 0, sizeof pQueueFamilyProperties[index]);
            pQueueFamilyProperties[index].queueFlags = index == 0 ? 4 : (VK_QUEUE_COMPUTE_BIT | 1);
            pQueueFamilyProperties[index].queueCount = 1;
        }
    }
    pthread_mutex_unlock(&mock_lock);
}

void TZ_VKAPI vkGetPhysicalDeviceMemoryProperties(VkPhysicalDevice physicalDevice, VkPhysicalDeviceMemoryProperties *pMemoryProperties) {
    pthread_mutex_lock(&mock_lock);
    check(physicalDevice, O_PHYSICAL, "vkGetPhysicalDeviceMemoryProperties");
    memset(pMemoryProperties, 0, sizeof *pMemoryProperties);
    const VkFlags host = VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT | VK_MEMORY_PROPERTY_HOST_COHERENT_BIT;
    const VkFlags local = VK_MEMORY_PROPERTY_DEVICE_LOCAL_BIT;
    pMemoryProperties->memoryHeapCount = 2;
    pMemoryProperties->memoryHeaps[0].size = (uint64_t)1 << 32;
    pMemoryProperties->memoryHeaps[1].size = (uint64_t)1 << 32;
    pMemoryProperties->memoryTypes[0].propertyFlags = local;
    pMemoryProperties->memoryTypes[1].propertyFlags = host;
    pMemoryProperties->memoryTypes[1].heapIndex = 1;
    pMemoryProperties->memoryTypeCount = 2;
    if (config.unified) {
        pMemoryProperties->memoryTypes[2].propertyFlags = local | host;
        pMemoryProperties->memoryTypeCount = 3;
    }
    pthread_mutex_unlock(&mock_lock);
}

VkResult TZ_VKAPI vkEnumerateDeviceExtensionProperties(VkPhysicalDevice physicalDevice, const char *pLayerName, uint32_t *pPropertyCount, VkExtensionProperties *pProperties) {
    (void)pLayerName;
    ENTER("vkEnumerateDeviceExtensionProperties");
    check(physicalDevice, O_PHYSICAL, "vkEnumerateDeviceExtensionProperties");
    const char *names[2];
    uint32_t count = 0;
    if (config.portability) names[count++] = "VK_KHR_portability_subset";
    if (config.float_controls_extension) names[count++] = "VK_KHR_shader_float_controls";
    if (pProperties == NULL) {
        *pPropertyCount = count;
    } else {
        if (*pPropertyCount > count) *pPropertyCount = count;
        for (uint32_t index = 0; index < *pPropertyCount; index++) {
            memset(&pProperties[index], 0, sizeof pProperties[index]);
            snprintf(pProperties[index].extensionName, sizeof pProperties[index].extensionName, "%s", names[index]);
            pProperties[index].specVersion = 1;
        }
    }
    LEAVE();
    return VK_SUCCESS;
}

/* ---- the device ---- */

static int mock_extension_enabled(const VkDeviceCreateInfo *info, const char *name) {
    for (uint32_t index = 0; index < info->enabledExtensionCount; index++) {
        if (strcmp(info->ppEnabledExtensionNames[index], name) == 0) return 1;
    }
    return 0;
}

VkResult TZ_VKAPI vkCreateDevice(VkPhysicalDevice physicalDevice, const VkDeviceCreateInfo *pCreateInfo, const VkAllocationCallbacks *pAllocator, VkDevice *pDevice) {
    (void)pAllocator;
    ENTER("vkCreateDevice");
    check(physicalDevice, O_PHYSICAL, "vkCreateDevice");
    if (config.portability && !mock_extension_enabled(pCreateInfo, "VK_KHR_portability_subset")) {
        misuse("vkCreateDevice: VK_KHR_portability_subset must be enabled on a portability device");
    }
    if (config.api < VK_MAKE_API_VERSION(0, 1, 2, 0) && config.float_controls_extension
        && !mock_extension_enabled(pCreateInfo, "VK_KHR_shader_float_controls")) {
        misuse("vkCreateDevice: VK_KHR_shader_float_controls must be enabled below Vulkan 1.2");
    }
    if (pCreateInfo->pEnabledFeatures != NULL && pCreateInfo->pEnabledFeatures->shaderInt64 && !config.int64) {
        misuse("vkCreateDevice: shaderInt64 enabled on a device without it");
    }
    if (pCreateInfo->queueCreateInfoCount != 1 || pCreateInfo->pQueueCreateInfos[0].queueFamilyIndex != 1) {
        misuse("vkCreateDevice: the compute queue family was not requested");
    }
    *pDevice = (VkDevice)create(O_DEVICE, NULL);
    LEAVE();
    return VK_SUCCESS;
}

void TZ_VKAPI vkDestroyDevice(VkDevice device, const VkAllocationCallbacks *pAllocator) {
    (void)pAllocator;
    pthread_mutex_lock(&mock_lock);
    struct object *object = check(device, O_DEVICE, "vkDestroyDevice");
    if (object != NULL) {
        for (int type = O_BUFFER; type < O_TYPE_COUNT; type++) {
            if (live_objects[type] != 0) {
                char text[128];
                snprintf(text, sizeof text, "vkDestroyDevice: %d %s still alive", live_objects[type], object_names[type]);
                misuse(text);
            }
        }
        destroy(object);
    }
    pthread_mutex_unlock(&mock_lock);
}

void TZ_VKAPI vkGetDeviceQueue(VkDevice device, uint32_t queueFamilyIndex, uint32_t queueIndex, VkQueue *pQueue) {
    pthread_mutex_lock(&mock_lock);
    struct object *owner = check(device, O_DEVICE, "vkGetDeviceQueue");
    static struct object *queue;
    if (queueFamilyIndex != 1 || queueIndex != 0) misuse("vkGetDeviceQueue: unknown queue");
    if (queue == NULL || queue->magic != MOCK_LIVE || queue->parent != owner) {
        queue = create(O_QUEUE, owner);
    }
    *pQueue = (VkQueue)queue;
    pthread_mutex_unlock(&mock_lock);
}

VkResult TZ_VKAPI vkDeviceWaitIdle(VkDevice device) {
    ENTER("vkDeviceWaitIdle");
    check(device, O_DEVICE, "vkDeviceWaitIdle");
    LEAVE();
    return VK_SUCCESS;
}

VkResult TZ_VKAPI vkCreateBuffer(VkDevice device, const VkBufferCreateInfo *pCreateInfo, const VkAllocationCallbacks *pAllocator, VkBuffer *pBuffer) {
    (void)pAllocator;
    ENTER("vkCreateBuffer");
    struct object *owner = check(device, O_DEVICE, "vkCreateBuffer");
    if (pCreateInfo->size == 0) misuse("vkCreateBuffer: zero size");
    struct object *buffer = create(O_BUFFER, owner);
    buffer->size = pCreateInfo->size;
    buffer->usage = pCreateInfo->usage;
    *pBuffer = (VkBuffer)buffer;
    LEAVE();
    return VK_SUCCESS;
}

void TZ_VKAPI vkDestroyBuffer(VkDevice device, VkBuffer buffer, const VkAllocationCallbacks *pAllocator) {
    (void)pAllocator;
    pthread_mutex_lock(&mock_lock);
    check(device, O_DEVICE, "vkDestroyBuffer");
    struct object *object = check(buffer, O_BUFFER, "vkDestroyBuffer");
    if (object != NULL) destroy(object);
    pthread_mutex_unlock(&mock_lock);
}

void TZ_VKAPI vkGetBufferMemoryRequirements(VkDevice device, VkBuffer buffer, VkMemoryRequirements *pMemoryRequirements) {
    pthread_mutex_lock(&mock_lock);
    check(device, O_DEVICE, "vkGetBufferMemoryRequirements");
    struct object *object = check(buffer, O_BUFFER, "vkGetBufferMemoryRequirements");
    memset(pMemoryRequirements, 0, sizeof *pMemoryRequirements);
    if (object != NULL) {
        pMemoryRequirements->size = (object->size + 15) & ~(uint64_t)15;
        pMemoryRequirements->alignment = 16;
        pMemoryRequirements->memoryTypeBits = config.unified ? 7 : 3;
    }
    pthread_mutex_unlock(&mock_lock);
}

VkResult TZ_VKAPI vkAllocateMemory(VkDevice device, const VkMemoryAllocateInfo *pAllocateInfo, const VkAllocationCallbacks *pAllocator, VkDeviceMemory *pMemory) {
    (void)pAllocator;
    ENTER("vkAllocateMemory");
    struct object *owner = check(device, O_DEVICE, "vkAllocateMemory");
    if (pAllocateInfo->allocationSize > config.max_allocation) {
        LEAVE();
        return VK_ERROR_OUT_OF_DEVICE_MEMORY;
    }
    struct object *memory = create(O_MEMORY, owner);
    memory->size = pAllocateInfo->allocationSize;
    memory->type_index = (int)pAllocateInfo->memoryTypeIndex;
    memory->data = (unsigned char *)calloc(1, (size_t)memory->size);
    *pMemory = (VkDeviceMemory)memory;
    LEAVE();
    return VK_SUCCESS;
}

void TZ_VKAPI vkFreeMemory(VkDevice device, VkDeviceMemory memory, const VkAllocationCallbacks *pAllocator) {
    (void)pAllocator;
    pthread_mutex_lock(&mock_lock);
    check(device, O_DEVICE, "vkFreeMemory");
    struct object *object = check(memory, O_MEMORY, "vkFreeMemory");
    if (object != NULL) {
        if (object->mapped) misuse("vkFreeMemory: the memory is still mapped");
        destroy(object);
    }
    pthread_mutex_unlock(&mock_lock);
}

VkResult TZ_VKAPI vkBindBufferMemory(VkDevice device, VkBuffer buffer, VkDeviceMemory memory, VkDeviceSize memoryOffset) {
    ENTER("vkBindBufferMemory");
    check(device, O_DEVICE, "vkBindBufferMemory");
    struct object *b = check(buffer, O_BUFFER, "vkBindBufferMemory");
    struct object *m = check(memory, O_MEMORY, "vkBindBufferMemory");
    if (b != NULL && m != NULL) {
        if (b->memory != NULL) misuse("vkBindBufferMemory: bound twice");
        if (memoryOffset != 0 || m->size < b->size) misuse("vkBindBufferMemory: the memory is too small");
        b->memory = m;
        b->hit_count = b->size / 4;
        b->hits = (uint32_t *)calloc((size_t)b->hit_count + 1, sizeof(uint32_t));
    }
    LEAVE();
    return VK_SUCCESS;
}

VkResult TZ_VKAPI vkMapMemory(VkDevice device, VkDeviceMemory memory, VkDeviceSize offset, VkDeviceSize size, VkFlags flags, void **ppData) {
    (void)flags;
    (void)size;
    ENTER("vkMapMemory");
    check(device, O_DEVICE, "vkMapMemory");
    struct object *m = check(memory, O_MEMORY, "vkMapMemory");
    if (m != NULL) {
        int host_visible = m->type_index == 1 || m->type_index == 2;
        if (!host_visible) misuse("vkMapMemory: the memory type is not host visible");
        if (m->mapped) misuse("vkMapMemory: mapped twice");
        m->mapped = 1;
        *ppData = m->data + offset;
    }
    LEAVE();
    return VK_SUCCESS;
}

void TZ_VKAPI vkUnmapMemory(VkDevice device, VkDeviceMemory memory) {
    pthread_mutex_lock(&mock_lock);
    check(device, O_DEVICE, "vkUnmapMemory");
    struct object *m = check(memory, O_MEMORY, "vkUnmapMemory");
    if (m != NULL) {
        if (!m->mapped) misuse("vkUnmapMemory: not mapped");
        m->mapped = 0;
    }
    pthread_mutex_unlock(&mock_lock);
}

/* ---- descriptors, pipelines ---- */

#define MOCK_CREATE(function, owner_type, type, info_type, create_args, field) \
    VkResult TZ_VKAPI function(VkDevice device, const info_type *pCreateInfo, const VkAllocationCallbacks *pAllocator, create_args *field) { \
        (void)pAllocator; \
        (void)pCreateInfo; \
        ENTER(#function); \
        struct object *owner = check(device, owner_type, #function); \
        *field = (create_args)create(type, owner); \
        LEAVE(); \
        return VK_SUCCESS; \
    }

#define MOCK_DESTROY(function, handle_type, type) \
    void TZ_VKAPI function(VkDevice device, handle_type handle, const VkAllocationCallbacks *pAllocator) { \
        (void)pAllocator; \
        pthread_mutex_lock(&mock_lock); \
        check(device, O_DEVICE, #function); \
        struct object *object = check(handle, type, #function); \
        if (object != NULL) destroy(object); \
        pthread_mutex_unlock(&mock_lock); \
    }

MOCK_CREATE(vkCreateDescriptorSetLayout, O_DEVICE, O_SET_LAYOUT, VkDescriptorSetLayoutCreateInfo, VkDescriptorSetLayout, pSetLayout)
MOCK_DESTROY(vkDestroyDescriptorSetLayout, VkDescriptorSetLayout, O_SET_LAYOUT)
MOCK_DESTROY(vkDestroyPipelineLayout, VkPipelineLayout, O_PIPELINE_LAYOUT)
MOCK_DESTROY(vkDestroyCommandPool, VkCommandPool, O_COMMAND_POOL)
MOCK_DESTROY(vkDestroyFence, VkFence, O_FENCE)
MOCK_DESTROY(vkDestroyDescriptorPool, VkDescriptorPool, O_POOL)

VkResult TZ_VKAPI vkCreatePipelineLayout(VkDevice device, const VkPipelineLayoutCreateInfo *pCreateInfo, const VkAllocationCallbacks *pAllocator, VkPipelineLayout *pPipelineLayout) {
    (void)pAllocator;
    ENTER("vkCreatePipelineLayout");
    struct object *owner = check(device, O_DEVICE, "vkCreatePipelineLayout");
    if (pCreateInfo->pushConstantRangeCount != 1 || pCreateInfo->pPushConstantRanges[0].size != 8) {
        misuse("vkCreatePipelineLayout: the push constants must be one range of 8 bytes");
    }
    *pPipelineLayout = (VkPipelineLayout)create(O_PIPELINE_LAYOUT, owner);
    LEAVE();
    return VK_SUCCESS;
}

VkResult TZ_VKAPI vkCreateFence(VkDevice device, const VkFenceCreateInfo *pCreateInfo, const VkAllocationCallbacks *pAllocator, VkFence *pFence) {
    (void)pAllocator;
    (void)pCreateInfo;
    ENTER("vkCreateFence");
    struct object *owner = check(device, O_DEVICE, "vkCreateFence");
    *pFence = (VkFence)create(O_FENCE, owner);
    LEAVE();
    return VK_SUCCESS;
}

VkResult TZ_VKAPI vkCreateCommandPool(VkDevice device, const VkCommandPoolCreateInfo *pCreateInfo, const VkAllocationCallbacks *pAllocator, VkCommandPool *pCommandPool) {
    (void)pAllocator;
    ENTER("vkCreateCommandPool");
    struct object *owner = check(device, O_DEVICE, "vkCreateCommandPool");
    if (pCreateInfo->queueFamilyIndex != 1) misuse("vkCreateCommandPool: wrong queue family");
    struct object *pool = create(O_COMMAND_POOL, owner);
    pool->flags = pCreateInfo->flags;
    *pCommandPool = (VkCommandPool)pool;
    LEAVE();
    return VK_SUCCESS;
}

VkResult TZ_VKAPI vkCreateDescriptorPool(VkDevice device, const VkDescriptorPoolCreateInfo *pCreateInfo, const VkAllocationCallbacks *pAllocator, VkDescriptorPool *pDescriptorPool) {
    (void)pAllocator;
    (void)pCreateInfo;
    ENTER("vkCreateDescriptorPool");
    struct object *owner = check(device, O_DEVICE, "vkCreateDescriptorPool");
    *pDescriptorPool = (VkDescriptorPool)create(O_POOL, owner);
    LEAVE();
    return VK_SUCCESS;
}

VkResult TZ_VKAPI vkAllocateDescriptorSets(VkDevice device, const VkDescriptorSetAllocateInfo *pAllocateInfo, VkDescriptorSet *pDescriptorSets) {
    ENTER("vkAllocateDescriptorSets");
    check(device, O_DEVICE, "vkAllocateDescriptorSets");
    struct object *pool = check(pAllocateInfo->descriptorPool, O_POOL, "vkAllocateDescriptorSets");
    for (uint32_t index = 0; index < pAllocateInfo->descriptorSetCount; index++) {
        pDescriptorSets[index] = (VkDescriptorSet)create(O_SET, pool);
    }
    LEAVE();
    return VK_SUCCESS;
}

void TZ_VKAPI vkUpdateDescriptorSets(VkDevice device, uint32_t descriptorWriteCount, const VkWriteDescriptorSet *pDescriptorWrites, uint32_t descriptorCopyCount, const void *pDescriptorCopies) {
    (void)pDescriptorCopies;
    pthread_mutex_lock(&mock_lock);
    check(device, O_DEVICE, "vkUpdateDescriptorSets");
    if (descriptorCopyCount != 0) misuse("vkUpdateDescriptorSets: copies are not expected");
    for (uint32_t index = 0; index < descriptorWriteCount; index++) {
        const VkWriteDescriptorSet *write = &pDescriptorWrites[index];
        struct object *set = check(write->dstSet, O_SET, "vkUpdateDescriptorSets");
        struct object *buffer = check(write->pBufferInfo[0].buffer, O_BUFFER, "vkUpdateDescriptorSets");
        if (set == NULL || buffer == NULL || write->dstBinding > 1) {
            misuse("vkUpdateDescriptorSets: invalid write");
            continue;
        }
        if (write->pBufferInfo[0].range > buffer->size || write->pBufferInfo[0].offset != 0) misuse("vkUpdateDescriptorSets: range outside the buffer");
        if (!(buffer->usage & VK_BUFFER_USAGE_STORAGE_BUFFER_BIT)) misuse("vkUpdateDescriptorSets: the buffer is not a storage buffer");
        set->bound_buffers[write->dstBinding] = buffer;
        set->bound_ranges[write->dstBinding] = write->pBufferInfo[0].range;
    }
    pthread_mutex_unlock(&mock_lock);
}

VkResult TZ_VKAPI vkCreateShaderModule(VkDevice device, const VkShaderModuleCreateInfo *pCreateInfo, const VkAllocationCallbacks *pAllocator, VkShaderModule *pShaderModule) {
    (void)pAllocator;
    ENTER("vkCreateShaderModule");
    struct object *owner = check(device, O_DEVICE, "vkCreateShaderModule");
    if ((uintptr_t)pCreateInfo->pCode % 4 != 0 || pCreateInfo->codeSize % 4 != 0) misuse("vkCreateShaderModule: unaligned code");
    struct object *module = create(O_MODULE, owner);
    module->word_count = (uint32_t)(pCreateInfo->codeSize / 4);
    module->words = (uint32_t *)malloc(pCreateInfo->codeSize);
    memcpy(module->words, pCreateInfo->pCode, pCreateInfo->codeSize);
    if (module->word_count < 5 || module->words[0] != 0x07230203U) misuse("vkCreateShaderModule: not SPIR-V");
    for (uint32_t position = 5; position < module->word_count;) {
        uint32_t size = module->words[position] >> 16, opcode = module->words[position] & 0xFFFF;
        if (size == 0 || position + size > module->word_count) {
            misuse("vkCreateShaderModule: malformed instruction");
            break;
        }
        if (opcode == 15 && size >= 4 && module->entry_count < 4) { /* OpEntryPoint model id name... */
            snprintf(module->entries[module->entry_count++], 32, "%s", (const char *)&module->words[position + 3]);
        }
        position += size;
    }
    *pShaderModule = (VkShaderModule)module;
    LEAVE();
    return VK_SUCCESS;
}

MOCK_DESTROY(vkDestroyShaderModule, VkShaderModule, O_MODULE)
MOCK_DESTROY(vkDestroyPipeline, VkPipeline, O_PIPELINE)

VkResult TZ_VKAPI vkCreateComputePipelines(VkDevice device, VkPipelineCache pipelineCache, uint32_t createInfoCount, const VkComputePipelineCreateInfo *pCreateInfos, const VkAllocationCallbacks *pAllocator, VkPipeline *pPipelines) {
    (void)pAllocator;
    ENTER("vkCreateComputePipelines");
    struct object *owner = check(device, O_DEVICE, "vkCreateComputePipelines");
    if (pipelineCache != 0) misuse("vkCreateComputePipelines: no pipeline cache expected");
    for (uint32_t index = 0; index < createInfoCount; index++) {
        const VkComputePipelineCreateInfo *info = &pCreateInfos[index];
        struct object *module = check(info->stage.module, O_MODULE, "vkCreateComputePipelines");
        check(info->layout, O_PIPELINE_LAYOUT, "vkCreateComputePipelines");
        int found = -1;
        for (int entry = 0; module != NULL && entry < module->entry_count; entry++) {
            if (strcmp(module->entries[entry], info->stage.pName) == 0) found = entry;
        }
        if (found < 0) {
            for (uint32_t earlier = 0; earlier < index; earlier++) {
                destroy((struct object *)pPipelines[earlier]);
                pPipelines[earlier] = 0;
            }
            LEAVE();
            return VK_ERROR_INITIALIZATION_FAILED;
        }
        struct object *pipeline = create(O_PIPELINE, owner);
        pipeline->module = module;
        pipeline->entry = strcmp(info->stage.pName, "init_main") == 0 ? 1 : strcmp(info->stage.pName, "probe_main") == 0 ? 2 : 0;
        pPipelines[index] = (VkPipeline)pipeline;
    }
    LEAVE();
    return VK_SUCCESS;
}

/* ---- commands ---- */

VkResult TZ_VKAPI vkAllocateCommandBuffers(VkDevice device, const VkCommandBufferAllocateInfo *pAllocateInfo, VkCommandBuffer *pCommandBuffers) {
    ENTER("vkAllocateCommandBuffers");
    check(device, O_DEVICE, "vkAllocateCommandBuffers");
    struct object *pool = check(pAllocateInfo->commandPool, O_COMMAND_POOL, "vkAllocateCommandBuffers");
    for (uint32_t index = 0; index < pAllocateInfo->commandBufferCount; index++) {
        struct object *buffer = create(O_COMMAND_BUFFER, pool);
        buffer->commands = (struct command *)calloc(MOCK_MAX_COMMANDS, sizeof(struct command));
        pCommandBuffers[index] = (VkCommandBuffer)buffer;
    }
    LEAVE();
    return VK_SUCCESS;
}

VkResult TZ_VKAPI vkBeginCommandBuffer(VkCommandBuffer commandBuffer, const VkCommandBufferBeginInfo *pBeginInfo) {
    (void)pBeginInfo;
    ENTER("vkBeginCommandBuffer");
    struct object *buffer = check(commandBuffer, O_COMMAND_BUFFER, "vkBeginCommandBuffer");
    if (buffer != NULL) {
        if (buffer->state == 1) misuse("vkBeginCommandBuffer: already recording");
        if (buffer->state == 2 && !(buffer->parent->flags & VK_COMMAND_POOL_CREATE_RESET_COMMAND_BUFFER_BIT)) misuse("vkBeginCommandBuffer: implicit reset needs the reset flag");
        buffer->state = 1;
        buffer->command_count = 0;
    }
    LEAVE();
    return VK_SUCCESS;
}

VkResult TZ_VKAPI vkEndCommandBuffer(VkCommandBuffer commandBuffer) {
    ENTER("vkEndCommandBuffer");
    struct object *buffer = check(commandBuffer, O_COMMAND_BUFFER, "vkEndCommandBuffer");
    if (buffer != NULL) {
        if (buffer->state != 1) misuse("vkEndCommandBuffer: not recording");
        buffer->state = 2;
    }
    LEAVE();
    return VK_SUCCESS;
}

VkResult TZ_VKAPI vkResetCommandBuffer(VkCommandBuffer commandBuffer, VkFlags flags) {
    (void)flags;
    ENTER("vkResetCommandBuffer");
    struct object *buffer = check(commandBuffer, O_COMMAND_BUFFER, "vkResetCommandBuffer");
    if (buffer != NULL) {
        buffer->state = 0;
        buffer->command_count = 0;
    }
    LEAVE();
    return VK_SUCCESS;
}

static struct command *record(VkCommandBuffer commandBuffer, enum command_kind kind, const char *where) {
    struct object *buffer = check(commandBuffer, O_COMMAND_BUFFER, where);
    if (buffer == NULL) return NULL;
    if (buffer->state != 1) {
        misuse("a command was recorded outside vkBeginCommandBuffer/vkEndCommandBuffer");
        return NULL;
    }
    if (buffer->command_count >= MOCK_MAX_COMMANDS) {
        misuse("too many commands");
        return NULL;
    }
    struct command *command = &buffer->commands[buffer->command_count++];
    memset(command, 0, sizeof *command);
    command->kind = kind;
    return command;
}

void TZ_VKAPI vkCmdBindPipeline(VkCommandBuffer commandBuffer, int32_t pipelineBindPoint, VkPipeline pipeline) {
    pthread_mutex_lock(&mock_lock);
    struct command *command = record(commandBuffer, C_BIND_PIPELINE, "vkCmdBindPipeline");
    if (pipelineBindPoint != VK_PIPELINE_BIND_POINT_COMPUTE) misuse("vkCmdBindPipeline: not the compute bind point");
    if (command != NULL) command->a = check(pipeline, O_PIPELINE, "vkCmdBindPipeline");
    pthread_mutex_unlock(&mock_lock);
}

void TZ_VKAPI vkCmdBindDescriptorSets(VkCommandBuffer commandBuffer, int32_t pipelineBindPoint, VkPipelineLayout layout, uint32_t firstSet, uint32_t descriptorSetCount, const VkDescriptorSet *pDescriptorSets, uint32_t dynamicOffsetCount, const uint32_t *pDynamicOffsets) {
    (void)pDynamicOffsets;
    pthread_mutex_lock(&mock_lock);
    struct command *command = record(commandBuffer, C_BIND_SET, "vkCmdBindDescriptorSets");
    check(layout, O_PIPELINE_LAYOUT, "vkCmdBindDescriptorSets");
    if (pipelineBindPoint != VK_PIPELINE_BIND_POINT_COMPUTE || firstSet != 0 || descriptorSetCount != 1 || dynamicOffsetCount != 0) misuse("vkCmdBindDescriptorSets: unexpected arguments");
    if (command != NULL) command->a = check(pDescriptorSets[0], O_SET, "vkCmdBindDescriptorSets");
    pthread_mutex_unlock(&mock_lock);
}

void TZ_VKAPI vkCmdPushConstants(VkCommandBuffer commandBuffer, VkPipelineLayout layout, VkFlags stageFlags, uint32_t offset, uint32_t size, const void *pValues) {
    pthread_mutex_lock(&mock_lock);
    struct command *command = record(commandBuffer, C_PUSH, "vkCmdPushConstants");
    check(layout, O_PIPELINE_LAYOUT, "vkCmdPushConstants");
    if (stageFlags != VK_SHADER_STAGE_COMPUTE_BIT || offset != 0 || size != 8) misuse("vkCmdPushConstants: unexpected range");
    if (command != NULL) memcpy(command->words, pValues, 8);
    pthread_mutex_unlock(&mock_lock);
}

void TZ_VKAPI vkCmdDispatch(VkCommandBuffer commandBuffer, uint32_t groupCountX, uint32_t groupCountY, uint32_t groupCountZ) {
    pthread_mutex_lock(&mock_lock);
    struct command *command = record(commandBuffer, C_DISPATCH, "vkCmdDispatch");
    if (groupCountY != 1 || groupCountZ != 1 || groupCountX == 0 || groupCountX > config.max_groups) misuse("vkCmdDispatch: group count outside the limits");
    if (command != NULL) command->words[0] = groupCountX;
    pthread_mutex_unlock(&mock_lock);
}

void TZ_VKAPI vkCmdCopyBuffer(VkCommandBuffer commandBuffer, VkBuffer srcBuffer, VkBuffer dstBuffer, uint32_t regionCount, const VkBufferCopy *pRegions) {
    pthread_mutex_lock(&mock_lock);
    struct command *command = record(commandBuffer, C_COPY, "vkCmdCopyBuffer");
    struct object *source = check(srcBuffer, O_BUFFER, "vkCmdCopyBuffer");
    struct object *destination = check(dstBuffer, O_BUFFER, "vkCmdCopyBuffer");
    if (regionCount != 1) misuse("vkCmdCopyBuffer: one region expected");
    if (command != NULL && source != NULL && destination != NULL) {
        if (!(source->usage & VK_BUFFER_USAGE_TRANSFER_SRC_BIT) || !(destination->usage & VK_BUFFER_USAGE_TRANSFER_DST_BIT)) misuse("vkCmdCopyBuffer: missing transfer usage");
        command->a = source;
        command->b = destination;
        command->size = pRegions[0].size;
        if (pRegions[0].size > source->size || pRegions[0].size > destination->size) misuse("vkCmdCopyBuffer: region outside a buffer");
    }
    pthread_mutex_unlock(&mock_lock);
}

void TZ_VKAPI vkCmdPipelineBarrier(VkCommandBuffer commandBuffer, VkFlags srcStageMask, VkFlags dstStageMask, VkFlags dependencyFlags, uint32_t memoryBarrierCount, const VkMemoryBarrier *pMemoryBarriers, uint32_t bufferMemoryBarrierCount, const VkBufferMemoryBarrier *pBufferMemoryBarriers, uint32_t imageMemoryBarrierCount, const VkImageMemoryBarrier *pImageMemoryBarriers) {
    (void)srcStageMask;
    (void)dstStageMask;
    (void)dependencyFlags;
    (void)pMemoryBarriers;
    (void)pBufferMemoryBarriers;
    (void)pImageMemoryBarriers;
    pthread_mutex_lock(&mock_lock);
    record(commandBuffer, C_BARRIER, "vkCmdPipelineBarrier");
    if (memoryBarrierCount != 1 || bufferMemoryBarrierCount != 0 || imageMemoryBarrierCount != 0) misuse("vkCmdPipelineBarrier: one memory barrier expected");
    pthread_mutex_unlock(&mock_lock);
}

/* The lane arithmetic of the mock kernel: out = in * 3 + 7, truncated to the output lane. */
static uint64_t lane_load(const unsigned char *data, uint64_t index, uint64_t size) {
    uint64_t value = 0;
    memcpy(&value, data + index * size, (size_t)size);
    return value;
}

/* The eight operations of the conformance probe (tests/gpu_vulkan_probe.spvasm) as a device that honours the strict
   controls computes them: IEEE binary32 arithmetic, no fused multiply-add (the mock is built with -ffp-contract=off),
   subnormals and the sign of a zero preserved, a NaN as the canonical 0x7FC00000. Lane i performs operation i / 3. */
static uint32_t probe_operation(uint32_t lane, uint32_t a_bits, uint32_t b_bits) {
    float a, b, result;
    memcpy(&a, &a_bits, sizeof a);
    memcpy(&b, &b_bits, sizeof b);
    volatile float product;
    switch (lane / 3) {
    case 0: product = a * b; result = product + -1.00048828125f; break;
    case 1: product = a * b; result = -product; break;
    case 2: result = a + b; break;
    case 3:
    case 4: result = a * b; break;
    case 5: result = a - b; break;
    case 6: result = (float)a_bits; break;
    default: result = (float)(int32_t)a_bits; break;
    }
    uint32_t bits;
    memcpy(&bits, &result, sizeof bits);
    return lane / 3 == 5 && result != result ? 0x7FC00000U : bits;
}

static void execute(struct object *buffer) {
    struct object *pipeline = NULL, *set = NULL, *last_output = NULL;
    uint32_t push[2] = {0, 0};
    uint64_t last_length = 0, last_lane = 0;
    int have_push = 0;
    for (int index = 0; index < buffer->command_count; index++) {
        struct command *command = &buffer->commands[index];
        switch (command->kind) {
        case C_COPY:
            if (command->a->memory == NULL || command->b->memory == NULL) {
                misuse("vkQueueSubmit: a copy uses an unbound buffer");
                break;
            }
            memcpy(command->b->memory->data, command->a->memory->data, (size_t)command->size);
            break;
        case C_BIND_PIPELINE: pipeline = command->a; break;
        case C_BIND_SET: set = command->a; break;
        case C_PUSH: memcpy(push, command->words, 8); have_push = 1; break;
        case C_BARRIER: break;
        case C_DISPATCH: {
            if (pipeline == NULL || set == NULL || !have_push) {
                misuse("vkQueueSubmit: a dispatch without pipeline, descriptor set, or push constants");
                break;
            }
            struct object *input = set->bound_buffers[0], *output = set->bound_buffers[1];
            if (input == NULL || output == NULL || input->memory == NULL || output->memory == NULL) {
                misuse("vkQueueSubmit: a dispatch with unbound storage buffers");
                break;
            }
            uint64_t length = push[0], base = push[1];
            if (length == 0) {
                misuse("vkQueueSubmit: dispatch with length 0");
                break;
            }
            uint64_t out_lane = set->bound_ranges[1] / length, in_lane = pipeline->entry != 1 ? set->bound_ranges[0] / length : 0;
            if ((out_lane != 4 && out_lane != 8) || (pipeline->entry != 1 && in_lane != 4 && in_lane != 8)) {
                misuse("vkQueueSubmit: lane sizes are not 4 or 8 bytes");
                break;
            }
            last_output = output;
            last_length = length;
            last_lane = out_lane;
            if (pipeline->entry == 2) probe_dispatches++;
            uint64_t invocations = (uint64_t)command->words[0] * 256;
            for (uint64_t x = 0; x < invocations; x++) {
                uint64_t i = base + x;
                if (i >= length) continue;
                if (pipeline->entry == 2) {
                    if (in_lane != 8 || out_lane != 4) {
                        misuse("vkQueueSubmit: the probe needs 8-byte input lanes and 4-byte output lanes");
                        break;
                    }
                    uint32_t a_bits, b_bits;
                    memcpy(&a_bits, input->memory->data + i * 8, 4);
                    memcpy(&b_bits, input->memory->data + i * 8 + 4, 4);
                    uint32_t bits = probe_operation((uint32_t)i, a_bits, b_bits);
                    if (config.probe_fault == (int)i + 1) bits ^= 1U;
                    memcpy(output->memory->data + i * 4, &bits, 4);
                    output->hits[i]++;
                    continue;
                }
                uint64_t value = pipeline->entry == 0 ? lane_load(input->memory->data, i, in_lane) : i;
                uint64_t result = value * 3 + 7;
                if (out_lane == 4) result &= 0xFFFFFFFFU;
                memcpy(output->memory->data + i * out_lane, &result, (size_t)out_lane);
                output->hits[(i * out_lane) / 4]++;
            }
            break;
        }
        }
    }
    /* Every lane of the output was written exactly once, and nothing past it. */
    if (last_output != NULL) {
        for (uint64_t slot = 0; slot < last_output->hit_count; slot++) {
            uint64_t lane = slot * 4 / last_lane;
            int first_slot = (slot * 4) % last_lane == 0;
            uint32_t expected = lane < last_length && first_slot ? 1 : 0;
            if (last_output->hits[slot] != expected) {
                char text[160];
                snprintf(text, sizeof text, "vkQueueSubmit: lane %llu was written %u times, expected %u", (unsigned long long)lane, last_output->hits[slot], expected);
                misuse(text);
                break;
            }
        }
    }
}

VkResult TZ_VKAPI vkQueueSubmit(VkQueue queue, uint32_t submitCount, const VkSubmitInfo *pSubmits, VkFence fence) {
    ENTER("vkQueueSubmit");
    check(queue, O_QUEUE, "vkQueueSubmit");
    struct object *signal = check(fence, O_FENCE, "vkQueueSubmit");
    if (submitCount != 1) misuse("vkQueueSubmit: one submission expected");
    for (uint32_t index = 0; index < submitCount; index++) {
        for (uint32_t buffer = 0; buffer < pSubmits[index].commandBufferCount; buffer++) {
            struct object *commands = check(pSubmits[index].pCommandBuffers[buffer], O_COMMAND_BUFFER, "vkQueueSubmit");
            if (commands == NULL) continue;
            if (commands->state != 2) {
                misuse("vkQueueSubmit: the command buffer is not executable");
                continue;
            }
            execute(commands);
        }
    }
    total_submits++;
    if (signal != NULL) {
        if (signal->state == 1) misuse("vkQueueSubmit: the fence is already signaled");
        signal->state = 1;
    }
    LEAVE();
    return VK_SUCCESS;
}

VkResult TZ_VKAPI vkResetFences(VkDevice device, uint32_t fenceCount, const VkFence *pFences) {
    ENTER("vkResetFences");
    check(device, O_DEVICE, "vkResetFences");
    for (uint32_t index = 0; index < fenceCount; index++) {
        struct object *fence = check(pFences[index], O_FENCE, "vkResetFences");
        if (fence != NULL) fence->state = 0;
    }
    LEAVE();
    return VK_SUCCESS;
}

VkResult TZ_VKAPI vkWaitForFences(VkDevice device, uint32_t fenceCount, const VkFence *pFences, VkBool32 waitAll, uint64_t timeout) {
    (void)waitAll;
    (void)timeout;
    ENTER("vkWaitForFences");
    check(device, O_DEVICE, "vkWaitForFences");
    for (uint32_t index = 0; index < fenceCount; index++) {
        struct object *fence = check(pFences[index], O_FENCE, "vkWaitForFences");
        if (fence != NULL && fence->state != 1) misuse("vkWaitForFences: waiting for a fence that was not submitted");
    }
    LEAVE();
    return VK_SUCCESS;
}

/* ---- the entry points of the library ---- */

struct entry {
    const char *name;
    PFN_vkVoidFunction function;
};

static PFN_vkVoidFunction lookup(const char *name) {
    if (config.missing[0] != '\0' && strcmp(config.missing, name) == 0) return NULL;
    if (config.no_version && strcmp(name, "vkEnumerateInstanceVersion") == 0) return NULL;
#define TZ_MOCK_ENTRY(function) if (strcmp(name, #function) == 0) return (PFN_vkVoidFunction)function;
    TZ_VK_GLOBAL_FUNCTIONS(TZ_MOCK_ENTRY)
    TZ_VK_INSTANCE_FUNCTIONS(TZ_MOCK_ENTRY)
    TZ_VK_DEVICE_FUNCTIONS(TZ_MOCK_ENTRY)
    TZ_MOCK_ENTRY(vkEnumerateInstanceVersion)
    TZ_MOCK_ENTRY(vkGetInstanceProcAddr)
#undef TZ_MOCK_ENTRY
    return NULL;
}

PFN_vkVoidFunction TZ_VKAPI vkGetInstanceProcAddr(VkInstance instance, const char *pName) {
    (void)instance;
    return lookup(pName);
}

PFN_vkVoidFunction TZ_VKAPI vkGetDeviceProcAddr(VkDevice device, const char *pName) {
    (void)device;
    return lookup(pName);
}
