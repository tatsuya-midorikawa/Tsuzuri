#include <stdint.h>
#include <string.h>

#ifdef _WIN32
#define M06_EXPORT __declspec(dllexport)
#else
#define M06_EXPORT __attribute__((visibility("default")))
#endif

#ifndef M06_ZUTF8_ONLY
typedef struct m06_pair {
    int32_t x;
    int32_t y;
} m06_pair;

typedef struct m06_queue {
    int64_t tag;
} m06_queue;

static const int64_t m06_target_box = 777;

M06_EXPORT int8_t m06_min_i8 = -128;
M06_EXPORT uint16_t m06_max_u16 = 65535u;
M06_EXPORT int64_t m06_min_i64 = INT64_MIN;
M06_EXPORT float m06_neg_zero = -0.0f;
/* Quiet NaN with payload 0x12345. */
M06_EXPORT uint64_t m06_nan_raw = UINT64_C(0x7ff8000000012345);
M06_EXPORT double m06_nan_f64 = 0.0;
M06_EXPORT const void *m06_null_ptr = 0;
M06_EXPORT const void *m06_nonnull_ptr = &m06_target_box;
M06_EXPORT m06_pair m06_point = { -17, 29 };
M06_EXPORT m06_queue m06_main_q = { 909 };
M06_EXPORT int32_t m06_mutable = 20;

M06_EXPORT void m06_init_nan(void) {
    memcpy(&m06_nan_f64, &m06_nan_raw, sizeof(double));
}

M06_EXPORT void m06_mutate(int32_t next) {
    m06_mutable = next;
}

M06_EXPORT int32_t m06_check_queue_addr(const void *ptr) {
    return ptr == (const void *)&m06_main_q ? 1 : 0;
}

M06_EXPORT int64_t m06_read_box(const void *ptr) {
    return *(const int64_t *)ptr;
}
#endif

static const char m06_utf8_greeting[] = "\xE3\x81\x93\xE3\x82\x93\xE3\x81\xAB\xE3\x81\xA1\xE3\x81\xAF";
static const char m06_invalid_utf8[] = "\xFF\xFE";

M06_EXPORT int64_t m06_c_strlen(const char *s) {
    return s ? (int64_t)strlen(s) : -1;
}

M06_EXPORT const char *m06_c_lookup(int32_t kind) {
    if (kind == 0) {
        return 0;
    }
    if (kind == 1) {
        return m06_utf8_greeting;
    }
    if (kind == 2) {
        return "";
    }
    return m06_invalid_utf8;
}
