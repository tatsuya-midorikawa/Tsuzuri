#include <stdatomic.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define TRACKED_MAGIC UINT64_C(0x51a110ca7e)
static _Atomic uint64_t tracked_calls, tracked_bytes, tracked_live;

static void fail(const char *message) {
    fprintf(stderr, "tracked allocator: %s\n", message);
    abort();
}

static size_t allocation_size(uint64_t size) {
    if (size > SIZE_MAX - 16) fail("allocation size overflow");
    return (size_t)size + 16;
}

void *tracked_malloc(uint64_t size) {
    uint64_t *header = malloc(allocation_size(size));
    if (!header) fail("malloc failed");
    header[0] = size;
    header[1] = TRACKED_MAGIC;
    atomic_fetch_add_explicit(&tracked_calls, 1, memory_order_relaxed);
    atomic_fetch_add_explicit(&tracked_bytes, size, memory_order_relaxed);
    atomic_fetch_add_explicit(&tracked_live, size, memory_order_relaxed);
    return header + 2;
}

void *tracked_calloc(uint64_t count, uint64_t size) {
    if (size && count > UINT64_MAX / size) fail("calloc size overflow");
    uint64_t bytes = count * size;
    void *value = tracked_malloc(bytes);
    memset(value, 0, (size_t)bytes);
    return value;
}

void tracked_free(void *value) {
    if (!value) return;
    uint64_t *header = (uint64_t *)value - 2;
    if (header[1] != TRACKED_MAGIC) fail("invalid allocation header");
    header[1] = 0;
    if (atomic_fetch_sub_explicit(&tracked_live, header[0], memory_order_relaxed) < header[0])
        fail("live byte count underflow");
    free(header);
}

void *tracked_realloc(void *value, uint64_t size) {
    if (!value) return tracked_malloc(size);
    if (!size) {
        tracked_free(value);
        return NULL;
    }
    uint64_t *header = (uint64_t *)value - 2;
    if (header[1] != TRACKED_MAGIC) fail("invalid realloc header");
    const uint64_t previous = header[0];
    header = realloc(header, allocation_size(size));
    if (!header) fail("realloc failed");
    header[0] = size;
    atomic_fetch_add_explicit(&tracked_calls, 1, memory_order_relaxed);
    atomic_fetch_add_explicit(&tracked_bytes, size, memory_order_relaxed);
    if (atomic_fetch_sub_explicit(&tracked_live, previous, memory_order_relaxed) < previous)
        fail("live byte count underflow");
    atomic_fetch_add_explicit(&tracked_live, size, memory_order_relaxed);
    return header + 2;
}

void tracked_reset(void) {
    if (atomic_load_explicit(&tracked_live, memory_order_relaxed)) fail("reset with live allocations");
    atomic_store_explicit(&tracked_calls, 0, memory_order_relaxed);
    atomic_store_explicit(&tracked_bytes, 0, memory_order_relaxed);
}

void tracked_read(uint64_t *calls, uint64_t *bytes, uint64_t *live) {
    *calls = atomic_load_explicit(&tracked_calls, memory_order_relaxed);
    *bytes = atomic_load_explicit(&tracked_bytes, memory_order_relaxed);
    *live = atomic_load_explicit(&tracked_live, memory_order_relaxed);
}
