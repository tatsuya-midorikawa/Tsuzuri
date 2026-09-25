typedef unsigned long long u64;
extern int tz_soft_format(char *, const unsigned char *, int);
extern int tz_soft_parse(unsigned char *, const char *, u64, int);

static char input[4097], output[128];
static unsigned char value[16];

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
    return tz_soft_format(output, value, kind);
}
int parse_value(int kind, int length) {
    for (int i = 0; i < 16; ++i) value[i] = 0;
    return tz_soft_parse(value, input, (u64)length, kind);
}

#ifndef __wasm__
#include <stdio.h>
#include <string.h>

int main(void) {
    char operation, hex[8195];
    int kind;
    while (scanf(" %c %d", &operation, &kind) == 2) {
        if (operation == 'f') {
            u64 low, high;
            if (scanf("%llx %llx", &low, &high) != 2) return 1;
            int length = format_value(kind, low, high);
            if (length < 0 || length > 128) return 1;
            printf("%.*s\n", length, output);
        } else {
            if (scanf("%8194s", hex) != 1) return 1;
            int length = (int)strlen(hex) / 2;
            for (int i = 0; i < length; ++i) {
                unsigned int byte;
                if (sscanf(hex + i * 2, "%2x", &byte) != 1) return 1;
                input[i] = (char)byte;
            }
            int ok = parse_value(kind, length);
            printf("%d %016llx%016llx\n", ok, result_high(), result_low());
        }
    }
    return ferror(stdin) || ferror(stdout);
}
#endif
