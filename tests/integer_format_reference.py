"""Independent expected results of integer formatting and parsing cases (PM08).

Prints {"cases": [[kind, raw, text], ...], "specs": [[kind, flags, raw, text], ...],
"parses": [[kind, units, raw or null], ...]}. `kind` is the numeric kind of src/llvm.rs: 16 + log2(bytes)
for a signed integer and 24 + log2(bytes) for an unsigned one. `raw` is the two's complement bit
pattern in lowercase hexadecimal, and `text` is Python's own text of the value, which no code of the
compiler produces. A spec's `flags` are the ones of a format hole: bit 0 asks for a "+" and bits 4 to 7
pick the form (3 hexadecimal, 4 uppercase hexadecimal, 5 octal, 6 binary, anything else decimal).
A parse's `units` are the UTF-16 code units of the text as hexadecimal groups of four digits, and the
expected `raw` is null when the text is not an integer of the type: an optional sign, then decimal
digits, "0x" and hexadecimal digits, or "0b" and binary digits, where a single "_" may separate two
digits, within the type's range and 4096 units.
"""
import json
import random
import re
import sys

sys.set_int_max_str_digits(0)
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


def kind_of(bits, signed):
    return (16 if signed else 24) + (bits // 8).bit_length() - 1


def range_of(bits, signed):
    return (-(1 << (bits - 1)) if signed else 0), (1 << (bits - int(signed))) - 1


def cases():
    generator = random.Random(20261016)
    result = []
    for bits, signed in TYPES:
        low, high = range_of(bits, signed)
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
            result.append([kind_of(bits, signed), format(value % (1 << bits), "x"), str(value)])
    return result


def spec_text(value, flags):
    style = (flags >> 4) & 15
    sign = "-" if value < 0 else "+" if flags & 1 else ""
    magnitude = abs(value)
    digits = {3: "x", 4: "X", 5: "o", 6: "b"}.get(style)
    return sign + (format(magnitude, digits) if digits else str(magnitude))


def specs():
    generator = random.Random(20261017)
    result = []
    for bits, signed in TYPES:
        low, high = range_of(bits, signed)
        values = sorted({candidate for value in boundaries(low, high, bits) for candidate in (value, -value)
                         if low <= candidate <= high})
        picked = set(values[:6] + values[-6:] + [v for v in values if -20 <= v <= 20] + values[::17])
        picked.update(generator.randint(low, high) for _ in range(40))
        for value in sorted(picked):
            for style in (0, 1, 2, 3, 4, 5, 6, 15):
                for plus in (0, 1):
                    flags = plus | style << 4
                    result.append([kind_of(bits, signed), flags, format(value % (1 << bits), "x"), spec_text(value, flags)])
    return result


DIGITS = r"[0-9](?:_?[0-9])*"
INTEGER = re.compile(rf"[+-]?(?:{DIGITS}|0x[0-9a-fA-F](?:_?[0-9a-fA-F])*|0b[01](?:_?[01])*)")


def units_of(text):
    data = text.encode("utf-16-le", "surrogatepass")
    return [int.from_bytes(data[index:index + 2], "little") for index in range(0, len(data), 2)]


def parse(text, kind):
    if not 0 < len(units_of(text)) <= 4096 or not INTEGER.fullmatch(text):
        return None
    bits, signed = 8 << ((kind - 16) & 7), kind < 24
    negative = text[0] == "-"
    if negative and not signed:
        return None
    plain = text.lstrip("+-")
    base = 16 if plain.startswith("0x") else 2 if plain.startswith("0b") else 10
    value = int(plain.replace("_", ""), base)
    if negative:
        value = -value
    low, high = range_of(bits, signed)
    return value % (1 << bits) if low <= value <= high else None


# Units whose low byte is a character of the syntax but that are not that character.
SYNTAX = "0123456789abcdefABCDEF_+-xb"
POOL = list(SYNTAX + "XB gz.e/:@G`\0\u00a0\u00b2\u0661\uff11\ud83d\U0001d7cf") + [
    chr(high + ord(c)) for c in SYNTAX for high in (0x100, 0x8000, 0xff00)]


def literal(generator, value, base, upper, density, zeros, plus):
    digits = format(abs(value), {10: "d", 16: "x", 2: "b"}[base])
    digits = "0" * zeros + (digits.upper() if upper else digits)
    spaced = digits[0]
    for digit in digits[1:]:
        spaced += "_" + digit if generator.random() < density else digit
    sign = "-" if value < 0 else "+" if plus else ""
    return sign + {10: "", 16: "0x", 2: "0b"}[base] + spaced


def mutate(generator, text):
    characters = list(text)
    for _ in range(generator.choice([1, 1, 2])):
        position = generator.randrange(len(characters) + 1)
        operation = generator.randrange(6)
        if operation == 0 or not characters:
            characters.insert(position, generator.choice(POOL))
            continue
        position = min(position, len(characters) - 1)
        if operation == 1:
            del characters[position]
        elif operation == 2:
            characters[position] = generator.choice(POOL)
        elif operation == 3 and position + 1 < len(characters):
            characters[position], characters[position + 1] = characters[position + 1], characters[position]
        elif operation == 4:
            characters.insert(position, characters[position])
        else:
            del characters[position:]
    return "".join(characters)


def parses():
    generator = random.Random(20261018)
    fixed = ["", "+", "-", "_1", "1_", "1__0", "+-1", "-+1", "++1", "0x", "0b", "-0x", "0x_1", "0xg", "0b2", "0X1",
             "0B1", " 1", "1 ", "1_000", "0x1_f", "0b1_0", "-0", "+0", "-0x0", "0_0", "00", "0x00", "0b0", "+0b1",
             "0x0x1", "00x1", "0b0b1", "0xAbCdEf", "-0b1000_0000", "1\u0000", "\u0661\u0662", "\uff11",
             "1:", "1/", "/1", ":1", "9:", "0/", "0x1g", "0x1G", "0xg1", "0x1@", "0x1`", "0x/", "0x:", "0b1:", "0b1/", "0xfg", "0xFG",
             "0" * 4095 + "1", "0" * 4096, "0" * 4097, "0x" + "0" * 4094, "0x" + "0" * 4095, "0b" + "0" * 4094,
             "0_" * 2047 + "0", "0_" * 2048, "1" + "0" * 4095, "-" + "0" * 4095, "-" + "0" * 4094 + "1", "+" + "1" * 4095]
    result = []

    def add(kind, text):
        expected = parse(text, kind)
        result.append([kind, "".join(f"{unit:04x}" for unit in units_of(text)),
                       None if expected is None else format(expected, "x")])

    for bits, signed in TYPES:
        kind = kind_of(bits, signed)
        low, high = range_of(bits, signed)
        for text in fixed:
            add(kind, text)
        edge = sorted({low - 1, low, low + 1, -1, 0, 1, 9, 10, 15, 16, 255, 256, high - 1, high, high + 1,
                       high // 10, high // 10 + 1, high // 16, high // 16 + 1, high // 2, high // 2 + 1})
        for value in edge:
            for base in (10, 16, 2):
                for upper in (False, True):
                    for plus in (False, True):
                        add(kind, literal(generator, value, base, upper, 0, 0, plus))
                    add(kind, literal(generator, value, base, upper, 1, 0, False))
                    add(kind, literal(generator, value, base, upper, 0.3, generator.randrange(3), True))
        for _ in range(3000):
            value = generator.randint(low - 2, high + 2) if generator.random() < 0.5 else (
                generator.getrandbits(generator.randint(1, bits + 1)) * generator.choice([-1, 1]))
            text = literal(generator, value, generator.choice([10, 10, 16, 2]), generator.random() < 0.5,
                           generator.choice([0, 0, 0.2]), generator.choice([0, 0, 1, 5]), generator.random() < 0.3)
            add(kind, text)
            if generator.random() < 0.7:
                add(kind, mutate(generator, text))
        for _ in range(1500):
            add(kind, "".join(generator.choice(POOL) for _ in range(generator.randint(0, 9))))
    return result


if __name__ == "__main__":
    print(json.dumps({"cases": cases(), "specs": specs(), "parses": parses()}, separators=(",", ":")))
