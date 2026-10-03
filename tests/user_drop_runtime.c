#undef NDEBUG
#include <assert.h>
#include <stdatomic.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "tz-user-drop.h"

static _Atomic uint64_t live;
static _Atomic int64_t count;
static _Atomic int64_t sum;
static _Atomic int64_t values[64];
static int report;

void *tracked_alloc(uint64_t size) {
    uint64_t *value = malloc((size_t)size + 16);
    assert(value);
    value[0] = size;
    value[1] = UINT64_C(0x51a110ca7e);
    atomic_fetch_add(&live, size);
    return value + 2;
}

void tracked_free(void *pointer) {
    if (!pointer) return;
    uint64_t *value = (uint64_t *)pointer - 2;
    assert(value[1] == UINT64_C(0x51a110ca7e));
    assert(atomic_load(&live) >= value[0]);
    atomic_fetch_sub(&live, value[0]);
    value[1] = 0;
    free(value);
}

void *tracked_realloc(void *pointer, uint64_t size) {
    if (!pointer) return tracked_alloc(size);
    uint64_t old = ((uint64_t *)pointer)[-2];
    void *next = tracked_alloc(size);
    memcpy(next, pointer, (size_t)(old < size ? old : size));
    tracked_free(pointer);
    return next;
}

// Tasks drop their values on worker threads, so the log is atomic.
void tsuzuri_host_Main_drop_log(int64_t value) {
    int64_t index = atomic_fetch_add(&count, 1);
    atomic_fetch_add(&sum, value);
    if (index < 64) atomic_store(&values[index], value);
    if (report) {
        printf("dropped %lld\n", (long long)value);
        fflush(stdout);
    }
}

static void reset(void) {
    atomic_store(&count, 0);
    atomic_store(&sum, 0);
}

static void expect(const int64_t *expected, int64_t length) {
    assert(atomic_load(&count) == length);
    for (int64_t index = 0; index < length; ++index) assert(atomic_load(&values[index]) == expected[index]);
    assert(atomic_load(&live) == 0);
}

#define EXPECT(...) expect((const int64_t[]){__VA_ARGS__}, sizeof((const int64_t[]){__VA_ARGS__}) / sizeof(int64_t))

int main(int argc, char **argv) {
    if (argc > 1) {
        report = 1;
        tz_trap_after_create();
        return 99;
    }
    reset(); assert(tz_scopes() == 3); EXPECT(2, 1);
    reset(); assert(tz_fields() == 1); EXPECT(1, 10, 11);
    reset(); assert(tz_reassign() == 2); EXPECT(1, 2);
    reset(); assert(tz_consume() == 7); EXPECT(7);
    reset(); assert(tz_loop_break() == 4); EXPECT(0, 1, 2, 3);
    reset(); assert(tz_array() == 3); EXPECT(1, 2, 3);
    reset(); assert(tz_map_remove() == 2);
    assert(atomic_load(&count) == 4 && atomic_load(&values[0]) == 2 && atomic_load(&values[1]) == 100);
    assert(atomic_load(&values[2]) + atomic_load(&values[3]) == 4 && atomic_load(&values[2]) * atomic_load(&values[3]) == 3);
    assert(atomic_load(&live) == 0);
    reset(); assert(tz_generic() == 9); EXPECT(5, 4);
    reset(); assert(tz_chain() == 3); EXPECT(1, 2, 3);
    reset(); assert(tz_long_chain(1000000) == 1000000);
    assert(atomic_load(&count) == 1000001 && atomic_load(&sum) == 1000001 && atomic_load(&live) == 0);
    reset(); assert(tz_task_owned() == 7); EXPECT(7);
    reset(); assert(tz_task_unstarted() == 0); EXPECT(8);
    reset(); assert(tz_tasks_parallel() == 120);
    assert(atomic_load(&count) == 16 && atomic_load(&sum) == 120 && atomic_load(&live) == 0);
    reset(); assert(tz_drop_allocates() == 5); EXPECT(1);
    reset(); assert(tz_moved_field() == 1); EXPECT(1, 2);
    reset(); assert(tz_temporary_field() == 6); EXPECT(6);
    reset(); assert(tz_unions() == 123); EXPECT(31, 20, 21);
    reset(); assert(tz_use_scopes() == 1); EXPECT(2, 1);
    reset(); assert(tz_use_computation() == 15); EXPECT(3, 4);
    reset(); assert(tz_use_task() == 6); EXPECT(5);
    reset(); assert(tz_early_drop() == 2); EXPECT(1, 100, 2);
    reset(); assert(tz_owned_function() == 44); EXPECT(100, 7);
    reset(); assert(tz_owned_task() == 9); EXPECT(8);
    reset(); assert(tz_owned_unused() == 0); EXPECT(9);
    reset(); assert(tz_owned_array() == 31); EXPECT(1, 2);
    reset(); assert(tz_owned_nested() == 8); EXPECT(3);
    reset(); assert(tz_owned_generic() == 2); EXPECT(4);
    for (int index = 0; index < 1000; ++index) {
        reset();
        assert(tz_scopes() == 3);
        EXPECT(2, 1);
    }
    return 0;
}
