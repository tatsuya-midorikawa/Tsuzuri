"""PM03: how scripts/lldb/tsuzuri_lldb.py reads a `string` of either form, without a live LLDB session.

A `string` is UTF-16 (the length word is the count) or Latin-1 (bit 62 of the word is set, the low bits
count the bytes; `TSUZURI_STRINGS=compact`), and no word has bit 63 set. The formatter reads values
through LLDB's `SBValue` and `SBProcess`; stand-ins for the few members it calls let it run here.
tests/debugger.mjs runs this first and then reads the same strings in real sessions.

    python3 tests/lldb_strings.py
"""
import os
import struct
import sys
import types

lldb = types.ModuleType("lldb")


class SBError:
    def Fail(self):
        return False


lldb.SBError = SBError
sys.modules["lldb"] = lldb
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "scripts", "lldb"))
import tsuzuri_lldb as formatter  # noqa: E402

LATIN1 = 1 << 62


class Ok:
    def Fail(self):
        return False


class Type:
    def GetCanonicalType(self):
        return self

    def IsPointerType(self):
        return False


class Member:
    def __init__(self, number):
        self.number = number

    def IsValid(self):
        return True

    def GetError(self):
        return Ok()

    def GetValueAsSigned(self):
        return self.number - (1 << 64) if self.number >= 1 << 63 else self.number

    def GetValueAsUnsigned(self, default=0):
        return self.number


class Process:
    def __init__(self, memory):
        self.memory = memory

    def ReadMemory(self, address, size, error):
        return self.memory[address : address + size]


class Value:
    def __init__(self, word, memory):
        self.members = {"data": Member(0), "length": Member(word)}
        self.process = Process(memory)

    def GetNonSyntheticValue(self):
        return self

    def GetType(self):
        return Type()

    def IsValid(self):
        return True

    def GetError(self):
        return Ok()

    def GetChildMemberWithName(self, name):
        return self.members[name]

    def GetProcess(self):
        return self.process


def summary(word, memory):
    return formatter.string_summary(Value(word, memory), {})


def latin1(units):
    return summary(len(units) | LATIN1, bytes(units))


def utf16(units):
    return summary(len(units), b"".join(struct.pack("<H", unit) for unit in units))


cases = [
    ([0x61, 0x62, 0x63], '"abc"'),
    ([0x63, 0x61, 0x66, 0xE9], '"caf\u00e9"'),
    ([0x0A, 0x22, 0xFF, 0x5C, 0x00, 0x09], '"\\n\\"\u00ff\\\\\\0\\t"'),
    ([0x80, 0x9F], '"\\u{80}\\u{9f}"'),
]
for units, expected in cases:
    assert latin1(units) == expected, (units, latin1(units), expected)
    assert utf16(units) == expected, (units, utf16(units), expected)
assert utf16([0x3042, 0x3044]) == '"\u3042\u3044"'
assert utf16([0xD83D, 0xDE00]) == '"\U0001F600"'
assert utf16([0xD800]) == '"\\u{d800}"'

# A long string is cut at the text limit, in either form.
long_units = [0x61 + index % 26 for index in range(formatter.TEXT_LIMIT + 5)]
assert latin1(long_units) == utf16(long_units)
assert latin1(long_units).endswith('"...') or latin1(long_units).endswith('..."'), latin1(long_units)

# The empty string has the word 0. The niche of Maybe<string> (-1), a word with bit 63, and a length
# that no string can have are unreadable, not a count.
assert summary(0, b"") == '""'
assert summary((1 << 64) - 1, b"") == "<invalid length -1>"
assert summary(1 << 63, b"").startswith("<invalid length")
assert summary(LATIN1 | (1 << 40), b"").startswith("<invalid length")
print(f"lldb string formatter: {2 * len(cases) + 3} readings, a long string, the empty string and 3 invalid words")
