/* Portable round-to-nearest, ties-to-even arithmetic. No host floating-point
   operations are used, including for binary128 and BID decimal encodings. */
typedef unsigned int u32;
typedef unsigned long long u64;
typedef unsigned __int128 u128;

/* Enough for exact alignment of the most widely separated decimal128 values. */
#define LIMBS 1400
typedef struct { int n; u32 w[LIMBS]; } tzrt_big;
typedef struct { int base, precision, minimum, maximum, fraction, bias, width, integer, sign; } tzrt_format;
typedef struct { tzrt_big coefficient; int exponent, negative, special; } tzrt_number;

static tzrt_big small(u128 value) {
    tzrt_big result = {0};
    while (value) {
        result.w[result.n++] = (u32)value;
        value >>= 32;
    }
    return result;
}
static void normalize(tzrt_big *a) {
    while (a->n && !a->w[a->n - 1]) --a->n;
}
static int compare(const tzrt_big *a, const tzrt_big *b) {
    if (a->n != b->n) return a->n < b->n ? -1 : 1;
    for (int i = a->n; i--;) {
        if (a->w[i] != b->w[i]) return a->w[i] < b->w[i] ? -1 : 1;
    }
    return 0;
}
static int bits(const tzrt_big *a) {
    return a->n ? (a->n - 1) * 32 + 32 - __builtin_clz(a->w[a->n - 1]) : 0;
}
static void shift(tzrt_big *a, int count) {
    if (!a->n || !count) return;
    int words = count / 32, rest = count % 32, n = a->n;
    if (n + words + 1 > LIMBS) __builtin_trap();
    for (int i = n; i--;) a->w[i + words] = a->w[i];
    for (int i = 0; i < words; ++i) a->w[i] = 0;
    a->n += words;
    u64 carry = 0;
    for (int i = words; i < a->n; ++i) {
        u64 value = ((u64)a->w[i] << rest) | carry;
        a->w[i] = (u32)value;
        carry = value >> 32;
    }
    if (carry) a->w[a->n++] = (u32)carry;
}
static void shift_down(tzrt_big *a) {
    u32 carry = 0;
    for (int i = a->n; i--;) {
        u32 word = a->w[i];
        a->w[i] = (word >> 1) | carry;
        carry = word << 31;
    }
    normalize(a);
}
static void multiply_small(tzrt_big *a, u32 b) {
    u64 carry = 0;
    for (int i = 0; i < a->n; ++i) {
        u64 value = (u64)a->w[i] * b + carry;
        a->w[i] = (u32)value;
        carry = value >> 32;
    }
    if (carry) {
        if (a->n == LIMBS) __builtin_trap();
        a->w[a->n++] = (u32)carry;
    }
}
static void power(tzrt_big *a, int base, int exponent) {
    if (base == 2) { shift(a, exponent); return; }
    while (exponent >= 9) { multiply_small(a, 1000000000); exponent -= 9; }
    while (exponent-- > 0) multiply_small(a, 10);
}
static void add(tzrt_big *a, const tzrt_big *b) {
    int old = a->n;
    if (a->n < b->n) a->n = b->n;
    u64 carry = 0;
    for (int i = 0; i < a->n; ++i) {
        u64 value = (i < old ? (u64)a->w[i] : 0) + (i < b->n ? b->w[i] : 0) + carry;
        a->w[i] = (u32)value;
        carry = value >> 32;
    }
    if (carry) {
        if (a->n == LIMBS) __builtin_trap();
        a->w[a->n++] = (u32)carry;
    }
}
static void subtract(tzrt_big *a, const tzrt_big *b) {
    u64 borrow = 0;
    for (int i = 0; i < a->n; ++i) {
        u64 sub = (i < b->n ? (u64)b->w[i] : 0) + borrow;
        u32 word = a->w[i];
        a->w[i] = (u32)((u64)word - sub);
        borrow = (u64)word < sub;
    }
    normalize(a);
}
static tzrt_big multiply(const tzrt_big *a, const tzrt_big *b) {
    tzrt_big result = {0};
    if (a->n + b->n > LIMBS) __builtin_trap();
    result.n = a->n + b->n;
    for (int i = 0; i < a->n; ++i) {
        u64 carry = 0;
        for (int j = 0; j < b->n; ++j) {
            u64 value = (u64)a->w[i] * b->w[j] + result.w[i + j] + carry;
            result.w[i + j] = (u32)value;
            carry = value >> 32;
        }
        result.w[i + b->n] = (u32)carry;
    }
    normalize(&result);
    return result;
}
static tzrt_big divide(tzrt_big *remainder, const tzrt_big *denominator) {
    tzrt_big quotient = {0}, divisor = *denominator;
    if (!denominator->n) __builtin_trap();
    int distance = bits(remainder) - bits(denominator);
    if (distance < 0) return quotient;
    shift(&divisor, distance);
    quotient.n = distance / 32 + 1;
    for (int i = distance; i >= 0; --i) {
        if (compare(remainder, &divisor) >= 0) {
            subtract(remainder, &divisor);
            quotient.w[i / 32] |= (u32)1 << (i % 32);
        }
        shift_down(&divisor);
    }
    normalize(&quotient);
    return quotient;
}
static u32 divide_small(tzrt_big *a, u32 divisor) {
    u64 remainder = 0;
    for (int i = a->n; i--;) {
        u64 value = (remainder << 32) | a->w[i];
        a->w[i] = (u32)(value / divisor);
        remainder = value % divisor;
    }
    normalize(a);
    return (u32)remainder;
}
static u128 narrow(const tzrt_big *a) {
    if (a->n > 4) __builtin_trap();
    u128 value = 0;
    for (int i = a->n; i--;) value = (value << 32) | a->w[i];
    return value;
}
static tzrt_format format(int kind) {
    switch (kind) {
        case 0: return (tzrt_format){2, 11, -24, 5, 10, 15, 16, 0, 1};
        case 1: return (tzrt_format){2, 24, -149, 104, 23, 127, 32, 0, 1};
        case 2: return (tzrt_format){2, 53, -1074, 971, 52, 1023, 64, 0, 1};
        case 3: return (tzrt_format){2, 113, -16494, 16271, 112, 16383, 128, 0, 1};
        case 4: return (tzrt_format){10, 7, -101, 90, 23, 101, 32, 0, 1};
        case 5: return (tzrt_format){10, 16, -398, 369, 53, 398, 64, 0, 1};
        case 6: return (tzrt_format){10, 34, -6176, 6111, 113, 6176, 128, 0, 1};
        default: {
            int width = 8 << ((kind - 16) & 7);
            return (tzrt_format){2, width, 0, 0, 0, 0, width, 1, kind < 24};
        }
    }
}
static u128 mask(int count) { return count == 128 ? ~(u128)0 : ((u128)1 << count) - 1; }
static u128 load(const unsigned char *p, int width) {
#if __BYTE_ORDER__ == __ORDER_LITTLE_ENDIAN__
    switch (width) {
        case 8: return *p;
        case 16: {
            unsigned short raw;
            __builtin_memcpy(&raw, p, sizeof(raw));
            return raw;
        }
        case 32: {
            u32 raw;
            __builtin_memcpy(&raw, p, sizeof(raw));
            return raw;
        }
        case 64: {
            u64 raw;
            __builtin_memcpy(&raw, p, sizeof(raw));
            return raw;
        }
        case 128: {
            u128 raw;
            __builtin_memcpy(&raw, p, sizeof(raw));
            return raw;
        }
    }
#endif
    u128 value = 0;
    for (int i = width / 8; i--;) value = (value << 8) | p[i];
    return value;
}
static void store(unsigned char *p, u128 value, int width) {
#if __BYTE_ORDER__ == __ORDER_LITTLE_ENDIAN__
    switch (width) {
        case 8: *p = (unsigned char)value; return;
        case 16: {
            unsigned short raw = (unsigned short)value;
            __builtin_memcpy(p, &raw, sizeof(raw));
            return;
        }
        case 32: {
            u32 raw = (u32)value;
            __builtin_memcpy(p, &raw, sizeof(raw));
            return;
        }
        case 64: {
            u64 raw = (u64)value;
            __builtin_memcpy(p, &raw, sizeof(raw));
            return;
        }
        case 128: __builtin_memcpy(p, &value, sizeof(value)); return;
    }
#endif
    for (int i = 0; i < width / 8; ++i) { p[i] = (unsigned char)value; value >>= 8; }
}
static tzrt_number decode(const unsigned char *p, tzrt_format f) {
    u128 raw = load(p, f.width);
    tzrt_number result = {0};
    result.negative = (int)(raw >> (f.width - 1));
    if (f.integer) {
        result.negative &= f.sign;
        result.coefficient = small(result.negative ? (-raw & mask(f.width)) : raw);
        return result;
    }
    raw &= mask(f.width - 1);
    if (f.base == 2) {
        int exponent = (int)(raw >> f.fraction);
        u128 coefficient = raw & mask(f.fraction);
        if (exponent == f.bias * 2 + 1) {
            result.special = coefficient ? 2 : 1;
        } else {
            result.exponent = exponent ? exponent - f.bias - f.fraction : f.minimum;
            result.coefficient = small(coefficient | (exponent ? (u128)1 << f.fraction : 0));
        }
    } else {
        int combination = (int)(raw >> (f.width - 6));
        if (combination >= 30) {
            result.special = combination == 30 ? 1 : 2;
        } else {
            u128 coefficient;
            int exponent;
            if ((raw >> (f.width - 3)) == 3 && f.width != 128) {
                exponent = (int)((raw >> (f.fraction - 2)) & mask(f.width - f.fraction - 1));
                coefficient = (raw & mask(f.fraction - 2)) | ((u128)1 << f.fraction);
            } else {
                exponent = (int)(raw >> f.fraction);
                coefficient = raw & mask(f.fraction);
            }
            result.coefficient = small(coefficient);
            tzrt_big limit = small(1);
            power(&limit, 10, f.precision);
            if (compare(&result.coefficient, &limit) >= 0 || (f.width == 128 && (raw >> 125) == 3)) result.coefficient = small(0);
            result.exponent = exponent - f.bias;
        }
    }
    return result;
}
static int compare_power(const tzrt_big *n, const tzrt_big *d, int base, int exponent) {
    tzrt_big scaled = exponent >= 0 ? *d : *n;
    power(&scaled, base, exponent >= 0 ? exponent : -exponent);
    return exponent >= 0 ? compare(n, &scaled) : compare(&scaled, d);
}
static int magnitude(const tzrt_big *n, const tzrt_big *d, int base) {
    int exponent = bits(n) - bits(d);
    if (base == 10) exponent = exponent * 30103 / 100000;
    while (compare_power(n, d, base, exponent) < 0) --exponent;
    while (compare_power(n, d, base, exponent + 1) >= 0) ++exponent;
    return exponent;
}
static tzrt_big rounded(const tzrt_big *n, const tzrt_big *d, int base, int scale) {
    tzrt_big numerator = *n, denominator = *d;
    if (scale >= 0) power(&numerator, base, scale);
    else power(&denominator, base, -scale);
    tzrt_big result = divide(&numerator, &denominator);
    multiply_small(&numerator, 2);
    int order = compare(&numerator, &denominator);
    if (order > 0 || (order == 0 && result.n && (result.w[0] & 1))) {
        tzrt_big one = small(1);
        add(&result, &one);
    }
    return result;
}
static u128 special(tzrt_format f, int kind, int negative) {
    u128 value = (u128)negative << (f.width - 1);
    if (f.base == 2) {
        value |= (u128)(f.bias * 2 + 1) << f.fraction;
        if (kind == 2) value |= (u128)1 << (f.fraction - 1);
    } else {
        value |= (u128)(kind == 2 ? 31 : 30) << (f.width - 6);
    }
    return value;
}
static u128 pack(const tzrt_big *n, const tzrt_big *d, int exponent, int negative, tzrt_format f) {
    u128 sign = (u128)negative << (f.width - 1);
    if (!n->n) {
        int quantum = exponent < f.minimum ? f.minimum : exponent > f.maximum ? f.maximum : exponent;
        return sign | (f.base == 10 ? (u128)(quantum + f.bias) << f.fraction : 0);
    }
    int quantum = magnitude(n, d, f.base) + exponent - (f.precision - 1);
    if (quantum < f.minimum) quantum = f.minimum;
    if (quantum > f.maximum) return special(f, 1, negative);
    tzrt_big coefficient = rounded(n, d, f.base, exponent - quantum);
    tzrt_big limit = small(1);
    power(&limit, f.base, f.precision);
    if (compare(&coefficient, &limit) >= 0) {
        divide_small(&coefficient, f.base);
        if (++quantum > f.maximum) return special(f, 1, negative);
    }
    if (f.base == 10) {
        while (quantum < exponent && quantum < f.maximum && coefficient.n) {
            tzrt_big shortened = coefficient;
            if (divide_small(&shortened, 10)) break;
            coefficient = shortened;
            ++quantum;
        }
    }
    u128 value = narrow(&coefficient);
    if (f.base == 2) {
        int encoded = value >> f.fraction ? quantum + f.fraction + f.bias : 0;
        return sign | ((u128)encoded << f.fraction) | (value & mask(f.fraction));
    }
    int encoded = quantum + f.bias;
    if (f.width != 128 && value >> f.fraction) {
        return sign | ((u128)3 << (f.width - 3)) | ((u128)encoded << (f.fraction - 2)) | (value & mask(f.fraction - 2));
    }
    return sign | ((u128)encoded << f.fraction) | value;
}

__attribute__((visibility("hidden")))
void tz_soft_op(unsigned char *out, const unsigned char *left, const unsigned char *right, int kind, int op) {
    tzrt_format f = format(kind);
    tzrt_number a = decode(left, f), b = decode(right, f);
    if (op == 1) b.negative ^= 1;
    int negative = op < 2 ? a.negative : a.negative ^ b.negative;
    int exceptional = 0;
    if (a.special == 2 || b.special == 2) exceptional = 2;
    else if (op < 2 && (a.special || b.special)) {
        exceptional = a.special && b.special && a.negative != b.negative ? 2 : 1;
        negative = a.special ? a.negative : b.negative;
    } else if (op == 2 && (a.special || b.special)) {
        exceptional = (!a.special && !a.coefficient.n) || (!b.special && !b.coefficient.n) ? 2 : 1;
    } else if (op == 3) {
        if (a.special && b.special) exceptional = 2;
        else if (a.special) exceptional = 1;
        else if (b.special) { a.coefficient = small(0); b.coefficient = small(1); }
        else if (!b.coefficient.n) exceptional = a.coefficient.n ? 1 : 2;
    }
    if (exceptional) { store(out, special(f, exceptional, negative), f.width); return; }
    tzrt_big numerator = a.coefficient, denominator = small(1);
    int exponent;
    if (op < 2) {
        exponent = a.exponent < b.exponent ? a.exponent : b.exponent;
        power(&numerator, f.base, a.exponent - exponent);
        power(&b.coefficient, f.base, b.exponent - exponent);
        if (a.negative == b.negative) add(&numerator, &b.coefficient);
        else {
            int order = compare(&numerator, &b.coefficient);
            if (order >= 0) subtract(&numerator, &b.coefficient);
            else { subtract(&b.coefficient, &numerator); numerator = b.coefficient; negative = b.negative; }
            if (!numerator.n) negative = 0;
        }
    } else if (op == 2) {
        numerator = multiply(&a.coefficient, &b.coefficient);
        exponent = a.exponent + b.exponent;
    } else {
        denominator = b.coefficient;
        exponent = a.exponent - b.exponent;
    }
    store(out, pack(&numerator, &denominator, exponent, negative, f), f.width);
}

__attribute__((visibility("hidden")))
int tz_soft_cmp(const unsigned char *left, const unsigned char *right, int kind) {
    tzrt_format f = format(kind);
    tzrt_number a = decode(left, f), b = decode(right, f);
    if (a.special == 2 || b.special == 2) return 2;
    if (!a.special && !b.special && !a.coefficient.n && !b.coefficient.n) return 0;
    if (a.negative != b.negative) return a.negative ? -1 : 1;
    int order;
    if (a.special || b.special) order = (a.special > b.special) - (a.special < b.special);
    else {
        int exponent = a.exponent < b.exponent ? a.exponent : b.exponent;
        power(&a.coefficient, f.base, a.exponent - exponent);
        power(&b.coefficient, f.base, b.exponent - exponent);
        order = compare(&a.coefficient, &b.coefficient);
    }
    return a.negative ? -order : order;
}

__attribute__((visibility("hidden")))
void tz_soft_cast(unsigned char *out, const unsigned char *input, int from, int to) {
    tzrt_format source = format(from), target = format(to);
    tzrt_number value = decode(input, source);
    if (target.integer) {
        u128 maximum = mask(target.width - target.sign);
        if (target.sign && value.negative) maximum += 1;
        if ((!target.sign && value.negative) || value.special == 2) maximum = 0;
        if (value.special) { store(out, value.negative ? -maximum : maximum, target.width); return; }
        tzrt_big numerator = value.coefficient, denominator = small(1), limit = small(maximum);
        if (value.exponent >= 0) power(&numerator, source.base, value.exponent);
        else power(&denominator, source.base, -value.exponent);
        limit = multiply(&limit, &denominator);
        u128 result;
        if (compare(&numerator, &limit) >= 0) result = maximum;
        else { tzrt_big quotient = divide(&numerator, &denominator); result = narrow(&quotient); }
        store(out, value.negative ? -result : result, target.width);
        return;
    }
    if (value.special) { store(out, special(target, value.special, value.negative), target.width); return; }
    tzrt_big denominator = small(1);
    if (source.base != target.base) {
        if (value.exponent >= 0) power(&value.coefficient, source.base, value.exponent);
        else power(&denominator, source.base, -value.exponent);
        value.exponent = 0;
    }
    store(out, pack(&value.coefficient, &denominator, value.exponent, value.negative, target), target.width);
}

static int unsigned_text(char *out, u32 value) {
    char reversed[12];
    int count = 0;
    do { reversed[count++] = (char)('0' + value % 10); value /= 10; } while (value);
    for (int i = 0; i < count; ++i) out[i] = reversed[count - i - 1];
    return count;
}

/* Dragon4: generate digits until the remainder enters a rounding boundary.
   Powers of two (except the smallest normal) have a closer lower neighbor. */
static void shortest(tzrt_number *value, tzrt_format f) {
    u128 significand = narrow(&value->coefficient);
    int closed = !(significand & 1);
    tzrt_big numerator = small(significand << 2), denominator = small(4);
    tzrt_big lower = small(significand == ((u128)1 << f.fraction) && value->exponent > f.minimum ? 1 : 2);
    tzrt_big upper = small(2);
    if (value->exponent >= 0) {
        shift(&numerator, value->exponent);
        shift(&lower, value->exponent);
        shift(&upper, value->exponent);
    } else shift(&denominator, -value->exponent);
    int exponent = magnitude(&numerator, &denominator, 10);
    if (exponent >= 0) power(&denominator, 10, exponent);
    else {
        power(&numerator, 10, -exponent);
        power(&lower, 10, -exponent);
        power(&upper, 10, -exponent);
    }
    u128 coefficient = 0;
    for (int digits = 1; ; ++digits) {
        u32 digit = 0;
        while (compare(&numerator, &denominator) >= 0) {
            subtract(&numerator, &denominator);
            ++digit;
        }
        coefficient = coefficient * 10 + digit;
        int low_order = compare(&numerator, &lower);
        tzrt_big gap = denominator;
        subtract(&gap, &numerator);
        int high_order = compare(&gap, &upper);
        int low = low_order < 0 || (closed && !low_order);
        int high = high_order < 0 || (closed && !high_order);
        if (low || high) {
            multiply_small(&numerator, 2);
            int nearest = compare(&numerator, &denominator);
            if (high && (!low || nearest > 0 || (!nearest && (coefficient & 1)))) ++coefficient;
            value->coefficient = small(coefficient);
            value->exponent = exponent - digits + 1;
            return;
        }
        multiply_small(&numerator, 10);
        multiply_small(&lower, 10);
        multiply_small(&upper, 10);
    }
}

static __attribute__((noinline)) int format_float(char *out, const unsigned char *input, tzrt_format f);

/* The caller supplies 128 bytes. No locale, host FP, or allocation is used. */
__attribute__((visibility("hidden")))
int tz_soft_format(char *out, const unsigned char *input, int kind) {
    tzrt_format f = format(kind);
    if (f.integer) {
        u128 value = load(input, f.width);
        int negative = f.sign && (value >> (f.width - 1));
        if (negative) value = -value & mask(f.width);
        char digits[40];
        int count = 0, length = 0;
        while (value >> 64) {
            u32 pair = (u32)(value % 100);
            digits[count++] = (char)('0' + pair % 10);
            digits[count++] = (char)('0' + pair / 10);
            value /= 100;
        }
        u64 remaining = (u64)value;
        while (remaining >= 100) {
            u32 pair = (u32)(remaining % 100);
            digits[count++] = (char)('0' + pair % 10);
            digits[count++] = (char)('0' + pair / 10);
            remaining /= 100;
        }
        if (remaining >= 10) {
            digits[count++] = (char)('0' + (u32)remaining % 10);
            digits[count++] = (char)('0' + (u32)remaining / 10);
        } else digits[count++] = (char)('0' + remaining);
        if (negative) out[length++] = '-';
        while (count) out[length++] = digits[--count];
        return length;
    }
    if (f.base == 2) {
        u128 raw = load(input, f.width);
        if (!(raw & mask(f.width - 1))) {
            int length = 0;
            if (raw) out[length++] = '-';
            out[length++] = '0';
            return length;
        }
    }
    return format_float(out, input, f);
}

static __attribute__((noinline)) int format_float(char *out, const unsigned char *input, tzrt_format f) {
    tzrt_number value = decode(input, f);
    int length = 0;
    if (value.negative && value.special != 2) out[length++] = '-';
    if (value.special) {
        const char *text = value.special == 2 ? "nan" : "inf";
        for (int i = 0; i < 3; ++i) out[length++] = text[i];
        return length;
    }
    if (!value.coefficient.n) { out[length++] = '0'; return length; }
    if (f.base == 2) shortest(&value, f);
    char digits[48];
    int count = 0;
    do { digits[count++] = (char)('0' + divide_small(&value.coefficient, 10)); } while (value.coefficient.n);
    int first = 0;
    if (!f.integer) {
        while (first + 1 < count && digits[first] == '0') { ++first; ++value.exponent; }
    }
    int significant = count - first;
    int exponent = value.exponent + significant - 1;
    if (f.integer || (exponent >= -6 && exponent < significant + 6)) {
        int point = significant + value.exponent;
        if (point <= 0) {
            out[length++] = '0'; out[length++] = '.';
            for (int i = 0; i < -point; ++i) out[length++] = '0';
        }
        for (int i = count - 1; i >= first; --i) {
            out[length++] = digits[i];
            if (--point == 0 && i > first) out[length++] = '.';
        }
        while (point-- > 0) out[length++] = '0';
    } else {
        out[length++] = digits[count - 1];
        if (significant > 1) out[length++] = '.';
        for (int i = count - 2; i >= first; --i) out[length++] = digits[i];
        out[length++] = 'e';
        out[length++] = exponent < 0 ? '-' : '+';
        length += unsigned_text(out + length, (u32)(exponent < 0 ? -exponent : exponent));
    }
    return length;
}

static int digit_value(char c) {
    if (c >= '0' && c <= '9') return c - '0';
    if (c >= 'a' && c <= 'f') return c - 'a' + 10;
    if (c >= 'A' && c <= 'F') return c - 'A' + 10;
    return -1;
}

static int eight_decimal_digits(const char *input, u32 *result) {
    u64 packed;
    __builtin_memcpy(&packed, input, sizeof(packed));
#if __BYTE_ORDER__ == __ORDER_BIG_ENDIAN__
    packed = __builtin_bswap64(packed);
#endif
    u64 digits = packed - 0x3030303030303030ULL;
    if ((digits | (packed + 0x4646464646464646ULL)) & 0x8080808080808080ULL) return 0;
    digits = (digits * 10 + (digits >> 8)) & 0x00ff00ff00ff00ffULL;
    digits = (digits * 100 + (digits >> 16)) & 0x0000ffff0000ffffULL;
    *result = (u32)(digits * 10000 + (digits >> 32));
    return 1;
}

/* A null coefficient parses an exponent, saturating well beyond any format's
   range. Separators must have decimal digits on both sides in this section. */
static int decimal_digits(const char *input, u64 length, u64 *index, tzrt_big *coefficient, int *exponent) {
    int count = 0;
    while (*index < length) {
        char c = input[*index];
        if (c == '_') {
            if (!count || *index + 1 == length || input[*index + 1] < '0' || input[*index + 1] > '9') return -1;
            ++*index;
            continue;
        }
        if (c < '0' || c > '9') break;
        u32 digit = (u32)(c - '0');
        if (coefficient) {
            multiply_small(coefficient, 10);
            u64 carry = digit;
            for (int i = 0; carry; ++i) {
                if (i == coefficient->n) coefficient->w[coefficient->n++] = 0;
                carry += coefficient->w[i];
                coefficient->w[i] = (u32)carry;
                carry >>= 32;
            }
        } else if (*exponent < 100000) {
            *exponent = *exponent * 10 + (int)digit;
        }
        ++count;
        ++*index;
    }
    return count;
}

static __attribute__((noinline)) int parse_float(unsigned char *out, const char *input, u64 length, u64 index, int negative, tzrt_format f);

__attribute__((visibility("hidden")))
int tz_soft_parse(unsigned char *out, const char *input, u64 length, int kind) {
    if (!length || length > 4096) return 0;
    tzrt_format f = format(kind);
    u64 index = 0;
    int negative = input[0] == '-';
    if (negative || input[0] == '+') ++index;
    if (index == length) return 0;
    if (f.integer) {
        if (negative && !f.sign) return 0;
        int base = 10;
        if (index + 1 < length && input[index] == '0') {
            if (input[index + 1] == 'x') { base = 16; index += 2; }
            else if (input[index + 1] == 'b') { base = 2; index += 2; }
        }
        if (index == length) return 0;
        u128 limit = mask(f.width - f.sign) + (f.sign && negative);
        u128 cutoff = f.width <= 64 ? (u64)limit / (u32)base : limit / (u32)base;
        u32 last = f.width <= 64 ? (u32)((u64)limit % (u32)base) : (u32)(limit % (u32)base);
        u128 value = 0;
        int previous = 0;
        u32 group;
        if (base == 10 && length - index >= 8 && eight_decimal_digits(input + index, &group)) {
            u128 group_cutoff = f.width <= 64 ? (u64)limit / 100000000 : limit / 100000000;
            u32 group_last = f.width <= 64 ? (u32)((u64)limit % 100000000) : (u32)(limit % 100000000);
            do {
                if (value > group_cutoff || (value == group_cutoff && group > group_last)) return 0;
                value = value * 100000000 + group;
                previous = 1;
                index += 8;
            } while (length - index >= 8 && eight_decimal_digits(input + index, &group));
        }
        for (; index < length; ++index) {
            if (input[index] == '_') {
                if (!previous || index + 1 == length) return 0;
                previous = 0;
                continue;
            }
            int digit = digit_value(input[index]);
            if (digit < 0 || digit >= base || value > cutoff || (value == cutoff && (u32)digit > last)) return 0;
            value = value * (u32)base + (u32)digit;
            previous = 1;
        }
        store(out, negative ? -value : value, f.width);
        return 1;
    }
    if (length - index == 3) {
        const char *p = input + index;
        if (p[0] == 'i' && p[1] == 'n' && p[2] == 'f') {
            store(out, special(f, 1, negative), f.width);
            return 1;
        }
        if (p[0] == 'n' && p[1] == 'a' && p[2] == 'n') {
            store(out, special(f, 2, 0), f.width);
            return 1;
        }
    }
    return parse_float(out, input, length, index, negative, f);
}

static __attribute__((noinline)) int parse_float(unsigned char *out, const char *input, u64 length, u64 index, int negative, tzrt_format f) {
    tzrt_big coefficient = small(0), denominator = small(1);
    int integral = decimal_digits(input, length, &index, &coefficient, 0);
    if (integral < 0) return 0;
    int fractional = 0;
    if (index < length && input[index] == '.') {
        ++index;
        fractional = decimal_digits(input, length, &index, &coefficient, 0);
        if (fractional < 0) return 0;
    }
    if (!integral && !fractional) return 0;
    int exponent = 0;
    if (index < length && (input[index] == 'e' || input[index] == 'E')) {
        ++index;
        int sign = index < length && input[index] == '-';
        if (index < length && (sign || input[index] == '+')) ++index;
        if (decimal_digits(input, length, &index, 0, &exponent) <= 0) return 0;
        if (sign) exponent = -exponent;
    }
    if (index != length) return 0;
    exponent -= fractional;
    if (!coefficient.n) {
        store(out, pack(&coefficient, &denominator, exponent, negative, f), f.width);
        return 1;
    }
    int adjusted = magnitude(&coefficient, &denominator, 10) + exponent;
    int maximum = f.base == 10 ? f.maximum + f.precision - 1 : (f.maximum + f.precision) * 30103 / 100000 + 1;
    int minimum = f.base == 10 ? f.minimum - 1 : f.minimum * 30103 / 100000 - 2;
    if (adjusted > maximum) return 0;
    if (adjusted < minimum) {
        coefficient = small(0);
        exponent = f.minimum;
    } else if (f.base == 2) {
        if (exponent >= 0) power(&coefficient, 10, exponent);
        else power(&denominator, 10, -exponent);
        exponent = 0;
    }
    /* At most 4096 input digits plus the target's exponent range fit LIMBS.
       Classifying extreme exponents first keeps malformed input trap-free. */
    u128 raw = pack(&coefficient, &denominator, exponent, negative, f);
    if ((raw & mask(f.width - 1)) == special(f, 1, 0)) return 0;
    store(out, raw, f.width);
    return 1;
}
