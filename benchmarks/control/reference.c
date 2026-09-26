#include <stdint.h>
#include <stdlib.h>
#include <string.h>

#define JOIN_INNER(a, b) a##b
#define JOIN(a, b) JOIN_INNER(a, b)
#define KERNEL(name) JOIN(PREFIX, name)
#define NOINLINE __attribute__((noinline))

static void check_count(int64_t count) {
    if (count < 0 || count > 100000000) abort();
}

static uint64_t step(uint64_t state, uint64_t salt) {
    return (state ^ (state >> 13)) * UINT64_C(6364136223846793005)
        + salt + UINT64_C(1442695040888963407);
}

#ifdef __cplusplus
extern "C" {
#endif

NOINLINE uint64_t KERNEL(while_mix)(int64_t count, uint64_t seed) {
    check_count(count);
    while (count > 0) {
        seed = step(seed, (uint64_t)count);
        --count;
    }
    return seed;
}

NOINLINE uint64_t KERNEL(for_mix)(int64_t count, uint64_t seed) {
    check_count(count);
    for (int32_t remaining = (int32_t)count; remaining >= 1; --remaining) {
        seed = step(seed, (uint64_t)remaining);
    }
    return seed;
}

NOINLINE uint64_t KERNEL(tail_mix)(int64_t count, uint64_t seed) {
    check_count(count);
    while (count != 0) {
        seed = step(seed, (uint64_t)count);
        --count;
    }
    return seed;
}

static uint64_t classify(uint64_t n) {
    switch (n) {
        case 0: return 17;
        case 1: return 3;
        case 2: return 29;
        case 3: return 7;
        case 4: return 61;
        case 5: return 11;
        case 6: return 83;
        case 7: return 5;
        case 8: return 47;
        case 9: return 19;
        case 10: return 101;
        case 11: return 31;
        case 12: return 53;
        case 13: return 23;
        case 14: return 97;
        default: return 13;
    }
}

NOINLINE uint64_t KERNEL(match_dispatch)(int64_t count, uint64_t seed) {
    check_count(count);
    uint64_t total = seed;
    for (int64_t index = 0; index < count; ++index) {
        total += classify((seed + (uint64_t)index) & 15) * ((uint64_t)index + 1);
    }
    return total;
}

NOINLINE uint64_t KERNEL(array_sum)(int64_t count, uint64_t seed) {
    check_count(count);
    uint64_t *values = (uint64_t *)malloc(count == 0 ? 1 : (size_t)count * sizeof(uint64_t));
    if (!values) abort();
    for (int64_t index = 0; index < count; ++index) {
        values[index] = ((uint64_t)index ^ seed) * UINT64_C(6364136223846793005)
            + UINT64_C(1442695040888963407);
    }
    uint64_t total = 0;
    for (int64_t index = 0; index < count; ++index) total += values[index];
    free(values);
    return total;
}

NOINLINE uint64_t KERNEL(array_copy)(int64_t count, uint64_t seed) {
    check_count(count);
    size_t bytes = (size_t)count * sizeof(uint64_t);
    uint64_t *values = (uint64_t *)malloc(bytes ? bytes : 1);
    uint64_t *copied = (uint64_t *)malloc(bytes ? bytes : 1);
    if (!values || !copied) abort();
    for (int64_t index = 0; index < count; ++index) {
        values[index] = ((uint64_t)index ^ seed) * UINT64_C(6364136223846793005)
            + UINT64_C(1442695040888963407);
    }
    memcpy(copied, values, bytes);
    uint64_t total = 0;
    for (int64_t index = 0; index < count; ++index) total += values[index];
    for (int64_t index = 0; index < count; ++index) total += copied[index];
    free(copied);
    free(values);
    return total;
}

struct Node {
    uint64_t value;
    struct Node *next;
};

NOINLINE uint64_t KERNEL(list_sum)(int64_t count, uint64_t seed) {
    check_count(count);
    struct Node *head = NULL;
    struct Node **tail = &head;
    for (int64_t index = 0; index < count; ++index) {
        struct Node *node = (struct Node *)malloc(sizeof(struct Node));
        if (!node) abort();
        node->value = ((uint64_t)index ^ seed) * UINT64_C(6364136223846793005)
            + UINT64_C(1442695040888963407);
        node->next = NULL;
        *tail = node;
        tail = &node->next;
    }
    uint64_t total = 0;
    for (struct Node *node = head; node; node = node->next) total += node->value;
    while (head) {
        struct Node *next = head->next;
        free(head);
        head = next;
    }
    return total;
}

static uint64_t captured_first(uint64_t value, uint64_t seed) { return step(value, seed); }
static uint64_t captured_second(uint64_t value, uint64_t seed) { return step(value, seed ^ 71); }

NOINLINE uint64_t KERNEL(closure_capture)(int64_t count, uint64_t seed) {
    check_count(count);
    uint64_t (*transform)(uint64_t, uint64_t) = (seed & 1) == 0 ? captured_first : captured_second;
    uint64_t state = seed;
    for (int64_t index = 0; index < count; ++index) state = transform(state, seed);
    return state;
}

NOINLINE uint64_t KERNEL(closure_churn)(int64_t count, uint64_t seed) {
    check_count(count);
    uint64_t state = seed;
    for (int64_t index = 0; index < count; ++index) {
        uint64_t captured = state;
        uint64_t (*transform)(uint64_t, uint64_t) = (captured & 1) == 0 ? captured_first : captured_second;
        uint64_t (*copy)(uint64_t, uint64_t) = transform;
        state = copy(state, captured);
        state = transform(state, captured);
    }
    return state;
}

struct State { uint64_t value; int64_t remaining; };

static struct State advance(const struct State *state) {
    struct State next = {step(state->value, (uint64_t)state->remaining), state->remaining - 1};
    return next;
}

NOINLINE uint64_t KERNEL(record_pipeline)(int64_t count, uint64_t seed) {
    check_count(count);
    struct State state = {seed, count};
    while (state.remaining > 0) state = advance(&state);
    return state.value;
}

NOINLINE uint64_t KERNEL(integer128_mix)(int64_t count, uint64_t seed) {
    check_count(count);
    __uint128_t state = ((__uint128_t)seed << 64) | UINT64_C(1442695040888963407);
    for (int64_t remaining = count; remaining > 0; --remaining) {
        state = (state ^ (state >> 43)) * UINT64_C(6364136223846793005) + (uint64_t)remaining;
    }
    return (uint64_t)(state ^ (state >> 64));
}

NOINLINE uint64_t KERNEL(float32_mix)(int64_t count, uint64_t seed) {
    check_count(count);
    float state = (float)(seed & 65535) / 16.0f + 1.0f;
    for (int64_t index = 0; index < count; ++index) {
        state = state * 1.000001f + (float)(index & 7) / 16.0f;
    }
    return state >= 0x1p64f ? UINT64_MAX : (uint64_t)state;
}

NOINLINE uint64_t KERNEL(float64_mix)(int64_t count, uint64_t seed) {
    check_count(count);
    double state = (double)(seed & 65535) / 16.0 + 1.0;
    for (int64_t index = 0; index < count; ++index) {
        state = state * 1.0000001 + (double)(index & 7) / 16.0;
    }
    return state >= 0x1p64 ? UINT64_MAX : (uint64_t)state;
}

#ifdef __cplusplus
}
#endif
