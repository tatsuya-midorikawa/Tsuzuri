#!/usr/bin/env python3
"""Writes reference.json, the independent reference of tests/tensor_codec.rs (NC11).

Every expected byte and value comes from NumPy, zlib, zipfile or the Safetensors format description, never
from Tsuzuri:

    python3 -I tests/fixtures/tensor_codec/generate.py

needs NumPy (the checked-in file was written with the version it records). The output is deterministic:
the ZIP time stamps are fixed and the random bytes come from a seeded generator.
"""
import io
import json
import os
import random
import struct
import sys
import warnings
import zipfile
import zlib

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
random.seed(20261128)


def npy_bytes(array, **options):
    buffer = io.BytesIO()
    np.save(buffer, array, **options)
    return buffer.getvalue()


def npy_version(array, version):
    buffer = io.BytesIO()
    np.lib.format.write_array(buffer, array, version=version)
    return buffer.getvalue()


TYPES = {
    "bool": "bool", "int8": "i8", "int16": "i16", "int32": "i32", "int64": "i64",
    "uint8": "i8u", "uint16": "i16u", "uint32": "i32u", "uint64": "i64u",
    "float16": "f16", "float32": "f32", "float64": "f64",
    "complex64": "Complex<f32>", "complex128": "Complex<f64>",
}
SUFFIX = {
    "int8": "i8", "int16": "i16", "int32": "i32", "int64": "i64",
    "uint8": "i8u", "uint16": "i16u", "uint32": "i32u", "uint64": "i64u",
    "float16": "f16", "float32": "f32", "float64": "",
}


def literal(value, name):
    """A Tsuzuri literal of a finite value of the dtype `name`."""
    if name == "bool":
        return "true" if value else "false"
    if name.startswith("float"):
        text = repr(float(value))
        if "e" not in text and "." not in text:
            text += ".0"
        return text + SUFFIX[name]
    return str(int(value)) + SUFFIX[name]


def values_of(array):
    """The elements in row-major order as Tsuzuri literals; a complex element is its two parts."""
    flat = np.ascontiguousarray(array).reshape(-1)
    name = array.dtype.name
    if name.startswith("complex"):
        part = "float32" if name == "complex64" else "float64"
        return [literal(x, part) for c in flat for x in (c.real, c.imag)]
    return [literal(x, name) for x in flat]


def sample(name, shape):
    """A finite array of the dtype with distinct, exactly representable elements."""
    count = int(np.prod(shape)) if shape else 1
    index = np.arange(count)
    if name == "bool":
        flat = (index % 3 == 0)
    elif name.startswith("int"):
        info = np.iinfo(name)
        flat = np.array([info.min, info.max, -1, 0, 1, 7, -9, 100][:count] + list(range(max(count - 8, 0))))[:count]
    elif name.startswith("uint"):
        info = np.iinfo(name)
        flat = np.array([0, int(info.max), 1, 2, 200, 255, 17, 99][:count] + list(range(max(count - 8, 0))), dtype=np.uint64)[:count]
    elif name.startswith("float"):
        flat = np.array([0.5, -1.25, 3.0, 1024.0, -0.0078125, 2.5, 100.0, -65.5][:count] + [k * 0.25 for k in range(max(count - 8, 0))])[:count]
    else:
        real = np.array([0.5, -1.25, 3.0, 1024.0, -0.0078125, 2.5][:count] + [k * 0.25 for k in range(max(count - 6, 0))])[:count]
        flat = real + 1j * (real * 2 + 1)
    return np.array(flat).astype(name).reshape(shape)


def npy_cases():
    cases = []

    def add(name, array, data, values=True):
        # `copy(order="C")` keeps the rank; `ascontiguousarray` would turn a scalar into one element of rank 1.
        native = array.copy(order="C").astype(array.dtype.newbyteorder("<") if array.dtype.itemsize > 1 else array.dtype)
        canonical = npy_bytes(native)
        cases.append({
            "name": name,
            "type": TYPES[array.dtype.name],
            "shape": list(array.shape),
            "hex": data.hex(),
            "canonical": canonical.hex(),
            "values": values_of(array) if values else None,
        })

    for name in TYPES:
        array = sample(name, (2, 3))
        add(f"{name}_2x3", array, npy_bytes(array))
        if array.dtype.itemsize > 1:
            big = array.astype(array.dtype.newbyteorder(">"))
            add(f"{name}_big_endian", big, npy_bytes(big))
        add(f"{name}_fortran", np.asfortranarray(array), npy_bytes(np.asfortranarray(array)))
    f64 = sample("float64", (2, 3, 4))
    add("f64_3d", f64, npy_bytes(f64))
    add("f64_3d_fortran", np.asfortranarray(f64), npy_bytes(np.asfortranarray(f64)))
    big = np.asfortranarray(f64).astype(">f8")
    add("f64_3d_fortran_big_endian", big, npy_bytes(big))
    add("f64_scalar", np.array(2.5), npy_bytes(np.array(2.5)))
    add("i32_1d", sample("int32", (5,)), npy_bytes(sample("int32", (5,))))
    add("f32_empty", np.zeros((0,), dtype=np.float32), npy_bytes(np.zeros((0,), dtype=np.float32)))
    add("f32_empty_2d", np.zeros((3, 0), dtype=np.float32), npy_bytes(np.zeros((3, 0), dtype=np.float32)))
    add("i64_version_2", sample("int64", (4,)), npy_version(sample("int64", (4,)), (2, 0)))
    add("i64_version_3", sample("int64", (4,)), npy_version(sample("int64", (4,)), (3, 0)))
    add("f64_long_first_axis", np.zeros((1234567, 0)), npy_bytes(np.zeros((1234567, 0))))
    special = np.array([np.nan, -np.nan, np.inf, -np.inf, -0.0, 5e-324, 1.7976931348623157e308])
    special.view(np.uint64)[0] = 0x7FF8000000000123  # a NaN with a payload
    special.view(np.uint64)[1] = 0xFFF0000000000001  # a signalling NaN
    add("f64_special_values", special, npy_bytes(special), values=False)
    special32 = np.array([np.nan, np.inf, -np.inf, -0.0, 1e-45], dtype=np.float32)
    special32.view(np.uint32)[0] = 0x7FA00001
    add("f32_special_values", special32, npy_bytes(special32), values=False)
    special16 = np.array([np.nan, np.inf, -np.inf, -0.0, 6e-8], dtype=np.float16)
    special16.view(np.uint16)[0] = 0x7E01
    add("f16_special_values", special16, npy_bytes(special16), values=False)
    # A header as NumPy 1.x on Python 2 wrote it: no spare room, 16-byte alignment, a long suffix.
    body = b"{'descr': '<i4', 'fortran_order': False, 'shape': (3L,), }"
    body += b" " * ((16 - (10 + len(body) + 1) % 16) % 16) + b"\n"
    old = b"\x93NUMPY\x01\x00" + struct.pack("<H", len(body)) + body + np.array([1, 2, 3], dtype="<i4").tobytes()
    add("i32_python2_header", np.array([1, 2, 3], dtype=np.int32), old)
    # Double quotes, another key order, and trailing data, which a reader ignores.
    body = b'{"shape": (2,), "fortran_order": False, "descr": "|u1"}\n'
    other = b"\x93NUMPY\x01\x00" + struct.pack("<H", len(body)) + body + b"\x07\x09" + b"trailing"
    add("u8_other_header", np.array([7, 9], dtype=np.uint8), other)
    return cases


def view_cases():
    """What np.save writes for views of one array: Tsuzuri builds the same windows and must write the same bytes."""
    base = np.arange(24, dtype=np.float32).reshape(2, 3, 4) * 0.5
    return {
        "base": npy_bytes(base).hex(),
        "permuted_2_0_1": npy_bytes(np.ascontiguousarray(base.transpose(2, 0, 1))).hex(),
        "flipped_axis_1": npy_bytes(np.ascontiguousarray(base[:, ::-1, :])).hex(),
        "narrowed_axis_2": npy_bytes(np.ascontiguousarray(base[:, :, 1:3])).hex(),
        "row_1": npy_bytes(np.ascontiguousarray(base[1])).hex(),
    }


def header(dictionary, data=b"", version=(1, 0), length=None):
    body = dictionary if isinstance(dictionary, bytes) else dictionary.encode("latin1")
    if not body.endswith(b"\n"):
        body += b"\n"
    size = len(body) if length is None else length
    packed = struct.pack("<H", size) if version[0] == 1 else struct.pack("<I", size)
    return b"\x93NUMPY" + bytes(version) + packed + body + data


def npy_errors():
    good = npy_bytes(np.arange(6, dtype=np.float64))
    f8 = np.arange(2, dtype="<f8").tobytes()
    cases = [
        ("empty", b"", "InvalidMagic"),
        ("magic", b"PK\x03\x04" + good[4:], "InvalidMagic"),
        ("magic_only", good[:6], "TruncatedData"),
        ("version_4", good[:6] + b"\x04\x00" + good[8:], "UnsupportedVersion"),
        ("version_1_1", good[:6] + b"\x01\x01" + good[8:], "UnsupportedVersion"),
        ("header_cut", good[:40], "TruncatedData"),
        ("data_cut", good[:-1], "TruncatedData"),
        ("data_missing", good[:128], "TruncatedData"),
        ("header_length_past_end", header("{'descr': '<f8', 'fortran_order': False, 'shape': (2,), }", f8, length=60000), "TruncatedData"),
        ("version_2_length_past_end", b"\x93NUMPY\x02\x00\xff\xff\xff\xff" + b"{}\n", "TruncatedData"),
        ("object", npy_bytes(np.array([{"a": 1}, None], dtype=object), allow_pickle=True), "UnsupportedDtype"),
        ("structured", npy_bytes(np.zeros(2, dtype=[("a", "<i4"), ("b", "<f8")])), "UnsupportedDtype"),
        ("unicode", npy_bytes(np.array(["ab", "c"])), "UnsupportedDtype"),
        ("bytes", npy_bytes(np.array([b"ab", b"c"])), "UnsupportedDtype"),
        ("datetime", npy_bytes(np.array(["2026-11-28"], dtype="datetime64[D]")), "UnsupportedDtype"),
        ("long_double", header("{'descr': '<f16', 'fortran_order': False, 'shape': (1,), }", b"\x00" * 16), "UnsupportedDtype"),
        ("complex256", header("{'descr': '<c32', 'fortran_order': False, 'shape': (1,), }", b"\x00" * 32), "UnsupportedDtype"),
        ("int128", header("{'descr': '<i16', 'fortran_order': False, 'shape': (1,), }", b"\x00" * 16), "UnsupportedDtype"),
        ("shape_overflow", header("{'descr': '<f8', 'fortran_order': False, 'shape': (4611686018427387904, 4611686018427387904), }", f8), "ShapeOverflow"),
        ("shape_too_large", header("{'descr': '<f8', 'fortran_order': False, 'shape': (9223372036854775807,), }", f8), "ShapeOverflow"),
        ("extent_above_i64", header("{'descr': '<f8', 'fortran_order': False, 'shape': (99999999999999999999,), }", f8), "ShapeOverflow"),
        ("negative_extent", header("{'descr': '<f8', 'fortran_order': False, 'shape': (-2,), }", f8), "InvalidHeader"),
        ("too_many_axes", header("{'descr': '<f8', 'fortran_order': False, 'shape': (" + "1, " * 65 + "), }", f8), "InvalidHeader"),
        ("missing_key", header("{'descr': '<f8', 'shape': (2,), }", f8), "InvalidHeader"),
        ("extra_key", header("{'descr': '<f8', 'fortran_order': False, 'shape': (2,), 'more': 1, }", f8), "InvalidHeader"),
        ("repeated_key", header("{'descr': '<f8', 'descr': '<f8', 'fortran_order': False, 'shape': (2,), }", f8), "InvalidHeader"),
        ("not_a_dictionary", header("[1, 2, 3]", f8), "InvalidHeader"),
        ("code", header("__import__('os').system('true')", f8), "InvalidHeader"),
        ("unclosed", header("{'descr': '<f8', 'fortran_order': False, 'shape': (2,", f8), "InvalidHeader"),
        ("fortran_order_number", header("{'descr': '<f8', 'fortran_order': 0, 'shape': (2,), }", f8), "InvalidHeader"),
        ("shape_list", header("{'descr': '<f8', 'fortran_order': False, 'shape': [2], }", f8), "InvalidHeader"),
        ("text_after", header("{'descr': '<f8', 'fortran_order': False, 'shape': (2,), } x", f8), "InvalidHeader"),
        ("escape", header("{'de\\x73cr': '<f8', 'fortran_order': False, 'shape': (2,), }", f8), "InvalidHeader"),
        ("empty_header", header("", f8), "InvalidHeader"),
    ]
    return [{"name": name, "hex": data.hex(), "error": error} for name, data, error in cases]


def fixed_zip(entries, compression, patch=None):
    """A ZIP archive with fixed time stamps; `entries` are (name, bytes)."""
    buffer = io.BytesIO()
    with zipfile.ZipFile(buffer, "w", compression) as archive:
        for name, data in entries:
            info = zipfile.ZipInfo(name, date_time=(1980, 1, 1, 0, 0, 0))
            info.compress_type = compression
            if patch:
                patch(info)
            archive.writestr(info, data)
    return buffer.getvalue()


def numpy_npz(arrays, compressed):
    """What np.savez writes, with the time stamps of the entries zeroed so that the bytes are stable."""
    buffer = io.BytesIO()
    (np.savez_compressed if compressed else np.savez)(buffer, **arrays)
    data = bytearray(buffer.getvalue())
    # Local headers: time and date at offset 10; central headers: at offset 12.
    at = data.find(b"PK\x03\x04")
    while at >= 0:
        data[at + 10:at + 14] = b"\x00\x00\x21\x00"
        at = data.find(b"PK\x03\x04", at + 4)
    at = data.find(b"PK\x01\x02")
    while at >= 0:
        data[at + 12:at + 16] = b"\x00\x00\x21\x00"
        at = data.find(b"PK\x01\x02", at + 4)
    with zipfile.ZipFile(io.BytesIO(bytes(data))) as check:
        assert check.testzip() is None
    return bytes(data)


def npz_cases():
    arrays = {
        "weights": sample("float64", (3, 4)),
        "bias": sample("float32", (4,)),
        "labels": sample("int64", (5,)),
        "mask": sample("bool", (2, 3)),
        "scalar": np.array(7, dtype=np.int32),
        "runs": np.zeros((40, 50), dtype=np.float64),
        "名前": sample("uint8", (3,)),
    }

    def entries():
        return [{"name": name, "type": TYPES[a.dtype.name], "shape": list(a.shape),
                 "values": values_of(a) if a.size <= 24 else None,
                 "count": int(a.size)} for name, a in arrays.items()]

    cases = [
        {"name": "savez", "hex": numpy_npz(arrays, False).hex(), "entries": entries()},
        {"name": "savez_compressed", "hex": numpy_npz(arrays, True).hex(), "entries": entries()},
    ]
    # ZIP64 records everywhere, as an archive above 4 GiB has them.
    limit, count = zipfile.ZIP64_LIMIT, zipfile.ZIP_FILECOUNT_LIMIT
    zipfile.ZIP64_LIMIT, zipfile.ZIP_FILECOUNT_LIMIT = 16, 0
    try:
        files = [(name + ".npy", npy_bytes(a)) for name, a in arrays.items()]
        cases.append({"name": "zip64", "hex": fixed_zip(files, zipfile.ZIP_DEFLATED).hex(), "entries": entries()})
    finally:
        zipfile.ZIP64_LIMIT, zipfile.ZIP_FILECOUNT_LIMIT = limit, count
    return cases


def npz_errors():
    plain = [("a.npy", npy_bytes(np.arange(4, dtype=np.float64)))]
    stored = fixed_zip(plain, zipfile.ZIP_STORED)
    packed = fixed_zip([("a.npy", npy_bytes(np.zeros(4000)))], zipfile.ZIP_DEFLATED)
    cases = []

    def add(name, data, error, stage):
        cases.append({"name": name, "hex": bytes(data).hex(), "error": error, "stage": stage})

    add("not_zip", b"hello world, this is not an archive", "InvalidMagic", "index")
    add("empty", b"", "InvalidMagic", "index")
    add("end_missing", stored[:-22], "InvalidArchive", "index")
    add("cut", stored[: len(stored) // 2], "InvalidArchive", "index")
    central = stored.find(b"PK\x01\x02")
    end = stored.find(b"PK\x05\x06")
    bad = bytearray(stored)
    bad[end + 16:end + 20] = struct.pack("<I", len(stored) + 5)
    add("directory_outside", bad, "InvalidArchive", "index")
    bad = bytearray(stored)
    bad[end + 10:end + 12] = struct.pack("<H", 60000)
    bad[end + 8:end + 10] = struct.pack("<H", 60000)
    add("count_too_large", bad, "InvalidArchive", "index")
    bad = bytearray(stored)
    bad[central + 42:central + 46] = struct.pack("<I", len(stored))
    add("local_header_outside", bad, "InvalidArchive", "index")
    bad = bytearray(stored)
    bad[central + 20:central + 24] = struct.pack("<I", 4000000)
    bad[central + 24:central + 28] = struct.pack("<I", 4000000)
    add("data_outside", bad, "InvalidArchive", "index")
    bad = bytearray(stored)
    bad[central + 8] |= 1
    add("encrypted", bad, "InvalidArchive", "index")
    bad = bytearray(stored)
    bad[central + 24:central + 28] = struct.pack("<I", 7)
    add("stored_sizes_differ", bad, "InvalidArchive", "index")
    # A deflated entry that claims far more than DEFLATE can expand its bytes to.
    central = packed.find(b"PK\x01\x02")
    bad = bytearray(packed)
    bad[central + 24:central + 28] = struct.pack("<I", 0xFFFFFF00)
    add("impossible_ratio", bad, "InvalidArchive", "index")
    with warnings.catch_warnings():
        warnings.simplefilter("ignore")
        add("duplicate_names", fixed_zip([("a.npy", plain[0][1]), ("a.npy", plain[0][1])], zipfile.ZIP_STORED), "InvalidArchive", "index")
    # The index is fine and the entry is not.
    local = stored.find(b"PK\x03\x04")
    data_start = local + 30 + struct.unpack("<H", stored[local + 26:local + 28])[0] + struct.unpack("<H", stored[local + 28:local + 30])[0]
    bad = bytearray(stored)
    bad[data_start + 130] ^= 0x40
    add("stored_crc", bad, "CorruptData", "entry")
    local = packed.find(b"PK\x03\x04")
    data_start = local + 30 + struct.unpack("<H", packed[local + 26:local + 28])[0] + struct.unpack("<H", packed[local + 28:local + 30])[0]
    bad = bytearray(packed)
    bad[data_start + 3] ^= 0x10
    add("deflate_corrupt", bad, "CorruptData|TruncatedData", "entry")
    central = packed.find(b"PK\x01\x02")
    bad = bytearray(packed)
    size = struct.unpack("<I", packed[central + 24:central + 28])[0]
    bad[central + 24:central + 28] = struct.pack("<I", size - 8)
    add("deflate_longer_than_claimed", bad, "CorruptData", "entry")
    bad = bytearray(packed)
    bad[central + 24:central + 28] = struct.pack("<I", size + 8)
    add("deflate_shorter_than_claimed", bad, "CorruptData", "entry")
    bzip = fixed_zip(plain, zipfile.ZIP_BZIP2)
    add("bzip2", bzip, "UnsupportedCompression", "entry")
    add("not_npy_inside", fixed_zip([("a.npy", b"this is not an array")], zipfile.ZIP_STORED), "InvalidMagic", "entry")
    add("missing_entry", fixed_zip([("b.npy", plain[0][1])], zipfile.ZIP_STORED), "EntryNotFound", "entry")
    return cases


def raw_deflate(data, level=6, strategy=zlib.Z_DEFAULT_STRATEGY):
    compressor = zlib.compressobj(level, zlib.DEFLATED, -15, 8, strategy)
    return compressor.compress(data) + compressor.flush()


def deflate_cases():
    samples = {
        "empty": b"",
        "one": b"a",
        "text": b"the quick brown fox jumps over the lazy dog. " * 12,
        "runs": b"\x00" * 600 + b"\x01" * 300 + b"ab" * 200,
        "random": bytes(random.getrandbits(8) for _ in range(700)),
        "far": bytes(random.getrandbits(8) for _ in range(64)) + b"q" * 33000,
    }
    samples["far"] += samples["far"][:64]
    cases = []
    for name, data in samples.items():
        streams = {
            "stored": raw_deflate(data, 0),
            "fixed": raw_deflate(data, 6, zlib.Z_FIXED),
            "dynamic": raw_deflate(data, 9),
            "huffman": raw_deflate(data, 6, zlib.Z_HUFFMAN_ONLY),
            "fast": raw_deflate(data, 1),
        }
        cases.append({
            "name": name,
            "length": len(data),
            "crc32": zlib.crc32(data),
            "hex": data.hex() if len(data) <= 2000 else None,
            "streams": {label: stream.hex() for label, stream in streams.items()},
        })
    return cases


def safetensors_file(tensors, metadata=None, pad=True):
    """A Safetensors file from the format description: 8 bytes of little-endian header length, a JSON
    header {name: {dtype, shape, data_offsets}}, and the buffer. `tensors` are (name, dtype, shape, bytes)."""
    header_fields = {}
    if metadata is not None:
        header_fields["__metadata__"] = metadata
    buffer = b""
    for name, dtype, shape, data in tensors:
        header_fields[name] = {"dtype": dtype, "shape": shape, "data_offsets": [len(buffer), len(buffer) + len(data)]}
        buffer += data
    text = json.dumps(header_fields, separators=(",", ":"), ensure_ascii=False).encode("utf-8")
    if pad:
        text += b" " * ((8 - len(text) % 8) % 8)
    return struct.pack("<Q", len(text)) + text + buffer


SAFE = {"bool": "BOOL", "uint8": "U8", "int8": "I8", "int16": "I16", "uint16": "U16", "float16": "F16",
        "int32": "I32", "uint32": "U32", "float32": "F32", "float64": "F64", "int64": "I64", "uint64": "U64"}


def safetensors_cases():
    arrays = [(f"t_{name}", sample(name, (2, 3))) for name in SAFE]
    arrays.append(("scalar", np.array(3.5, dtype=np.float32)))
    arrays.append(("empty", np.zeros((0, 4), dtype=np.float32)))
    arrays.append(("名前", sample("int16", (4,))))
    tensors = [(name, SAFE[a.dtype.name], list(a.shape), a.astype(a.dtype.newbyteorder("<")).tobytes()) for name, a in arrays]
    data = safetensors_file(tensors, {"format": "pt", "note": "é \"quoted\""})
    entries = [{"name": name, "type": TYPES[a.dtype.name], "shape": list(a.shape), "values": values_of(a)} for name, a in arrays]
    return [{"name": "all_dtypes", "hex": data.hex(), "entries": entries,
             "metadata": [["format", "pt"], ["note", "é \"quoted\""]]}]


def safetensors_errors():
    f4 = np.arange(4, dtype="<f4").tobytes()
    good = [("a", "F32", [4], f4)]
    cases = []

    def add(name, data, error, stage="index"):
        cases.append({"name": name, "hex": bytes(data).hex(), "error": error, "stage": stage})

    def raw(text, buffer=b""):
        body = text.encode("utf-8")
        return struct.pack("<Q", len(body)) + body + buffer

    add("empty", b"", "TruncatedData")
    add("short", b"\x01\x02\x03", "TruncatedData")
    add("header_past_end", struct.pack("<Q", 5000) + b"{}", "TruncatedData")
    add("header_huge", struct.pack("<Q", 2 ** 63 + 5) + b"{}", "InvalidHeader")
    add("header_above_100mb", struct.pack("<Q", 100000001) + b"{}", "InvalidHeader")
    add("not_json", raw("{not json}", f4), "InvalidHeader")
    add("not_object", raw("[1,2,3]", f4), "InvalidHeader")
    add("leading_space", raw(' {"a":{"dtype":"F32","shape":[4],"data_offsets":[0,16]}}', f4), "InvalidHeader")
    add("not_utf8", struct.pack("<Q", 4) + b"{\xff\xfe}", "InvalidHeader")
    add("duplicate_key", raw('{"a":{"dtype":"F32","shape":[4],"data_offsets":[0,16]},"a":{"dtype":"F32","shape":[4],"data_offsets":[0,16]}}', f4), "InvalidHeader")
    add("offset_past_end", raw('{"a":{"dtype":"F32","shape":[4],"data_offsets":[0,32]}}', f4), "InvalidHeader")
    add("offset_reversed", raw('{"a":{"dtype":"F32","shape":[4],"data_offsets":[16,0]}}', f4), "InvalidHeader")
    add("offset_negative", raw('{"a":{"dtype":"F32","shape":[4],"data_offsets":[-4,12]}}', f4), "InvalidHeader")
    add("offset_fraction", raw('{"a":{"dtype":"F32","shape":[4],"data_offsets":[0,16.5]}}', f4), "InvalidHeader")
    add("size_mismatch", raw('{"a":{"dtype":"F32","shape":[3],"data_offsets":[0,16]}}', f4), "InvalidHeader")
    add("overlap", raw('{"a":{"dtype":"F32","shape":[4],"data_offsets":[0,16]},"b":{"dtype":"F32","shape":[2],"data_offsets":[8,16]}}', f4), "InvalidHeader")
    add("hole", raw('{"a":{"dtype":"F32","shape":[2],"data_offsets":[0,8]},"b":{"dtype":"F32","shape":[1],"data_offsets":[12,16]}}', f4), "InvalidHeader")
    add("unused_tail", raw('{"a":{"dtype":"F32","shape":[2],"data_offsets":[0,8]}}', f4), "InvalidHeader")
    add("shape_overflow", raw('{"a":{"dtype":"F32","shape":[4611686018427387904,4611686018427387904],"data_offsets":[0,16]}}', f4), "ShapeOverflow")
    add("shape_negative", raw('{"a":{"dtype":"F32","shape":[-4],"data_offsets":[0,16]}}', f4), "InvalidHeader")
    add("missing_dtype", raw('{"a":{"shape":[4],"data_offsets":[0,16]}}', f4), "InvalidHeader")
    add("tensor_not_object", raw('{"a":[1,2]}', f4), "InvalidHeader")
    add("metadata_not_text", raw('{"__metadata__":{"k":1},"a":{"dtype":"F32","shape":[4],"data_offsets":[0,16]}}', f4), "InvalidHeader")
    add("too_many_axes", raw('{"a":{"dtype":"F32","shape":[' + ",".join(["1"] * 65) + '],"data_offsets":[0,4]}}', f4[:4]), "InvalidHeader")
    add("half_precision_brain", safetensors_file([("a", "BF16", [8], f4)]), "UnsupportedDtype", "entry")
    add("wrong_type", safetensors_file(good), "DtypeMismatch", "entry")
    add("missing_entry", safetensors_file([("b", "F32", [4], f4)]), "EntryNotFound", "entry")
    return cases


def csv_cases():
    table = np.array([[1.5, -2.25, 3.0], [0.1, 1e-300, 6.02e23]])
    cases = []

    def add(name, array, separator, comment, skip, **options):
        buffer = io.StringIO()
        np.savetxt(buffer, array, **options)
        text = buffer.getvalue()
        loaded = np.loadtxt(io.StringIO(text), delimiter=options.get("delimiter", None), skiprows=skip, ndmin=2,
                            dtype=array.dtype)
        cases.append({"name": name, "text": text, "delimiter": separator, "comment": comment, "skip_rows": skip,
                      "type": TYPES[loaded.dtype.name], "shape": list(loaded.shape), "values": values_of(loaded)})

    add("default", table, "", "#", 0)
    add("comma", table, ",", "#", 0, delimiter=",")
    add("tab_header", table, "\t", "#", 0, delimiter="\t", header="x\ty\tz", footer="end")
    add("skip_header", table, ",", "", 1, delimiter=",", header="x,y,z", comments="")
    add("vector", np.array([1.0, 2.0, 3.5]), ",", "#", 0, delimiter=",")
    add("integers", np.array([[1, -2], [300, 4]], dtype=np.int64), ",", "#", 0, delimiter=",", fmt="%d")
    special = {"name": "special_spellings", "text": "nan,inf,-inf,NaN\nInfinity,-Infinity,+1.5,1E3\r\n", "delimiter": ",",
               "comment": "#", "skip_rows": 0, "type": "f64", "shape": [2, 4], "values": None,
               "bits": [format(struct.unpack("<Q", struct.pack("<d", float(v)))[0], "d") for v in
                        ["nan", "inf", "-inf", "nan", "inf", "-inf", "1.5", "1000"]]}
    cases.append(special)
    return cases


def main():
    reference = {
        "numpy": np.__version__,
        "python": sys.version.split()[0],
        "npy": npy_cases(),
        "views": view_cases(),
        "npy_errors": npy_errors(),
        "npz": npz_cases(),
        "npz_errors": npz_errors(),
        "deflate": deflate_cases(),
        "safetensors": safetensors_cases(),
        "safetensors_errors": safetensors_errors(),
        "csv": csv_cases(),
    }
    path = os.path.join(HERE, "reference.json")
    with open(path, "w", encoding="utf-8") as output:
        json.dump(reference, output, ensure_ascii=False, indent=1)
        output.write("\n")
    print(path, os.path.getsize(path), "bytes")


if __name__ == "__main__":
    main()
