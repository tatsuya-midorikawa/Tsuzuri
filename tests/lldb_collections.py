"""Chunk traversal, including Vec headers with LLDB's synthetic children enabled."""
import struct
import sys
import types
from pathlib import Path

fake = types.ModuleType("lldb")
fake.SBError = type("SBError", (), {"Fail": lambda self: False})
fake.eTypeClassStruct = 1
sys.modules["lldb"] = fake
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts" / "lldb"))
import tsuzuri_lldb as formatter  # noqa: E402

memory = bytearray(1 << 20)
cursor = 64


def allocate(size):
    global cursor
    address = cursor
    cursor += (size + 15) // 16 * 16
    return address


class Field:
    def __init__(self, name, ty, offset):
        self.name, self.ty, self.offset = name, ty, offset

    def GetName(self):
        return self.name

    def GetType(self):
        return self.ty

    def GetOffsetInBytes(self):
        return self.offset


class Type:
    def __init__(self, name, size, fields=(), pointee=None, pointer=False):
        self.name, self.size, self.fields, self.pointee, self.pointer = name, size, list(fields), pointee, pointer

    def IsTypedefType(self):
        return False

    def GetCanonicalType(self):
        return self

    def GetName(self):
        return self.name

    def GetByteSize(self):
        return self.size

    def IsValid(self):
        return True

    def GetPointeeType(self):
        return self.pointee

    def IsPointerType(self):
        return self.pointer

    def GetNumberOfFields(self):
        return len(self.fields)

    def GetFieldAtIndex(self, index):
        return self.fields[index]


class Error:
    def Fail(self):
        return False


class Value:
    def __init__(self, ty, address, name="value"):
        self.ty, self.address, self.name = ty, address, name

    def GetNonSyntheticValue(self):
        return self

    def GetType(self):
        return self.ty

    def IsValid(self):
        return self.ty.name != "invalid"

    def GetError(self):
        return Error()

    def GetChildMemberWithName(self, name):
        for field in self.ty.fields:
            if field.name == name:
                return Value(field.ty, self.address + field.offset, name)
        return Value(Type("invalid", 0), 0, name)

    def GetValueAsUnsigned(self, default=0):
        return struct.unpack_from("<Q", memory, self.address)[0]

    def GetValueAsSigned(self, default=0):
        return struct.unpack_from("<q", memory, self.address)[0]

    def CreateValueFromAddress(self, name, address, ty):
        value = SyntheticValue if ty.name.startswith("Vec<") else Value
        return value(ty, address, name)


class SyntheticValue(Value):
    def GetNonSyntheticValue(self):
        return Value(self.ty, self.address, self.name)

    def GetChildMemberWithName(self, name):
        return Value(Type("invalid", 0), 0, name)


i32 = Type("i32", 4)
i64 = Type("i64", 8)
entry = Type("Map.Entry<i32, i32>", 8, [Field("key", i32, 0), Field("value", i32, 4)])
entry_pointer = Type("Map.Entry<i32, i32> *", 8, pointee=entry, pointer=True)
chunk = Type("Vec<Map.Entry<i32, i32>>", 24, [Field("data", entry_pointer, 0), Field("length", i64, 8), Field("capacity", i64, 16)])
chunk_pointer = Type("Vec<Map.Entry<i32, i32>> *", 8, pointee=chunk, pointer=True)
chunks = Type("Vec<Vec<Map.Entry<i32, i32>>>", 24, [Field("data", chunk_pointer, 0), Field("length", i64, 8), Field("capacity", i64, 16)])
map_type = Type("Map<i32, i32>", 56, [Field("head", chunk, 0), Field("rest", chunks, 24), Field("length", i64, 48)])


def vec(address, data, length):
    struct.pack_into("<QQQ", memory, address, data, length, length)


def build(sizes):
    addresses = [allocate(8 * max(sizes[0], 1))]
    rest = allocate(24 * max(len(sizes) - 1, 1))
    for number, size in enumerate(sizes[1:]):
        data = allocate(8 * size)
        addresses.append(data)
        vec(rest + 24 * number, data, size)
    root = allocate(56)
    vec(root, addresses[0], sizes[0])
    vec(root + 24, rest, len(sizes) - 1)
    struct.pack_into("<q", memory, root + 48, sum(sizes))
    where = []
    for number, size in enumerate(sizes):
        where.extend(addresses[number] + 8 * offset for offset in range(size))
    return Value(map_type, root), where


assert formatter.is_collection(map_type, {})
assert not formatter.is_collection(Type("Map<i32, i32>", 24, [Field("entries", chunk, 0)]), {})

for sizes in ([0], [1], [3], [2, 3, 1], [4, 1], [1, 1, 1, 1, 1], [512, 512, 7]):
    value, where = build(sizes)
    provider = formatter.CollectionProvider(value, {})
    total = sum(sizes)
    assert provider.num_children() == total, (sizes, provider.num_children())
    assert provider.has_children() == (total > 0)
    assert provider.read == 0
    assert formatter.collection_summary(value, {}) == f"size={total}"
    for order in (range(total), reversed(range(total))):
        provider.update()
        for index in order:
            child = provider.get_child_at_index(index)
            assert child is not None and child.address == where[index] and child.name == f"[{index}]", (sizes, index)
    assert provider.get_child_at_index(total) is None
    assert provider.get_child_at_index(-1) is None
    assert provider.get_child_index("[3]") == 3


class CountedBoundaries(list):
    reads = 0

    def __getitem__(self, index):
        self.reads += 1
        return super().__getitem__(index)


value, where = build([257] * 128)
provider = formatter.CollectionProvider(value, {})
assert provider.read == 0
assert provider.get_child_at_index(len(where) - 1).address == where[-1]
provider.ends = CountedBoundaries(provider.ends)
for index in [len(where) - 1, 0, len(where) // 2, 257 * 127]:
    provider.ends.reads = 0
    assert provider.get_child_at_index(index).address == where[index]
    assert provider.ends.reads <= len(provider.ends).bit_length() + 2
    assert provider.read == 127
print("collection provider: 7 layouts, synthetic Vec headers, lazy/logarithmic traversal: ok")
