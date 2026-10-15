#undef NDEBUG
#define _POSIX_C_SOURCE 200809L
#include <assert.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <time.h>

#define DECLARE(OP) extern int64_t tz_utf_##OP(int64_t, int64_t, int64_t)
DECLARE(to_utf8); DECLARE(from_utf8); DECLARE(equal); DECLARE(compare);
DECLARE(is_well_formed); DECLARE(to_well_formed);
static double now(void) {
    struct timespec t;
    assert(clock_gettime(CLOCK_MONOTONIC, &t) == 0);
    return (double)t.tv_sec + (double)t.tv_nsec * 1e-9;
}
int main(int argc, char **argv) {
    const int quick = argc > 1 && atoi(argv[1]);
    int64_t (*operations[])(int64_t,int64_t,int64_t) = {
        tz_utf_to_utf8,tz_utf_from_utf8,tz_utf_equal,tz_utf_compare,tz_utf_is_well_formed,tz_utf_to_well_formed
    };
    const char *names[] = {"to_utf8","from_utf8","equal","compare","is_well_formed","to_well_formed"};
    const char *kinds[] = {"ascii","latin","cjk","emoji","mixed"};
    const int bytes[] = {8,11,24,16,13};
    const int64_t sizes[] = {8,64,65536};
    for (unsigned kind = 0; kind < 5; ++kind) for (unsigned si = 0; si < (quick ? 1u : 3u); ++si) {
        int64_t size = quick ? 64 : sizes[si];
        int64_t repeat = quick ? 2 : INT64_C(16777216)/size;
        double values[6][9] = {{0}};
        unsigned samples = quick ? 1 : 9;
        for (unsigned sample = 0; sample < samples; ++sample) for (unsigned offset = 0; offset < 6; ++offset) {
            unsigned op = (sample+offset)%6;
            int64_t expected = repeat * (op == 0 ? size/8*bytes[kind] : op == 1 ? size : op == 5 ? size+1 : 1);
            double start = now();
            assert(operations[op](kind,size,repeat) == expected);
            values[op][sample] = (now()-start)*1e9/(double)(size*repeat);
        }
        for (unsigned op = 0; op < 6; ++op) {
            printf("{\"op\":\"%s\",\"kind\":\"%s\",\"size\":%lld,\"repeat\":%lld,\"samples\":[",
                names[op],kinds[kind],(long long)size,(long long)repeat);
            for (unsigned i = 0; i < samples; ++i) printf("%s%.9f",i?",":"",values[op][i]);
            for (unsigned i = 1; i < samples; ++i) for (unsigned j = i; j && values[op][j] < values[op][j-1]; --j) {
                double x = values[op][j]; values[op][j] = values[op][j-1]; values[op][j-1] = x;
            }
            printf("],\"ns_per_unit\":%.9f,\"min\":%.9f,\"max\":%.9f}\n",
                values[op][samples/2],values[op][0],values[op][samples-1]);
        }
    }
}
