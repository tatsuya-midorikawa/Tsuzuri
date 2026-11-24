/* Portable extended Elementary math functions for NC01 (f64 and f32).
 * Compiled into src/runtime/math.ll with -ffreestanding -fno-builtin -fno-fast-math
 * -ffp-contract=off -fno-math-errno so native and WebAssembly produce identical bits.
 */
#include <stdint.h>

extern double tz_math_exp_f64(double);
extern double tz_math_exp2_f64(double);
extern double tz_math_log_f64(double);
extern double tz_math_log2_f64(double);
extern double tz_math_sin_f64(double);
extern double tz_math_floor_f64(double);
extern double tz_math_scalbn_f64(double, int);
extern double tz_math_sqrt_f64(double);
extern double tz_math_fabs_f64(double);

static inline uint64_t tz_f64_to_bits(double x) {
    union { double f; uint64_t u; } v = { x };
    return v.u;
}
static inline double tz_bits_to_f64(uint64_t u) {
    union { uint64_t u; double f; } v = { u };
    return v.f;
}
static inline uint32_t tz_f32_to_bits(float x) {
    union { float f; uint32_t u; } v = { x };
    return v.u;
}
static inline float tz_bits_to_f32(uint32_t u) {
    union { uint32_t u; float f; } v = { u };
    return v.f;
}

static inline int tz_f64_isnan(double x) {
    return (tz_f64_to_bits(x) & UINT64_C(0x7fffffffffffffff)) > UINT64_C(0x7ff0000000000000);
}
static inline int tz_f64_isinf(double x) {
    return (tz_f64_to_bits(x) & UINT64_C(0x7fffffffffffffff)) == UINT64_C(0x7ff0000000000000);
}
static inline double tz_f64_copysign(double mag, double sgn) {
    uint64_t m = tz_f64_to_bits(mag) & UINT64_C(0x7fffffffffffffff);
    uint64_t s = tz_f64_to_bits(sgn) & UINT64_C(0x8000000000000000);
    return tz_bits_to_f64(m | s);
}
static inline float tz_f32_copysign(float mag, float sgn) {
    uint32_t m = tz_f32_to_bits(mag) & 0x7fffffffu;
    uint32_t s = tz_f32_to_bits(sgn) & 0x80000000u;
    return tz_bits_to_f32(m | s);
}

/* Exact IEEE 754 binary fmod (musl algorithm). */
double tz_math_fmod_f64(double x, double y) {
    uint64_t ux = tz_f64_to_bits(x);
    uint64_t uy = tz_f64_to_bits(y);
    int ex = (int)((ux >> 52) & 0x7ff);
    int ey = (int)((uy >> 52) & 0x7ff);
    uint64_t sx = ux >> 63;
    uint64_t i;

    if ((uy << 1) == 0 || tz_f64_isnan(y) || ex == 0x7ff) {
        return (x * y) / (x * y);
    }
    if ((ux << 1) <= (uy << 1)) {
        if ((ux << 1) == (uy << 1)) return 0.0 * x;
        return x;
    }

    if (!ex) {
        for (i = ux << 12; (i >> 63) == 0; ex--, i <<= 1) {}
        ux <<= -ex + 1;
    } else {
        ux &= UINT64_C(0xffffffffffffffff) >> 12;
        ux |= UINT64_C(1) << 52;
    }
    if (!ey) {
        for (i = uy << 12; (i >> 63) == 0; ey--, i <<= 1) {}
        uy <<= -ey + 1;
    } else {
        uy &= UINT64_C(0xffffffffffffffff) >> 12;
        uy |= UINT64_C(1) << 52;
    }

    for (; ex > ey; ex--) {
        i = ux - uy;
        if ((i >> 63) == 0) {
            if (i == 0) return 0.0 * x;
            ux = i;
        }
        ux <<= 1;
    }
    i = ux - uy;
    if ((i >> 63) == 0) {
        if (i == 0) return 0.0 * x;
        ux = i;
    }
    for (; (ux >> 52) == 0; ux <<= 1, ex--) {}

    if (ex > 0) {
        ux -= UINT64_C(1) << 52;
        ux |= (uint64_t)ex << 52;
    } else {
        ux >>= -ex + 1;
    }
    ux |= sx << 63;
    return tz_bits_to_f64(ux);
}

float tz_math_fmod_f32(float x, float y) {
    uint32_t ux = tz_f32_to_bits(x);
    uint32_t uy = tz_f32_to_bits(y);
    int ex = (int)((ux >> 23) & 0xff);
    int ey = (int)((uy >> 23) & 0xff);
    uint32_t sx = ux & 0x80000000u;
    uint32_t i;

    if ((uy << 1) == 0 || (uy & 0x7fffffffu) > 0x7f800000u || ex == 0xff) {
        return (x * y) / (x * y);
    }
    if ((ux << 1) <= (uy << 1)) {
        if ((ux << 1) == (uy << 1)) return 0.0f * x;
        return x;
    }

    if (!ex) {
        for (i = ux << 9; (i >> 31) == 0; ex--, i <<= 1) {}
        ux <<= -ex + 1;
    } else {
        ux &= 0x007fffffu;
        ux |= 0x00800000u;
    }
    if (!ey) {
        for (i = uy << 9; (i >> 31) == 0; ey--, i <<= 1) {}
        uy <<= -ey + 1;
    } else {
        uy &= 0x007fffffu;
        uy |= 0x00800000u;
    }

    for (; ex > ey; ex--) {
        i = ux - uy;
        if ((i >> 31) == 0) {
            if (i == 0) return 0.0f * x;
            ux = i;
        }
        ux <<= 1;
    }
    i = ux - uy;
    if ((i >> 31) == 0) {
        if (i == 0) return 0.0f * x;
        ux = i;
    }
    for (; (ux >> 23) == 0; ux <<= 1, ex--) {}

    if (ex > 0) {
        ux -= 0x00800000u;
        ux |= (uint32_t)ex << 23;
    } else {
        ux >>= -ex + 1;
    }
    ux |= sx;
    return tz_bits_to_f32(ux);
}

/* Exact IEEE 754 remainder (round-to-nearest-even quotient, musl remquo algorithm). */
double tz_math_ieee_remainder_f64(double x, double y) {
    uint64_t ux = tz_f64_to_bits(x);
    uint64_t uy = tz_f64_to_bits(y);
    int ex = (int)((ux >> 52) & 0x7ff);
    int ey = (int)((uy >> 52) & 0x7ff);
    int sx = (int)(ux >> 63);
    uint64_t i;
    uint32_t q = 0;

    if ((uy << 1) == 0 || tz_f64_isnan(y) || ex == 0x7ff) {
        return (x * y) / (x * y);
    }
    if ((ux << 1) == 0) return x;

    if (!ex) {
        for (i = ux << 12; (i >> 63) == 0; ex--, i <<= 1) {}
        ux <<= -ex + 1;
    } else {
        ux &= UINT64_C(0xffffffffffffffff) >> 12;
        ux |= UINT64_C(1) << 52;
    }
    if (!ey) {
        for (i = uy << 12; (i >> 63) == 0; ey--, i <<= 1) {}
        uy <<= -ey + 1;
    } else {
        uy &= UINT64_C(0xffffffffffffffff) >> 12;
        uy |= UINT64_C(1) << 52;
    }

    if (ex < ey) {
        if (ex + 1 == ey) {
            /* x >= y/2? handled below via double comparison */
            double ax = tz_math_fabs_f64(x);
            double ay = tz_math_fabs_f64(y);
            if (ax > 0.5 * ay) {
                ax -= ay;
                return sx ? -ax : ax;
            }
        }
        return x;
    }

    for (; ex > ey; ex--) {
        i = ux - uy;
        if ((i >> 63) == 0) {
            ux = i;
            q++;
        }
        ux <<= 1;
        q <<= 1;
    }
    i = ux - uy;
    if ((i >> 63) == 0) {
        ux = i;
        q++;
    }
    if (ux == 0) return tz_bits_to_f64((uint64_t)sx << 63);

    /* Compare 2*ux against uy for nearest-even tie-breaking. */
    if (ux + ux > uy || (ux + ux == uy && (q & 1u) != 0)) {
        ux = uy - ux;
        sx ^= 1;
    }
    for (; (ux >> 52) == 0; ux <<= 1, ex--) {}
    if (ex > 0) {
        ux -= UINT64_C(1) << 52;
        ux |= (uint64_t)ex << 52;
    } else {
        ux >>= -ex + 1;
    }
    ux |= (uint64_t)sx << 63;
    return tz_bits_to_f64(ux);
}

float tz_math_ieee_remainder_f32(float x, float y) {
    uint32_t ux = tz_f32_to_bits(x);
    uint32_t uy = tz_f32_to_bits(y);
    int ex = (int)((ux >> 23) & 0xff);
    int ey = (int)((uy >> 23) & 0xff);
    int sx = (int)(ux >> 31);
    uint32_t i;
    uint32_t q = 0;

    if ((uy << 1) == 0 || (uy & 0x7fffffffu) > 0x7f800000u || ex == 0xff) {
        return (x * y) / (x * y);
    }
    if ((ux << 1) == 0) return x;

    if (!ex) {
        for (i = ux << 9; (i >> 31) == 0; ex--, i <<= 1) {}
        ux <<= -ex + 1;
    } else {
        ux &= 0x007fffffu;
        ux |= 0x00800000u;
    }
    if (!ey) {
        for (i = uy << 9; (i >> 31) == 0; ey--, i <<= 1) {}
        uy <<= -ey + 1;
    } else {
        uy &= 0x007fffffu;
        uy |= 0x00800000u;
    }

    if (ex < ey) {
        if (ex + 1 == ey) {
            float ax = x < 0.0f ? -x : x;
            float ay = y < 0.0f ? -y : y;
            if (ax > 0.5f * ay) {
                ax -= ay;
                return sx ? -ax : ax;
            }
        }
        return x;
    }

    for (; ex > ey; ex--) {
        i = ux - uy;
        if ((i >> 31) == 0) {
            ux = i;
            q++;
        }
        ux <<= 1;
        q <<= 1;
    }
    i = ux - uy;
    if ((i >> 31) == 0) {
        ux = i;
        q++;
    }
    if (ux == 0) return tz_bits_to_f32((uint32_t)sx << 31);

    if (ux + ux > uy || (ux + ux == uy && (q & 1u) != 0)) {
        ux = uy - ux;
        sx ^= 1;
    }
    for (; (ux >> 23) == 0; ux <<= 1, ex--) {}
    if (ex > 0) {
        ux -= 0x00800000u;
        ux |= (uint32_t)ex << 23;
    } else {
        ux >>= -ex + 1;
    }
    ux |= (uint32_t)sx << 31;
    return tz_bits_to_f32(ux);
}

/* Floor remainder & floor division (NumPy remainder / floor_divide semantics). */
double tz_math_floor_mod_f64(double x, double y) {
    if (tz_f64_isnan(x) || tz_f64_isnan(y) || y == 0.0 || tz_f64_isinf(x)) {
        return (x * y) / (x * y);
    }
    double r = tz_math_fmod_f64(x, y);
    if (r != 0.0) {
        if ((r < 0.0) != (y < 0.0)) r += y;
    } else {
        r = tz_f64_copysign(0.0, y);
    }
    return r;
}

float tz_math_floor_mod_f32(float x, float y) {
    uint32_t ux = tz_f32_to_bits(x) & 0x7fffffffu;
    uint32_t uy = tz_f32_to_bits(y) & 0x7fffffffu;
    if (ux >= 0x7f800000u || uy > 0x7f800000u || uy == 0) {
        return (x * y) / (x * y);
    }
    float r = tz_math_fmod_f32(x, y);
    if (r != 0.0f) {
        if ((r < 0.0f) != (y < 0.0f)) r += y;
    } else {
        r = tz_f32_copysign(0.0f, y);
    }
    return r;
}

double tz_math_floor_div_f64(double x, double y) {
    if (tz_f64_isnan(x) || tz_f64_isnan(y)) return x + y;
    if (y == 0.0 || tz_f64_isinf(x)) return x / y;
    if (tz_f64_isinf(y)) {
        if (x == 0.0) return x / y;
        return ((x < 0.0) != (y < 0.0)) ? -1.0 : 0.0;
    }
    double r = tz_math_fmod_f64(x, y);
    double q = (x - r) / y;
    if (r != 0.0) {
        if ((r < 0.0) != (y < 0.0)) q -= 1.0;
    } else {
        q = tz_f64_copysign(q, (x < 0.0) == (y < 0.0) ? 1.0 : -1.0);
    }
    return tz_math_floor_f64(q);
}

float tz_math_floor_div_f32(float x, float y) {
    uint32_t ux = tz_f32_to_bits(x) & 0x7fffffffu;
    uint32_t uy = tz_f32_to_bits(y) & 0x7fffffffu;
    if (ux > 0x7f800000u || uy > 0x7f800000u) return x + y;
    if (uy == 0 || ux == 0x7f800000u) return x / y;
    if (uy == 0x7f800000u) {
        if (x == 0.0f) return x / y;
        return ((x < 0.0f) != (y < 0.0f)) ? -1.0f : 0.0f;
    }
    double xd = (double)x, yd = (double)y;
    return (float)tz_math_floor_div_f64(xd, yd);
}

/* Sun/musl expm1 for f64 (<= 1 ulp across full domain). */
double tz_math_expm1_f64(double x) {
    static const double o_threshold = 7.09782712893383973096e+02;
    static const double ln2_hi = 6.93147180369123816490e-01;
    static const double ln2_lo = 1.90821492927058770002e-10;
    static const double invln2 = 1.44269504088896338700e+00;
    static const double Q1 = -3.33333333333331316428e-02;
    static const double Q2 =  1.58730158725481460165e-03;
    static const double Q3 = -7.93650757867487942473e-05;
    static const double Q4 =  4.00821782732936239552e-06;
    static const double Q5 = -2.01099218183624371326e-07;

    uint64_t ux = tz_f64_to_bits(x);
    uint32_t hx = (uint32_t)(ux >> 32);
    uint32_t sign = hx >> 31;
    hx &= 0x7fffffffu;

    if (hx >= 0x4043687au) { /* |x| >= 56*ln2 */
        if (hx >= 0x7ff00000u) {
            if (((ux & UINT64_C(0x000fffffffffffff)) != 0)) return x; /* NaN */
            return sign ? -1.0 : x; /* -Inf -> -1, +Inf -> +Inf */
        }
        if (sign) return -1.0;
        if (x > o_threshold) return tz_bits_to_f64(UINT64_C(0x7ff0000000000000));
    }

    int k;
    double hi = 0.0, lo = 0.0, c = 0.0;
    if (hx > 0x3fd62e42u) { /* |x| > 0.5*ln2 */
        if (hx < 0x3ff0a2b2u) { /* |x| < 1.5*ln2 */
            if (!sign) { hi = x - ln2_hi; lo = ln2_lo; k = 1; }
            else       { hi = x + ln2_hi; lo = -ln2_lo; k = -1; }
        } else {
            k = (int)(invln2 * x + (sign ? -0.5 : 0.5));
            double t = (double)k;
            hi = x - t * ln2_hi;
            lo = t * ln2_lo;
        }
        x = hi - lo;
        c = (hi - x) - lo;
    } else if (hx < 0x3c900000u) { /* |x| < 2^-54 */
        return x;
    } else {
        k = 0;
    }

    double hfx = 0.5 * x;
    double hxs = x * hfx;
    double r1 = 1.0 + hxs * (Q1 + hxs * (Q2 + hxs * (Q3 + hxs * (Q4 + hxs * Q5))));
    double t = 3.0 - r1 * hfx;
    double e = hxs * ((r1 - t) / (6.0 - x * t));
    if (k == 0) {
        return x - (x * e - hxs);
    }
    e = (x * (e - c) - c);
    e -= hxs;
    if (k == -1) return 0.5 * (x - e) - 0.5;
    if (k == 1) {
        if (x < -0.25) return -2.0 * (e - (x + 0.5));
        return 1.0 + 2.0 * (x - e);
    }
    if (k <= -2 || k > 56) {
        double y = 1.0 - (e - x);
        return tz_math_scalbn_f64(y, k) - 1.0;
    }
    if (k < 20) {
        double t2 = tz_bits_to_f64(UINT64_C(0x3ff0000000000000) - (UINT64_C(0x20000000000000) >> k));
        double y = t2 - (e - x);
        return tz_math_scalbn_f64(y, k);
    }
    double t2 = tz_bits_to_f64((uint64_t)(0x3ff - k) << 52);
    double y = x - (e + t2) + 1.0;
    return tz_math_scalbn_f64(y, k);
}

float tz_math_expm1_f32(float x) {
    return (float)tz_math_expm1_f64((double)x);
}

/* Sun/musl log1p for f64 (<= 1 ulp across full domain). */
double tz_math_log1p_f64(double x) {
    static const double ln2_hi = 6.93147180369123816490e-01;
    static const double ln2_lo = 1.90821492927058770002e-10;
    static const double Lg1 = 6.666666666666735130e-01;
    static const double Lg2 = 3.999999999940941908e-01;
    static const double Lg3 = 2.857142874366239149e-01;
    static const double Lg4 = 2.222219843214978396e-01;
    static const double Lg5 = 1.818357216161805012e-01;
    static const double Lg6 = 1.531383769920937332e-01;
    static const double Lg7 = 1.479819860511658591e-01;

    uint64_t ux = tz_f64_to_bits(x);
    uint32_t hx = (uint32_t)(ux >> 32);
    uint32_t ax = hx & 0x7fffffffu;

    if (tz_f64_isnan(x)) return x;
    if (x == 0.0) return x;
    if (x < -1.0) return (x - x) / (x - x); /* NaN */
    if (x == -1.0) return tz_bits_to_f64(UINT64_C(0xfff0000000000000)); /* -Inf */
    if (tz_f64_isinf(x)) return x; /* +Inf */

    if (ax < 0x3c900000u) return x; /* |x| < 2^-54 */

    int k = 1;
    double c = 0.0, f = x;
    if (hx < 0x3fda827au || (hx >> 31) != 0) { /* x < sqrt(2)-1 or x < 0 */
        if (x >= -0.29289321881345247560 && x <= 0.41421356237309504880) {
            k = 0;
            f = x;
            c = 0.0;
        }
    }
    if (k != 0) {
        double u;
        if (ax >= 0x43500000u) { /* x >= 2^54 */
            return tz_math_log_f64(x);
        }
        u = 1.0 + x;
        uint64_t uu = tz_f64_to_bits(u);
        int hu = (int)(uu >> 32);
        k = (hu >> 20) - 1023;
        if (k > 0) {
            c = 1.0 - (u - x);
        } else {
            c = x - (u - 1.0);
        }
        c /= u;
        hu = (hu & 0x000fffff) | 0x3ff00000;
        if (hu > 0x3ff6a09e) { /* > sqrt(2) */
            hu -= 0x00100000;
            k++;
        }
        u = tz_bits_to_f64(((uint64_t)(uint32_t)hu << 32) | (uu & UINT64_C(0xffffffff)));
        f = u - 1.0;
    }

    double hfsq = 0.5 * f * f;
    double s = f / (2.0 + f);
    double z = s * s;
    double w = z * z;
    double t1 = w * (Lg2 + w * (Lg4 + w * Lg6));
    double t2 = z * (Lg1 + w * (Lg3 + w * (Lg5 + w * Lg7)));
    double R = t2 + t1;
    if (k == 0) {
        return f - (hfsq - s * (hfsq + R));
    }
    double dk = (double)k;
    return dk * ln2_hi - ((hfsq - (s * (hfsq + R) + (dk * ln2_lo + c))) - f);
}

float tz_math_log1p_f32(float x) {
    return (float)tz_math_log1p_f64((double)x);
}

/* Hyperbolic and inverse hyperbolic functions (<= 1 ulp). */
double tz_math_sinh_f64(double x) {
    if (tz_f64_isnan(x) || tz_f64_isinf(x) || x == 0.0) return x;
    double ax = tz_math_fabs_f64(x);
    double res;
    if (ax < 22.0) {
        double t = tz_math_expm1_f64(ax);
        res = 0.5 * (t + t / (t + 1.0));
    } else if (ax < 709.782712893384) {
        res = 0.5 * tz_math_exp_f64(ax);
    } else {
        double h = tz_math_exp_f64(0.5 * ax);
        res = (0.5 * h) * h;
    }
    return tz_f64_copysign(res, x);
}

float tz_math_sinh_f32(float x) {
    return (float)tz_math_sinh_f64((double)x);
}

double tz_math_cosh_f64(double x) {
    if (tz_f64_isnan(x)) return x;
    if (tz_f64_isinf(x)) return tz_bits_to_f64(UINT64_C(0x7ff0000000000000));
    if (x == 0.0) return 1.0;
    double ax = tz_math_fabs_f64(x);
    if (ax < 22.0) {
        double t = tz_math_expm1_f64(ax);
        double w = 1.0 + t;
        return 1.0 + (t * t) / (2.0 * w);
    }
    if (ax < 709.782712893384) {
        return 0.5 * tz_math_exp_f64(ax);
    }
    double h = tz_math_exp_f64(0.5 * ax);
    return (0.5 * h) * h;
}

float tz_math_cosh_f32(float x) {
    return (float)tz_math_cosh_f64((double)x);
}

double tz_math_tanh_f64(double x) {
    if (tz_f64_isnan(x) || x == 0.0) return x;
    double ax = tz_math_fabs_f64(x);
    double res;
    if (ax >= 22.0) {
        res = 1.0;
    } else if (ax >= 0.5) {
        double t = tz_math_expm1_f64(2.0 * ax);
        res = t / (t + 2.0);
    } else {
        double t = tz_math_expm1_f64(-2.0 * ax);
        res = -t / (t + 2.0);
    }
    return tz_f64_copysign(res, x);
}

float tz_math_tanh_f32(float x) {
    return (float)tz_math_tanh_f64((double)x);
}

double tz_math_asinh_f64(double x) {
    static const double ln2 = 6.93147180559945309417232121458176568e-1;
    if (tz_f64_isnan(x) || tz_f64_isinf(x) || x == 0.0) return x;
    double ax = tz_math_fabs_f64(x);
    double res;
    if (ax < 0x1p-28) {
        res = ax;
    } else if (ax < 2.0) {
        res = tz_math_log1p_f64(ax + (ax * ax) / (1.0 + tz_math_sqrt_f64(1.0 + ax * ax)));
    } else if (ax < 0x1p28) {
        res = tz_math_log_f64(2.0 * ax + 1.0 / (ax + tz_math_sqrt_f64(ax * ax + 1.0)));
    } else {
        res = tz_math_log_f64(ax) + ln2;
    }
    return tz_f64_copysign(res, x);
}

float tz_math_asinh_f32(float x) {
    return (float)tz_math_asinh_f64((double)x);
}

double tz_math_acosh_f64(double x) {
    static const double ln2 = 6.93147180559945309417232121458176568e-1;
    if (tz_f64_isnan(x)) return x;
    if (x < 1.0) return (x - x) / (x - x);
    if (x == 1.0) return 0.0;
    if (tz_f64_isinf(x)) return x;
    if (x < 2.0) {
        double t = x - 1.0;
        return tz_math_log1p_f64(t + tz_math_sqrt_f64(2.0 * t + t * t));
    }
    if (x < 0x1p28) {
        return tz_math_log_f64(2.0 * x - 1.0 / (x + tz_math_sqrt_f64(x * x - 1.0)));
    }
    return tz_math_log_f64(x) + ln2;
}

float tz_math_acosh_f32(float x) {
    return (float)tz_math_acosh_f64((double)x);
}

double tz_math_atanh_f64(double x) {
    if (tz_f64_isnan(x) || x == 0.0) return x;
    double ax = tz_math_fabs_f64(x);
    if (ax > 1.0) return (x - x) / (x - x);
    if (ax == 1.0) return tz_f64_copysign(tz_bits_to_f64(UINT64_C(0x7ff0000000000000)), x);
    double res;
    if (ax < 0.5) {
        res = 0.5 * tz_math_log1p_f64(2.0 * ax + (2.0 * ax * ax) / (1.0 - ax));
    } else {
        res = 0.5 * tz_math_log1p_f64(2.0 * ax / (1.0 - ax));
    }
    return tz_f64_copysign(res, x);
}

float tz_math_atanh_f32(float x) {
    return (float)tz_math_atanh_f64((double)x);
}

/* logaddexp, logaddexp2, sinc, degrees, radians. */
double tz_math_logaddexp_f64(double x, double y) {
    static const double ln2 = 6.93147180559945309417232121458176568e-1;
    if (tz_f64_isnan(x) || tz_f64_isnan(y)) return x + y;
    if (x == y) {
        if (tz_f64_isinf(x)) return x;
        return x + ln2;
    }
    double hi = x > y ? x : y;
    double lo = x > y ? y : x;
    if (tz_f64_isinf(hi)) return hi;
    return hi + tz_math_log1p_f64(tz_math_exp_f64(lo - hi));
}

float tz_math_logaddexp_f32(float x, float y) {
    return (float)tz_math_logaddexp_f64((double)x, (double)y);
}

double tz_math_logaddexp2_f64(double x, double y) {
    static const double log2e = 1.44269504088896340735992468100189214e+0;
    if (tz_f64_isnan(x) || tz_f64_isnan(y)) return x + y;
    if (x == y) {
        if (tz_f64_isinf(x)) return x;
        return x + 1.0;
    }
    double hi = x > y ? x : y;
    double lo = x > y ? y : x;
    if (tz_f64_isinf(hi)) return hi;
    return hi + tz_math_log1p_f64(tz_math_exp2_f64(lo - hi)) * log2e;
}

float tz_math_logaddexp2_f32(float x, float y) {
    return (float)tz_math_logaddexp2_f64((double)x, (double)y);
}

/* Normalized sinc: sin(pi * x) / (pi * x), 1.0 at x == 0, 0.0 at non-zero integers and +-Inf. */
double tz_math_sinc_f64(double x) {
    static const double pi = 3.14159265358979323846264338327950288;
    if (tz_f64_isnan(x)) return x;
    if (x == 0.0) return 1.0;
    if (tz_f64_isinf(x)) return 0.0;
    double ax = tz_math_fabs_f64(x);
    if (ax >= 0x1p53) return 0.0;
    double r = tz_math_fmod_f64(ax, 2.0);
    double s;
    if (r == 0.0 || r == 1.0) {
        return 0.0;
    } else if (r < 0.5) {
        s = tz_math_sin_f64(pi * r);
    } else if (r < 1.5) {
        s = tz_math_sin_f64(pi * (1.0 - r));
    } else {
        s = -tz_math_sin_f64(pi * (2.0 - r));
    }
    return s / (pi * ax);
}

float tz_math_sinc_f32(float x) {
    return (float)tz_math_sinc_f64((double)x);
}

double tz_math_degrees_f64(double x) {
    return x * 57.29577951308232087679815481410517033;
}

float tz_math_degrees_f32(float x) {
    return (float)((double)x * 57.29577951308232087679815481410517033);
}

double tz_math_radians_f64(double x) {
    return x * 0.0174532925199432957692369076848861271;
}

float tz_math_radians_f32(float x) {
    return (float)((double)x * 0.0174532925199432957692369076848861271);
}

/* frexp decomposed into mantissa and exponent helpers (avoids pointer ABI in LLVM). */
double tz_math_frexp_mant_f64(double x) {
    uint64_t u = tz_f64_to_bits(x);
    int ee = (int)((u >> 52) & 0x7ff);
    if (ee == 0) {
        if (x == 0.0) return x;
        x *= 0x1p64;
        u = tz_f64_to_bits(x);
    } else if (ee == 0x7ff) {
        return x;
    }
    u = (u & UINT64_C(0x800fffffffffffff)) | UINT64_C(0x3fe0000000000000);
    return tz_bits_to_f64(u);
}

int32_t tz_math_frexp_exp_f64(double x) {
    uint64_t u = tz_f64_to_bits(x);
    int ee = (int)((u >> 52) & 0x7ff);
    if (ee == 0) {
        if (x == 0.0) return 0;
        x *= 0x1p64;
        u = tz_f64_to_bits(x);
        ee = (int)((u >> 52) & 0x7ff);
        return (int32_t)(ee - 0x3fe - 64);
    }
    if (ee == 0x7ff) return 0;
    return (int32_t)(ee - 0x3fe);
}

float tz_math_frexp_mant_f32(float x) {
    uint32_t u = tz_f32_to_bits(x);
    int ee = (int)((u >> 23) & 0xff);
    if (ee == 0) {
        if (x == 0.0f) return x;
        x *= 0x1p32f;
        u = tz_f32_to_bits(x);
    } else if (ee == 0xff) {
        return x;
    }
    u = (u & 0x807fffffu) | 0x3f000000u;
    return tz_bits_to_f32(u);
}

int32_t tz_math_frexp_exp_f32(float x) {
    uint32_t u = tz_f32_to_bits(x);
    int ee = (int)((u >> 23) & 0xff);
    if (ee == 0) {
        if (x == 0.0f) return 0;
        x *= 0x1p32f;
        u = tz_f32_to_bits(x);
        ee = (int)((u >> 23) & 0xff);
        return (int32_t)(ee - 0x7e - 32);
    }
    if (ee == 0xff) return 0;
    return (int32_t)(ee - 0x7e);
}

/* Exact IEEE 754 nextafter (musl algorithm). */
double tz_math_nextafter_f64(double x, double y) {
    if (tz_f64_isnan(x) || tz_f64_isnan(y)) return x + y;
    if (x == y) return y;
    uint64_t ux = tz_f64_to_bits(x);
    uint64_t ax = ux & UINT64_C(0x7fffffffffffffff);
    if (ax == 0) {
        return tz_bits_to_f64((tz_f64_to_bits(y) & UINT64_C(0x8000000000000000)) | UINT64_C(1));
    }
    uint64_t uy = tz_f64_to_bits(y);
    uint64_t ay = uy & UINT64_C(0x7fffffffffffffff);
    if ((ux >> 63) != (uy >> 63) || ax > ay) {
        ux--;
    } else {
        ux++;
    }
    return tz_bits_to_f64(ux);
}

float tz_math_nextafter_f32(float x, float y) {
    uint32_t ux = tz_f32_to_bits(x);
    uint32_t uy = tz_f32_to_bits(y);
    uint32_t ax = ux & 0x7fffffffu;
    uint32_t ay = uy & 0x7fffffffu;
    if (ax > 0x7f800000u || ay > 0x7f800000u) return x + y;
    if (x == y) return y;
    if (ax == 0) {
        return tz_bits_to_f32((uy & 0x80000000u) | 1u);
    }
    if ((ux >> 31) != (uy >> 31) || ax > ay) {
        ux--;
    } else {
        ux++;
    }
    return tz_bits_to_f32(ux);
}
