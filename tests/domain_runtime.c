/* Unit tests of the failure domain (B10 Phase 2): src/runtime/trap.c with every allocation counted, so
   each case ends with the number of live blocks. Usage: domain_runtime */
#if defined(__linux__) && !defined(_GNU_SOURCE)
#define _GNU_SOURCE
#endif
#undef NDEBUG
#include <assert.h>
#include <pthread.h>
#include <stdatomic.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

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

static atomic_uint checks;
#define CHECK(condition) do { assert(condition); atomic_fetch_add(&checks, 1); } while (0)

struct work {
    long count;
    void *result;
    void *outer_block;
    int trap;
};

/* Allocates `count` blocks, frees all but the first, which stays as the result. */
static void survivor_domain(void *argument) {
    struct work *work = argument;
    for (long index = 0; index < work->count; ++index) {
        unsigned char *block = tsuzuri_tracked_malloc(16 + (size_t)index);
        assert(block && (uintptr_t)block % 16 == 0);
        memset(block, 0x5a, 16 + (size_t)index);
        if (index == 0) work->result = block;
        else tsuzuri_tracked_free(block);
    }
}

/* Allocates, frees a third, grows one, then traps with nothing released. */
static void trapping_domain(void *argument) {
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
    assert(!"a trap inside a domain must not return");
}

static void host_boundary(void *argument) {
    struct work *work = argument;
    work->outer_block = tsuzuri_tracked_malloc(64);
    struct work inner = { .count = 50 };
    tsuzuri_trap_info trap = { 0, 0 };
    CHECK(tsuzuri_domain_run(survivor_domain, &inner, &trap) == 0);
    CHECK(tsuzuri_domain_run(trapping_domain, &inner, &trap) == 1);
    CHECK(trap.site == 42 && trap.kind == 3);
    /* The block of the host and the survivor of the first domain are intact (a sanitizer checks). */
    memset(work->outer_block, 1, 64);
    memset(inner.result, 2, 16);
    if (work->trap) tsuzuri_trap_raise(7, 1);
    tsuzuri_tracked_free(inner.result);
}

static void free_outer_in_domain(void *argument) {
    struct work *work = argument;
    tsuzuri_tracked_free(work->outer_block);
    work->outer_block = NULL;
    tsuzuri_trap_raise(9, 2);
}

static void host_cross_free(void *argument) {
    struct work *work = argument;
    work->outer_block = tsuzuri_tracked_malloc(64);
    tsuzuri_trap_info trap = { 0, 0 };
    CHECK(tsuzuri_domain_run(free_outer_in_domain, work, &trap) == 1);
    CHECK(trap.site == 9 && work->outer_block == NULL);
}

static void realloc_outer_in_domain(void *argument) {
    struct work *work = argument;
    work->outer_block = tsuzuri_tracked_realloc(work->outer_block, 4096);
    assert(work->outer_block);
    memset(work->outer_block, 4, 4096);
    tsuzuri_trap_raise(10, 2);
}

static void host_cross_realloc(void *argument) {
    struct work *work = argument;
    work->outer_block = tsuzuri_tracked_malloc(8);
    tsuzuri_trap_info trap = { 0, 0 };
    CHECK(tsuzuri_domain_run(realloc_outer_in_domain, work, &trap) == 1);
    memset(work->outer_block, 5, 4096);
    if (work->trap) tsuzuri_trap_raise(8, 1);
}

static void nested_outer(void *argument) {
    struct work *work = argument;
    work->outer_block = tsuzuri_tracked_malloc(32);
    struct work inner = { .count = 10 };
    tsuzuri_trap_info trap = { 0, 0 };
    CHECK(tsuzuri_domain_run(survivor_domain, &inner, &trap) == 0);
    CHECK(tsuzuri_domain_run(trapping_domain, &inner, &trap) == 1);
    if (work->trap) tsuzuri_trap_raise(11, 4);
    work->result = inner.result;
}

static void reenter_boundary(void *argument) {
    (void)argument;
    CHECK(0);
}

static void domain_with_boundary_inside(void *argument) {
    struct work *work = argument;
    tsuzuri_trap_info trap = { 0, 0 };
    CHECK(tsuzuri_boundary_run(reenter_boundary, work, &trap) == 2);
    work->result = tsuzuri_tracked_malloc(24);
}

static void *thread_main(void *argument) {
    (void)argument;
    for (int round = 0; round < 2000; ++round) {
        struct work work = { .count = 20 };
        tsuzuri_trap_info trap = { 0, 0 };
        if (round % 2 == 0) {
            CHECK(tsuzuri_domain_run(survivor_domain, &work, &trap) == 0);
            tsuzuri_tracked_free(work.result);
        } else {
            CHECK(tsuzuri_domain_run(trapping_domain, &work, &trap) == 1);
        }
    }
    return NULL;
}

int main(void) {
    tsuzuri_trap_info trap = { 0, 0 };
    struct work work = { .count = 100 };

    /* A trap outside every frame returns. */
    tsuzuri_trap_raise(1, 1);

    /* A finished domain leaves its result, and only that. */
    CHECK(tsuzuri_domain_run(survivor_domain, &work, &trap) == 0);
    CHECK(work.result && live == 1);
    tsuzuri_tracked_free(work.result);
    CHECK(live == 0);

    /* A trapped domain leaves nothing, and says where. */
    CHECK(tsuzuri_domain_run(trapping_domain, &work, &trap) == 1);
    CHECK(trap.site == 42 && trap.kind == 3 && live == 0);

    /* Alternating, 10,000 times. */
    for (int round = 0; round < 10000; ++round) {
        struct work again = { .count = 5 + round % 17 };
        if (round % 2 == 0) {
            CHECK(tsuzuri_domain_run(survivor_domain, &again, &trap) == 0);
            tsuzuri_tracked_free(again.result);
        } else {
            CHECK(tsuzuri_domain_run(trapping_domain, &again, &trap) == 1);
        }
    }
    CHECK(live == 0);

    /* Inside a host boundary: the domain's trap does not touch the host's blocks; the host's own trap frees all. */
    struct work host = { .trap = 0 };
    CHECK(tsuzuri_boundary_run(host_boundary, &host, &trap) == 0);
    CHECK(live == 1);
    tsuzuri_tracked_free(host.outer_block);
    CHECK(live == 0);
    host = (struct work){ .trap = 1 };
    CHECK(tsuzuri_boundary_run(host_boundary, &host, &trap) == 1 && trap.site == 7);
    CHECK(live == 0);

    /* A domain frees a block of the boundary around it, then traps: no double free later. */
    host = (struct work){ 0 };
    CHECK(tsuzuri_boundary_run(host_cross_free, &host, &trap) == 0);
    CHECK(live == 0);

    /* A domain grows a block of the boundary around it, then traps: the boundary still owns it. */
    host = (struct work){ .trap = 0 };
    CHECK(tsuzuri_boundary_run(host_cross_realloc, &host, &trap) == 0);
    CHECK(live == 1);
    tsuzuri_tracked_free(host.outer_block);
    CHECK(live == 0);
    host = (struct work){ .trap = 1 };
    CHECK(tsuzuri_boundary_run(host_cross_realloc, &host, &trap) == 1 && trap.site == 8);
    CHECK(live == 0);

    /* Domains inside a domain. */
    host = (struct work){ .trap = 1 };
    CHECK(tsuzuri_domain_run(nested_outer, &host, &trap) == 1 && trap.site == 11);
    CHECK(live == 0);
    host = (struct work){ .trap = 0 };
    CHECK(tsuzuri_domain_run(nested_outer, &host, &trap) == 0);
    CHECK(live == 2);
    tsuzuri_tracked_free(host.outer_block);
    tsuzuri_tracked_free(host.result);
    CHECK(live == 0);

    /* A host boundary cannot open inside a domain. */
    host = (struct work){ 0 };
    CHECK(tsuzuri_domain_run(domain_with_boundary_inside, &host, &trap) == 0);
    tsuzuri_tracked_free(host.result);
    CHECK(live == 0);

    /* Threads keep their own frames. */
    pthread_t threads[4];
    for (int index = 0; index < 4; ++index) assert(pthread_create(&threads[index], NULL, thread_main, NULL) == 0);
    for (int index = 0; index < 4; ++index) assert(pthread_join(threads[index], NULL) == 0);
    CHECK(live == 0);

    printf("domain runtime: %u checks passed\n", atomic_load(&checks));
    return 0;
}
