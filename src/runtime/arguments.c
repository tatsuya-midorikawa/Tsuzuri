// The arguments of a `def main :: Array<string> -> i32`: the command-line arguments after the
// program name, each as a UTF-16 string. UTF-8 is decoded, and each maximal invalid subsequence
// becomes one U+FFFD. POSIX systems and WASI hosts pass arguments that the shell or the host has
// already split. On Windows a process receives one command line, which this file splits: spaces
// and tabs separate arguments except inside double quotes, the quotes are removed, and "" is an
// empty argument. Backslashes have no special meaning.
// On WASI this file is compiled after os-wasi.c and uses its imports.
#include <stdint.h>
#if defined(_WIN32)
#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#include <windows.h>
#endif

// tsuzuri_alloc allocates at least one byte for a size of 0 (the host ABI contract), so an empty
// array or argument needs no special case.
extern void *tsuzuri_alloc(int64_t size);
extern void tsuzuri_free(void *pointer);

// The layouts of %tz.string and %tz.array.
struct tz_arguments_text {
    uint16_t *data;
    int64_t length;
};

struct tz_arguments_array {
    struct tz_arguments_text *data;
    int64_t length;
};

#if !defined(_WIN32) || defined(TZ_ARGUMENTS_TEST)
// Decodes `length` bytes of UTF-8, stores the UTF-16 units in `output` unless it is NULL, and
// returns their number.
static int64_t tz_arguments_decode(const unsigned char *bytes, int64_t length, uint16_t *output) {
    int64_t count = 0;
    int64_t index = 0;
    while (index < length) {
        uint32_t first = bytes[index];
        uint32_t point = first;
        int64_t size = 1;
        if (first >= 0x80) {
            int needed = 0;
            uint32_t low = 0x80, high = 0xBF;
            if (first >= 0xC2 && first <= 0xDF) {
                needed = 1;
                point = first & 0x1F;
            } else if (first >= 0xE0 && first <= 0xEF) {
                needed = 2;
                point = first & 0x0F;
                if (first == 0xE0) low = 0xA0;
                if (first == 0xED) high = 0x9F;
            } else if (first >= 0xF0 && first <= 0xF4) {
                needed = 3;
                point = first & 0x07;
                if (first == 0xF0) low = 0x90;
                if (first == 0xF4) high = 0x8F;
            }
            int taken = 0;
            while (taken < needed && index + 1 + taken < length) {
                uint32_t next = bytes[index + 1 + taken];
                if (next < low || next > high) break;
                point = (point << 6) | (next & 0x3F);
                low = 0x80;
                high = 0xBF;
                taken++;
            }
            size = 1 + taken;
            if (needed == 0 || taken < needed) point = 0xFFFD;
        }
        if (point >= 0x10000) {
            if (output) {
                output[count] = (uint16_t)(0xD800 + ((point - 0x10000) >> 10));
                output[count + 1] = (uint16_t)(0xDC00 + ((point - 0x10000) & 0x3FF));
            }
            count += 2;
        } else {
            if (output) output[count] = (uint16_t)point;
            count++;
        }
        index += size;
    }
    return count;
}

// The arguments `values[1]` to `values[count - 1]`, decoded.
static void tz_arguments_from_utf8(int64_t count, char **values, struct tz_arguments_array *output) {
    int64_t total = count > 1 ? count - 1 : 0;
    struct tz_arguments_text *items = tsuzuri_alloc(total * (int64_t)sizeof *items);
    for (int64_t index = 0; index < total; index++) {
        const unsigned char *bytes = (const unsigned char *)values[index + 1];
        int64_t length = 0;
        while (bytes[length] != 0) length++;
        int64_t units = tz_arguments_decode(bytes, length, 0);
        uint16_t *data = tsuzuri_alloc(units * 2);
        tz_arguments_decode(bytes, length, data);
        items[index] = (struct tz_arguments_text){ data, units };
    }
    output->data = items;
    output->length = total;
}
#endif

#if defined(_WIN32) || defined(TZ_ARGUMENTS_TEST)
static const uint16_t *tz_arguments_blank(const uint16_t *at) {
    while (*at == ' ' || *at == '\t') at++;
    return at;
}

// Reads the argument at `at`, stores its units in `output` unless it is NULL, and returns where
// it ends.
static const uint16_t *tz_arguments_token(const uint16_t *at, uint16_t *output, int64_t *length) {
    int quoted = 0;
    int64_t count = 0;
    for (; *at != 0 && (quoted || (*at != ' ' && *at != '\t')); at++) {
        if (*at == '"') {
            quoted = !quoted;
        } else {
            if (output) output[count] = *at;
            count++;
        }
    }
    *length = count;
    return at;
}

// The arguments of a command line after the program name.
static void tz_arguments_split(const uint16_t *line, struct tz_arguments_array *output) {
    int64_t length = 0;
    const uint16_t *start = tz_arguments_blank(line);
    if (*start != 0) start = tz_arguments_blank(tz_arguments_token(start, 0, &length));
    int64_t count = 0;
    for (const uint16_t *at = start; *at != 0; count++) {
        at = tz_arguments_blank(tz_arguments_token(at, 0, &length));
    }
    struct tz_arguments_text *items = tsuzuri_alloc(count * (int64_t)sizeof *items);
    const uint16_t *at = start;
    for (int64_t index = 0; index < count; index++) {
        tz_arguments_token(at, 0, &length);
        uint16_t *data = tsuzuri_alloc(length * 2);
        at = tz_arguments_blank(tz_arguments_token(at, data, &length));
        items[index] = (struct tz_arguments_text){ data, length };
    }
    output->data = items;
    output->length = count;
}
#endif

#if defined(_WIN32)
void tsuzuri_arguments(int32_t count, char **values, struct tz_arguments_array *output) {
    (void)count;
    (void)values;
    tz_arguments_split((const uint16_t *)GetCommandLineW(), output);
}
#elif defined(__wasm__)
void tsuzuri_arguments(int32_t count, char **values, struct tz_arguments_array *output) {
    (void)count;
    (void)values;
    uint32_t total = 0, size = 0;
    if (tz_wasi_args_sizes_get(&total, &size) != 0) __builtin_trap();
    char **pointers = tsuzuri_alloc((int64_t)total * (int64_t)sizeof(char *));
    char *buffer = tsuzuri_alloc((int64_t)size);
    if (tz_wasi_args_get(pointers, buffer) != 0) __builtin_trap();
    tz_arguments_from_utf8(total, pointers, output);
    tsuzuri_free(pointers);
    tsuzuri_free(buffer);
}
#else
void tsuzuri_arguments(int32_t count, char **values, struct tz_arguments_array *output) {
    tz_arguments_from_utf8(count, values, output);
}
#endif
