#undef NDEBUG
#include <stdint.h>
#ifndef __wasm__
#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#ifdef _WIN32
#include <windows.h>
#else
#include <sys/mman.h>
#include <unistd.h>
#endif
#endif

#define DECLARE(P) \
    extern int64_t P##_length8(const void *, uint64_t); \
    extern int64_t P##_length16(const void *, uint64_t); \
    extern int P##_valid8(const void *, uint64_t); \
    extern uint64_t P##_decode(const void *, uint64_t, void **); \
    extern uint64_t P##_encode(const void *, uint64_t, void **); \
    extern uint64_t P##_repair(const void *, uint64_t, void **); \
    extern int P##_well(const void *, uint64_t); \
    extern int P##_equal8(const void *, uint64_t, const void *, uint64_t); \
    extern int P##_equal16(const void *, uint64_t, const void *, uint64_t); \
    extern int P##_compare8(const void *, uint64_t, const void *, uint64_t); \
    extern int P##_compare16(const void *, uint64_t, const void *, uint64_t)
DECLARE(v);
DECLARE(r);
extern void test_drop(void *);
extern uint64_t test_calls(void);
extern uint64_t test_bytes(void);
extern uint64_t test_decodes16(void);
extern uint64_t test_scalar16_starts(void);
extern uint64_t test_utf_probes(void);
extern uint64_t test_utf_starts(void);
extern uint64_t p_decode(const void *, uint64_t, void **);
extern uint64_t p_encode(const void *, uint64_t, void **);
extern int p_valid8(const void *, uint64_t);
#ifndef __wasm__
extern int64_t tsuzuri_cpu_utf8_to_utf16(const uint8_t *, uint64_t, uint16_t *);
extern int64_t tsuzuri_cpu_utf16_to_utf8(const uint16_t *, uint64_t, uint8_t *);
extern uint64_t tsuzuri_cpu_features(void);
extern int tsuzuri_cpu_utf8_validate(const uint8_t *, uint64_t);
extern uint64_t test_cpu8_probes(void), test_cpu8_starts(void), test_cpu8_width(void);
extern uint64_t test_cpu16_probes(void), test_cpu16_starts(void), test_cpu16_width(void);
extern uint64_t c_encode(const void *, uint64_t, void **);
#endif

static uint8_t input8[600], other8[600];
static uint16_t input16[300], other16[300];
uint8_t *utf8_input(void) { return input8; }
uint16_t *utf16_input(void) { return input16; }
static uintptr_t result;
uintptr_t *result_slot(void) { return &result; }

static int equal_bytes(const void *a, const void *b, uint64_t size) {
    const uint8_t *x = a, *y = b;
    for (uint64_t i = 0; i < size; ++i) if (x[i] != y[i]) return 0;
    return 1;
}

static int check8(const uint8_t *p, uint64_t n, int convert) {
    int64_t expected = r_length8(p, n);
    if (v_length8(p, n) != expected) return 1;
    if (v_valid8(p, n) != (expected >= 0) || r_valid8(p, n) != (expected >= 0)) return 34;
#ifndef __wasm__
    if (tsuzuri_cpu_utf8_validate(p,n) != (expected >= 0)) return 40;
    if (tsuzuri_cpu_utf8_to_utf16(p, n, 0) != expected) return 2;
#endif
    if (expected < 0 || !convert) return 0;
    void *a, *b;
    uint64_t calls = test_calls(), bytes = test_bytes();
    if (v_decode(p, n, &a) != (uint64_t)expected) return 3;
    uint64_t vcalls = test_calls() - calls, vbytes = test_bytes() - bytes;
    calls = test_calls(); bytes = test_bytes();
    if (r_decode(p, n, &b) != (uint64_t)expected) return 4;
    if (test_calls()-calls != vcalls || test_bytes()-bytes != vbytes) return 30;
    int equal = equal_bytes(a, b, (uint64_t)expected * 2);
#ifndef __wasm__
    uint16_t output[600];
    if (tsuzuri_cpu_utf8_to_utf16(p, n, output) != expected ||
        !equal_bytes(output, b, (uint64_t)expected * 2)) equal = 0;
#endif
    test_drop(a); test_drop(b);
    return equal ? 0 : 5;
}

static int check16(const uint16_t *p, uint64_t n) {
    uint64_t validation_calls = test_calls(), validation_bytes = test_bytes();
    int64_t expected = r_length16(p, n);
    if (v_length16(p, n) != expected || v_well(p, n) != r_well(p, n)) return 6;
    if (test_calls() != validation_calls || test_bytes() != validation_bytes) return 43;
#ifndef __wasm__
    if (tsuzuri_cpu_utf16_to_utf8(p, n, 0) != expected) return 7;
#endif
    void *a, *b;
    uint64_t calls = test_calls(), bytes = test_bytes();
    uint64_t an = v_repair(p, n, &a);
    uint64_t vcalls = test_calls()-calls, vbytes = test_bytes()-bytes;
    calls = test_calls(); bytes = test_bytes();
    uint64_t bn = r_repair(p, n, &b);
    if (test_calls()-calls != vcalls || test_bytes()-bytes != vbytes) return 31;
    int equal = an == bn && equal_bytes(a, b, an * 2);
    test_drop(a); test_drop(b);
    if (!equal) return 8;
    if (expected < 0) return 0;
    calls = test_calls(); bytes = test_bytes();
    if (v_encode(p, n, &a) != (uint64_t)expected) return 9;
    vcalls = test_calls()-calls; vbytes = test_bytes()-bytes;
    calls = test_calls(); bytes = test_bytes();
    if (r_encode(p, n, &b) != (uint64_t)expected) return 9;
    if (test_calls()-calls != vcalls || test_bytes()-bytes != vbytes) return 32;
    equal = equal_bytes(a, b, (uint64_t)expected);
#ifndef __wasm__
    uint8_t output[1200];
    if (tsuzuri_cpu_utf16_to_utf8(p, n, output) != expected ||
        !equal_bytes(output, b, (uint64_t)expected)) equal = 0;
    calls = test_calls(); bytes = test_bytes();
    void *cpu_encoded;
    if (c_encode(p, n, &cpu_encoded) != (uint64_t)expected ||
        test_calls() - calls != vcalls || test_bytes() - bytes != vbytes ||
        !equal_bytes(cpu_encoded, b, (uint64_t)expected)) equal = 0;
    test_drop(cpu_encoded);
#endif
    test_drop(a); test_drop(b);
    return equal ? 0 : 10;
}

static int conversion_cases(void) {
    const uint16_t prefix[] = {0x41, 0xe9, 0x4e2d};
    const uint16_t mixed[] = {0x41, 0, 0x3a9, 0xd83d, 0xde00, 0x4e2d, 0x7a, 0xa};
    const uint16_t short_patterns[][8] = {
        {65,122,48,57,45,95,32,10}, {65,233,48,252,45,241,32,10},
        {0x4e2d,0x6587,0x5b57,0x65e5,0x672c,0x8a9e,0x6f22,0x5b57},
        {0xd83d,0xde00,0xd83d,0xde01,0xd83d,0xde02,0xd83d,0xde03},
        {65,0,0x3a9,0xd83d,0xde00,0x4e2d,122,10},
    };
    for (unsigned n = 8; n <= 64; n += 8) {
        for (unsigned i = 0; i < n; ++i) input16[i] = short_patterns[1][i % 8];
        uint64_t probes = test_utf_probes(), decodes = test_decodes16(), calls = test_calls();
        if (v_length16(input16, n) != r_length16(input16, n) || test_calls() != calls) return 87;
#ifdef UTF_VECTOR
        if (test_utf_probes() - probes != n / 8 || test_decodes16() != decodes) return 88;
#endif
        (void)probes; (void)decodes;
    }
    for (unsigned kind = 0; kind < 5; ++kind) {
        for (unsigned n = 7; n <= 17; ++n) {
            for (unsigned i = 0; i < n; ++i) input16[i] = other16[i] = short_patterns[kind][i % 8];
            other16[n-1] = 0xffff;
            if (v_compare16(input16, n, other16, n) != r_compare16(input16, n, other16, n) ||
                v_compare16(other16, n, input16, n) != r_compare16(other16, n, input16, n)) return 83;
        }
        for (unsigned i = 0; i < 8; ++i) input16[i] = short_patterns[kind][i];
        int code = check16(input16, 8);
        if (code) return code;
        void *encoded;
        uint64_t n = r_encode(input16, 8, &encoded);
        code = check8(encoded, n, 1);
        if (code) return code;
        if (n < 16) {
            void *decoded;
            uint64_t probes = test_utf_probes(), starts = test_utf_starts();
#ifndef __wasm__
            uint64_t cpu_probes = test_cpu8_probes(), cpu_starts = test_cpu8_starts();
#endif
            if (v_decode(encoded, n, &decoded) != 8 || !equal_bytes(decoded, input16, 16)) return 84;
            if (test_utf_probes() != probes || test_utf_starts() != starts) return 85;
#ifndef __wasm__
            if (test_cpu8_probes() != cpu_probes || test_cpu8_starts() != cpu_starts) return 86;
#endif
            test_drop(decoded);
        }
        test_drop(encoded);
    }
    for (unsigned kind = 0; kind < 3; ++kind) {
        for (unsigned i = 0; i < 64; ++i) {
            input16[i] = prefix[kind];
            input16[64+i] = mixed[i % 8];
            input16[128+i] = 65;
        }
        int64_t expected = r_length16(input16, 192);
        uint64_t probes = test_utf_probes(), starts = test_utf_starts(), calls = test_calls();
        if (v_length16(input16, 192) != expected || test_calls() != calls) return 56;
#ifdef UTF_VECTOR
        if (test_utf_probes() - probes != 9 || test_utf_starts() - starts != 1) return 57;
#endif
        probes = test_utf_probes(); starts = test_utf_starts();
        uint64_t allocated = test_bytes();
        void *encoded;
        if (p_encode(input16, 192, &encoded) != (uint64_t)expected) return 58;
#ifdef UTF_VECTOR
        if (test_utf_probes() - probes != 18 || test_utf_starts() - starts != 2) return 59;
#endif
        if (test_calls() - calls != 1) return 60;
        uint64_t encoded_bytes = test_bytes() - allocated;
        calls = test_calls(); allocated = test_bytes();
        void *reference;
        if (r_encode(input16, 192, &reference) != (uint64_t)expected ||
            !equal_bytes(reference, encoded, expected) || test_calls() - calls != 1 ||
            test_bytes() - allocated != encoded_bytes) return 80;
        test_drop(reference);
        for (unsigned i = 0; i < (unsigned)expected; ++i) input8[i] = ((uint8_t *)encoded)[i];
        test_drop(encoded);
        unsigned prefix_bytes = 64 * (kind + 1);
        unsigned bulk_bytes = kind == 2 ? 12 : 16;
        probes = test_utf_probes(); starts = test_utf_starts(); calls = test_calls();
        if (v_length8(input8, expected) != 192 || test_calls() != calls) return 61;
#ifdef UTF_VECTOR
        if (test_utf_probes() - probes != prefix_bytes / bulk_bytes + 1 ||
            test_utf_starts() - starts != 1) return 62;
#endif
        probes = test_utf_probes(); starts = test_utf_starts();
        calls = test_calls(); allocated = test_bytes();
        void *decoded;
        if (p_decode(input8, expected, &decoded) != 192 || !equal_bytes(decoded, input16, 384)) return 63;
        if (test_calls() - calls != 1) return 81;
        uint64_t decoded_bytes = test_bytes() - allocated;
        test_drop(decoded);
#ifdef UTF_VECTOR
        if (test_utf_probes() - probes != 2 * (prefix_bytes / bulk_bytes + 1) ||
            test_utf_starts() - starts != 2) return 64;
#endif
        calls = test_calls(); allocated = test_bytes();
        if (r_decode(input8, expected, &reference) != 192 || test_calls() - calls != 1 ||
            test_bytes() - allocated != decoded_bytes) return 82;
        test_drop(reference);
        probes = test_utf_probes(); starts = test_utf_starts(); calls = test_calls();
        if (!p_valid8(input8, expected) || test_calls() != calls) return 65;
#ifdef UTF_VECTOR
        if (test_utf_probes() - probes != prefix_bytes / bulk_bytes + 1 ||
            test_utf_starts() - starts != 1) return 66;
#endif
#ifndef __wasm__
        uint8_t bytes[1200];
        uint16_t units[600];
        for (unsigned write = 0; write < 2; ++write) {
            uint64_t cp = test_cpu16_probes(), cs = test_cpu16_starts();
            if (tsuzuri_cpu_utf16_to_utf8(input16, 192, write ? bytes : 0) != expected) return 67;
            if (test_cpu16_probes() - cp != 64 / test_cpu16_width() + 1 ||
                test_cpu16_starts() - cs != 1) return 68;
            if (write && !equal_bytes(bytes, input8, expected)) return 69;
            cp = test_cpu8_probes(); cs = test_cpu8_starts();
            if (tsuzuri_cpu_utf8_to_utf16(input8, expected, write ? units : 0) != 192) return 70;
            uint64_t consumed = kind == 2 ? 3 * test_cpu8_width() / 4 : test_cpu8_width();
            if (test_cpu8_starts() - cs > 1 ||
                ((write || test_cpu8_width() <= 32) &&
                 (test_cpu8_probes() - cp != prefix_bytes / consumed + 1 ||
                  test_cpu8_starts() - cs != 1))) return 71;
            if (write && !equal_bytes(units, input16, 384)) return 72;
        }
        uint64_t cp = test_cpu8_probes(), cs = test_cpu8_starts();
        if (!tsuzuri_cpu_utf8_validate(input8, expected)) return 73;
        uint64_t consumed = kind == 2 ? 3 * test_cpu8_width() / 4 : test_cpu8_width();
        if (test_cpu8_starts() - cs > 1 ||
            (test_cpu8_width() <= 32 &&
             (test_cpu8_probes() - cp != prefix_bytes / consumed + 1 ||
              test_cpu8_starts() - cs != 1))) return 74;
#endif
        input8[prefix_bytes] = 0xff;
        probes = test_utf_probes(); starts = test_utf_starts(); calls = test_calls();
        if (v_length8(input8, expected) != -1 || p_valid8(input8, expected) || test_calls() != calls) return 75;
#ifdef UTF_VECTOR
        if (test_utf_probes() - probes != 2 * (prefix_bytes / bulk_bytes + 1) ||
            test_utf_starts() - starts != 2) return 76;
#endif
        (void)probes; (void)starts; (void)bulk_bytes;
    }
    return 0;
}

static int validation_cases(void) {
    const uint16_t mixed[] = {0x41, 0, 0x3a9, 0xd83d, 0xde00, 0x4e2d, 0x7a, 0xa};
    for (unsigned scenario = 0; scenario < 6; ++scenario) {
        unsigned prefix = scenario / 2 * 8, malformed = scenario % 2;
        for (unsigned i = 0; i < prefix; ++i) input16[i] = 0x3a9;
        for (unsigned i = 0; i < 64; ++i) input16[prefix+i] = mixed[i % 8];
        input16[prefix+64] = 0xd800;
        unsigned n = prefix + 64 + malformed;
        uint64_t calls = test_calls(), bytes = test_bytes(), decodes = test_decodes16();
        uint64_t starts = test_scalar16_starts();
        if (v_well(input16, n) != !malformed || test_calls() != calls || test_bytes() != bytes) return 42;
#ifdef UTF_VECTOR
        if (test_decodes16() - decodes != 56 + malformed || test_scalar16_starts() - starts != 1) return 41;
#endif
        decodes = test_decodes16();
        starts = test_scalar16_starts();
        void *repaired;
        if (v_repair(input16, n, &repaired) != n) return 44;
        if (test_calls() - calls != 1 || test_bytes() - bytes != n * 2) return 45;
        int equal = equal_bytes(repaired, input16, (prefix+64)*2) &&
            (!malformed || ((uint16_t *)repaired)[prefix+64] == 0xfffd);
        test_drop(repaired);
        if (!equal) return 46;
#ifdef UTF_VECTOR
        if (test_decodes16() - decodes != 56 + malformed || test_scalar16_starts() - starts != 1) return 47;
#endif
        (void)decodes;
        (void)starts;
    }
    const uint16_t bmp[] = {0x41, 0xe9, 0x4e2d};
    for (unsigned kind = 0; kind < sizeof(bmp)/sizeof(bmp[0]); ++kind) {
        for (unsigned i = 0; i < 64; ++i) input16[i] = bmp[kind];
        uint64_t decodes = test_decodes16();
        if (check16(input16, 64)) return 54;
#ifdef UTF_VECTOR
        // Conversion helpers also bulk-process these uniform ASCII/BMP inputs.
        if (test_decodes16() != decodes) return 55;
#endif
        (void)decodes;
    }
    // Every BMP/high/low arrangement across an eight-unit block and its boundary.
    const uint16_t classes[] = {0x3a9, 0xd800, 0xdfff};
    for (unsigned code = 0; code < 19683; ++code) {
        unsigned remaining = code;
        for (unsigned i = 0; i < 9; ++i) {
            input16[i] = classes[remaining % 3];
            remaining /= 3;
        }
        if (check16(input16, 9)) return 48;
    }
    const uint64_t lengths[] = {UINT64_C(27021597764222974), UINT64_C(1) << 63, UINT64_MAX};
    input8[0] = 0xff;
    for (unsigned i = 0; i < sizeof(lengths) / sizeof(lengths[0]); ++i) {
        uint64_t calls = test_calls(), bytes = test_bytes();
        uint64_t probes = test_utf_probes(), starts = test_utf_starts();
        if (p_valid8(input8, lengths[i])) return 77;
#ifdef UTF_VECTOR
        if (test_utf_probes() - probes != 1 || test_utf_starts() - starts != 1) return 78;
#endif
#ifndef __wasm__
        uint64_t cp = test_cpu8_probes(), cs = test_cpu8_starts();
        if (v_valid8(input8, lengths[i]) ||
            test_cpu8_probes() - cp != 1 || test_cpu8_starts() - cs != 1) return 79;
#endif
        if (v_valid8(input8, lengths[i]) || r_valid8(input8, lengths[i])) return 49;
        if (v_well(0, lengths[i]) || r_well(0, lengths[i])) return 50;
        if (test_calls() != calls || test_bytes() != bytes) return 51;
        (void)probes; (void)starts;
    }
    return 0;
}

int differential(void) {
    int conversions = conversion_cases();
    if (conversions) return conversions;
    int validation = validation_cases();
    if (validation) return validation;
    const uint16_t samples[] = {0, 0x7f, 0x80, 0x7ff, 0x800, 0x7fff, 0x8000, 0xd7ff, 0xd800, 0xdc00, 0xdfff, 0xe000, 0xffff};
    const uint8_t patterns[][4] = {{0xc2,0x80}, {0xdf,0xbf}, {0xe0,0xa0,0x80}, {0xed,0x9f,0xbf}, {0xef,0xbf,0xbf}, {0xf0,0x90,0x80,0x80}, {0xf4,0x8f,0xbf,0xbf}};
    const unsigned widths[] = {2,2,3,3,3,4,4};
    const unsigned offsets =
#ifdef UTF_QUICK
        4;
#else
        16;
#endif
    const unsigned max_length =
#ifdef UTF_QUICK
        65;
#else
        257;
#endif
    for (unsigned offset = 0; offset < offsets; ++offset) {
        uint8_t *p = input8 + offset, *q = other8 + offset;
        uint16_t *u = input16 + offset / 2, *w = other16 + offset / 2;
        for (unsigned n = 0; n <= max_length; ++n) {
            for (unsigned i = 0; i < n; ++i) { p[i] = q[i] = 65; u[i] = w[i] = 65; }
            if (check8(p, n, 1) || check16(u, n)) return 11;
            if (!v_equal8(p,n,q,n) || !v_equal16(u,n,w,n) ||
                v_compare8(p,n,q,n) || v_compare16(u,n,w,n)) return 12;
            if (n && (v_compare8(p,n,p,n-1) != 1 || v_compare16(u,n,u,n-1) != 1)) return 13;
            for (unsigned at = 0; at < n; ++at) {
                q[at] = 255; w[at] = 0x8000;
                if (v_equal8(p,n,q,n) || v_equal16(u,n,w,n) ||
                    v_compare8(p,n,q,n) != -1 || v_compare16(u,n,w,n) != -1 ||
                    v_compare8(q,n,p,n) != 1 || v_compare16(w,n,u,n) != 1) return 14;
                q[at] = 65; w[at] = 65;
                const uint16_t pairs16[][2] = {{0x41,0x42},{0x7fff,0x8000},{0xffff,0},{0xd800,0xe000}};
                const uint8_t pairs8[][2] = {{0x7f,0x80},{0xff,0}};
                for (unsigned k = 0; k < 4; ++k) {
                    u[at] = pairs16[k][0]; w[at] = pairs16[k][1];
                    int expected = u[at] < w[at] ? -1 : 1;
                    if (v_equal16(u,n,w,n) || v_compare16(u,n,w,n) != expected ||
                        v_compare16(w,n,u,n) != -expected) return 37;
                }
                u[at] = w[at] = 65;
                for (unsigned k = 0; k < 2; ++k) {
                    p[at] = pairs8[k][0]; q[at] = pairs8[k][1];
                    int expected = p[at] < q[at] ? -1 : 1;
                    if (v_equal8(p,n,q,n) || v_compare8(p,n,q,n) != expected ||
                        v_compare8(q,n,p,n) != -expected) return 38;
                }
                p[at] = 0xc3; q[at] = 65;
                if (at+1 < n) p[at+1] = 0xa9;
                if (check8(p,n,1)) return 39;
                p[at] = 65;
                if (at+1 < n) p[at+1] = 65;
                for (unsigned k = 0; k < sizeof(samples)/sizeof(samples[0]); ++k) {
                    u[at] = samples[k];
                    if (check16(u,n)) return 15;
                }
                u[at] = 65;
                if (at+1 < n) {
                    u[at] = 0xd83d; u[at+1] = 0xde00;
                    if (check16(u,n)) return 16;
                    u[at] = u[at+1] = 65;
                }
            }
            for (unsigned k = 0; k < 7; ++k) {
                for (unsigned i = 0; i < n; ++i) p[i] = patterns[k][i % widths[k]];
                if (check8(p,n,1)) return 17;
            }
        }
    }
    for (unsigned surrogate = 0xd800; surrogate <= 0xdfff; ++surrogate) {
#ifdef UTF_QUICK
        if (surrogate != 0xd800 && surrogate != 0xdbff && surrogate != 0xdc00 && surrogate != 0xdfff) continue;
#endif
        for (unsigned at = 0; at < 32; ++at) {
            for (unsigned i = 0; i < 40; ++i) input16[i] = 65;
            input16[at] = (uint16_t)surrogate;
            if (check16(input16,40)) return 18;
        }
    }
    const uint8_t tails[] = {0,0x7f,0x80,0x8f,0x90,0x9f,0xa0,0xbf,0xc0,0xff};
    for (unsigned n = 1; n <= 4; ++n) for (unsigned lead = 0; lead < 256; ++lead)
        for (unsigned a = 0; a < (n > 1 ? 10u : 1u); ++a)
        for (unsigned b = 0; b < (n > 2 ? 10u : 1u); ++b)
        for (unsigned c = 0; c < (n > 3 ? 10u : 1u); ++c) {
#ifdef UTF_QUICK
                if (n == 4 && (lead != 0xf0 && lead != 0xf4 && lead != 0xf5 && lead != 0xff)) continue;
                if (n == 4 && (a > 3 || b > 3 || c > 3)) continue;
#endif
                uint8_t sequence[4] = {(uint8_t)lead,tails[a],tails[b],tails[c]};
                if (check8(sequence,n,1)) return 19;
                for (unsigned at = 0; at <= 16; ++at) {
#ifdef UTF_QUICK
                    if (at != 0 && at != 13 && at != 15 && at != 16) continue;
#endif
                    for (unsigned i = 0; i < 48; ++i) input8[i] = 65;
                    for (unsigned i = 0; i < n; ++i) input8[at+i] = sequence[i];
                    if (check8(input8,48,1)) return 20;
                }
            }
    return 0;
}

int exhaustive(void) {
    for (unsigned n = 1; n <= 3; ++n) {
        uint32_t end = UINT32_C(1) << (8*n);
        for (uint32_t bits = 0; bits < end; ++bits) {
            uint8_t p[3] = {(uint8_t)bits,(uint8_t)(bits>>8),(uint8_t)(bits>>16)};
            if (check8(p,n,1)) return 21;
            for (unsigned at = 0; at <= 13; at += 13) {
                for (unsigned i = 0; i < 32; ++i) input8[i] = 65;
                for (unsigned i = 0; i < n; ++i) input8[at+i] = p[i];
                if (check8(input8,32,1)) return 22;
            }
        }
    }
    return 0;
}

int guard_pages(void) {
    int validation = validation_cases();
    if (validation) return validation;
#ifndef __wasm__
#ifdef _WIN32
    SYSTEM_INFO info;
    GetSystemInfo(&info);
    size_t page = info.dwPageSize;
    uint8_t *memory = VirtualAlloc(0,page*2,MEM_RESERVE|MEM_COMMIT,PAGE_READWRITE);
    DWORD old;
    assert(memory && VirtualProtect(memory+page,page,PAGE_NOACCESS,&old));
#else
    size_t page = (size_t)sysconf(_SC_PAGESIZE);
    uint8_t *memory = mmap(0, page*2, PROT_READ|PROT_WRITE, MAP_PRIVATE|MAP_ANONYMOUS, -1, 0);
    assert(memory != MAP_FAILED && mprotect(memory+page,page,PROT_NONE) == 0);
#endif
    for (unsigned n = 0; n <= 257; ++n) {
#ifdef UTF_QUICK
        if (n > 17 && n != 31 && n != 32 && n != 33 && n != 63 && n != 64 && n != 65 &&
            n != 127 && n != 128 && n != 129 && n < 255) continue;
#endif
        uint8_t *p = memory + page - n;
        for (unsigned i = 0; i < n; ++i) p[i] = 65;
        if (check8(p,n,1) || !v_equal8(p,n,p,n) || v_compare8(p,n,p,n)) return 23;
        if (n) { p[n-1] = 0xf0; if (check8(p,n,0)) return 24; }
        const uint8_t patterns[][4] = {{0xc3,0xa9}, {0xe4,0xb8,0xad}, {0xf0,0x9f,0x98,0x80}};
        const unsigned widths[] = {2,3,4};
        for (unsigned k = 0; k < 3; ++k) {
            for (unsigned i = 0; i < n; ++i) p[i] = patterns[k][i % widths[k]];
            if (check8(p,n,1)) return 35;
        }
        uint16_t *u = (uint16_t *)(memory+page-2*n);
        for (unsigned i = 0; i < n; ++i) u[i] = 65;
        if (check16(u,n) || !v_equal16(u,n,u,n) || v_compare16(u,n,u,n)) return 25;
        if (n) { u[n-1] = 0xd800; if (check16(u,n)) return 26; }
        for (unsigned k = 0; k < 4; ++k) {
            for (unsigned i = 0; i < n; ++i)
                u[i] = k == 0 ? 0xe9 : k == 1 ? 0x4e2d : k == 2 ? (i % 2 ? 0xde00 : 0xd83d) :
                    i % 8 == 7 ? 0xd83d : i && i % 8 == 0 ? 0xde00 : 0x3a9;
            if (check16(u,n)) return 36;
        }
    }
#ifdef _WIN32
    assert(VirtualFree(memory,0,MEM_RELEASE));
#else
    assert(munmap(memory,page*2) == 0);
#endif
#else
    unsigned previous = __builtin_wasm_memory_grow(0,1);
    if (previous == (unsigned)-1) return 27;
    uintptr_t end = ((uintptr_t)previous+1)*65536;
    for (unsigned n = 0; n <= 257; ++n) {
        uint8_t *p = (uint8_t *)(end-n);
        for (unsigned i = 0; i < n; ++i) p[i] = 65;
        if (v_length8(p,n) != n || !v_equal8(p,n,p,n) || v_compare8(p,n,p,n)) return 28;
        uint16_t *u = (uint16_t *)(end-2*n);
        for (unsigned i = 0; i < n; ++i) u[i] = 65;
        if (v_length16(u,n) != n || !v_well(u,n) || !v_equal16(u,n,u,n) || v_compare16(u,n,u,n)) return 29;
        for (unsigned i = 0; i < n; ++i)
            u[i] = i % 8 == 7 ? 0xd83d : i && i % 8 == 0 ? 0xde00 : 0x3a9;
        if (v_well(u,n) != r_well(u,n)) return 52;
        if (n) {
            u[n-1] = 0xd800;
            if (v_well(u,n) || r_well(u,n)) return 53;
        }
    }
#endif
    return 0;
}

#define TZ_SIMD_TEST
#include "strings_runtime.c"

#if defined(_WIN32) && !defined(__wasm__)
static unsigned trap_case_index;

static LONG WINAPI report_unhandled_trap(EXCEPTION_POINTERS *exception) {
    DWORD code = exception->ExceptionRecord->ExceptionCode;
    if (code != EXCEPTION_ILLEGAL_INSTRUCTION)
        fprintf(stderr, "trap case %u raised exception 0x%08lx at %p (image base %p)\n", trap_case_index,
            (unsigned long)code, exception->ExceptionRecord->ExceptionAddress, GetModuleHandleW(NULL));
    return EXCEPTION_CONTINUE_SEARCH;
}
#endif

#ifndef __wasm__
int main(int argc, char **argv) {
    if (argc > 1 && !strcmp(argv[1],"--features")) {
        printf("%llu\n",(unsigned long long)tsuzuri_cpu_features());
        return 0;
    }
    if (argc > 1 && argv[1][0] == 's') { storage_trap_case((unsigned)atoi(argv[1]+1)); return 0; }
    if (argc > 1 && argv[1][0] == 't') {
#ifdef _WIN32
        trap_case_index = (unsigned)atoi(argv[1]+1);
        fprintf(stderr, "trap case %u started\n", trap_case_index);
        SetUnhandledExceptionFilter(report_unhandled_trap);
#endif
        runtime_trap_case((unsigned)atoi(argv[1]+1)); return 0;
    }
    int guard_only = argc > 1 && !strcmp(argv[1],"--guard-only");
    int result = guard_only ? 0 : argc > 1 && !strcmp(argv[1],"--exhaustive") ? exhaustive() : differential();
    if (!result) result = guard_pages();
    if (!result) result = storage_cases();
    if (!guard_only && !result && all_scalars() != 4344) result = 33;
    if (result || live) { fprintf(stderr,"UTF differential failure %d; decodes16=%llu; live=%llu\n",result,
        (unsigned long long)test_decodes16(),(unsigned long long)live); return 1; }
    return 0;
}
#endif
