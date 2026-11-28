import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, truncateSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { inflateRawSync } from "node:zlib";

import { crc32, npyFile } from "./tensor-codec-cases.mjs";

// E2E for `TensorIO` and, through real files, `TensorCodec` (NC11). The program of tests/fixtures/tensor_io runs as
// a native executable at -O0 and -O3 and as a WASI module:
//
// 1. it reads and maps the files that NumPy wrote (tests/fixtures/tensor_codec/reference.json holds their bytes;
//    generate.py beside it wrote them), and this suite computes the expected elements from the same formulas;
// 2. it writes files, which this suite compares byte for byte with the .npy format, reads with its own ZIP and
//    Safetensors readers and Node's zlib, and, when a Python with NumPy is found, loads with NumPy itself
//    (`np.load`, `np.loadtxt`, `zlib`). TSUZURI_PYTHON names the interpreter; without one that stage is skipped
//    and the suite says so.
const compiler = resolve(process.argv[2] ?? "target/release/tsuzuri");
const root = mkdtempSync(join(tmpdir(), "tsuzuri-tensor-io-"));
const project = resolve("tests/fixtures/tensor_io");
const reference = JSON.parse(readFileSync(resolve("tests/fixtures/tensor_codec/reference.json"), "utf8"));
const windows = process.platform === "win32";
const optimizations = ["-O0", "-O3"];
let checks = 0;

function execute(program, args, options = {}, success = true) {
  const result = spawnSync(program, args, { encoding: "utf8", timeout: 600_000, maxBuffer: 64 * 1024 * 1024, ...options });
  assert.ifError(result.error);
  if (success) assert.equal(result.status, 0, `${program} ${args.join(" ")}\n${result.stdout}\n${result.stderr}`);
  return result;
}

/** The `key=value` lines of a run. */
function report(result) {
  const lines = new Map();
  for (const line of result.stdout.split(/\r?\n/)) {
    const at = line.indexOf("=");
    if (at > 0) lines.set(line.slice(0, at), line.slice(at + 1));
  }
  return lines;
}

/** A text with every number in the spelling of JavaScript, so that `3`, `3.0` and `3e0` compare equal. */
const canon = (text) => text.replace(/(?<![A-Za-z_\d.])-?(?:inf|nan|\d+(?:\.\d+)?(?:e[+-]?\d+)?)(?![A-Za-z_\d])/g, (token) => {
  if (token === "inf") return "Infinity";
  if (token === "-inf") return "-Infinity";
  if (token === "nan") return "NaN";
  return Object.is(Number(token), -0) ? "-0" : String(Number(token));
});

function expectLines(actual, expected, label) {
  for (const [key, value] of Object.entries(expected)) {
    assert.ok(actual.has(key), `${label}: no line ${key}\n${[...actual].map(([k, v]) => `${k}=${v}`).join("\n")}`);
    assert.equal(canon(actual.get(key)), canon(value), `${label}: ${key}`);
    checks++;
  }
}

const list = (values) => `[${values.join(", ")}]`;
const raw = (typed) => new Uint8Array(typed.buffer, typed.byteOffset, typed.byteLength);
const same = (left, right) => left.length === right.length && Buffer.compare(Buffer.from(left), Buffer.from(right)) === 0;

// ---- the arrays of both directions, from their formulas ----
const weightsIn = Array.from({ length: 12 }, (_, index) => index * 0.5 - 1);
const weightsOut = Array.from({ length: 12 }, (_, index) => index * 0.1 - 1.5);
const intsOut = Array.from({ length: 5 }, (_, index) => index * 7 - 3);
const labelsOut = Array.from({ length: 4 }, (_, index) => BigInt(index * index - 5));
const half = (value) => {
  const bits = new Uint32Array(new Float32Array([value]).buffer)[0];
  const exponent = (bits >>> 23) & 0xff;
  return exponent === 0 ? (bits >>> 16) & 0x8000 : ((bits >>> 16) & 0x8000) | ((exponent - 112) << 10) | ((bits >>> 13) & 0x3ff);
};
const pattern = Uint8Array.from({ length: 70000 }, (_, index) => (index * index + Math.floor(index / 7)) % 251);
const table = [[1.5, -2.25], [1024, 0.1], [Infinity, 3e-300]];

const expectedFiles = {
  "out_f64.npy": npyFile("<f8", [3, 4], raw(new Float64Array(weightsOut))),
  "out_permuted.npy": npyFile("<f8", [4, 3], raw(new Float64Array(Array.from({ length: 12 }, (_, position) => weightsOut[(position % 3) * 4 + Math.floor(position / 3)])))),
  "out_i32.npy": npyFile("<i4", [5], raw(new Int32Array(intsOut))),
  "out_bool.npy": npyFile("|b1", [2, 2], new Uint8Array([1, 0, 1, 0])),
  "out_c64.npy": npyFile("<c8", [3], raw(new Float32Array([0, 0, 0.5, -1, 1, -2]))),
  "out_f16.npy": npyFile("<f2", [4], raw(new Uint16Array([0, 0.25, 0.5, 0.75].map(half)))),
  "out_u8_raw.npy": npyFile("|u1", [3], new Uint8Array([1, 2, 3])),
};
const archiveEntries = {
  "weights.npy": expectedFiles["out_f64.npy"],
  "labels.npy": npyFile("<i8", [4], raw(new BigInt64Array(labelsOut))),
  "flags.npy": expectedFiles["out_bool.npy"],
  "名前.npy": expectedFiles["out_i32.npy"],
};

/** The entries of a ZIP archive, read from its central directory (with the ZIP64 records), each checked against its
 *  CRC-32 and its sizes. */
function zipEntries(bytes) {
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  const u64 = (at) => Number(view.getBigUint64(at, true));
  let end = bytes.length - 22;
  while (end >= 0 && view.getUint32(end, true) !== 0x06054b50) end--;
  assert.ok(end >= 0, "no end of central directory");
  let count = view.getUint16(end + 10, true), directory = view.getUint32(end + 16, true), wide = false;
  if (end >= 20 && view.getUint32(end - 20, true) === 0x07064b50) {
    const record = u64(end - 12);
    assert.equal(view.getUint32(record, true), 0x06064b50);
    count = u64(record + 32);
    directory = u64(record + 48);
    wide = true;
  }
  const entries = new Map();
  let at = directory;
  for (let index = 0; index < count; index++) {
    assert.equal(view.getUint32(at, true), 0x02014b50);
    const method = view.getUint16(at + 10, true), crc = view.getUint32(at + 16, true);
    let packed = view.getUint32(at + 20, true), size = view.getUint32(at + 24, true), local = view.getUint32(at + 42, true);
    const nameLength = view.getUint16(at + 28, true), extraLength = view.getUint16(at + 30, true);
    const name = Buffer.from(bytes.subarray(at + 46, at + 46 + nameLength)).toString("utf8");
    // The time stamp is 1980-01-01 00:00:00, so that the archive is the same bytes every time.
    assert.deepEqual([view.getUint16(at + 12, true), view.getUint16(at + 14, true)], [0, 33], name);
    for (let field = at + 46 + nameLength; field < at + 46 + nameLength + extraLength; field += 4 + view.getUint16(field + 2, true)) {
      if (view.getUint16(field, true) !== 1) continue;
      let value = field + 4;
      if (size === 0xffffffff) { size = u64(value); value += 8; }
      if (packed === 0xffffffff) { packed = u64(value); value += 8; }
      if (local === 0xffffffff) local = u64(value);
    }
    assert.equal(view.getUint32(local, true), 0x04034b50, name);
    const data = local + 30 + view.getUint16(local + 26, true) + view.getUint16(local + 28, true);
    const stored = bytes.subarray(data, data + packed);
    const plain = method === 8 ? new Uint8Array(inflateRawSync(stored)) : stored;
    assert.ok(method === 0 || method === 8, name);
    assert.equal(plain.length, size, name);
    assert.equal(crc32(plain), BigInt(crc), name);
    entries.set(name, { plain, method });
    at += 46 + nameLength + extraLength + view.getUint16(at + 32, true);
  }
  return { entries, wide };
}

/** The header and the buffer of a Safetensors file: 8 bytes of length, JSON, the bytes of the tensors. */
function safetensors(bytes) {
  const length = Number(new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength).getBigUint64(0, true));
  assert.equal((8 + length) % 8, 0, "the header is padded to a multiple of 8 bytes");
  const text = Buffer.from(bytes.subarray(8, 8 + length)).toString("utf8");
  assert.equal(text[0], "{");
  return { header: JSON.parse(text), buffer: bytes.subarray(8 + length) };
}

function checkArchive(path, compressed, wide) {
  const { entries, wide: found } = zipEntries(new Uint8Array(readFileSync(path)));
  assert.equal(found, wide, path);
  assert.deepEqual([...entries.keys()], Object.keys(archiveEntries), path);
  for (const [name, expected] of Object.entries(archiveEntries)) {
    assert.ok(same(entries.get(name).plain, expected), `${path}: ${name}`);
    assert.equal(entries.get(name).method, compressed ? 8 : 0, `${path}: ${name}`);
    checks++;
  }
}

function checkWritten(directory, label) {
  for (const [name, expected] of Object.entries(expectedFiles)) {
    assert.ok(same(new Uint8Array(readFileSync(join(directory, name))), expected), `${label}: ${name} is not the bytes that np.save writes`);
    checks++;
  }
  checkArchive(join(directory, "out.npz"), false, false);
  checkArchive(join(directory, "out_compressed.npz"), true, false);
  checkArchive(join(directory, "out_zip64.npz"), true, true);
  for (const [name, metadata] of [["out_plain.safetensors", undefined], ["out.safetensors", { format: "pt", note: "é \"quoted\"" }]]) {
    const { header, buffer } = safetensors(new Uint8Array(readFileSync(join(directory, name))));
    // The larger element types come first, so that every tensor is aligned: i64, then f32, then bool.
    assert.deepEqual(header, {
      ...(metadata ? { __metadata__: metadata } : {}),
      b: { dtype: "I64", shape: [4], data_offsets: [0, 32] },
      w: { dtype: "F32", shape: [2, 2], data_offsets: [32, 48] },
      flags: { dtype: "BOOL", shape: [2, 2], data_offsets: [48, 52] },
    }, name);
    assert.deepEqual(Object.keys(header), [...(metadata ? ["__metadata__"] : []), "b", "w", "flags"], name);
    const expected = Buffer.concat([raw(new BigInt64Array(labelsOut)), raw(new Float32Array([0.5, 1.5, 2.5, 3.5])), new Uint8Array([1, 0, 1, 0])].map((part) => Buffer.from(part)));
    assert.ok(same(buffer, expected), name);
    checks++;
  }
  for (const [name, separator] of [["out.csv", ","], ["out.tsv", "\t"]]) {
    const text = readFileSync(join(directory, name), "utf8");
    assert.ok(text.endsWith("\n") && !text.includes("\r"), name);
    const rows = text.trimEnd().split("\n").map((line) => line.split(separator).map((field) => (field === "inf" ? Infinity : Number(field))));
    assert.deepEqual(rows, table, name);
    checks++;
  }
  assert.equal(readFileSync(join(directory, "out_spaced.txt"), "utf8"), `${intsOut.join("\n")}\n`);
  assert.ok(same(new Uint8Array(inflateRawSync(readFileSync(join(directory, "out.deflate")))), pattern), "out.deflate");
  assert.ok(!existsSync(join(directory, "out_cube.csv")), "a tensor of rank 3 has no table form");
  checks += 3;
}

// What NumPy itself reads from the files that Tsuzuri wrote.
const pythonCheck = String.raw`
import json, sys, zlib
import numpy as np

d = sys.argv[1]
weights = (np.arange(12) * 0.1 - 1.5).reshape(3, 4)
ints = (np.arange(5) * 7 - 3).astype(np.int32)
labels = (np.arange(4) ** 2 - 5).astype(np.int64)
flags = np.array([[True, False], [True, False]])
done = 0

def same(name, got, want):
    global done
    assert got.dtype == want.dtype, (name, got.dtype, want.dtype)
    assert got.shape == want.shape, (name, got.shape, want.shape)
    assert np.array_equal(got, want), (name, got, want)
    done += 1

same("out_f64.npy", np.load(d + "/out_f64.npy"), weights)
same("out_permuted.npy", np.load(d + "/out_permuted.npy"), np.ascontiguousarray(weights.T))
same("out_i32.npy", np.load(d + "/out_i32.npy"), ints)
same("out_bool.npy", np.load(d + "/out_bool.npy"), flags)
same("out_c64.npy", np.load(d + "/out_c64.npy"), (np.arange(3) * 0.5 - 1j * np.arange(3)).astype(np.complex64))
same("out_f16.npy", np.load(d + "/out_f16.npy"), (np.arange(4) * 0.25).astype(np.float16))
same("out_u8_raw.npy", np.load(d + "/out_u8_raw.npy"), np.array([1, 2, 3], dtype=np.uint8))
for name in ["out.npz", "out_compressed.npz", "out_zip64.npz"]:
    with np.load(d + "/" + name) as archive:
        assert sorted(archive.files) == sorted(["weights", "labels", "flags", "名前"]), archive.files
        same(name, archive["weights"], weights)
        same(name, archive["labels"], labels)
        same(name, archive["flags"], flags)
        same(name, archive["名前"], ints)
table = np.array([[1.5, -2.25], [1024, 0.1], [np.inf, 3e-300]])
same("out.csv", np.loadtxt(d + "/out.csv", delimiter=","), table)
same("out.tsv", np.loadtxt(d + "/out.tsv", delimiter="\t"), table)
same("out_spaced.txt", np.loadtxt(d + "/out_spaced.txt", dtype=np.int32), ints)
plain = zlib.decompress(open(d + "/out.deflate", "rb").read(), -15)
assert plain == bytes((i * i + i // 7) % 251 for i in range(70000))
print(json.dumps({"numpy": np.__version__, "checks": done + 1}))
`;

function findPython() {
  for (const candidate of [process.env.TSUZURI_PYTHON, "python3", "python"].filter(Boolean)) {
    const probe = spawnSync(candidate, ["-I", "-c", "import numpy; print(numpy.__version__)"], { encoding: "utf8" });
    if (probe.status === 0) return { program: candidate, version: probe.stdout.trim() };
  }
  return null;
}

try {
  const python = findPython();
  const script = join(root, "check.py");
  writeFileSync(script, pythonCheck);

  // The files that NumPy wrote, and one that is too large to read: 4 GiB of zeros that take no room on the disk.
  const inputs = join(root, "inputs");
  mkdirSync(inputs);
  for (const [name, hex] of Object.entries(reference.io)) writeFileSync(join(inputs, name), Buffer.from(hex, "hex"));
  const expectedRead = {
    "in_f64.npy": `[3, 4] 21 2`,
    "in_fortran.npy": list(weightsIn),
    "in_be.npy": "[1, -2, 3, 400000]",
    "in_be.npy raw": "int32 [4] [1, 0, 0, 0, 254, 255, 255, 255, 3, 0, 0, 0, 128, 26, 6, 0]",
    "wrong type": "codec:DtypeMismatch",
    missing: "io",
    "not npy": "codec:InvalidMagic",
    "in.npz": `["weights", "labels"] 21 [4, -5, 6] []`,
    "in_compressed.npz": `["weights", "labels"] 21 [4, -5, 6] []`,
    "in.safetensors": `["w", "labels"] no entry named weights [4, -5, 6] [("format", "pt")]`,
    "in.csv": `[3, 4] ${list(weightsIn)}`,
    "in_default.txt": `[3, 4] ${list(weightsIn)}`,
    "not text": "codec:other",
  };
  const expectedMapped = {
    "in_f64.npy": `float64 [3, 4] ${list(weightsIn)}`,
    "in_fortran.npy": `float64 [3, 4] ${list(weightsIn)}`,
    "in_i32.npy": "int32 [2, 3] [-3, 4, 11, 18, 25, 32]",
    "in_bool.npy": "bool [2, 2] [true, false, false, true]",
    "in_c128.npy": "complex128 [2] [1, 2, -0.5, 0.25]",
    "in_f16.npy": "float16 [3] [0.5, -1.25, 3]",
    "in_u8.npy": "uint8 [3] [1, 2, 255]",
    "in_empty.npy": "float64 [0, 3] []",
    "in_scalar.npy": "float64 [] [2.5]",
    "as f64": `float64 [3, 4] ${list(weightsIn)}`,
    "as f32": "codec:DtypeMismatch",
    "safetensors w": "float32 [2, 2] [0.5, 1.5, 2.5, 3.5]",
    "safetensors labels": "int64 [3] [4, -5, 6]",
    "safetensors absent": "codec:EntryNotFound",
    with: "21",
    // A file that is valid but cannot be used where it is, and files and paths that are not valid.
    "in_be.npy": "unmappable",
    "in_bad_bool.npy": "unmappable",
    "in_unaligned.npy": "unmappable",
    "in_cut.npy": "codec:TruncatedData",
    "in_zero_bytes.npy": "codec:InvalidMagic",
    missing: "io",
    directory: "io",
    // A FIFO without a writer on POSIX (the open must not wait for one); the path does not exist elsewhere.
    pipe: "io",
  };
  if (!windows) {
    const made = spawnSync("mkfifo", [join(inputs, "pipe.npy")], { encoding: "utf8" });
    assert.equal(made.status, 0, `mkfifo: ${made.stderr}`);
    checks += 1;
  }

  for (const optimization of optimizations) {
    const executable = join(root, `tensor_io${optimization}${windows ? ".exe" : ""}`);
    const built = execute(compiler, ["build", project, optimization, "-o", executable]);
    // The C compiler says nothing about the code of the std modules (an unroll hint that it cannot honor is
    // `-Wpass-failed`). The MSVC headers add deprecation warnings for the runtime's own C on Windows, which are
    // not about the program.
    const warnings = built.stderr.split(/\r?\n/).filter((line) => /warning:/.test(line) && (!windows || /pass-failed|loop not/.test(line)));
    assert.deepEqual(warnings, [], built.stderr);

    // 1. Tsuzuri reads what NumPy wrote.
    expectLines(report(execute(executable, ["read", inputs])), expectedRead, `read ${optimization}`);

    // 2. Tsuzuri maps what NumPy wrote, without copying it, and refuses what cannot be used in place.
    expectLines(report(execute(executable, ["mmap", inputs])), expectedMapped, `mmap ${optimization}`);

    // 3. A window of another element type than the file's is a trap, before any element is read.
    const trapped = execute(executable, ["trap", inputs], {}, false);
    assert.notEqual(trapped.status, 0, `${trapped.stdout}\n${trapped.stderr}`);
    assert.ok(trapped.stdout.includes("before") && !trapped.stdout.includes("after"), trapped.stdout);
    checks++;

    // 4. What Tsuzuri writes: the bytes of np.save, archives that an independent reader takes apart, and tables.
    const outputs = join(root, `outputs${optimization}`);
    mkdirSync(outputs);
    const written = report(execute(executable, ["write", outputs]));
    expectLines(written, {
      ...Object.fromEntries([...Object.keys(expectedFiles), "out.npz", "out_compressed.npz", "out_zip64.npz", "out_plain.safetensors",
        "out.safetensors", "out.csv", "out.tsv", "out_spaced.txt", "out.deflate"].map((name) => [name, "ok"])),
      "out_cube.csv": "codec:other",
      nowhere: "io",
    }, `write ${optimization}`);
    checkWritten(outputs, `write ${optimization}`);
    if (python) {
      const loaded = JSON.parse(execute(python.program, ["-I", script, outputs]).stdout);
      assert.equal(loaded.checks, 23);
      checks += loaded.checks;
    }

    // 5. Dropping a mapped tensor releases its mapping: 100,000 mappings of 4 GiB (400 TiB) only fit in the
    // address space one after another. NTFS would allocate the file, so Windows only runs the loop on a small one.
    const large = join(root, "large");
    mkdirSync(large, { recursive: true });
    const size = windows ? 65536 : 2 ** 32;
    const big = join(large, "big.npy");
    if (!existsSync(big)) {
      const header = npyFile("|u1", [size], new Uint8Array(0));
      writeFileSync(big, header);
      truncateSync(big, header.length + size);
    }
    const count = windows ? 2000 : 100000;
    expectLines(report(execute(executable, ["leak", large, String(count)])), { leak: `${count} 0` }, `leak ${optimization}`);
  }

  // 6. The part of the runtime behind the mappings is compiled only into a program that reaches it (as M15's
  // Env.executable_path): a program that only reads and writes files does not name it.
  const plain = join(root, "plain");
  mkdirSync(plain);
  writeFileSync(join(plain, "Main.tz"), `let t = Tensor.of_array [2] [1.0, 2.0]
let window = Tensor.view (ref t)
let! saved = TensorIO.save_npy "x.npy" (ref window)
let! loaded = TensorIO.load_npy_raw "x.npy"
match (saved, loaded) with
| (Result.Ok _, Result.Ok raw) -> IO.writeln raw.data.length
| _ -> IO.writeln "failed"
`);
  const ir = join(root, "plain.ll");
  execute(compiler, ["build", plain, "--emit", "llvm", "-o", ir]);
  const text = readFileSync(ir, "utf8");
  assert.ok(/@tsuzuri_os_read/.test(text) && !/@tsuzuri_os_map|@tsuzuri_os_unmap|__mmap_slice/.test(text), "a program without a mapping names no mapping primitive");
  // The default WebAssembly target has no host: TensorIO is refused there like File, and TensorCodec is not.
  const hostless = execute(compiler, ["build", plain, "--target", "wasm32", "-o", join(root, "plain.wasm")], {}, false);
  assert.ok(hostless.status !== 0 && hostless.stderr.includes("E2000") && hostless.stderr.includes("--wasm-host wasi"), hostless.stderr);
  checks += 2;

  // 7. The same program as a WASI module: files are read and written as on native, and nothing can be mapped.
  const wasiHost = resolve("tests/os-wasi-host.mjs");
  for (const optimization of optimizations) {
    const module = join(root, `tensor_io${optimization}.wasm`);
    execute(compiler, ["build", project, "--target", "wasm32", "--wasm-host", "wasi", optimization, "-o", module]);
    const outputs = join(root, `wasi${optimization}`);
    mkdirSync(outputs);
    for (const [name, hex] of Object.entries(reference.io)) writeFileSync(join(outputs, name), Buffer.from(hex, "hex"));
    const runWasi = (mode) => execute(process.execPath, ["--no-warnings", wasiHost, module, "/work", outputs, mode, "/work"]);
    expectLines(report(runWasi("read")), expectedRead, `wasi read ${optimization}`);
    const mapped = report(runWasi("mmap"));
    expectLines(mapped, Object.fromEntries(Object.keys(expectedMapped).map((key) => [key, key.startsWith("safetensors") || key.startsWith("in_") || key === "with" || key.startsWith("as ") || key === "missing" || key === "directory" || key === "pipe" ? "io" : expectedMapped[key]])), `wasi mmap ${optimization}`);
    const written = report(runWasi("write"));
    expectLines(written, { "out_f64.npy": "ok", "out.npz": "ok", "out.safetensors": "ok", "out.csv": "ok", nowhere: "io" }, `wasi write ${optimization}`);
    checkWritten(outputs, `wasi write ${optimization}`);
  }

  console.log(`tensor_io: ${checks} checks passed (native and WASI at -O0 and -O3)${python ? `, NumPy ${python.version} loaded every file that Tsuzuri wrote` : ""}`);
  if (!python) console.log("tensor_io: no Python with NumPy was found (TSUZURI_PYTHON, python3, python): the stage that loads Tsuzuri's files with NumPy was skipped; the byte comparisons and the independent readers ran");
} finally {
  rmSync(root, { recursive: true, force: true });
}
