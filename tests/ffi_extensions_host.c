// Host for the E12 end-to-end test (tests/ffi_extensions.mjs).
// Without E12_HOST_MAIN it only implements the fixture's externs, so it can be
// linked into an executable with --link, -l or the manifest's [native] section.
// With E12_HOST_MAIN it also checks every export against hand-computed values.
#include <assert.h>
#include <stdatomic.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

typedef struct {
    int64_t value;
} counter;

static _Atomic int64_t live_counters;

void *e12_counter_new(int64_t start) {
    counter *handle = malloc(sizeof *handle);
    assert(handle);
    handle->value = start;
    atomic_fetch_add(&live_counters, 1);
    return handle;
}

int64_t e12_counter_add(void *handle, int64_t amount) {
    return ((counter *)handle)->value += amount;
}

int64_t e12_counter_free(void *handle) {
    int64_t last = ((counter *)handle)->value;
    free(handle);
    atomic_fetch_sub(&live_counters, 1);
    return last;
}

int64_t e12_add_one(int64_t value) { return value + 1; }

// Declared as extern "env" "e12_host_now": native ignores the module.
int64_t e12_host_now(void) { return 40; }

int64_t e12_apply_twice(int64_t (*callback)(int64_t), int64_t value) {
    return callback(callback(value));
}

// total = callback(total, i) for i = 1..count, starting from 0.
int64_t e12_fold(int64_t (*callback)(int64_t, int64_t), int64_t count) {
    int64_t total = 0;
    for (int64_t item = 1; item <= count; item++) total = callback(total, item);
    return total;
}

int64_t e12_run(int64_t (*callback)(void)) { return callback(); }

int64_t e12_visit(int64_t (*callback)(void *), void *handle) { return callback(handle); }

#ifdef E12_HOST_MAIN
#include "tz-ffi.h"

static _Atomic uint64_t live;

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

static void clean(void) {
    assert(atomic_load(&live) == 0);
    assert(atomic_load(&live_counters) == 0);
}

int main(int argc, char **argv) {
    // A trap inside a callback ends the process abnormally.
    if (argc > 1) {
        tz_callback_trap(1);
        return 0;
    }
    assert(tz_counters(10) == 59);
    clean();
    assert(tz_callbacks(4) == 39);
    clean();
    assert(tz_callback_shapes(10) == 82);
    clean();
    assert(tz_misc(2.25) == 40);
    assert(tz_misc(2.0) == 0);
    clean();
    void *handle = e12_counter_new(7);
    assert(tz_pass_through(handle) == handle);
    assert(e12_counter_free(handle) == 7);
    clean();
    return 0;
}
#endif
