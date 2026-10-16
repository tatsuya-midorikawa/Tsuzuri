/* Unit tests of src/runtime/heap-small.c, the allocator behind `--allocator small` (PM05 Phase 2).
   The allocator source is included, so the tests reach its state; its malloc, realloc, and free
   are replaced by the hooks below, which count calls by size, fail on request, and check that a
   block returned to free came from malloc. Expected values come from the design (16-byte
   classes, 4096-byte batches, 1 MiB regions), worked out by hand, not from what the allocator
   reports. Build and run: clang -std=c11 -O1 -fno-strict-aliasing -pthread tests/heap_small_test.c
   (also with -fsanitize=address,undefined and with -fsanitize=thread); tests/small_allocator.mjs --unit
   runs all of them. */
#undef NDEBUG
#include <assert.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#if defined(_WIN32)
#include "../src/runtime/task-windows.h"
#else
#include <pthread.h>
#endif

#if defined(__SANITIZE_THREAD__)
#define SLOW 1
#elif defined(__has_feature)
#if __has_feature(thread_sanitizer)
#define SLOW 1
#endif
#endif
#ifndef SLOW
#define SLOW 0
#endif

void *test_malloc(size_t size);
void *test_realloc(void *pointer, size_t size);
void test_free(void *pointer);
#define malloc test_malloc
#define realloc test_realloc
#define free test_free
#include "../src/runtime/heap-small.c"
#undef malloc
#undef realloc
#undef free

#define MAGIC UINT64_C(0x5a11c0de5a11c0de)
#define HEADER 32
#define CACHE_BYTES sizeof(struct tz_small_cache)
#define LEAF_BYTES 16384
#define REGION_BYTES (2u << 20)

/* live_foreign: the bytes of the malloc blocks that are neither a region, a leaf, nor a cache. A block is counted from
   the size it was made with, so a request of the size of a cache is counted too, as a block of its own would be. */
static size_t malloc_calls, region_calls, leaf_calls, cache_calls, free_calls, realloc_calls, last_malloc, live_foreign;
/* A malloc of exactly this size fails; (size_t)-1 fails every malloc. */
static size_t injected_failure;

void *test_malloc(size_t size) {
    if (injected_failure == size || injected_failure == (size_t)-1) return NULL;
    uint64_t *block = malloc(size + HEADER);
    assert(block);
    block[0] = size;
    block[1] = MAGIC;
    block[2] = 0;
    /* A block that is not zeroed must not look like one (a region is too large to touch). */
    if (size != REGION_BYTES) memset((char *)block + HEADER, 0xa5, size);
    __atomic_fetch_add(&malloc_calls, 1, __ATOMIC_RELAXED);
    if (size == REGION_BYTES) __atomic_fetch_add(&region_calls, 1, __ATOMIC_RELAXED);
    else if (size == LEAF_BYTES) __atomic_fetch_add(&leaf_calls, 1, __ATOMIC_RELAXED);
    else if (size == CACHE_BYTES) __atomic_fetch_add(&cache_calls, 1, __ATOMIC_RELAXED);
    else {
        block[2] = 1;
        __atomic_fetch_add(&live_foreign, size, __ATOMIC_RELAXED);
        __atomic_store_n(&last_malloc, size, __ATOMIC_RELAXED);
    }
    return (char *)block + HEADER;
}

void test_free(void *pointer) {
    if (!pointer) return;
    uint64_t *block = (uint64_t *)((char *)pointer - HEADER);
    assert(block[1] == MAGIC);
    block[1] = 0;
    __atomic_fetch_add(&free_calls, 1, __ATOMIC_RELAXED);
    if (block[2]) __atomic_fetch_sub(&live_foreign, block[0], __ATOMIC_RELAXED);
    free(block);
}

void *test_realloc(void *pointer, size_t size) {
    if (!pointer) return test_malloc(size);
    if (injected_failure == size || injected_failure == (size_t)-1) return NULL;
    uint64_t *block = (uint64_t *)((char *)pointer - HEADER);
    assert(block[1] == MAGIC);
    size_t old = block[0];
    uint64_t counted = block[2];
    uint64_t *moved = realloc(block, size + HEADER);
    assert(moved);
    moved[0] = size;
    __atomic_fetch_add(&realloc_calls, 1, __ATOMIC_RELAXED);
    if (counted) __atomic_fetch_add(&live_foreign, size - old, __ATOMIC_RELAXED);
    return (char *)moved + HEADER;
}

static void reset(void) {
    memset(&tz_small_own, 0, sizeof tz_small_own);
    tz_small_tc = NULL;
    tz_small_limit = (uintptr_t)1 << TZ_SMALL_ADDRESS_BITS;
    injected_failure = 0;
    malloc_calls = region_calls = leaf_calls = cache_calls = free_calls = realloc_calls = 0;
}

struct stats { uint64_t carved, stocked, regions, caches, persistent; };

static struct stats stats(void) {
    uint64_t out[5];
    tz_small_stats(out);
    return (struct stats){out[0], out[1], out[2], out[3], out[4]};
}

/* The allocator's own malloc blocks that were not given back: what the hooks saw, not what the allocator reports. */
static uint64_t persistent(void) {
    return (uint64_t)region_calls * REGION_BYTES + (uint64_t)leaf_calls * LEAF_BYTES + (uint64_t)cache_calls * CACHE_BYTES;
}

static size_t block_of(size_t size) { return ((size ? size : 1) + 15) / 16 * 16; }
static unsigned class_of(size_t size) { return (unsigned)(block_of(size) / 16 - 1); }
static unsigned batch_of(unsigned class_index) { return 4096 / (16 * (class_index + 1)); }

static void fill(void *pointer, size_t size, unsigned tag) {
    for (size_t index = 0; index < size; index++) ((unsigned char *)pointer)[index] = (unsigned char)(tag + index * 31);
}

static void check_fill(const void *pointer, size_t size, unsigned tag) {
    for (size_t index = 0; index < size; index++) assert(((const unsigned char *)pointer)[index] == (unsigned char)(tag + index * 31));
}

static int compare(const void *left, const void *right) {
    uintptr_t a = *(const uintptr_t *)left, b = *(const uintptr_t *)right;
    return a < b ? -1 : a > b;
}

/* Every list is intact: a cache's count matches its list, a central chain has exactly `batch` blocks,
   every stocked block is 16-aligned, lies in a region of its class, and appears once, and none of the
   `live` blocks is stocked. Then the blocks cut from the regions are the stocked and the live ones. */
static void check_invariants(void *const *live, size_t live_count, int strict) {
    size_t capacity = 1024, stocked = 0;
    uintptr_t *seen = malloc(capacity * sizeof *seen);
    assert(seen);
#define SEE(block, class_index) do { \
        uintptr_t address = (uintptr_t)(block); \
        assert(address % 16 == 0 && tz_small_lookup(address) == (class_index) + 1); \
        if (stocked == capacity) { capacity *= 2; seen = realloc(seen, capacity * sizeof *seen); assert(seen); } \
        seen[stocked++] = address; \
    } while (0)
    for (struct tz_small_cache *cache = tz_small_shared->caches; cache; cache = cache->next) {
        for (unsigned class_index = 0; class_index < TZ_SMALL_CLASSES; class_index++) {
            struct tz_small_slot *slot = &cache->slot[class_index];
            /* A flush that finds the lock held keeps its blocks, so only a single thread must stay within the limit. */
            assert(slot->limit == 2 * batch_of(class_index) && (!strict || slot->count <= slot->limit));
            void *block = slot->head;
            for (unsigned index = 0; index < slot->count; index++, block = *(void **)block) {
                assert(block);
                SEE(block, class_index);
            }
            assert(!block);
        }
    }
    for (unsigned class_index = 0; class_index < TZ_SMALL_CLASSES; class_index++) {
        struct tz_small_class *state = &tz_small_shared->classes[class_index];
        void *chain = state->chains;
        for (uint64_t index = 0; index < state->chain_count; index++, chain = ((void **)chain)[1]) {
            assert(chain);
            void *block = chain;
            for (unsigned item = 0; item < batch_of(class_index); item++, block = *(void **)block) {
                assert(block);
                SEE(block, class_index);
            }
            assert(!block);
        }
        assert(!chain);
        assert(state->bump <= state->bump_end);
    }
    qsort(seen, stocked, sizeof *seen, compare);
    for (size_t index = 1; index < stocked; index++) assert(seen[index - 1] != seen[index]);
    uint64_t used = 0;
    for (size_t index = 0; index < live_count; index++) {
        unsigned found = tz_small_lookup((uintptr_t)live[index]);
        if (!found) continue;
        used += 16 * (uint64_t)found;
        assert(!bsearch(&(uintptr_t){(uintptr_t)live[index]}, seen, stocked, sizeof *seen, compare));
    }
    struct stats now = stats();
    assert(now.carved == now.stocked + used);
    free(seen);
#undef SEE
}

static unsigned long long seed = 0x9e3779b97f4a7c15ull;
static unsigned long long next_random(unsigned long long *state) {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    return *state;
}

/* Class boundaries: the block that serves each size has the class worked out by hand (0..16 bytes -> class 0,
   17..32 -> 1, ..., 497..512 -> 31); 513 and up goes to malloc. */
static void test_classes(void) {
    reset();
    static const struct { size_t size; unsigned class_index; } cases[] = {
        {0, 0}, {1, 0}, {8, 0}, {15, 0}, {16, 0}, {17, 1}, {31, 1}, {32, 1}, {33, 2}, {47, 2}, {48, 2}, {49, 3},
        {64, 3}, {65, 4}, {255, 15}, {256, 15}, {257, 16}, {496, 30}, {497, 31}, {511, 31}, {512, 31},
    };
    void *blocks[sizeof cases / sizeof *cases];
    for (size_t index = 0; index < sizeof cases / sizeof *cases; index++) {
        blocks[index] = tz_small_alloc(cases[index].size);
        assert(blocks[index] && (uintptr_t)blocks[index] % 16 == 0);
        assert(class_of(cases[index].size) == cases[index].class_index);
        assert(tz_small_lookup((uintptr_t)blocks[index]) == cases[index].class_index + 1);
        fill(blocks[index], cases[index].size, (unsigned)index);
    }
    for (size_t index = 0; index < sizeof cases / sizeof *cases; index++) check_fill(blocks[index], cases[index].size, (unsigned)index);
    check_invariants(blocks, sizeof cases / sizeof *cases, 1);
    for (size_t index = 0; index < sizeof cases / sizeof *cases; index++) tz_small_free(blocks[index]);
    struct stats now = stats();
    assert(now.carved == now.stocked && now.regions == 9 && malloc_calls == region_calls + leaf_calls + cache_calls && now.persistent == persistent());
    /* 513 bytes and larger leave the size classes. */
    size_t before = malloc_calls;
    void *large = tz_small_alloc(513);
    assert(large && malloc_calls == before + 1 && last_malloc == 513 && !tz_small_lookup((uintptr_t)large) && live_foreign == 513);
    tz_small_free(large);
    assert(live_foreign == 0);
    large = tz_small_alloc((size_t)1 << 20);
    assert(large && !tz_small_lookup((uintptr_t)large));
    tz_small_free(large);
    assert(live_foreign == 0);
    tz_small_free(NULL);
    assert(tz_small_lookup(0) == 0 && tz_small_lookup(1) == 0 && tz_small_lookup(16) == 0);
    assert(tz_small_lookup((uintptr_t)-16) == 0 && tz_small_lookup((uintptr_t)1 << 48) == 0 && tz_small_lookup((uintptr_t)&blocks) == 0);
    /* A region of the registry that nobody registered reads as 0: a leaf is zeroed when it is made (the hook fills what
       malloc gives it with 0xa5), and the 1 MiB regions 8192 places away are not regions. */
    for (size_t index = 0; index < sizeof cases / sizeof *cases; index++) assert(!tz_small_lookup((uintptr_t)blocks[index] ^ ((uintptr_t)1 << 33)));
    /* Whatever the table holds, an address with a bit above the 48th is not in a region: every top entry, and the field
       after the table, point at a leaf of 7s. */
    static unsigned char sevens[TZ_SMALL_LEAF];
    memset(sevens, 7, sizeof sevens);
    for (size_t index = 0; index < TZ_SMALL_TOP; index++) tz_small_own.top[index] = sevens;
    tz_small_own.caches = (struct tz_small_cache *)sevens;
    assert(tz_small_lookup(0x1000) == 7 && tz_small_lookup(((uintptr_t)1 << 48) - 16) == 7);
    assert(tz_small_lookup((uintptr_t)1 << 48) == 0 && tz_small_lookup(((uintptr_t)1 << 48) + 0x1000) == 0 && tz_small_lookup((uintptr_t)-1) == 0);
}

/* 10000 blocks of every class, the smallest and the largest request of the class: aligned, disjoint (the patterns
   survive), carved in whole batches, and all back after the free. */
static void test_every_class(void) {
    reset();
    /* ThreadSanitizer makes every byte access slow. */
    enum { COUNT = SLOW ? 2500 : 10000 };
    void **blocks = malloc(COUNT * sizeof *blocks);
    assert(blocks);
    for (unsigned class_index = 0; class_index < TZ_SMALL_CLASSES; class_index++) {
        for (int largest = 0; largest < 2; largest++) {
            size_t size = largest ? 16 * (class_index + 1) : (class_index ? 16 * class_index + 1 : 1);
            struct stats before = stats();
            for (size_t index = 0; index < COUNT; index++) {
                blocks[index] = tz_small_alloc(size);
                assert(blocks[index] && (uintptr_t)blocks[index] % 16 == 0 && tz_small_lookup((uintptr_t)blocks[index]) == class_index + 1);
                fill(blocks[index], size, (unsigned)index);
            }
            for (size_t index = 0; index < COUNT; index++) check_fill(blocks[index], size, (unsigned)index);
            /* The blocks come in chains of batch blocks: the first pass cuts ceil(10000 / batch) chains, the second reuses them. */
            uint64_t chains = (COUNT + batch_of(class_index) - 1) / batch_of(class_index);
            assert(stats().carved - before.carved == (largest ? 0 : chains * batch_of(class_index) * 16 * (class_index + 1)));
            check_invariants(blocks, COUNT, 1);
            for (size_t index = 0; index < COUNT; index++) {
                size_t other = (index * 7919) % COUNT;
                void *swap = blocks[index];
                blocks[index] = blocks[other];
                blocks[other] = swap;
            }
            for (size_t index = 0; index < COUNT; index++) tz_small_free(blocks[index]);
            struct stats after = stats();
            assert(after.carved == after.stocked);
            check_invariants(NULL, 0, 1);
        }
    }
    free(blocks);
}

/* Class 1 (32-byte blocks): batch 128, cache limit 256. Every count below is worked out by hand. */
static void test_chains_and_flush(void) {
    reset();
    void *first = tz_small_alloc(32);
    struct tz_small_slot *slot = &tz_small_tc->slot[1];
    struct tz_small_class *state = &tz_small_shared->classes[1];
    /* One chain of 128 blocks was cut; one block is out. */
    assert(state->carved == 128 && slot->count == 127 && state->chain_count == 0 && state->regions == 1);
    tz_small_free(first);
    assert(slot->count == 128);
    assert(tz_small_alloc(32) == first);
    tz_small_free(first);
    /* 384 blocks need three chains. Freeing them: the 257th free finds 257 > 256 blocks and gives 128 away. */
    void **blocks = malloc(384 * sizeof *blocks);
    assert(blocks);
    for (int index = 0; index < 384; index++) blocks[index] = tz_small_alloc(32);
    assert(state->carved == 384 && slot->count == 0 && state->chain_count == 0);
    for (int index = 0; index < 256; index++) tz_small_free(blocks[index]);
    assert(slot->count == 256 && state->chain_count == 0);
    tz_small_free(blocks[256]);
    assert(slot->count == 129 && state->chain_count == 1);
    for (int index = 257; index < 384; index++) tz_small_free(blocks[index]);
    assert(slot->count == 256 && state->chain_count == 1);
    check_invariants(NULL, 0, 1);
    struct stats now = stats();
    assert(now.carved == 384 * 32 && now.stocked == 384 * 32);
    /* The chain in the central stack serves the next refill: 256 blocks come from the cache, the 257th from the chain. */
    for (int index = 0; index < 256; index++) blocks[index] = tz_small_alloc(32);
    assert(slot->count == 0 && state->chain_count == 1);
    blocks[256] = tz_small_alloc(32);
    assert(slot->count == 127 && state->chain_count == 0 && state->carved == 384);
    check_invariants(blocks, 257, 1);
    for (int index = 0; index < 257; index++) tz_small_free(blocks[index]);
    assert(stats().carved == stats().stocked);
    free(blocks);
}

/* The central stack of a class is held by someone else (the test takes the lock). A refill falls back to malloc;
   a flush keeps its blocks, and the first free that finds the lock released gives a batch away.
   Class 1 again: batch 128, limit 256. */
static void test_contention(void) {
    reset();
    void *first = tz_small_alloc(32);
    struct tz_small_slot *slot = &tz_small_tc->slot[1];
    struct tz_small_class *state = &tz_small_shared->classes[1];
    tz_small_free(first);
    void **blocks = malloc(384 * sizeof *blocks);
    assert(blocks);
    for (int index = 0; index < 128; index++) blocks[index] = tz_small_alloc(32);
    assert(slot->count == 0 && state->carved == 128);
    assert(tz_small_try_lock(state));
    void *extra = tz_small_alloc(32);
    assert(extra && !tz_small_lookup((uintptr_t)extra) && last_malloc == 32 && live_foreign == 32 && state->carved == 128);
    tz_small_free(extra);
    assert(live_foreign == 0);
    tz_small_unlock(state);
    for (int index = 128; index < 384; index++) blocks[index] = tz_small_alloc(32);
    assert(state->carved == 384 && slot->count == 0);
    assert(tz_small_try_lock(state));
    for (int index = 0; index < 384; index++) tz_small_free(blocks[index]);
    assert(slot->count == 384 && state->chain_count == 0);
    tz_small_unlock(state);
    void *again = tz_small_alloc(32);
    assert(slot->count == 383);
    tz_small_free(again);
    assert(slot->count == 256 && state->chain_count == 1);
    check_invariants(NULL, 0, 1);
    assert(stats().carved == 384 * 32 && stats().carved == stats().stocked);
    free(blocks);
}

/* A region holds a whole number of chains. Class 31 (512-byte blocks): 2048 blocks per region, batch 8.
   Class 5 (96-byte blocks): 10922 fit, batch 42, so 260 chains = 10920 blocks. */
static void test_regions(void) {
    reset();
    static const struct { unsigned class_index; size_t blocks; } cases[] = {{31, 2048}, {5, 10920}, {0, 65536}};
    for (size_t item = 0; item < sizeof cases / sizeof *cases; item++) {
        size_t count = cases[item].blocks + 1;
        void **blocks = malloc(count * sizeof *blocks);
        assert(blocks);
        size_t size = 16 * (cases[item].class_index + 1);
        for (size_t index = 0; index < count; index++) blocks[index] = tz_small_alloc(size);
        assert(tz_small_shared->classes[cases[item].class_index].regions == 2);
        uintptr_t region = (uintptr_t)blocks[0] >> TZ_SMALL_REGION_SHIFT;
        for (size_t index = 0; index < cases[item].blocks; index++) assert((uintptr_t)blocks[index] >> TZ_SMALL_REGION_SHIFT == region);
        assert((uintptr_t)blocks[cases[item].blocks] >> TZ_SMALL_REGION_SHIFT != region);
        assert(tz_small_lookup((uintptr_t)blocks[cases[item].blocks]) == cases[item].class_index + 1);
        for (size_t index = 0; index < count; index++) tz_small_free(blocks[index]);
        free(blocks);
    }
    assert(region_calls == 6);
    check_invariants(NULL, 0, 1);
}

static void test_resize(void) {
    reset();
    /* Same class: the same block, whatever the direction. */
    char *p = tz_small_alloc(20);
    fill(p, 20, 1);
    assert(tz_small_realloc(p, 32) == p && tz_small_realloc(p, 17) == p);
    check_fill(p, 20, 1);
    /* Another class: a new block with the common prefix; the old block is the next one handed out for its class. */
    char *q = tz_small_realloc(p, 33);
    assert(q != p && tz_small_lookup((uintptr_t)q) == 3);
    check_fill(q, 20, 1);
    assert(tz_small_alloc(20) == p);
    fill(q, 33, 2);
    char *r = tz_small_realloc(q, 100);
    assert(tz_small_lookup((uintptr_t)r) == 7);
    check_fill(r, 33, 2);
    fill(r, 100, 3);
    char *s = tz_small_realloc(r, 40);
    assert(s != r && tz_small_lookup((uintptr_t)s) == 3);
    check_fill(s, 40, 3);
    /* Small to large and back to a foreign block. */
    fill(s, 48, 4);
    char *large = tz_small_realloc(s, 513);
    assert(!tz_small_lookup((uintptr_t)large) && live_foreign == 513);
    check_fill(large, 48, 4);
    fill(large, 513, 5);
    char *grown = tz_small_realloc(large, 5000);
    assert(!tz_small_lookup((uintptr_t)grown) && live_foreign == 5000);
    check_fill(grown, 513, 5);
    /* A foreign block stays with the C library when it shrinks below 512 bytes. */
    char *kept = tz_small_realloc(grown, 40);
    assert(!tz_small_lookup((uintptr_t)kept) && live_foreign == 40);
    check_fill(kept, 40, 5);
    assert(tz_small_realloc(kept, 0) == NULL && live_foreign == 0);
    /* A block of the largest class grows into malloc with all 512 bytes. */
    char *top = tz_small_alloc(512);
    fill(top, 512, 6);
    char *beyond = tz_small_realloc(top, 513);
    check_fill(beyond, 512, 6);
    assert(tz_small_alloc(512) == top);
    tz_small_free(beyond);
    /* NULL allocates; size 0 releases and returns NULL; the block is reused. */
    char *fresh = tz_small_realloc(NULL, 24);
    assert(fresh && tz_small_lookup((uintptr_t)fresh) == 2);
    assert(tz_small_realloc(fresh, 0) == NULL);
    assert(tz_small_alloc(24) == fresh);
    char *zero = tz_small_realloc(NULL, 0);
    assert(zero && tz_small_lookup((uintptr_t)zero) == 1);
    assert(live_foreign == 0);
}

/* Failing mallocs: a region that cannot be made, a cache that cannot be made, and a block that cannot be made. */
static void test_failures(void) {
    reset();
    injected_failure = REGION_BYTES;
    void *fallback = tz_small_alloc(32);
    assert(fallback && !tz_small_lookup((uintptr_t)fallback) && live_foreign == 32 && stats().regions == 0);
    tz_small_free(fallback);
    assert(live_foreign == 0);
    /* A leaf that cannot be made: the region goes back to malloc and the request falls back to it too. */
    reset();
    injected_failure = LEAF_BYTES;
    fallback = tz_small_alloc(32);
    assert(fallback && !tz_small_lookup((uintptr_t)fallback) && stats().regions == 0 && region_calls == 1 && free_calls == 1);
    tz_small_free(fallback);
    assert(live_foreign == 0 && free_calls == 2);
    reset();
    injected_failure = CACHE_BYTES;
    fallback = tz_small_alloc(32);
    assert(fallback && !tz_small_lookup((uintptr_t)fallback) && live_foreign == 32);
    tz_small_free(fallback);
    /* A thread that never allocated frees a block of a class: the cache is made first. */
    injected_failure = 0;
    char *block = tz_small_alloc(32);
    struct tz_small_cache *own = tz_small_tc;
    tz_small_tc = NULL;
    injected_failure = CACHE_BYTES;
    tz_small_free(block);
    assert(tz_small_tc == NULL && stats().stocked == 128 * 32 - 32);
    injected_failure = 0;
    tz_small_tc = own;
    reset();
    injected_failure = (size_t)-1;
    assert(tz_small_alloc(32) == NULL && tz_small_alloc(1000) == NULL);
    injected_failure = 0;
    char *kept = tz_small_alloc(100);
    fill(kept, 100, 9);
    injected_failure = (size_t)-1;
    /* A failed realloc leaves the block valid: it is neither freed nor changed. */
    assert(tz_small_realloc(kept, 600) == NULL);
    check_fill(kept, 100, 9);
    injected_failure = 0;
    char *large = tz_small_alloc(2000);
    injected_failure = 3000;
    assert(tz_small_realloc(large, 3000) == NULL);
    injected_failure = 0;
    tz_small_free(large);
    tz_small_free(kept);
    assert(live_foreign == 0);
}

/* A region above the address limit is never used: small requests fall back to malloc, once the limit is seen. */
static void test_address_limit(void) {
    reset();
    tz_small_limit = 1;
    void *block = tz_small_alloc(32);
    assert(block && !tz_small_lookup((uintptr_t)block) && last_malloc == 32 && region_calls == 1 && free_calls == 1);
    assert(tz_small_shared->off == 1);
    void *again = tz_small_alloc(48);
    assert(again && region_calls == 1 && !tz_small_lookup((uintptr_t)again));
    char *grown = tz_small_realloc(block, 200);
    assert(grown && !tz_small_lookup((uintptr_t)grown));
    tz_small_free(grown);
    tz_small_free(again);
    assert(live_foreign == 0 && stats().regions == 0);
    tz_small_limit = (uintptr_t)1 << TZ_SMALL_ADDRESS_BITS;
}

/* The blocks of each class that the test holds, and the most it ever held: with one thread, a chain is cut only when the cache
   and the central stack are empty, so every block cut is held, except those of the last chain. */
static size_t held_class[TZ_SMALL_CLASSES], peak_class[TZ_SMALL_CLASSES];

static void hold(size_t size) {
    unsigned class_index = class_of(size);
    if (++held_class[class_index] > peak_class[class_index]) peak_class[class_index] = held_class[class_index];
}

/* Random operations of one thread against a model: sizes 0..600, allocate/free/realloc, the patterns must survive,
   the lists stay intact, and the accounting identity holds at every checkpoint. A block that is not in a region
   (over 512 bytes) stays with the C library, also when it shrinks. */
static void test_random(void) {
    reset();
    enum { SLOTS = 3000 };
    void **blocks = calloc(SLOTS, sizeof *blocks);
    size_t *sizes = calloc(SLOTS, sizeof *sizes);
    unsigned char *foreign = calloc(SLOTS, 1);
    assert(blocks && sizes && foreign);
    memset(held_class, 0, sizeof held_class);
    memset(peak_class, 0, sizeof peak_class);
    unsigned long long state = seed;
    for (int step = 0; step < (SLOW ? 100000 : 400000); step++) {
        size_t slot = (size_t)(next_random(&state) % SLOTS);
        size_t size = (size_t)(next_random(&state) % 601);
        unsigned operation = (unsigned)(next_random(&state) % 4);
        if (blocks[slot]) {
            check_fill(blocks[slot], sizes[slot], (unsigned)slot);
            if (operation == 0) {
                tz_small_free(blocks[slot]);
                if (!foreign[slot]) held_class[class_of(sizes[slot])]--;
                blocks[slot] = NULL;
            } else if (operation == 1 && size) {
                void *moved = tz_small_realloc(blocks[slot], size);
                assert(moved);
                check_fill(moved, size < sizes[slot] ? size : sizes[slot], (unsigned)slot);
                if (!foreign[slot]) {
                    if (size > 512) {
                        held_class[class_of(sizes[slot])]--;
                        foreign[slot] = 1;
                    } else if (class_of(size) != class_of(sizes[slot])) {
                        hold(size);
                        held_class[class_of(sizes[slot])]--;
                    }
                }
                assert(!tz_small_lookup((uintptr_t)moved) == !!foreign[slot]);
                blocks[slot] = moved;
                sizes[slot] = size;
                fill(moved, size, (unsigned)slot);
            }
        } else {
            blocks[slot] = tz_small_alloc(size);
            assert(blocks[slot] && (uintptr_t)blocks[slot] % 16 == 0);
            foreign[slot] = size > 512;
            if (!foreign[slot]) hold(size);
            sizes[slot] = size;
            fill(blocks[slot], size, (unsigned)slot);
        }
        if (step % 25000 == 24999) check_invariants(blocks, SLOTS, 1);
    }
    for (unsigned class_index = 0; class_index < TZ_SMALL_CLASSES; class_index++) {
        uint64_t carved = tz_small_shared->classes[class_index].carved;
        assert(carved >= peak_class[class_index] && carved <= peak_class[class_index] + batch_of(class_index));
    }
    for (size_t slot = 0; slot < SLOTS; slot++) {
        if (!blocks[slot]) continue;
        check_fill(blocks[slot], sizes[slot], (unsigned)slot);
        tz_small_free(blocks[slot]);
    }
    struct stats now = stats();
    if (now.carved != now.stocked || live_foreign) fprintf(stderr, "carved %llu stocked %llu live_foreign %zu\n", (unsigned long long)now.carved, (unsigned long long)now.stocked, live_foreign);
    assert(now.carved == now.stocked && live_foreign == 0);
    check_invariants(NULL, 0, 1);
    free(blocks);
    free(sizes);
    free(foreign);
}

#define THREADS 4
#define MAILBOXES 64
#define OPERATIONS (SLOW ? 25000 : 100000)
#define LIVE 512

static void *mailbox[MAILBOXES];
static int ready;

struct job { unsigned id; unsigned long long state; };

/* A block starts with its size and a tag, then a pattern; any thread can verify and free it. */
static void *make(size_t size, unsigned tag) {
    unsigned char *block = tz_small_alloc(size);
    assert(block);
    uint32_t header[2] = {(uint32_t)size, tag};
    memcpy(block, header, sizeof header);
    fill(block + 8, size - 8, tag);
    return block;
}

static void release(void *block) {
    uint32_t header[2];
    memcpy(header, block, sizeof header);
    check_fill((unsigned char *)block + 8, header[0] - 8, header[1]);
    tz_small_free(block);
}

static void *worker(void *argument) {
    struct job *job = argument;
    void *live[LIVE] = {0};
    __atomic_fetch_add(&ready, 1, __ATOMIC_SEQ_CST);
    while (__atomic_load_n(&ready, __ATOMIC_SEQ_CST) < THREADS) {
    }
    for (int step = 0; step < OPERATIONS; step++) {
        size_t slot = (size_t)(next_random(&job->state) % LIVE);
        size_t size = 8 + (size_t)(next_random(&job->state) % 593);
        unsigned tag = job->id * 1000003u + (unsigned)step;
        if (live[slot]) {
            /* Half of the blocks go to another thread: through a mailbox, and the block found there is freed here. */
            if (next_random(&job->state) % 2) {
                void *taken = __atomic_exchange_n(&mailbox[next_random(&job->state) % MAILBOXES], live[slot], __ATOMIC_ACQ_REL);
                if (taken) release(taken);
            } else {
                release(live[slot]);
            }
            live[slot] = NULL;
        } else {
            live[slot] = make(size, tag);
        }
    }
    for (size_t slot = 0; slot < LIVE; slot++) if (live[slot]) release(live[slot]);
    return NULL;
}

/* Four threads allocate and free at once and hand blocks to each other. After they ended and the mailboxes were
   emptied, every block cut from a region is stocked; the second round proves that the memory is reused. */
static void test_threads(void) {
    reset();
    pthread_t threads[THREADS];
    struct job jobs[THREADS];
    uint64_t first_round = 0;
    for (int round = 0; round < 2; round++) {
        ready = 0;
        for (unsigned index = 0; index < THREADS; index++) {
            jobs[index] = (struct job){index, seed ^ ((unsigned long long)(index + 1) * 0x2545f4914f6cdd1dull) ^ (unsigned long long)round};
            assert(pthread_create(&threads[index], NULL, worker, &jobs[index]) == 0);
        }
        for (unsigned index = 0; index < THREADS; index++) assert(pthread_join(threads[index], NULL) == 0);
        for (int index = 0; index < MAILBOXES; index++) {
            if (mailbox[index]) release(mailbox[index]);
            mailbox[index] = NULL;
        }
        struct stats now = stats();
        assert(now.carved == now.stocked && live_foreign == 0 && now.caches >= (uint64_t)THREADS * (round + 1) && now.caches <= (uint64_t)THREADS * (round + 1) + 1);
        check_invariants(NULL, 0, 0);
        if (round == 0) first_round = now.carved;
        /* At most THREADS * LIVE blocks of the largest class are live; even if every cache keeps a full limit, twice that bounds the growth. */
        else assert(now.carved <= first_round + 2 * (uint64_t)THREADS * LIVE * 608);
    }
}

int main(void) {
    test_classes();
    test_every_class();
    test_chains_and_flush();
    test_contention();
    test_regions();
    test_resize();
    test_failures();
    test_address_limit();
    test_random();
    test_threads();
    puts("heap_small_test: ok");
    return 0;
}
