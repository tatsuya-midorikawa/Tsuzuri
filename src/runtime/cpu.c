#if defined(_WIN32)
#define _CRT_SECURE_NO_WARNINGS
#endif
#include <stdint.h>
#include <stdatomic.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

/* The kernels are Clang vector code: the vector extension, __builtin_reduce_*, and
   __builtin_elementwise_* become typed LLVM intrinsics on every target (F08 D3). */
#if !defined(__clang__) || !defined(__has_builtin)
#error "Tsuzuri CPU runtime requires Clang vector builtins"
#elif !__has_builtin(__builtin_reduce_add) || !__has_builtin(__builtin_reduce_min) \
    || !__has_builtin(__builtin_reduce_or) || !__has_builtin(__builtin_elementwise_min)
#error "Tsuzuri CPU runtime requires Clang vector builtins"
#endif

#if (defined(__x86_64__) || defined(__i386__)) && !defined(_WIN32)
#define TZ_CPU_X86 1
#include <cpuid.h>
#else
#define TZ_CPU_X86 0
#endif

/* SVE is detected through the Linux auxiliary vector; other AArch64 systems report none. */
#if defined(__aarch64__) && defined(__linux__)
#define TZ_CPU_ARM_LINUX 1
unsigned long getauxval(unsigned long type);
#else
#define TZ_CPU_ARM_LINUX 0
#endif

#if defined(_WIN32)
#define TZ_CPU_API
#else
#define TZ_CPU_API __attribute__((weak, visibility("hidden")))
#endif

/* Levels: the value of tsuzuri_cpu_variant and the clone a kernel runs. */
#define TZ_CPU_BASELINE 0
#define TZ_CPU_SSE42 1
#define TZ_CPU_AVX2 2
#define TZ_CPU_AVX512 3
#define TZ_CPU_SVE 4
#define TZ_CPU_SVE2 5

/* Feature bits of tsuzuri_cpu_features. */
#define TZ_CPU_HAS_SSE42 UINT64_C(1)
#define TZ_CPU_HAS_AVX2 UINT64_C(2)
#define TZ_CPU_HAS_AVX512 UINT64_C(4)
#define TZ_CPU_HAS_SVE (UINT64_C(1) << 16)
#define TZ_CPU_HAS_SVE2 (UINT64_C(1) << 17)

/* AVX-512 is F, BW, CD, DQ, and VL (x86-64-v4) with the opmask and ZMM state enabled by the OS. */
static uint64_t tz_cpu_decode(unsigned features, unsigned extended, uint64_t xcr0) {
    uint64_t result = (features & (1u << 20)) ? TZ_CPU_HAS_SSE42 : 0;
    if ((features & (1u << 27)) && (features & (1u << 28)) && (xcr0 & 6) == 6 && (extended & (1u << 5))) {
        result |= TZ_CPU_HAS_AVX2;
        unsigned avx512 = (1u << 16) | (1u << 17) | (1u << 28) | (1u << 30) | (1u << 31);
        if ((extended & avx512) == avx512 && (xcr0 & 0xE6) == 0xE6) result |= TZ_CPU_HAS_AVX512;
    }
    return result;
}

/* Linux AArch64 HWCAP_SVE is bit 22 of AT_HWCAP; HWCAP2_SVE2 is bit 1 of AT_HWCAP2. */
__attribute__((unused)) static uint64_t tz_cpu_decode_arm(unsigned long hwcap, unsigned long hwcap2) {
    uint64_t result = 0;
    if (hwcap & (1ul << 22)) {
        result |= TZ_CPU_HAS_SVE;
        if (hwcap2 & (1ul << 1)) result |= TZ_CPU_HAS_SVE2;
    }
    return result;
}

TZ_CPU_API uint64_t tsuzuri_cpu_features(void) {
    static _Atomic uint64_t cached = UINT64_MAX;
    uint64_t value = atomic_load_explicit(&cached, memory_order_acquire);
    if (value != UINT64_MAX) return value;
    unsigned features = 0;
    unsigned extended = 0;
    uint64_t xcr0 = 0;
#if TZ_CPU_X86
    unsigned eax, ebx, ecx, edx;
    unsigned maximum = __get_cpuid_max(0, NULL);
    if (maximum >= 1) {
        __cpuid_count(1, 0, eax, ebx, ecx, edx);
        features = ecx;
        if ((ecx & (1u << 27)) && (ecx & (1u << 28))) {
            unsigned low, high;
            __asm__("xgetbv" : "=a"(low), "=d"(high) : "c"(0));
            xcr0 = ((uint64_t)high << 32) | low;
        }
    }
    if (maximum >= 7) { __cpuid_count(7, 0, eax, ebx, ecx, edx); extended = ebx; }
#endif
    value = tz_cpu_decode(features, extended, xcr0);
#if TZ_CPU_ARM_LINUX
    value |= tz_cpu_decode_arm(getauxval(16), getauxval(26));
#endif
    atomic_store_explicit(&cached, value, memory_order_release);
    return value;
}

static _Noreturn void tz_cpu_unavailable(void) {
    fputs("Tsuzuri CPU runtime: requested variant is unavailable or unknown\n", stderr);
    abort();
}

/* The feature a level needs; 0 for the baseline, which every CPU runs. */
static uint64_t tz_cpu_feature(int level) {
    switch (level) {
    case TZ_CPU_SSE42: return TZ_CPU_HAS_SSE42;
    case TZ_CPU_AVX2: return TZ_CPU_HAS_AVX2;
    case TZ_CPU_AVX512: return TZ_CPU_HAS_AVX512;
    case TZ_CPU_SVE: return TZ_CPU_HAS_SVE;
    case TZ_CPU_SVE2: return TZ_CPU_HAS_SVE2;
    default: return 0;
    }
}

/* The level TSUZURI_CPU_FORCE names, -2 when it is unset, or -1 when the CPU lacks it or the
   name is unknown. */
static int tz_cpu_forced_level(uint64_t features, const char *forced) {
    static const char *const names[] = { "baseline", "sse4.2", "avx2", "avx512", "sve", "sve2" };
    if (!forced) return -2;
    for (int level = TZ_CPU_BASELINE; level <= TZ_CPU_SVE2; ++level) {
        if (strcmp(forced, names[level]) == 0) {
            uint64_t feature = tz_cpu_feature(level);
            return (features & feature) == feature ? level : -1;
        }
    }
    return -1;
}

/* The kernels' level: TSUZURI_CPU_FORCE, or the best of SSE4.2 and AVX2. AVX-512 and SVE run
   only when forced, until they are measured (F08 D9). */
static int tz_cpu_select(uint64_t features, const char *forced) {
    int level = tz_cpu_forced_level(features, forced);
    if (level != -2) return level;
    if (features & TZ_CPU_HAS_AVX2) return TZ_CPU_AVX2;
    if (features & TZ_CPU_HAS_SSE42) return TZ_CPU_SSE42;
    return TZ_CPU_BASELINE;
}

/* TSUZURI_CPU_FORCE's level as tz_cpu_forced_level reported it, stored before the level is. */
static _Atomic int tz_cpu_forced_cache = -2;

/* Decided once: TSUZURI_CPU_FORCE is read on the first kernel call. */
static int tz_cpu_level(void) {
    static _Atomic int cached = -1;
    int level = atomic_load_explicit(&cached, memory_order_acquire);
    if (level >= 0) return level;
    uint64_t features = tsuzuri_cpu_features();
    const char *forced = getenv("TSUZURI_CPU_FORCE");
    level = tz_cpu_select(features, forced);
    if (level < 0) tz_cpu_unavailable();
    atomic_store_explicit(&tz_cpu_forced_cache, tz_cpu_forced_level(features, forced), memory_order_release);
    int expected = -1;
    if (!atomic_compare_exchange_strong_explicit(&cached, &expected, level, memory_order_acq_rel, memory_order_acquire)) level = expected;
    return level;
}

/* Whether code for `level` may run when TSUZURI_CPU_FORCE names `forced`: the same family
   (x86 or SVE), and no newer than the forced level. */
static int tz_cpu_within(int level, int forced) {
    int x86 = level >= TZ_CPU_SSE42 && level <= TZ_CPU_AVX512;
    int forced_x86 = forced >= TZ_CPU_SSE42 && forced <= TZ_CPU_AVX512;
    int arm = level >= TZ_CPU_SVE;
    int forced_arm = forced >= TZ_CPU_SVE;
    return ((x86 && forced_x86) || (arm && forced_arm)) && level <= forced;
}

/* A sum adds in the unsigned type, so it wraps without undefined behavior; addition is
   associative, so lane sums equal the left-to-right sum bit for bit. Blocks are read with
   memcpy only while a whole block remains. Four accumulators keep four blocks in flight, as
   one would wait for each add. */
#define TZ_CPU_SUM(NAME, T, U, BYTES, ATTR) \
    ATTR static T NAME(const T *values, int64_t length) { \
        typedef U tz_vector __attribute__((vector_size(BYTES))); \
        enum { lanes = BYTES / sizeof(T) }; \
        tz_vector first = {0}, second = {0}, third = {0}, fourth = {0}; \
        int64_t index = 0; \
        for (; length - index >= 4 * lanes; index += 4 * lanes) { \
            tz_vector block[4]; \
            memcpy(block, values + index, 4 * BYTES); \
            first += block[0]; \
            second += block[1]; \
            third += block[2]; \
            fourth += block[3]; \
        } \
        for (; length - index >= lanes; index += lanes) { \
            tz_vector value; \
            memcpy(&value, values + index, BYTES); \
            first += value; \
        } \
        U result = __builtin_reduce_add((first + second) + (third + fourth)); \
        for (; index < length; ++index) result += (U)values[index]; \
        return (T)result; \
    }

/* The first position of the minimum (BETTER `<`) or maximum (`>`): the value is unique whatever
   the order, so a second scan finds the same index as the strict left-to-right scan. The
   element type's signedness selects the signed or unsigned comparison. Both scans take four
   blocks at a time, like the sum. */
#define TZ_CPU_BEST(NAME, T, BYTES, ATTR, BETTER, ELEMENTWISE, REDUCE) \
    ATTR static int64_t NAME(const T *values, int64_t length) { \
        typedef T tz_vector __attribute__((vector_size(BYTES))); \
        enum { lanes = BYTES / sizeof(T) }; \
        if (length <= 0) return 0; \
        T best = values[0]; \
        int64_t index = 0; \
        if (length >= lanes) { \
            tz_vector first, second, third, fourth; \
            memcpy(&first, values, BYTES); \
            second = third = fourth = first; \
            for (index = lanes; length - index >= 4 * lanes; index += 4 * lanes) { \
                tz_vector block[4]; \
                memcpy(block, values + index, 4 * BYTES); \
                first = ELEMENTWISE(first, block[0]); \
                second = ELEMENTWISE(second, block[1]); \
                third = ELEMENTWISE(third, block[2]); \
                fourth = ELEMENTWISE(fourth, block[3]); \
            } \
            for (; length - index >= lanes; index += lanes) { \
                tz_vector value; \
                memcpy(&value, values + index, BYTES); \
                first = ELEMENTWISE(first, value); \
            } \
            best = REDUCE(ELEMENTWISE(ELEMENTWISE(first, second), ELEMENTWISE(third, fourth))); \
        } \
        for (; index < length; ++index) if (values[index] BETTER best) best = values[index]; \
        for (index = 0; length - index >= 4 * lanes; index += 4 * lanes) { \
            tz_vector block[4]; \
            memcpy(block, values + index, 4 * BYTES); \
            if (__builtin_reduce_or((block[0] == best) | (block[1] == best) | (block[2] == best) | (block[3] == best))) break; \
        } \
        for (; length - index >= lanes; index += lanes) { \
            tz_vector value; \
            memcpy(&value, values + index, BYTES); \
            if (__builtin_reduce_or(value == best)) break; \
        } \
        while (values[index] != best) ++index; \
        return index; \
    }

#define TZ_CPU_MIN(NAME, T, BYTES, ATTR) TZ_CPU_BEST(NAME, T, BYTES, ATTR, <, __builtin_elementwise_min, __builtin_reduce_min)
#define TZ_CPU_MAX(NAME, T, BYTES, ATTR) TZ_CPU_BEST(NAME, T, BYTES, ATTR, >, __builtin_elementwise_max, __builtin_reduce_max)

/* The 20 kernels of one level, named tz_cpu_<operation>_<type>_<level>. */
#define TZ_CPU_LEVEL_KERNELS(LEVEL, BYTES, ATTR) \
    TZ_CPU_SUM(tz_cpu_sum_i8_##LEVEL, int8_t, uint8_t, BYTES, ATTR) \
    TZ_CPU_SUM(tz_cpu_sum_i16_##LEVEL, int16_t, uint16_t, BYTES, ATTR) \
    TZ_CPU_SUM(tz_cpu_sum_i32_##LEVEL, int32_t, uint32_t, BYTES, ATTR) \
    TZ_CPU_SUM(tz_cpu_sum_i64_##LEVEL, int64_t, uint64_t, BYTES, ATTR) \
    TZ_CPU_MIN(tz_cpu_min_i8_##LEVEL, int8_t, BYTES, ATTR) \
    TZ_CPU_MIN(tz_cpu_min_i16_##LEVEL, int16_t, BYTES, ATTR) \
    TZ_CPU_MIN(tz_cpu_min_i32_##LEVEL, int32_t, BYTES, ATTR) \
    TZ_CPU_MIN(tz_cpu_min_i64_##LEVEL, int64_t, BYTES, ATTR) \
    TZ_CPU_MIN(tz_cpu_min_i8u_##LEVEL, uint8_t, BYTES, ATTR) \
    TZ_CPU_MIN(tz_cpu_min_i16u_##LEVEL, uint16_t, BYTES, ATTR) \
    TZ_CPU_MIN(tz_cpu_min_i32u_##LEVEL, uint32_t, BYTES, ATTR) \
    TZ_CPU_MIN(tz_cpu_min_i64u_##LEVEL, uint64_t, BYTES, ATTR) \
    TZ_CPU_MAX(tz_cpu_max_i8_##LEVEL, int8_t, BYTES, ATTR) \
    TZ_CPU_MAX(tz_cpu_max_i16_##LEVEL, int16_t, BYTES, ATTR) \
    TZ_CPU_MAX(tz_cpu_max_i32_##LEVEL, int32_t, BYTES, ATTR) \
    TZ_CPU_MAX(tz_cpu_max_i64_##LEVEL, int64_t, BYTES, ATTR) \
    TZ_CPU_MAX(tz_cpu_max_i8u_##LEVEL, uint8_t, BYTES, ATTR) \
    TZ_CPU_MAX(tz_cpu_max_i16u_##LEVEL, uint16_t, BYTES, ATTR) \
    TZ_CPU_MAX(tz_cpu_max_i32u_##LEVEL, uint32_t, BYTES, ATTR) \
    TZ_CPU_MAX(tz_cpu_max_i64u_##LEVEL, uint64_t, BYTES, ATTR)

/* The baseline is the target's own 128-bit vectors: NEON on AArch64, SSE2 (SSE4.1 on macOS)
   on x86-64. */
TZ_CPU_LEVEL_KERNELS(baseline, 16, __attribute__((noinline)))

#if TZ_CPU_X86
#define TZ_CPU_TARGET_SSE42 __attribute__((target("sse4.2"), noinline))
#define TZ_CPU_TARGET_AVX2 __attribute__((target("avx2"), noinline))
#define TZ_CPU_TARGET_AVX512 __attribute__((target("avx512f,avx512bw,avx512cd,avx512dq,avx512vl"), noinline))
TZ_CPU_LEVEL_KERNELS(sse42, 16, TZ_CPU_TARGET_SSE42)
TZ_CPU_LEVEL_KERNELS(avx2, 32, TZ_CPU_TARGET_AVX2)
TZ_CPU_LEVEL_KERNELS(avx512, 64, TZ_CPU_TARGET_AVX512)

/* Returns the kernel of the current level. */
#define TZ_CPU_PICK(name, ...) \
    switch (tz_cpu_level()) { \
    case TZ_CPU_AVX512: return name##_avx512(__VA_ARGS__); \
    case TZ_CPU_AVX2: return name##_avx2(__VA_ARGS__); \
    case TZ_CPU_SSE42: return name##_sse42(__VA_ARGS__); \
    default: return name##_baseline(__VA_ARGS__); \
    }
#else
/* The level still validates TSUZURI_CPU_FORCE; SVE has no kernel of its own yet. */
#define TZ_CPU_PICK(name, ...) \
    do { \
        (void)tz_cpu_level(); \
        return name##_baseline(__VA_ARGS__); \
    } while (0)
#endif

TZ_CPU_API int tsuzuri_cpu_variant(void) {
    return tz_cpu_level();
}

#define TZ_CPU_ENTRY(OPERATION, TYPE, T, RESULT) \
    TZ_CPU_API RESULT tsuzuri_cpu_##OPERATION##_##TYPE(const T *values, int64_t length) { \
        if (length < 0) abort(); \
        TZ_CPU_PICK(tz_cpu_##OPERATION##_##TYPE, values, length); \
    }

TZ_CPU_ENTRY(sum, i8, int8_t, int8_t)
TZ_CPU_ENTRY(sum, i16, int16_t, int16_t)
TZ_CPU_ENTRY(sum, i32, int32_t, int32_t)
TZ_CPU_ENTRY(sum, i64, int64_t, int64_t)
TZ_CPU_ENTRY(min, i8, int8_t, int64_t)
TZ_CPU_ENTRY(min, i16, int16_t, int64_t)
TZ_CPU_ENTRY(min, i32, int32_t, int64_t)
TZ_CPU_ENTRY(min, i64, int64_t, int64_t)
TZ_CPU_ENTRY(min, i8u, uint8_t, int64_t)
TZ_CPU_ENTRY(min, i16u, uint16_t, int64_t)
TZ_CPU_ENTRY(min, i32u, uint32_t, int64_t)
TZ_CPU_ENTRY(min, i64u, uint64_t, int64_t)
TZ_CPU_ENTRY(max, i8, int8_t, int64_t)
TZ_CPU_ENTRY(max, i16, int16_t, int64_t)
TZ_CPU_ENTRY(max, i32, int32_t, int64_t)
TZ_CPU_ENTRY(max, i64, int64_t, int64_t)
TZ_CPU_ENTRY(max, i8u, uint8_t, int64_t)
TZ_CPU_ENTRY(max, i16u, uint16_t, int64_t)
TZ_CPU_ENTRY(max, i32u, uint32_t, int64_t)
TZ_CPU_ENTRY(max, i64u, uint64_t, int64_t)

/* UTF conversions share one validating/counting pass and one writing pass.
   output == NULL never writes; a conversion allocates only after validation.
   A failed vector probe transfers once to the scalar remainder. */
static int tz_utf8_scalar(const uint8_t *s, uint64_t n, uint64_t *index, uint32_t *value) {
    uint64_t i = *index;
    uint32_t first = s[i++];
    if (first < 128) { *index = i; *value = first; return 1; }
    unsigned width = first >= 194 && first <= 223 ? 2 :
        first >= 224 && first <= 239 ? 3 : first >= 240 && first <= 244 ? 4 : 0;
    if (!width || n - *index < width) return 0;
    uint32_t scalar = first & (127u >> width);
    for (unsigned j = 1; j < width; ++j) {
        uint32_t b = s[i++];
        if ((b & 192) != 128) return 0;
        scalar = (scalar << 6) | (b & 63);
    }
    uint32_t minimum = width == 2 ? 128 : width == 3 ? 2048 : 65536;
    if (scalar < minimum || scalar > 0x10ffff || scalar - 0xd800 < 2048) return 0;
    *index = i; *value = scalar;
    return 1;
}

#define TZ_CPU_UTF8(NAME, BYTES, ATTR) \
    ATTR static int64_t NAME(const uint8_t *source, uint64_t length, uint16_t *output, int validate) { \
        typedef uint8_t vector __attribute__((ext_vector_type(BYTES))); \
        uint64_t i = 0, count = 0; \
        if (!validate && length > UINT64_C(27021597764222973)) return -1; \
        while (i < length) { \
            if (length - i >= BYTES) { \
                vector v; memcpy(&v, source + i, BYTES); \
                if (!__builtin_reduce_or(v & (vector)128)) { \
                    if (output) { \
                        typedef uint16_t wide __attribute__((ext_vector_type(BYTES))); \
                        wide units = __builtin_convertvector(v, wide); \
                        memcpy(output + count, &units, 2 * BYTES); \
                    } \
                    i += BYTES; count += BYTES; \
                    if (!validate && count > UINT64_C(9007199254740991)) return -1; \
                    continue; \
                } \
                typedef uint16_t pairs __attribute__((ext_vector_type(BYTES / 2))); \
                pairs lead, tail; \
                for (unsigned j = 0; j < BYTES / 2; ++j) { lead[j] = v[2*j]; tail[j] = v[2*j+1]; } \
                if (__builtin_reduce_and((lead >= (pairs)194) & (lead <= (pairs)223) \
                    & ((tail & (pairs)192) == (pairs)128))) { \
                    pairs units = ((lead & (pairs)31) << 6) | (tail & (pairs)63); \
                    if (output) memcpy(output + count, &units, BYTES); \
                    i += BYTES; count += BYTES / 2; \
                    if (!validate && count > UINT64_C(9007199254740991)) return -1; \
                    continue; \
                } \
                typedef uint32_t triples __attribute__((ext_vector_type(BYTES / 4))); \
                triples a, b, c; \
                for (unsigned j = 0; j < BYTES / 4; ++j) { a[j] = v[3*j]; b[j] = v[3*j+1]; c[j] = v[3*j+2]; } \
                triples scalars = ((a & (triples)15) << 12) | ((b & (triples)63) << 6) | (c & (triples)63); \
                if (__builtin_reduce_and((a >= (triples)224) & (a <= (triples)239) \
                    & ((b & (triples)192) == (triples)128) & ((c & (triples)192) == (triples)128) \
                    & (scalars >= (triples)2048) & ((scalars - (triples)0xd800) >= (triples)2048))) { \
                    if (output) { \
                        typedef uint16_t units __attribute__((ext_vector_type(BYTES / 4))); \
                        units converted = __builtin_convertvector(scalars, units); \
                        memcpy(output + count, &converted, BYTES / 2); \
                    } \
                    i += 3 * BYTES / 4; count += BYTES / 4; \
                    if (!validate && count > UINT64_C(9007199254740991)) return -1; \
                    continue; \
                } \
                if (!output) { \
                    vector p1 = {0}, p2 = {0}, p3 = {0}; \
                    for (unsigned j = 1; j < BYTES; ++j) p1[j] = v[j-1]; \
                    for (unsigned j = 2; j < BYTES; ++j) p2[j] = v[j-2]; \
                    for (unsigned j = 3; j < BYTES; ++j) p3[j] = v[j-3]; \
                    vector expected = (vector)((p1 >= (vector)194) & (p1 <= (vector)244)) \
                        | (vector)((p2 >= (vector)224) & (p2 <= (vector)244)) \
                        | (vector)((p3 >= (vector)240) & (p3 <= (vector)244)); \
                    vector actual = (vector)((v & (vector)192) == (vector)128); \
                    vector bad = expected ^ actual; \
                    bad |= (vector)((v == (vector)192) | (v == (vector)193) | (v >= (vector)245)); \
                    bad |= (vector)((p1 == (vector)224) & (v < (vector)160)); \
                    bad |= (vector)((p1 == (vector)237) & (v >= (vector)160)); \
                    bad |= (vector)((p1 == (vector)240) & (v < (vector)144)); \
                    bad |= (vector)((p1 == (vector)244) & (v >= (vector)144)); \
                    int complete = v[BYTES-1] < 194 && v[BYTES-2] < 224 && v[BYTES-3] < 240; \
                    if (complete && !__builtin_reduce_or(bad)) { \
                        vector widths = ((vector)((v < (vector)128) | (v >= (vector)194)) & (vector)1) \
                            + ((vector)(v >= (vector)240) & (vector)1); \
                        count += __builtin_reduce_add(widths); i += BYTES; \
                        if (!validate && count > UINT64_C(9007199254740991)) return -1; \
                        continue; \
                    } \
                } \
            } \
            break; \
        } \
        while (i < length) { \
                uint32_t scalar; \
                if (!tz_utf8_scalar(source, length, &i, &scalar)) return -1; \
                if (scalar < 65536) { \
                    if (output) output[count] = (uint16_t)scalar; \
                    ++count; \
                } else { \
                    scalar -= 65536; \
                    if (output) { output[count] = (uint16_t)(0xd800 + (scalar >> 10)); \
                        output[count + 1] = (uint16_t)(0xdc00 + (scalar & 1023)); } \
                    count += 2; \
                } \
                if (!validate && count > UINT64_C(9007199254740991)) return -1; \
        } \
        return validate ? 0 : (int64_t)count; \
    }

#define TZ_CPU_UTF16(NAME, BYTES, ATTR) \
    ATTR static int64_t NAME(const uint16_t *source, uint64_t length, uint8_t *output) { \
        enum { lanes = BYTES / 2 }; \
        typedef uint16_t vector __attribute__((ext_vector_type(lanes))); \
        typedef uint8_t narrow __attribute__((ext_vector_type(lanes))); \
        uint64_t i = 0, count = 0; \
        if (length > UINT64_C(9007199254740991)) return -1; \
        while (i < length) { \
            if (length - i >= lanes) { \
                vector v; memcpy(&v, source + i, BYTES); \
                if (!__builtin_reduce_or(v & (vector)0xff80)) { \
                    if (output) { narrow b = __builtin_convertvector(v, narrow); memcpy(output + count, &b, lanes); } \
                    i += lanes; count += lanes; continue; \
                } \
                if (__builtin_reduce_and((v & (vector)0xf800) != (vector)0xd800)) { \
                    vector widths = (vector)1 + (vector)((v >= (vector)128) & (vector)1) \
                        + (vector)((v >= (vector)2048) & (vector)1); \
                    if (!output) { count += __builtin_reduce_add(widths); i += lanes; continue; } \
                    unsigned width = __builtin_reduce_and((v >= (vector)128) & (v < (vector)2048)) ? 2 \
                        : __builtin_reduce_and(v >= (vector)2048) ? 3 : 0; \
                    if (width) { \
                        narrow first = __builtin_convertvector((v >> (width == 2 ? 6 : 12)) \
                            | (vector)(width == 2 ? 192 : 224), narrow); \
                        narrow middle = __builtin_convertvector(((v >> 6) & (vector)63) | (vector)128, narrow); \
                        narrow last = __builtin_convertvector((v & (vector)63) | (vector)128, narrow); \
                        uint8_t packed[3 * lanes]; \
                        if (width == 2) { \
                            for (unsigned j = 0; j < lanes; ++j) { \
                                packed[2*j] = first[j]; packed[2*j+1] = last[j]; \
                            } \
                        } else { \
                            for (unsigned j = 0; j < lanes; ++j) { \
                                packed[3*j] = first[j]; packed[3*j+1] = middle[j]; packed[3*j+2] = last[j]; \
                            } \
                        } \
                        memcpy(output + count, packed, width * lanes); \
                        count += width * lanes; i += lanes; continue; \
                    } \
                } \
            } \
            break; \
        } \
        while (i < length) { \
                uint32_t scalar = source[i++]; \
                if (scalar - 0xd800 < 2048) { \
                    if (scalar >= 0xdc00 || i == length || (uint32_t)source[i] - 0xdc00 >= 1024) return -1; \
                    scalar = 65536 + ((scalar - 0xd800) << 10) + (source[i++] - 0xdc00); \
                } \
                unsigned width = scalar < 128 ? 1 : scalar < 2048 ? 2 : scalar < 65536 ? 3 : 4; \
                if (output) { \
                    uint32_t bits = scalar; \
                    for (unsigned j = width - 1; j; --j) { output[count + j] = (uint8_t)(128 | (bits & 63)); bits >>= 6; } \
                    output[count] = (uint8_t)(bits | (width == 1 ? 0 : width == 2 ? 192 : width == 3 ? 224 : 240)); \
                } \
                count += width; \
        } \
        return (int64_t)count; \
    }

#define TZ_CPU_UTF_LEVEL(LEVEL, BYTES, ATTR) \
    TZ_CPU_UTF8(tz_cpu_utf8_to_utf16_##LEVEL, BYTES, ATTR) \
    TZ_CPU_UTF16(tz_cpu_utf16_to_utf8_##LEVEL, BYTES, ATTR) \
    ATTR static int tz_cpu_utf8_validate_##LEVEL(const uint8_t *source, uint64_t length) { \
        return tz_cpu_utf8_to_utf16_##LEVEL(source, length, NULL, 1) >= 0; \
    }

TZ_CPU_UTF_LEVEL(baseline, 16, __attribute__((noinline, aligned(64))))
#if TZ_CPU_X86
TZ_CPU_UTF_LEVEL(sse42, 16, TZ_CPU_TARGET_SSE42)
TZ_CPU_UTF_LEVEL(avx2, 32, TZ_CPU_TARGET_AVX2)
TZ_CPU_UTF_LEVEL(avx512, 64, TZ_CPU_TARGET_AVX512)
#endif

TZ_CPU_API int64_t tsuzuri_cpu_utf8_to_utf16(const uint8_t *source, uint64_t length, uint16_t *output) {
    TZ_CPU_PICK(tz_cpu_utf8_to_utf16, source, length, output, 0);
}
TZ_CPU_API int64_t tsuzuri_cpu_utf16_to_utf8(const uint16_t *source, uint64_t length, uint8_t *output) {
    TZ_CPU_PICK(tz_cpu_utf16_to_utf8, source, length, output);
}
TZ_CPU_API int tsuzuri_cpu_utf8_validate(const uint8_t *source, uint64_t length) {
    TZ_CPU_PICK(tz_cpu_utf8_validate, source, length);
}

/* The version of a multiversioned function to run (F08 Phase 3). Bit L of `clones` is set for
   each level L the function was compiled for; the result is the newest of them that the CPU
   supports, no newer than TSUZURI_CPU_FORCE, or 0 for the portable version. */
TZ_CPU_API int tsuzuri_cpu_pick(uint64_t clones) {
    static const int newest_first[] = { TZ_CPU_SVE2, TZ_CPU_SVE, TZ_CPU_AVX512, TZ_CPU_AVX2, TZ_CPU_SSE42 };
    (void)tz_cpu_level(); /* validates TSUZURI_CPU_FORCE as the kernels do */
    int forced = atomic_load_explicit(&tz_cpu_forced_cache, memory_order_acquire);
    uint64_t features = tsuzuri_cpu_features();
    for (size_t index = 0; index < sizeof(newest_first) / sizeof(newest_first[0]); ++index) {
        int level = newest_first[index];
        uint64_t feature = tz_cpu_feature(level);
        if (!(clones & (UINT64_C(1) << level)) || (features & feature) != feature) continue;
        if (forced != -2 && !tz_cpu_within(level, forced)) continue;
        return level;
    }
    return TZ_CPU_BASELINE;
}
