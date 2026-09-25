"""Independent exact-interval formatting and single-rounding parse references."""
import decimal
from fractions import Fraction
from functools import lru_cache
import json
import random
import re
import sys

sys.set_int_max_str_digits(0)
rng = random.Random(314159)
BINARY = [(16, 10, 15), (32, 23, 127), (64, 52, 1023), (128, 112, 16383)]
DECIMAL = [(32, 7, -95, 96, 23, 101), (64, 16, -383, 384, 53, 398),
           (128, 34, -6143, 6144, 113, 6176)]


@lru_cache(maxsize=128)
def ten(exponent):
    return Fraction(10**exponent) if exponent >= 0 else Fraction(1, 10**-exponent)


def two(exponent):
    return Fraction(2**exponent) if exponent >= 0 else Fraction(1, 2**-exponent)


def render(coefficient, exponent, negative):
    if not coefficient:
        return "-0" if negative else "0"
    while coefficient % 10 == 0:
        coefficient //= 10
        exponent += 1
    digits = str(coefficient)
    adjusted = exponent + len(digits) - 1
    if adjusted < -6 or adjusted >= len(digits) + 6:
        text = digits[0] + ("." + digits[1:] if len(digits) > 1 else "")
        text += f"e{adjusted:+d}"
    else:
        point = len(digits) + exponent
        if point <= 0:
            text = "0." + "0" * -point + digits
        elif point >= len(digits):
            text = digits + "0" * (point - len(digits))
        else:
            text = digits[:point] + "." + digits[point:]
    return ("-" if negative else "") + text


def binary_value(raw, kind):
    _, fraction, bias = BINARY[kind]
    exponent = raw >> fraction
    coefficient = raw & ((1 << fraction) - 1)
    if exponent:
        coefficient += 1 << fraction
    return coefficient * two((exponent if exponent else 1) - bias - fraction)


def binary_text(raw, kind):
    width, fraction, bias = BINARY[kind]
    negative = bool(raw >> (width - 1))
    raw &= (1 << (width - 1)) - 1
    if raw >> fraction == 2 * bias + 1:
        return "nan" if raw & ((1 << fraction) - 1) else "-inf" if negative else "inf"
    if not raw:
        return "-0" if negative else "0"
    value = binary_value(raw, kind)
    previous = binary_value(raw - 1, kind)
    following = (binary_value(raw + 1, kind)
                 if raw + 1 < (2 * bias + 1) << fraction else value + value - previous)
    lower, upper = (previous + value) / 2, (value + following) / 2
    closed = raw % 2 == 0
    exponent = (value.numerator.bit_length() - value.denominator.bit_length()) * 30103 // 100000
    while value < ten(exponent):
        exponent -= 1
    while value >= ten(exponent + 1):
        exponent += 1
    # Search the interval of integer decimal coefficients, rather than
    # reproducing the runtime's digit-at-a-time remainder algorithm.
    for digits in range(1, 38):
        quantum = exponent - digits + 1
        scaled_lower, scaled_upper = lower / ten(quantum), upper / ten(quantum)
        lo = -(-scaled_lower.numerator // scaled_lower.denominator)
        hi = scaled_upper.numerator // scaled_upper.denominator
        if not closed and scaled_lower == lo:
            lo += 1
        if not closed and scaled_upper == hi:
            hi -= 1
        if lo <= hi:
            closest = min(hi, max(lo, round(value / ten(quantum))))
            return render(closest, quantum, negative)
    raise AssertionError((kind, raw))


def round_binary(value, negative, kind):
    width, fraction, bias = BINARY[kind]
    sign = int(negative) << (width - 1)
    if not value:
        return sign
    n, d = value.numerator, value.denominator
    exponent = n.bit_length() - d.bit_length()
    if value < two(exponent):
        exponent -= 1
    quantum = max(exponent - fraction, 1 - bias - fraction)
    coefficient = round(value / two(quantum))
    if coefficient >= 1 << (fraction + 1):
        coefficient //= 2
        quantum += 1
    if quantum > bias - fraction:
        return None
    encoded = quantum + fraction + bias if coefficient >= 1 << fraction else 0
    return sign | encoded << fraction | coefficient & ((1 << fraction) - 1)


def decimal_context(kind):
    _, precision, minimum, maximum, _, _ = DECIMAL[kind - 4]
    context = decimal.Context(prec=precision, Emin=minimum, Emax=maximum,
                              rounding=decimal.ROUND_HALF_EVEN, clamp=1)
    for trap in context.traps:
        context.traps[trap] = False
    return context


def decimal_bits(value, kind):
    width, _, _, _, fraction, bias = DECIMAL[kind - 4]
    sign = int(value.is_signed()) << (width - 1)
    if value.is_nan():
        return 31 << (width - 6)
    if value.is_infinite():
        return sign | 30 << (width - 6)
    _, digits, exponent = value.as_tuple()
    coefficient = int("".join(map(str, digits)))
    if width != 128 and coefficient >= 1 << fraction:
        return sign | 3 << (width - 3) | (exponent + bias) << (fraction - 2) | coefficient & ((1 << (fraction - 2)) - 1)
    return sign | (exponent + bias) << fraction | coefficient


DS = r"[0-9](?:_?[0-9])*"
FLOAT = re.compile(rf"[+-]?(?:{DS}(?:\.(?:{DS})?)?|\.(?:{DS}))(?:[eE][+-]?{DS})?")
INTEGER = re.compile(rf"[+-]?(?:{DS}|0x[0-9a-fA-F](?:_?[0-9a-fA-F])*|0b[01](?:_?[01])*)")


def parse(text, kind):
    if not text or len(text.encode()) > 4096:
        return None
    negative = text[0] == "-"
    plain = text.lstrip("+-")
    if kind >= 16:
        width, signed = 8 << ((kind - 16) & 7), kind < 24
        if (negative and not signed) or not INTEGER.fullmatch(text):
            return None
        value = int(plain.replace("_", ""), 16 if plain.startswith("0x") else 2 if plain.startswith("0b") else 10)
        if negative:
            value = -value
        if not (-(1 << (width - 1)) if signed else 0) <= value <= (1 << (width - int(signed))) - 1:
            return None
        return value % (1 << width)
    if text in [s + v for s in ["", "+", "-"] for v in ["inf", "nan"]]:
        if kind >= 4:
            return decimal_bits(decimal.Decimal(text), kind)
        width, fraction, bias = BINARY[kind]
        if plain == "nan":
            return (2 * bias + 1) << fraction | 1 << (fraction - 1)
        return int(negative) << (width - 1) | (2 * bias + 1) << fraction
    if not FLOAT.fullmatch(text):
        return None
    clean = text.replace("_", "")
    if kind >= 4:
        value = decimal_context(kind).create_decimal(clean)
        return None if value.is_infinite() else decimal_bits(value, kind)
    value = decimal.Decimal(clean)
    if value and value.adjusted() > 6000:
        return None
    if not value or value.adjusted() < -6000:
        return int(negative) << (BINARY[kind][0] - 1)
    return round_binary(abs(Fraction(value)), negative, kind)


formats, parses = [], []
for kind, (width, fraction, bias) in enumerate(BINARY):
    boundary = [0, 1, (1 << fraction) - 1, 1 << fraction,
                (bias << fraction) - 1, bias << fraction, (bias << fraction) + 1,
                ((2 * bias + 1) << fraction) - 1, (2 * bias + 1) << fraction,
                ((2 * bias + 1) << fraction) + 1]
    inputs = (range(65536) if kind == 0 else
              boundary + [n | 1 << (width - 1) for n in boundary] +
              [rng.getrandbits(width) for _ in range(2000 if kind == 3 else 10000)])
    for raw in inputs:
        text = binary_text(raw, kind)
        formats.append([kind, format(raw, "x"), text])
        expected = parse(text, kind)
        if text != "nan":
            assert expected == raw, (kind, raw, text, expected)
        parses.append([kind, text, None if expected is None else format(expected, "x")])

for kind in range(4, 7):
    width, precision, emin, emax, _, _ = DECIMAL[kind - 4]
    context = decimal_context(kind)
    texts = ["0", "-0", "0.10", "-1.000", "inf", "-inf", "nan",
             "9." + "9" * (precision - 1) + f"e{emax}", f"1e{emin - precision + 1}",
             "1." + "0" * (precision - 1) + "5", "1." + "0" * (precision - 2) + "15"]
    texts += [f"{rng.randrange(-10**precision + 1, 10**precision)}e{rng.randint(emin, emax - precision + 1)}"
              for _ in range(128)]
    for text in texts:
        value = context.create_decimal(text)
        raw = decimal_bits(value, kind)
        if value.is_nan():
            expected = "nan"
        elif value.is_infinite():
            expected = "-inf" if value.is_signed() else "inf"
        else:
            sign, digits, exponent = value.as_tuple()
            expected = render(int("".join(map(str, digits))), exponent, sign)
        formats.append([kind, format(raw, "x"), expected])
        parsed = parse(expected, kind)
        parses.append([kind, expected, format(parsed, "x")])

invalid = ["", " ", " 1", "1 ", "\t1", "1\n", "1\0", "++1", "--1", "+-1",
           "_1", "1_", "1__0", "1_.0", "1._0", "1e_1", "1e1_", ".", "1e", "1e+",
           "INF", "NaN", "0X10", "0B01", "0x_1", "0b2", "true", "\uff11",
           "0" * 4097]
for kind in list(range(7)) + list(range(16, 21)) + list(range(24, 29)):
    inputs = invalid + ["0", "-0", "+0", "+1", "-1", "0xFF", "-0x80", "0b10_10", "1_000",
                        ".5", "1.", "1.e+1", "1.2_5", "1e1_0", "inf", "-inf", "+nan", "-nan"]
    if kind >= 16:
        width, signed = 8 << ((kind - 16) & 7), kind < 24
        low, high = (-(1 << (width - 1)) if signed else 0), (1 << (width - int(signed))) - 1
        for number in sorted({low, low - 1, low + 1, 0, 1, high - 1, high, high + 1}):
            inputs.extend([str(number), ("-" if number < 0 else "+") + hex(abs(number)),
                           ("-" if number < 0 else "") + bin(abs(number))])
            if low <= number <= high:
                formats.append([kind, format(number % (1 << width), "x"), str(number)])
    else:
        inputs.extend(["1e100000", "-1e100000", "1e-100000", "-1e-100000",
                       "0e100000", "-0e-100000", "0" * 4095 + "1",
                       "9" * 4086 + "e-4085"])
        if kind < 4:
            _, fraction, _ = BINARY[kind]
            with decimal.localcontext() as context:
                context.prec = 200
                for n in [1, 3]:
                    value = Fraction(1) + Fraction(n, 2 ** (fraction + 1))
                    inputs.append(str(decimal.Decimal(value.numerator) / decimal.Decimal(value.denominator)))
        inputs.extend(f"{rng.randrange(-10**40, 10**40)}e{rng.randint(-7000, 7000)}"
                      for _ in range(48))
    for text in inputs:
        expected = parse(text, kind)
        parses.append([kind, text, None if expected is None else format(expected, "x")])

quad = "1.0000000000000000000000000000000002"
console = [
    ["0.1", "0.1"], ["0.1f32", "0.1"], ["-0.0", "-0"],
    ["1.0 / 0.0", "inf"], ["-1.0 / 0.0", "-inf"], ["0.0 / 0.0", "nan"],
    ["1000000.0", "1000000"], ["10000000.0", "1e+7"],
    [quad + "f128", binary_text(parse(quad, 3), 3)],
    ["0.10d128", "0.1"], ["-0.0d64", "-0"],
    ["-170141183460469231731687303715884105728i128", "-170141183460469231731687303715884105728"],
    ["true", "true"], ["false", "false"], ['"a\\0b"', "a\0b"],
]
json.dump({"formats": formats, "parses": parses, "console": console},
          sys.stdout, separators=(",", ":"))
