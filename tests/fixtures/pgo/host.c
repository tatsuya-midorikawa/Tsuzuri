#include <stdint.h>
#include <stdio.h>

extern int64_t tz_total(int64_t);

int main(void) {
    printf("%lld\n", (long long)tz_total(100000));
    return 0;
}
