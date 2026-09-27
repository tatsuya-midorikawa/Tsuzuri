"""256-bit mpmath oracle for the portable f32/f64 elementary runtime."""
import json
import sys
import mpmath as mp

mp.mp.prec = 256


def decode(raw, bits):
    fraction, bias = (23, 127) if bits == 32 else (52, 1023)
    negative = bool(raw >> (bits - 1))
    exponent = (raw >> fraction) & (2 * bias + 1)
    coefficient = raw & ((1 << fraction) - 1)
    if exponent == 2 * bias + 1:
        value = mp.nan if coefficient else mp.inf
    else:
        if exponent:
            coefficient |= 1 << fraction
        value = mp.mpf(coefficient) * mp.power(2, (exponent or 1) - bias - fraction)
    return -value if negative else value, negative


def reference(name, first, second, first_negative, second_negative):
    negative_zero = first_negative and first == 0 and name in ("sin", "tan", "asin", "atan", "cbrt")
    if name == "pow":
        if second == 0 or first == 1:
            return mp.mpf(1), False
        if mp.isnan(first) or mp.isnan(second):
            return mp.nan, False
        if mp.isinf(second):
            if abs(first) == 1:
                return mp.mpf(1), False
            return (mp.inf if (abs(first) > 1) == (second > 0) else mp.mpf(0)), False
        integral = mp.isint(second)
        odd = integral and int(second) % 2 != 0
        negative = first_negative and odd
        if mp.isinf(first) or first == 0:
            infinite = (second > 0) == mp.isinf(first)
            return (-mp.inf if negative else mp.inf) if infinite else mp.mpf(0), negative and not infinite
        if first < 0 and not integral:
            return mp.nan, False
        exponent = second * mp.log(abs(first))
        if exponent > 2000:
            return (-mp.inf if negative else mp.inf), False
        if exponent < -2000:
            return mp.mpf(0), negative
        value = mp.power(abs(first), second)
        return -value if negative else value, negative
    if name == "hypot":
        if mp.isinf(first) or mp.isinf(second):
            return mp.inf, False
        return mp.sqrt(first * first + second * second), False
    if mp.isnan(first) or (name == "atan2" and mp.isnan(second)):
        return mp.nan, False
    if name == "atan2":
        if first == 0:
            return (-mp.pi if first_negative else mp.pi) if second_negative else mp.mpf(0), first_negative and not second_negative
        if mp.isinf(first) and mp.isinf(second):
            value = mp.pi * (3 if second_negative else 1) / 4
            return -value if first_negative else value, False
        if mp.isinf(second):
            return (-mp.pi if first_negative else mp.pi) if second_negative else mp.mpf(0), first_negative and not second_negative
        return mp.atan2(first, second), False
    if name in ("sin", "cos", "tan") and mp.isinf(first):
        return mp.nan, False
    if name in ("asin", "acos") and abs(first) > 1:
        return mp.nan, False
    if name in ("log", "log2", "log10"):
        if first < 0:
            return mp.nan, False
        if first == 0:
            return -mp.inf, False
        return mp.log(first) if name == "log" else mp.log(first, 2 if name == "log2" else 10), False
    if name in ("exp", "exp2"):
        if first > 2000:
            return mp.inf, False
        if first < -2000:
            return mp.mpf(0), False
        return mp.exp(first) if name == "exp" else mp.power(2, first), False
    if name == "cbrt":
        return (-1 if first < 0 else 1) * mp.root(abs(first), 3), negative_zero
    return getattr(mp, name)(first), negative_zero


def encode(value, bits, negative_zero):
    fraction, bias = (23, 127) if bits == 32 else (52, 1023)
    if mp.isnan(value):
        return [hex(((2 * bias + 1) << fraction) | (1 << (fraction - 1))), None, 0]
    if mp.isinf(value):
        return [hex(((2 * bias + 1) << fraction) | (int(value < 0) << (bits - 1))), None, 0]
    negative, coefficient, exponent, bit_count = value._mpf_
    negative = negative or (not coefficient and negative_zero)
    exact = -coefficient if negative else coefficient
    if not coefficient:
        return [hex(int(negative) << (bits - 1)), "0", 0]
    quantum = max(1 - bias - fraction, exponent + bit_count - 1 - fraction)
    discarded = quantum - exponent
    if discarded > 0:
        retained, remainder = divmod(coefficient, 1 << discarded)
        halfway = 1 << (discarded - 1)
        if remainder > halfway or (remainder == halfway and retained % 2):
            retained += 1
    else:
        retained = coefficient << -discarded
    if retained >= 1 << (fraction + 1):
        retained >>= 1
        quantum += 1
    encoded_exponent = quantum + fraction + bias if retained >= 1 << fraction else 0
    if encoded_exponent >= 2 * bias + 1:
        return [hex(((2 * bias + 1) << fraction) | (int(negative) << (bits - 1))), None, 0]
    raw = (int(negative) << (bits - 1)) | (encoded_exponent << fraction) | (retained & ((1 << fraction) - 1))
    return [hex(raw), str(exact), exponent]


for line in sys.stdin:
    operation, width, first_raw, second_raw = json.loads(line)
    first_value, first_sign = decode(int(first_raw, 16), width)
    second_value, second_sign = decode(int(second_raw, 16), width)
    result, zero_sign = reference(operation, first_value, second_value, first_sign, second_sign)
    print(json.dumps(encode(result, width, zero_sign), separators=(",", ":")))