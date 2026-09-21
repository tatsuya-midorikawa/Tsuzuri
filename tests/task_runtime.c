#include <assert.h>
#include <errno.h>
#include <pthread.h>
#include <stdatomic.h>
#include <stdint.h>
#include <string.h>
#include <unistd.h>

static long processors = 4;
static atomic_uint created, joined, outstanding, peak;
static int fail_create, fail_join;

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

#define sysconf test_sysconf
#define pthread_create test_create
#define pthread_join test_join
#include "../src/runtime/task.c"
#undef sysconf
#undef pthread_create
#undef pthread_join

enum { ITEMS = 257 };
static atomic_uint hits[ITEMS];
static pthread_mutex_t mutex = PTHREAD_MUTEX_INITIALIZER;
static pthread_cond_t ready = PTHREAD_COND_INITIALIZER;
static unsigned arrivals;

static void visit(void *context, uint64_t index) {
    assert(context == hits && index < ITEMS);
    atomic_fetch_add(&hits[index], 1);
}

static void rendezvous(void *context, uint64_t index) {
    if (index < 4) {
        assert(pthread_mutex_lock(&mutex) == 0);
        ++arrivals;
        if (arrivals == 4) {
            assert(pthread_cond_broadcast(&ready) == 0);
        }
        while (arrivals < 4) {
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

static void reset(long available) {
    assert(atomic_load(&tz_task_workers) == 0);
    assert(atomic_load(&outstanding) == 0);
    assert(atomic_load(&created) == atomic_load(&joined));
    atomic_store(&tz_task_limit, 0);
    atomic_store(&peak, 0);
    atomic_store(&created, 0);
    atomic_store(&joined, 0);
    processors = available;
    for (unsigned index = 0; index < ITEMS; ++index) {
        atomic_store(&hits[index], 0);
    }
}

static void check_hits(unsigned expected) {
    for (unsigned index = 0; index < ITEMS; ++index) {
        assert(atomic_load(&hits[index]) == expected);
    }
    assert(atomic_load(&tz_task_workers) == 0);
    assert(atomic_load(&outstanding) == 0);
    assert(atomic_load(&created) == atomic_load(&joined));
}

int main(int argc, char **argv) {
    if (argc == 2) {
        fail_create = strcmp(argv[1], "create") == 0;
        fail_join = strcmp(argv[1], "join") == 0;
        tsuzuri_task_parallel(visit, hits, ITEMS);
        assert(!"thread failure must terminate with a diagnostic");
    }
    tsuzuri_task_parallel(NULL, NULL, 0);
    tsuzuri_task_parallel(visit, hits, 1);
    assert(atomic_load(&created) == 0 && atomic_load(&hits[0]) == 1);

    reset(4);
    tsuzuri_task_parallel(rendezvous, hits, ITEMS);
    assert(arrivals == 4 && atomic_load(&peak) == 3);
    check_hits(1);

    reset(4);
    tsuzuri_task_parallel(nested, hits, 32);
    assert(atomic_load(&peak) <= 3);
    check_hits(32);

    reset(4);
    pthread_t callers[8];
    for (unsigned index = 0; index < 8; ++index) {
        assert(pthread_create(&callers[index], NULL, host_group, hits) == 0);
    }
    for (unsigned index = 0; index < 8; ++index) {
        assert(pthread_join(callers[index], NULL) == 0);
    }
    assert(atomic_load(&peak) <= 3);
    check_hits(8);

    reset(1000);
    tsuzuri_task_parallel(visit, hits, ITEMS);
    assert(atomic_load(&peak) == TZ_TASK_MAX_THREADS - 1);
    check_hits(1);

    for (long available = -1; available <= 1; ++available) {
        reset(available);
        tsuzuri_task_parallel(visit, hits, ITEMS);
        assert(atomic_load(&created) == 0);
        check_hits(1);
    }
    assert(pthread_mutex_destroy(&mutex) == 0);
    assert(pthread_cond_destroy(&ready) == 0);
    return 0;
}
