/* C host of tests/trap_return.mjs: calls the tsuzuri_try_<name> exports of tests/fixtures/trap_return.
   Built with -DCOUNTING it also provides src/runtime/trap.c itself over a counting allocator, so
   every case can check that no block outlives the call that allocated it. */
/* The checks are assertions: some toolchains define NDEBUG for optimized builds. */
#undef NDEBUG
#include <assert.h>
#include <stdatomic.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#ifdef COUNTING
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

#include "trap_return.h"

#ifdef MIXED_OBJECTS
extern void tz_ordinary_make(tsuzuri_i64_buffer *, int64_t);
extern int64_t tz_ordinary_parallel(int64_t);
extern int64_t tz_ordinary_sum(const int64_t *, int64_t);
#endif

int64_t e14_reenter(int64_t value);

int32_t reentered;

int64_t e14_reenter(int64_t value) {
    tsuzuri_trap_info trap = { 7, 7 };
    int64_t result = -1;
    /* Inside a call the boundary is taken: nothing runs and the arguments stay untouched. */
    reentered = tsuzuri_try_div(&trap, &result, value, 1);
    assert(result == -1 && trap.site == 7 && trap.kind == 7);
    return reentered;
}

void e14_host_buffer(tsuzuri_i64_buffer *out, int64_t count) {
    out->ptr = tsuzuri_alloc(count * (int64_t)sizeof *out->ptr);
    out->len = count;
    for (int64_t index = 0; index < count; ++index) out->ptr[index] = index * 3 + 1;
}

static void trapped(const char *name, int32_t status, const tsuzuri_trap_info *trap) {
    assert(status == 1);
    assert(trap->site != 0 && trap->kind < 15);
    LIVE_IS(0);
    printf("trap %s %u %u\n", name, trap->site, trap->kind);
}

static void scalar_cases(void) {
    tsuzuri_trap_info trap = { 0, 0 };
    int64_t result = -1;

    assert(tsuzuri_try_div(&trap, &result, 84, 2) == 0 && result == 42);
    assert(tsuzuri_try_div(&trap, &result, -7, 2) == 0 && result == -3);
    assert(tsuzuri_try_noop(&trap, &result) == 0 && result == 7);

    result = -1;
    trapped("div_zero", tsuzuri_try_div(&trap, &result, 1, 0), &trap);
    assert(result == -1);
    trapped("div_overflow", tsuzuri_try_div(&trap, &result, INT64_MIN, -1), &trap);
    assert(tsuzuri_try_div(&trap, &result, 9, 3) == 0 && result == 3);

    assert(tsuzuri_try_fill_div(&trap, &result, 10, 3) == 0 && result == 17);
    LIVE_IS(0);
    trapped("fill_div", tsuzuri_try_fill_div(&trap, &result, 10, 0), &trap);

    assert(tsuzuri_try_imported_div(&trap, &result, 10, 2) == 0 && result == 14);
    LIVE_IS(0);
    trapped("imported_div", tsuzuri_try_imported_div(&trap, &result, 10, 0), &trap);

    reentered = 0;
    assert(tsuzuri_try_call_host(&trap, &result, 5) == 0 && result == 3 && reentered == 2);
    LIVE_IS(0);
}

static void buffer_cases(void) {
    tsuzuri_trap_info trap = { 0, 0 };
    tsuzuri_i64_buffer numbers = { NULL, 0 };
    tsuzuri_string_buffer text = { NULL, 0 };

    assert(tsuzuri_try_make(&trap, &numbers, 6) == 0);
    assert(numbers.len == 6 && numbers.ptr);
    for (int64_t index = 0; index < 6; ++index) assert(numbers.ptr[index] == index * index);
    /* The host owns a finished call's result. */
    LIVE_IS(1);
    tsuzuri_free(numbers.ptr);
    LIVE_IS(0);

    numbers.ptr = NULL;
    trapped("make_negative", tsuzuri_try_make(&trap, &numbers, -1), &trap);
    assert(numbers.ptr == NULL);

    assert(tsuzuri_try_label(&trap, &text, 7) == 0);
    static const char expected[] = "value 7";
    assert(text.len == 7);
    for (int index = 0; index < 7; ++index) assert(text.ptr[index] == (uint16_t)expected[index]);
    LIVE_IS(1);
    tsuzuri_free(text.ptr);
    LIVE_IS(0);

    assert(tsuzuri_try_label_div(&trap, &text, 5) == 0 && text.len == 8);
    tsuzuri_free(text.ptr);
    text.ptr = NULL;
    trapped("label_div", tsuzuri_try_label_div(&trap, &text, 0), &trap);
    assert(text.ptr == NULL);
    LIVE_IS(0);
}

static void parallel_cases(void) {
    tsuzuri_trap_info trap = { 0, 0 };
    int64_t result = -1;

    assert(tsuzuri_try_parallel_div(&trap, &result, 64, 1) == 0 && result == 64 * 64);
    assert(tsuzuri_try_parallel_div(&trap, &result, 4, 0) == 0 && result == 16);
    assert(tsuzuri_try_parallel_div(&trap, &result, 64, 2) == 0 && result == 64 * 64 - 6);
    LIVE_IS(0);
    trapped("parallel_div", tsuzuri_try_parallel_div(&trap, &result, 64, 0), &trap);

    /* Task.parallel_results: the trap of an item that ran is reported even after an earlier Err
       (a trapped task is in no defined state), so an Err before a trap may show either outcome. */
    assert(tsuzuri_try_parallel_mixed(&trap, &result, -1, -1) == 0 && result == 120);
    trap.site = 0;
    int32_t status = tsuzuri_try_parallel_mixed(&trap, &result, 3, 9);
    assert((status == 0 && result == 1003) || (status == 1 && trap.site != 0));
    LIVE_IS(0);
    trapped("mixed_trap_first", tsuzuri_try_parallel_mixed(&trap, &result, 12, 4), &trap);
    assert(tsuzuri_try_parallel_mixed(&trap, &result, 3, -1) == 0 && result == 1003);

    assert(tsuzuri_try_nested_div(&trap, &result, -1) == 0 && result == 334);
    LIVE_IS(0);
    trapped("nested_div", tsuzuri_try_nested_div(&trap, &result, 5), &trap);
}

static void alternating(int rounds) {
    tsuzuri_trap_info trap = { 0, 0 };
    int64_t result = -1;
    for (int round = 0; round < rounds; ++round) {
        int64_t count = 1 + round % 40;
        if (round % 2 == 0) {
            assert(tsuzuri_try_fill_div(&trap, &result, count, 1) == 0);
            assert(result == (count - 1) * 3 + 1 + (int64_t)(7 + (count >= 10) + (count >= 100)));
        } else {
            assert(tsuzuri_try_fill_div(&trap, &result, count, 0) == 1);
            assert(trap.site != 0);
        }
        LIVE_IS(0);
        if (round % 10 == 0) {
            assert(tsuzuri_try_parallel_div(&trap, &result, 16, 1) == 0 && result == 256);
            assert(tsuzuri_try_parallel_div(&trap, &result, 16, 0) == 1);
            LIVE_IS(0);
        }
        if (round % 10 == 5) {
            trap.site = 0;
            int32_t mixed = tsuzuri_try_parallel_mixed(&trap, &result, 3, 9);
            assert((mixed == 0 && result == 1003) || (mixed == 1 && trap.site != 0));
            LIVE_IS(0);
        }
    }
}

int main(int argc, char **argv) {
    int rounds = argc > 1 ? atoi(argv[1]) : 3000;
#ifdef MIXED_OBJECTS
    const int64_t input[] = { 20, 22 };
    assert(tz_ordinary_sum(input, 2) == 42);
#ifndef COUNTING
    tsuzuri_i64_buffer ordinary = { NULL, 0 };
    tz_ordinary_make(&ordinary, 4);
    assert(ordinary.len == 4 && ordinary.ptr[3] == 3);
    tsuzuri_free(ordinary.ptr);
    assert(tz_ordinary_parallel(64) == 2016);
#endif
#endif
    scalar_cases();
    buffer_cases();
    parallel_cases();
    alternating(rounds);
    printf("ok %d\n", rounds);
    return 0;
}
