#include <stdint.h>

extern uint64_t test_decode(const uint8_t *, uint64_t, uint16_t **);
extern uint64_t test_encode(const uint16_t *, uint64_t, uint8_t **);
extern uint64_t test_repair(const uint16_t *, uint64_t, uint16_t **);
extern int test_well_formed(const uint16_t *, uint64_t);
extern void test_drop(void *);
extern void test_allocate(uint64_t);
extern void test_concat(uint64_t, uint64_t);
extern uint64_t test_concat_into16(void *, uint64_t, const void *, uint64_t, const void *, uint64_t, void **);
extern uint64_t test_concat_into8(void *, uint64_t, const void *, uint64_t, const void *, uint64_t, void **);
extern uint64_t test_append16(void *, uint64_t, const void *, uint64_t, void **);
extern uint64_t test_append8(void *, uint64_t, const void *, uint64_t, void **);
extern uint64_t test_clone16(const void *, uint64_t, void **);
extern uint64_t test_clone8(const void *, uint64_t, void **);

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

int storage_cases(void) {
    uint16_t left16[129], right16[129], buffer16[130];
    uint8_t left8[257], right8[257], buffer8[258];
    for (unsigned i = 0; i < 129; ++i) { left16[i] = (uint16_t)(0xd800 + i); right16[i] = (uint16_t)(0xdc00 + i); }
    for (unsigned i = 0; i < 257; ++i) { left8[i] = (uint8_t)i; right8[i] = (uint8_t)(255 - i); }
    const unsigned caps16[] = {0, 128, 129}, caps8[] = {0, 256, 257};
    for (unsigned c = 0; c < 3; ++c) {
        for (unsigned length = 0; length <= 129; ++length) {
#ifdef UTF_QUICK
            if (length > 17 && length != 31 && length != 32 && length != 33 && length != 63 && length != 64 && length != 65 &&
                length != 127 && length != 128 && length != 129) continue;
#endif
            unsigned a = length / 2, b = length - a;
            void *out = (void *)1;
            if (test_concat_into16(buffer16, caps16[c], left16, a, right16, b, &out) != length) return 1;
            if (!length) { if (out) return 2; continue; }
            if ((out == buffer16) != (length <= caps16[c])) return 3;
            for (unsigned i = 0; i < length; ++i)
                if (((uint16_t *)out)[i] != (i < a ? left16[i] : right16[i-a])) return 4;
            if (out != buffer16) test_drop(out);
        }
        for (unsigned length = 0; length <= 257; ++length) {
#ifdef UTF_QUICK
            if (length > 17 && length != 31 && length != 32 && length != 33 && length != 63 && length != 64 && length != 65 &&
                length != 127 && length != 128 && length != 129 && length != 255 && length != 256 && length != 257) continue;
#endif
            unsigned a = length / 2, b = length - a;
            void *out = (void *)1;
            if (test_concat_into8(buffer8, caps8[c], left8, a, right8, b, &out) != length) return 5;
            if (!length) { if (out) return 6; continue; }
            if ((out == buffer8) != (length <= caps8[c])) return 7;
            for (unsigned i = 0; i < length; ++i)
                if (((uint8_t *)out)[i] != (i < a ? left8[i] : right8[i-a])) return 8;
            if (out != buffer8) test_drop(out);
        }
    }
    for (unsigned a = 0; a < 3; ++a) for (unsigned b = 0; b < 3; ++b) {
        void *owned, *out;
        if (test_clone16(left16, a, &owned) != a) return 9;
        if (test_append16(owned, a, right16, b, &out) != a+b) return 10;
        for (unsigned i = 0; i < a+b; ++i)
            if (((uint16_t *)out)[i] != (i < a ? left16[i] : right16[i-a])) return 11;
        if (!a && !b && out) return 12;
        test_drop(out);
        if (test_clone8(left8, a, &owned) != a) return 13;
        if (test_append8(owned, a, right8, b, &out) != a+b) return 14;
        for (unsigned i = 0; i < a+b; ++i)
            if (((uint8_t *)out)[i] != (i < a ? left8[i] : right8[i-a])) return 15;
        if (!a && !b && out) return 16;
        test_drop(out);
    }
    return 0;
}

void storage_trap_case(unsigned index) {
    void *out;
    const uint64_t max_length = UINT64_C(9007199254740991);
    switch (index) {
        case 0: test_append16(0, max_length, 0, 1, &out); break;
        case 1: test_append16(0, UINT64_MAX, 0, 1, &out); break;
        case 2: test_append8(0, UINT64_MAX, 0, 1, &out); break;
        case 3: test_concat_into16(0, UINT64_MAX, 0, max_length, 0, 1, &out); break;
        case 4: test_concat_into16(0, UINT64_MAX, 0, UINT64_MAX, 0, 1, &out); break;
        case 5: test_concat_into8(0, UINT64_MAX, 0, UINT64_MAX, 0, 1, &out); break;
        default: __builtin_trap();
    }
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

void *tracked_realloc(void *value, uint64_t size) {
    if (!value) return tracked_alloc(size);
    if (!size) { tracked_free(value); return 0; }
    uint64_t *old = (uint64_t *)value - 2;
    assert(old[1] == UINT64_C(0x51a110ca7e) && live >= old[0]);
    uint64_t old_size = old[0];
    uint64_t *next = realloc(old, (size_t)size + 16);
    assert(next);
    next[0] = size;
    next[1] = UINT64_C(0x51a110ca7e);
    live = live - old_size + size;
    return next + 2;
}

#ifndef TZ_SIMD_TEST
int main(int argc, char **argv) {
    if (argc == 2) {
        if (argv[1][0] == 's') { storage_trap_case((unsigned)atoi(argv[1] + 1)); return 0; }
        runtime_trap_case((unsigned)atoi(argv[1]));
        return 0;
    }
    int batches = all_scalars();
    if (batches != 4344 || storage_cases() || live != 0 || runtime_trap_count() != 28) {
        fprintf(stderr, "Unicode batches: %d; live bytes: %llu\n", batches, (unsigned long long)live);
        return 1;
    }
    return 0;
}
#endif
#endif
