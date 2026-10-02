/* Trap boundary runtime (E14 Phase 2), embedded into `--trap-mode return` objects.
   A boundary is one tsuzuri_try_<name> call: the first trap inside it longjmps back, every
   allocation made since the call is freed, and the host gets status 1. Tsuzuri code never
   unwinds, so no destructor or lock is skipped; the runtime releases its own locks before any
   callback runs. */
#include <pthread.h>
#include <setjmp.h>
#include <stddef.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

#ifndef TZ_TRAP_API
#if defined(_WIN32)
#define TZ_TRAP_API
#else
#define TZ_TRAP_API __attribute__((weak, visibility("hidden")))
#endif
#endif
#ifndef TZ_TRAP_MALLOC
#define TZ_TRAP_MALLOC malloc
#endif
#ifndef TZ_TRAP_FREE
#define TZ_TRAP_FREE free
#endif
#ifndef TZ_TRAP_REALLOC
#define TZ_TRAP_REALLOC realloc
#endif
#ifndef TZ_TRAP_STATE_MALLOC
#define TZ_TRAP_STATE_MALLOC malloc
#endif
#ifndef TZ_TRAP_STATE_FREE
#define TZ_TRAP_STATE_FREE free
#endif

#ifndef TSUZURI_TRAP_INFO_DEFINED
#define TSUZURI_TRAP_INFO_DEFINED
typedef struct {
    uint32_t site;
    uint32_t kind;
} tsuzuri_trap_info;
#endif

#ifndef TSUZURI_TRAP_HOOKS_DEFINED
#define TSUZURI_TRAP_HOOKS_DEFINED
struct tz_trap_hooks {
    void *(*owner)(void);
    int32_t (*item)(void *, void (*)(void *, uint64_t), uint32_t (*)(void *, uint64_t),
                    void *, uint64_t, uint32_t *, tsuzuri_trap_info *);
    void (*resume)(const tsuzuri_trap_info *);
    void *(*allocate)(size_t);
    void (*release)(void *);
};
__attribute__((weak, visibility("hidden"))) struct tz_trap_hooks tsuzuri_trap_hooks;
#endif

struct tz_block;

/* The owner of the blocks allocated while one tsuzuri_try_<name> call runs. Worker threads of a
   Task.parallel group allocate into the owner of the thread that submitted the group. */
struct tz_boundary {
    pthread_mutex_t lock;
    struct tz_block *volatile blocks;
    struct tz_block **volatile buckets;
    size_t capacity;
    size_t count;
};

struct tz_block {
    struct tz_block *prev;
    struct tz_block *next;
    struct tz_block *bucket_next;
    void *pointer;
};

struct tz_frame {
    jmp_buf env;
    struct tz_boundary *owner;
    struct tz_frame *outer;
    volatile uint32_t site;
    volatile uint32_t kind;
};

static _Thread_local struct tz_frame *tz_frame;

TZ_TRAP_API
void tsuzuri_trap_raise(uint32_t site, uint32_t kind) {
    struct tz_frame *frame = tz_frame;
    if (!frame) return;
    frame->site = site;
    frame->kind = kind;
    longjmp(frame->env, 1);
}

/* The boundary a Task.parallel group submitted from this thread belongs to, or NULL. */
TZ_TRAP_API
void *tsuzuri_boundary_owner(void) {
    return tz_frame ? tz_frame->owner : NULL;
}

static void tz_boundary_release(struct tz_boundary *boundary, int discard) {
    struct tz_block *block = boundary->blocks;
    boundary->blocks = NULL;
    while (block) {
        struct tz_block *next = block->next;
        if (discard) {
            TZ_TRAP_FREE(block->pointer);
        }
        TZ_TRAP_FREE(block);
        block = next;
    }
    TZ_TRAP_FREE(boundary->buckets);
    boundary->buckets = NULL;
}

/* Runs one item of a group on a worker (or the submitting thread) under its own frame, so a trap
   stops that item only; the group then reports it to the call that submitted the group. Returns
   1 and fills `trap` if the item trapped. */
TZ_TRAP_API
int32_t tsuzuri_boundary_item(void *owner, void (*run)(void *, uint64_t),
                              uint32_t (*run_result)(void *, uint64_t), void *context,
                              uint64_t index, uint32_t *failed, tsuzuri_trap_info *trap) {
    struct tz_frame frame;
    frame.owner = owner;
    frame.outer = tz_frame;
    frame.site = 0;
    frame.kind = 0;
    tz_frame = &frame;
    int32_t trapped = 0;
    if (setjmp(frame.env) == 0) {
        if (run_result) *failed = run_result(context, index);
        else run(context, index);
    } else {
        trapped = 1;
    }
    tz_frame = frame.outer;
    if (trapped) {
        trap->site = frame.site;
        trap->kind = frame.kind;
    }
    return trapped;
}

/* Raises the trap of a finished group in the thread that submitted it. */
TZ_TRAP_API
void tsuzuri_boundary_resume(const tsuzuri_trap_info *trap) {
    tsuzuri_trap_raise(trap->site, trap->kind);
}

/* 0: finished, 1: trapped (`trap` filled), 2: this thread is already inside a boundary. */
TZ_TRAP_API
int32_t tsuzuri_boundary_run(void (*thunk)(void *), void *argument, tsuzuri_trap_info *trap) {
    if (tz_frame) return 2;
    /* Off the stack: code that runs after setjmp changes it, and an automatic object changed
       since setjmp is indeterminate once longjmp returns here. */
    struct tz_boundary *volatile boundary = TZ_TRAP_STATE_MALLOC(sizeof *boundary);
    if (!boundary) abort();
    pthread_mutex_init(&boundary->lock, NULL);
    boundary->blocks = NULL;
    boundary->buckets = NULL;
    boundary->capacity = 0;
    boundary->count = 0;
    struct tz_frame frame;
    frame.owner = boundary;
    frame.outer = NULL;
    frame.site = 0;
    frame.kind = 0;
    tz_frame = &frame;
    int32_t trapped = 0;
    if (setjmp(frame.env) == 0) thunk(argument);
    else trapped = 1;
    tz_frame = NULL;
    /* A finished call hands what is left (results the host owns) over; a trapped one drops it all. */
    tz_boundary_release(boundary, trapped);
    pthread_mutex_destroy(&boundary->lock);
    TZ_TRAP_STATE_FREE(boundary);
    if (trapped && trap) {
        trap->site = frame.site;
        trap->kind = frame.kind;
    }
    return trapped;
}

static size_t tz_block_bucket(void *pointer, size_t capacity) {
    uintptr_t bits = (uintptr_t)pointer >> 4;
    return (size_t)(bits ^ (bits >> 12)) & (capacity - 1);
}

static struct tz_block **tz_block_slot(struct tz_boundary *owner, void *pointer) {
    struct tz_block **slot = &owner->buckets[tz_block_bucket(pointer, owner->capacity)];
    while (*slot && (*slot)->pointer != pointer) slot = &(*slot)->bucket_next;
    return slot;
}

static struct tz_block *tz_block_track(struct tz_boundary *owner, void *pointer) {
    struct tz_block *block = TZ_TRAP_MALLOC(sizeof *block);
    if (!block) return NULL;
    if (owner->count == owner->capacity) {
        if (owner->capacity > SIZE_MAX / sizeof(struct tz_block *) / 2) {
            TZ_TRAP_FREE(block);
            return NULL;
        }
        size_t capacity = owner->capacity ? owner->capacity * 2 : 16;
        struct tz_block **buckets = TZ_TRAP_MALLOC(capacity * sizeof *buckets);
        if (!buckets) {
            TZ_TRAP_FREE(block);
            return NULL;
        }
        memset(buckets, 0, capacity * sizeof *buckets);
        for (struct tz_block *entry = owner->blocks; entry; entry = entry->next) {
            size_t slot = tz_block_bucket(entry->pointer, capacity);
            entry->bucket_next = buckets[slot];
            buckets[slot] = entry;
        }
        TZ_TRAP_FREE(owner->buckets);
        owner->buckets = buckets;
        owner->capacity = capacity;
    }
    block->pointer = pointer;
    block->prev = NULL;
    block->next = owner->blocks;
    if (owner->blocks) owner->blocks->prev = block;
    owner->blocks = block;
    size_t slot = tz_block_bucket(pointer, owner->capacity);
    block->bucket_next = owner->buckets[slot];
    owner->buckets[slot] = block;
    ++owner->count;
    return block;
}

static struct tz_block *tz_block_untrack(struct tz_boundary *owner, void *pointer) {
    if (owner->capacity == 0) return NULL;
    struct tz_block **slot = tz_block_slot(owner, pointer);
    struct tz_block *block = *slot;
    if (!block) return NULL;
    *slot = block->bucket_next;
    if (block->prev) block->prev->next = block->next;
    else owner->blocks = block->next;
    if (block->next) block->next->prev = block->prev;
    --owner->count;
    return block;
}

TZ_TRAP_API
void *tsuzuri_tracked_malloc(size_t size) {
    void *pointer = TZ_TRAP_MALLOC(size);
    if (!pointer) return NULL;
    struct tz_boundary *owner = tz_frame ? tz_frame->owner : NULL;
    if (owner) {
        pthread_mutex_lock(&owner->lock);
        struct tz_block *block = tz_block_track(owner, pointer);
        pthread_mutex_unlock(&owner->lock);
        if (!block) {
            TZ_TRAP_FREE(pointer);
            return NULL;
        }
    }
    return pointer;
}

TZ_TRAP_API
void tsuzuri_tracked_free(void *pointer) {
    if (!pointer) return;
    struct tz_boundary *owner = tz_frame ? tz_frame->owner : NULL;
    if (owner) {
        pthread_mutex_lock(&owner->lock);
        struct tz_block *block = tz_block_untrack(owner, pointer);
        pthread_mutex_unlock(&owner->lock);
        TZ_TRAP_FREE(block);
    }
    TZ_TRAP_FREE(pointer);
}

TZ_TRAP_API
void *tsuzuri_tracked_realloc(void *pointer, size_t size) {
    if (!pointer) return tsuzuri_tracked_malloc(size);
    if (size == 0) {
        tsuzuri_tracked_free(pointer);
        return NULL;
    }
    struct tz_boundary *owner = tz_frame ? tz_frame->owner : NULL;
    if (!owner) return TZ_TRAP_REALLOC(pointer, size);
    pthread_mutex_lock(&owner->lock);
    struct tz_block *block = owner->capacity ? *tz_block_slot(owner, pointer) : NULL;
    int added = block == NULL;
    if (added) block = tz_block_track(owner, pointer);
    if (!block) {
        pthread_mutex_unlock(&owner->lock);
        return NULL;
    }
    struct tz_block **slot = tz_block_slot(owner, pointer);
    void *next = TZ_TRAP_REALLOC(pointer, size);
    if (next) {
        *slot = block->bucket_next;
        block->pointer = next;
        size_t bucket = tz_block_bucket(next, owner->capacity);
        block->bucket_next = owner->buckets[bucket];
        owner->buckets[bucket] = block;
    } else if (added) {
        TZ_TRAP_FREE(tz_block_untrack(owner, pointer));
    }
    pthread_mutex_unlock(&owner->lock);
    return next;
}

__attribute__((constructor))
static void tz_boundary_install(void) {
    tsuzuri_trap_hooks = (struct tz_trap_hooks) {
        .owner = tsuzuri_boundary_owner,
        .item = tsuzuri_boundary_item,
        .resume = tsuzuri_boundary_resume,
        .allocate = tsuzuri_tracked_malloc,
        .release = tsuzuri_tracked_free,
    };
}
