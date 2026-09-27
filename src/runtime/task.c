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
#ifndef TZ_TASK_ONCE
#define TZ_TASK_ONCE pthread_once
#endif
#ifndef TZ_TASK_ATEXIT
#define TZ_TASK_ATEXIT atexit
#endif

enum { TZ_TASK_MAX_THREADS = 32 };

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
};

static struct {
    pthread_mutex_t mutex;
    pthread_cond_t work_available;
    pthread_cond_t work_done;
    pthread_t threads[TZ_TASK_MAX_THREADS - 1];
    unsigned thread_count;
    int stopping;
    struct tz_task_group *groups;
    struct tz_task_group *tail;
} tz_task_pool = {
    .mutex = PTHREAD_MUTEX_INITIALIZER,
    .work_available = PTHREAD_COND_INITIALIZER,
    .work_done = PTHREAD_COND_INITIALIZER,
};

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

static void tz_task_execute(struct tz_task_group *group, uint64_t index) {
    if (!group->run_result) {
        group->run(group->context, index);
        if (atomic_fetch_sub_explicit(&group->remaining, 1, memory_order_acq_rel) == 1) {
            tz_task_lock();
            tz_task_wake(&tz_task_pool.work_done);
            tz_task_unlock();
        }
        return;
    }
    uint32_t failed = group->run_result(group->context, index);
    tz_task_lock();
    if (failed && index < group->failure) {
        group->failure = index;
        uint64_t skipped = group->next < group->length ? group->length - group->next : 0;
        group->length = index;
        atomic_fetch_sub_explicit(&group->remaining, skipped, memory_order_relaxed);
        tz_task_wake(&tz_task_pool.work_available);
    }
    if (atomic_fetch_sub_explicit(&group->remaining, 1, memory_order_release) == 1) {
        tz_task_wake(&tz_task_pool.work_done);
    }
    tz_task_unlock();
}

static void *tz_task_worker_main(void *pointer) {
    (void)pointer;
    tz_task_lock();
    while (!tz_task_pool.stopping) {
        struct tz_task_group *group = tz_task_pool.groups;
        while (group && group->next >= group->length) group = group->next_group;
        if (!group) {
            tz_task_wait(&tz_task_pool.work_available);
            continue;
        }
        uint64_t index = group->next++;
        tz_task_unlock();
        tz_task_execute(group, index);
        tz_task_lock();
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
    struct tz_task_group group = { run, run_result, context, length, 0, length, UINT64_MAX, NULL };
    tz_task_lock();
    if (tz_task_pool.stopping) tz_task_fail("submit after shutdown", EINVAL);
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
            tz_task_wait(&tz_task_pool.work_done);
        }
    }
    struct tz_task_group **link = &tz_task_pool.groups;
    struct tz_task_group *previous = NULL;
    while (*link != &group) { previous = *link; link = &(*link)->next_group; }
    *link = group.next_group;
    if (tz_task_pool.tail == &group) tz_task_pool.tail = previous;
    tz_task_unlock();
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
