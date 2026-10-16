import assert from "node:assert/strict";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath, pathToFileURL } from "node:url";

// The WASM heap runtime alone: src/runtime/heap-wasm.ll (wasm32) and heap-wasm64.ll (wasm64) are
// compiled with three wrappers and no compiler build, so the reference values below come from the
// design of the heap (16-byte header, capacity = request + 16 rounded up to 16) and from arithmetic,
// never from what a build prints.
//   node tests/wasm_heap.mjs [wasm32|wasm64|both]     (wasm64 needs Node.js 24 or newer)
const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const clang = process.env.TSUZURI_CLANG ?? "clang";
const wasmLd = process.env.TSUZURI_WASM_LD ?? "wasm-ld";
export const LIMIT = 16 * 1024 * 1024;
export const MAX_REQUEST = LIMIT - 32;

const WRAPPERS = `
define ptr @allocate(i64 %size) { %value = call ptr @tz.alloc(i64 %size) ret ptr %value }
define ptr @resize(ptr %old, i64 %previous, i64 %size) { %value = call ptr @tz.realloc(ptr %old, i64 %previous, i64 %size) ret ptr %value }
define void @release(ptr %value) { call void @tz.free(ptr %value) ret void }
`;

// An empty module with one 64-bit memory.
export const memory64Supported = () => WebAssembly.validate(new Uint8Array([0, 97, 115, 109, 1, 0, 0, 0, 5, 3, 1, 4, 0]));

function execute(program, args) {
  const result = spawnSync(program, args, { encoding: "utf8", timeout: 180_000 });
  if (result.error) throw result.error;
  assert.equal(result.status, 0, `${program} ${args.join(" ")}\n${result.stdout}\n${result.stderr}`);
}

// Compiles the heap of `target` with the wrappers (and `extra` IR that may read the heap's own
// globals) and links a module that exports allocate, resize, release, __heap_base and memory.
export function buildHeap({ target = "wasm32", optimization = 3, directory, extra = "", exports = [], bulkMemory = false }) {
  const heap = readFileSync(join(root, "src/runtime", target === "wasm64" ? "heap-wasm64.ll" : "heap-wasm.ll"), "utf8");
  const stem = join(directory, `heap-${target}-${optimization}`);
  writeFileSync(`${stem}.ll`, `declare void @llvm.trap()\n${heap}\n${WRAPPERS}\n${extra}`);
  execute(clang, [`--target=${target}`, `-O${optimization}`, ...(bulkMemory ? ["-mbulk-memory"] : []), "-Wno-override-module", "-c", `${stem}.ll`, "-o", `${stem}.o`]);
  execute(wasmLd, [
    ...(target === "wasm64" ? ["-mwasm64"] : []), `${stem}.o`, "--no-entry", "--export=allocate", "--export=resize", "--export=release",
    "--export=__heap_base", "--export-memory", `--max-memory=${LIMIT}`, ...exports.map((name) => `--export=${name}`), "-o", `${stem}.wasm`,
  ]);
  return new WebAssembly.Module(readFileSync(`${stem}.wasm`));
}

// A fresh instance whose addresses are Numbers for both targets (a heap of at most 16 MiB).
export function instantiate(module, target) {
  const exports = new WebAssembly.Instance(module).exports;
  const wide = target === "wasm64";
  const pointer = (value) => (wide ? BigInt(value) : value);
  const number = (value) => (wide ? Number(value) : value >>> 0);
  const base = number(exports.__heap_base.value);
  return {
    exports,
    memory: exports.memory,
    // The first block starts at __heap_base rounded up to 16; its payload follows the 16-byte header.
    firstPayload: ((base + 15) & ~15) + 16,
    bytes: () => new Uint8Array(exports.memory.buffer),
    allocate: (size) => number(exports.allocate(BigInt(size))),
    resize: (old, previous, size) => number(exports.resize(pointer(old), BigInt(previous), BigInt(size))),
    release: (address) => exports.release(pointer(address)),
  };
}

// The capacity of a block that serves `size` bytes: the 16-byte header plus the request, rounded up to 16.
export const capacity = (size) => (size + 31) & ~15;

function xorshift32(seed) {
  let state = seed | 0;
  return () => {
    state ^= state << 13;
    state ^= state >>> 17;
    state ^= state << 5;
    return state >>> 0;
  };
}

const holds = (bytes, address, size, value) => {
  for (let index = address, end = address + size; index < end; index++) if (bytes[index] !== value) return false;
  return true;
};

// Hand-derived address cases. Each case starts from a fresh instance, so every address is relative.
export function reuseCases({ fresh }) {
  // A freed block of capacity 32 serves the next request of capacity 32.
  { const api = fresh(); const a = api.allocate(8); api.allocate(600); api.release(a); assert.equal(api.allocate(16), a); }
  // A freed block of capacity 128 splits into 64 + 64: the first half, then the second half.
  { const api = fresh(); const a = api.allocate(100); api.allocate(600); api.release(a); assert.equal(api.allocate(40), a); assert.equal(api.allocate(40), a + 64); }
  // Three adjacent blocks of capacity 48 become one block of 144 in any release order; a request of 128 needs 144.
  for (const order of [[0, 1, 2], [0, 2, 1], [1, 0, 2], [1, 2, 0], [2, 0, 1], [2, 1, 0]]) {
    const api = fresh();
    const blocks = [api.allocate(32), api.allocate(32), api.allocate(32)];
    api.allocate(16);
    for (const index of order) api.release(blocks[index]);
    assert.equal(api.allocate(128), blocks[0], `release order ${order}`);
  }
  // A block at the top of the heap that is released and requested again comes back at its address without growth.
  { const api = fresh(); const a = api.allocate(1000); const before = api.memory.buffer.byteLength; api.release(a); assert.equal(api.allocate(1000), a); assert.equal(api.memory.buffer.byteLength, before); }
  // Large free blocks are used in address order, whatever the release order.
  for (const order of [["l2", "l1"], ["l1", "l2"]]) {
    const api = fresh();
    const blocks = { l1: api.allocate(1000) };
    api.allocate(16);
    blocks.l2 = api.allocate(1000);
    api.allocate(16);
    for (const name of order) api.release(blocks[name]);
    assert.equal(api.allocate(1000), blocks.l1, `release order ${order}`);
  }
  // Eight blocks of capacity 48 and a guard: the even ones, then the odd ones, merge 48 -> 144 -> 240 -> 336 -> 384 -> 416.
  {
    const api = fresh();
    const blocks = Array.from({ length: 8 }, () => api.allocate(32));
    const guard = api.allocate(16);
    blocks.forEach((address, index) => assert.equal(address, blocks[0] + 48 * index));
    assert.equal(guard, blocks[0] + 48 * 8);
    const before = api.memory.buffer.byteLength;
    for (const index of [0, 2, 4, 6, 1, 3, 5, 7]) api.release(blocks[index]);
    api.release(guard);
    assert.equal(api.allocate(32), blocks[0]);
    assert.equal(api.allocate(32), blocks[1]);
    assert.equal(api.memory.buffer.byteLength, before);
  }
}

// Sizes around every capacity step: neighbouring blocks are exactly one capacity apart.
export function boundaryCases({ fresh }) {
  for (const size of [1, 15, 16, 17, 31, 32, 33, 47, 48, 49, 511, 512, 513, 528, 1000, 4096, 70000]) {
    const api = fresh();
    const first = api.allocate(size), second = api.allocate(1);
    assert.equal(first % 16, 0, `alignment of ${size}`);
    assert.equal(first, api.firstPayload);
    assert.equal(second - first, capacity(size), `capacity of ${size}`);
    api.bytes().fill(0xa5, first, first + size);
    api.release(first);
    assert.equal(api.allocate(size), first, `reuse of ${size}`);
  }
  // Growing or shrinking a block across a capacity step: in place exactly when the capacity suffices.
  for (const [from, to] of [[16, 16], [16, 17], [17, 16], [32, 33], [40, 8], [512, 513], [513, 512], [1, 2000], [2000, 1], [64, 70000]]) {
    const api = fresh();
    const block = api.allocate(from);
    api.allocate(16);
    const kept = Math.min(from, to);
    api.bytes().fill(0x3c, block, block + from);
    const moved = api.resize(block, from, to);
    assert.notEqual(moved, 0);
    assert.equal(moved % 16, 0);
    assert.equal(moved === block, capacity(to) <= capacity(from), `resize ${from} -> ${to}`);
    assert.ok(holds(api.bytes(), moved, kept, 0x3c), `resize ${from} -> ${to} keeps ${kept} bytes`);
  }
  // null and zero.
  {
    const api = fresh();
    assert.equal(api.resize(0, 0, 0), 0);
    const block = api.allocate(40);
    assert.equal(api.resize(block, 40, 0), 0);
    assert.equal(api.allocate(40), block);
    const made = api.resize(0, 0, 40);
    assert.notEqual(made, 0);
    assert.equal(made % 16, 0);
  }
}

// The 16 MiB limit: requests above 16777184 fail the size check, and the last page cannot be passed.
export function limitCases({ fresh }) {
  for (const size of [MAX_REQUEST + 1, 2 ** 40]) assert.throws(() => fresh().allocate(size), WebAssembly.RuntimeError, `allocate ${size}`);
  assert.throws(() => fresh().resize(0, 0, MAX_REQUEST + 1), WebAssembly.RuntimeError);
  {
    const api = fresh();
    const block = api.allocate(16);
    assert.throws(() => api.resize(block, 16, MAX_REQUEST + 1), WebAssembly.RuntimeError);
  }
  // The block that ends exactly at the limit fits; one byte more does not.
  {
    const api = fresh();
    const size = LIMIT - (api.firstPayload - 16) - 16;
    assert.equal(api.allocate(size), api.firstPayload);
    assert.equal(api.memory.buffer.byteLength, LIMIT);
    assert.throws(() => api.allocate(1), WebAssembly.RuntimeError);
  }
  assert.throws(() => { const api = fresh(); api.allocate(LIMIT - (api.firstPayload - 16) - 15); }, WebAssembly.RuntimeError);
}

// 20000 random allocations, releases and resizes of up to 512 live blocks. A reference model of
// the live blocks checks alignment, that payloads never overlap or leave the heap, and that every
// block keeps its bytes (each is filled with its own id) through moves, until the last release:
// then the heap is whole again and serves the first request at the address of the very first block.
export function stress({ fresh }, { seed = 0x9e3779b9, operations = 20000, maxLive = 512 } = {}) {
  const api = fresh();
  const bytes = () => api.bytes();
  const next = xorshift32(seed);
  const size = () => ((next() & 7) === 7 ? 601 + (next() % 69400) : next() % 601);
  const live = [];
  let serial = 0, first = null;
  const check = (block, label) => {
    assert.equal(block.address % 16, 0, `${label}: alignment`);
    assert.ok(block.address >= api.firstPayload && block.address + block.size <= api.memory.buffer.byteLength, `${label}: inside the heap`);
    if (block.size === 0) return;
    for (const other of live) {
      if (other === block || other.size === 0) continue;
      assert.ok(block.address + block.size <= other.address || other.address + other.size <= block.address, `${label}: ${block.id} overlaps ${other.id}`);
    }
  };
  const fill = (block) => bytes().fill(block.id & 0xff, block.address, block.address + block.size);
  const verify = (block, count, label) => assert.ok(holds(bytes(), block.address, count, block.id & 0xff), `${label}: block ${block.id} lost its bytes`);
  const release = (index) => {
    const [block] = live.splice(index, 1);
    verify(block, block.size, "release");
    api.release(block.address);
  };
  for (let step = 0; step < operations; step++) {
    const operation = next() & 3;
    if (operation < 2 && live.length < maxLive || live.length === 0) {
      const block = { id: ++serial, size: size(), address: 0 };
      block.address = api.allocate(block.size);
      if (first === null) first = block.address;
      live.push(block);
      check(block, "allocate");
      fill(block);
    } else if (operation === 3 && live.length > 0) {
      const block = live[next() % live.length];
      const wanted = size();
      const moved = api.resize(block.address, block.size, wanted);
      if (wanted === 0) {
        assert.equal(moved, 0, "resize to 0 releases");
        live.splice(live.indexOf(block), 1);
        continue;
      }
      const previous = block.address;
      block.address = moved;
      const count = Math.min(block.size, wanted);
      // The old bytes are in place at the new address before anything is rewritten.
      verify(block, count, `resize ${block.size} -> ${wanted} from ${previous}`);
      block.size = wanted;
      check(block, "resize");
      fill(block);
    } else {
      release(next() % live.length);
    }
  }
  while (live.length > 0) release(next() % live.length);
  assert.equal(api.allocate(32), first, "everything released: the heap is whole again");
  assert.ok(api.memory.buffer.byteLength <= LIMIT);
  assert.ok(serial > operations / 4, "allocations happened");
}

export function runHeapTests({ targets = ["wasm32"], optimizations = [0, 3], extraCases = [] } = {}) {
  const directory = mkdtempSync(join(tmpdir(), "tsuzuri-wasm-heap-"));
  try {
    for (const target of targets) {
      for (const optimization of optimizations) {
        const module = buildHeap({ target, optimization, directory });
        const context = { fresh: () => instantiate(module, target), target, optimization };
        reuseCases(context);
        boundaryCases(context);
        limitCases(context);
        stress(context);
        for (const run of extraCases) run(context);
      }
      console.log(`WASM heap (${target}): reuse, coalescing, boundary sizes, resize, limits and a 20000-operation stress passed at O0/O3`);
    }
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  const choice = process.argv[2] ?? "both";
  assert.ok(["wasm32", "wasm64", "both"].includes(choice), "usage: node tests/wasm_heap.mjs [wasm32|wasm64|both]");
  const targets = choice === "both" ? ["wasm32", "wasm64"] : [choice];
  if (targets.includes("wasm64") && !memory64Supported()) {
    if (choice === "wasm64") {
      console.error("tests/wasm_heap.mjs wasm64 needs memory64 (Node.js 24 or newer): npx --yes --package=node@24 node tests/wasm_heap.mjs wasm64");
      process.exit(1);
    }
    console.log("WASM heap (wasm64): skipped, memory64 needs Node.js 24 or newer");
    targets.splice(targets.indexOf("wasm64"), 1);
  }
  runHeapTests({ targets });
}
