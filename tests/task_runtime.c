/* The checks are assertions: some toolchains define NDEBUG for optimized builds (the bundled Windows toolchain does from -O1). */
#undef NDEBUG
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
static atomic_uint parked, broadcasts, signals;
/* How often the pool mutex was taken, how many workers are asleep on the pool's condition now, and how many
   of the signals sent to them have not woken one yet. */
static atomic_uint locks, asleep, pending;
static int fail_create, fail_join;
static const char *failure = "";
static pthread_mutex_t observation = PTHREAD_MUTEX_INITIALIZER;
static pthread_cond_t observed = PTHREAD_COND_INITIALIZER;
static pthread_cond_t *worker_condition;
static pthread_cond_t *done_condition;
static pthread_mutex_t *pool_mutex;

/* The thread of main, the only submitter that the checks below watch. */
static _Thread_local int on_main;
/* I1: the first signal of the next group (sent by main) is held until the workers have run every item but the first. */
static atomic_int first_child_probe, others_done, first_child_on_main;
/* D14: the worker that finishes a group releases its items, wakes the submitter and counts itself free in one critical
   section. While the check is armed, (1) a worker that takes the pool mutex while it is counted running still has
   items to release (check_leaving), so it has not released them before taking the mutex; (2) a thread that lets go of
   the mutex or takes it again right after it woke the submitter (a broadcast on work_done) is held back until the test
   has looked at the pool, which a submitter that has returned would find still counting the thread as running; and
   (3) whenever a thread lets go of the mutex, a group that is claimed to the end and not released yet has a thread that
   the pool counts as running (check_counted). A pool that does none of these things is not touched by the hold. */
static atomic_int hold_armed, hold_released;
static _Thread_local int woke_submitter;
static void probe_published_group(void);
static void wait_for_workers(void);
static void check_counted(void);
static void check_leaving(void);

static void hold_back(void) {
    if (!atomic_load(&hold_armed)) return;
    assert(pthread_mutex_lock(&observation) == 0);
    while (!atomic_load(&hold_released)) assert(pthread_cond_wait(&observed, &observation) == 0);
    assert(pthread_mutex_unlock(&observation) == 0);
}

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
    if (woke_submitter) {
        woke_submitter = 0;
        hold_back();
    }
    atomic_fetch_add(&locks, 1);
    if (strcmp(failure, "mutex_lock") == 0) return EINVAL;
    int error = pthread_mutex_lock(mutex);
    if (error == 0 && !on_main && atomic_load(&hold_armed)) check_leaving();
    return error;
}

static int test_unlock(pthread_mutex_t *mutex) {
    if (atomic_load(&hold_armed)) check_counted();
    int held = woke_submitter;
    woke_submitter = 0;
    int error = strcmp(failure, "mutex_unlock") == 0 ? EINVAL : pthread_mutex_unlock(mutex);
    if (held) hold_back();
    return error;
}

static int test_wait(pthread_cond_t *condition, pthread_mutex_t *mutex) {
    if (atomic_load(&hold_armed)) check_counted();
    woke_submitter = 0;
    if (strcmp(failure, "cond_wait") == 0) return EINVAL;
    if (condition == worker_condition) {
        assert(pthread_mutex_lock(&observation) == 0);
        atomic_fetch_add(&parked, 1);
        atomic_fetch_add(&asleep, 1);
        assert(pthread_cond_broadcast(&observed) == 0);
        assert(pthread_mutex_unlock(&observation) == 0);
        int error = pthread_cond_wait(condition, mutex);
        assert(pthread_mutex_lock(&observation) == 0);
        atomic_fetch_sub(&asleep, 1);
        if (atomic_load(&pending) > 0) atomic_fetch_sub(&pending, 1);
        assert(pthread_cond_broadcast(&observed) == 0);
        assert(pthread_mutex_unlock(&observation) == 0);
        return error;
    }
    return pthread_cond_wait(condition, mutex);
}

static int test_broadcast(pthread_cond_t *condition) {
    atomic_fetch_add(&broadcasts, 1);
    if (condition == done_condition) woke_submitter = 1;
    return strcmp(failure, "cond_broadcast") == 0 ? EINVAL : pthread_cond_broadcast(condition);
}

static int test_signal(pthread_cond_t *condition) {
    atomic_fetch_add(&signals, 1);
    if (strcmp(failure, "cond_signal") == 0) return EINVAL;
    if (condition != worker_condition) return pthread_cond_signal(condition);
    // POSIX drops a signal that finds nobody to wake. macOS remembers it and wakes a later sleeper, which would hide a
    // lost wakeup, so the signal is passed on only when a sleeper exists that no earlier signal is on its way to wake.
    // With the pool mutex held a counted sleeper really waits, so a signal that is passed on never misses it, and the
    // counts change only under this mutex (a worker updates them after it has taken the mutex back).
    assert(pthread_mutex_lock(pool_mutex) == 0);
    // The submitter's first signal of a group that is being watched (I1): the group is published and nothing has woken
    // yet, so the first item has to be the submitter's already; then the submitter waits here until the workers have
    // run everything else, and has to be the one that runs the first item whatever the workers have done meanwhile.
    int probe = on_main && atomic_exchange(&first_child_probe, 0);
    if (probe) probe_published_group();
    if (atomic_load(&pending) < atomic_load(&asleep)) {
        atomic_fetch_add(&pending, 1);
        assert(pthread_cond_signal(condition) == 0);
    }
    assert(pthread_mutex_unlock(pool_mutex) == 0);
    if (probe) wait_for_workers();
    return 0;
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
#define TZ_TASK_COND_SIGNAL test_signal
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

/* Waits until every worker is asleep and no signal is still on its way to wake one, so that nothing of an earlier
   group is left that could take the mutex or wake without a signal of the next group. */
static void quiesce(void) {
    assert(pthread_mutex_lock(&observation) == 0);
    while (atomic_load(&asleep) != required - 1 || atomic_load(&pending) != 0) assert(pthread_cond_wait(&observed, &observation) == 0);
    assert(pthread_mutex_unlock(&observation) == 0);
}

static uint32_t result_visit(void *context, uint64_t index) {
    visit(context, index);
    return 0;
}

/* With the pool mutex held, from the first signal of a group: the group is published and no worker has been woken, so
   the first item must be claimed already, as the submitter's. A pool that publishes the group first and lets the
   submitter claim afterwards would let the workers take every item, the first too. */
static void probe_published_group(void) {
    assert(tz_task_pool.tail != NULL && atomic_load(&tz_task_pool.tail->next) >= 1);
}

static void wait_for_workers(void) {
    assert(pthread_mutex_lock(&observation) == 0);
    while (atomic_load(&others_done) != ITEMS - 1) assert(pthread_cond_wait(&observed, &observation) == 0);
    assert(pthread_mutex_unlock(&observation) == 0);
}

/* With the pool mutex held, while the check of D14 runs (one group, nothing that waits on a channel, so no thread is
   uncounted): when every item of a group is claimed and some are not released, the thread that holds them is running
   or about to release them, and the pool counts it as running until it has. A pool that counted the worker free
   first, in a critical section of its own, would show nobody running while the last item is still unreleased. */
static void check_counted(void) {
    for (struct tz_task_group *group = tz_task_pool.groups; group; group = group->next_group) {
        if (atomic_load(&group->next) >= group->length && atomic_load(&group->remaining) != 0) assert(tz_task_pool.busy != 0);
    }
}

/* With the pool mutex just taken by a worker, in the check of D14: a worker that is counted running has items that it
   has not released, so no group is finished. A worker that released its items before it took the mutex to count
   itself free would find the group finished here. */
static void check_leaving(void) {
    if (!tz_task_counted) return;
    for (struct tz_task_group *group = tz_task_pool.groups; group; group = group->next_group) {
        assert(atomic_load(&group->remaining) != 0);
    }
}

/* The first item records on which thread it ran, and every other item counts itself; the one that ends last tells the
   submitter, which waits in its first signal (test_signal), that the workers have run all the rest. */
static void first_child(void *context, uint64_t index) {
    visit(context, index);
    if (index == 0) {
        atomic_store(&first_child_on_main, on_main);
        return;
    }
    if (atomic_fetch_add(&others_done, 1) + 1 == ITEMS - 1) {
        assert(pthread_mutex_lock(&observation) == 0);
        assert(pthread_cond_broadcast(&observed) == 0);
        assert(pthread_mutex_unlock(&observation) == 0);
    }
}

/* One group of ITEMS indices on a pool that is asleep. The mutex is taken a few times, not once for each index
   (259 times for `run` and 515 for `results` before PR06): twice by the submitter, once by each worker that took
   part. The workers are woken by signals that pass along the group: some at the submit, up to
   TZ_TASK_CHAIN_SIGNALS for each worker that joins while items are unstarted, and at most one broadcast, by the
   worker that finishes the group for a submitter that waits. A pool of one thread takes no lock and wakes nobody. */
static void check_group_cost(int results) {
    quiesce();
    reset_hits();
    unsigned locks_before = atomic_load(&locks), signals_before = atomic_load(&signals), broadcasts_before = atomic_load(&broadcasts);
    if (results) assert(tsuzuri_task_parallel_results(result_visit, hits, ITEMS) == UINT64_MAX);
    else tsuzuri_task_parallel(visit, hits, ITEMS);
    unsigned taken = atomic_load(&locks) - locks_before, woken = atomic_load(&signals) - signals_before;
    unsigned broadcast = atomic_load(&broadcasts) - broadcasts_before;
    check_hits(1);
    if (required == 1) {
        assert(taken == 0 && woken == 0 && broadcast == 0);
        return;
    }
    assert(taken <= 2 + (required - 1));
    assert(woken >= 1 && woken <= TZ_TASK_INITIAL_SIGNALS + TZ_TASK_CHAIN_SIGNALS * (required - 1));
    assert(broadcast <= 1);
}

static atomic_uint late;

/* Item 1 is the only one that a worker can take: item 0, the submitter's, ends after the worker started it, and
   the worker ends after the submitter waits for the join (it no longer counts as running). So the worker
   finishes the group, and has to wake the submitter that sleeps. */
static void last_worker(void *context, uint64_t index) {
    (void)context;
    if (index == 0) {
        assert(pthread_mutex_lock(&mutex) == 0);
        while (atomic_load(&late) == 0) assert(pthread_cond_wait(&ready, &mutex) == 0);
        assert(pthread_mutex_unlock(&mutex) == 0);
        return;
    }
    assert(pthread_mutex_lock(&mutex) == 0);
    atomic_store(&late, 1);
    assert(pthread_cond_broadcast(&ready) == 0);
    assert(pthread_mutex_unlock(&mutex) == 0);
    for (;;) {
        tz_task_lock();
        uint64_t running = tz_task_pool.busy;
        tz_task_unlock();
        if (running == 1) break;
    }
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
    on_main = 1;
    worker_condition = &tz_task_pool.work_available;
    done_condition = &tz_task_pool.work_done;
    pool_mutex = &tz_task_pool.mutex;
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

    // A group takes the pool mutex a few times, not once for each index: the submitter publishes the group and
    // waits for it, and each worker that took part lets go of its items. A pool of one thread never takes it.
    // (The mutex was taken once for each index before PR06: 259 times for this group, 515 for `results`.)
    quiesce();
    for (unsigned round = 0; round < 3; ++round) {
        check_group_cost(0);
        check_group_cost(1);
    }

    // A pool that sleeps is woken as far as the group needs it: every item waits for all the workers, so a
    // wakeup that is lost hangs here (the alarm of tests/tasks.mjs and of the callers ends it).
    quiesce();
    arrivals = 0;
    reset_hits();
    tsuzuri_task_parallel(rendezvous, hits, ITEMS);
    assert(arrivals == required);
    check_hits(1);

    if (required > 1) {
        // I1: the thread that submits a group starts its first item, even when it is held up for as long as the
        // workers need to take every other one. The first signal of the group waits (test_signal) until they have.
        quiesce();
        reset_hits();
        atomic_store(&others_done, 0);
        atomic_store(&first_child_on_main, 0);
        atomic_store(&first_child_probe, 1);
        tsuzuri_task_parallel(first_child, hits, ITEMS);
        assert(atomic_load(&first_child_probe) == 0);
        assert(atomic_load(&first_child_on_main) == 1);
        check_hits(1);
    }

    if (required > 1) {
        // D14: the worker that finishes the group releases its items, wakes the submitter and counts itself free in one
        // critical section. Armed, the hooks check that a worker has not released its items before it takes the mutex
        // (check_leaving), hold back a worker that lets go of the mutex or takes it again right after it woke the
        // submitter until the submitter has returned and the pool has been looked at (a submitter that returned meanwhile
        // finds the worker still counted as running), and check at every release of the mutex (check_counted) that no
        // worker was counted free while it still had an item to release.
        quiesce();
        unsigned signals_before = atomic_load(&signals), broadcasts_before = atomic_load(&broadcasts);
        atomic_store(&hold_released, 0);
        atomic_store(&hold_armed, 1);
        tsuzuri_task_parallel(last_worker, NULL, 2);
        assert(atomic_load(&signals) - signals_before == 1);
        assert(atomic_load(&broadcasts) - broadcasts_before == 1);
        tz_task_lock();
        assert(tz_task_pool.groups == NULL && tz_task_pool.busy == 0 && tz_task_pool.idle == required - 1);
        tz_task_unlock();
        atomic_store(&hold_armed, 0);
        assert(pthread_mutex_lock(&observation) == 0);
        atomic_store(&hold_released, 1);
        assert(pthread_cond_broadcast(&observed) == 0);
        assert(pthread_mutex_unlock(&observation) == 0);
    }

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
    // Whatever was started after the failure was recorded ran once, never twice.
    for (unsigned index = 4; index < ITEMS; ++index) assert(atomic_load(&hits[index]) <= 1);
    reset_hits();
    assert(tsuzuri_task_parallel_results(result_failure, hits, ITEMS) == 2);
    assert(atomic_load(&hits[0]) == 1 && atomic_load(&hits[1]) == 1 && atomic_load(&hits[2]) == 1);
    for (unsigned index = 3; index < ITEMS; ++index) assert(atomic_load(&hits[index]) <= 1);
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
