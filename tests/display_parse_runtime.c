typedef unsigned long long u64;
extern int tz_soft_format(char *, const unsigned char *, int);
extern int tz_soft_parse(unsigned char *, const char *, u64, int);

static char input[4097], output[128];
static unsigned char value_storage[18] = { [0] = 0xa5, [17] = 0x5a };
static unsigned char *const value = value_storage + 1;

static void check_value_guards(void) {
    if (value_storage[0] != 0xa5 || value_storage[17] != 0x5a) __builtin_trap();
}

char *input_pointer(void) { return input; }
char *output_pointer(void) { return output; }
u64 result_low(void) {
    u64 result = 0;
    for (int i = 7; i >= 0; --i) result = (result << 8) | value[i];
    return result;
}
u64 result_high(void) {
    u64 result = 0;
    for (int i = 15; i >= 8; --i) result = (result << 8) | value[i];
    return result;
}
int format_value(int kind, u64 low, u64 high) {
    for (int i = 0; i < 8; ++i) {
        value[i] = (unsigned char)(low >> (8 * i));
        value[i + 8] = (unsigned char)(high >> (8 * i));
    }
    int length = tz_soft_format(output, value, kind);
    check_value_guards();
    return length;
}
int parse_value(int kind, int length) {
    for (int i = 0; i < 16; ++i) value[i] = 0;
    int ok = tz_soft_parse(value, input, (u64)length, kind);
    check_value_guards();
    return ok;
}

#ifndef __wasm__
#include <stdio.h>
#include <string.h>
#include <time.h>

static int print_timing(clock_t start, int repeats, u64 checksum) {
    clock_t end = clock();
    if (start == (clock_t)-1 || end == (clock_t)-1 || end < start) return 1;
    printf("%.9f %llu\n", (double)(end - start) * 1000.0 / CLOCKS_PER_SEC / repeats, checksum);
    return 0;
}

int main(void) {
    char operation, hex[8195];
    int kind;
    while (scanf(" %c %d", &operation, &kind) == 2) {
        int benchmark = operation == 'F' || operation == 'P';
        int repeats = 1;
        if (benchmark && (scanf("%d", &repeats) != 1 || repeats < 1 || repeats > 10000000)) return 1;
        volatile int input_kind = kind;
        if (operation == 'f' || operation == 'F') {
            u64 low, high;
            if (scanf("%llx %llx", &low, &high) != 2) return 1;
            volatile u64 input_low = low, input_high = high;
            int length = 0;
            u64 checksum = 0;
            clock_t start = benchmark ? clock() : 0;
            for (int repeat = 0; repeat < repeats; ++repeat) {
                length = format_value(input_kind, input_low, input_high);
                if (length < 1 || length > 128) return 1;
                checksum += (u64)length + (unsigned char)output[0] + (unsigned char)output[length - 1];
            }
            if (benchmark) {
                if (print_timing(start, repeats, checksum)) return 1;
                continue;
            }
            printf("%.*s\n", length, output);
        } else {
            if (scanf("%8194s", hex) != 1) return 1;
            int length = (int)strlen(hex) / 2;
            for (int i = 0; i < length; ++i) {
                unsigned int byte;
                if (sscanf(hex + i * 2, "%2x", &byte) != 1) return 1;
                input[i] = (char)byte;
            }
            int ok = 0;
            u64 checksum = 0;
            volatile int input_length = length;
            clock_t start = benchmark ? clock() : 0;
            for (int repeat = 0; repeat < repeats; ++repeat) {
                ok = parse_value(input_kind, input_length);
                checksum += (u64)ok + result_low() + result_high();
            }
            if (benchmark) {
                if (print_timing(start, repeats, checksum)) return 1;
                continue;
            }
            printf("%d %016llx%016llx\n", ok, result_high(), result_low());
        }
    }
    return ferror(stdin) || ferror(stdout);
}
#endif
