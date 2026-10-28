/* The host of tests/overflow_report.mjs: calls each export of Main.tz once with an input that
   wraps and once with one that does not, and prints the results. The report goes to stderr. */
#include <inttypes.h>
#include <stdint.h>
#include <stdio.h>

int32_t tz_add32(int32_t, int32_t);
uint32_t tz_sub32u(uint32_t, uint32_t);
int32_t tz_mul8(int32_t, int32_t);
int64_t tz_neg64(int64_t);
int32_t tz_pow16(int32_t, int32_t);
int64_t tz_wide64(int64_t);
int32_t tz_tail32(int32_t);
int32_t tz_masked(int32_t);
int32_t tz_saturated(int32_t, int32_t);

int main(void) {
    printf("%" PRId32 " %" PRId32 "\n", tz_add32(INT32_MAX, 1), tz_add32(1, 2));
    printf("%" PRIu32 " %" PRIu32 "\n", tz_sub32u(0, 1), tz_sub32u(5, 3));
    printf("%" PRId32 " %" PRId32 "\n", tz_mul8(100, 100), tz_mul8(3, 4));
    printf("%" PRId64 " %" PRId64 "\n", tz_neg64(INT64_MIN), tz_neg64(5));
    printf("%" PRId32 " %" PRId32 "\n", tz_pow16(200, 2), tz_pow16(3, 2));
    printf("%" PRId64 " %" PRId64 "\n", tz_wide64(INT64_C(4611686018427387904)), tz_wide64(1));
    printf("%" PRId32 " %" PRId32 "\n", tz_tail32(3), tz_tail32(2));
    printf("%" PRId32 " %" PRId32 "\n", tz_masked(1000), tz_saturated(INT32_MAX, 1));
    return 0;
}
