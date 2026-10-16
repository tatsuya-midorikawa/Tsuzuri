/* The size-class allocator of `--allocator small` (PM05 Phase 2), native 64-bit targets only.
   generate_heap.py turns this file into heap-small.ll; tests/heap_small_test.c includes it
   as it is. It needs only malloc, realloc, free, memcpy, and memset, no thread library, and
   no system call that the C library does not wrap.

   Requests of 0 to 512 bytes are served from 32 size classes of 16-byte steps (class
   c holds blocks of 16 * (c + 1) bytes); everything larger goes to malloc, realloc, and free.
   A block has no header. Its class is found from its address:

     a region is 1 MiB, 1 MiB aligned, cut from a malloc(2 MiB) block, and holds blocks of
     one class; a two-level table (top[a >> 34], then a byte per region) tells whether an
     address lies in a region and of which class. An address that is not registered, such as
     a malloc block or a pointer of the C library's own, is freed with free.

   Each thread keeps a cache of free blocks per class (a LIFO list and its count) behind one
   thread-local pointer. A cache that runs dry takes one chain of `batch` = 4096 / block
   blocks from the class's central stack, or cuts a fresh chain from the region's untouched
   part; a cache that holds more than 2 * batch blocks gives its newest batch back. The
   central stack of a class is guarded by a try-lock: a thread that finds it held never waits.
   A refill then falls back to malloc (a block the registry does not know, freed with free
   later), and a flush keeps its blocks and tries again at the next free. A block is not
   owned by a thread, so a block freed by another thread goes into the caller's cache.

   Memory is never returned to the system. A thread that ends keeps its cache (at most
   16 KiB per class, 256 KiB in all): the blocks stay counted but nothing uses them again.

   Every object that carries this code shares one allocator state through the pointer
   tz_small_shared, which the linker merges (weak, hidden). Each object has its own thread-local
   pointer to its caches; a cache is just a list of blocks, so any cache can hold any block. */
typedef unsigned char tz_small_u8;
typedef unsigned int tz_small_u32;
typedef unsigned long long tz_small_u64;
typedef __SIZE_TYPE__ tz_small_usize;
typedef __UINTPTR_TYPE__ tz_small_uptr;

void *malloc(tz_small_usize);
void *realloc(void *, tz_small_usize);
void free(void *);

#if defined(_WIN32)
#define TZ_SMALL_HIDDEN
#else
#define TZ_SMALL_HIDDEN __attribute__((visibility("hidden")))
#endif

#define TZ_SMALL_CLASSES 32
#define TZ_SMALL_MAX 512
#define TZ_SMALL_BATCH_BYTES 4096
#define TZ_SMALL_REGION_SHIFT 20
#define TZ_SMALL_REGION ((tz_small_usize)1 << TZ_SMALL_REGION_SHIFT)
#define TZ_SMALL_LEAF_SHIFT 14
#define TZ_SMALL_LEAF ((tz_small_usize)1 << TZ_SMALL_LEAF_SHIFT)
#define TZ_SMALL_TOP_SHIFT (TZ_SMALL_REGION_SHIFT + TZ_SMALL_LEAF_SHIFT)
#define TZ_SMALL_ADDRESS_BITS 48
#define TZ_SMALL_TOP ((tz_small_usize)1 << (TZ_SMALL_ADDRESS_BITS - TZ_SMALL_TOP_SHIFT))

struct tz_small_slot {
    void *head;
    tz_small_u32 count;
    tz_small_u32 limit;
};

struct tz_small_cache {
    struct tz_small_cache *next;
    struct tz_small_slot slot[TZ_SMALL_CLASSES];
};

/* One class of the central state. Padded so that two classes never share a cache line. */
struct tz_small_class {
    int lock;
    tz_small_u32 regions;
    tz_small_uptr bump;
    tz_small_uptr bump_end;
    void *chains;
    tz_small_u64 chain_count;
    tz_small_u64 carved;
} __attribute__((aligned(64)));

struct tz_small_state {
    tz_small_u8 *top[TZ_SMALL_TOP];
    struct tz_small_cache *caches;
    tz_small_u64 leaves;
    int off;
    struct tz_small_class classes[TZ_SMALL_CLASSES];
};

static struct tz_small_state tz_small_own;
TZ_SMALL_HIDDEN struct tz_small_state *tz_small_shared = &tz_small_own;
static _Thread_local struct tz_small_cache *tz_small_tc;
/* Regions must lie below this address; the tests lower it to reach the fallback. */
static tz_small_uptr tz_small_limit = (tz_small_uptr)1 << TZ_SMALL_ADDRESS_BITS;

static inline tz_small_usize tz_small_block(tz_small_u32 class_index) {
    return ((tz_small_usize)class_index + 1) << 4;
}

static inline tz_small_u32 tz_small_batch(tz_small_u32 class_index) {
    return (tz_small_u32)(TZ_SMALL_BATCH_BYTES / tz_small_block(class_index));
}

/* The class of a request of 1 to 512 bytes; 0 bytes is served like 1. */
static inline tz_small_u32 tz_small_class_of(tz_small_usize size) {
    return (tz_small_u32)((size - (size != 0)) >> 4);
}

static inline int tz_small_try_lock(struct tz_small_class *state) {
    return !__atomic_exchange_n(&state->lock, 1, __ATOMIC_ACQUIRE);
}

static inline void tz_small_unlock(struct tz_small_class *state) {
    __atomic_store_n(&state->lock, 0, __ATOMIC_RELEASE);
}

/* 0 for an address outside the regions, else the class plus 1. */
static inline tz_small_u32 tz_small_lookup(tz_small_uptr address) {
    if (address >> TZ_SMALL_ADDRESS_BITS) return 0;
    tz_small_u8 *leaf = __atomic_load_n(&tz_small_shared->top[address >> TZ_SMALL_TOP_SHIFT], __ATOMIC_ACQUIRE);
    return leaf ? leaf[(address >> TZ_SMALL_REGION_SHIFT) & (TZ_SMALL_LEAF - 1)] : 0;
}

static int tz_small_register(tz_small_uptr region, tz_small_u32 value) {
    struct tz_small_state *shared = tz_small_shared;
    tz_small_u8 **entry = &shared->top[region >> TZ_SMALL_LEAF_SHIFT];
    tz_small_u8 *leaf = __atomic_load_n(entry, __ATOMIC_ACQUIRE);
    if (!leaf) {
        tz_small_u8 *fresh = malloc(TZ_SMALL_LEAF);
        if (!fresh) return 0;
        __builtin_memset(fresh, 0, TZ_SMALL_LEAF);
        if (__atomic_compare_exchange_n(entry, &leaf, fresh, 0, __ATOMIC_ACQ_REL, __ATOMIC_ACQUIRE)) {
            leaf = fresh;
            __atomic_fetch_add(&shared->leaves, 1, __ATOMIC_RELAXED);
        } else {
            free(fresh);
        }
    }
    /* Only the thread that holds the class's lock writes this byte, before any block of the region exists. */
    leaf[region & (TZ_SMALL_LEAF - 1)] = (tz_small_u8)value;
    return 1;
}

/* Called with the class's lock held. */
static int tz_small_region(struct tz_small_class *state, tz_small_u32 class_index) {
    void *raw = malloc(2 * TZ_SMALL_REGION);
    if (!raw) return 0;
    tz_small_uptr base = ((tz_small_uptr)raw + TZ_SMALL_REGION - 1) & ~(tz_small_uptr)(TZ_SMALL_REGION - 1);
    if (base + TZ_SMALL_REGION > tz_small_limit) {
        __atomic_store_n(&tz_small_shared->off, 1, __ATOMIC_RELAXED);
        free(raw);
        return 0;
    }
    if (!tz_small_register(base >> TZ_SMALL_REGION_SHIFT, class_index + 1)) {
        free(raw);
        return 0;
    }
    tz_small_usize block = tz_small_block(class_index);
    tz_small_u32 batch = tz_small_batch(class_index);
    /* A whole number of chains, so that every chain in the central stack has `batch` blocks. */
    tz_small_usize blocks = (TZ_SMALL_REGION / block) / batch * batch;
    state->bump = base;
    state->bump_end = base + blocks * block;
    state->regions++;
    return 1;
}

/* A chain of one batch of blocks from the central stack or the region, or null when the lock is held
   or no region can be made. */
static void *tz_small_take(tz_small_u32 class_index) {
    struct tz_small_class *state = &tz_small_shared->classes[class_index];
    if (__atomic_load_n(&tz_small_shared->off, __ATOMIC_RELAXED) || !tz_small_try_lock(state)) return 0;
    void *chain = state->chains;
    if (chain) {
        state->chains = ((void **)chain)[1];
        state->chain_count--;
        tz_small_unlock(state);
        return chain;
    }
    if (state->bump == state->bump_end && !tz_small_region(state, class_index)) {
        tz_small_unlock(state);
        return 0;
    }
    tz_small_usize block = tz_small_block(class_index);
    tz_small_u32 batch = tz_small_batch(class_index);
    char *first = (char *)state->bump;
    state->bump += (tz_small_uptr)batch * block;
    state->carved += batch;
    tz_small_unlock(state);
    char *cursor = first;
    for (tz_small_u32 index = 1; index < batch; index++) {
        *(void **)cursor = cursor + block;
        cursor += block;
    }
    *(void **)cursor = 0;
    return first;
}

static struct tz_small_cache *tz_small_cache_new(void) {
    struct tz_small_cache *cache = malloc(sizeof *cache);
    if (!cache) return 0;
    __builtin_memset(cache, 0, sizeof *cache);
    for (tz_small_u32 index = 0; index < TZ_SMALL_CLASSES; index++) cache->slot[index].limit = 2 * tz_small_batch(index);
    struct tz_small_state *shared = tz_small_shared;
    /* cache->next is null; a failed exchange stores the current head there. */
    while (!__atomic_compare_exchange_n(&shared->caches, &cache->next, cache, 1, __ATOMIC_RELEASE, __ATOMIC_RELAXED)) {
    }
    tz_small_tc = cache;
    return cache;
}

__attribute__((noinline)) static void *tz_small_alloc_slow(tz_small_usize size) {
    tz_small_u32 class_index = tz_small_class_of(size);
    struct tz_small_cache *cache = tz_small_tc ? tz_small_tc : tz_small_cache_new();
    void *chain = cache ? tz_small_take(class_index) : 0;
    if (!chain) return malloc(size);
    struct tz_small_slot *slot = &cache->slot[class_index];
    slot->head = *(void **)chain;
    slot->count = slot->limit / 2 - 1;
    return chain;
}

/* Gives the newest `batch` blocks of a cache that holds more than 2 * batch of them to the central stack. */
__attribute__((noinline)) static void tz_small_flush(struct tz_small_slot *slot, tz_small_u32 class_index) {
    struct tz_small_class *state = &tz_small_shared->classes[class_index];
    if (!tz_small_try_lock(state)) return;
    tz_small_u32 batch = slot->limit / 2;
    void *first = slot->head;
    void *last = first;
    for (tz_small_u32 index = 1; index < batch; index++) last = *(void **)last;
    slot->head = *(void **)last;
    slot->count -= batch;
    *(void **)last = 0;
    ((void **)first)[1] = state->chains;
    state->chains = first;
    state->chain_count++;
    tz_small_unlock(state);
}

__attribute__((noinline)) static void tz_small_free_slow(void *pointer, tz_small_u32 class_index) {
    struct tz_small_cache *cache = tz_small_cache_new();
    if (!cache) return;
    struct tz_small_slot *slot = &cache->slot[class_index];
    *(void **)pointer = 0;
    slot->head = pointer;
    slot->count = 1;
}

TZ_SMALL_HIDDEN void *tz_small_alloc(tz_small_usize size) {
    if (size > TZ_SMALL_MAX) return malloc(size);
    struct tz_small_cache *cache = tz_small_tc;
    if (cache) {
        struct tz_small_slot *slot = &cache->slot[tz_small_class_of(size)];
        void *block = slot->head;
        if (block) {
            slot->head = *(void **)block;
            slot->count--;
            return block;
        }
    }
    return tz_small_alloc_slow(size);
}

TZ_SMALL_HIDDEN void tz_small_free(void *pointer) {
    tz_small_u32 found = tz_small_lookup((tz_small_uptr)pointer);
    if (!found) {
        free(pointer);
        return;
    }
    struct tz_small_cache *cache = tz_small_tc;
    if (!cache) {
        tz_small_free_slow(pointer, found - 1);
        return;
    }
    struct tz_small_slot *slot = &cache->slot[found - 1];
    *(void **)pointer = slot->head;
    slot->head = pointer;
    if (++slot->count > slot->limit) tz_small_flush(slot, found - 1);
}

/* A block of a class stays where it is if the new size has the same class, otherwise it moves.
   A block that is not in a region stays with the C library. new_size 0 frees. */
TZ_SMALL_HIDDEN void *tz_small_realloc(void *old, tz_small_usize size) {
    if (!old) return tz_small_alloc(size);
    if (size == 0) {
        tz_small_free(old);
        return 0;
    }
    tz_small_u32 found = tz_small_lookup((tz_small_uptr)old);
    if (!found) return realloc(old, size);
    if (size <= TZ_SMALL_MAX && tz_small_class_of(size) == found - 1) return old;
    void *fresh = tz_small_alloc(size);
    if (!fresh) return 0;
    tz_small_usize block = tz_small_block(found - 1);
    __builtin_memcpy(fresh, old, block < size ? block : size);
    tz_small_free(old);
    return fresh;
}

/* For the tests only; generated code never calls it, and the threads that allocate must have stopped.
   out[0] bytes cut from regions, out[1] bytes in the central stacks and the caches (equal to out[0]
   once every block was freed), out[2] regions, out[3] caches, out[4] bytes of the allocator's own
   malloc blocks that are never freed (regions, leaves, and caches). */
TZ_SMALL_HIDDEN void tz_small_stats(tz_small_u64 *out) {
    struct tz_small_state *shared = tz_small_shared;
    tz_small_u64 carved = 0, stocked = 0, regions = 0, caches = 0;
    for (tz_small_u32 index = 0; index < TZ_SMALL_CLASSES; index++) {
        struct tz_small_class *state = &shared->classes[index];
        tz_small_u64 block = tz_small_block(index);
        while (!tz_small_try_lock(state)) {
        }
        carved += state->carved * block;
        stocked += state->chain_count * tz_small_batch(index) * block;
        regions += state->regions;
        tz_small_unlock(state);
    }
    for (struct tz_small_cache *cache = __atomic_load_n(&shared->caches, __ATOMIC_ACQUIRE); cache; cache = cache->next) {
        caches++;
        for (tz_small_u32 index = 0; index < TZ_SMALL_CLASSES; index++) stocked += (tz_small_u64)cache->slot[index].count * tz_small_block(index);
    }
    out[0] = carved;
    out[1] = stocked;
    out[2] = regions;
    out[3] = caches;
    out[4] = regions * 2 * TZ_SMALL_REGION + __atomic_load_n(&shared->leaves, __ATOMIC_RELAXED) * TZ_SMALL_LEAF + caches * sizeof(struct tz_small_cache);
}
