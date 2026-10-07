#include <stdint.h>
#define SAMPLE_LIMIT 42
typedef int32_t sample_count;
enum sample_mode { SAMPLE_FAST, SAMPLE_SLOW = 5, SAMPLE_BEST };
struct sample_point { double x; double y; int32_t tag; };
int32_t sample_add(int32_t a, int32_t b);
double sample_norm(const struct sample_point *p);
uint8_t sample_low_byte(uint64_t value);
_Bool sample_is_even(int64_t value);
void sample_reset(void);
enum sample_mode sample_next(enum sample_mode mode);
int sample_log(const char *format, ...);
static inline int sample_twice(int v) { return v * 2; }
