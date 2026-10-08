"""LLDB formatters for programs that `tsuzuri build -g` compiles.

Load them with `command script import <path>/tsuzuri_lldb.py`. They show strings, arrays,
slices, `Vec`, lists, unions (including `Maybe` and `Result`), `Map`, `Set`, characters, `unit`,
and function values as Tsuzuri values. They read the DWARF shapes that the compiler emits (the
"Debug information" section of `docs/architecture.md`): types carry their Tsuzuri names, and a
union stores `$tag` and `$payload`, names that no Tsuzuri identifier can take. Only LLDB's own
`lldb` module is used. A value that cannot be read, such as an uninitialized local, gets a summary
in angle brackets instead of an exception.
"""

import lldb

TEXT_LIMIT = 1024
LENGTH_LIMIT = 1 << 32
NEST_LIMIT = 3
CATEGORY = "tsuzuri"
ESCAPES = {0: "\\0", 9: "\\t", 10: "\\n", 13: "\\r", 0x5C: "\\\\"}


def __lldb_init_module(debugger, _dict):
    module = __name__
    recognized = [
        # Summaries also apply through references; children only to values of the type itself.
        # `-e` prints a collection's elements after its summary; `-v` hides the pointer that
        # holds a character or a recursive union's node.
        ("summary", "-v -F", "char_summary", "is_char"),
        ("summary", "-e -F", "sequence_summary", "is_sequence"),
        ("synthetic", "-p -r -l", "SequenceProvider", "is_sequence"),
        ("summary", "-e -F", "list_summary", "is_list"),
        ("synthetic", "-p -r -l", "ListProvider", "is_list"),
        ("summary", "-e -F", "collection_summary", "is_collection"),
        ("synthetic", "-p -r -l", "CollectionProvider", "is_collection"),
        ("summary", "-v -F", "union_summary", "is_union"),
        ("synthetic", "-p -r -l", "UnionProvider", "is_union"),
        ("summary", "-F", "function_summary", "is_function"),
    ]
    commands = [
        f"type category define {CATEGORY}",
        f"type summary add -w {CATEGORY} -F {module}.string_summary string",
        f"type summary add -w {CATEGORY} -F {module}.utf8_summary utf8string",
        f'type summary add -w {CATEGORY} -v -s "()" unit',
    ]
    for kind, options, formatter, recognizer in recognized:
        commands.append(
            f"type {kind} add -w {CATEGORY} {options} {module}.{formatter} "
            f"--recognizer-function {module}.{recognizer}"
        )
    commands.append(f"type category enable {CATEGORY}")
    for command in commands:
        debugger.HandleCommand(command)


class Unreadable(Exception):
    """A value whose memory or fields do not hold a valid Tsuzuri value."""

    def __init__(self, summary="<unreadable>"):
        super().__init__(summary)
        self.summary = summary


def guarded(summary):
    """A summary function for LLDB that reports an unreadable value instead of raising."""

    def run(valobj, _dict, *_options):
        try:
            return summary(valobj)
        except Unreadable as error:
            return error.summary
        except Exception:
            return "<unreadable>"

    run.__name__ = summary.__name__
    return run


def strip(sbtype):
    """`sbtype` without the typedefs around it, keeping the names inside it."""
    while sbtype.IsTypedefType():
        sbtype = sbtype.GetTypedefedType()
    return sbtype


def field_names(sbtype):
    sbtype = sbtype.GetCanonicalType()
    return [sbtype.GetFieldAtIndex(index).GetName() for index in range(sbtype.GetNumberOfFields())]


def target(valobj):
    """The value itself, or the value that a reference to it points to."""
    value = valobj.GetNonSyntheticValue()
    while value.GetType().GetCanonicalType().IsPointerType():
        if value.GetValueAsUnsigned(0) == 0:
            raise Unreadable()
        value = value.Dereference().GetNonSyntheticValue()
    if not value.IsValid() or value.GetError().Fail():
        raise Unreadable()
    return value


def member(value, name):
    child = value.GetChildMemberWithName(name)
    if not child.IsValid() or child.GetError().Fail():
        raise Unreadable()
    return child


def length_of(value, name="length"):
    length = member(value, name).GetValueAsSigned()
    if not 0 <= length < LENGTH_LIMIT:
        raise Unreadable(f"<invalid length {length}>")
    return length


def read(valobj, address, size):
    if size == 0:
        return b""
    error = lldb.SBError()
    data = valobj.GetProcess().ReadMemory(address, size, error)
    if error.Fail() or data is None or len(data) != size:
        raise Unreadable()
    return data


def escape(code, quote):
    """The text of code point `code` inside a Tsuzuri literal quoted with `quote`."""
    if code == quote:
        return "\\" + chr(quote)
    if code in ESCAPES:
        return ESCAPES[code]
    if 0xD800 <= code <= 0xDFFF or not chr(code).isprintable():
        return "\\u{%x}" % code
    return chr(code)


def text(codes, quote, truncated):
    return "".join(escape(code, ord(quote)) for code in codes) + ("..." if truncated else "")


def utf16(units):
    """The code points of UTF-16 `units`; a lone surrogate stays a surrogate, shown as an escape."""
    codes = []
    index = 0
    while index < len(units):
        unit = units[index]
        if (
            0xD800 <= unit < 0xDC00
            and index + 1 < len(units)
            and 0xDC00 <= units[index + 1] < 0xE000
        ):
            codes.append(0x10000 + ((unit - 0xD800) << 10) + units[index + 1] - 0xDC00)
            index += 2
        else:
            codes.append(unit)
            index += 1
    return codes


@guarded
def string_summary(valobj):
    value = target(valobj)
    length = length_of(value)
    count = min(length, TEXT_LIMIT)
    data = read(valobj, member(value, "data").GetValueAsUnsigned(), 2 * count)
    units = [data[index] | data[index + 1] << 8 for index in range(0, len(data), 2)]
    return '"' + text(utf16(units), '"', length > count) + '"'


@guarded
def utf8_summary(valobj):
    value = target(valobj)
    length = length_of(value)
    count = min(length, TEXT_LIMIT)
    data = read(valobj, member(value, "data").GetValueAsUnsigned(), count)
    codes = [ord(character) for character in data.decode("utf-8", errors="replace")]
    return 'u8"' + text(codes, '"', length > count) + '"'


def is_char(sbtype, _dict):
    return sbtype.IsTypedefType() and sbtype.GetName() in ("char", "utf8char")


@guarded
def char_summary(valobj):
    value = target(valobj)
    code = value.GetValueAsUnsigned()
    if code > 0x10FFFF:
        raise Unreadable(f"<invalid character {code}>")
    prefix = "u8" if value.GetType().GetName() == "utf8char" else ""
    return prefix + "'" + escape(code, ord("'")) + "'"


def is_sequence(sbtype, _dict):
    """An array `[T]`, a slice `ref [T]` or `ref mut [T..]`, or a `Vec<T>`."""
    name = sbtype.GetName() or ""
    if name.startswith("[|") or not name.startswith(("[", "ref [", "ref mut [", "Vec<")):
        return False
    return field_names(sbtype) in (["data", "length"], ["data", "length", "capacity"])


@guarded
def sequence_summary(valobj):
    value = target(valobj)
    summary = f"length={length_of(value)}"
    if value.GetChildMemberWithName("capacity").IsValid():
        summary += f" capacity={length_of(value, 'capacity')}"
    return summary


def child_index(name):
    if name.startswith("[") and name.endswith("]") and name[1:-1].isdigit():
        return int(name[1:-1])
    return -1


class SequenceProvider:
    """The elements `[0]`, `[1]`, ... of an array, a slice, or a `Vec`."""

    def __init__(self, valobj, _dict):
        self.valobj = valobj
        self.update()

    def update(self):
        self.count = 0
        try:
            value = self.valobj.GetNonSyntheticValue()
            data = member(value, "data")
            self.element = strip(data.GetType()).GetPointeeType()
            self.stride = self.element.GetByteSize()
            self.address = data.GetValueAsUnsigned()
            if self.element.IsValid() and self.stride > 0:
                self.count = length_of(value)
        except Exception:
            self.count = 0
        return False

    def num_children(self, max_children=LENGTH_LIMIT):
        return min(self.count, max_children)

    def has_children(self):
        return self.count > 0

    def get_child_index(self, name):
        return child_index(name)

    def get_child_at_index(self, index):
        if not 0 <= index < self.count:
            return None
        return self.valobj.CreateValueFromAddress(
            f"[{index}]", self.address + index * self.stride, self.element
        )


def is_list(sbtype, _dict):
    name = sbtype.GetName() or ""
    return name.startswith("[|") and field_names(sbtype) == ["head", "length"]


@guarded
def list_summary(valobj):
    return f"length={length_of(target(valobj))}"


class ListProvider:
    """The elements of a list, in order along the `next` links of its nodes."""

    def __init__(self, valobj, _dict):
        self.valobj = valobj
        self.update()

    def update(self):
        self.count = 0
        self.nodes = []
        try:
            value = self.valobj.GetNonSyntheticValue()
            head = member(value, "head")
            node = strip(head.GetType()).GetPointeeType()
            field = node.GetFieldAtIndex(1)
            self.offset = field.GetOffsetInBytes()
            self.element = field.GetType()
            self.head = head.GetValueAsUnsigned()
            self.count = length_of(value)
        except Exception:
            self.count = 0
        return False

    def node(self, index):
        """The address of node `index`, following the links from the last node visited."""
        process = self.valobj.GetProcess()
        while len(self.nodes) <= index:
            if self.nodes:
                error = lldb.SBError()
                address = process.ReadPointerFromMemory(self.nodes[-1], error)
                if error.Fail():
                    return 0
            else:
                address = self.head
            if address == 0:
                return 0
            self.nodes.append(address)
        return self.nodes[index]

    def num_children(self, max_children=LENGTH_LIMIT):
        return min(self.count, max_children)

    def has_children(self):
        return self.count > 0

    def get_child_index(self, name):
        return child_index(name)

    def get_child_at_index(self, index):
        if not 0 <= index < self.count:
            return None
        address = self.node(index)
        if address == 0:
            return None
        return self.valobj.CreateValueFromAddress(
            f"[{index}]", address + self.offset, self.element
        )


def is_collection(sbtype, _dict):
    """The std `Map<K, V>` or `Set<K>`, a record of its sorted `entries`."""
    name = sbtype.GetName() or ""
    return name.startswith(("Map<", "Set<")) and field_names(sbtype) == ["entries"]


@guarded
def collection_summary(valobj):
    return f"size={length_of(member(target(valobj), 'entries'))}"


class CollectionProvider(SequenceProvider):
    """The entries of a `Map` or the keys of a `Set`."""

    def __init__(self, valobj, internal_dict):
        entries = valobj.GetNonSyntheticValue().GetChildMemberWithName("entries")
        super().__init__(entries, internal_dict)


def union_node_type(sbtype):
    """The structure with `$tag` that a value of `sbtype` holds, or that it points to as a
    recursive union's node; otherwise None."""
    node = sbtype.GetCanonicalType()
    if node.IsPointerType():
        node = node.GetPointeeType().GetCanonicalType()
        if not (node.GetName() or "").endswith(".node"):
            return None
    if node.GetTypeClass() != lldb.eTypeClassStruct:
        return None
    return node if "$tag" in field_names(node) else None


def is_union(sbtype, _dict):
    return union_node_type(sbtype) is not None


def empty_case(node):
    """The case that a null node stands for: the first case without a payload."""
    payloads = set()
    tag = None
    for index in range(node.GetNumberOfFields()):
        field = node.GetFieldAtIndex(index)
        if field.GetName() == "$tag":
            tag = strip(field.GetType())
        elif field.GetName() == "$payload":
            payloads = set(field_names(field.GetType()))
    for case in tag.GetEnumMembers():
        if case.GetName() not in payloads:
            return case.GetName()
    raise Unreadable()


def active_case(valobj):
    """The case name and payload (None if nullary) of a union value or of the union that a
    reference points to."""
    value = valobj.GetNonSyntheticValue()
    while value.GetType().GetCanonicalType().IsPointerType():
        if value.GetValueAsUnsigned(0) == 0:
            node = union_node_type(value.GetType())
            if node is None:
                raise Unreadable()
            return empty_case(node), None
        value = value.Dereference().GetNonSyntheticValue()
    if not value.IsValid() or value.GetError().Fail():
        raise Unreadable()
    tag = member(value, "$tag")
    code = tag.GetValueAsUnsigned()
    names = {case.GetValueAsUnsigned(): case.GetName() for case in strip(tag.GetType()).GetEnumMembers()}
    if code not in names:
        raise Unreadable(f"<invalid tag {code}>")
    payload = value.GetChildMemberWithName("$payload")
    if payload.IsValid():
        payload = payload.GetChildMemberWithName(names[code])
    return names[code], payload if payload.IsValid() else None


def parts(payload):
    """The values of a payload: the elements of a tuple, or the payload itself."""
    if (strip(payload.GetType()).GetName() or "").startswith("("):
        return [payload.GetChildAtIndex(index) for index in range(payload.GetNumChildren())]
    return [payload]


def describe(value, depth):
    if is_union(value.GetType(), None):
        return union_text(value, depth + 1)
    summary = value.GetSummary()
    if summary:
        return summary
    return value.GetValue() or "{...}"


def union_text(valobj, depth):
    try:
        name, payload = active_case(valobj)
    except Unreadable as error:
        return error.summary
    if payload is None:
        return name
    if depth >= NEST_LIMIT:
        return name + "(...)"
    return name + "(" + ", ".join(describe(part, depth) for part in parts(payload)) + ")"


@guarded
def union_summary(valobj):
    return union_text(valobj, 0)


class UnionProvider:
    """The payload of the active case: a tuple's elements, or the payload as `0`."""

    def __init__(self, valobj, _dict):
        self.valobj = valobj
        self.update()

    def update(self):
        self.children = []
        try:
            _name, payload = active_case(self.valobj)
            if payload is not None:
                for index, part in enumerate(parts(payload)):
                    self.children.append(
                        self.valobj.CreateValueFromAddress(
                            str(index), part.GetLoadAddress(), part.GetType()
                        )
                    )
        except Exception:
            self.children = []
        return False

    def num_children(self, max_children=LENGTH_LIMIT):
        return min(len(self.children), max_children)

    def has_children(self):
        return bool(self.children)

    def get_child_index(self, name):
        return int(name) if name.isdigit() and int(name) < len(self.children) else -1

    def get_child_at_index(self, index):
        return self.children[index] if 0 <= index < len(self.children) else None


def is_function(sbtype, _dict):
    return field_names(sbtype) == ["code", "environment", "clone", "drop"]


@guarded
def function_summary(valobj):
    value = target(valobj)
    if (strip(value.GetType()).GetName() or "").startswith("Task<"):
        return "<task>"
    code = member(value, "code").GetValueAsUnsigned()
    return f"<fn {function_name(valobj.GetTarget(), code)}>"


def function_name(sbtarget, code):
    """The source name of the function that a function value calls. Its code is the adapter
    `tz.apply.<symbol>.<captures>`, which calls `tz.fn.<symbol>`."""
    symbol = sbtarget.ResolveLoadAddress(code).GetSymbol()
    name = symbol.GetName() if symbol.IsValid() else None
    if not name:
        return f"0x{code:x}"
    if name.startswith("tz.apply."):
        callee = "tz.fn." + name[len("tz.apply."):].rsplit(".", 1)[0]
        for context in sbtarget.FindSymbols(callee):
            function = context.GetSymbol().GetStartAddress().GetFunction()
            if function.IsValid() and function.GetName():
                return function.GetName()
        return callee
    function = symbol.GetStartAddress().GetFunction()
    return function.GetName() if function.IsValid() and function.GetName() else name
