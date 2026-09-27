#include <errno.h>
#include <stdint.h>
#include <stdlib.h>

extern uint32_t tsuzuri_test_count(void);
extern int32_t tsuzuri_test_run(uint32_t index);

int main(int argc, char **argv) {
    if (argc != 2 || argv[1][0] < '0' || argv[1][0] > '9') return 2;
    char *end = NULL;
    errno = 0;
    unsigned long long index = strtoull(argv[1], &end, 10);
    if (errno == ERANGE || end == argv[1] || *end != '\0' ||
        index > UINT32_MAX || index >= tsuzuri_test_count()) return 2;
    return tsuzuri_test_run((uint32_t)index);
}