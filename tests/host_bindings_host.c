// The host functions that tests/fixtures/bindings_native imports, linked into its shared library
// with --link (tests/host_bindings.mjs).
#include <stdint.h>
#include <stdlib.h>

void *tsuzuri_alloc(int64_t size);

typedef struct {
    int64_t value;
} counter;

void *e13_counter_new(int64_t start) {
    counter *created = malloc(sizeof *created);
    if (!created) abort();
    created->value = start;
    return created;
}

int64_t e13_counter_add(void *handle, int64_t amount) {
    counter *existing = handle;
    existing->value += amount;
    return existing->value;
}

int64_t e13_counter_free(void *handle) {
    counter *existing = handle;
    int64_t value = existing->value;
    free(existing);
    return value;
}

typedef struct {
    double *ptr;
    int64_t len;
} f64_buffer;

// Owned results come from tsuzuri_alloc, which hands the memory to Tsuzuri.
void tsuzuri_host_Main_host_scale(f64_buffer *out, const double *values, int64_t length, double factor) {
    double *result = tsuzuri_alloc(length * (int64_t)sizeof(double));
    for (int64_t index = 0; index < length; ++index) result[index] = values[index] * factor;
    out->ptr = result;
    out->len = length;
}
