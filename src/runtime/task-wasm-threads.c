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

__attribute__((import_module("tsuzuri_threads"), import_name("spawn_workers")))
int32_t spawn_workers(int32_t desired);
__attribute__((import_module("tsuzuri_threads"), import_name("worker_ready")))
void worker_ready(int32_t worker_id);

static void check_failed(void) {
    if (atomic_load_explicit(&state.failed, memory_order_acquire)) __builtin_trap();
}

static void signal_work(void) {
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
        if (!requested || spawn_workers((int32_t)(requested - 1)) != (int32_t)(requested - 1)) __builtin_trap();
        atomic_store_explicit(&state.initialized, 2, memory_order_release);
        signal_work();
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

static int run_one(Group *preferred) {
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
    unlock(&state.queue_mutex);
    signal_work();
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
    unlock(&state.queue_mutex);
    signal_work();
    for (;;) {
        unsigned epoch = atomic_load_explicit(&state.epoch, memory_order_acquire);
        check_failed();
        if (atomic_load_explicit(&group.remaining, memory_order_acquire) == 0) break;
        if (!run_one(&group)) __builtin_wasm_memory_atomic_wait32((int *)&state.epoch, epoch, -1);
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

void tsuzuri_thread_entry(uint32_t worker_id) {
    unsigned desired = atomic_load_explicit(&state.desired, memory_order_acquire);
    if (!worker_id || worker_id >= desired) __builtin_trap();
    atomic_fetch_add_explicit(&state.ready, 1, memory_order_release);
    worker_ready((int32_t)worker_id);
    for (;;) {
        unsigned epoch = atomic_load_explicit(&state.epoch, memory_order_acquire);
        check_failed();
        if (!run_one(0)) __builtin_wasm_memory_atomic_wait32((int *)&state.epoch, epoch, -1);
    }
}

uint32_t tsuzuri_thread_stack_size(void) { return 262144; }
