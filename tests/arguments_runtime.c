// Checks src/runtime/arguments.c on any host: the Windows command-line splitter and the UTF-8
// decoder that POSIX and WASI arguments go through. Built and run by tests/arguments.rs.
#define TZ_ARGUMENTS_TEST
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

static int64_t live;

void *tsuzuri_alloc(int64_t size) {
    void *value = malloc(size > 0 ? (size_t)size : 1);
    if (!value) abort();
    live++;
    return value;
}

void tsuzuri_free(void *pointer) {
    if (pointer) live--;
    free(pointer);
}

#include "../src/runtime/arguments.c"

static int failures;

static int64_t units(const uint16_t *text) {
    int64_t length = 0;
    while (text[length] != 0) length++;
    return length;
}

// Compares and frees the arguments.
static void expect(const char *name, struct tz_arguments_array *actual, const uint16_t *const *expected, int64_t count) {
    int same = actual->length == count;
    for (int64_t index = 0; same && index < count; index++) {
        int64_t length = units(expected[index]);
        same = actual->data[index].length == length;
        for (int64_t at = 0; same && at < length; at++) same = actual->data[index].data[at] == expected[index][at];
    }
    if (!same) {
        failures++;
        fprintf(stderr, "%s: %lld arguments:", name, (long long)actual->length);
        for (int64_t index = 0; index < actual->length; index++) {
            fprintf(stderr, " [");
            for (int64_t at = 0; at < actual->data[index].length; at++) fprintf(stderr, " %04x", actual->data[index].data[at]);
            fprintf(stderr, " ]");
        }
        fprintf(stderr, "\n");
    }
    for (int64_t index = 0; index < actual->length; index++) tsuzuri_free(actual->data[index].data);
    tsuzuri_free(actual->data);
}

static void split(const uint16_t *line, const uint16_t *const *expected, int64_t count) {
    struct tz_arguments_array actual;
    tz_arguments_split(line, &actual);
    char name[64];
    snprintf(name, sizeof name, "split #%lld", (long long)count);
    expect(name, &actual, expected, count);
}

static void decode(const char *name, const char *value, const uint16_t *expected) {
    char *values[] = { "program", (char *)value };
    struct tz_arguments_array actual;
    tz_arguments_from_utf8(2, values, &actual);
    expect(name, &actual, &expected, 1);
}

#define SPLIT(line, ...) do { \
    const uint16_t *expected[] = { __VA_ARGS__ }; \
    split(line, expected + 1, (int64_t)(sizeof expected / sizeof expected[0]) - 1); \
} while (0)

int main(void) {
    // The first entry of each list is a placeholder, so a line may have no arguments.
    SPLIT(u"", 0);
    SPLIT(u"app", 0);
    SPLIT(u"  app  ", 0);
    SPLIT(u"app a b", 0, u"a", u"b");
    SPLIT(u"app \t a\t\tb ", 0, u"a", u"b");
    SPLIT(u"\"C:\\Program Files\\app.exe\" a", 0, u"a");
    SPLIT(u"app \"b c\" \"\" d", 0, u"b c", u"", u"d");
    SPLIT(u"app a\"b c\"d", 0, u"ab cd");
    SPLIT(u"app \"a\\\" b", 0, u"a\\", u"b");
    SPLIT(u"app C:\\dir\\ \"x\\\\\"", 0, u"C:\\dir\\", u"x\\\\");
    SPLIT(u"app \"unterminated b", 0, u"unterminated b");
    SPLIT(u"app \"\"\"\"", 0, u"");
    SPLIT(u"\"\" a", 0, u"a");
    SPLIT(u"app \u65e5\u672c \"\U0001F600 x\"", 0, u"\u65e5\u672c", u"\U0001F600 x");

    decode("ascii", "abc", u"abc");
    decode("empty", "", u"");
    decode("two bytes", "\xc3\xa9", u"\u00e9");
    decode("three bytes", "\xe6\x97\xa5", u"\u65e5");
    decode("four bytes", "\xf0\x9f\x98\x80", u"\U0001F600");
    decode("lone continuation", "\x80", u"\ufffd");
    decode("invalid lead", "\xff", u"\ufffd");
    decode("overlong", "\xc0\xaf", u"\ufffd\ufffd");
    decode("overlong three", "\xe0\x80\x80", u"\ufffd\ufffd\ufffd");
    decode("surrogate", "\xed\xa0\x80", u"\ufffd\ufffd\ufffd");
    decode("beyond U+10FFFF", "\xf4\x90\x80\x80", u"\ufffd\ufffd\ufffd\ufffd");
    decode("truncated three", "a\xe6\x97", u"a\ufffd");
    decode("truncated four", "\xf0\x9f\x98z", u"\ufffdz");
    decode("interrupted", "\xe6x\x97", u"\ufffdx\ufffd");
    decode("maximum", "\xf4\x8f\xbf\xbf", u"\U0010FFFF");

    char *none[] = { "program" };
    struct tz_arguments_array empty;
    tz_arguments_from_utf8(1, none, &empty);
    expect("no arguments", &empty, NULL, 0);
    tz_arguments_from_utf8(0, NULL, &empty);
    expect("no program name", &empty, NULL, 0);

    if (live != 0) {
        fprintf(stderr, "%lld allocations leaked\n", (long long)live);
        failures++;
    }
    if (failures) return 1;
    puts("arguments runtime: Windows splitting and UTF-8 decoding passed");
    return 0;
}
