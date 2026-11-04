/* The host of tests/fixtures/blocking (TZ08), linked by tests/tzroutine.mjs: a call that sleeps, and
 * one that calls Tsuzuri back in the middle of its own blocking work. */
#include <stdint.h>
#include <time.h>

static void pause_ms(int64_t ms) {
    struct timespec t = {(time_t)(ms / 1000), (long)(ms % 1000) * 1000000L};
    while (nanosleep(&t, &t) != 0) {
    }
}

void tsuzuri_host_Main_host_sleep_ms(int64_t ms) { pause_ms(ms); }

int64_t tsuzuri_host_Main_host_apply(int64_t (*callback)(int64_t), int64_t x) {
    pause_ms(2);
    int64_t result = callback(x);
    pause_ms(2);
    return result;
}
