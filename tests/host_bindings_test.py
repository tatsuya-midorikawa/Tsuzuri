"""Checks the Python bindings of tests/fixtures/bindings_native (tests/host_bindings.mjs).

usage: python3 host_bindings_test.py BINDINGS.py LIBRARY [TRAP_BINDINGS.py TRAP_LIBRARY TRAP_JSON]
The first library is built with --allocator counting, the second with --trap-mode return.
"""

import array
import ctypes
import importlib.util
import json
import struct
import subprocess
import sys


def module_at(path, name):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module
    spec.loader.exec_module(module)
    return module


def raises(kind, call, text=None):
    try:
        call()
    except kind as error:
        assert text is None or text in str(error), f"{error!r} lacks {text!r}"
        return error
    raise AssertionError(f"expected {kind.__name__}")


def f32(value):
    return struct.unpack("f", struct.pack("f", value))[0]


def live_bytes(path):
    stats = (ctypes.c_uint64 * 4)()
    ctypes.CDLL(path).tsuzuri_alloc_stats(stats)
    return stats[2]


def check_common(native, lib):
    assert lib.add(40, 2) == 42
    assert lib.add_u64(2**64 - 1, 0) == 2**64 - 1
    assert lib.widen(-128, 65535, 4294967295) == -128 + 65535 + 4294967295
    assert lib.narrow(300) == 300 - 256 and lib.narrow(-129) == 127
    assert lib.negate(True) is False and lib.negate(False) is True
    assert lib.twice32(0.1) == f32(0.1) * 2
    raises(TypeError, lambda: lib.add(1.5, 2), "argument 0 of 'add' must be an int")
    raises(TypeError, lambda: lib.add(True, 2))
    raises(OverflowError, lambda: lib.add(2**63, 0), "argument 0 of 'add' is out of range for i64")
    raises(OverflowError, lambda: lib.widen(128, 0, 0))
    raises(OverflowError, lambda: lib.add_u64(-1, 0))
    raises(TypeError, lambda: lib.negate(1), "must be a bool")
    assert lib.sum_float([0.5, 0.25]) == 0.75
    assert lib.sum_float(array.array("d", [1.5, 2.5])) == 4.0
    assert lib.sum_float(()) == 0.0
    assert lib.checksum(b"\x01\x02\xff") == 258
    assert lib.checksum(bytearray([7, 9])) == 16
    raises(TypeError, lambda: lib.checksum(array.array("d", [1.0])))
    assert lib.make_bytes(4) == b"\x00\x01\x02\x03" and lib.make_bytes(0) == b""
    copied = lib.copy_values(array.array("q", [1, -2, 2**63 - 1]))
    assert isinstance(copied, array.array) and list(copied) == [1, -2, 2**63 - 1]
    assert list(lib.copy_values([3, 4])) == [3, 4] and len(lib.copy_values([])) == 0
    raises(OverflowError, lambda: lib.copy_values([2**63]), "argument 0 of 'copy_values' element 0")
    text = "a\ud800b\U0001f600"
    assert lib.copy_text(text) == text and lib.copy_text("") == ""
    assert lib.copy_utf8("\u00e9\U0001f600") == "\u00e9\U0001f600"
    assert lib.copy_utf8(b"abc") == "abc"
    raises(ValueError, lambda: lib.copy_utf8("\ud800"), "well-formed")
    raises(ValueError, lambda: lib.copy_utf8(b"\xff"), "valid UTF-8")
    flags = native.tz_record_4Main_5Flags(-1, 65535, True)
    updated = lib.update(native.tz_record_4Main_6Sample(flags, 1.5))
    assert updated == native.tz_record_4Main_6Sample(native.tz_record_4Main_5Flags(-1, 65535, False), 2.5)
    raises(OverflowError, lambda: lib.update(native.tz_record_4Main_6Sample(native.tz_record_4Main_5Flags(-129, 0, True), 0.0)), "field 'flags' field 'tiny'")
    raises(TypeError, lambda: lib.update(object()), "must be a tz_record_4Main_6Sample")
    assert lib.area(native.tz_record_8Geometry_5Point(2, 3.5)) == 7.0
    assert lib.window_total(native.tz_record_4Main_6Window((1, 2, 3), 2.0, 7)) == 19.0
    assert lib.make_window(5) == native.tz_record_4Main_6Window((5, 10, -5), 0.5, 255)
    raises(TypeError, lambda: lib.window_total(native.tz_record_4Main_6Window((1, 2), 2.0, 7)), "sequence of 3 elements")
    assert lib.counters(10) == 30
    handle = native.tz_handle_4Main_7Counter(0x1234)
    assert lib.pass_through(handle) == handle
    raises(TypeError, lambda: lib.pass_through(0x1234), "must be a tz_handle_4Main_7Counter")
    # A borrowed handle reaches the host as the pointer itself: here, an int64 counter cell.
    cell = ctypes.c_int64(41)
    assert lib.peek(native.tz_handle_4Main_7Counter(ctypes.addressof(cell))) == 41
    assert list(lib.scaled([1.0, 2.0], 3.0)) == [3.0, 6.0]
    assert lib.check(5) is None


def main():
    bindings, library = sys.argv[1], sys.argv[2]
    native = module_at(bindings, "native")
    lib = native.load(library)
    check_common(native, lib)
    assert live_bytes(library) == 0
    # A trap ends the process: the library was built without --trap-mode return.
    child = subprocess.run([sys.executable, "-c", f"import importlib.util as u; s = u.spec_from_file_location('n', {bindings!r}); m = u.module_from_spec(s); s.loader.exec_module(m); m.load({library!r}).divide(1, 0)"], capture_output=True)
    assert child.returncode != 0, child
    if len(sys.argv) > 3:
        trap_bindings, trap_library, sites = sys.argv[3], sys.argv[4], sys.argv[5]
        trapping = module_at(trap_bindings, "native_trap")
        lib = trapping.load(trap_library)
        check_common(trapping, lib)
        error = raises(trapping.TsuzuriTrap, lambda: lib.divide(1, 0), "trap: integer division by zero")
        assert error.kind == 1 and error.kind_name == "integer division by zero"
        site = next(item for item in json.load(open(sites))["sites"] if item["id"] == error.site)
        assert site["kind"] == "integer division by zero" and site["path"].endswith("Main.tz")
        assert lib.divide(-7, 2) == -3
        raises(trapping.TsuzuriTrap, lambda: lib.check(-1), "assertion failed")
        assert lib.copy_text("after") == "after"
    print("python bindings passed")


main()
