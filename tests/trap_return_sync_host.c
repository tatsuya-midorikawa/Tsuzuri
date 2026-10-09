/* C host of tests/trap_return_sync.mjs: calls the tsuzuri_try_<name> exports of
   tests/fixtures/trap_return_sync. A trap inside Mutex.with_lock must release the lock and clear the
   thread's state, and a group whose child traps while it holds the lock must finish: the children that
   wait for the lock get it, and the trap reaches the call that started the group. */
/* The checks are assertions: some toolchains define NDEBUG for optimized builds. */
#undef NDEBUG
#include <assert.h>
#include <stdatomic.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

#if defined(LINKED) || defined(COUNTING)
/* The host of a program linked from LLVM IR provides the trap runtime itself, here over a counting
   allocator so that every call can check that no block outlives it. COUNTING does the same for a
   shipped object, whose embedded runtime this one replaces. */
static atomic_long live_blocks;

static void *counted_malloc(size_t size) {
    void *pointer = malloc(size);
    if (pointer) atomic_fetch_add(&live_blocks, 1);
    return pointer;
}

static void counted_free(void *pointer) {
    if (!pointer) return;
    atomic_fetch_sub(&live_blocks, 1);
    free(pointer);
}

#define TZ_TRAP_MALLOC counted_malloc
#define TZ_TRAP_FREE counted_free
#define TZ_TRAP_API
#include "../src/runtime/trap.c"
#define LIVE_IS(count) assert(atomic_load(&live_blocks) == (count))
#else
#define LIVE_IS(count) ((void)0)
#endif

#include "trap_return_sync.h"

static void single_thread(int rounds) {
    tsuzuri_trap_info trap = { 0, 0 };
    int64_t result = -1;
    for (int round = 0; round < rounds; ++round) {
        assert(tsuzuri_try_lock_trap(&trap, &result, 5) == 0 && result == 20);
        LIVE_IS(0);
        result = -1;
        /* The division by zero inside the lock is caught by the boundary of this call... */
        assert(tsuzuri_try_lock_trap(&trap, &result, 0) == 1 && trap.site != 0 && result == -1);
        /* ... which freed what the call allocated, the mutex and its value included. */
        LIVE_IS(0);
        /* It also released the lock and forgot it, so this thread locks again (a lock still held would
           make the next call refuse to nest). */
        assert(tsuzuri_try_lock_after(&trap, &result) == 0 && result == 6);
        assert(tsuzuri_try_lock_again(&trap, &result) == 0 && result == 42);
        LIVE_IS(0);
    }
    /* The misuse traps are ordinary traps: a status, and the thread is usable afterwards. */
    for (int round = 0; round < 3; ++round) {
        assert(tsuzuri_try_nested_lock(&trap, &result) == 1 && trap.site != 0);
        LIVE_IS(0);
        assert(tsuzuri_try_lock_again(&trap, &result) == 0 && result == 42);
        assert(tsuzuri_try_start_inside(&trap, &result) == 1 && trap.site != 0);
        LIVE_IS(0);
        assert(tsuzuri_try_lock_again(&trap, &result) == 0 && result == 42);
    }
}

static void groups(int rounds) {
    tsuzuri_trap_info trap = { 0, 0 };
    int64_t result = -1;
    for (int round = 0; round < rounds; ++round) {
        int64_t count = 2 + round % 63;
        /* Without a trap every child adds 100 under the lock, and returns its index. */
        assert(tsuzuri_try_group_lock_trap(&trap, &result, count, -1) == 0);
        assert(result == count * 100 + count * (count - 1) / 2);
        LIVE_IS(0);
        /* With one: the child that traps holds the lock, the others are waiting for it. */
        int64_t bad = round % count;
        assert(tsuzuri_try_group_lock_trap(&trap, &result, count, bad) == 1 && trap.site != 0);
        LIVE_IS(0);
        assert(tsuzuri_try_lock_after(&trap, &result) == 0 && result == 6);
    }
}

int main(int argc, char **argv) {
    int rounds = argc > 1 ? atoi(argv[1]) : 300;
    single_thread(rounds);
    groups(rounds);
    printf("ok %d\n", rounds);
    return 0;
}
