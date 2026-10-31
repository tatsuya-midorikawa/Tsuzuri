/* A host of failure domains (B10 Phase 2): tests/fixtures/domain_host built with --emit llvm --trap-mode return,
   linked with src/runtime/trap.c whose allocations are counted. 10,000 calls that alternately succeed and trap
   inside Domain.run must leave no block alive, and a domain must work inside a host boundary.
   Usage: domain_host */
#if defined(__linux__) && !defined(_GNU_SOURCE)
#define _GNU_SOURCE
#endif
#undef NDEBUG
#include <assert.h>
#include <stdatomic.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

static atomic_long live;

static void *counted_malloc(size_t size) {
    void *pointer = malloc(size);
    if (pointer) atomic_fetch_add(&live, 1);
    return pointer;
}

static void counted_free(void *pointer) {
    if (!pointer) return;
    atomic_fetch_sub(&live, 1);
    free(pointer);
}

static void *counted_realloc(void *pointer, size_t size) {
    return realloc(pointer, size);
}

#define TZ_TRAP_MALLOC counted_malloc
#define TZ_TRAP_FREE counted_free
#define TZ_TRAP_REALLOC counted_realloc
#define TZ_TRAP_API
#include "../src/runtime/trap.c"

int64_t tz_risky(int64_t count, int64_t divisor);
int64_t tz_guarded(int64_t count, int64_t divisor);

/* values[999] = 999 * 3 + 1 = 2998, and "buffer1000" has 10 characters. */
#define EXPECTED (2998 / 7 + 10)

static void nested(void *argument) {
    int64_t *value = argument;
    *value = tz_guarded(1000, 7) + tz_guarded(1000, 0); /* domains inside a host boundary */
    tz_risky(10, 0);                                    /* outside every domain: the boundary traps */
}

int main(void) {
    for (int round = 0; round < 10000; ++round) {
        int64_t divisor = round % 2 == 0 ? 7 : 0;
        int64_t expected = divisor == 0 ? -1 : EXPECTED; /* kind 1: integer division by zero */
        assert(tz_guarded(1000, divisor) == expected);
    }
    assert(live == 0);
    tsuzuri_trap_info trap = { 0, 0 };
    int64_t value = 0;
    assert(tsuzuri_boundary_run(nested, &value, &trap) == 1 && trap.kind == 1);
    assert(value == EXPECTED - 1 && live == 0);
    printf("domain host: ok, live %ld\n", (long)live);
    return 0;
}
