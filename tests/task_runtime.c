#include <assert.h>
#include <errno.h>
#if defined(_WIN32)
#include "../src/runtime/task-windows.h"
#else
#include <pthread.h>
#include <unistd.h>
#endif
#include <stdatomic.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

static long processors = 4;
static atomic_uint created, joined, outstanding, peak;
static atomic_uint parked, broadcasts;
static int fail_create, fail_join;
static const char *failure = "";
static pthread_mutex_t observation = PTHREAD_MUTEX_INITIALIZER;
static pthread_cond_t observed = PTHREAD_COND_INITIALIZER;
static pthread_cond_t *worker_condition;

static long test_sysconf(int name) {
    assert(name == _SC_NPROCESSORS_ONLN);
    return processors;
}

static int test_create(pthread_t *thread, const pthread_attr_t *attributes,
                       void *(*run)(void *), void *context) {
    if (fail_create) {
        return EAGAIN;
    }
    unsigned active = atomic_fetch_add(&outstanding, 1) + 1;
    unsigned before = atomic_load(&peak);
    while (before < active && !atomic_compare_exchange_weak(&peak, &before, active)) {}
    atomic_fetch_add(&created, 1);
    return pthread_create(thread, attributes, run, context);
}

static int test_join(pthread_t thread, void **result) {
    int error = pthread_join(thread, result);
    atomic_fetch_sub(&outstanding, 1);
    atomic_fetch_add(&joined, 1);
    return fail_join ? EINVAL : error;
}

static int test_lock(pthread_mutex_t *mutex) {
    return strcmp(failure, "mutex_lock") == 0 ? EINVAL : pthread_mutex_lock(mutex);
}

static int test_unlock(pthread_mutex_t *mutex) {
    return strcmp(failure, "mutex_unlock") == 0 ? EINVAL : pthread_mutex_unlock(mutex);
}

static int test_wait(pthread_cond_t *condition, pthread_mutex_t *mutex) {
    if (strcmp(failure, "cond_wait") == 0) return EINVAL;
    if (condition == worker_condition) {
        assert(pthread_mutex_lock(&observation) == 0);
        atomic_fetch_add(&parked, 1);
        assert(pthread_cond_broadcast(&observed) == 0);
        assert(pthread_mutex_unlock(&observation) == 0);
    }
    return pthread_cond_wait(condition, mutex);
}

static int test_broadcast(pthread_cond_t *condition) {
    atomic_fetch_add(&broadcasts, 1);
    return strcmp(failure, "cond_broadcast") == 0 ? EINVAL : pthread_cond_broadcast(condition);
}

static int test_once(pthread_once_t *once, void (*initialize)(void)) {
    return strcmp(failure, "once") == 0 ? EINVAL : pthread_once(once, initialize);
}

static int test_atexit(void (*callback)(void)) {
    return strcmp(failure, "atexit") == 0 ? 1 : atexit(callback);
}

#define TZ_TASK_SYSCONF test_sysconf
#define TZ_TASK_PTHREAD_CREATE test_create
#define TZ_TASK_PTHREAD_JOIN test_join
#define TZ_TASK_MUTEX_LOCK test_lock
#define TZ_TASK_MUTEX_UNLOCK test_unlock
#define TZ_TASK_COND_WAIT test_wait
#define TZ_TASK_COND_BROADCAST test_broadcast
#define TZ_TASK_ONCE test_once
#define TZ_TASK_ATEXIT test_atexit
#include "../src/runtime/task.c"

enum { ITEMS = 257 };
static atomic_uint hits[ITEMS];
static uint64_t published[ITEMS];
static pthread_mutex_t mutex = PTHREAD_MUTEX_INITIALIZER;
static pthread_cond_t ready = PTHREAD_COND_INITIALIZER;
static unsigned arrivals, required;

static void visit(void *context, uint64_t index) {
    assert(context == hits && index < ITEMS);
    atomic_fetch_add(&hits[index], 1);
}

static void publish(void *context, uint64_t index) {
    uint64_t *values = context;
    values[index] = (index + 1) * 37;
}

static void rendezvous(void *context, uint64_t index) {
    if (index < required) {
        assert(pthread_mutex_lock(&mutex) == 0);
        ++arrivals;
        if (arrivals == required) {
            assert(pthread_cond_broadcast(&ready) == 0);
        }
        while (arrivals < required) {
            assert(pthread_cond_wait(&ready, &mutex) == 0);
        }
        assert(pthread_mutex_unlock(&mutex) == 0);
    }
    visit(context, index);
}

static void nested(void *context, uint64_t index) {
    (void)index;
    tsuzuri_task_parallel(visit, context, ITEMS);
}

static void *host_group(void *context) {
    tsuzuri_task_parallel(visit, context, ITEMS);
    return NULL;
}

static void reset_hits(void) {
    for (unsigned index = 0; index < ITEMS; ++index) {
        atomic_store(&hits[index], 0);
    }
}

static void check_hits(unsigned expected) {
    for (unsigned index = 0; index < ITEMS; ++index) {
        assert(atomic_load(&hits[index]) == expected);
    }
    assert(tz_task_pool.groups == NULL && tz_task_pool.tail == NULL);
}

static uint32_t result_visit(void *context, uint64_t index) {
    visit(context, index);
    return 0;
}

static uint32_t result_failure(void *context, uint64_t index) {
    visit(context, index);
    if (required > 1 && index == 2) {
        tz_task_lock();
        while (tz_task_pool.groups->failure == UINT64_MAX)
            tz_task_wait(&tz_task_pool.work_available);
        tz_task_unlock();
    }
    if (index == 7) {
        return 1;
    }
    return index == 2;
}

static uint32_t result_stop(void *context, uint64_t index) {
    visit(context, index);
    return index == 3;
}

int main(int argc, char **argv) {
    worker_condition = &tz_task_pool.work_available;
    if (argc == 2 && (argv[1][0] < '0' || argv[1][0] > '9') && argv[1][0] != '-') {
        failure = argv[1];
        fail_create = strcmp(argv[1], "create") == 0;
        fail_join = strcmp(argv[1], "join") == 0;
        if (strcmp(failure, "cond_wait") == 0) { tz_task_lock(); tz_task_wait(&tz_task_pool.work_done); }
        tsuzuri_task_parallel(visit, hits, ITEMS);
        tz_task_shutdown();
        assert(!"thread failure must terminate with a diagnostic");
    }
    if (argc == 2) processors = strtol(argv[1], NULL, 10);
    required = processors < 1 ? 1 : processors > TZ_TASK_MAX_THREADS ? TZ_TASK_MAX_THREADS : (unsigned)processors;
    tsuzuri_task_parallel(NULL, NULL, 0);
    tsuzuri_task_parallel(visit, hits, 1);
    assert(atomic_load(&created) == 0 && atomic_load(&hits[0]) == 1);

    reset_hits();
    tsuzuri_task_parallel(rendezvous, hits, ITEMS);
    assert(arrivals == required && atomic_load(&peak) == required - 1);
    check_hits(1);

    reset_hits();
    for (unsigned index = 0; index < 1000; ++index) tsuzuri_task_parallel(visit, hits, ITEMS);
    check_hits(1000);
    assert(atomic_load(&created) == required - 1 && atomic_load(&joined) == 0);
    tsuzuri_task_parallel(publish, published, ITEMS);
    for (unsigned index = 0; index < ITEMS; ++index) assert(published[index] == (index + 1) * 37);

    reset_hits();
    tsuzuri_task_parallel(nested, hits, 32);
    assert(atomic_load(&peak) == required - 1);
    check_hits(32);

    reset_hits();
    pthread_t callers[8];
    for (unsigned index = 0; index < 8; ++index) {
        assert(pthread_create(&callers[index], NULL, host_group, hits) == 0);
    }
    for (unsigned index = 0; index < 8; ++index) {
        assert(pthread_join(callers[index], NULL) == 0);
    }
    assert(atomic_load(&peak) == required - 1);
    check_hits(8);

    if (required > 1) {
        assert(pthread_mutex_lock(&observation) == 0);
        while (atomic_load(&parked) == 0) assert(pthread_cond_wait(&observed, &observation) == 0);
        assert(pthread_mutex_unlock(&observation) == 0);
        assert(atomic_load(&broadcasts) > 0);
    }
    reset_hits();
    assert(tsuzuri_task_parallel_results(result_visit, hits, ITEMS) == UINT64_MAX);
    check_hits(1);
    reset_hits();
    assert(tsuzuri_task_parallel_results(result_stop, hits, ITEMS) == 3);
    for (unsigned index = 0; index <= 3; ++index) assert(atomic_load(&hits[index]) == 1);
    if (required == 1) for (unsigned index = 4; index < ITEMS; ++index) assert(atomic_load(&hits[index]) == 0);
    reset_hits();
    assert(tsuzuri_task_parallel_results(result_failure, hits, ITEMS) == 2);
    assert(atomic_load(&hits[2]) == 1);
    assert(tsuzuri_task_parallel_results(NULL, NULL, 0) == UINT64_MAX);
    tz_task_shutdown();
    tz_task_shutdown();
    assert(atomic_load(&created) == atomic_load(&joined));
    assert(atomic_load(&outstanding) == 0);
    assert(pthread_mutex_destroy(&mutex) == 0);
    assert(pthread_cond_destroy(&ready) == 0);
    assert(pthread_mutex_destroy(&observation) == 0);
    assert(pthread_cond_destroy(&observed) == 0);
    return 0;
}
