"""Independent expected text of integer formatting cases (PM08).

Prints {"cases": [[kind, raw, text], ...]}. `kind` is the numeric kind of src/llvm.rs: 16 + log2(bytes)
for a signed integer and 24 + log2(bytes) for an unsigned one. `raw` is the two's complement bit
pattern in lowercase hexadecimal, and `text` is Python's decimal text of the value, which no code of
the compiler produces.
"""
import json
import random

TYPES = [(bits, signed) for bits in (8, 16, 32, 64, 128) for signed in (True, False)]


def boundaries(low, high, bits):
    values = {low, low + 1, high - 1, high, 0, 1, 2, 3, 7, 8, 9, 10, 11, 42, 99, 100, 101, 127, 128, 255, 256}
    for exponent in range(0, 130):
        for base in (10 ** exponent, 2 ** exponent):
            for delta in (-1, 0, 1):
                values.add(base + delta)
    for digit in range(1, 10):
        for length in range(1, 40):
            values.add(digit * (10 ** length - 1) // 9)
    if bits == 128:
        limit = 10 ** 19
        for multiple in range(1, 5):
            for delta in (-2, -1, 0, 1, 2):
                values.add(multiple * limit + delta)
                values.add(multiple * limit * limit + delta)
                values.add(multiple * 2 ** 63 + delta)
                values.add(multiple * 2 ** 64 + delta)
        values.update({2 ** 64 - 1, 2 ** 64, 2 ** 64 + 1, 2 ** 127 - 1, 2 ** 127, 2 ** 128 - 1,
                       limit * limit - 1, limit * limit, limit * limit + 1, 3 * limit * limit - 1,
                       3 * limit * limit, 340282366920938463463374607431768211455 // 3})
    return values


def cases():
    generator = random.Random(20261016)
    result = []
    for bits, signed in TYPES:
        kind = (16 if signed else 24) + (bits // 8).bit_length() - 1
        low = -(1 << (bits - 1)) if signed else 0
        high = (1 << (bits - int(signed))) - 1
        values = set()
        for value in boundaries(low, high, bits):
            for candidate in (value, -value):
                if low <= candidate <= high:
                    values.add(candidate)
        for _ in range(3000):
            values.add(generator.randint(low, high))
            length = generator.randint(1, bits)
            magnitude = generator.getrandbits(length)
            candidate = -magnitude if signed and generator.getrandbits(1) else magnitude
            if low <= candidate <= high:
                values.add(candidate)
        for value in sorted(values):
            result.append([kind, format(value % (1 << bits), "x"), str(value)])
    return result


if __name__ == "__main__":
    print(json.dumps({"cases": cases()}, separators=(",", ":")))
