/* F10: the lock of Mutex.with_lock in src/runtime/task.c, driven from C. The tests use condition
   variables, never time: a thread that must wait is seen waiting through the observed wait, and a
   thread that must be released is released by the test. Built by tests/tasks.mjs at -O0 and -O3, and
   under ThreadSanitizer when TSUZURI_TSAN=1. */
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

static pthread_mutex_t observation = PTHREAD_MUTEX_INITIALIZER;
static pthread_cond_t observed = PTHREAD_COND_INITIALIZER;
static pthread_cond_t *lock_condition;
static unsigned parked;

/* A thread waiting for the lock enters this wait; the test learns that it did. */
static int observed_wait(pthread_cond_t *condition, pthread_mutex_t *mutex) {
    if (condition == lock_condition) {
        assert(pthread_mutex_lock(&observation) == 0);
        ++parked;
        assert(pthread_cond_broadcast(&observed) == 0);
        assert(pthread_mutex_unlock(&observation) == 0);
    }
    return pthread_cond_wait(condition, mutex);
}

#define TZ_TASK_COND_WAIT observed_wait
#include "../src/runtime/task.c"

static void wait_until_parked(unsigned count) {
    assert(pthread_mutex_lock(&observation) == 0);
    while (parked < count) assert(pthread_cond_wait(&observed, &observation) == 0);
    assert(pthread_mutex_unlock(&observation) == 0);
}

/* The cell of a Mutex: the lock word, then the value. */
struct cell {
    atomic_uint word;
    uint64_t value;
};

enum { THREADS = 8, ROUNDS = 10000 };
static struct cell shared;

static void *count(void *argument) {
    (void)argument;
    for (int round = 0; round < ROUNDS; ++round) {
        assert(tsuzuri_mutex_lock(&shared) == 0);
        /* A plain increment: only the lock keeps the threads from losing updates. */
        shared.value = shared.value + 1;
        tsuzuri_mutex_unlock(&shared);
    }
    return NULL;
}

static void exclusion(void) {
    memset(&shared, 0, sizeof shared);
    pthread_t threads[THREADS];
    for (int index = 0; index < THREADS; ++index) assert(pthread_create(&threads[index], NULL, count, NULL) == 0);
    for (int index = 0; index < THREADS; ++index) assert(pthread_join(threads[index], NULL) == 0);
    assert(shared.value == (uint64_t)THREADS * ROUNDS);
    assert(atomic_load(&shared.word) == 0);
}

static atomic_uint acquired;
static struct cell guarded;

static void *acquire_once(void *argument) {
    (void)argument;
    assert(tsuzuri_mutex_lock(&guarded) == 0);
    atomic_fetch_add(&acquired, 1);
    tsuzuri_mutex_unlock(&guarded);
    return NULL;
}

/* B asks for the lock A holds and waits for it; A lets go; B gets it. */
static void handshake(void) {
    memset(&guarded, 0, sizeof guarded);
    atomic_store(&acquired, 0);
    lock_condition = &tz_mutex_wake;
    parked = 0;
    assert(tsuzuri_mutex_lock(&guarded) == 0);
    pthread_t waiter;
    assert(pthread_create(&waiter, NULL, acquire_once, NULL) == 0);
    wait_until_parked(1);
    /* The waiter parked inside the lock call, so it has not acquired the lock. */
    assert(atomic_load(&acquired) == 0);
    assert(atomic_load(&guarded.word) == 2);
    tsuzuri_mutex_unlock(&guarded);
    assert(pthread_join(waiter, NULL) == 0);
    assert(atomic_load(&acquired) == 1);
    assert(atomic_load(&guarded.word) == 0);
}

/* Several waiters on one lock: every one gets it, and a broadcast loses none of them. */
static void waiters(void) {
    memset(&guarded, 0, sizeof guarded);
    atomic_store(&acquired, 0);
    lock_condition = &tz_mutex_wake;
    parked = 0;
    assert(tsuzuri_mutex_lock(&guarded) == 0);
    pthread_t threads[THREADS];
    for (int index = 0; index < THREADS; ++index) assert(pthread_create(&threads[index], NULL, acquire_once, NULL) == 0);
    wait_until_parked(THREADS);
    tsuzuri_mutex_unlock(&guarded);
    for (int index = 0; index < THREADS; ++index) assert(pthread_join(threads[index], NULL) == 0);
    assert(atomic_load(&acquired) == THREADS);
}

/* Two mutexes are independent: a thread holds one while another thread takes the other. */
static struct cell first, second;
static atomic_uint second_taken;

static void *take_second(void *argument) {
    (void)argument;
    assert(tsuzuri_mutex_lock(&second) == 0);
    atomic_store(&second_taken, 1);
    tsuzuri_mutex_unlock(&second);
    return NULL;
}

static void independent(void) {
    memset(&first, 0, sizeof first);
    memset(&second, 0, sizeof second);
    atomic_store(&second_taken, 0);
    assert(tsuzuri_mutex_lock(&first) == 0);
    pthread_t thread;
    assert(pthread_create(&thread, NULL, take_second, NULL) == 0);
    assert(pthread_join(thread, NULL) == 0);
    assert(atomic_load(&second_taken) == 1);
    tsuzuri_mutex_unlock(&first);
}

/* What the runtime says to stderr is what a user reads; read it back through a pipe. */
static char *capture(int (*action)(void), int *result) {
    static char text[512];
    int channel[2];
    assert(pipe(channel) == 0);
    fflush(stderr);
    int saved = dup(2);
    assert(saved >= 0 && dup2(channel[1], 2) >= 0);
    *result = action();
    fflush(stderr);
    assert(dup2(saved, 2) >= 0);
    close(saved);
    close(channel[1]);
    ssize_t length = read(channel[0], text, sizeof text - 1);
    close(channel[0]);
    text[length < 0 ? 0 : length] = '\0';
    return text;
}

static struct cell outer, inner;

static int nested(void) {
    return tsuzuri_mutex_lock(&inner);
}

static int parallel(void) {
    return tsuzuri_mutex_parallel_ok();
}

static void refusals(void) {
    memset(&outer, 0, sizeof outer);
    memset(&inner, 0, sizeof inner);
    int result;
    char *text = capture(parallel, &result);
    assert(result == 1 && text[0] == '\0');
    assert(tsuzuri_mutex_lock(&outer) == 0);
    text = capture(nested, &result);
    assert(result == 1);
    assert(strstr(text, "Tsuzuri runtime: Mutex.with_lock cannot be nested; release the outer mutex first\n") == text);
    /* A refused lock changes nothing: the inner word is free, the outer is still held. */
    assert(atomic_load(&inner.word) == 0 && atomic_load(&outer.word) != 0);
    text = capture(parallel, &result);
    assert(result == 0);
    assert(strstr(text, "Tsuzuri runtime: parallel work cannot start inside Mutex.with_lock; move it outside the critical section\n") == text);
    tsuzuri_mutex_unlock(&outer);
    assert(atomic_load(&outer.word) == 0);
    text = capture(parallel, &result);
    assert(result == 1 && text[0] == '\0');
    assert(tsuzuri_mutex_lock(&inner) == 0);
    tsuzuri_mutex_unlock(&inner);
}

/* The trap boundary releases the lock of a thread whose call trapped (src/runtime/trap.c calls this). */
static struct cell trapped;

static void *wait_for_abandoned(void *argument) {
    (void)argument;
    assert(tsuzuri_mutex_lock(&trapped) == 0);
    atomic_fetch_add(&acquired, 1);
    tsuzuri_mutex_unlock(&trapped);
    return NULL;
}

static void abandonment(void) {
    memset(&trapped, 0, sizeof trapped);
    atomic_store(&acquired, 0);
    assert(tsuzuri_sync_hooks.abandon == tz_mutex_abandon);
    /* Without a lock held the hook does nothing. */
    tsuzuri_sync_hooks.abandon();
    assert(atomic_load(&trapped.word) == 0);
    lock_condition = &tz_mutex_wake;
    parked = 0;
    assert(tsuzuri_mutex_lock(&trapped) == 0);
    pthread_t waiter;
    assert(pthread_create(&waiter, NULL, wait_for_abandoned, NULL) == 0);
    wait_until_parked(1);
    /* The trap: the callback never returns to unlock, the boundary abandons the call instead. */
    tsuzuri_sync_hooks.abandon();
    assert(pthread_join(waiter, NULL) == 0);
    assert(atomic_load(&acquired) == 1 && atomic_load(&trapped.word) == 0);
    /* The thread forgot the lock, so it can lock again. */
    assert(tsuzuri_mutex_lock(&trapped) == 0);
    tsuzuri_mutex_unlock(&trapped);
    assert(tsuzuri_mutex_parallel_ok() == 1);
}

int main(void) {
    exclusion();
    handshake();
    waiters();
    independent();
    refusals();
    abandonment();
    puts("sync_runtime: exclusion, parking, refusals and abandonment passed");
    return 0;
}
