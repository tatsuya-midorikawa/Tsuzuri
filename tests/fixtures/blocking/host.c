/* The host of tests/fixtures/blocking (TZ08), linked by tests/tzroutine.mjs: a call that waits until
 * another tzroutine releases it (with a watchdog that ends a hang), and two that call Tsuzuri back in
 * the middle of their own blocking work: through a function value, and through a public export. */
#include <stdatomic.h>
#include <stdint.h>
#include <stdlib.h>
#include <time.h>

static void pause_ms(int64_t ms) {
    struct timespec t = {(time_t)(ms / 1000), (long)(ms % 1000) * 1000000L};
    while (nanosleep(&t, &t) != 0) {
    }
}

static _Atomic int released;

void tsuzuri_host_Main_host_wait_released(void) {
    for (int waited = 0; !atomic_load(&released); waited++) {
        if (waited > 60000) abort(); /* a minute: the others never ran */
        pause_ms(1);
    }
}

void tsuzuri_host_Main_host_release(void) { atomic_store(&released, 1); }

int64_t tsuzuri_host_Main_host_apply(int64_t (*callback)(int64_t), int64_t x) {
    pause_ms(2);
    int64_t result = callback(x);
    pause_ms(2);
    return result;
}

int64_t tz_doubled_export(int64_t x);

int64_t tsuzuri_host_Main_host_call_export(int64_t x) {
    pause_ms(2);
    int64_t result = tz_doubled_export(x);
    pause_ms(2);
    return result;
}
