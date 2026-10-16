// Test host for src/runtime/integer.ll (PM08). The test makes the runtime's `internal` definitions
// external and adds the C-callable wrappers below, so the same file builds as a native program that
// reads "f KIND LOW HIGH", "p KIND FLAGS LOW HIGH", "r KIND LENGTH UNITS", and "s KIND" lines and as a
// WebAssembly module that exports format_value, spec_value, and parse_value.
//
// KIND is the numeric kind of src/llvm.rs: 16 + log2(bytes) for a signed integer, 24 + log2(bytes) for an
// unsigned one. LOW and HIGH are the halves of the two's complement bit pattern. FLAGS are those of a
// format hole. UNITS are the UTF-16 code units of the text to parse as groups of four hexadecimal digits.
// Built with -DCROSS_CHECK and linked with src/runtime/numeric.ll, the program also runs the numeric
// runtime's own tz_soft_format, tz_soft_format_spec, and tz_soft_parse (behind the ASCII conversion that
// Parse.parse applies) on every input and stops when a result differs.
typedef unsigned long long u64;

// Wrappers around the runtime's functions; those that take an i128 take it as two halves, because
// the C ABI of a 128-bit integer differs between targets. The functions that write text take the
// address after the buffer and write the text before it.
extern int tz_integer_format(char *, long long, int);
extern int tz_integer_format128(char *, u64, u64, int);
extern int tz_integer_spec(char *, long long, int, int);
extern int tz_integer_spec128(char *, u64, u64, int, int);
extern int tz_integer_parse(const unsigned short *, long long, int, int, u64 *);
extern int tz_integer_parse128(const unsigned short *, long long, int, u64 *);

#define GUARD 0xa5
// Guard bytes, the bytes a caller provides (40 for a decimal text, 160 for a spec), then guard bytes:
// the runtime writes the text and nothing else.
static unsigned char storage[8 + 160 + 8];
static unsigned char *const output = storage + 8;
static int text_capacity, text_length;

// The first byte of the text of the last format_value or spec_value.
char *text_pointer(void) { return (char *)output + text_capacity - text_length; }

static char *begin(int capacity) {
    for (int i = 0; i < 8 + capacity + 8; ++i) storage[i] = GUARD;
    text_capacity = capacity;
    return (char *)output + capacity;
}

static int finish(int capacity, int length) {
    if (length < 1 || length > capacity) __builtin_trap();
    for (int i = 0; i < 8 + capacity - length; ++i) if (storage[i] != GUARD) __builtin_trap();
    for (int i = 8 + capacity; i < 16 + capacity; ++i) if (storage[i] != GUARD) __builtin_trap();
    text_length = length;
    return length;
}

// The value of an integer of at most 64 bits as the runtime takes it: extended to 64 bits.
static long long extend(int bits, int is_signed, u64 low) {
    switch (bits) {
    case 8: return is_signed ? (long long)(signed char)low : (long long)(unsigned char)low;
    case 16: return is_signed ? (long long)(short)low : (long long)(unsigned short)low;
    case 32: return is_signed ? (long long)(int)low : (long long)(unsigned int)low;
    default: return (long long)low;
    }
}

int format_value(int kind, u64 low, u64 high) {
    int bits = 8 << (kind & 7);
    int is_signed = kind < 24;
    char *end = begin(40);
    int length = bits == 128 ? tz_integer_format128(end, low, high, is_signed)
                             : tz_integer_format(end, extend(bits, is_signed, low), is_signed);
    return finish(40, length);
}

int spec_value(int kind, int flags, u64 low, u64 high) {
    int bits = 8 << (kind & 7);
    int is_signed = kind < 24;
    char *end = begin(160);
    int length = bits == 128 ? tz_integer_spec128(end, low, high, is_signed, flags)
                             : tz_integer_spec(end, extend(bits, is_signed, low), is_signed, flags);
    return finish(160, length);
}

#define POISON 0x5a5a5a5a5a5a5a5aull
// The text to parse (up to 4100 units, which exceeds the 4096 that Parse accepts), then four digits that no
// parse may read.
static unsigned short input[4100 + 4];
static u64 result[2];

unsigned short *input_pointer(void) { return input; }
u64 result_low(void) { return result[0]; }
u64 result_high(void) { return result[1]; }

// Parses `length` units of input_pointer(); on success result_low and result_high are the bit pattern.
int parse_value(int kind, int length) {
    int bits = 8 << (kind & 7);
    int is_signed = kind < 24;
    for (int i = 0; i < 4; ++i) input[length + i] = '1';
    result[0] = result[1] = POISON;
    int ok;
    if (bits == 128) {
        ok = tz_integer_parse128(input, length, is_signed, result);
    } else {
        ok = tz_integer_parse(input, length, bits, is_signed, result);
        if (result[1] != POISON) __builtin_trap();
        // The runtime leaves the sign or zeros above the type's own bits.
        if (ok) result[0] &= bits == 64 ? ~0ull : (1ull << bits) - 1;
        result[1] = 0;
    }
    return ok;
}

#ifndef __wasm__
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#ifdef CROSS_CHECK
extern int tz_soft_format(char *, const unsigned char *, int);
extern int tz_soft_format_spec(char *, const unsigned char *, int, int, int);
extern int tz_soft_parse(unsigned char *, const char *, u64, int);

static void raw_bytes(unsigned char *raw, u64 low, u64 high) {
    for (int i = 0; i < 8; ++i) {
        raw[i] = (unsigned char)(low >> (8 * i));
        raw[i + 8] = (unsigned char)(high >> (8 * i));
    }
}

static void same_text(const char *what, int kind, u64 low, u64 high, int flags, int length) {
    unsigned char raw[16];
    char old[512];
    raw_bytes(raw, low, high);
    int old_length = flags < 0 ? tz_soft_format(old, raw, kind) : tz_soft_format_spec(old, raw, kind, flags, 0);
    if (old_length != length || memcmp(old, text_pointer(), (size_t)length) != 0) {
        printf("%s differs from numeric.ll: kind %d flags %d low %llx high %llx: %.*s, numeric.ll %.*s\n", what, kind,
               flags, low, high, length, text_pointer(), old_length, old);
        exit(1);
    }
}

// What Parse.parse does with a string: text beyond ASCII or 4096 units is None, else tz_soft_parse decides.
static void same_parse(int kind, int length, int ok) {
    static char ascii[4097];
    unsigned char raw[16] = {0};
    int old_ok = length <= 4096;
    for (int i = 0; old_ok && i < length; ++i) {
        old_ok = input[i] <= 127;
        ascii[i] = (char)input[i];
    }
    if (old_ok) old_ok = tz_soft_parse(raw, ascii, (u64)length, kind) != 0;
    u64 low = 0, high = 0;
    for (int i = 7; i >= 0; --i) low = (low << 8) | raw[i];
    for (int i = 15; i >= 8; --i) high = (high << 8) | raw[i];
    if ((old_ok != 0) != (ok != 0) || (ok && (low != result[0] || high != result[1]))) {
        printf("parse differs from numeric.ll: kind %d length %d: %d %llx %llx, numeric.ll %d %llx %llx\n", kind, length,
               ok, result[0], result[1], old_ok, low, high);
        exit(1);
    }
}
#endif

// The text a format hole with `flags` shows for `value`, built with the C library and no code of the runtime;
// `prefix` goes after the sign (to build the text of a parse).
static int expected_spec(char *out, long long value, int flags, const char *prefix) {
    int style = flags >> 4 & 15, count = 0;
    unsigned long long magnitude = value < 0 ? 0ull - (unsigned long long)value : (unsigned long long)value;
    if (value < 0) out[count++] = '-';
    else if (flags & 1) out[count++] = '+';
    count += snprintf(out + count, 8, "%s", prefix);
    if (style == 3) return count + snprintf(out + count, 24, "%llx", magnitude);
    if (style == 4) return count + snprintf(out + count, 24, "%llX", magnitude);
    if (style == 5) return count + snprintf(out + count, 24, "%llo", magnitude);
    if (style == 6) {
        int bit = 63;
        while (bit > 0 && !(magnitude >> bit & 1)) --bit;
        for (; bit >= 0; --bit) out[count++] = (char)('0' + (magnitude >> bit & 1));
        out[count] = 0;
        return count;
    }
    return count + snprintf(out + count, 24, "%llu", magnitude);
}

// Every value of an 8- or 16-bit type against the C library: its decimal text, the text of every
// form of a format hole, and the parse of the text of every base and sign.
static int sweep(int kind) {
    int bits = 8 << (kind & 7);
    _Bool is_signed = kind < 24;
    char expected[128];
    for (unsigned long long raw = 0; raw < (1ull << bits); ++raw) {
        long long value = is_signed ? (bits == 8 ? (long long)(signed char)raw : (long long)(short)raw) : (long long)raw;
        int length = format_value(kind, raw, 0);
        snprintf(expected, sizeof expected, "%lld", value);
        if ((int)strlen(expected) != length || memcmp(expected, text_pointer(), (size_t)length) != 0) {
            printf("mismatch kind %d raw %llu: %.*s, expected %s\n", kind, raw, length, text_pointer(), expected);
            return 1;
        }
        static const int forms[] = {0, 3, 4, 5, 6};
        static const char *const prefixes[] = {"", "0x", "0x", "", "0b"};
        for (int form = 0; form < 5; ++form) {
            for (int plus = 0; plus < 2; ++plus) {
                int flags = plus | forms[form] << 4;
                int count = expected_spec(expected, value, flags, "");
                length = spec_value(kind, flags, raw, 0);
                if (count != length || memcmp(expected, text_pointer(), (size_t)length) != 0) {
                    printf("spec mismatch kind %d flags %d raw %llu: %.*s, expected %s\n", kind, flags, raw, length,
                           text_pointer(), expected);
                    return 1;
                }
                if (form == 3) continue;
                count = expected_spec(expected, value, flags, prefixes[form]);
                for (int i = 0; i < count; ++i) input[i] = (unsigned char)expected[i];
                if (!parse_value(kind, count) || result[0] != raw) {
                    printf("parse mismatch kind %d raw %llu: %.*s\n", kind, raw, count, expected);
                    return 1;
                }
            }
        }
    }
    return 0;
}

static int hex_value(char c) { return c <= '9' ? c - '0' : c - 'a' + 10; }

int main(void) {
    char operation;
    int kind;
    static char units[4 * 4100 + 1];
    while (scanf(" %c %d", &operation, &kind) == 2) {
        if (operation == 'f') {
            u64 low, high;
            if (scanf("%llx %llx", &low, &high) != 2) return 1;
            int length = format_value(kind, low, high);
#ifdef CROSS_CHECK
            same_text("format", kind, low, high, -1, length);
#endif
            printf("%.*s\n", length, text_pointer());
        } else if (operation == 'p') {
            int flags;
            u64 low, high;
            if (scanf("%d %llx %llx", &flags, &low, &high) != 3) return 1;
            int length = spec_value(kind, flags, low, high);
#ifdef CROSS_CHECK
            same_text("spec", kind, low, high, flags, length);
#endif
            printf("%.*s\n", length, text_pointer());
        } else if (operation == 'r') {
            int length;
            if (scanf("%d", &length) != 1 || length < 0 || length > 4100) return 1;
            if (length) {
                if (scanf("%16400s", units) != 1 || (int)strlen(units) != 4 * length) return 1;
                for (int i = 0; i < length; ++i) {
                    input[i] = (unsigned short)(hex_value(units[4 * i]) << 12 | hex_value(units[4 * i + 1]) << 8 |
                                                hex_value(units[4 * i + 2]) << 4 | hex_value(units[4 * i + 3]));
                }
            }
            int ok = parse_value(kind, length);
#ifdef CROSS_CHECK
            same_parse(kind, length, ok);
#endif
            if (!ok) printf("none\n");
            else if (result[1]) printf("%llx%016llx\n", result[1], result[0]);
            else printf("%llx\n", result[0]);
        } else if (operation == 's') {
            if (kind < 16 || kind > 25 || (kind & 7) > 1) return 1;
            if (sweep(kind)) return 1;
            printf("swept %d\n", kind);
        } else {
            return 1;
        }
    }
    return ferror(stdin) || ferror(stdout);
}
#endif
