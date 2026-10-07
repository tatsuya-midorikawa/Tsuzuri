/* The C library that tests/bindgen.mjs binds from ext.h. */
#include <stdlib.h>
#include "ext.h"

struct ext_counter {
    int64_t value;
};

static int64_t live_counters;

ext_counter *ext_counter_new(int64_t start) {
    ext_counter *counter = malloc(sizeof *counter);
    if (!counter) abort();
    counter->value = start;
    live_counters++;
    return counter;
}

int64_t ext_counter_add(ext_counter *counter, int64_t amount) { return counter->value += amount; }

int64_t ext_counter_peek(const ext_counter *counter) { return counter->value; }

int64_t ext_counter_free(ext_counter *counter) {
    int64_t value = counter->value;
    free(counter);
    live_counters--;
    return value;
}

const ext_counter *ext_counter_shared(void) {
    static ext_counter shared;
    return &shared;
}

int ext_counter_open(ext_counter **out) {
    *out = ext_counter_new(0);
    return 0;
}

/* Not in ext.h: the host checks that every counter was freed. */
int64_t ext_live_counters(void) { return live_counters; }

int64_t ext_apply_twice(ext_map map, int64_t value) { return map(map(value)); }

int64_t ext_fold(int64_t (*step)(int64_t total, int32_t item), int64_t start) {
    int64_t total = start;
    for (int32_t item = 1; item <= 4; item++) total = step(total, item);
    return total;
}

int64_t ext_visit(int64_t (*visit)(const ext_counter *counter), const ext_counter *counter) {
    return visit(counter);
}

int32_t ext_run(int32_t (*task)(void)) { return task(); }

_Bool ext_any(_Bool (*test)(_Bool flag, uint8_t small), int32_t count) {
    for (int32_t index = 0; index < count; index++) {
        if (test(index % 2 == 1, (uint8_t)(index * 7))) return 1;
    }
    return 0;
}

int ext_with_context(void (*callback)(void *context), void *context) {
    callback(context);
    return 0;
}

int64_t ext_sum(const int64_t *values, size_t count) {
    int64_t total = 0;
    for (size_t index = 0; index < count; index++) total += values[index];
    return total;
}

double ext_mean(const double *values, int64_t count) {
    double total = 0;
    for (int64_t index = 0; index < count; index++) total += values[index];
    return count ? total / (double)count : 0;
}

uint64_t ext_checksum(const unsigned char *bytes, size_t length) {
    uint64_t sum = 0;
    for (size_t index = 0; index < length; index++) sum += (uint64_t)(index + 1) * bytes[index];
    return sum;
}

/* The length times 10 plus the number of bytes outside ASCII. */
size_t ext_text_length(const char *text, size_t length) {
    size_t wide = 0;
    for (size_t index = 0; index < length; index++) wide += (unsigned char)text[index] >= 0x80;
    return length * 10 + wide;
}

int ext_fill(int64_t *values, size_t count) {
    for (size_t index = 0; index < count; index++) values[index] = 0;
    return 0;
}

int ext_split(const int64_t *values, int flags, size_t count) { return values && flags ? (int)count : 0; }

int64_t ext_sum32(const int32_t *values, size_t count) {
    int64_t total = 0;
    for (size_t index = 0; index < count; index++) total += values[index];
    return total;
}
