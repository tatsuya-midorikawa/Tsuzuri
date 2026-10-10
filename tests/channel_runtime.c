/* F10 Phase 2: the channel runtime in src/runtime/task.c, driven from C. The tests use condition
   variables, never time: a thread that must wait is seen waiting through the observed wait, and a
   thread that must be released is released by the test. The CPU count of the pool is an argument,
   because the pool reads it once: tests/tasks.mjs runs this at one, two and four threads, at -O0
   and -O3, and under ThreadSanitizer and AddressSanitizer when TSUZURI_TSAN=1 or TSUZURI_ASAN=1.
   An alarm turns a hang, which a lost wakeup would be, into a failure. */
/* The checks are assertions: some toolchains define NDEBUG for optimized builds. */
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

static pthread_mutex_t observation = PTHREAD_MUTEX_INITIALIZER;
static pthread_cond_t observed = PTHREAD_COND_INITIALIZER;
static unsigned parked;

static int observed_wait(pthread_cond_t *condition, pthread_mutex_t *mutex);

#define TZ_TASK_SYSCONF test_sysconf
#define TZ_TASK_COND_WAIT observed_wait
#include "../src/runtime/task.c"

/* A thread waiting on a channel enters this wait, on a condition of its own; the test learns that
   it did. The workers wait on the pool's conditions and the lock on its own, which are not counted. */
static int observed_wait(pthread_cond_t *condition, pthread_mutex_t *mutex) {
    if (condition != &tz_task_pool.work_available && condition != &tz_task_pool.work_done
        && condition != &tz_mutex_wake) {
        assert(pthread_mutex_lock(&observation) == 0);
        ++parked;
        assert(pthread_cond_broadcast(&observed) == 0);
        assert(pthread_mutex_unlock(&observation) == 0);
    }
    return pthread_cond_wait(condition, mutex);
}

static void wait_until_parked(unsigned count) {
    assert(pthread_mutex_lock(&observation) == 0);
    while (parked < count) assert(pthread_cond_wait(&observed, &observation) == 0);
    assert(pthread_mutex_unlock(&observation) == 0);
}

static void forget_parked(void) {
    assert(pthread_mutex_lock(&observation) == 0);
    parked = 0;
    assert(pthread_mutex_unlock(&observation) == 0);
}

/* The block of a channel as the generated code lays it out (src/llvm_sync.rs). */
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
_Static_assert(sizeof(struct block) == 80, "the items start at offset 80");
_Static_assert(sizeof(struct block) == sizeof(struct tz_channel), "the runtime agrees with the lowering");

static struct block *make(uint64_t capacity, uint64_t item_size) {
    uint64_t stride = item_size ? item_size : 1;
    struct block *channel = calloc(1, sizeof *channel + capacity * stride);
    assert(channel);
    channel->capacity = capacity;
    channel->item_size = item_size;
    channel->senders = 1;
    channel->receivers = 1;
    channel->owners = 2;
    return channel;
}

static int32_t send_value(struct block *channel, uint64_t value) {
    return tsuzuri_channel_send(channel, &value);
}

/* The item that came out, or UINT64_MAX with the status in `*status`. */
static uint64_t receive_value(struct block *channel, int32_t *status) {
    uint64_t value = UINT64_MAX;
    *status = tsuzuri_channel_recv(channel, &value);
    return value;
}

/* The test thread stands for a thread that runs Tsuzuri code: in a real program, the one that submits
   a group is counted as running while it runs items, and a worker is counted from the moment that it
   takes an item. A thread that waits for a join runs nothing, and is counted out. */
static void main_active(int active) {
    tz_task_lock();
    if (active && !tz_task_counted) {
        ++tz_task_pool.busy;
        tz_task_counted = 1;
    } else if (!active && tz_task_counted) {
        --tz_task_pool.busy;
        tz_task_counted = 0;
        tz_channel_check_deadlock();
    }
    tz_task_unlock();
}

/* The claim of threads that the test is about to create, as a worker's claim of an item. */
static void claim(int count) {
    tz_task_lock();
    tz_task_pool.busy += (uint64_t)count;
    tz_task_unlock();
}

/* The first and last lines of a thread whose claim was made for it. */
static void adopt(void) {
    tz_task_counted = 1;
}

static void finish(void) {
    tz_task_lock();
    tz_task_counted = 0;
    --tz_task_pool.busy;
    tz_channel_check_deadlock();
    tz_task_unlock();
}

/* With every thread outside the pool at rest, only the test thread counts as running, and nobody
   waits. */
static void quiescent(void) {
    assert(tz_task_pool.waiters == NULL);
    assert(tz_task_pool.busy == 1);
}

static void ring(void) {
    struct block *channel = make(3, sizeof(uint64_t));
    int32_t status;
    for (uint64_t value = 1; value <= 3; ++value) assert(send_value(channel, value) == 0);
    assert(channel->count == 3);
    assert(receive_value(channel, &status) == 1 && status == 0);
    assert(send_value(channel, 4) == 0);
    /* The ring wraps: the oldest item is always first, whatever the slot. */
    for (uint64_t value = 2; value <= 4; ++value) assert(receive_value(channel, &status) == value && status == 0);
    for (uint64_t round = 0; round < 10; ++round) {
        assert(send_value(channel, 100 + round) == 0);
        assert(receive_value(channel, &status) == 100 + round && status == 0);
    }
    assert(channel->count == 0);
    assert(tsuzuri_channel_close(channel, 0) == 0);
    assert(tsuzuri_channel_close(channel, 1) == 1);
    free(channel);
    quiescent();
}

static void closing(void) {
    struct block *channel = make(4, sizeof(uint64_t));
    int32_t status;
    tsuzuri_channel_clone_sender(channel);
    assert(channel->senders == 2 && channel->owners == 3);
    assert(send_value(channel, 7) == 0);
    assert(tsuzuri_channel_close(channel, 0) == 0);
    /* One sender is left: the channel is not closed. */
    assert(channel->senders == 1);
    assert(tsuzuri_channel_close(channel, 0) == 0);
    /* The item that was sent is still there, then the channel says it is closed, every time. */
    assert(receive_value(channel, &status) == 7 && status == 0);
    receive_value(channel, &status);
    assert(status == 1);
    receive_value(channel, &status);
    assert(status == 1);
    /* No receiver: a send is refused and the item stays with the caller. */
    assert(tsuzuri_channel_close(channel, 1) == 1);
    free(channel);
    channel = make(2, sizeof(uint64_t));
    assert(tsuzuri_channel_close(channel, 1) == 0);
    assert(send_value(channel, 9) == 1);
    assert(channel->count == 0);
    assert(tsuzuri_channel_close(channel, 0) == 1);
    free(channel);
    quiescent();
}

/* A channel of zero-sized items counts, and moves nothing. */
static void signals(void) {
    struct block *channel = make(2, 0);
    int32_t status;
    assert(tsuzuri_channel_send(channel, NULL) == 0);
    assert(tsuzuri_channel_send(channel, NULL) == 0);
    assert(channel->count == 2);
    assert(tsuzuri_channel_recv(channel, NULL) == 0);
    assert(tsuzuri_channel_recv(channel, NULL) == 0);
    assert(tsuzuri_channel_close(channel, 0) == 0);
    (void)receive_value(channel, &status);
    assert(status == 1);
    assert(tsuzuri_channel_close(channel, 1) == 1);
    free(channel);
    quiescent();
}

struct party {
    struct block *channel;
    struct block *other;
    int32_t status;
    uint64_t value;
};

static void *receive_one(void *argument) {
    struct party *party = argument;
    adopt();
    party->value = receive_value(party->channel, &party->status);
    finish();
    return NULL;
}

static void *send_one(void *argument) {
    struct party *party = argument;
    adopt();
    party->status = send_value(party->channel, party->value);
    finish();
    return NULL;
}

/* A receiver waits on an empty channel; a send wakes it with the item. */
static void blocked_receiver(void) {
    struct block *channel = make(2, sizeof(uint64_t));
    struct party party = { channel, NULL, -1, 0 };
    forget_parked();
    claim(1);
    pthread_t thread;
    assert(pthread_create(&thread, NULL, receive_one, &party) == 0);
    wait_until_parked(1);
    assert(party.status == -1);
    assert(send_value(channel, 77) == 0);
    assert(pthread_join(thread, NULL) == 0);
    assert(party.status == 0 && party.value == 77);
    assert(tsuzuri_channel_close(channel, 0) == 0);
    assert(tsuzuri_channel_close(channel, 1) == 1);
    free(channel);
    quiescent();
}

/* A sender waits on a full channel; a receive makes room, and the item that waited goes in last. */
static void blocked_sender(void) {
    struct block *channel = make(1, sizeof(uint64_t));
    int32_t status;
    assert(send_value(channel, 1) == 0);
    struct party party = { channel, NULL, -1, 2 };
    forget_parked();
    claim(1);
    pthread_t thread;
    assert(pthread_create(&thread, NULL, send_one, &party) == 0);
    wait_until_parked(1);
    assert(party.status == -1 && channel->count == 1);
    assert(receive_value(channel, &status) == 1 && status == 0);
    assert(pthread_join(thread, NULL) == 0);
    assert(party.status == 0);
    assert(receive_value(channel, &status) == 2 && status == 0);
    assert(tsuzuri_channel_close(channel, 0) == 0);
    assert(tsuzuri_channel_close(channel, 1) == 1);
    free(channel);
    quiescent();
}

enum { WAITERS = 4 };

/* The last sender closes the channel: every receiver that waits for an item gets "closed". */
static void closing_wakes_receivers(void) {
    struct block *channel = make(2, sizeof(uint64_t));
    struct party parties[WAITERS];
    pthread_t threads[WAITERS];
    forget_parked();
    claim(WAITERS);
    for (int index = 0; index < WAITERS; ++index) {
        parties[index] = (struct party){ channel, NULL, -1, 0 };
        assert(pthread_create(&threads[index], NULL, receive_one, &parties[index]) == 0);
    }
    wait_until_parked(WAITERS);
    assert(tsuzuri_channel_close(channel, 0) == 0);
    for (int index = 0; index < WAITERS; ++index) {
        assert(pthread_join(threads[index], NULL) == 0);
        assert(parties[index].status == 1);
    }
    assert(tsuzuri_channel_close(channel, 1) == 1);
    free(channel);
    quiescent();
}

/* The last receiver goes: every sender that waits for room gives up and keeps its item. */
static void dropping_the_receiver_wakes_senders(void) {
    struct block *channel = make(1, sizeof(uint64_t));
    assert(send_value(channel, 1) == 0);
    struct party parties[WAITERS];
    pthread_t threads[WAITERS];
    forget_parked();
    claim(WAITERS);
    for (int index = 0; index < WAITERS; ++index) {
        parties[index] = (struct party){ channel, NULL, -1, (uint64_t)index };
        assert(pthread_create(&threads[index], NULL, send_one, &parties[index]) == 0);
    }
    wait_until_parked(WAITERS);
    assert(tsuzuri_channel_close(channel, 1) == 0);
    for (int index = 0; index < WAITERS; ++index) {
        assert(pthread_join(threads[index], NULL) == 0);
        assert(parties[index].status == 1);
    }
    assert(channel->count == 1);
    assert(tsuzuri_channel_close(channel, 0) == 1);
    free(channel);
    quiescent();
}

/* What the runtime says to stderr is what a user reads; read it back through a pipe. */
static char *capture(void (*action)(void)) {
    static char text[1024];
    int pipes[2];
    assert(pipe(pipes) == 0);
    fflush(stderr);
    int saved = dup(2);
    assert(saved >= 0 && dup2(pipes[1], 2) >= 0);
    action();
    fflush(stderr);
    assert(dup2(saved, 2) >= 0);
    close(saved);
    close(pipes[1]);
    ssize_t length = read(pipes[0], text, sizeof text - 1);
    close(pipes[0]);
    text[length < 0 ? 0 : length] = '\0';
    return text;
}

#define DEADLOCK "Tsuzuri runtime: deadlock: every task is waiting on a channel\n"

static void alone_empty(void) {
    struct block *channel = make(1, sizeof(uint64_t));
    int32_t status;
    /* The sender is alive and nobody else can send: the only thread would wait for ever. */
    (void)receive_value(channel, &status);
    assert(status == 2);
    /* The verdict does not poison the channel or the pool: the thread can go on. */
    assert(send_value(channel, 3) == 0);
    assert(receive_value(channel, &status) == 3 && status == 0);
    free(channel);
}

static void alone_full(void) {
    struct block *channel = make(1, sizeof(uint64_t));
    int32_t status;
    assert(send_value(channel, 1) == 0);
    assert(send_value(channel, 2) == 2);
    assert(receive_value(channel, &status) == 1 && status == 0);
    assert(send_value(channel, 3) == 0);
    free(channel);
}

static void deadlock_alone(void) {
    char *text = capture(alone_empty);
    assert(strcmp(text, DEADLOCK) == 0);
    quiescent();
    text = capture(alone_full);
    assert(strcmp(text, DEADLOCK) == 0);
    quiescent();
}

/* A thread at the depth limit does not run an item, and with no worker free an item that nobody
   started cannot be taken by anyone, so it must not keep the verdict from coming. The group here
   is a stand-in that is never run. The pool has not started, so no worker is free. */
static int32_t unstarted_status;

static void unstarted_run(void) {
    struct block *channel = make(1, sizeof(uint64_t));
    uint64_t value;
    unstarted_status = tsuzuri_channel_recv(channel, &value);
    free(channel);
}

static void unstarted_without_a_free_worker(void) {
    struct tz_task_group group = { .length = 1, .remaining = 1 };
    tz_task_lock();
    assert(tz_task_pool.idle == 0 && tz_task_pool.groups == NULL);
    tz_task_pool.groups = tz_task_pool.tail = &group;
    unsigned depth = tz_task_help_depth;
    tz_task_help_depth = TZ_CHANNEL_HELP_DEPTH;
    tz_task_unlock();
    unstarted_status = -1;
    char *text = capture(unstarted_run);
    tz_task_lock();
    tz_task_pool.groups = tz_task_pool.tail = NULL;
    tz_task_help_depth = depth;
    tz_task_unlock();
    assert(unstarted_status == 2);
    assert(strcmp(text, DEADLOCK) == 0);
    assert(group.next == 0);
    quiescent();
}

static struct block *first_channel, *second_channel;

static void *wait_on_first(void *argument) {
    struct party *party = argument;
    adopt();
    party->value = receive_value(first_channel, &party->status);
    finish();
    return NULL;
}

static void *wait_on_second(void *argument) {
    struct party *party = argument;
    adopt();
    party->value = receive_value(second_channel, &party->status);
    finish();
    return NULL;
}

static struct party left, right;

/* Two threads wait, each for the other to send; the test thread, which waits for them to join,
   runs nothing, so no thread runs and both are woken with the verdict. */
static void deadlock_pair_run(void) {
    first_channel = make(1, sizeof(uint64_t));
    second_channel = make(1, sizeof(uint64_t));
    left = (struct party){ NULL, NULL, -1, 0 };
    right = (struct party){ NULL, NULL, -1, 0 };
    forget_parked();
    claim(2);
    pthread_t one, two;
    assert(pthread_create(&one, NULL, wait_on_first, &left) == 0);
    assert(pthread_create(&two, NULL, wait_on_second, &right) == 0);
    wait_until_parked(2);
    main_active(0);
    assert(pthread_join(one, NULL) == 0);
    assert(pthread_join(two, NULL) == 0);
    main_active(1);
}

static void deadlock_pair(void) {
    char *text = capture(deadlock_pair_run);
    assert(left.status == 2 && right.status == 2);
    assert(strcmp(text, DEADLOCK) == 0);
    quiescent();
    /* Another round on the same channels works: nothing is left of the verdict. */
    assert(send_value(first_channel, 5) == 0);
    int32_t status;
    assert(receive_value(first_channel, &status) == 5 && status == 0);
    free(first_channel);
    free(second_channel);
}

/* Two threads hand a counter to each other through two channels of one slot. A wakeup that was
   counted late would make the thread that parks last see "every task is waiting"; it never does. */
enum { ROUNDS = 20000 };
static struct block *forward, *backward;
static atomic_uint failures;

static void *ping(void *argument) {
    (void)argument;
    adopt();
    for (uint64_t round = 0; round < ROUNDS; ++round) {
        int32_t status;
        if (send_value(forward, round) != 0) atomic_fetch_add(&failures, 1);
        uint64_t answer = receive_value(backward, &status);
        if (status != 0 || answer != round + 1) atomic_fetch_add(&failures, 1);
    }
    finish();
    return NULL;
}

static void *pong(void *argument) {
    (void)argument;
    adopt();
    for (uint64_t round = 0; round < ROUNDS; ++round) {
        int32_t status;
        uint64_t value = receive_value(forward, &status);
        if (status != 0 || value != round) atomic_fetch_add(&failures, 1);
        if (send_value(backward, value + 1) != 0) atomic_fetch_add(&failures, 1);
    }
    finish();
    return NULL;
}

static void ping_pong(void) {
    forward = make(1, sizeof(uint64_t));
    backward = make(1, sizeof(uint64_t));
    atomic_store(&failures, 0);
    claim(2);
    main_active(0);
    pthread_t one, two;
    assert(pthread_create(&one, NULL, ping, NULL) == 0);
    assert(pthread_create(&two, NULL, pong, NULL) == 0);
    assert(pthread_join(one, NULL) == 0);
    assert(pthread_join(two, NULL) == 0);
    main_active(1);
    assert(atomic_load(&failures) == 0);
    free(forward);
    free(backward);
    quiescent();
}

/* Many senders and many receivers share one channel: every item arrives once, and the receivers end
   when the last sender closes. */
enum { PRODUCERS = 4, CONSUMERS = 4, EACH = 5000 };
static struct block *shared;
static atomic_uchar seen[PRODUCERS * EACH];
static atomic_uint received_items;

static void *produce(void *argument) {
    uint64_t producer = (uint64_t)(uintptr_t)argument;
    adopt();
    for (uint64_t index = 0; index < EACH; ++index) {
        if (send_value(shared, producer * EACH + index) != 0) atomic_fetch_add(&failures, 1);
    }
    /* This thread's own sender. */
    tsuzuri_channel_close(shared, 0);
    finish();
    return NULL;
}

static void *consume(void *argument) {
    (void)argument;
    adopt();
    for (;;) {
        int32_t status;
        uint64_t item = receive_value(shared, &status);
        if (status == 1) break;
        if (status != 0 || item >= PRODUCERS * EACH || atomic_exchange(&seen[item], 1) != 0) {
            atomic_fetch_add(&failures, 1);
            break;
        }
        atomic_fetch_add(&received_items, 1);
    }
    finish();
    return NULL;
}

static void many_to_many(void) {
    shared = make(3, sizeof(uint64_t));
    atomic_store(&failures, 0);
    atomic_store(&received_items, 0);
    memset(seen, 0, sizeof seen);
    for (int index = 0; index < PRODUCERS; ++index) tsuzuri_channel_clone_sender(shared);
    /* The original sender goes: the clones are the producers'. */
    assert(tsuzuri_channel_close(shared, 0) == 0);
    pthread_t producers[PRODUCERS], consumers[CONSUMERS];
    claim(PRODUCERS + CONSUMERS);
    main_active(0);
    for (int index = 0; index < CONSUMERS; ++index) assert(pthread_create(&consumers[index], NULL, consume, NULL) == 0);
    for (int index = 0; index < PRODUCERS; ++index) assert(pthread_create(&producers[index], NULL, produce, (void *)(uintptr_t)index) == 0);
    for (int index = 0; index < PRODUCERS; ++index) assert(pthread_join(producers[index], NULL) == 0);
    for (int index = 0; index < CONSUMERS; ++index) assert(pthread_join(consumers[index], NULL) == 0);
    main_active(1);
    assert(atomic_load(&failures) == 0);
    assert(atomic_load(&received_items) == PRODUCERS * EACH);
    assert(tsuzuri_channel_close(shared, 1) == 1);
    free(shared);
    quiescent();
}

static int wait_status;

static void check_wait_ok(void) {
    wait_status = tsuzuri_mutex_wait_ok();
}

static void guarded(void) {
    struct { atomic_uint word; uint64_t value; } cell = { 0, 0 };
    assert(tsuzuri_mutex_wait_ok() == 1);
    assert(tsuzuri_mutex_lock(&cell) == 0);
    /* A channel operation may wait, which a critical section never does. */
    wait_status = 1;
    char *text = capture(check_wait_ok);
    assert(wait_status == 0);
    assert(strcmp(text, "Tsuzuri runtime: a Channel operation may wait; move it outside Mutex.with_lock\n") == 0);
    tsuzuri_mutex_unlock(&cell);
    assert(tsuzuri_mutex_wait_ok() == 1);
}

/* The items of a group that run channel operations: they wait and wake as threads do, and a thread
   that waits runs an item that nobody started only when no worker is free to. */
struct pipeline {
    struct block *channel;
    uint64_t items;
    uint64_t sum;
    int32_t status[2];
};

static void produce_item(void *context, uint64_t index) {
    struct pipeline *pipeline = context;
    if (index == 0) {
        for (uint64_t value = 1; value <= pipeline->items; ++value) {
            int32_t status = send_value(pipeline->channel, value);
            if (status != 0) { pipeline->status[0] = status; return; }
        }
        tsuzuri_channel_close(pipeline->channel, 0);
        pipeline->status[0] = 0;
    } else {
        for (;;) {
            int32_t status;
            uint64_t value = receive_value(pipeline->channel, &status);
            if (status == 1) { pipeline->status[1] = 0; return; }
            if (status != 0) { pipeline->status[1] = status; return; }
            pipeline->sum += value;
        }
    }
}

static void pipeline_run(void) {
    struct pipeline pipeline = { make(2, sizeof(uint64_t)), 2000, 0, { -1, -1 } };
    tsuzuri_task_parallel(produce_item, &pipeline, 2);
    if (processors >= 2) {
        /* A thread each: the producer waits for room, the consumer for items, and both finish. */
        assert(pipeline.status[0] == 0 && pipeline.status[1] == 0);
        assert(pipeline.sum == pipeline.items * (pipeline.items + 1) / 2);
    } else {
        /* One thread runs the items in order: the producer fills the two slots and waits for ever,
           so it is told that every task is waiting; the consumer then drains what is there and is
           told the same, because the producer's sender is alive. */
        assert(pipeline.status[0] == 2);
        assert(pipeline.status[1] == 2);
        assert(pipeline.sum == 3);
    }
    free(pipeline.channel);
}

static void pipeline_group(void) {
    char *text = capture(pipeline_run);
    if (processors >= 2) assert(text[0] == '\0');
    else assert(strcmp(text, DEADLOCK DEADLOCK) == 0);
}

/* A group in which every item waits for another that will never send: all of them are woken with
   the verdict, the group completes, and the pool is usable again. */
struct stuck {
    struct block *channels[3];
    atomic_int status[3];
};

static void stuck_item(void *context, uint64_t index) {
    struct stuck *stuck = context;
    uint64_t value;
    atomic_store(&stuck->status[index], tsuzuri_channel_recv(stuck->channels[index], &value));
}

static void stuck_run(void) {
    struct stuck stuck;
    for (int index = 0; index < 3; ++index) {
        stuck.channels[index] = make(1, sizeof(uint64_t));
        atomic_store(&stuck.status[index], -1);
    }
    tsuzuri_task_parallel(stuck_item, &stuck, processors >= 3 ? 3 : 2);
    for (int index = 0; index < (processors >= 3 ? 3 : 2); ++index) assert(atomic_load(&stuck.status[index]) == 2);
    for (int index = 0; index < 3; ++index) free(stuck.channels[index]);
}

static void stuck_group(void) {
    char *text = capture(stuck_run);
    /* One verdict per time that nothing can run: with fewer threads than items the later items start
       after the earlier ones were woken, so there can be more than one line, but never none. */
    assert(strncmp(text, DEADLOCK, sizeof DEADLOCK - 1) == 0);
    /* The pool still runs groups. */
    struct pipeline pipeline = { make(2, sizeof(uint64_t)), 10, 0, { -1, -1 } };
    if (processors >= 2) {
        tsuzuri_task_parallel(produce_item, &pipeline, 2);
        assert(pipeline.status[0] == 0 && pipeline.status[1] == 0 && pipeline.sum == 55);
    }
    free(pipeline.channel);
}

/* Three items, two threads: one thread is held by an item that waits for the test, the other waits
   for a channel that only the third item fills, and runs that item itself because no worker is
   free. Without the help, the waiting thread would wait for ever. */
struct helping {
    struct block *channel;
    atomic_uint started;
    pthread_mutex_t lock;
    pthread_cond_t changed;
    int blocker_ready;
    int released;
    uint64_t received;
};

static void helping_item(void *context, uint64_t index) {
    (void)index;
    struct helping *helping = context;
    unsigned role = atomic_fetch_add(&helping->started, 1);
    if (role == 0) {
        assert(pthread_mutex_lock(&helping->lock) == 0);
        helping->blocker_ready = 1;
        assert(pthread_cond_broadcast(&helping->changed) == 0);
        while (!helping->released) assert(pthread_cond_wait(&helping->changed, &helping->lock) == 0);
        assert(pthread_mutex_unlock(&helping->lock) == 0);
    } else if (role == 1) {
        assert(pthread_mutex_lock(&helping->lock) == 0);
        while (!helping->blocker_ready) assert(pthread_cond_wait(&helping->changed, &helping->lock) == 0);
        assert(pthread_mutex_unlock(&helping->lock) == 0);
        int32_t status;
        helping->received = receive_value(helping->channel, &status);
        assert(status == 0);
        assert(pthread_mutex_lock(&helping->lock) == 0);
        helping->released = 1;
        assert(pthread_cond_broadcast(&helping->changed) == 0);
        assert(pthread_mutex_unlock(&helping->lock) == 0);
    } else {
        assert(send_value(helping->channel, 42) == 0);
    }
}

static void helping(void) {
    struct helping context = { make(1, sizeof(uint64_t)), 0, PTHREAD_MUTEX_INITIALIZER, PTHREAD_COND_INITIALIZER, 0, 0, 0 };
    tsuzuri_task_parallel(helping_item, &context, 3);
    assert(context.received == 42);
    assert(atomic_load(&context.started) == 3);
    free(context.channel);
}

/* `channel_runtime <threads> -v` names each test before it runs, to see where a hang is. */
static int verbose;

#define RUN(test) do { if (verbose) { printf("%s\n", #test); fflush(stdout); } test(); } while (0)

int main(int argc, char **argv) {
    if (argc > 1) processors = atol(argv[1]);
    verbose = argc > 2 && strcmp(argv[2], "-v") == 0;
    assert(processors >= 1);
    alarm(120);
    /* The test thread runs Tsuzuri code, as the thread that runs `main` does. */
    main_active(1);
    RUN(ring);
    RUN(closing);
    RUN(signals);
    RUN(blocked_receiver);
    RUN(blocked_sender);
    RUN(closing_wakes_receivers);
    RUN(dropping_the_receiver_wakes_senders);
    RUN(deadlock_alone);
    RUN(unstarted_without_a_free_worker);
    RUN(deadlock_pair);
    RUN(ping_pong);
    RUN(many_to_many);
    RUN(guarded);
    RUN(pipeline_group);
    RUN(stuck_group);
    if (processors == 2) RUN(helping);
    printf("channel_runtime: %ld threads passed\n", processors);
    return 0;
}
