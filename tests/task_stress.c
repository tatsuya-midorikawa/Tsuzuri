/* PR06: the task scheduler (src/runtime/task.c) under several hosts at once. Each check has an exact
   expected value that comes from the plan of the call alone (closed forms and the lowest failing index),
   never from what the scheduler printed. No check waits for a time: an alarm turns a hang, which a lost
   wakeup would be, into a failure.
     - nested groups: every inner sum and every inner index is exact, however the items are spread;
     - Task.parallel_results: the lowest failing index is returned, and every index below it ran once;
     - one large group: every index runs exactly once under contention for the claim;
     - pipelines (a producer and consumers on one channel): a group may end with the deadlock verdict when
       the threads run out, which is allowed here, but never with a hang, and never with a wrong sum when
       no verdict was given.
   Usage: task_stress [cpus [hosts [rounds]]]. The pool reads the CPU count once, so the count is an
   argument. tests/tasks.mjs builds this at -O0 and -O3, and under ThreadSanitizer and AddressSanitizer
   when TSUZURI_TSAN=1 or TSUZURI_ASAN=1. POSIX only (alarm). */
#undef NDEBUG
#include <assert.h>
#include <pthread.h>
#include <stdatomic.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

static long processors = 4;

static long test_sysconf(int name) {
    (void)name;
    return processors;
}

#define TZ_TASK_SYSCONF test_sysconf
#include "../src/runtime/task.c"

enum { MAX_OUTER = 12, MAX_INNER = 9, MAX_ITEMS = 64, LARGE_ITEMS = 65536 };

/* A little work of a size that depends on the index, so that the items of a group do not all end together. */
static void spin(unsigned amount) {
    volatile unsigned sink = 0;
    for (unsigned count = 0; count < amount; ++count) sink = sink + count;
}

struct nested_plan {
    unsigned outer;
    unsigned seed;
    uint64_t sums[MAX_OUTER];
    _Atomic unsigned visits[MAX_OUTER][MAX_INNER];
};

struct inner_context {
    struct nested_plan *plan;
    uint64_t outer;
    _Atomic uint64_t sum;
};

static unsigned inner_length(unsigned seed, uint64_t outer) {
    return 1 + (seed + 5 * (unsigned)outer) % MAX_INNER;
}

static uint64_t term(uint64_t outer, uint64_t inner) {
    return outer * 1000 + inner * 7 + 1;
}

static void inner_item(void *context, uint64_t index) {
    struct inner_context *inner = context;
    spin((unsigned)(index * 3) % 7);
    atomic_fetch_add(&inner->plan->visits[inner->outer][index], 1);
    atomic_fetch_add(&inner->sum, term(inner->outer, index));
}

static void outer_item(void *context, uint64_t index) {
    struct nested_plan *plan = context;
    struct inner_context inner = { plan, index, 0 };
    tsuzuri_task_parallel(inner_item, &inner, inner_length(plan->seed, index));
    plan->sums[index] = atomic_load(&inner.sum);
}

static void nested_groups(unsigned seed) {
    struct nested_plan plan = { 2 + (seed >> 4) % (MAX_OUTER - 1), seed >> 9, { 0 }, { { 0 } } };
    tsuzuri_task_parallel(outer_item, &plan, plan.outer);
    for (unsigned outer = 0; outer < plan.outer; ++outer) {
        uint64_t length = inner_length(plan.seed, outer);
        /* The sum of outer * 1000 + inner * 7 + 1 over the inner indexes below length. */
        assert(plan.sums[outer] == length * (outer * 1000 + 1) + 7 * (length * (length - 1) / 2));
        for (unsigned inner = 0; inner < MAX_INNER; ++inner) {
            assert(atomic_load(&plan.visits[outer][inner]) == (inner < length ? 1u : 0u));
        }
    }
}

struct failing_plan {
    unsigned length;
    unsigned period;
    unsigned remainder;
    _Atomic unsigned runs[MAX_ITEMS];
};

static uint32_t failing_item(void *context, uint64_t index) {
    struct failing_plan *plan = context;
    atomic_fetch_add(&plan->runs[index], 1);
    spin((unsigned)(index * 5) % 11);
    return index % plan->period == plan->remainder;
}

/* The lowest index that fails is `remainder` when it is below the length; every index below it must have run
   once, and no index may run twice. */
static void failing_group(unsigned seed) {
    struct failing_plan plan = { 2 + (seed >> 3) % (MAX_ITEMS - 1), 2 + (seed >> 11) % 8, 0, { 0 } };
    plan.remainder = (seed >> 17) % plan.period;
    uint64_t expected = plan.remainder < plan.length ? plan.remainder : UINT64_MAX;
    assert(tsuzuri_task_parallel_results(failing_item, &plan, plan.length) == expected);
    for (unsigned index = 0; index < plan.length; ++index) {
        unsigned runs = atomic_load(&plan.runs[index]);
        assert(runs <= 1);
        if (index <= expected) assert(runs == 1);
    }
}

static void large_item(void *context, uint64_t index) {
    atomic_fetch_add(&((atomic_uchar *)context)[index], 1);
}

static void large_group(void) {
    atomic_uchar *visits = calloc(LARGE_ITEMS, sizeof *visits);
    assert(visits);
    tsuzuri_task_parallel(large_item, visits, LARGE_ITEMS);
    for (unsigned index = 0; index < LARGE_ITEMS; ++index) assert(atomic_load(&visits[index]) == 1);
    free(visits);
}

/* A channel block as the generated code lays it out (src/llvm_sync.rs). */
struct block {
    uint64_t capacity;
    uint64_t item_size;
    uint64_t head;
    uint64_t count;
    uint64_t senders;
    uint64_t receivers;
    uint64_t owners;
    uint64_t reserved;
    uint64_t waiters[2];
};
_Static_assert(sizeof(struct block) == sizeof(struct tz_channel), "the runtime agrees with the lowering");

static struct block *make_channel(uint64_t capacity) {
    struct block *channel = calloc(1, sizeof *channel + capacity * sizeof(uint64_t));
    assert(channel);
    channel->capacity = capacity;
    channel->item_size = sizeof(uint64_t);
    channel->senders = 1;
    channel->receivers = 1;
    channel->owners = 2;
    return channel;
}

struct pipeline {
    struct block *channel;
    uint64_t items;
    int nested;
    _Atomic uint64_t sum;
    _Atomic int verdicts;
};

static void compute_item(void *context, uint64_t index) {
    (void)context;
    spin((unsigned)(index * 11) % 17);
}

static void stage(void *context, uint64_t index) {
    struct pipeline *pipeline = context;
    if (pipeline->nested && (index & 1)) tsuzuri_task_parallel(compute_item, NULL, 4);
    if (index == 0) {
        for (uint64_t value = 1; value <= pipeline->items; ++value) {
            if (tsuzuri_channel_send(pipeline->channel, &value) != 0) {
                atomic_fetch_add(&pipeline->verdicts, 1);
                break;
            }
        }
        tsuzuri_channel_close(pipeline->channel, 0);
        return;
    }
    for (;;) {
        uint64_t value = 0;
        int32_t status = tsuzuri_channel_recv(pipeline->channel, &value);
        if (status == 1) break;
        if (status != 0) {
            atomic_fetch_add(&pipeline->verdicts, 1);
            break;
        }
        atomic_fetch_add(&pipeline->sum, value);
    }
}

static unsigned hosts = 4, rounds = 40;
static atomic_uint verdict_groups;

static void pipeline_group(unsigned seed) {
    struct pipeline pipeline = { make_channel(1 + (seed >> 8) % 3), 20 + (seed >> 12) % 60, (int)((seed >> 20) & 1), 0, 0 };
    unsigned length = 2 + (seed >> 4) % 4;
    tsuzuri_task_parallel(stage, &pipeline, length);
    if (atomic_load(&pipeline.verdicts) == 0) {
        assert(atomic_load(&pipeline.sum) == pipeline.items * (pipeline.items + 1) / 2);
    } else {
        atomic_fetch_add(&verdict_groups, 1);
    }
    free(pipeline.channel);
}

static void *host(void *argument) {
    unsigned seed = (unsigned)(uintptr_t)argument * 2654435761u + 1u;
    for (unsigned round = 0; round < rounds; ++round) {
        seed = seed * 1103515245u + 12345u;
        nested_groups(seed);
        seed = seed * 1103515245u + 12345u;
        failing_group(seed);
        seed = seed * 1103515245u + 12345u;
        if (round % 8 == 0) large_group();
        pipeline_group(seed);
    }
    return NULL;
}

int main(int argc, char **argv) {
    if (argc > 1) processors = atol(argv[1]);
    if (argc > 2) hosts = (unsigned)atoi(argv[2]);
    if (argc > 3) rounds = (unsigned)atoi(argv[3]);
    assert(hosts >= 1 && hosts <= 16 && rounds >= 1);
    alarm(240);
    pthread_t threads[16];
    for (uintptr_t index = 0; index < hosts; ++index) assert(pthread_create(&threads[index], NULL, host, (void *)index) == 0);
    for (unsigned index = 0; index < hosts; ++index) assert(pthread_join(threads[index], NULL) == 0);
    assert(tz_task_pool.groups == NULL && tz_task_pool.tail == NULL);
    tz_task_shutdown();
    printf("task_stress: cpus=%ld hosts=%u rounds=%u verdict_groups=%u passed\n", processors, hosts, rounds, atomic_load(&verdict_groups));
    return 0;
}
