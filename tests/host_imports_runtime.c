#include <assert.h>
#include <stdint.h>
#include <stdatomic.h>
#include <stdlib.h>
#include <string.h>
#include "tz-imports.h"

static _Atomic uint64_t live;
static _Atomic uint64_t calls;
static int64_t order;

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

int64_t tsuzuri_host_Main_now(void) { atomic_fetch_add(&calls, 1); return 40; }
int64_t tsuzuri_host_Main_mark(int64_t value) { order = order * 10 + value; return value; }
int64_t tsuzuri_host_Main_combine(int64_t left, int64_t right) { return left * 10 + right; }
int32_t tsuzuri_host_Main_small(int32_t value) { return value + 1; }
int32_t tsuzuri_host_Main_flag(int32_t value) { return value ? 0 : 7; }

int64_t tsuzuri_host_Main_host_read(const int64_t *values, int64_t count, const uint16_t *text, int64_t units, const uint8_t *utf8, int64_t bytes, const tz_record_4Main_5Point *point) {
    assert(count == 2 && values[0] == 20 && values[1] == 22);
    assert(units == 3 && text[0] == 'a' && text[2] == 'c');
    assert(bytes == 2 && utf8[0] == 'x' && utf8[1] == 'y');
    assert(point->x == 3.0 && point->flag == 1);
    return 42;
}

void tsuzuri_host_Main_host_buffer(tsuzuri_i64_buffer *out) {
    out->ptr = tsuzuri_alloc(16);
    out->len = 2;
    out->ptr[0] = 20; out->ptr[1] = 22;
}

void tsuzuri_host_Main_host_text(tsuzuri_utf8string_buffer *out) {
    out->ptr = tsuzuri_alloc(2);
    out->len = 2;
    out->ptr[0] = 'o'; out->ptr[1] = 'k';
}

void tsuzuri_host_Main_host_point(tz_record_4Main_5Point *out, const tz_record_4Main_5Point *point) { out->x = point->x + 0.5; out->flag = 7; }

void tsuzuri_host_Main_invalid_buffer(tsuzuri_i64_buffer *out, int64_t mode) {
    out->ptr = NULL;
    out->len = mode == 0 ? -1 : mode == 1 ? 1 : INT64_MAX;
}

int main(int argc, char **argv) {
    if (argc > 1) { tz_bad_buffer(strtoll(argv[1], NULL, 10)); return 99; }
    assert(tz_effects(0) == 57 && order == 1235 && atomic_load(&calls) == 1);
    order = 0;
    assert(tz_effects(1) == 56 && order == 1234 && atomic_load(&calls) == 2);
    assert(tz_function_value() == 82);
    for (int64_t value = -260; value <= 260; ++value) assert(tz_normalized(value));
    for (int index = 0; index < 1000; ++index) { assert(tz_buffers() == 86); assert(atomic_load(&live) == 0); }
    uint64_t before = atomic_load(&calls);
    assert(tz_tasks() == 640);
    assert(atomic_load(&calls) == before + 16);
    assert(atomic_load(&live) == 0);
    return 0;
}
