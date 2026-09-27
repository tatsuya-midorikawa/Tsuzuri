#ifndef TZ_MATH_H
#define TZ_MATH_H
#include <stdint.h>
#include <float.h>
typedef double double_t;
typedef float float_t;
#define hidden __attribute__((visibility("hidden")))
#define INFINITY (__builtin_inff())
#define NAN (__builtin_nanf(""))
#define M_PI 0x1.921fb54442d18p+1
#define M_PI_2 0x1.921fb54442d18p+0
#define M_PI_4 0x1.921fb54442d18p-1
#define isnan(value) __builtin_isnan(value)
#define isfinite(value) __builtin_isfinite(value)
#define isinf(value) __builtin_isinf(value)
#define signbit(value) __builtin_signbit(value)
#define TZ_UNARY(name) double name(double); float name##f(float);
#define TZ_BINARY(name) double name(double, double); float name##f(float, float);
TZ_UNARY(sin)
TZ_UNARY(cos)
TZ_UNARY(tan)
TZ_UNARY(asin)
TZ_UNARY(acos)
TZ_UNARY(atan)
TZ_UNARY(exp)
TZ_UNARY(exp2)
TZ_UNARY(log)
TZ_UNARY(log2)
TZ_UNARY(log10)
TZ_UNARY(cbrt)
TZ_UNARY(sqrt)
TZ_UNARY(fabs)
TZ_UNARY(floor)
TZ_BINARY(atan2)
TZ_BINARY(pow)
TZ_BINARY(hypot)
double scalbn(double, int);
float scalbnf(float, int);
#undef TZ_UNARY
#undef TZ_BINARY
#endif