#if defined(_WIN32)
#define _CRT_SECURE_NO_WARNINGS
#endif
#include <stdint.h>
#include <stdatomic.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#if (defined(__x86_64__) || defined(__i386__)) && !defined(_WIN32)
#define TZ_CPU_X86 1
#include <cpuid.h>
#include <immintrin.h>
#else
#define TZ_CPU_X86 0
#endif

#if defined(_WIN32)
#define TZ_CPU_API
#else
#define TZ_CPU_API __attribute__((weak, visibility("hidden")))
#endif

static uint64_t tz_cpu_decode(unsigned features, unsigned extended, uint64_t xcr0) {
    uint64_t result = (features & (1u << 20)) ? 1 : 0;
    if ((features & (1u << 27)) && (features & (1u << 28)) && (xcr0 & 6) == 6 && (extended & (1u << 5))) result |= 2;
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
    atomic_store_explicit(&cached, value, memory_order_release);
    return value;
}

static int64_t tz_cpu_bits(uint64_t value) {
    int64_t result;
    memcpy(&result, &value, sizeof(result));
    return result;
}

static int64_t tz_cpu_sum_baseline(const int64_t *values, int64_t length) {
    uint64_t total = 0;
    for (int64_t index = 0; index < length; ++index) total += (uint64_t)values[index];
    return tz_cpu_bits(total);
}

#if TZ_CPU_X86
__attribute__((target("sse4.2"), noinline))
static int64_t tz_cpu_sum_sse42(const int64_t *values, int64_t length) {
    __m128i total = _mm_setzero_si128();
    int64_t index = 0;
    for (; index <= length - 2; index += 2) {
        __m128i value;
        memcpy(&value, values + index, sizeof(value));
        total = _mm_add_epi64(total, value);
    }
    uint64_t lanes[2];
    memcpy(lanes, &total, sizeof(lanes));
    uint64_t result = lanes[0] + lanes[1];
    for (; index < length; ++index) result += (uint64_t)values[index];
    return tz_cpu_bits(result);
}

__attribute__((target("avx2"), noinline))
static int64_t tz_cpu_sum_avx2(const int64_t *values, int64_t length) {
    __m256i total = _mm256_setzero_si256();
    int64_t index = 0;
    for (; index <= length - 4; index += 4) {
        __m256i value;
        memcpy(&value, values + index, sizeof(value));
        total = _mm256_add_epi64(total, value);
    }
    uint64_t lanes[4];
    memcpy(lanes, &total, sizeof(lanes));
    uint64_t result = lanes[0] + lanes[1] + lanes[2] + lanes[3];
    for (; index < length; ++index) result += (uint64_t)values[index];
    return tz_cpu_bits(result);
}
#endif

typedef int64_t (*tz_sum_function)(const int64_t *, int64_t);
static _Atomic(tz_sum_function) tz_cpu_sum_cached = NULL;

static _Noreturn void tz_cpu_unavailable(void) {
    fputs("Tsuzuri CPU runtime: requested variant is unavailable or unknown\n", stderr);
    abort();
}

static tz_sum_function tz_cpu_resolve(void) {
    tz_sum_function function = atomic_load_explicit(&tz_cpu_sum_cached, memory_order_acquire);
    if (function) return function;
    uint64_t features = tsuzuri_cpu_features();
    function = tz_cpu_sum_baseline;
    const char *forced = getenv("TSUZURI_CPU_FORCE");
    if (forced && strcmp(forced, "baseline") == 0) features = 0;
    else if (forced && strcmp(forced, "sse4.2") == 0) { if (!(features & 1)) tz_cpu_unavailable(); features = 1; }
    else if (forced && strcmp(forced, "avx2") == 0) { if (!(features & 2)) tz_cpu_unavailable(); features = 2; }
    else if (forced) tz_cpu_unavailable();
#if TZ_CPU_X86
    if (features & 2) function = tz_cpu_sum_avx2;
    else if (features & 1) function = tz_cpu_sum_sse42;
#else
    (void)features;
#endif
    tz_sum_function expected = NULL;
    if (!atomic_compare_exchange_strong_explicit(&tz_cpu_sum_cached, &expected, function, memory_order_acq_rel, memory_order_acquire)) function = expected;
    return function;
}

TZ_CPU_API int tsuzuri_cpu_variant(void) {
    tz_sum_function function = tz_cpu_resolve();
#if TZ_CPU_X86
    if (function == tz_cpu_sum_avx2) return 2;
    if (function == tz_cpu_sum_sse42) return 1;
#else
    (void)function;
#endif
    return 0;
}

TZ_CPU_API int64_t tsuzuri_cpu_sum_i64(const int64_t *values, int64_t length) {
    if (length < 0) abort();
    return tz_cpu_resolve()(values, length);
}
