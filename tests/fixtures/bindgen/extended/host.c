/* Round trip for tests/bindgen.mjs: every value that Tsuzuri gets through the externs
   generated from ext.h must equal the direct C computation and the value worked out by
   hand. Linked with the IR, Tsuzuri's malloc/free/realloc are these tracked versions, so
   every array that Tsuzuri lends to C as a buffer must be freed again (live == 0). */
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "ext.h"
#include "tz-extended.h"

int64_t ext_live_counters(void);

static uint64_t live;

void *tracked_alloc(uint64_t size) {
    uint64_t *value = malloc((size_t)size + 16);
    if (!value) abort();
    value[0] = size;
    live += size;
    return value + 2;
}

void tracked_free(void *pointer) {
    if (!pointer) return;
    uint64_t *value = (uint64_t *)pointer - 2;
    live -= value[0];
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

static int failures;
#define CHECK(condition) \
    do { \
        if (!(condition)) { \
            fprintf(stderr, "%s:%d: check failed: %s\n", __FILE__, __LINE__, #condition); \
            failures++; \
        } \
    } while (0)

static int64_t triple(int64_t value) { return value * 3; }
static int64_t add_item(int64_t total, int32_t item) { return total + item; }
static int64_t peek_plus_one(const ext_counter *counter) { return ext_counter_peek(counter) + 1; }
static int32_t answer(void) { return 42; }

int main(void) {
    int64_t constants = (int64_t)EXT_VERSION + (int64_t)EXT_MASK + EXT_NEGATIVE + EXT_OCTAL + EXT_BINARY +
                        EXT_PAREN + EXT_WIDE + (int64_t)EXT_HEX_WIDE + EXT_MIN_INT + (int64_t)EXT_ALL_ONES + EXT_LONG;
    CHECK(tz_constants_case() == constants);
    CHECK(tz_constants_case() == INT64_C(6442451674));
    CHECK(tz_top_case() == EXT_TOP);
    CHECK(tz_top_case() == UINT64_MAX);

    ext_counter *counter = ext_counter_new(10);
    int64_t first = ext_counter_add(counter, 5);
    int64_t second = ext_counter_add(counter, 7);
    int64_t peeked = ext_counter_peek(counter);
    int64_t seen = ext_visit(peek_plus_one, counter);
    int64_t direct = first + second + peeked + seen + ext_counter_free(counter);
    CHECK(tz_counters(10) == direct);
    CHECK(tz_counters(10) == 15 + 22 + 22 + 23 + 22);
    CHECK(ext_live_counters() == 0);

    CHECK(tz_callbacks(4) == ext_apply_twice(triple, 4) + ext_fold(add_item, 100) + ext_run(answer));
    CHECK(tz_callbacks(4) == 36 + 110 + 42);
    CHECK(tz_any_case(3) == 0);
    CHECK(tz_any_case(4) == 1);
    CHECK(live == 0);

    int64_t values[] = {1, 2, 3, 4};
    double doubles[] = {1.0, 2.0, 6.0};
    unsigned char bytes[] = {1, 2, 3};
    int64_t buffers = ext_sum(values, 4) + ext_sum(NULL, 0) + (int64_t)ext_mean(doubles, 3) * 100 +
                      (int64_t)ext_checksum(bytes, 3) * 10000 + (int64_t)ext_text_length("h\xc3\xa9llo", 6) * 1000000;
    for (int round = 0; round < 1000; round++) {
        CHECK(tz_buffers() == buffers);
        CHECK(live == 0);
    }
    CHECK(tz_buffers() == 10 + 300 + 140000 + 62000000);

    if (failures == 0) puts("bindgen extended round trip ok");
    return failures != 0;
}
