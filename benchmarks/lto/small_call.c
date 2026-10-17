// PR08: the host of the `lto/small_call` benchmark (benchmarks/run-lto.mjs). It calls a small exported Tsuzuri function in
// a loop. `values` is filled at run time and `n` is an argument, so the loop cannot be folded into a constant; only a
// link that sees the body of `tz_add` can inline it.
//
//   tz_add         the Tsuzuri function `fn add x y = x + y`, from an object (variant object) or from bitcode (variant
//                  bitcode)
//   -DADD=c_add    the same function written in C (variant c-only)
//
// `c_add` is the `add i64` of Tsuzuri's `+` (it wraps) without undefined behavior.
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

int64_t tz_add(int64_t x, int64_t y);
static int64_t c_add(int64_t x, int64_t y) { return (int64_t)((uint64_t)x + (uint64_t)y); }
#ifndef ADD
#define ADD tz_add
#endif

int main(int argc, char **argv) {
    int64_t n = argc > 1 ? atoll(argv[1]) : 1000;
    int64_t values[1024];
    for (int i = 0; i < 1024; i++) values[i] = (int64_t)(i * 2654435761u % 1000u);
    int64_t total = 0;
    for (int64_t i = 0; i < n; i++) total = ADD(total, values[i & 1023]);
    printf("%lld\n", (long long)total);
    return 0;
}
