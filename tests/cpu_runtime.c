#include <assert.h>
#include <pthread.h>
#include "../src/runtime/cpu.c"

static void *check_sums(void *unused) {
    (void)unused;
    int64_t values[259];
    for (int index = 0; index < 259; ++index) values[index] = tz_cpu_bits(UINT64_MAX - (uint64_t)index * 37);
    for (int length = 0; length < 257; ++length) {
        int64_t expected = tz_cpu_sum_baseline(values + 1, length);
        assert(tsuzuri_cpu_sum_i64(values + 1, length) == expected);
#if TZ_CPU_X86
        if (tsuzuri_cpu_features() & 1) assert(tz_cpu_sum_sse42(values + 1, length) == expected);
        if (tsuzuri_cpu_features() & 2) assert(tz_cpu_sum_avx2(values + 1, length) == expected);
#endif
    }
    return NULL;
}

int main(void) {
    unsigned avx = (1u << 27) | (1u << 28);
    assert(tz_cpu_decode(0, 0, 0) == 0);
    assert(tz_cpu_decode(1u << 20, 0, 0) == 1);
    assert(tz_cpu_decode(avx, 1u << 5, 6) == 2);
    assert(tz_cpu_decode(avx, 1u << 5, 2) == 0);
    assert(tz_cpu_decode(1u << 28, 1u << 5, 6) == 0);
    pthread_t threads[8];
    for (int index = 0; index < 8; ++index) assert(pthread_create(&threads[index], NULL, check_sums, NULL) == 0);
    for (int index = 0; index < 8; ++index) assert(pthread_join(threads[index], NULL) == 0);
    printf("features=%llu variant=%d\n", (unsigned long long)tsuzuri_cpu_features(), tsuzuri_cpu_variant());
    return 0;
}
