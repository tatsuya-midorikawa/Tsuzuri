#include <stdint.h>
#include <stdatomic.h>

typedef struct {
    atomic_uint failed;
    atomic_uint epoch;
    atomic_uint heap_mutex;
    atomic_uint queue_mutex;
    atomic_uint initialized;
    atomic_uint desired;
    atomic_uint ready;
} ThreadState;

static ThreadState state;
_Static_assert(sizeof(ThreadState) == 28, "host control layout");

/* What the threads that wait on the epoch have in common (F10): everything here is guarded by
   `queue_mutex`, and the host never reads it. `blocked` counts the threads that found nothing to do
   since the last progress event, `channel_waiters` those of them that wait on a channel. When all
   threads are blocked and one of them waits on a channel, no thread can ever change a channel
   again, and the waiters are told (`verdicts`). `idle` counts the workers that run no item. */
typedef struct {
    unsigned blocked;
    unsigned channel_waiters;
    unsigned verdicts;
    unsigned idle;
} Waiting;

static Waiting waiting;

/* Every thread runs its own instance of the module, so a WebAssembly global is a thread-local
   variable that needs no TLS block: the lock that the thread holds, and how many items it runs on
   top of one another while it waits on channels. */
__attribute__((address_space(1))) static uint32_t mutex_held;
__attribute__((address_space(1))) static uint32_t help_depth;

__attribute__((import_module("tsuzuri_threads"), import_name("spawn_workers")))
int32_t spawn_workers(int32_t desired);
__attribute__((import_module("tsuzuri_threads"), import_name("worker_ready")))
void worker_ready(int32_t worker_id);

static void check_failed(void) {
    if (atomic_load_explicit(&state.failed, memory_order_acquire)) __builtin_trap();
}

/* Wakes every thread that waits on the epoch: idle workers, joins, lock waiters and channel waiters.
   The host's `fail` bumps the same word, so each of them also wakes to trap when the pool failed. */
static void wake_all(void) {
    atomic_fetch_add_explicit(&state.epoch, 1, memory_order_release);
    __builtin_wasm_memory_atomic_notify((int *)&state.epoch, UINT32_MAX);
}

static void lock(atomic_uint *mutex) {
    for (;;) {
        check_failed();
        unsigned expected = 0;
        if (atomic_compare_exchange_weak_explicit(mutex, &expected, 1,
                memory_order_acquire, memory_order_relaxed)) return;
        __builtin_wasm_memory_atomic_wait32((int *)mutex, 1, -1);
    }
}

static void unlock(atomic_uint *mutex) {
    atomic_fetch_and_explicit(mutex, 0x80000000u, memory_order_release);
    __builtin_wasm_memory_atomic_notify((int *)mutex, UINT32_MAX);
}

/* With `queue_mutex` held: something that a waiting thread waits for may have happened, so every
   waiter wakes, looks again and counts itself blocked again if it still cannot go on. */
static void progress(void) {
    waiting.blocked = 0;
    waiting.channel_waiters = 0;
    wake_all();
}

/* The threads that can change a channel: all of them once the pool runs, the one before. */
static unsigned thread_count(void) {
    return atomic_load_explicit(&state.initialized, memory_order_acquire) == 2
        ? atomic_load_explicit(&state.desired, memory_order_relaxed) : 1;
}

/* With `queue_mutex` held: this thread is about to wait on the epoch with nothing to run. The last
   thread to block, when a channel waiter is among them, finds the deadlock. */
static void register_blocked(int channel) {
    ++waiting.blocked;
    if (channel) ++waiting.channel_waiters;
    if (waiting.channel_waiters && waiting.blocked == thread_count()) {
        ++waiting.verdicts;
        progress();
    }
}

void tsuzuri_threads_heap_lock(void) { lock(&state.heap_mutex); }
void tsuzuri_threads_heap_unlock(void) { unlock(&state.heap_mutex); }

uintptr_t tsuzuri_threads_control(void) { return (uintptr_t)&state; }

void tsuzuri_threads_init(uint32_t workers) {
    unsigned expected = 0;
    if (workers > 31 || !atomic_compare_exchange_strong_explicit(&state.desired,
            &expected, workers + 1, memory_order_release, memory_order_relaxed)) __builtin_trap();
}

static void start_pool(void) {
    unsigned expected = 0;
    if (atomic_compare_exchange_strong_explicit(&state.initialized, &expected, 1,
            memory_order_acq_rel, memory_order_acquire)) {
        unsigned requested = atomic_load_explicit(&state.desired, memory_order_acquire);
        if (!requested) __builtin_trap();
        // A worker that is starting is free to take an item.
        lock(&state.queue_mutex);
        waiting.idle = requested - 1;
        unlock(&state.queue_mutex);
        if (spawn_workers((int32_t)(requested - 1)) != (int32_t)(requested - 1)) __builtin_trap();
        atomic_store_explicit(&state.initialized, 2, memory_order_release);
        lock(&state.queue_mutex);
        progress();
        unlock(&state.queue_mutex);
    } else {
        for (;;) {
            unsigned epoch = atomic_load_explicit(&state.epoch, memory_order_acquire);
            check_failed();
            if (atomic_load_explicit(&state.initialized, memory_order_acquire) == 2) return;
            __builtin_wasm_memory_atomic_wait32((int *)&state.epoch, epoch, -1);
        }
    }
}

typedef struct Group {
    void (*run)(void *, uint64_t);
    uint32_t (*run_result)(void *, uint64_t);
    void *context;
    uint64_t length;
    atomic_uint_least64_t next;
    atomic_uint_least64_t remaining;
    uint64_t failure;
    struct Group *previous;
} Group;

static Group *groups;

/* With `queue_mutex` held: whether some group has an item that no thread has started. */
static int has_unstarted(void) {
    for (Group *group = groups; group; group = group->previous) {
        if (atomic_load_explicit(&group->next, memory_order_relaxed) < group->length) return 1;
    }
    return 0;
}

/* Runs one unstarted item, of `preferred` if it has one. A worker that runs an item is not idle. */
static int run_one(Group *preferred, int worker) {
    lock(&state.queue_mutex);
    Group *group = preferred;
    if (!group || atomic_load_explicit(&group->next, memory_order_relaxed) >= group->length) {
        group = groups;
        while (group && atomic_load_explicit(&group->next, memory_order_relaxed) >= group->length)
            group = group->previous;
    }
    if (!group) {
        unlock(&state.queue_mutex);
        return 0;
    }
    uint64_t index = atomic_fetch_add_explicit(&group->next, 1, memory_order_relaxed);
    if (worker) --waiting.idle;
    void (*run)(void *, uint64_t) = group->run;
    void *context = group->context;
    uint32_t (*run_result)(void *, uint64_t) = group->run_result;
    unlock(&state.queue_mutex);
    uint32_t failed = 0;
    if (run_result) failed = run_result(context, index);
    else run(context, index);
    lock(&state.queue_mutex);
    if (failed && index < group->failure) {
        group->failure = index;
        uint64_t next = atomic_load_explicit(&group->next, memory_order_relaxed);
        uint64_t skipped = next < group->length ? group->length - next : 0;
        group->length = index;
        atomic_fetch_sub_explicit(&group->remaining, skipped, memory_order_relaxed);
    }
    atomic_fetch_sub_explicit(&group->remaining, 1, memory_order_release);
    if (worker) ++waiting.idle;
    progress();
    unlock(&state.queue_mutex);
    return 1;
}

static uint64_t submit(void (*run)(void *, uint64_t), uint32_t (*run_result)(void *, uint64_t), void *context, uint64_t length) {
    check_failed();
    start_pool();
    if (!length) return UINT64_MAX;
    Group group;
    group.run = run;
    group.run_result = run_result;
    group.context = context;
    group.length = length;
    group.failure = UINT64_MAX;
    atomic_init(&group.next, 0);
    atomic_init(&group.remaining, length);
    lock(&state.queue_mutex);
    group.previous = groups;
    groups = &group;
    progress();
    unlock(&state.queue_mutex);
    for (;;) {
        check_failed();
        if (atomic_load_explicit(&group.remaining, memory_order_acquire) == 0) break;
        if (run_one(&group, 0)) continue;
        // Nothing to run: the thread waits for the join, which cannot change a channel.
        lock(&state.queue_mutex);
        if (atomic_load_explicit(&group.remaining, memory_order_acquire) == 0 || has_unstarted()) {
            unlock(&state.queue_mutex);
            continue;
        }
        unsigned epoch = atomic_load_explicit(&state.epoch, memory_order_acquire);
        register_blocked(0);
        unlock(&state.queue_mutex);
        __builtin_wasm_memory_atomic_wait32((int *)&state.epoch, epoch, -1);
    }
    lock(&state.queue_mutex);
    Group **link = &groups;
    while (*link != &group) link = &(*link)->previous;
    *link = group.previous;
    unlock(&state.queue_mutex);
    return group.failure;
}

void tsuzuri_task_parallel(void (*run)(void *, uint64_t), void *context, uint64_t length) {
    (void)submit(run, 0, context, length);
}

uint64_t tsuzuri_task_parallel_results(uint32_t (*run)(void *, uint64_t), void *context, uint64_t length) {
    return submit(0, run, context, length);
}

/* Mutex.with_lock (F10). The lock word is the first field of a cell: 0 free, 1 held, 2 held and
   somebody may wait. A critical section never waits (the generated code refuses to nest a lock, to
   start parallel work or to run a channel operation inside one), so a waiter always gets the lock
   once its holder is done. A waiter waits on the epoch, as every other thread does, so the host's
   failure wakes it; the unlock that finds a waiter bumps the epoch after the waiter set the word
   to 2, so the waiter either sees the new epoch or is woken by it. */
int32_t tsuzuri_mutex_lock(void *cell) {
    if (mutex_held) return 1;
    atomic_uint *word = cell;
    unsigned expected = 0;
    if (!atomic_compare_exchange_strong_explicit(word, &expected, 1,
            memory_order_acquire, memory_order_relaxed)) {
        for (;;) {
            unsigned epoch = atomic_load_explicit(&state.epoch, memory_order_acquire);
            check_failed();
            if (atomic_exchange_explicit(word, 2, memory_order_acquire) == 0) break;
            __builtin_wasm_memory_atomic_wait32((int *)&state.epoch, epoch, -1);
        }
    }
    mutex_held = (uint32_t)(uintptr_t)cell;
    return 0;
}

void tsuzuri_mutex_unlock(void *cell) {
    mutex_held = 0;
    atomic_uint *word = cell;
    if (atomic_exchange_explicit(word, 0, memory_order_release) == 2) {
        lock(&state.queue_mutex);
        progress();
        unlock(&state.queue_mutex);
    }
}

/* Nonzero when parallel work may start, or a channel operation may run: this thread holds no lock.
   There is no console to explain a refusal, so the caller traps. */
int32_t tsuzuri_mutex_parallel_ok(void) { return mutex_held == 0; }
int32_t tsuzuri_mutex_wait_ok(void) { return mutex_held == 0; }

/* Channel (F10 Phase 2): the block that the generated code allocates (src/llvm_sync.rs), as in the
   native runtime (src/runtime/task.c). Every operation runs under `queue_mutex`, which also guards
   the counts of `waiting`, so that "every task is waiting" is decided on one consistent view. */
typedef struct {
    uint64_t capacity;
    uint64_t item_size;
    uint64_t head;
    uint64_t count;
    uint64_t senders;
    uint64_t receivers;
    uint64_t owners;
    uint64_t reserved;
    uint64_t waiters[2];
} Channel;
enum { CHANNEL_ITEMS = 80 };
_Static_assert(sizeof(Channel) == CHANNEL_ITEMS, "channel block layout");

static unsigned char *channel_slot(Channel *channel, uint64_t offset) {
    uint64_t stride = channel->item_size ? channel->item_size : 1;
    return (unsigned char *)channel + CHANNEL_ITEMS + ((channel->head + offset) % channel->capacity) * stride;
}

/* With `queue_mutex` held: the state of a channel changed, which matters to the threads that wait on one. */
static void channel_changed(void) {
    if (waiting.channel_waiters) progress();
}

/* How many items a thread runs on top of one another while it waits on channels, which bounds the stack. */
enum { HELP_DEPTH = 16 };

/* With `queue_mutex` held: runs an unstarted item on top of this thread's work, as a thread that
   waits for a join does, unless a worker is free to take it (an item that runs on top of a waiting
   one can never end before the waiting one goes on). Returns 1 when the lock was released, so the
   caller has to look at its channel again. */
static int help_one(void) {
    if (waiting.idle != 0 || help_depth >= HELP_DEPTH || !has_unstarted()) return 0;
    ++help_depth;
    unlock(&state.queue_mutex);
    (void)run_one(0, 0);
    lock(&state.queue_mutex);
    --help_depth;
    return 1;
}

/* With `queue_mutex` held: helps or waits until something changed. 0: try again; 2: deadlock. */
static int channel_wait(void) {
    if (help_one()) return 0;
    unsigned epoch = atomic_load_explicit(&state.epoch, memory_order_acquire);
    unsigned verdicts = waiting.verdicts;
    register_blocked(1);
    if (waiting.verdicts != verdicts) return 2;
    unlock(&state.queue_mutex);
    __builtin_wasm_memory_atomic_wait32((int *)&state.epoch, epoch, -1);
    lock(&state.queue_mutex);
    return waiting.verdicts != verdicts ? 2 : 0;
}

/* 0: sent; 1: no receiver is left, and the item is still the caller's; 2: deadlock. */
int32_t tsuzuri_channel_send(void *block, const void *item) {
    Channel *channel = block;
    int32_t status = 0;
    lock(&state.queue_mutex);
    for (;;) {
        if (channel->receivers == 0) { status = 1; break; }
        if (channel->count < channel->capacity) {
            __builtin_memcpy(channel_slot(channel, channel->count), item, channel->item_size);
            ++channel->count;
            channel_changed();
            break;
        }
        if (channel_wait()) { status = 2; break; }
    }
    unlock(&state.queue_mutex);
    return status;
}

/* 0: received; 1: empty and no sender is left; 2: deadlock. */
int32_t tsuzuri_channel_recv(void *block, void *item) {
    Channel *channel = block;
    int32_t status = 0;
    lock(&state.queue_mutex);
    for (;;) {
        if (channel->count > 0) {
            __builtin_memcpy(item, channel_slot(channel, 0), channel->item_size);
            channel->head = (channel->head + 1) % channel->capacity;
            --channel->count;
            channel_changed();
            break;
        }
        if (channel->senders == 0) { status = 1; break; }
        if (channel_wait()) { status = 2; break; }
    }
    unlock(&state.queue_mutex);
    return status;
}

void tsuzuri_channel_clone_sender(void *block) {
    Channel *channel = block;
    lock(&state.queue_mutex);
    ++channel->senders;
    ++channel->owners;
    unlock(&state.queue_mutex);
}

/* Releases a sender (kind 0) or a receiver (kind 1). The result is 1 for the owner of the last
   handle, which drops what is left and frees the block. */
int32_t tsuzuri_channel_close(void *block, int32_t kind) {
    Channel *channel = block;
    lock(&state.queue_mutex);
    if (kind == 0) {
        if (--channel->senders == 0) channel_changed();
    } else if (--channel->receivers == 0) {
        channel_changed();
    }
    int32_t last = --channel->owners == 0;
    unlock(&state.queue_mutex);
    return last;
}

void tsuzuri_thread_entry(uint32_t worker_id) {
    unsigned desired = atomic_load_explicit(&state.desired, memory_order_acquire);
    if (!worker_id || worker_id >= desired) __builtin_trap();
    atomic_fetch_add_explicit(&state.ready, 1, memory_order_release);
    worker_ready((int32_t)worker_id);
    for (;;) {
        check_failed();
        if (run_one(0, 1)) continue;
        lock(&state.queue_mutex);
        if (has_unstarted()) {
            unlock(&state.queue_mutex);
            continue;
        }
        unsigned epoch = atomic_load_explicit(&state.epoch, memory_order_acquire);
        register_blocked(0);
        unlock(&state.queue_mutex);
        __builtin_wasm_memory_atomic_wait32((int *)&state.epoch, epoch, -1);
    }
}

uint32_t tsuzuri_thread_stack_size(void) { return 262144; }
