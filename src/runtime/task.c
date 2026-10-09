#if defined(_WIN32)
#include "task-windows.h"
#define TZ_TASK_API
#else
#include <pthread.h>
#include <unistd.h>
#define TZ_TASK_API __attribute__((weak, visibility("hidden")))
#endif
#include <errno.h>
#include <stdatomic.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#ifndef TZ_TASK_SYSCONF
#define TZ_TASK_SYSCONF sysconf
#endif
#ifndef TZ_TASK_PTHREAD_CREATE
#define TZ_TASK_PTHREAD_CREATE pthread_create
#endif
#ifndef TZ_TASK_PTHREAD_JOIN
#define TZ_TASK_PTHREAD_JOIN pthread_join
#endif
#ifndef TZ_TASK_MUTEX_LOCK
#define TZ_TASK_MUTEX_LOCK pthread_mutex_lock
#endif
#ifndef TZ_TASK_MUTEX_UNLOCK
#define TZ_TASK_MUTEX_UNLOCK pthread_mutex_unlock
#endif
#ifndef TZ_TASK_COND_WAIT
#define TZ_TASK_COND_WAIT pthread_cond_wait
#endif
#ifndef TZ_TASK_COND_BROADCAST
#define TZ_TASK_COND_BROADCAST pthread_cond_broadcast
#endif
#ifndef TZ_TASK_COND_SIGNAL
#define TZ_TASK_COND_SIGNAL pthread_cond_signal
#endif
#ifndef TZ_TASK_ONCE
#define TZ_TASK_ONCE pthread_once
#endif
#ifndef TZ_TASK_ATEXIT
#define TZ_TASK_ATEXIT atexit
#endif

enum { TZ_TASK_MAX_THREADS = 32 };

#if !defined(_WIN32)
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
#endif

#ifdef TZ_STACK_GUARD
/* src/runtime/stack.c, linked into native executables only: covers a worker with the overflow report. */
void tsuzuri_stack_thread(void);
#endif

static atomic_uint tz_task_limit;

struct tz_task_group {
    void (*run)(void *, uint64_t);
    uint32_t (*run_result)(void *, uint64_t);
    void *context;
    uint64_t length;
    uint64_t next;
    _Atomic uint64_t remaining;
    uint64_t failure;
    struct tz_task_group *next_group;
#if !defined(_WIN32)
    /* The trap boundary of the submitting thread (NULL: a trap ends the process), and the
       lowest-index trap of an item that ran under it. */
    void *owner;
    uint64_t trap_index;
    tsuzuri_trap_info trap;
#endif
};

struct tz_waiter;

static struct {
    pthread_mutex_t mutex;
    pthread_cond_t work_available;
    pthread_cond_t work_done;
    pthread_t threads[TZ_TASK_MAX_THREADS - 1];
    unsigned thread_count;
    int stopping;
    struct tz_task_group *groups;
    struct tz_task_group *tail;
    /* Threads that run Tsuzuri code under the pool: a worker with an item, a thread inside a
       submit that is not waiting to join, a thread that was woken from a channel wait. Counted
       under the mutex so that the last one to stop can tell that every task is waiting (F10). */
    uint64_t busy;
    /* The threads that wait on a channel and have not been woken. */
    struct tz_waiter *waiters;
    /* Workers that run no item and are free to take one: those that wait for work and those that
       are starting. A thread that waits on a channel runs an unstarted item itself only when
       there is no such worker, because an item that runs on top of a waiting one can never end
       before the waiting one goes on. */
    unsigned idle;
} tz_task_pool = {
    .mutex = PTHREAD_MUTEX_INITIALIZER,
    .work_available = PTHREAD_COND_INITIALIZER,
    .work_done = PTHREAD_COND_INITIALIZER,
};

/* Whether this thread is one of the `busy` count, and how many items it runs on top of a channel
   wait (a bound that keeps a long chain of waits from using up the stack). */
static _Thread_local int tz_task_counted;
static _Thread_local unsigned tz_task_help_depth;

static pthread_once_t tz_task_once = PTHREAD_ONCE_INIT;

static _Noreturn void tz_task_fail(const char *operation, int error) {
    fprintf(stderr, "Tsuzuri task runtime: %s failed (%d)\n", operation, error);
    abort();
}

static unsigned tz_task_parallelism(void) {
    unsigned limit = atomic_load_explicit(&tz_task_limit, memory_order_relaxed);
    if (limit == 0) {
        long processors = TZ_TASK_SYSCONF(_SC_NPROCESSORS_ONLN);
        limit = processors < 1 ? 1 : processors > TZ_TASK_MAX_THREADS
            ? TZ_TASK_MAX_THREADS : (unsigned)processors;
        atomic_store_explicit(&tz_task_limit, limit, memory_order_relaxed);
    }
    return limit;
}

static void tz_task_check(const char *operation, int error) {
    if (error != 0) tz_task_fail(operation, error);
}

static void tz_task_lock(void) {
    tz_task_check("pthread_mutex_lock", TZ_TASK_MUTEX_LOCK(&tz_task_pool.mutex));
}

static void tz_task_unlock(void) {
    tz_task_check("pthread_mutex_unlock", TZ_TASK_MUTEX_UNLOCK(&tz_task_pool.mutex));
}

static void tz_task_wait(pthread_cond_t *condition) {
    tz_task_check("pthread_cond_wait", TZ_TASK_COND_WAIT(condition, &tz_task_pool.mutex));
}

static void tz_task_wake(pthread_cond_t *condition) {
    tz_task_check("pthread_cond_broadcast", TZ_TASK_COND_BROADCAST(condition));
}

/* The index from which unstarted items are skipped: the lowest failing or trapping item. */
static uint64_t tz_task_stop(const struct tz_task_group *group) {
#if !defined(_WIN32)
    return group->failure < group->trap_index ? group->failure : group->trap_index;
#else
    return group->failure;
#endif
}

static void tz_task_execute(struct tz_task_group *group, uint64_t index) {
    uint32_t failed = 0;
    int trapped = 0;
#if !defined(_WIN32)
    tsuzuri_trap_info trap = { 0, 0 };
    if (group->owner) {
        trapped = tsuzuri_trap_hooks.item(group->owner, group->run, group->run_result, group->context, index, &failed, &trap);
    } else
#endif
    if (group->run_result) failed = group->run_result(group->context, index);
    else group->run(group->context, index);
    if (!group->run_result && !trapped) {
        if (atomic_fetch_sub_explicit(&group->remaining, 1, memory_order_acq_rel) == 1) {
            tz_task_lock();
            tz_task_wake(&tz_task_pool.work_done);
            tz_task_unlock();
        }
        return;
    }
    tz_task_lock();
    uint64_t before = tz_task_stop(group);
    if (failed && index < group->failure) group->failure = index;
#if !defined(_WIN32)
    if (trapped && index < group->trap_index) {
        group->trap_index = index;
        group->trap = trap;
    }
#endif
    uint64_t stop = tz_task_stop(group);
    if (stop < before) {
        uint64_t skipped = group->next < group->length ? group->length - group->next : 0;
        group->length = stop;
        atomic_fetch_sub_explicit(&group->remaining, skipped, memory_order_relaxed);
        tz_task_wake(&tz_task_pool.work_available);
    }
    if (atomic_fetch_sub_explicit(&group->remaining, 1, memory_order_release) == 1) {
        tz_task_wake(&tz_task_pool.work_done);
    }
    tz_task_unlock();
}

/* A thread that waits for a channel to change. It lives on the stack of the waiting thread; the
   thread that changes the channel wakes it (F10 Phase 2). */
struct tz_waiter {
    struct tz_waiter *next;
    pthread_cond_t condition;
    void *channel;
    int sender; /* waits for room (1), or for an item (0) */
    int woken;
    int doomed; /* woken because every task is waiting */
    int counted;
};

/* How many items a thread runs on top of one another while it waits on channels. */
enum { TZ_CHANNEL_HELP_DEPTH = 16 };

/* With the mutex held: the waiter is off the list. A thread that was counted as running is
   counted again from now on, so that no other thread mistakes it for a parked one. */
static void tz_waiter_wake(struct tz_waiter *waiter, int doomed) {
    waiter->woken = 1;
    waiter->doomed = doomed;
    if (waiter->counted) ++tz_task_pool.busy;
    tz_task_check("pthread_cond_signal", TZ_TASK_COND_SIGNAL(&waiter->condition));
}

/* With the mutex held: wakes the first waiter of `channel` for room (`sender`) or for an item, or all. */
static void tz_channel_wake(void *channel, int sender, int all) {
    struct tz_waiter **link = &tz_task_pool.waiters;
    while (*link) {
        struct tz_waiter *waiter = *link;
        if (waiter->channel == channel && waiter->sender == sender) {
            *link = waiter->next;
            tz_waiter_wake(waiter, 0);
            if (!all) return;
        } else {
            link = &waiter->next;
        }
    }
}

/* With the mutex held, after a thread stopped running or started to wait: when no thread runs, no
   worker is free, and no group waits for a join that can end, nothing will ever change a channel
   again, so every waiter is woken to trap. An item that nobody started does not prevent that when
   no worker is free to take it: the threads that could are all waiting. */
static void tz_channel_check_deadlock(void) {
    if (!tz_task_pool.waiters || tz_task_pool.busy != 0) return;
    for (struct tz_task_group *group = tz_task_pool.groups; group; group = group->next_group) {
        if (tz_task_pool.idle != 0 && group->next < group->length) return;
        if (atomic_load_explicit(&group->remaining, memory_order_acquire) == 0) return;
    }
    fprintf(stderr, "Tsuzuri runtime: deadlock: every task is waiting on a channel\n");
    fflush(stderr);
    struct tz_waiter *waiter = tz_task_pool.waiters;
    tz_task_pool.waiters = NULL;
    while (waiter) {
        struct tz_waiter *next = waiter->next;
        tz_waiter_wake(waiter, 1);
        waiter = next;
    }
}

/* With the mutex held: runs one unstarted item of any group on top of this thread's work, as a
   thread that waits for a join does, and returns 1; 0 when there is none or the stack of items is
   deep. The mutex is not held while the item runs. */
static int tz_task_help_one(void) {
    if (tz_task_pool.idle != 0 || tz_task_help_depth >= TZ_CHANNEL_HELP_DEPTH) return 0;
    for (struct tz_task_group *group = tz_task_pool.groups; group; group = group->next_group) {
        if (group->next < group->length) {
            uint64_t index = group->next++;
            ++tz_task_help_depth;
            tz_task_unlock();
            tz_task_execute(group, index);
            tz_task_lock();
            --tz_task_help_depth;
            return 1;
        }
    }
    return 0;
}

static void *tz_task_worker_main(void *pointer) {
    (void)pointer;
#ifdef TZ_STACK_GUARD
    tsuzuri_stack_thread();
#endif
    tz_task_lock();
    while (!tz_task_pool.stopping) {
        struct tz_task_group *group = tz_task_pool.groups;
        while (group && group->next >= group->length) group = group->next_group;
        if (!group) {
            tz_task_wait(&tz_task_pool.work_available);
            continue;
        }
        uint64_t index = group->next++;
        ++tz_task_pool.busy;
        --tz_task_pool.idle;
        tz_task_counted = 1;
        tz_task_unlock();
        tz_task_execute(group, index);
        tz_task_lock();
        tz_task_counted = 0;
        --tz_task_pool.busy;
        ++tz_task_pool.idle;
        tz_channel_check_deadlock();
    }
    tz_task_unlock();
    return NULL;
}

static void tz_task_shutdown(void) {
    tz_task_lock();
    if (tz_task_pool.stopping) { tz_task_unlock(); return; }
    if (tz_task_pool.groups) tz_task_fail("shutdown with active work", EBUSY);
    tz_task_pool.stopping = 1;
    tz_task_wake(&tz_task_pool.work_available);
    unsigned count = tz_task_pool.thread_count;
    tz_task_unlock();
    for (unsigned index = 0; index < count; ++index) {
        tz_task_check("pthread_join", TZ_TASK_PTHREAD_JOIN(tz_task_pool.threads[index], NULL));
    }
    tz_task_pool.thread_count = 0;
}

static void tz_task_start_pool(void) {
    unsigned count = tz_task_parallelism() - 1;
    if (count == 0) return;
    tz_task_check("atexit", TZ_TASK_ATEXIT(tz_task_shutdown));
    for (unsigned index = 0; index < count; ++index) {
        // A worker that is starting is free to take an item; the mutex is taken because other
        // threads already read the count.
        tz_task_lock();
        ++tz_task_pool.idle;
        tz_task_unlock();
        tz_task_check("pthread_create", TZ_TASK_PTHREAD_CREATE(&tz_task_pool.threads[index], NULL, tz_task_worker_main, NULL));
        ++tz_task_pool.thread_count;
    }
}

static uint64_t tz_task_submit(void (*run)(void *, uint64_t), uint32_t (*run_result)(void *, uint64_t), void *context, uint64_t length) {
    if (length == 0) return UINT64_MAX;
    if (length == 1) {
        if (run_result) return run_result(context, 0) ? 0 : UINT64_MAX;
        run(context, 0);
        return UINT64_MAX;
    }
    tz_task_check("pthread_once", TZ_TASK_ONCE(&tz_task_once, tz_task_start_pool));
    if (tz_task_parallelism() == 1) {
        for (uint64_t index = 0; index < length; ++index) {
            if (run_result) { if (run_result(context, index)) return index; }
            else run(context, index);
        }
        return UINT64_MAX;
    }
    struct tz_task_group group = {
        .run = run, .run_result = run_result, .context = context, .length = length,
        .remaining = length, .failure = UINT64_MAX,
    };
#if !defined(_WIN32)
    group.owner = tsuzuri_trap_hooks.owner ? tsuzuri_trap_hooks.owner() : NULL;
    group.trap_index = UINT64_MAX;
    group.trap.site = 0;
    group.trap.kind = 0;
#endif
    tz_task_lock();
    if (tz_task_pool.stopping) tz_task_fail("submit after shutdown", EINVAL);
    // A thread that is not running an item of the pool counts as running while it submits.
    int counted_here = !tz_task_counted;
    if (counted_here) {
        ++tz_task_pool.busy;
        tz_task_counted = 1;
    }
    if (tz_task_pool.tail) tz_task_pool.tail->next_group = &group;
    else tz_task_pool.groups = &group;
    tz_task_pool.tail = &group;
    tz_task_wake(&tz_task_pool.work_available);
    while (atomic_load_explicit(&group.remaining, memory_order_acquire) != 0) {
        if (group.next < group.length) {
            uint64_t index = group.next++;
            tz_task_unlock();
            tz_task_execute(&group, index);
            tz_task_lock();
        } else {
            // Waiting for the join, this thread cannot change a channel.
            --tz_task_pool.busy;
            tz_channel_check_deadlock();
            tz_task_wait(&tz_task_pool.work_done);
            ++tz_task_pool.busy;
        }
    }
    struct tz_task_group **link = &tz_task_pool.groups;
    struct tz_task_group *previous = NULL;
    while (*link != &group) { previous = *link; link = &(*link)->next_group; }
    *link = group.next_group;
    if (tz_task_pool.tail == &group) tz_task_pool.tail = previous;
    if (counted_here) {
        tz_task_counted = 0;
        --tz_task_pool.busy;
        tz_channel_check_deadlock();
    }
    tz_task_unlock();
#if !defined(_WIN32)
    /* A trapped item leaves its task in no defined state, so the group cannot go on: the trap of
       the lowest item that ran is reported even if an earlier item returned Err, as a process
       that ends on a trap would. */
    if (group.trap_index != UINT64_MAX) tsuzuri_trap_hooks.resume(&group.trap);
#endif
    return group.failure;
}

TZ_TASK_API
void tsuzuri_task_parallel(void (*run)(void *, uint64_t), void *context, uint64_t length) {
    (void)tz_task_submit(run, NULL, context, length);
}

TZ_TASK_API
uint64_t tsuzuri_task_parallel_results(uint32_t (*run)(void *, uint64_t), void *context, uint64_t length) {
    return tz_task_submit(NULL, run, context, length);
}

/* Mutex.with_lock (F10). The lock word is the first field of a cell: 0 free, 1 held, 2 held and
   somebody may wait. A critical section never waits (the generated code refuses to nest a lock or
   to start parallel work inside one), so a waiter always gets the lock once its holder is done. All
   mutexes park on one pthread mutex and condition variable, so the lock word of a mutex that
   is never contended is touched by compare-exchange alone. */
static _Thread_local void *tz_mutex_held;
static pthread_mutex_t tz_mutex_park = PTHREAD_MUTEX_INITIALIZER;
static pthread_cond_t tz_mutex_wake = PTHREAD_COND_INITIALIZER;

static void tz_mutex_refuse(const char *message) {
    fprintf(stderr, "Tsuzuri runtime: %s\n", message);
    fflush(stderr);
}

/* 0 when this thread now holds the lock of `cell`; 1 when it holds one already (nothing changed). */
TZ_TASK_API
int32_t tsuzuri_mutex_lock(void *cell) {
    if (tz_mutex_held) {
        tz_mutex_refuse("Mutex.with_lock cannot be nested; release the outer mutex first");
        return 1;
    }
    atomic_uint *state = cell;
    unsigned expected = 0;
    if (!atomic_compare_exchange_strong_explicit(state, &expected, 1,
            memory_order_acquire, memory_order_relaxed)) {
        tz_task_check("pthread_mutex_lock", TZ_TASK_MUTEX_LOCK(&tz_mutex_park));
        /* The exchange and the wait are atomic under the park lock, so the broadcast of the unlock
           that sees 2 cannot come before the wait: no wakeup is lost. */
        while (atomic_exchange_explicit(state, 2, memory_order_acquire) != 0) {
            tz_task_check("pthread_cond_wait", TZ_TASK_COND_WAIT(&tz_mutex_wake, &tz_mutex_park));
        }
        tz_task_check("pthread_mutex_unlock", TZ_TASK_MUTEX_UNLOCK(&tz_mutex_park));
    }
    tz_mutex_held = cell;
    return 0;
}

TZ_TASK_API
void tsuzuri_mutex_unlock(void *cell) {
    tz_mutex_held = NULL;
    atomic_uint *state = cell;
    if (atomic_exchange_explicit(state, 0, memory_order_release) == 2) {
        tz_task_check("pthread_mutex_lock", TZ_TASK_MUTEX_LOCK(&tz_mutex_park));
        tz_task_check("pthread_cond_broadcast", TZ_TASK_COND_BROADCAST(&tz_mutex_wake));
        tz_task_check("pthread_mutex_unlock", TZ_TASK_MUTEX_UNLOCK(&tz_mutex_park));
    }
}

/* Nonzero when parallel work may start: this thread holds no lock. */
TZ_TASK_API
int32_t tsuzuri_mutex_parallel_ok(void) {
    if (!tz_mutex_held) return 1;
    tz_mutex_refuse("parallel work cannot start inside Mutex.with_lock; move it outside the critical section");
    return 0;
}

/* Nonzero when a channel operation may run: it may wait, which a critical section never does. */
TZ_TASK_API
int32_t tsuzuri_mutex_wait_ok(void) {
    if (!tz_mutex_held) return 1;
    tz_mutex_refuse("a Channel operation may wait; move it outside Mutex.with_lock");
    return 0;
}

/* Channel (F10 Phase 2). A channel is one block that the generated code allocates and initializes
   (src/llvm_sync.rs), with the ring of items after the header, which is laid out as below. Every
   operation runs under the pool mutex, which also guards the count of running threads, so that
   "every task is waiting" is decided on one consistent view of the pool and the channels.
   A thread that cannot go on first runs an unstarted item of the pool, as a thread that waits for
   a join does, and parks only when there is none. The thread that changes the channel wakes
   exactly the waiter that can go on: one for an item or for room, all when the channel closes. */
struct tz_channel {
    uint64_t capacity;
    uint64_t item_size;
    uint64_t head;
    uint64_t count;
    uint64_t senders;
    uint64_t receivers;
    uint64_t owners;
    uint64_t reserved;
    uint64_t waiters[2]; /* unused here: the waiters are the pool's */
};
enum { TZ_CHANNEL_ITEMS = 80 };
_Static_assert(sizeof(struct tz_channel) == TZ_CHANNEL_ITEMS, "channel block layout");

static unsigned char *tz_channel_slot(struct tz_channel *channel, uint64_t offset) {
    uint64_t stride = channel->item_size ? channel->item_size : 1;
    return (unsigned char *)channel + TZ_CHANNEL_ITEMS
        + ((channel->head + offset) % channel->capacity) * stride;
}

/* With the mutex held: counts this thread as running for the length of a channel operation if it
   is not counted already. The result says whether `tz_channel_leave` has to undo it. */
static int tz_channel_enter(void) {
    if (tz_task_counted) return 0;
    ++tz_task_pool.busy;
    tz_task_counted = 1;
    return 1;
}

static void tz_channel_leave(int temporary) {
    if (!temporary) return;
    tz_task_counted = 0;
    --tz_task_pool.busy;
    tz_channel_check_deadlock();
}

/* With the mutex held: runs an unstarted item or waits until the channel may have changed.
   0: try the operation again; 2: every task is waiting. */
static int tz_channel_wait(struct tz_channel *channel, int sender) {
    if (tz_task_help_one()) return 0;
    struct tz_waiter waiter;
    waiter.channel = channel;
    waiter.sender = sender;
    waiter.woken = 0;
    waiter.doomed = 0;
    waiter.counted = tz_task_counted;
    tz_task_check("pthread_cond_init", pthread_cond_init(&waiter.condition, NULL));
    waiter.next = tz_task_pool.waiters;
    tz_task_pool.waiters = &waiter;
    if (waiter.counted) --tz_task_pool.busy;
    tz_channel_check_deadlock();
    while (!waiter.woken) tz_task_wait(&waiter.condition);
    tz_task_check("pthread_cond_destroy", pthread_cond_destroy(&waiter.condition));
    return waiter.doomed ? 2 : 0;
}

/* 0: sent; 1: no receiver is left, and the item is still the caller's; 2: deadlock. */
TZ_TASK_API
int32_t tsuzuri_channel_send(void *block, const void *item) {
    struct tz_channel *channel = block;
    tz_task_lock();
    int temporary = tz_channel_enter();
    int32_t status = 0;
    for (;;) {
        if (channel->receivers == 0) { status = 1; break; }
        if (channel->count < channel->capacity) {
            memcpy(tz_channel_slot(channel, channel->count), item, channel->item_size);
            ++channel->count;
            tz_channel_wake(channel, 0, 0);
            break;
        }
        if (tz_channel_wait(channel, 1)) { status = 2; break; }
    }
    tz_channel_leave(temporary);
    tz_task_unlock();
    return status;
}

/* 0: received; 1: empty and no sender is left; 2: deadlock. */
TZ_TASK_API
int32_t tsuzuri_channel_recv(void *block, void *item) {
    struct tz_channel *channel = block;
    tz_task_lock();
    int temporary = tz_channel_enter();
    int32_t status = 0;
    for (;;) {
        if (channel->count > 0) {
            memcpy(item, tz_channel_slot(channel, 0), channel->item_size);
            channel->head = (channel->head + 1) % channel->capacity;
            --channel->count;
            tz_channel_wake(channel, 1, 0);
            break;
        }
        if (channel->senders == 0) { status = 1; break; }
        if (tz_channel_wait(channel, 0)) { status = 2; break; }
    }
    tz_channel_leave(temporary);
    tz_task_unlock();
    return status;
}

TZ_TASK_API
void tsuzuri_channel_clone_sender(void *block) {
    struct tz_channel *channel = block;
    tz_task_lock();
    ++channel->senders;
    ++channel->owners;
    tz_task_unlock();
}

/* Releases a sender (kind 0) or a receiver (kind 1). The last sender closes the channel for the
   receivers that wait for an item; the last receiver makes the waiting senders give up. The
   result is 1 for the owner of the last handle, which drops what is left and frees the block. */
TZ_TASK_API
int32_t tsuzuri_channel_close(void *block, int32_t kind) {
    struct tz_channel *channel = block;
    tz_task_lock();
    if (kind == 0) {
        if (--channel->senders == 0) tz_channel_wake(channel, 0, 1);
    } else if (--channel->receivers == 0) {
        tz_channel_wake(channel, 1, 1);
    }
    int32_t last = --channel->owners == 0;
    tz_task_unlock();
    return last;
}

#if !defined(_WIN32)
/* A trap boundary (src/runtime/trap.c) that catches a trap inside Mutex.with_lock calls this on the
   thread that trapped. The call is abandoned, so the lock is released and the thread forgets it:
   a waiter in the same group gets the lock instead of waiting for ever, and the next call on this
   thread can lock again. */
static void tz_mutex_abandon(void) {
    void *cell = tz_mutex_held;
    if (cell) tsuzuri_mutex_unlock(cell);
}

#ifndef TSUZURI_SYNC_HOOKS_DEFINED
#define TSUZURI_SYNC_HOOKS_DEFINED
struct tz_sync_hooks {
    void (*abandon)(void);
};
__attribute__((weak, visibility("hidden"))) struct tz_sync_hooks tsuzuri_sync_hooks;
#endif

__attribute__((constructor))
static void tz_sync_install(void) {
    tsuzuri_sync_hooks.abandon = tz_mutex_abandon;
}
#endif
