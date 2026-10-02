/* Unit tests of the trap boundary runtime (E14 Phase 2): src/runtime/trap.c over the task
   scheduler of src/runtime/task.c. Every allocation is counted, so each case ends with the
   number of live blocks, which must be zero once the host has released its results.
   Usage: trap_boundary_runtime [parallelism]   (1 runs groups on the calling thread) */
#if defined(__linux__) && !defined(_GNU_SOURCE)
#define _GNU_SOURCE
#endif
/* The checks are assertions: some toolchains define NDEBUG for optimized builds. */
#undef NDEBUG
#include <assert.h>
#include <stdatomic.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static atomic_long live, total;
static atomic_long fail_after = -1;

static int allocation_fails(void) {
    return atomic_load(&fail_after) >= 0 && atomic_fetch_sub(&fail_after, 1) == 0;
}

static void *counted_malloc(size_t size) {
    if (allocation_fails()) return NULL;
    void *pointer = malloc(size);
    if (pointer) {
        atomic_fetch_add(&live, 1);
        atomic_fetch_add(&total, 1);
    }
    return pointer;
}

static void counted_free(void *pointer) {
    if (!pointer) return;
    atomic_fetch_sub(&live, 1);
    free(pointer);
}

static void *counted_realloc(void *pointer, size_t size) {
    return allocation_fails() ? NULL : realloc(pointer, size);
}

#define TZ_TRAP_MALLOC counted_malloc
#define TZ_TRAP_FREE counted_free
#define TZ_TRAP_REALLOC counted_realloc
#define TZ_TRAP_API
#define TZ_TRAP_BOUNDARY
#include "../src/runtime/trap.c"
#include "../src/runtime/task.c"

static unsigned checks;
#define CHECK(condition) do { assert(condition); ++checks; } while (0)

struct work {
    long count;
    void *result;
};

/* Allocates `count` blocks, frees all but the first, which stays as the result for the host. */
static void finish(void *argument) {
    struct work *work = argument;
    for (long index = 0; index < work->count; ++index) {
        unsigned char *block = tsuzuri_tracked_malloc(16 + (size_t)index);
        assert(block && (uintptr_t)block % 16 == 0);
        memset(block, 0x5a, 16 + (size_t)index);
        if (index == 0) work->result = block;
        else tsuzuri_tracked_free(block);
    }
}

/* Allocates `count` blocks, frees a third of them, grows one, then traps with nothing released. */
static void trapping(void *argument) {
    struct work *work = argument;
    for (long index = 0; index < work->count; ++index) {
        void *block = tsuzuri_tracked_malloc(32 + (size_t)index);
        assert(block);
        if (index % 3 == 0) tsuzuri_tracked_free(block);
    }
    unsigned char *grown = tsuzuri_tracked_malloc(8);
    memset(grown, 0x33, 8);
    grown = tsuzuri_tracked_realloc(grown, 4096);
    assert(grown && grown[7] == 0x33);
    tsuzuri_trap_raise(42, 3);
    assert(!"a trap inside a boundary must not return");
}

static int ran_inner;

static void inner_boundary(void *argument) {
    (void)argument;
    ran_inner = 1;
}

static void reenter(void *argument) {
    tsuzuri_trap_info trap = { 0, 0 };
    CHECK(tsuzuri_boundary_run(inner_boundary, argument, &trap) == 2);
    CHECK(!ran_inner);
    struct work *work = argument;
    work->result = tsuzuri_tracked_malloc(24);
}

static int32_t run(void (*thunk)(void *), struct work *work, tsuzuri_trap_info *trap) {
    trap->site = 0;
    trap->kind = 0;
    return tsuzuri_boundary_run(thunk, work, trap);
}

struct plan {
    uint64_t trap_at[3];
    uint64_t fail_at;
    atomic_uint started;
    atomic_uint trapped;
    int wait;
};

static void leak_and_trap(struct plan *plan, uint64_t index) {
    for (unsigned slot = 0; slot < 3; ++slot) {
        if (plan->trap_at[slot] == index) {
            tsuzuri_tracked_malloc(48);
            atomic_store(&plan->trapped, 1);
            tsuzuri_trap_raise((uint32_t)(1000 + index), 4);
            assert(!"a trap inside a group item must not return");
        }
    }
}

static void item(void *context, uint64_t index) {
    struct plan *plan = context;
    atomic_fetch_add(&plan->started, 1);
    void *scratch = tsuzuri_tracked_malloc(64 + (size_t)index);
    leak_and_trap(plan, index);
    tsuzuri_tracked_free(scratch);
}

static uint32_t item_result(void *context, uint64_t index) {
    struct plan *plan = context;
    atomic_fetch_add(&plan->started, 1);
    void *scratch = tsuzuri_tracked_malloc(64 + (size_t)index);
    if (plan->wait && index == plan->fail_at) {
        /* Fail only after a later item has trapped, so the trap is discarded after it happened. */
        for (unsigned spin = 0; spin < 20000 && atomic_load(&plan->trapped) == 0; ++spin) usleep(100);
    }
    leak_and_trap(plan, index);
    tsuzuri_tracked_free(scratch);
    return index == plan->fail_at;
}

static struct plan inner_plans[8];

static void outer_item(void *context, uint64_t index) {
    (void)context;
    tsuzuri_task_parallel(item, &inner_plans[index], 6);
}

struct group {
    uint64_t length;
    struct plan plan;
    uint64_t failed;
    int results;
    int nested;
};

static void submit(void *argument) {
    struct group *group = argument;
    if (group->nested) tsuzuri_task_parallel(outer_item, NULL, group->length);
    else if (group->results) group->failed = tsuzuri_task_parallel_results(item_result, &group->plan, group->length);
    else tsuzuri_task_parallel(item, &group->plan, group->length);
}

static int32_t submit_group(struct group *group, tsuzuri_trap_info *trap) {
    trap->site = 0;
    trap->kind = 0;
    group->failed = UINT64_MAX;
    atomic_store(&group->plan.started, 0);
    atomic_store(&group->plan.trapped, 0);
    return tsuzuri_boundary_run(submit, group, trap);
}

static struct plan no_traps(void) {
    struct plan plan = { .trap_at = { UINT64_MAX, UINT64_MAX, UINT64_MAX }, .fail_at = UINT64_MAX };
    return plan;
}

static void check_clean(void) {
    CHECK(atomic_load(&live) == 0);
    CHECK(tz_task_pool.groups == NULL && tz_task_pool.tail == NULL);
}

static void metadata_edges(void *argument) {
    (void)argument;
    unsigned char *blocks[1024];
    for (unsigned index = 0; index < 1024; ++index) {
        if (index == 16) {
            for (long budget = 0; budget < 3; ++budget) {
                atomic_store(&fail_after, budget);
                CHECK(tsuzuri_tracked_malloc(16) == NULL);
            }
        }
        blocks[index] = tsuzuri_tracked_malloc(16);
        CHECK(blocks[index] != NULL);
        blocks[index][0] = (unsigned char)index;
    }
    for (unsigned index = 0; index < 1024; index += 17) {
        atomic_store(&fail_after, 0);
        CHECK(tsuzuri_tracked_realloc(blocks[index], 64) == NULL);
        CHECK(blocks[index][0] == (unsigned char)index);
        blocks[index] = tsuzuri_tracked_realloc(blocks[index], 64);
        CHECK(blocks[index] != NULL && blocks[index][0] == (unsigned char)index);
    }
    for (unsigned index = 0; index < 1024; ++index) tsuzuri_tracked_free(blocks[index]);
}

static void metadata_failure(void *argument) {
    struct work *work = argument;
    work->result = tsuzuri_tracked_malloc(16);
    CHECK(work->result == NULL);
}

static void single_calls(void) {
    tsuzuri_trap_info trap;
    struct work work = { 5, NULL };

    CHECK(run(finish, &work, &trap) == 0);
    CHECK(work.result && atomic_load(&live) == 1);
    tsuzuri_tracked_free(work.result);
    check_clean();

    work.result = NULL;
    CHECK(run(trapping, &work, &trap) == 1);
    CHECK(trap.site == 42 && trap.kind == 3);
    check_clean();

    work.count = 0;
    CHECK(run(trapping, &work, &trap) == 1 && trap.site == 42);
    CHECK(run(finish, &work, &trap) == 0 && work.result == NULL);
    check_clean();

    /* A host that alternates for a long time leaves nothing behind. */
    for (long round = 0; round < 10000; ++round) {
        work.count = 1 + round % 7;
        work.result = NULL;
        if (round % 2) {
            CHECK(run(trapping, &work, &trap) == 1);
            CHECK(trap.site == 42);
        } else {
            CHECK(run(finish, &work, &trap) == 0);
            tsuzuri_tracked_free(work.result);
        }
        CHECK(atomic_load(&live) == 0);
    }

    /* A second boundary on the same thread is refused and runs nothing. */
    work.result = NULL;
    ran_inner = 0;
    CHECK(run(reenter, &work, &trap) == 0);
    tsuzuri_tracked_free(work.result);
    check_clean();

    /* Without a boundary a raise returns to the caller (the report then ends the process) and
       blocks are untracked. */
    tsuzuri_trap_raise(1, 1);
    void *outside = tsuzuri_tracked_malloc(10);
    CHECK(atomic_load(&live) == 1);
    CHECK(tsuzuri_boundary_owner() == NULL);
    tsuzuri_tracked_free(outside);
    tsuzuri_tracked_free(NULL);
    check_clean();

    /* A block allocated outside a boundary is not freed by the trap of a later one. */
    outside = tsuzuri_tracked_malloc(10);
    work.count = 3;
    CHECK(run(trapping, &work, &trap) == 1);
    CHECK(atomic_load(&live) == 1);
    memset(outside, 1, 10);
    tsuzuri_tracked_free(outside);
    check_clean();

    for (long budget = 0; budget < 3; ++budget) {
        atomic_store(&fail_after, budget);
        CHECK(run(metadata_failure, &work, &trap) == 0);
        check_clean();
    }
    CHECK(run(metadata_edges, &work, &trap) == 0);
    check_clean();
}

static void groups(void) {
    tsuzuri_trap_info trap;
    struct group group = { 64, no_traps(), 0, 0, 0 };

    CHECK(submit_group(&group, &trap) == 0);
    CHECK(atomic_load(&group.plan.started) == 64);
    check_clean();

    /* One trapping item: the call that submitted the group returns status 1 with its site. */
    group.plan.trap_at[0] = 5;
    CHECK(submit_group(&group, &trap) == 1);
    CHECK(trap.site == 1005 && trap.kind == 4);
    CHECK(atomic_load(&group.plan.started) >= 6 && atomic_load(&group.plan.started) <= 64);
    check_clean();

    /* Several: the lowest index wins, whichever worker finishes first. */
    for (unsigned round = 0; round < 200; ++round) {
        group.plan.trap_at[0] = 12;
        group.plan.trap_at[1] = 3;
        group.plan.trap_at[2] = 40;
        CHECK(submit_group(&group, &trap) == 1);
        CHECK(trap.site == 1003);
        CHECK(atomic_load(&live) == 0);
    }

    /* The trapped group leaves the scheduler usable. */
    group.plan = no_traps();
    CHECK(submit_group(&group, &trap) == 0);
    CHECK(atomic_load(&group.plan.started) == 64);
    check_clean();

    /* A single item runs on the calling thread without a group. */
    group.length = 1;
    group.plan.trap_at[0] = 0;
    CHECK(submit_group(&group, &trap) == 1 && trap.site == 1000);
    check_clean();

    /* Task.parallel_results. A trap in an item that ran is reported even if an earlier item
       returned Err, because a trapped item leaves its task in no defined state; so with an Err
       before a trap either may be seen, unless the Err waits for the trap to have happened. */
    group.length = 16;
    group.results = 1;
    group.plan = no_traps();
    group.plan.fail_at = 2;
    group.plan.trap_at[0] = 7;
    int32_t status = submit_group(&group, &trap);
    CHECK((status == 0 && group.failed == 2) || (status == 1 && trap.site == 1007));
    check_clean();
    group.plan = no_traps();
    group.plan.fail_at = 2;
    group.plan.trap_at[0] = 7;
    group.plan.wait = tz_task_parallelism() > 1;
    status = submit_group(&group, &trap);
    if (group.plan.wait) CHECK(status == 1 && trap.site == 1007 && atomic_load(&group.plan.trapped) == 1);
    else CHECK(status == 0 && group.failed == 2);
    check_clean();
    group.plan = no_traps();
    group.plan.fail_at = 9;
    group.plan.trap_at[0] = 4;
    CHECK(submit_group(&group, &trap) == 1);
    CHECK(trap.site == 1004);
    check_clean();
    group.plan = no_traps();
    group.plan.fail_at = 6;
    CHECK(submit_group(&group, &trap) == 0 && group.failed == 6);
    check_clean();

    /* A trap in a group nested in an item reaches the boundary of the outermost call. */
    group.results = 0;
    group.nested = 1;
    group.length = 8;
    for (unsigned index = 0; index < 8; ++index) inner_plans[index] = no_traps();
    inner_plans[2].trap_at[0] = 1;
    inner_plans[5].trap_at[0] = 4;
    CHECK(submit_group(&group, &trap) == 1);
    CHECK(trap.site == 1001);
    check_clean();
    for (unsigned index = 0; index < 8; ++index) inner_plans[index] = no_traps();
    CHECK(submit_group(&group, &trap) == 0);
    check_clean();
}

struct host {
    unsigned id;
    int failures;
};

static void *host_thread(void *argument) {
    struct host *host = argument;
    tsuzuri_trap_info trap;
    struct group group = { 24, no_traps(), 0, 0, 0 };
    struct work work = { 4, NULL };
    for (unsigned round = 0; round < 300; ++round) {
        switch ((round + host->id) % 4) {
        case 0:
            if (run(finish, &work, &trap) != 0 || !work.result) ++host->failures;
            tsuzuri_tracked_free(work.result);
            break;
        case 1:
            if (run(trapping, &work, &trap) != 1 || trap.site != 42) ++host->failures;
            break;
        case 2:
            group.plan = no_traps();
            if (submit_group(&group, &trap) != 0) ++host->failures;
            break;
        default:
            group.plan = no_traps();
            group.plan.trap_at[0] = (round + host->id) % 24;
            if (submit_group(&group, &trap) != 1 || trap.site != 1000 + (round + host->id) % 24) ++host->failures;
            break;
        }
    }
    return NULL;
}

static void concurrent_hosts(void) {
    pthread_t threads[6];
    struct host hosts[6];
    for (unsigned index = 0; index < 6; ++index) {
        hosts[index].id = index;
        hosts[index].failures = 0;
        assert(pthread_create(&threads[index], NULL, host_thread, &hosts[index]) == 0);
    }
    for (unsigned index = 0; index < 6; ++index) {
        assert(pthread_join(threads[index], NULL) == 0);
        CHECK(hosts[index].failures == 0);
    }
    check_clean();
}

int main(int argc, char **argv) {
    unsigned parallelism = 0;
    if (argc == 2) {
        parallelism = (unsigned)strtoul(argv[1], NULL, 10);
        atomic_store(&tz_task_limit, parallelism);
    }
    single_calls();
    groups();
    concurrent_hosts();
    CHECK(atomic_load(&total) > 1000);
    tz_task_shutdown();
    check_clean();
    printf("trap boundary runtime: %u checks passed\n", checks);
    return 0;
}
