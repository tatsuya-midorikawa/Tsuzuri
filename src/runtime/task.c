#include <pthread.h>
#include <stdatomic.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>

enum { TZ_TASK_MAX_THREADS = 32 };

static atomic_uint tz_task_workers;
static atomic_uint tz_task_limit;

struct tz_task_group {
    void (*run)(void *, uint64_t);
    void *context;
    uint64_t length;
    _Atomic uint64_t next;
};

static _Noreturn void tz_task_fail(const char *operation, int error) {
    fprintf(stderr, "Tsuzuri task runtime: %s failed (%d)\n", operation, error);
    abort();
}

static unsigned tz_task_parallelism(void) {
    unsigned limit = atomic_load_explicit(&tz_task_limit, memory_order_relaxed);
    if (limit == 0) {
        long processors = sysconf(_SC_NPROCESSORS_ONLN);
        limit = processors < 1 ? 1 : processors > TZ_TASK_MAX_THREADS
            ? TZ_TASK_MAX_THREADS : (unsigned)processors;
        atomic_store_explicit(&tz_task_limit, limit, memory_order_relaxed);
    }
    return limit;
}

static void *tz_task_work(void *pointer) {
    struct tz_task_group *group = pointer;
    for (;;) {
        uint64_t index = atomic_fetch_add_explicit(&group->next, 1, memory_order_relaxed);
        if (index >= group->length) {
            return NULL;
        }
        group->run(group->context, index);
    }
}

__attribute__((weak, visibility("hidden")))
void tsuzuri_task_parallel(void (*run)(void *, uint64_t), void *context, uint64_t length) {
    if (length == 0) {
        return;
    }
    if (length == 1) {
        run(context, 0);
        return;
    }
    struct tz_task_group group = { run, context, length, 0 };
    pthread_t threads[TZ_TASK_MAX_THREADS - 1];
    unsigned count = 0;
    unsigned limit = tz_task_parallelism() - 1;
    while (count < TZ_TASK_MAX_THREADS - 1 && (uint64_t)count + 1 < length) {
        unsigned active = atomic_load_explicit(&tz_task_workers, memory_order_relaxed);
        if (active >= limit) {
            break;
        }
        if (!atomic_compare_exchange_weak_explicit(&tz_task_workers, &active, active + 1,
                memory_order_relaxed, memory_order_relaxed)) {
            continue;
        }
        int error = pthread_create(&threads[count], NULL, tz_task_work, &group);
        if (error != 0) {
            tz_task_fail("pthread_create", error);
        }
        ++count;
    }
    /* No queued waits: nested groups can always finish on their calling thread. */
    if (count == 0) {
        for (uint64_t index = 0; index < length; ++index) {
            run(context, index);
        }
    } else {
        tz_task_work(&group);
    }
    for (unsigned index = 0; index < count; ++index) {
        int error = pthread_join(threads[index], NULL);
        if (error != 0) {
            tz_task_fail("pthread_join", error);
        }
        atomic_fetch_sub_explicit(&tz_task_workers, 1, memory_order_relaxed);
    }
}
