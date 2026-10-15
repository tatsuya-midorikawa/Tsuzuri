#include <inttypes.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

extern int tz_soft_format(char *, const unsigned char *, int);
extern int tz_soft_parse(unsigned char *, const char *, uint64_t, int);
extern int tz_base_format(char *, const unsigned char *, int);
extern int tz_base_parse(unsigned char *, const char *, uint64_t, int);

static uint64_t random_raw(uint64_t index) {
    uint64_t z = index + UINT64_C(0x9e3779b97f4a7c15);
    z = (z ^ (z >> 30)) * UINT64_C(0xbf58476d1ce4e5b9);
    z = (z ^ (z >> 27)) * UINT64_C(0x94d049bb133111eb);
    return z ^ (z >> 31);
}

int main(int argc, char **argv) {
    if (argc != 4) return 2;
    int mode = atoi(argv[1]), kind = mode == 2 ? 2 : 1, width = kind == 1 ? 4 : 8;
    uint64_t start = strtoull(argv[2], 0, 10), count = strtoull(argv[3], 0, 10);
    static const uint32_t boundaries[] = {
        0, 0x80000000, 0x7f800000, 0xff800000, 0x7fc00000, 0xffc00000,
        1, 0x80000001, 0x007fffff, 0x00800000, 0x00800001,
        0x3dcccccd, 0x3f800000, 0x3f800001, 0x7f7fffff, 0xff7fffff,
    };
    uint64_t mismatches = 0;
    for (uint64_t i = 0; i < count; ++i) {
        uint64_t raw = mode == 1 ? start + i : random_raw(start + i);
        if (mode == 3) raw = start + i < sizeof(boundaries) / sizeof(boundaries[0])
            ? boundaries[start + i] : (uint32_t)raw;
        unsigned char input[16] = {0}, parsed[16] = {0}, old_parsed[16] = {0};
        for (int byte = 0; byte < width; ++byte) input[byte] = (unsigned char)(raw >> (8 * byte));
        char text[128], old_text[128];
        int length = tz_soft_format(text, input, kind);
        int old_length = tz_base_format(old_text, input, kind);
        int valid_length = length > 0 && length <= 128 && old_length > 0 && old_length <= 128;
        int same_text = valid_length && length == old_length && !memcmp(text, old_text, (size_t)length);
        int ok = valid_length ? tz_soft_parse(parsed, text, (uint64_t)length, kind) : 0;
        int old_ok = valid_length ? tz_base_parse(old_parsed, text, (uint64_t)length, kind) : 0;
        int nan = kind == 1 ? (raw & UINT64_C(0x7fffffff)) > UINT64_C(0x7f800000)
                            : (raw & UINT64_C(0x7fffffffffffffff)) > UINT64_C(0x7ff0000000000000);
        uint64_t expected = nan ? (kind == 1 ? UINT64_C(0x7fc00000) : UINT64_C(0x7ff8000000000000)) : raw;
        int round_trip = 1;
        for (int byte = 0; byte < width; ++byte)
            if (parsed[byte] != (unsigned char)(expected >> (8 * byte))) round_trip = 0;
        if (!same_text || !ok || ok != old_ok || memcmp(parsed, old_parsed, (size_t)width) || !round_trip) {
            fprintf(stderr, "f%d raw=%016" PRIx64 " format=%d parse=%d/%d round-trip=%d\n",
                    kind == 1 ? 32 : 64, raw, same_text, ok, old_ok, round_trip);
            if (++mismatches == 10) break;
        }
    }
    printf("%" PRIu64 " %" PRIu64 "\n", count, mismatches);
    return mismatches ? 1 : 0;
}
