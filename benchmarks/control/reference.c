#include <stdint.h>
#include <stdlib.h>

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

#ifdef __cplusplus
}
#endif
