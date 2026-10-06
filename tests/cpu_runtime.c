/* Checks src/runtime/cpu.c from the inside (F08): feature decoding, TSUZURI_CPU_FORCE parsing,
   and every kernel of every level the CPU runs against scalar references. tests/cpu_kernels.mjs
   compiles and runs it at -O0 and -O3. */
#include <assert.h>
#include <pthread.h>
#include <string.h>
#include "../src/runtime/cpu.c"

#define CHECK_SUM(T, U, TYPE, LEVEL, ARRAY) \
    do { \
        const T *values = (const T *)data.ARRAY + offset; \
        U expected = 0; \
        for (int64_t index = 0; index < length; ++index) expected += (U)values[index]; \
        assert(tz_cpu_sum_##TYPE##_##LEVEL(values, length) == (T)expected); \
    } while (0)

#define CHECK_BEST(OPERATION, T, TYPE, LEVEL, BETTER, ARRAY) \
    do { \
        const T *values = (const T *)data.ARRAY + offset; \
        int64_t expected = 0; \
        for (int64_t index = 1; index < length; ++index) \
            if (values[index] BETTER values[expected]) expected = index; \
        assert(tz_cpu_##OPERATION##_##TYPE##_##LEVEL(values, length) == expected); \
    } while (0)

#define CHECK_LEVEL(LEVEL) \
    do { \
        CHECK_SUM(int8_t, uint8_t, i8, LEVEL, b8); \
        CHECK_SUM(int16_t, uint16_t, i16, LEVEL, b16); \
        CHECK_SUM(int32_t, uint32_t, i32, LEVEL, b32); \
        CHECK_SUM(int64_t, uint64_t, i64, LEVEL, b64); \
        CHECK_BEST(min, int8_t, i8, LEVEL, <, b8); \
        CHECK_BEST(min, int16_t, i16, LEVEL, <, b16); \
        CHECK_BEST(min, int32_t, i32, LEVEL, <, b32); \
        CHECK_BEST(min, int64_t, i64, LEVEL, <, b64); \
        CHECK_BEST(min, uint8_t, i8u, LEVEL, <, b8); \
        CHECK_BEST(min, uint16_t, i16u, LEVEL, <, b16); \
        CHECK_BEST(min, uint32_t, i32u, LEVEL, <, b32); \
        CHECK_BEST(min, uint64_t, i64u, LEVEL, <, b64); \
        CHECK_BEST(max, int8_t, i8, LEVEL, >, b8); \
        CHECK_BEST(max, int16_t, i16, LEVEL, >, b16); \
        CHECK_BEST(max, int32_t, i32, LEVEL, >, b32); \
        CHECK_BEST(max, int64_t, i64, LEVEL, >, b64); \
        CHECK_BEST(max, uint8_t, i8u, LEVEL, >, b8); \
        CHECK_BEST(max, uint16_t, i16u, LEVEL, >, b16); \
        CHECK_BEST(max, uint32_t, i32u, LEVEL, >, b32); \
        CHECK_BEST(max, uint64_t, i64u, LEVEL, >, b64); \
    } while (0)

/* The same bytes as arrays of each element width. C reads an object only through its own type or
   the matching unsigned one, so a byte buffer cannot stand in for them. */
struct data {
    _Alignas(64) int8_t b8[8 * 300];
    _Alignas(64) int16_t b16[4 * 300];
    _Alignas(64) int32_t b32[2 * 300];
    _Alignas(64) int64_t b64[300];
};

/* Bytes of a 64-bit LCG; pattern 1 has few distinct values, so ties are common. */
static void fill(unsigned char *bytes, size_t count, int pattern) {
    uint64_t state = (uint64_t)pattern + 1;
    for (size_t index = 0; index < count; ++index) {
        state = state * UINT64_C(6364136223846793005) + UINT64_C(1442695040888963407);
        bytes[index] = pattern == 1 ? (unsigned char)(state >> 62) : (unsigned char)(state >> 56);
    }
}

static void *check_kernels(void *unused) {
    (void)unused;
    /* Every worker fills its own data: one shared buffer would be written by all of them at once. */
    struct data data;
    uint64_t features = tsuzuri_cpu_features();
    for (int pattern = 0; pattern < 3; ++pattern) {
        unsigned char bytes[8 * 300];
        fill(bytes, sizeof bytes, pattern);
        memcpy(data.b8, bytes, sizeof bytes);
        memcpy(data.b16, bytes, sizeof bytes);
        memcpy(data.b32, bytes, sizeof bytes);
        memcpy(data.b64, bytes, sizeof bytes);
        /* Starting 0 to 3 elements into aligned storage moves blocks off vector alignment. */
        for (int offset = 0; offset < 4; ++offset) {
            for (int64_t length = 0; length <= 257; ++length) {
                CHECK_LEVEL(baseline);
#if TZ_CPU_X86
                if (features & TZ_CPU_HAS_SSE42) CHECK_LEVEL(sse42);
                if (features & TZ_CPU_HAS_AVX2) CHECK_LEVEL(avx2);
                if (features & TZ_CPU_HAS_AVX512) CHECK_LEVEL(avx512);
#else
                (void)features;
#endif
                const int32_t *values = data.b32 + offset;
                int64_t expected = 0;
                for (int64_t index = 1; index < length; ++index)
                    if (values[index] < values[expected]) expected = index;
                assert(tsuzuri_cpu_min_i32(values, length) == expected);
            }
        }
    }
    return NULL;
}

int main(void) {
    unsigned avx = (1u << 27) | (1u << 28);
    unsigned avx512 = (1u << 5) | (1u << 16) | (1u << 17) | (1u << 28) | (1u << 30) | (1u << 31);
    assert(tz_cpu_decode(0, 0, 0) == 0);
    assert(tz_cpu_decode(1u << 20, 0, 0) == 1);
    assert(tz_cpu_decode(avx, 1u << 5, 6) == 2);
    assert(tz_cpu_decode(avx, 1u << 5, 2) == 0);
    assert(tz_cpu_decode(1u << 28, 1u << 5, 6) == 0);
    assert(tz_cpu_decode(avx, avx512, 0xE6) == (TZ_CPU_HAS_AVX2 | TZ_CPU_HAS_AVX512));
    assert(tz_cpu_decode(avx, avx512, 6) == TZ_CPU_HAS_AVX2);
    assert(tz_cpu_decode(avx, avx512 & ~(1u << 31), 0xE6) == TZ_CPU_HAS_AVX2);
    assert(tz_cpu_decode_arm(0, 2) == 0);
    assert(tz_cpu_decode_arm(1ul << 22, 0) == TZ_CPU_HAS_SVE);
    assert(tz_cpu_decode_arm(1ul << 22, 2) == (TZ_CPU_HAS_SVE | TZ_CPU_HAS_SVE2));
    /* TSUZURI_CPU_FORCE: unset picks SSE4.2 or AVX2 but never AVX-512 or SVE by itself. */
    uint64_t all = TZ_CPU_HAS_SSE42 | TZ_CPU_HAS_AVX2 | TZ_CPU_HAS_AVX512;
    assert(tz_cpu_select(0, NULL) == TZ_CPU_BASELINE);
    assert(tz_cpu_select(1, NULL) == TZ_CPU_SSE42);
    assert(tz_cpu_select(3, NULL) == TZ_CPU_AVX2);
    assert(tz_cpu_select(all, NULL) == TZ_CPU_AVX2);
    assert(tz_cpu_select(TZ_CPU_HAS_SVE, NULL) == TZ_CPU_BASELINE);
    assert(tz_cpu_select(3, "baseline") == TZ_CPU_BASELINE);
    assert(tz_cpu_select(0, "sse4.2") == -1);
    assert(tz_cpu_select(3, "avx2") == TZ_CPU_AVX2);
    assert(tz_cpu_select(3, "avx512") == -1);
    assert(tz_cpu_select(all, "avx512") == TZ_CPU_AVX512);
    assert(tz_cpu_select(0, "sve") == -1);
    assert(tz_cpu_select(TZ_CPU_HAS_SVE, "sve") == TZ_CPU_SVE);
    assert(tz_cpu_select(TZ_CPU_HAS_SVE, "sve2") == -1);
    assert(tz_cpu_select(all, "neon") == -1);
    /* A forced level admits older levels of its own family only. */
    assert(tz_cpu_within(TZ_CPU_SSE42, TZ_CPU_AVX2) && tz_cpu_within(TZ_CPU_AVX2, TZ_CPU_AVX2));
    assert(!tz_cpu_within(TZ_CPU_AVX512, TZ_CPU_AVX2) && !tz_cpu_within(TZ_CPU_AVX2, TZ_CPU_BASELINE));
    assert(tz_cpu_within(TZ_CPU_SVE, TZ_CPU_SVE2) && !tz_cpu_within(TZ_CPU_SVE2, TZ_CPU_SVE));
    assert(!tz_cpu_within(TZ_CPU_SVE, TZ_CPU_AVX512) && !tz_cpu_within(TZ_CPU_AVX2, TZ_CPU_SVE2));
    pthread_t threads[8];
    for (int index = 0; index < 8; ++index) assert(pthread_create(&threads[index], NULL, check_kernels, NULL) == 0);
    for (int index = 0; index < 8; ++index) assert(pthread_join(threads[index], NULL) == 0);
    /* A multiversioned function runs the newest clone the CPU supports, or the portable one. */
    uint64_t features = tsuzuri_cpu_features();
    int picked = tsuzuri_cpu_pick((UINT64_C(1) << TZ_CPU_AVX2) | (UINT64_C(1) << TZ_CPU_SVE));
    assert(picked == ((features & TZ_CPU_HAS_AVX2) ? TZ_CPU_AVX2 : (features & TZ_CPU_HAS_SVE) ? TZ_CPU_SVE : 0));
    assert(tsuzuri_cpu_pick(0) == TZ_CPU_BASELINE);
    printf("features=%llu variant=%d\n", (unsigned long long)features, tsuzuri_cpu_variant());
    return 0;
}
