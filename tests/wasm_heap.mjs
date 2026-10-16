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

// Reads of the heap's own globals, so that the invariants of the design can be checked from outside, and a
// flush of the blocks that are only cached (the internal @tz.heap.flush).
const probes = (target) => {
  const t = target === "wasm32" ? "i32" : "i64";
  return `
define ${t} @probe_end() { %value = load ${t}, ptr @tz.heap.end ret ${t} %value }
define ${t} @probe_free() { %value = load ${t}, ptr @tz.heap.free ret ${t} %value }
define ${t} @probe_mask() { %value = load ${t}, ptr @tz.heap.smallmask ret ${t} %value }
define ${t} @probe_small(${t} %index) {
  %slot = getelementptr [32 x ${t}], ptr @tz.heap.small, ${t} 0, ${t} %index
  %value = load ${t}, ptr %slot
  ret ${t} %value
}
define ${t} @probe_fastmask() { %value = load ${t}, ptr @tz.heap.fastmask ret ${t} %value }
define ${t} @probe_fast(${t} %index) {
  %slot = getelementptr [32 x ${t}], ptr @tz.heap.fast, ${t} 0, ${t} %index
  %value = load ${t}, ptr %slot
  ret ${t} %value
}
define void @probe_flush() { call void @tz.heap.flush() ret void }
`;
};
const probeExports = ["probe_end", "probe_free", "probe_mask", "probe_small", "probe_fastmask", "probe_fast", "probe_flush"];

// An empty module with one 64-bit memory.
export const memory64Supported = () => WebAssembly.validate(new Uint8Array([0, 97, 115, 109, 1, 0, 0, 0, 5, 3, 1, 4, 0]));

function execute(program, args) {
  const result = spawnSync(program, args, { encoding: "utf8", timeout: 180_000 });
  if (result.error) throw result.error;
  assert.equal(result.status, 0, `${program} ${args.join(" ")}\n${result.stdout}\n${result.stderr}`);
}

// The live-bytes function of the threads heap (heap-wasm-threads.ll), which tests/wasm_threads.mjs asserts on real workers,
// compiled with the heap and a lock that does nothing: the walk is checked against the blocks that the walker finds.
function threadsLiveBytes() {
  const text = readFileSync(join(root, "src/runtime/heap-wasm-threads.ll"), "utf8");
  const start = text.indexOf("define i64 @tsuzuri_thread_heap_live_bytes()");
  assert.ok(start >= 0, "the live-bytes function is in heap-wasm-threads.ll");
  const end = text.indexOf("\n}\n", start) + 3;
  return `define void @tsuzuri_threads_heap_lock() { ret void }\ndefine void @tsuzuri_threads_heap_unlock() { ret void }\n${text.slice(start, end)}`;
}

// Compiles the heap of `target` with the wrappers (and `extra` IR that may read the heap's own
// globals) and links a module that exports allocate, resize, release, __heap_base and memory.
export function buildHeap({ target = "wasm32", optimization = 3, directory, extra = "", exports = [], bulkMemory = true }) {
  const heap = readFileSync(join(root, "src/runtime", target === "wasm64" ? "heap-wasm64.ll" : "heap-wasm.ll"), "utf8");
  const stem = join(directory, `heap-${target}-${optimization}`);
  const threads = target === "wasm32" ? threadsLiveBytes() : "";
  if (threads) exports = [...exports, "tsuzuri_thread_heap_live_bytes"];
  writeFileSync(`${stem}.ll`, `declare void @llvm.trap()\n${heap}\n${WRAPPERS}\n${probes(target)}\n${threads}\n${extra}`);
  execute(clang, [`--target=${target}`, `-O${optimization}`, ...(bulkMemory ? ["-mbulk-memory"] : []), "-Wno-override-module", "-c", `${stem}.ll`, "-o", `${stem}.o`]);
  execute(wasmLd, [
    ...(target === "wasm64" ? ["-mwasm64"] : []), `${stem}.o`, "--no-entry", "--export=allocate", "--export=resize", "--export=release",
    "--export=__heap_base", "--export-memory", `--max-memory=${LIMIT}`, ...[...probeExports, ...exports].map((name) => `--export=${name}`), "-o", `${stem}.wasm`,
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
  const api = {
    exports,
    target,
    word: wide ? 8 : 4,
    memory: exports.memory,
    // The first block starts at __heap_base rounded up to 16; its payload follows the 16-byte header.
    firstPayload: ((base + 15) & ~15) + 16,
    bytes: () => new Uint8Array(exports.memory.buffer),
    allocate: (size) => number(exports.allocate(BigInt(size))),
    resize: (old, previous, size) => number(exports.resize(pointer(old), BigInt(previous), BigInt(size))),
    release: (address) => exports.release(pointer(address)),
    probe: (name, ...argument) => number(exports[name](...argument.map(pointer))),
    // Releases the blocks that were only cached for real, as the heap does before it grows.
    flush: () => exports.probe_flush(),
    // The design's invariants, from the blocks themselves.
    check: () => inspectHeap(api),
  };
  return api;
}

// The blocks of the heap from its start to its end, with every invariant of the design checked:
// capacities are multiples of 16 and at least 32 and tile the heap exactly; a free block's footer
// holds its capacity; PREV_FREE is set exactly when the block before is free; no two free blocks
// touch; the last block is not free; every free block is in exactly one list with correct prev
// links: the list of its capacity when that is 32 to 528 (and the mask has a bit for every list that
// is not empty), otherwise the large list, which is in address order; every cached block is a block
// in use of the capacity of its list, in one list only (the fast mask may have bits of empty lists).
// Returns the block addresses (headers, not payloads) of each list, head first; a block in a fast
// list has `cached: true`.
export function inspectHeap(api) {
  const word = api.word;
  const view = new DataView(api.memory.buffer);
  const read = (address) => (word === 8 ? Number(view.getBigUint64(address, true)) : view.getUint32(address, true));
  const start = api.firstPayload - 16;
  const end = api.probe("probe_end");
  assert.ok(end >= start, "the end is not below the start");
  const blocks = [];
  let previousFree = false;
  for (let address = start; address < end;) {
    const flags = read(address);
    const size = flags - (flags % 16);
    const free = (flags & 1) !== 0, behind = (flags & 2) !== 0;
    assert.ok(size >= 32 && address + size <= end, `block ${address}: capacity ${size} inside the heap`);
    assert.equal(behind, previousFree, `block ${address}: PREV_FREE`);
    if (free) {
      assert.ok(!previousFree, `block ${address}: two free blocks touch`);
      assert.equal(read(address + size - word), size, `block ${address}: footer`);
    }
    blocks.push({ address, size, free });
    previousFree = free;
    address += size;
  }
  assert.equal(blocks.reduce((sum, block) => sum + block.size, 0), end - start, "blocks tile the heap");
  assert.ok(blocks.length === 0 || !blocks[blocks.length - 1].free, "the last block is not free");
  const byAddress = new Map(blocks.map((block) => [block.address, block]));
  const listed = new Set();
  const walk = (head, label, accept) => {
    const nodes = [];
    for (let node = head, previous = 0; node !== 0; previous = node, node = read(node + word)) {
      const block = byAddress.get(node);
      assert.ok(block?.free, `${label}: ${node} is a free block`);
      assert.ok(accept(block), `${label}: ${node} has capacity ${block.size}`);
      assert.equal(read(node + 2 * word), previous, `${label}: block ${node} prev link`);
      assert.ok(!listed.has(node), `${label}: block ${node} is in one list`);
      listed.add(node);
      nodes.push(node);
      assert.ok(nodes.length <= blocks.length, `${label} ends`);
    }
    return nodes;
  };
  const mask = api.probe("probe_mask");
  const small = Array.from({ length: 32 }, (_, index) => {
    const head = api.probe("probe_small", index);
    assert.equal((mask >>> index) & 1, head === 0 ? 0 : 1, `mask bit ${index}`);
    return walk(head, `small[${index}]`, (block) => block.size === (index + 2) * 16);
  });
  const large = walk(api.probe("probe_free"), "large", (block) => block.size > 528);
  assert.deepEqual(large, [...large].sort((a, b) => a - b), "the large list is in address order");
  assert.equal(listed.size, blocks.filter((block) => block.free).length, "every free block is in a list");
  const fastMask = api.probe("probe_fastmask");
  const fast = Array.from({ length: 32 }, (_, index) => {
    const head = api.probe("probe_fast", index);
    if (head !== 0) assert.equal((fastMask >>> index) & 1, 1, `fast mask bit ${index}`);
    const nodes = [];
    for (let node = head; node !== 0; node = read(node + 16)) {
      const block = byAddress.get(node);
      assert.ok(block && !block.free, `fast[${index}]: ${node} is a block in use`);
      assert.equal(block.size, (index + 2) * 16, `fast[${index}]: ${node} has capacity ${block.size}`);
      assert.ok(!listed.has(node), `fast[${index}]: block ${node} is in one list`);
      listed.add(node);
      block.cached = true;
      nodes.push(node);
      assert.ok(nodes.length <= blocks.length, `fast[${index}] ends`);
    }
    return nodes;
  });
  // The threads heap counts the bytes in use as the heap less every free and every cached block.
  if (api.exports.tsuzuri_thread_heap_live_bytes) {
    const live = blocks.filter((block) => !block.free && !block.cached).reduce((sum, block) => sum + block.size, 0);
    assert.equal(api.exports.tsuzuri_thread_heap_live_bytes(), BigInt(live), "the live bytes of the threads heap");
  }
  return { start, end, blocks, small, large, fast };
}

// The capacity of a block that serves `size` bytes: the 16-byte header plus the request, rounded up
// to 16, and never below 32 (a request of 0 still takes a block that can be freed and linked).
export const capacity = (size) => Math.max(32, (size + 31) & ~15);

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
  { const api = fresh(); const a = api.allocate(8); api.allocate(600); api.release(a); api.check(); assert.equal(api.allocate(16), a); api.check(); }
  // A freed block of capacity 128 splits into 64 + 64: the first half, then the second half.
  { const api = fresh(); const a = api.allocate(100); api.allocate(600); api.release(a); assert.equal(api.allocate(40), a); api.check(); assert.equal(api.allocate(40), a + 64); api.check(); }
  // Three adjacent blocks of capacity 48 become one block of 144 in any release order; a request of 128 needs 144.
  for (const order of [[0, 1, 2], [0, 2, 1], [1, 0, 2], [1, 2, 0], [2, 0, 1], [2, 1, 0]]) {
    const api = fresh();
    const blocks = [api.allocate(32), api.allocate(32), api.allocate(32)];
    api.allocate(16);
    for (const index of order) { api.release(blocks[index]); api.check(); }
    assert.equal(api.allocate(128), blocks[0], `release order ${order}`);
    api.check();
  }
  // A block at the top of the heap that is released and requested again comes back at its address without growth.
  { const api = fresh(); const a = api.allocate(1000); const before = api.memory.buffer.byteLength; api.release(a); api.check(); assert.equal(api.allocate(1000), a); assert.equal(api.memory.buffer.byteLength, before); }
  // Large free blocks are used in address order, whatever the release order.
  for (const order of [["l2", "l1"], ["l1", "l2"]]) {
    const api = fresh();
    const blocks = { l1: api.allocate(1000) };
    api.allocate(16);
    blocks.l2 = api.allocate(1000);
    api.allocate(16);
    for (const name of order) { api.release(blocks[name]); api.check(); }
    assert.equal(api.allocate(1000), blocks.l1, `release order ${order}`);
    api.check();
  }
  // Eight blocks of capacity 48 and a guard: the even ones, then the odd ones, are released; they are only cached
  // until the flush, which merges 48 -> 144 -> 240 -> 336 -> 384 -> 416 in whatever order it takes them.
  {
    const api = fresh();
    const blocks = Array.from({ length: 8 }, () => api.allocate(32));
    const guard = api.allocate(16);
    blocks.forEach((address, index) => assert.equal(address, blocks[0] + 48 * index));
    assert.equal(guard, blocks[0] + 48 * 8);
    const before = api.memory.buffer.byteLength;
    for (const index of [0, 2, 4, 6, 1, 3, 5, 7]) api.release(blocks[index]);
    api.release(guard);
    assert.equal(api.check().blocks.filter((block) => block.free).length, 0, "nothing is merged before the flush");
    api.flush();
    // The merged block touched the end: the heap is empty again, and grows from its start.
    assert.deepEqual(api.check().blocks, []);
    assert.equal(api.allocate(32), blocks[0]);
    assert.equal(api.allocate(32), blocks[1]);
    assert.equal(api.memory.buffer.byteLength, before);
    api.check();
  }
}

// Sizes around every capacity step: neighbouring blocks are exactly one capacity apart.
export function boundaryCases({ fresh }) {
  for (const size of [0, 1, 15, 16, 17, 31, 32, 33, 47, 48, 49, 511, 512, 513, 528, 1000, 4096, 70000]) {
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
  for (const [from, to] of [[16, 16], [16, 17], [17, 16], [32, 33], [40, 8], [512, 513], [513, 512], [1, 2000], [2000, 1], [64, 70000], [0, 8], [0, 100]]) {
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
  // The copy of a move reads the old size, not the new one: the old block is at the end of the memory here, and 2 MiB above it
  // is beyond the memory.
  {
    const api = fresh();
    const big = api.allocate(2 * 1024 * 1024);
    const old = api.allocate(8);
    api.allocate(8);
    api.bytes().fill(0x4d, old, old + 8);
    api.release(big);
    const moved = api.resize(old, 8, 2 * 1024 * 1024 - 64);
    assert.equal(moved, big);
    assert.ok(holds(api.bytes(), moved, 8, 0x4d));
    api.check();
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

// A block that touches the end of the heap is given back by lowering the end (the memory itself
// stays); a block in the middle is only linked. The end is read from the heap's own global. A block of
// capacity 528 or less is only cached by its release, and the flush gives it back for real.
export function trimCases({ fresh }) {
  { // one block (capacity 1024)
    const api = fresh();
    const a = api.allocate(1000);
    api.release(a);
    assert.equal(api.probe("probe_end"), api.firstPayload - 16);
    assert.deepEqual(api.check().blocks, []);
  }
  { // two blocks of capacity 624: the lower one first, then the top one, which takes it along
    const api = fresh();
    const start = api.firstPayload - 16;
    const a = api.allocate(600), b = api.allocate(600);
    assert.equal(b, a + 624);
    api.release(a);
    assert.equal(api.probe("probe_end"), start + 1248);
    assert.deepEqual(api.check().blocks, [{ address: start, size: 624, free: true }, { address: start + 624, size: 624, free: false }]);
    api.release(b);
    assert.equal(api.probe("probe_end"), start);
    assert.deepEqual(api.check().blocks, []);
  }
  { // a block in the middle does not lower the end; the top one then takes it along but not the block below
    const api = fresh();
    const start = api.firstPayload - 16;
    const a = api.allocate(600), b = api.allocate(600), c = api.allocate(600);
    const before = api.memory.buffer.byteLength;
    api.release(b);
    assert.equal(api.probe("probe_end"), start + 1872);
    api.release(c);
    assert.equal(api.probe("probe_end"), start + 624);
    assert.deepEqual(api.check().blocks, [{ address: start, size: 624, free: false }]);
    assert.equal(api.allocate(600), b);
    assert.equal(api.memory.buffer.byteLength, before);
    api.release(a);
    api.check();
  }
  { // two blocks of capacity 128: cached by the release, given back by the flush, the lower one first
    const api = fresh();
    const start = api.firstPayload - 16;
    const a = api.allocate(100), b = api.allocate(100);
    assert.equal(b, a + 128);
    api.release(a);
    assert.equal(api.probe("probe_end"), start + 256);
    assert.deepEqual(api.check().blocks, [{ address: start, size: 128, free: false, cached: true }, { address: start + 128, size: 128, free: false }]);
    api.flush();
    assert.deepEqual(api.check().blocks, [{ address: start, size: 128, free: true }, { address: start + 128, size: 128, free: false }]);
    api.release(b);
    api.flush();
    assert.equal(api.probe("probe_end"), start);
    assert.deepEqual(api.check().blocks, []);
  }
  { // a request of 0 still takes a block of capacity 32, which comes back whole from the cache
    const api = fresh();
    const start = api.firstPayload - 16;
    const zero = api.allocate(0), one = api.allocate(1);
    assert.equal(one - zero, 32);
    api.release(zero);
    assert.deepEqual(api.check().blocks, [{ address: start, size: 32, free: false, cached: true }, { address: start + 32, size: 32, free: false }]);
    assert.equal(api.allocate(0), zero);
    api.release(one);
    api.release(zero);
    api.flush();
    assert.equal(api.probe("probe_end"), start);
  }
}

// Growing a block into the free block after it: with a rest of at least 32 the rest stays free, with
// less the block takes it all; PREV_FREE of the block and of the block after are kept right; and a
// request that the neighbour cannot serve moves the block. The neighbour that a release only cached counts as
// free: a block that has to grow and cannot yet releases the cached blocks for real and looks again.
// Capacities: 32 bytes -> 48, 128 -> 144, 16 -> 32.
export function absorbCases({ fresh }) {
  { // 48 + 144 = 192; a request of 80 needs 96: the rest of 96 is a free block, which serves a request of 64 (capacity 80) whole
    const api = fresh();
    const a = api.allocate(32), b = api.allocate(128), guard = api.allocate(16);
    api.bytes().fill(0x5a, a, a + 32);
    api.release(b);
    assert.equal(api.resize(a, 32, 80), a);
    assert.ok(holds(api.bytes(), a, 32, 0x5a));
    const start = api.firstPayload - 16;
    assert.deepEqual(api.check().blocks, [{ address: start, size: 96, free: false }, { address: start + 96, size: 96, free: true }, { address: start + 192, size: 32, free: false }]);
    assert.equal(api.allocate(64), a + 96);
    assert.equal(guard, a + 48 + 144);
    api.check();
  }
  { // 48 + 80 = 128 after the release: a request of 80 needs 96 and leaves 32, which is a free block; a request of 90 needs 112 and
    // leaves 16, which stays with the block
    for (const [size, rest] of [[80, 32], [90, 16]]) {
      const api = fresh();
      const a = api.allocate(32), b = api.allocate(64);
      api.allocate(16);
      api.release(b);
      assert.equal(api.resize(a, 32, size), a);
      const start = api.firstPayload - 16;
      const kept = rest === 32 ? 96 : 128;
      const tail = rest === 32 ? [{ address: start + 96, size: 32, free: true }] : [];
      assert.deepEqual(api.check().blocks, [{ address: start, size: kept, free: false }, ...tail, { address: start + 128, size: 32, free: false }]);
    }
  }
  { // 48 + 48 = 96 = a request of 70: nothing is left, and the block after the pair no longer follows a free block
    const api = fresh();
    const a = api.allocate(32), b = api.allocate(32);
    api.allocate(16);
    api.release(b);
    assert.equal(api.resize(a, 32, 70), a);
    api.check();
  }
  { // the block keeps PREV_FREE when the block before it is free (the cached one becomes free in the same flush); releasing it
    // then merges downwards
    const api = fresh();
    const p = api.allocate(32), a = api.allocate(32), b = api.allocate(32);
    api.allocate(16);
    api.release(p);
    api.release(b);
    assert.equal(api.resize(a, 32, 70), a);
    api.check();
    api.release(a);
    api.flush();
    api.check();
    assert.equal(api.allocate(128), p);
    api.check();
  }
  { // 48 + 48 = 96 is less than the 128 that a request of 100 needs: the block moves to the end and its old place is merged
    const api = fresh();
    const a = api.allocate(32), b = api.allocate(32), guard = api.allocate(16);
    api.bytes().fill(0x6b, a, a + 32);
    api.release(b);
    const moved = api.resize(a, 32, 100);
    assert.equal(moved, guard + 32);
    assert.ok(holds(api.bytes(), moved, 32, 0x6b));
    api.flush();
    const start = api.firstPayload - 16;
    assert.deepEqual(api.check().blocks, [{ address: start, size: 96, free: true }, { address: start + 96, size: 32, free: false }, { address: start + 128, size: 128, free: false }]);
  }
  { // the neighbour is free but too small, and the block after it is cached: the flush merges the two and the block grows
    const api = fresh();
    const a = api.allocate(32), b = api.allocate(32), c = api.allocate(64);
    api.allocate(16);
    api.bytes().fill(0x6d, a, a + 32);
    api.release(b);
    api.flush();
    api.release(c);
    assert.equal(api.resize(a, 32, 130), a);
    assert.ok(holds(api.bytes(), a, 32, 0x6d));
    const start = api.firstPayload - 16;
    assert.deepEqual(api.check().blocks, [{ address: start, size: 176, free: false }, { address: start + 176, size: 32, free: false }]);
  }
  { // the neighbour is in use and a block elsewhere is cached: the block moves, and the cached block was released for real on the way
    const api = fresh();
    const a = api.allocate(32);
    api.allocate(32);
    api.allocate(16);
    const spare = api.allocate(32);
    api.allocate(16);
    api.bytes().fill(0x6c, a, a + 32);
    api.release(spare);
    assert.deepEqual(api.check().fast[1], [spare - 16]);
    const moved = api.resize(a, 32, 70);
    assert.notEqual(moved, a);
    assert.ok(holds(api.bytes(), moved, 32, 0x6c));
    const lists = api.check();
    assert.deepEqual([lists.small[1], lists.fast[1]], [[spare - 16], [a - 16]]);
  }
  { // the memory above the end holds the stale header of a free block (b and c were merged and given back to the end):
    // a block that ends at the end must not read it as a neighbour; it grows in place instead
    const api = fresh();
    const p = api.allocate(32);
    api.allocate(32);
    const a = api.allocate(32), b = api.allocate(600), c = api.allocate(600);
    api.bytes().fill(0x7d, a, a + 32);
    api.release(p);
    api.release(b);
    api.release(c);
    api.check();
    assert.equal(api.probe("probe_end"), a + 32);
    // 48 + the stale 624 would serve a request of 100 (capacity 128) if the stale header were read
    const moved = api.resize(a, 32, 100);
    assert.equal(moved, a);
    assert.ok(holds(api.bytes(), moved, 32, 0x7d));
    assert.equal(api.probe("probe_end"), a - 16 + 128);
    api.check();
  }
}

// Free blocks of capacity 32 to 528 wait in a list per capacity, last in first out, and a request takes
// from the smallest list that serves it; larger free blocks are in one list in address order and a
// request takes the first that fits. A release only caches a small block, so the blocks get into these
// lists by the flush, which takes the cached lists from the lowest capacity up and each from its head:
// the blocks of a list of free blocks end up in the reverse of their order in the cache.
// Capacities: 16 bytes -> 32, 32 -> 48, 40 -> 64, 64 -> 80, 128 -> 144.
export function binCases({ fresh }) {
  { // last in, first out
    const api = fresh();
    const a0 = api.allocate(32);
    api.allocate(32);
    const a2 = api.allocate(32);
    api.allocate(32);
    api.allocate(16);
    api.release(a0);
    api.release(a2);
    api.flush();
    assert.deepEqual(api.check().small[1], [a0 - 16, a2 - 16]);
    assert.equal(api.allocate(32), a0);
    assert.equal(api.allocate(32), a2);
    assert.deepEqual(api.check().small[1], []);
  }
  { // the smallest list that serves the request: a request of capacity 64 takes the block of 80 (the 16 left stay with it)
    const api = fresh();
    const x = api.allocate(32);
    api.allocate(16);
    const y = api.allocate(64);
    api.allocate(16);
    const z = api.allocate(128);
    api.allocate(16);
    for (const block of [x, y, z]) api.release(block);
    api.flush();
    const lists = api.check().small;
    assert.deepEqual([lists[1], lists[3], lists[7]], [[x - 16], [y - 16], [z - 16]]);
    assert.equal(api.allocate(40), y);
    assert.equal(api.allocate(100), z);
    assert.equal(api.allocate(32), x);
    assert.deepEqual(api.check().small.flat(), []);
  }
  { // a block of 144 serves a request of 48 and keeps 96 in the list of 96, which then serves a request of 80 whole
    const api = fresh();
    const z = api.allocate(128);
    api.allocate(16);
    api.release(z);
    api.flush();
    assert.equal(api.allocate(32), z);
    assert.deepEqual(api.check().small[4], [z - 16 + 48]);
    assert.equal(api.allocate(64), z + 48);
    assert.deepEqual(api.check().small.flat(), []);
  }
  { // the rest of a block is split off from 32 bytes up: 80 -> 48 + 32 (the smallest list)
    const api = fresh();
    const y = api.allocate(64);
    api.allocate(16);
    api.release(y);
    api.flush();
    assert.deepEqual(api.check().small[3], [y - 16]);
    assert.equal(api.allocate(32), y);
    const lists = api.check().small;
    assert.deepEqual([lists[0], lists[3]], [[y - 16 + 48], []]);
  }
  { // no small list serves the request: the first large block is split and its rest stays in the large list
    const api = fresh();
    const l1 = api.allocate(1000);
    api.allocate(16);
    api.release(l1);
    assert.deepEqual(api.check().large, [l1 - 16]);
    assert.equal(api.allocate(16), l1);
    assert.deepEqual(api.check().large, [l1 - 16 + 32]);
  }
  { // first fit by address among large blocks (capacities 1024 and 2016): 1520 does not fit the first; the rest of 496 is a small list
    const api = fresh();
    const l1 = api.allocate(1000);
    api.allocate(16);
    const l2 = api.allocate(2000);
    api.allocate(16);
    api.release(l2);
    api.release(l1);
    assert.deepEqual(api.check().large, [l1 - 16, l2 - 16]);
    assert.equal(api.allocate(1500), l2);
    const lists = api.check();
    assert.deepEqual(lists.large, [l1 - 16]);
    assert.deepEqual(lists.small[29], [l2 - 16 + 1520]);
  }
  { // 528 is the largest capacity with a list of its own, 544 the smallest large one
    const api = fresh();
    const small = api.allocate(512);
    api.allocate(16);
    const large = api.allocate(513);
    api.allocate(16);
    api.release(small);
    api.release(large);
    const cached = api.check();
    assert.deepEqual(cached.fast[31], [small - 16]);
    assert.deepEqual(cached.large, [large - 16]);
    api.flush();
    const lists = api.check();
    assert.deepEqual(lists.small[31], [small - 16]);
    assert.deepEqual(lists.large, [large - 16]);
    assert.equal(api.allocate(512), small);
    assert.deepEqual(api.check().small[31], []);
    assert.equal(api.allocate(513), large);
    assert.deepEqual(api.check().large, []);
  }
  { // the rest of a large block takes its place in the large list while it is above 528, and goes to a list of its own from 528 down
    for (const [size, list] of [[460, "large"], [470, "small"]]) {
      const api = fresh();
      const l = api.allocate(1000);
      api.allocate(16);
      api.release(l);
      assert.equal(api.allocate(size), l);
      const lists = api.check();
      const rest = l - 16 + capacity(size);
      if (list === "large") assert.deepEqual([lists.large, lists.small[31]], [[rest], []]);
      else assert.deepEqual([lists.large, lists.small[31]], [[], [rest]]);
    }
  }
}

// A block of capacity 32 to 528 that is released is cached on the list of its capacity, last in first out:
// it stays a block in use for every other block, so nothing merges and the end of the heap does not move
// until the flush. A request of exactly that capacity pops it; any other request that would have to grow the
// heap releases the cached blocks for real first.
export function fastCases({ fresh }) {
  { // pop in the reverse order of the release; a pop leaves the bit of its list set until the flush
    const api = fresh();
    const a0 = api.allocate(32);
    api.allocate(32);
    const a2 = api.allocate(32);
    api.allocate(32);
    api.allocate(16);
    api.release(a0);
    api.release(a2);
    const cached = api.check();
    assert.deepEqual(cached.fast[1], [a2 - 16, a0 - 16]);
    assert.deepEqual(cached.blocks.filter((block) => block.cached).map((block) => block.address), [a0 - 16, a2 - 16]);
    assert.equal(api.allocate(32), a2);
    assert.equal(api.allocate(32), a0);
    assert.deepEqual(api.check().fast[1], []);
    assert.equal(api.probe("probe_fastmask"), 2, "the bit of the emptied list stays");
    api.flush();
    assert.equal(api.probe("probe_fastmask"), 0);
    api.check();
  }
  { // nothing merges before the flush; the flush merges all three neighbours (48 + 48 + 48 = 144) in the order it takes them
    const api = fresh();
    const a = api.allocate(32), b = api.allocate(32), c = api.allocate(32);
    const guard = api.allocate(16);
    const start = api.firstPayload - 16;
    api.release(b);
    api.release(a);
    api.release(c);
    const cached = api.check();
    assert.equal(cached.blocks.filter((block) => block.free).length, 0);
    assert.deepEqual(cached.fast[1], [c - 16, a - 16, b - 16]);
    api.flush();
    const flushed = api.check();
    assert.deepEqual(flushed.blocks, [{ address: start, size: 144, free: true }, { address: start + 144, size: 32, free: false }]);
    assert.deepEqual([flushed.small[7], flushed.small.flat().length], [[a - 16], 1]);
    assert.equal(guard, a + 144);
  }
  { // only the exact capacity is popped: a request of 16 (capacity 32) is served by the block of 48 after the flush, whole
    const api = fresh();
    const x = api.allocate(32);
    api.allocate(16);
    api.release(x);
    assert.deepEqual(api.check().fast[1], [x - 16]);
    assert.equal(api.allocate(16), x);
    const after = api.check();
    assert.deepEqual(after.fast.flat(), []);
    assert.equal(api.probe("probe_fastmask"), 0, "the flush cleared the bits");
    assert.equal(after.blocks[0].size, 48);
  }
  { // a request that would grow the heap flushes first: 100 blocks of capacity 48 become one block of 4800
    const api = fresh();
    const blocks = Array.from({ length: 100 }, () => api.allocate(32));
    api.allocate(16);
    for (const block of blocks) api.release(block);
    const end = api.probe("probe_end"), size = api.memory.buffer.byteLength;
    assert.equal(api.allocate(4000), blocks[0]);
    assert.equal(api.probe("probe_end"), end);
    assert.equal(api.memory.buffer.byteLength, size);
    const lists = api.check();
    assert.deepEqual(lists.large, [blocks[0] - 16 + 4016]);
    assert.deepEqual(lists.fast.flat(), []);
  }
  { // a cached block that touches the end keeps the end where it is until the flush
    const api = fresh();
    const start = api.firstPayload - 16;
    const a = api.allocate(32);
    api.release(a);
    assert.equal(api.probe("probe_end"), start + 48);
    assert.deepEqual(api.check().blocks, [{ address: start, size: 48, free: false, cached: true }]);
    api.flush();
    assert.equal(api.probe("probe_end"), start);
    assert.deepEqual(api.check().blocks, []);
  }
  { // 528 is the largest capacity that is cached and popped: the block released last is the one that comes back
    const api = fresh();
    const x = api.allocate(512);
    api.allocate(16);
    const y = api.allocate(512);
    api.allocate(16);
    api.release(x);
    api.release(y);
    assert.equal(api.allocate(512), y);
    assert.deepEqual(api.check().fast[31], [x - 16]);
  }
  { // a request of any other capacity does not touch the cache while a list can serve it
    const api = fresh();
    const x = api.allocate(32);
    api.allocate(16);
    const y = api.allocate(64);
    api.allocate(16);
    const z = api.allocate(200);
    api.allocate(16);
    api.release(y);
    api.release(z);
    api.flush();
    api.release(x);
    assert.equal(api.allocate(100), z);
    assert.deepEqual(api.check().fast[1], [x - 16], "the cached block of 48 is still there");
  }
}

// A block at the end of the heap grows in place: the end moves up by what it needs, the bytes stay where they are, and the
// flag of a free block before it is kept.
export function topGrowthCases({ fresh }) {
  { // a Vec that doubles from 16 bytes to 1 MiB never moves and leaves an empty heap behind
    const api = fresh();
    const start = api.firstPayload - 16;
    let block = api.resize(0, 0, 16);
    assert.equal(block, api.firstPayload);
    let size = 16;
    for (let next = 32; next <= 2 ** 20; next *= 2) {
      api.bytes().fill(next & 0xff, block, block + size);
      const moved = api.resize(block, size, next);
      assert.equal(moved, block, `grown to ${next}`);
      assert.ok(holds(api.bytes(), block, size, next & 0xff), `grown to ${next}: the bytes stay`);
      size = next;
      assert.equal(api.probe("probe_end"), start + capacity(next));
      api.check();
    }
    api.release(block);
    assert.equal(api.probe("probe_end"), start);
    assert.deepEqual(api.check().blocks, []);
  }
  { // the free block before it stays free, and PREV_FREE stays set
    const api = fresh();
    const start = api.firstPayload - 16;
    const p = api.allocate(600), a = api.allocate(8);
    api.release(p);
    api.bytes().fill(0x42, a, a + 8);
    assert.equal(api.resize(a, 8, 200), a);
    assert.ok(holds(api.bytes(), a, 8, 0x42));
    assert.deepEqual(api.check().blocks, [{ address: start, size: 624, free: true }, { address: start + 624, size: 224, free: false }]);
  }
  { // growth across pages grows the memory by whole pages, exactly
    const api = fresh();
    const start = api.firstPayload - 16;
    const block = api.allocate(16);
    const bytes = 4 * 1024 * 1024;
    assert.equal(api.resize(block, 16, bytes), block);
    const end = start + capacity(bytes);
    assert.equal(api.probe("probe_end"), end);
    assert.equal(api.memory.buffer.byteLength, Math.ceil(end / 65536) * 65536);
  }
  { // a block that has a neighbour above it does not grow in place
    const api = fresh();
    const a = api.allocate(16);
    api.allocate(16);
    assert.notEqual(api.resize(a, 16, 100), a);
    api.check();
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
  // A block at the end grows in place up to the limit and no further: the one past it is refused by the grow and by the
  // fallback, which is an allocation that does not fit either.
  {
    const api = fresh();
    const block = api.allocate(16);
    const exact = LIMIT - (api.firstPayload - 16) - 16;
    assert.equal(api.resize(block, 16, exact), block);
    assert.equal(api.memory.buffer.byteLength, LIMIT);
    api.check();
    assert.throws(() => api.resize(block, exact, exact + 1), WebAssembly.RuntimeError);
  }
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
    if (step % 50 === 0) api.check();
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
  api.flush();
  const whole = api.check();
  assert.deepEqual(whole.blocks, [], "everything released: no block is left");
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
        trimCases(context);
        absorbCases(context);
        binCases(context);
        fastCases(context);
        topGrowthCases(context);
        boundaryCases(context);
        limitCases(context);
        stress(context);
        for (const run of extraCases) run(context);
      }
      console.log(`WASM heap (${target}): reuse, cache, flush, coalescing, top return and growth, absorption, boundary sizes, resize, limits and a 20000-operation stress passed at O0/O3`);
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
