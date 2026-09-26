#include <stdint.h>

extern uint64_t test_decode(const uint8_t *, uint64_t, uint16_t **);
extern uint64_t test_encode(const uint16_t *, uint64_t, uint8_t **);
extern uint64_t test_repair(const uint16_t *, uint64_t, uint16_t **);
extern int test_well_formed(const uint16_t *, uint64_t);
extern void test_drop(void *);
extern void test_allocate(uint64_t);
extern void test_concat(uint64_t, uint64_t);

static uint16_t units[512];
static uint8_t bytes[1024];

static void append_scalar(uint32_t scalar, uint64_t *unit_count, uint64_t *byte_count) {
    if (scalar <= 0xffff) {
        units[(*unit_count)++] = (uint16_t)scalar;
    } else {
        uint32_t remainder = scalar - 0x10000;
        units[(*unit_count)++] = (uint16_t)(0xd800 + remainder / 1024);
        units[(*unit_count)++] = (uint16_t)(0xdc00 + remainder % 1024);
    }
    if (scalar < 0x80) {
        bytes[(*byte_count)++] = (uint8_t)scalar;
    } else if (scalar < 0x800) {
        bytes[(*byte_count)++] = (uint8_t)(0xc0 + scalar / 64);
        bytes[(*byte_count)++] = (uint8_t)(0x80 + scalar % 64);
    } else if (scalar < 0x10000) {
        bytes[(*byte_count)++] = (uint8_t)(0xe0 + scalar / 4096);
        bytes[(*byte_count)++] = (uint8_t)(0x80 + scalar / 64 % 64);
        bytes[(*byte_count)++] = (uint8_t)(0x80 + scalar % 64);
    } else {
        bytes[(*byte_count)++] = (uint8_t)(0xf0 + scalar / 262144);
        bytes[(*byte_count)++] = (uint8_t)(0x80 + scalar / 4096 % 64);
        bytes[(*byte_count)++] = (uint8_t)(0x80 + scalar / 64 % 64);
        bytes[(*byte_count)++] = (uint8_t)(0x80 + scalar % 64);
    }
}

static int check_batch(uint64_t unit_count, uint64_t byte_count) {
    uint16_t *decoded;
    uint8_t *encoded;
    if (test_decode(bytes, byte_count, &decoded) != unit_count) return 1;
    for (uint64_t i = 0; i < unit_count; ++i) {
        if (decoded[i] != units[i]) return 2;
    }
    if (!test_well_formed(decoded, unit_count)) return 3;
    if (test_encode(units, unit_count, &encoded) != byte_count) return 4;
    for (uint64_t i = 0; i < byte_count; ++i) {
        if (encoded[i] != bytes[i]) return 5;
    }
    test_drop(decoded);
    test_drop(encoded);
    return 0;
}

int all_scalars(void) {
    uint64_t unit_count = 0, byte_count = 0;
    unsigned batch = 0;
    int batches = 0;
    if (check_batch(0, 0)) return -1;
    for (uint32_t scalar = 0; scalar <= 0x10ffff; ++scalar) {
        if (scalar >= 0xd800 && scalar <= 0xdfff) continue;
        append_scalar(scalar, &unit_count, &byte_count);
        if (++batch == 256 || scalar == 0x10ffff) {
            if (check_batch(unit_count, byte_count)) return -(int)(scalar + 1);
            unit_count = byte_count = 0;
            batch = 0;
            ++batches;
        }
    }
    return batches;
}

static const struct {
    unsigned length;
    uint8_t data[4];
} malformed_utf8[] = {
    {1, {0x80}},
    {2, {0xc0, 0x80}},
    {2, {0xc1, 0xbf}},
    {1, {0xc2}},
    {2, {0xc2, 0x20}},
    {3, {0xe0, 0x80, 0x80}},
    {3, {0xed, 0xa0, 0x80}},
    {3, {0xed, 0xbf, 0xbf}},
    {2, {0xe1, 0x80}},
    {3, {0xe1, 0x80, 0x7f}},
    {4, {0xf0, 0x80, 0x80, 0x80}},
    {4, {0xf4, 0x90, 0x80, 0x80}},
    {4, {0xf5, 0x80, 0x80, 0x80}},
    {1, {0xff}},
    {3, {0xf0, 0x90, 0x80}},
    {4, {0xf0, 0x90, 0x80, 0xff}},
    {1, {0xfe}},
};

static const struct {
    unsigned length;
    uint16_t data[2];
} malformed_utf16[] = {
    {1, {0xd800}},
    {1, {0xdc00}},
    {2, {0xd800, 0x0041}},
    {2, {0xd800, 0xd800}},
};

enum {
    UTF8_TRAPS = sizeof malformed_utf8 / sizeof malformed_utf8[0],
    UTF16_TRAPS = sizeof malformed_utf16 / sizeof malformed_utf16[0],
};

int runtime_trap_count(void) { return UTF8_TRAPS + UTF16_TRAPS + 7; }

void runtime_trap_case(unsigned index) {
    uint16_t *decoded;
    uint8_t *encoded;
    if (index < UTF8_TRAPS) {
        test_decode(malformed_utf8[index].data, malformed_utf8[index].length, &decoded);
        return;
    }
    index -= UTF8_TRAPS;
    if (index < UTF16_TRAPS) {
        test_encode(malformed_utf16[index].data, malformed_utf16[index].length, &encoded);
        return;
    }
    const uint64_t max_length = UINT64_C(9007199254740991);
    switch (index - UTF16_TRAPS) {
        case 0: test_allocate(max_length + 1); break;
        case 1: test_allocate(UINT64_MAX); break;
        case 2: test_concat(max_length, 1); break;
        case 3: test_concat(UINT64_MAX, 1); break;
        case 4: test_repair(0, max_length + 1, &decoded); break;
        case 5: test_encode(0, max_length + 1, &encoded); break;
        case 6: test_decode(0, max_length * 3 + 1, &decoded); break;
        default: __builtin_trap();
    }
}

#ifndef __wasm__
#include <assert.h>
#include <stdio.h>
#include <stdlib.h>

static uint64_t live;

void *tracked_alloc(uint64_t size) {
    uint64_t *p = malloc((size_t)size + 16);
    assert(p);
    p[0] = size;
    p[1] = UINT64_C(0x51a110ca7e);
    live += size;
    return p + 2;
}

void tracked_free(void *value) {
    if (!value) return;
    uint64_t *p = (uint64_t *)value - 2;
    assert(p[1] == UINT64_C(0x51a110ca7e) && live >= p[0]);
    p[1] = 0;
    live -= p[0];
    free(p);
}

int main(int argc, char **argv) {
    if (argc == 2) {
        runtime_trap_case((unsigned)atoi(argv[1]));
        return 0;
    }
    int batches = all_scalars();
    if (batches != 4344 || live != 0 || runtime_trap_count() != 28) {
        fprintf(stderr, "Unicode batches: %d; live bytes: %llu\n", batches, (unsigned long long)live);
        return 1;
    }
    return 0;
}
#endif
