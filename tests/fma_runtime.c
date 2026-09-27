#include <assert.h>
#include <math.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include "../src/runtime/numeric.c"

static uint64_t random_state = UINT64_C(0x7aa5138159f0145b);
static uint64_t next_bits(void) { random_state = random_state * UINT64_C(6364136223846793005) + 1442695040888963407; return random_state; }

static void check_double(uint64_t left_bits, uint64_t right_bits, uint64_t addend_bits) {
    double left, right, addend, result;
    memcpy(&left, &left_bits, 8); memcpy(&right, &right_bits, 8); memcpy(&addend, &addend_bits, 8);
    double expected = fma(left, right, addend);
    tz_soft_fma((unsigned char *)&result, (unsigned char *)&left, (unsigned char *)&right, (unsigned char *)&addend, 2);
    assert((isnan(result) && isnan(expected)) || memcmp(&result, &expected, 8) == 0);
}

static void check_single(uint32_t left_bits, uint32_t right_bits, uint32_t addend_bits) {
    float left, right, addend, result;
    memcpy(&left, &left_bits, 4); memcpy(&right, &right_bits, 4); memcpy(&addend, &addend_bits, 4);
    float expected = fmaf(left, right, addend);
    tz_soft_fma((unsigned char *)&result, (unsigned char *)&left, (unsigned char *)&right, (unsigned char *)&addend, 1);
    assert((isnan(result) && isnan(expected)) || memcmp(&result, &expected, 4) == 0);
}

int main(void) {
    uint64_t specials[] = {0, UINT64_C(0x8000000000000000), 1, UINT64_C(0x8000000000000001), UINT64_C(0x7ff0000000000000), UINT64_C(0xfff0000000000000), UINT64_C(0x7ff8000000000000), UINT64_C(0x3ff0000000000000)};
    for (unsigned left = 0; left < 8; ++left) for (unsigned right = 0; right < 8; ++right) for (unsigned addend = 0; addend < 8; ++addend) check_double(specials[left], specials[right], specials[addend]);
    check_double(UINT64_C(0x3ff0000000000001), UINT64_C(0x3feffffffffffffe), UINT64_C(0xbff0000000000000));
    for (int index = 0; index < 10000; ++index) {
        uint64_t left = next_bits(), right = next_bits(), addend = next_bits();
        check_double(left, right, addend);
        check_single((uint32_t)left, (uint32_t)right, (uint32_t)addend);
    }
    puts("soft FMA: 20513 native references passed");
    return 0;
}
