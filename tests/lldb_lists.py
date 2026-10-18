"""PM04: how scripts/lldb/tsuzuri_lldb.py reads a list, without a live LLDB session.

A list is `{ head, length }`. `head` points to the first chunk `{ next, rest, capacity, slots }`. A chunk holds the last
`live = length - rest` of its `capacity` slots (`rest` is the number of elements that follow the chunk), so the elements
of the list are the live slots of each chunk in turn, along the `next` links. The formatter reads this through LLDB's
`SBType`, `SBValue` and `SBProcess`; stand-ins for the few members it calls let it run here, with the shapes that the debug
info has (tests/debug_info.rs pins the member names and offsets): the chunk type `[|T|].chunk`, whose `slots` is an array of
length 0 of the element type, at offset 24, or at 32 for elements that are aligned to 16 bytes.
tests/debugger.mjs runs this first and then reads lists in real sessions.

    python3 tests/lldb_lists.py
"""
import os
import random
import struct
import sys
import types

lldb = types.ModuleType("lldb")


class SBError:
    def __init__(self):
        self.failed = False

    def Fail(self):
        return self.failed


lldb.SBError = SBError
sys.modules["lldb"] = lldb
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "scripts", "lldb"))
import tsuzuri_lldb as formatter  # noqa: E402


class Ok:
    def Fail(self):
        return False


class Field:
    def __init__(self, name, offset, ty):
        self.name = name
        self.offset = offset
        self.ty = ty

    def GetName(self):
        return self.name

    def GetOffsetInBytes(self):
        return self.offset

    def GetType(self):
        return self.ty


class Type:
    """An SBType: a name, a size, members, and what it points to or holds."""

    def __init__(self, name, size=8, fields=(), pointee=None, element=None):
        self.name = name
        self.size = size
        self.fields = list(fields)
        self.pointee = pointee
        self.element = element

    def GetName(self):
        return self.name

    def GetByteSize(self):
        return self.size

    def GetCanonicalType(self):
        return self

    def IsTypedefType(self):
        return False

    def IsPointerType(self):
        return self.pointee is not None

    def GetPointeeType(self):
        return self.pointee

    def GetArrayElementType(self):
        return self.element

    def GetNumberOfFields(self):
        return len(self.fields)

    def GetFieldAtIndex(self, index):
        return self.fields[index]


I64 = Type("long", 8)


class Shape:
    """The types of `[|T|]` for an element of `size` bytes aligned to `align`, as the debug info has them."""

    def __init__(self, name, size, align=8):
        self.stride = size
        self.slots = (24 + align - 1) // align * align
        self.element = Type(name, size)
        self.chunk = Type(
            f"[|{name}|].chunk",
            self.slots,
            [
                Field("next", 0, Type("ptr", 8)),
                Field("rest", 8, I64),
                Field("capacity", 16, I64),
                Field("slots", self.slots, Type(f"{name}[0]", 0, element=self.element)),
            ],
        )
        self.list = Type(
            f"[|{name}|]", 16, [Field("head", 0, Type("chunk *", 8, pointee=self.chunk)), Field("length", 8, I64)]
        )


class Process:
    def __init__(self, image):
        self.image = image
        self.reads = 0

    def GetAddressByteSize(self):
        return 8

    def ReadUnsignedFromMemory(self, address, size, error):
        self.reads += 1
        for base, data in self.image.items():
            if base <= address and address + size <= base + len(data):
                offset = address - base
                return int.from_bytes(data[offset : offset + size], "little")
        error.failed = True
        return 0


class Member:
    def __init__(self, ty, number):
        self.ty = ty
        self.number = number

    def IsValid(self):
        return True

    def GetError(self):
        return Ok()

    def GetType(self):
        return self.ty

    def GetValueAsUnsigned(self, default=0):
        return self.number % (1 << 64)

    def GetValueAsSigned(self):
        number = self.number % (1 << 64)
        return number - (1 << 64) if number >= 1 << 63 else number


class List:
    """The SBValue of a list, with the process it lives in and the children that the provider creates."""

    def __init__(self, shape, head, length, image):
        self.shape = shape
        self.members = {"head": Member(shape.list.fields[0].ty, head), "length": Member(I64, length)}
        self.process = Process(image)
        self.children = []

    def GetNonSyntheticValue(self):
        return self

    def GetType(self):
        return self.shape.list

    def IsValid(self):
        return True

    def GetError(self):
        return Ok()

    def GetChildMemberWithName(self, name):
        return self.members[name]

    def GetProcess(self):
        return self.process

    def CreateValueFromAddress(self, name, address, ty):
        child = (name, address, ty.GetName())
        self.children.append(child)
        return child


def chain(shape, chunks, base=0x10000, gap=0x10000):
    """A list of `chunks`, the `(capacity, live)` of each from the head. Returns the head, the length, the memory
    and the address that each element has, in list order."""
    addresses = [base + gap * index for index in range(len(chunks))]
    length = sum(live for _, live in chunks)
    image = {}
    elements = []
    following = length
    for index, (capacity, live) in enumerate(chunks):
        following -= live
        link = addresses[index + 1] if index + 1 < len(chunks) else 0
        header = struct.pack("<QQQ", link, following, capacity) + bytes(shape.slots - 24)
        image[addresses[index]] = header + bytes(capacity * shape.stride)
        first = capacity - live
        elements += [addresses[index] + shape.slots + (first + step) * shape.stride for step in range(live)]
    return addresses[0], length, image, elements


def provider(shape, head, length, image):
    value = List(shape, head, length, image)
    return formatter.ListProvider(value, {}), value


i32 = Shape("int", 4)

# A list that List.cons grew: its first chunk has 3 slots and holds 2 elements (the first slot is 1), the next holds 3.
head, length, image, elements = chain(i32, [(3, 2), (3, 3)])
assert elements == [0x1001C, 0x10020, 0x20018, 0x2001C, 0x20020], [hex(address) for address in elements]
p, value = provider(i32, head, length, image)
assert (p.num_children(), p.has_children(), p.get_child_index("[3]")) == (5, True, 3)
for index, address in enumerate(elements):
    assert p.get_child_at_index(index) == (f"[{index}]", address, "int"), index
assert p.get_child_at_index(5) is None and p.get_child_at_index(-1) is None
assert p.get_child_index("length") == -1 and p.get_child_index("[x]") == -1 and p.get_child_index("3") == -1
assert p.num_children(2) == 2
assert formatter.list_summary(value, {}) == "length=5"

# A list that was built at once is one chunk with no free slot; a pattern tail or List.tail is the same chunk with a
# shorter length, so the live slots are the last ones and the first element is not at slot 0.
head, length, image, elements = chain(i32, [(4, 4)])
for shortened in range(4, 0, -1):
    p, value = provider(i32, head, shortened, image)
    # The chunk says `rest = 0` and `capacity = 4`: the view of the last `shortened` elements starts at slot `4 - shortened`.
    expected = [head + 24 + (4 - shortened + step) * 4 for step in range(shortened)]
    assert [p.get_child_at_index(index)[1] for index in range(shortened)] == expected, shortened
    assert p.get_child_at_index(shortened) is None

# One chunk with free slots in front: a list of 1-byte elements starts with a chunk of 8 slots.
i8 = Shape("signed char", 1)
head, length, image, elements = chain(i8, [(8, 1)])
assert elements == [0x10000 + 24 + 7]
p, _ = provider(i8, head, length, image)
assert p.get_child_at_index(0) == ("[0]", 0x1001F, "signed char") and p.get_child_at_index(1) is None
head, length, image, elements = chain(i8, [(8, 3), (13, 13)])
p, _ = provider(i8, head, length, image)
assert [p.get_child_at_index(index)[1] for index in range(length)] == elements

# Elements aligned to 16 bytes put the slots at offset 32.
wide = Shape("__int128", 16, align=16)
assert wide.slots == 32
head, length, image, elements = chain(wide, [(3, 2), (1, 1)])
assert elements == [0x10000 + 32 + 16, 0x10000 + 32 + 32, 0x20000 + 32]
p, _ = provider(wide, head, length, image)
assert [p.get_child_at_index(index)[1] for index in range(length)] == elements

# The chunks are found by binary search over the positions where they start, and only as far as a query needs: the first
# element reads the header of the first chunk alone, and a chunk is read once.
sizes = [(40, 17), (26, 26), (17, 17), (11, 11), (7, 7), (4, 4), (2, 2), (1, 1)]
head, length, image, elements = chain(i32, sizes)
p, value = provider(i32, head, length, image)
assert p.get_child_at_index(0)[1] == elements[0]
assert value.process.reads == 2, value.process.reads
assert p.get_child_at_index(length - 1)[1] == elements[-1]
reads = value.process.reads
assert reads <= 2 + 3 * (len(sizes) - 1), reads
assert p.get_child_at_index(length - 1)[1] == elements[-1] and p.get_child_at_index(0)[1] == elements[0]
assert value.process.reads == reads, "a chunk that was read is remembered"
many = [(5, 2)] + [(3, 3)] * 199
head, length, image, elements = chain(i32, many)
assert length == 599
p, _ = provider(i32, head, length, image)
order = list(range(length))
random.Random(4).shuffle(order)
for index in order:
    assert p.get_child_at_index(index) == (f"[{index}]", elements[index], "int"), index
# A new provider, asked out of order, also finds each element, because the walk is incremental and the search is not.
p, _ = provider(i32, head, length, image)
for index in [598, 3, 300, 2, 299, 301, 0, 597]:
    assert p.get_child_at_index(index)[1] == elements[index], index
p.update()
assert p.get_child_at_index(1)[1] == elements[1]

# A single element and the empty list.
head, length, image, elements = chain(i32, [(1, 1)])
p, value = provider(i32, head, length, image)
assert (p.num_children(), p.get_child_at_index(0)) == (1, ("[0]", 0x10000 + 24, "int"))
assert formatter.list_summary(value, {}) == "length=1"
p, value = provider(i32, 0, 0, {})
assert (p.num_children(), p.has_children(), p.get_child_at_index(0)) == (0, False, None)
assert formatter.list_summary(value, {}) == "length=0"

# Memory that is not a list: nothing is read past what the header allows, and nothing raises.
head, length, image, elements = chain(i32, [(3, 2), (3, 3)])
for name, garbage_head, garbage_length, garbage_image in [
    ("a head past the address space", (1 << 64) - 4, 2, image),
    ("no head", 0, 3, image),
    ("an unmapped head", 0x30000, 2, image),
]:
    p, _ = provider(i32, garbage_head, garbage_length, garbage_image)
    assert p.get_child_at_index(0) is None, name
# More elements than the chunks hold, and a chunk that claims more live slots than it has.
p, _ = provider(i32, head, 9, image)
assert p.get_child_at_index(0) is None
p, _ = provider(i32, head, 5, {**image, 0x20000: struct.pack("<QQQ", 0, 0, 2) + bytes(8)})
assert p.get_child_at_index(1) is not None and p.get_child_at_index(2) is None, "the second chunk claims 3 live slots of 2"
# A chunk that links to itself: the elements it has are read, the walk ends where they do.
loop = {0x10000: struct.pack("<QQQ", 0x10000, 3, 2) + bytes(8)}
p, _ = provider(i32, 0x10000, 5, loop)
assert p.get_child_at_index(0) is not None and p.get_child_at_index(1) is not None and p.get_child_at_index(2) is None
# A chunk at the end of the address space: the slot that is past it has no child.
top = (1 << 64) - 32
p, _ = provider(i32, top, 3, {top: struct.pack("<QQQ", 0, 0, 4) + bytes(8)})
assert p.get_child_at_index(0) == ("[0]", top + 24 + 4, "int")
assert p.get_child_at_index(1) is None and p.get_child_at_index(2) is None
# A length that no list can have is unreadable, not a count; the empty list's word is 0.
for word, summary in [(-1, "<invalid length -1>"), (1 << 32, f"<invalid length {1 << 32}>")]:
    p, value = provider(i32, 0x10000, word, {})
    assert (p.num_children(), p.get_child_at_index(0)) == (0, None), word
    assert formatter.list_summary(value, {}) == summary, formatter.list_summary(value, {})

# Which types are lists: a name that starts with `[|` and the members `head` and `length`, in that order.
assert formatter.is_list(i32.list, {})
assert not formatter.is_list(Type("Pair", 16, i32.list.fields), {})
assert not formatter.is_list(Type("[|int|]", 16, list(reversed(i32.list.fields))), {})
assert not formatter.is_list(Type("[|int|]", 16, i32.list.fields[:1]), {})
print("lldb list provider: chunks with free slots in front, a view, 3 element sizes, 599 elements in 200 chunks, and garbage")
