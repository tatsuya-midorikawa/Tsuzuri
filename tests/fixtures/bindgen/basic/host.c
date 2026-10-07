/* Round-trip check for tests/bindgen.mjs: every value that Tsuzuri gets through the
   generated externs must equal the direct C call and the value worked out by hand. */
#include <stdint.h>
#include <stdio.h>
#include "sample.h"
#include "tz-basic.h"

int sample_reset_count(void);

static int failures;
#define CHECK(condition) \
    do { \
        if (!(condition)) { \
            fprintf(stderr, "%s:%d: check failed: %s\n", __FILE__, __LINE__, #condition); \
            failures++; \
        } \
    } while (0)

int main(void) {
    CHECK(tz_add_case(40, 2) == sample_add(40, 2));
    CHECK(tz_add_case(40, 2) == 42);
    CHECK(tz_add_case(-5, 3) == -2);
    struct sample_point point = {3.0, 4.0, 0};
    CHECK(tz_norm_case(3.0, 4.0, 0) == sample_norm(&point));
    CHECK(tz_norm_case(3.0, 4.0, 0) == 5.0);
    CHECK(tz_norm_case(6.0, 8.0, -3) == 7.0);
    CHECK(tz_low_byte_case(0x1234) == sample_low_byte(0x1234));
    CHECK(tz_low_byte_case(0x1234) == 0x34);
    CHECK(tz_low_byte_case(UINT64_C(0xFFFFFFFFFFFFFF80)) == 0x80);
    CHECK(tz_even_case(-4) == 1);
    CHECK(tz_even_case(7) == 0);
    CHECK(tz_even_case(INT64_MIN) == 1);
    CHECK(tz_next_case(SAMPLE_SLOW) == sample_next(SAMPLE_SLOW));
    CHECK(tz_next_case(SAMPLE_SLOW) == SAMPLE_BEST);
    CHECK(tz_next_case(SAMPLE_FAST) == 5);
    int before = sample_reset_count();
    CHECK(tz_reset_twice(40) == 42);
    CHECK(sample_reset_count() == before + 2);
    CHECK(tz_constants_case(1) == 1 + SAMPLE_FAST + SAMPLE_SLOW * 10 + SAMPLE_BEST * 100);
    CHECK(tz_constants_case(1) == 651);
    if (failures == 0) puts("bindgen round trip ok");
    return failures != 0;
}
