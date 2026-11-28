// NC11: the cases of the suite `tensor_codec` of tests/features.mjs (fixture tests/fixtures/tensor_codec/Main.tz).
// Every expected value is computed here, from the format of an .npy file and from the definitions of the
// fixture, never from Tsuzuri. tests/tensor_codec.rs compares the same functions with what NumPy and zlib write.

const TABLE = new Uint32Array(256).map((_, index) => {
  let value = index;
  for (let bit = 0; bit < 8; bit++) value = value & 1 ? 0xedb88320 ^ (value >>> 1) : value >>> 1;
  return value >>> 0;
});

/** The CRC-32 of ZIP, PNG and zlib. */
export function crc32(bytes) {
  let value = 0xffffffff;
  for (const byte of bytes) value = TABLE[(value ^ byte) & 255] ^ (value >>> 8);
  return BigInt((value ^ 0xffffffff) >>> 0);
}

/** An .npy file of format 1.0 as NumPy's `np.save` writes it: the dictionary, spare room for the first extent
 *  (21 characters in all), spaces up to a multiple of 64 bytes, a line feed, and the little-endian elements. */
export function npyFile(descr, shape, data) {
  const tuple = shape.length === 1 ? `${shape[0]},` : shape.join(", ");
  let header = `{'descr': '${descr}', 'fortran_order': False, 'shape': (${tuple}), }`;
  if (shape.length > 0) header += " ".repeat(21 - String(shape[0]).length);
  const body = header.length + 1;
  const padding = 64 - ((10 + body) % 64);
  const text = `${header}${" ".repeat(padding)}\n`;
  const bytes = new Uint8Array(10 + text.length + data.length);
  bytes.set([0x93, 0x4e, 0x55, 0x4d, 0x50, 0x59, 1, 0, text.length & 255, text.length >> 8]);
  bytes.set(Buffer.from(text, "latin1"), 10);
  bytes.set(data, 10 + text.length);
  return bytes;
}

const raw = (typed) => new Uint8Array(typed.buffer, typed.byteOffset, typed.byteLength);

/** The bits of a binary16 number that a `f32` holds exactly. */
function half(value) {
  const bits = new Uint32Array(new Float32Array([value]).buffer)[0];
  const sign = (bits >>> 16) & 0x8000;
  const exponent = (bits >>> 23) & 0xff;
  if (exponent === 0) return sign;
  return sign | ((exponent - 127 + 15) << 10) | ((bits >>> 13) & 0x3ff);
}

const index12 = Array.from({ length: 12 }, (_, index) => index);
const wide = (index) => BigInt(index) * 1234567890123456789n;
const f64Element = (index) => index * 0.1 - 1.5;

// The element `i` of each dtype number of the fixture's `npy_crc`, and its NumPy type string.
const DTYPES = [
  ["|b1", () => new Uint8Array(index12.map((index) => (index % 3 === 0 ? 1 : 0)))],
  ["|i1", () => raw(new Int8Array(index12.map((index) => index * 37 - 100)))],
  ["<i2", () => raw(new Int16Array(index12.map((index) => index * 3700 - 10000)))],
  ["<i4", () => raw(new Int32Array(index12.map((index) => index * 370000000 - 1000000000)))],
  ["<i8", () => raw(new BigInt64Array(index12.map((index) => BigInt.asIntN(64, wide(index) - 5n))))],
  ["|u1", () => new Uint8Array(index12.map((index) => index * 37))],
  ["<u2", () => raw(new Uint16Array(index12.map((index) => index * 3700)))],
  ["<u4", () => raw(new Uint32Array(index12.map((index) => index * 370000000)))],
  ["<u8", () => raw(new BigUint64Array(index12.map((index) => BigInt.asUintN(64, wide(index)))))],
  ["<f2", () => raw(new Uint16Array(index12.map((index) => half(index * 0.25 - 1.5))))],
  ["<f4", () => raw(new Float32Array(index12.map((index) => index * 0.25 - 1.5)))],
  ["<f8", () => raw(new Float64Array(index12.map(f64Element)))],
  ["<c8", () => raw(new Float32Array(index12.flatMap((index) => [index * 0.5, 0 - index])))],
  ["<c16", () => raw(new Float64Array(index12.flatMap((index) => [index * 0.1, 0 - index])))],
];

// The windows of `npy_window_crc` over the 3 by 4 tensor: [shape, the row-major positions that it shows].
const WINDOWS = [
  [[4, 3], Array.from({ length: 12 }, (_, position) => (position % 3) * 4 + Math.floor(position / 3))],
  [[3, 4], Array.from({ length: 12 }, (_, position) => (2 - Math.floor(position / 4)) * 4 + (position % 4))],
  [[3, 2], Array.from({ length: 6 }, (_, position) => Math.floor(position / 2) * 4 + (position % 2) * 2)],
];

const pattern = (count) => Uint8Array.from({ length: count }, (_, index) => (index * index + Math.floor(index / 7)) % 251);
const sumBelow = (count) => BigInt(Array.from({ length: count }, (_, index) => index - 3).reduce((sum, value) => sum + value, 0));
// b"".join(b"%d," % (i * i) for i in range(70)) in Python.
const squares = Buffer.from(Array.from({ length: 70 }, (_, index) => `${index * index},`).join(""), "latin1");

export const cases = [
  ...DTYPES.map(([descr, data], kind) => ["npy_crc", [BigInt(kind)], crc32(npyFile(descr, [3, 4], data()))]),
  ["npy_crc", [14n], -1n],
  ...WINDOWS.map(([shape, positions], kind) => ["npy_window_crc", [BigInt(kind)],
    crc32(npyFile("<f8", shape, raw(new Float64Array(positions.map(f64Element)))))]),
  ...[0, 1, 10, 1000].map((count) => ["npy_round_trip", [BigInt(count)], sumBelow(count)]),
  // InvalidMagic 1, TruncatedData 6, UnsupportedVersion 2, InvalidHeader 3, DtypeMismatch 4, CorruptData 12,
  // CsvParseError 10, UnsupportedRank 13.
  ...[1, 6, 2, 3, 4, 1, 3, 3, 12, 12, 10, 13].map((error, kind) => ["hostile", [BigInt(kind)], BigInt(error)]),
  ["hostile", [12n], -1n],
  ...[0, 1, 2, 100, 5000, 70000].map((count) => ["deflate_round_trip", [BigInt(count)], crc32(pattern(count))]),
  ...[0, 1, 2].map((kind) => ["inflate_zlib", [BigInt(kind)], crc32(squares)]),
  // Seven checks hold, and the archive has five names and no metadata.
  ...[[0, 0], [1, 0], [0, 1], [1, 1]].map(([compress, zip64]) => ["npz_round_trip", [BigInt(compress), BigInt(zip64)], 7005n]),
  ["safetensors_round_trip", [0n], 7n],
  // (1.5 - 2.25 + 1024 + 0.125 - 0 + 3e10) * 8
  ...[0, 1, 2].map((kind) => ["csv_round_trip", [BigInt(kind)], 240000008187n]),
];
