/* The C library that tests/bindgen.mjs binds from sample.h. */
#include <math.h>
#include "sample.h"

static int reset_calls;

int32_t sample_add(int32_t a, int32_t b) { return a + b; }

double sample_norm(const struct sample_point *p) { return sqrt(p->x * p->x + p->y * p->y) + p->tag; }

uint8_t sample_low_byte(uint64_t value) { return (uint8_t)value; }

_Bool sample_is_even(int64_t value) { return value % 2 == 0; }

void sample_reset(void) { reset_calls++; }

/* Not in sample.h: the host reads how often Tsuzuri called sample_reset. */
int sample_reset_count(void) { return reset_calls; }

enum sample_mode sample_next(enum sample_mode mode) {
    return mode == SAMPLE_FAST ? SAMPLE_SLOW : SAMPLE_BEST;
}

int sample_log(const char *format, ...) { return format != 0; }
