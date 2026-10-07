/* Phase 2 of E11: integer #defines, opaque struct handles, callbacks, and buffers. */
#ifndef EXT_H
#define EXT_H
#include <stddef.h>
#include <stdint.h>

#define EXT_VERSION 3
#define EXT_MASK 0xFFu
#define EXT_NEGATIVE (-42)
#define EXT_OCTAL 0755
#define EXT_BINARY 0b1010
#define EXT_PAREN ((7))
#define EXT_WIDE 2147483648
#define EXT_HEX_WIDE 0x80000000
#define EXT_MIN_INT (-2147483648)
#define EXT_ALL_ONES (-1u)
#define EXT_TOP 0xFFFFFFFFFFFFFFFFull
#define EXT_LONG 5L
#define EXT_HUGE 18446744073709551616
#define EXT_SIGNED_TOO_BIG 9223372036854775808
#define EXT_BAD_OCTAL 09
#define EXT_RATIO 1.5
#define EXT_FLAG (1 << 4)
#define EXT_NAME "ext"
#define EXT_TWICE(x) ((x) * 2)
#define EXT_TEMPORARY 1
#undef EXT_TEMPORARY

struct ext_counter;
typedef struct ext_counter ext_counter;
ext_counter *ext_counter_new(int64_t start);
int64_t ext_counter_add(ext_counter *counter, int64_t amount);
int64_t ext_counter_peek(const ext_counter *counter);
int64_t ext_counter_free(ext_counter *counter);
const ext_counter *ext_counter_shared(void);
int ext_counter_open(ext_counter **out);

typedef int64_t (*ext_map)(int64_t value);
int64_t ext_apply_twice(ext_map map, int64_t value);
int64_t ext_fold(int64_t (*step)(int64_t total, int32_t item), int64_t start);
int64_t ext_visit(int64_t (*visit)(const ext_counter *counter), const ext_counter *counter);
int32_t ext_run(int32_t (*task)(void));
_Bool ext_any(_Bool (*test)(_Bool flag, uint8_t small), int32_t count);
int ext_with_context(void (*callback)(void *context), void *context);

int64_t ext_sum(const int64_t *values, size_t count);
double ext_mean(const double *values, int64_t count);
uint64_t ext_checksum(const unsigned char *bytes, size_t length);
size_t ext_text_length(const char *text, size_t length);
int ext_fill(int64_t *values, size_t count);
int ext_split(const int64_t *values, int flags, size_t count);
int64_t ext_sum32(const int32_t *values, size_t count);
#endif
