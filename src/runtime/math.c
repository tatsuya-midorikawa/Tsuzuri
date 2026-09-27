#include <math.h>

double sqrt(double value) { return __builtin_sqrt(value); }
float sqrtf(float value) { return __builtin_sqrtf(value); }
double fabs(double value) { return __builtin_fabs(value); }
float fabsf(float value) { return __builtin_fabsf(value); }

typedef struct { double high, low; } tz_math_dd;

static tz_math_dd dd_add(tz_math_dd left, tz_math_dd right) {
	double high = left.high + right.high;
	double virtual_right = high - left.high;
	double low = (left.high - (high - virtual_right)) + (right.high - virtual_right);
	low += left.low + right.low;
	double result = high + low;
	return (tz_math_dd){ result, low - (result - high) };
}

static tz_math_dd dd_negate(tz_math_dd value) { return (tz_math_dd){ -value.high, -value.low }; }

static tz_math_dd dd_multiply(tz_math_dd left, tz_math_dd right) {
	double high = left.high * right.high;
	double left_split = 0x1.0000002p27 * left.high;
	double right_split = 0x1.0000002p27 * right.high;
	double left_high = left_split - (left_split - left.high);
	double right_high = right_split - (right_split - right.high);
	double left_low = left.high - left_high;
	double right_low = right.high - right_high;
	double low = ((left_high * right_high - high) + left_high * right_low + left_low * right_high) + left_low * right_low;
	low += left.high * right.low + left.low * right.high;
	double result = high + low;
	return (tz_math_dd){ result, low - (result - high) };
}

static tz_math_dd dd_divide(tz_math_dd numerator, tz_math_dd denominator) {
	double quotient = numerator.high / denominator.high;
	tz_math_dd remainder = dd_add(numerator, dd_negate(dd_multiply((tz_math_dd){ quotient, 0 }, denominator)));
	return dd_add((tz_math_dd){ quotient, 0 }, (tz_math_dd){ (remainder.high + remainder.low) / denominator.high, 0 });
}

static const tz_math_dd atan_centers[] = {
	{0x0.0p+0, 0x0.0p+0},
	{0x1.f5b75f92c80ddp-3, 0x1.8ab6e3cf7afbdp-57},
	{0x1.dac670561bb4fp-2, 0x1.a2b7f222f65e2p-56},
	{0x1.4978fa3269ee1p-1, 0x1.2419a87f2a458p-56},
	{0x1.921fb54442d18p-1, 0x1.1a62633145c07p-55},
};
static const tz_math_dd atan_series[] = {
	{0x1.0000000000000p+0, 0x0.0p+0},
	{0x1.5555555555555p-2, 0x1.5555555555555p-56},
	{0x1.999999999999ap-3, -0x1.999999999999ap-57},
	{0x1.2492492492492p-3, 0x1.2492492492492p-57},
	{0x1.c71c71c71c71cp-4, 0x1.c71c71c71c71cp-58},
	{0x1.745d1745d1746p-4, -0x1.745d1745d1746p-59},
	{0x1.3b13b13b13b14p-4, -0x1.3b13b13b13b14p-58},
	{0x1.1111111111111p-4, 0x1.1111111111111p-60},
	{0x1.e1e1e1e1e1e1ep-5, 0x1.e1e1e1e1e1e1ep-61},
	{0x1.af286bca1af28p-5, 0x1.af286bca1af28p-59},
	{0x1.8618618618618p-5, 0x1.8618618618618p-59},
	{0x1.642c8590b2164p-5, 0x1.642c8590b2164p-60},
	{0x1.47ae147ae147bp-5, -0x1.eb851eb851eb8p-61},
	{0x1.2f684bda12f68p-5, 0x1.2f684bda12f68p-59},
	{0x1.1a7b9611a7b96p-5, 0x1.1a7b9611a7b96p-61},
	{0x1.0842108421084p-5, 0x1.0842108421084p-60},
};

double tz_math_atan2_f64(double vertical, double horizontal) {
	double first = fabs(vertical), second = fabs(horizontal);
	if (!isfinite(first) || !isfinite(second) || first == 0 || second == 0) return atan2(vertical, horizontal);
	int swapped = first > second;
	double numerator = swapped ? second : first;
	double denominator = swapped ? first : second;
	double approximate = numerator / denominator;
	if (approximate < 0x1p-27) return atan2(vertical, horizontal);
	union { double value; uint64_t bits; } encoded = { denominator };
	int exponent = (int)(encoded.bits >> 52) - 1023;
	numerator = scalbn(numerator, -exponent);
	denominator = scalbn(denominator, -exponent);
	tz_math_dd ratio = dd_divide((tz_math_dd){ numerator, 0 }, (tz_math_dd){ denominator, 0 });
	int index = (int)(approximate * 4 + 0.5);
	tz_math_dd center = { index * 0.25, 0 };
	tz_math_dd reduced = dd_divide(dd_add(ratio, dd_negate(center)), dd_add((tz_math_dd){ 1, 0 }, dd_multiply(ratio, center)));
	tz_math_dd square = dd_negate(dd_multiply(reduced, reduced));
	tz_math_dd polynomial = atan_series[15];
	for (int term = 14; term >= 0; --term) polynomial = dd_add(atan_series[term], dd_multiply(square, polynomial));
	tz_math_dd angle = dd_add(atan_centers[index], dd_multiply(reduced, polynomial));
	if (swapped) angle = dd_add((tz_math_dd){ 0x1.921fb54442d18p+0, 0x1.1a62633145c07p-54 }, dd_negate(angle));
	if (signbit(horizontal)) angle = dd_add((tz_math_dd){ 0x1.921fb54442d18p+1, 0x1.1a62633145c07p-53 }, dd_negate(angle));
	double result = angle.high + angle.low;
	return signbit(vertical) ? -result : result;
}