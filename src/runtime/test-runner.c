#if defined(TSUZURI_COVERAGE) && defined(_WIN32)
#define _CRT_SECURE_NO_WARNINGS
#endif
#include <errno.h>
#include <stdint.h>
#include <stdlib.h>
#ifdef TSUZURI_COVERAGE
#include <stdio.h>

extern uint64_t tsuzuri_coverage_counters[];
extern const uint64_t tsuzuri_coverage_count;

/* Writes the counters of a passed test to TSUZURI_COVERAGE_FILE in native byte order. */
static int write_coverage(void) {
    const char *path = getenv("TSUZURI_COVERAGE_FILE");
    if (path == NULL) return 3;
    FILE *file = fopen(path, "wb");
    if (file == NULL) return 3;
    size_t count = (size_t)tsuzuri_coverage_count;
    size_t written = count == 0 ? 0 : fwrite(tsuzuri_coverage_counters, sizeof(uint64_t), count, file);
    int closed = fclose(file);
    return written == count && closed == 0 ? 0 : 3;
}
#endif

extern uint32_t tsuzuri_test_count(void);
extern int32_t tsuzuri_test_run(uint32_t index);

int main(int argc, char **argv) {
    if (argc != 2 || argv[1][0] < '0' || argv[1][0] > '9') return 2;
    char *end = NULL;
    errno = 0;
    unsigned long long index = strtoull(argv[1], &end, 10);
    if (errno == ERANGE || end == argv[1] || *end != '\0' ||
        index > UINT32_MAX || index >= tsuzuri_test_count()) return 2;
#ifdef TSUZURI_COVERAGE
    int32_t result = tsuzuri_test_run((uint32_t)index);
    return result == 0 ? write_coverage() : result;
#else
    return tsuzuri_test_run((uint32_t)index);
#endif
}
