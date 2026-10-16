// Test host for src/runtime/integer.ll (PM08). The test makes the formatter's `internal` definitions
// external and adds the two C-callable wrappers below, so the same file builds as a native program that
// reads "f KIND LOW HIGH" and "s KIND" lines and as a WebAssembly module that exports format_value.
//
// KIND is the numeric kind of src/llvm.rs: 16 + log2(bytes) for a signed integer, 24 + log2(bytes) for an
// unsigned one. LOW and HIGH are the halves of the two's complement bit pattern.
typedef unsigned long long u64;

// Wrappers around tz.integer.format and tz.integer.format128 that the test appends to the IR; the second
// takes the i128 as two halves, because the C ABI of a 128-bit integer differs between targets.
// Like the formatter, they take the address after the buffer and write the text before it.
extern int tz_integer_format(char *, long long, int);
extern int tz_integer_format128(char *, u64, u64, int);

#define GUARD 0xa5
// 8 guard bytes, the 40 bytes a caller provides, then guard bytes: the formatter writes the text and nothing else.
static unsigned char storage[72];
static unsigned char *const output = storage + 8;
static int text_length;

// The first byte of the text of the last format_value.
char *text_pointer(void) { return (char *)output + 40 - text_length; }

int format_value(int kind, u64 low, u64 high) {
    for (int i = 0; i < 72; ++i) storage[i] = GUARD;
    int bits = 8 << (kind & 7);
    int is_signed = kind < 24;
    char *end = (char *)output + 40;
    int length;
    if (bits == 128) {
        length = tz_integer_format128(end, low, high, is_signed);
    } else {
        long long value;
        switch (bits) {
        case 8: value = is_signed ? (long long)(signed char)low : (long long)(unsigned char)low; break;
        case 16: value = is_signed ? (long long)(short)low : (long long)(unsigned short)low; break;
        case 32: value = is_signed ? (long long)(int)low : (long long)(unsigned int)low; break;
        default: value = (long long)low; break;
        }
        length = tz_integer_format(end, value, is_signed);
    }
    if (length < 1 || length > 40) __builtin_trap();
    for (int i = 0; i < 48 - length; ++i) if (storage[i] != GUARD) __builtin_trap();
    for (int i = 48; i < 72; ++i) if (storage[i] != GUARD) __builtin_trap();
    text_length = length;
    return length;
}

#ifndef __wasm__
#include <stdio.h>
#include <string.h>

// Every value of an 8- or 16-bit type against the C library's own formatting.
static int sweep(int kind) {
    int bits = 8 << (kind & 7);
    _Bool is_signed = kind < 24;
    char expected[16];
    for (unsigned long long raw = 0; raw < (1ull << bits); ++raw) {
        long long value = is_signed ? (bits == 8 ? (long long)(signed char)raw : (long long)(short)raw) : (long long)raw;
        int length = format_value(kind, raw, 0);
        snprintf(expected, sizeof expected, "%lld", value);
        if ((int)strlen(expected) != length || memcmp(expected, text_pointer(), (size_t)length) != 0) {
            printf("mismatch kind %d raw %llu: %.*s, expected %s\n", kind, raw, length, text_pointer(), expected);
            return 1;
        }
    }
    return 0;
}

int main(void) {
    char operation;
    int kind;
    while (scanf(" %c %d", &operation, &kind) == 2) {
        if (operation == 'f') {
            u64 low, high;
            if (scanf("%llx %llx", &low, &high) != 2) return 1;
            int length = format_value(kind, low, high);
            printf("%.*s\n", length, text_pointer());
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
