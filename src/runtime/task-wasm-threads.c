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
   again, and the waiters are told (`verdicts`). `idle` counts the workers that run no item, as in
   the native pool. `wake` says that the epoch moved and the threads that sleep on it are not yet
   notified. */
typedef struct {
    unsigned blocked;
    unsigned channel_waiters;
    unsigned verdicts;
    unsigned idle;
    unsigned wake;
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

/* Wakes every thread that sleeps on the epoch: idle workers, joins, lock waiters and channel waiters.
   The host's `fail` bumps the same word, so each of them also wakes to trap when the pool failed. */
static void notify_all(void) {
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

/* With `queue_mutex` held: something that a waiting thread waits for may have happened. Every
   registration of a blocked thread is void from here on, and a thread counts itself blocked again
   only after it saw the epoch move, so no thread is counted twice in one window. The epoch moves
   now, so that a thread that is about to sleep sees it; the sleepers are notified when the lock is
   released (`unlock_queue`), so that they do not wake to wait for a lock that this thread holds.
   Only an event that a sleeper waits for calls this: new work, the end of a group, a change of a
   channel and the release of a contended lock. An item that is done while others of its group still
   run wakes nobody, because every thread that is asleep waits for one of those events. */
static void progress(void) {
    waiting.blocked = 0;
    waiting.channel_waiters = 0;
    waiting.wake = 1;
    atomic_fetch_add_explicit(&state.epoch, 1, memory_order_release);
}

/* Releases `queue_mutex`, then notifies the sleepers if the critical section moved the epoch. */
static void unlock_queue(void) {
    unsigned wake = waiting.wake;
    waiting.wake = 0;
    unlock(&state.queue_mutex);
    if (wake) notify_all();
}

/* Sleeps on the epoch that the thread registered as blocked at, until it moves. A notify that comes
   for another reason, or late, finds the thread still registered and sends it back to sleep. The
   host's `fail` stores `failed` before it moves the epoch, so a thread that read the epoch after
   that move finds the flag at the check before the wait, and one that read an older epoch is not
   put to sleep by the wait: without the check, the first would wait on a word that never moves again. */
static void sleep_until_progress(unsigned epoch) {
    do {
        check_failed();
        __builtin_wasm_memory_atomic_wait32((int *)&state.epoch, (int)epoch, -1);
    } while (atomic_load_explicit(&state.epoch, memory_order_acquire) == epoch);
    check_failed();
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
        unlock_queue();
        if (spawn_workers((int32_t)(requested - 1)) != (int32_t)(requested - 1)) __builtin_trap();
        atomic_store_explicit(&state.initialized, 2, memory_order_release);
        lock(&state.queue_mutex);
        progress();
        unlock_queue();
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
    struct Group *next_group;
} Group;

/* The groups that have not been joined, in the order they were submitted (the oldest first, as in
   the native pool), guarded by `queue_mutex`. */
static Group *groups;
static Group *groups_tail;

/* With `queue_mutex` held: whether `group` has an item that no thread has started. */
static int has_unstarted(Group *group) {
    return atomic_load_explicit(&group->next, memory_order_relaxed) < group->length;
}

/* With `queue_mutex` held: the oldest group that has an item that no thread has started. */
static Group *unstarted_group(void) {
    Group *group = groups;
    while (group && !has_unstarted(group)) group = group->next_group;
    return group;
}

/* With `queue_mutex` held: takes the next item of a group that has one to start. */
static uint64_t claim(Group *group) {
    return atomic_fetch_add_explicit(&group->next, 1, memory_order_relaxed);
}

/* Runs item `index` of `group`, which the calling thread took, and records its end. Called without
   `queue_mutex`; returns with it held. A `worker` is idle again afterwards. */
static void run_item(Group *group, uint64_t index, int worker) {
    uint32_t failed = 0;
    if (group->run_result) failed = group->run_result(group->context, index);
    else group->run(group->context, index);
    lock(&state.queue_mutex);
    if (failed && index < group->failure) {
        group->failure = index;
        uint64_t next = atomic_load_explicit(&group->next, memory_order_relaxed);
        uint64_t skipped = next < group->length ? group->length - next : 0;
        group->length = index;
        atomic_fetch_sub_explicit(&group->remaining, skipped, memory_order_relaxed);
    }
    atomic_fetch_sub_explicit(&group->remaining, 1, memory_order_release);
    // The group is the joiner's, which cannot unlink it before this lock is released.
    int ended = atomic_load_explicit(&group->remaining, memory_order_relaxed) == 0;
    if (worker) ++waiting.idle;
    if (ended) progress();
}

/* Runs one unstarted item: of the group `own` when it is not null, otherwise of the oldest group
   that has one. A thread that waits for the end of its group (`own`) runs that group's items and
   no others: an item of another group would run on top of the frame that waits for the join, and
   could wait for something that only the code after the join, lower on the same stack, produces.
   A `worker` that runs an item is not idle meanwhile.
   Returns 1 when an item ran. When none is left to start, the result is 0 and nothing ran; if
   `epoch` is not null, the caller is about to sleep (an idle worker, or a thread that waits for the
   group `own`): it is counted as blocked under the lock that found nothing to start, so that no
   item can appear between the two, and `*epoch` is the epoch to sleep on (`sleep_until_progress`).
   The result is -1 instead when `own` has already ended, and the caller needs no sleep. */
static int run_one(Group *own, int worker, unsigned *epoch) {
    lock(&state.queue_mutex);
    Group *group = own ? (has_unstarted(own) ? own : 0) : unstarted_group();
    if (!group) {
        int result = 0;
        if (epoch) {
            if (own && atomic_load_explicit(&own->remaining, memory_order_acquire) == 0) {
                result = -1;
            } else {
                *epoch = atomic_load_explicit(&state.epoch, memory_order_acquire);
                register_blocked(0);
            }
        }
        unlock_queue();
        return result;
    }
    uint64_t index = claim(group);
    if (worker) --waiting.idle;
    unlock_queue();
    run_item(group, index, worker);
    unlock_queue();
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
    group.next_group = 0;
    atomic_init(&group.next, 0);
    atomic_init(&group.remaining, length);
    lock(&state.queue_mutex);
    if (groups_tail) groups_tail->next_group = &group;
    else groups = &group;
    groups_tail = &group;
    // The first item is this thread's, taken under the lock that publishes the group, as in the
    // native pool: no worker can take every item before the thread that waits for the group has
    // started one.
    uint64_t first = claim(&group);
    progress();
    unlock_queue();
    run_item(&group, first, 0);
    unlock_queue();
    for (;;) {
        check_failed();
        if (atomic_load_explicit(&group.remaining, memory_order_acquire) == 0) break;
        // With nothing of its own group to start, the thread waits for the join, which cannot change
        // a channel.
        unsigned epoch;
        int ran = run_one(&group, 0, &epoch);
        if (ran < 0) break;
        if (!ran) sleep_until_progress(epoch);
    }
    lock(&state.queue_mutex);
    Group **link = &groups;
    Group *before = 0;
    while (*link != &group) {
        before = *link;
        link = &before->next_group;
    }
    *link = group.next_group;
    if (groups_tail == &group) groups_tail = before;
    unlock_queue();
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
        unlock_queue();
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

/* With `queue_mutex` held: runs an unstarted item, of the oldest group that has one, on top of this
   thread's work, unless a worker is free to take it (an item that runs on top of a waiting one can
   never end before the waiting one goes on, so a pipeline that needs every stage to run at once
   would stall if the third stage were stacked on the second while a worker was free to run it).
   Returns 1 when the lock was released while the item ran, so the caller has to look at its
   channel again; the lock is held again when it returns. */
static int help_one(void) {
    if (waiting.idle != 0 || help_depth >= HELP_DEPTH) return 0;
    Group *group = unstarted_group();
    if (!group) return 0;
    uint64_t index = claim(group);
    ++help_depth;
    unlock_queue();
    run_item(group, index, 0);
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
    unlock_queue();
    sleep_until_progress(epoch);
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
    unlock_queue();
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
    unlock_queue();
    return status;
}

void tsuzuri_channel_clone_sender(void *block) {
    Channel *channel = block;
    lock(&state.queue_mutex);
    ++channel->senders;
    ++channel->owners;
    unlock_queue();
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
    unlock_queue();
    return last;
}

void tsuzuri_thread_entry(uint32_t worker_id) {
    unsigned desired = atomic_load_explicit(&state.desired, memory_order_acquire);
    if (!worker_id || worker_id >= desired) __builtin_trap();
    atomic_fetch_add_explicit(&state.ready, 1, memory_order_release);
    worker_ready((int32_t)worker_id);
    for (;;) {
        check_failed();
        unsigned epoch;
        if (!run_one(0, 1, &epoch)) sleep_until_progress(epoch);
    }
}

uint32_t tsuzuri_thread_stack_size(void) { return 262144; }
