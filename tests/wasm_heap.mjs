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

// Reads of the heap's own globals, so that the invariants of the design can be checked from outside.
const probes = (target) => (target === "wasm32" ? `
define i32 @probe_end() { %value = load i32, ptr @tz.heap.end ret i32 %value }
define i32 @probe_free() { %value = load i32, ptr @tz.heap.free ret i32 %value }
` : "");
const probeExports = (target) => (target === "wasm32" ? ["probe_end", "probe_free"] : []);

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
  writeFileSync(`${stem}.ll`, `declare void @llvm.trap()\n${heap}\n${WRAPPERS}\n${probes(target)}\n${extra}`);
  execute(clang, [`--target=${target}`, `-O${optimization}`, ...(bulkMemory ? ["-mbulk-memory"] : []), "-Wno-override-module", "-c", `${stem}.ll`, "-o", `${stem}.o`]);
  execute(wasmLd, [
    ...(target === "wasm64" ? ["-mwasm64"] : []), `${stem}.o`, "--no-entry", "--export=allocate", "--export=resize", "--export=release",
    "--export=__heap_base", "--export-memory", `--max-memory=${LIMIT}`, ...[...probeExports(target), ...exports].map((name) => `--export=${name}`), "-o", `${stem}.wasm`,
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
    probe: (name, ...argument) => number(exports[name](...argument)),
    // The design's invariants, from the blocks themselves (only where the walker knows the layout).
    check: () => (target === "wasm32" ? inspectHeap(api) : null),
  };
  return api;
}

// The blocks of the heap from its start to its end, with every invariant of the design checked:
// capacities are multiples of 16 and at least 32 and tile the heap exactly; a free block's footer
// holds its capacity; PREV_FREE is set exactly when the block before is free; no two free blocks
// touch; the last block is not free; the list holds the free blocks, once each, in address order,
// with correct prev links.
export function inspectHeap(api) {
  const word = api.word;
  const view = new DataView(api.memory.buffer);
  const read = (address) => (word === 8 ? Number(view.getBigUint64(address, true)) : view.getUint32(address, true));
  const start = api.firstPayload - 16;
  const end = api.probe("probe_end");
  const blocks = [];
  if (end !== 0) {
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
  }
  const byAddress = new Map(blocks.map((block) => [block.address, block]));
  const listed = [];
  for (let node = api.probe("probe_free"), previous = 0; node !== 0; previous = node, node = read(node + word)) {
    assert.ok(byAddress.get(node)?.free, `list entry ${node} is a free block`);
    assert.equal(read(node + 2 * word), previous, `block ${node}: prev link`);
    assert.ok(node > previous, "the list is in address order");
    assert.ok(listed.length <= blocks.length, "the list ends");
    listed.push(node);
  }
  assert.deepEqual(listed, blocks.filter((block) => block.free).map((block) => block.address), "the list holds every free block once");
  return { start, end, blocks };
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
  // Eight blocks of capacity 48 and a guard: the even ones, then the odd ones, merge 48 -> 144 -> 240 -> 336 -> 384 -> 416.
  {
    const api = fresh();
    const blocks = Array.from({ length: 8 }, () => api.allocate(32));
    const guard = api.allocate(16);
    blocks.forEach((address, index) => assert.equal(address, blocks[0] + 48 * index));
    assert.equal(guard, blocks[0] + 48 * 8);
    const before = api.memory.buffer.byteLength;
    for (const index of [0, 2, 4, 6, 1, 3, 5, 7]) api.release(blocks[index]);
    api.check();
    api.release(guard);
    // The merged block touched the end: the heap is empty again, and grows from its start.
    assert.deepEqual(api.check()?.blocks ?? [], []);
    assert.equal(api.allocate(32), blocks[0]);
    assert.equal(api.allocate(32), blocks[1]);
    assert.equal(api.memory.buffer.byteLength, before);
    api.check();
  }
}

// Sizes around every capacity step: neighbouring blocks are exactly one capacity apart.
export function boundaryCases({ fresh, target }) {
  // The size-0 clamp is part of the wasm32 heap; the wasm64 heap follows when it is ported (until then it keeps a block of 16).
  const zero = target === "wasm32" ? [0] : [];
  for (const size of [...zero, 1, 15, 16, 17, 31, 32, 33, 47, 48, 49, 511, 512, 513, 528, 1000, 4096, 70000]) {
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
  for (const [from, to] of [[16, 16], [16, 17], [17, 16], [32, 33], [40, 8], [512, 513], [513, 512], [1, 2000], [2000, 1], [64, 70000], ...(target === "wasm32" ? [[0, 8], [0, 100]] : [])]) {
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

// A block that touches the end of the heap is given back by lowering the end (the memory itself
// stays); a block in the middle is only linked. The end is read from the heap's own global.
export function trimCases({ fresh }) {
  const api0 = fresh();
  if (api0.check() === null) return;
  { // one block
    const api = fresh();
    const a = api.allocate(1000);
    api.release(a);
    assert.equal(api.probe("probe_end"), api.firstPayload - 16);
    assert.deepEqual(api.check().blocks, []);
  }
  { // two blocks of capacity 128: the lower one first, then the top one, which takes it along
    const api = fresh();
    const start = api.firstPayload - 16;
    const a = api.allocate(100), b = api.allocate(100);
    assert.equal(b, a + 128);
    api.release(a);
    assert.equal(api.probe("probe_end"), start + 256);
    assert.deepEqual(api.check().blocks, [{ address: start, size: 128, free: true }, { address: start + 128, size: 128, free: false }]);
    api.release(b);
    assert.equal(api.probe("probe_end"), start);
    assert.deepEqual(api.check().blocks, []);
  }
  { // a block in the middle does not lower the end; the top one then takes it along but not the block below
    const api = fresh();
    const start = api.firstPayload - 16;
    const a = api.allocate(100), b = api.allocate(100), c = api.allocate(100);
    const before = api.memory.buffer.byteLength;
    api.release(b);
    assert.equal(api.probe("probe_end"), start + 384);
    api.release(c);
    assert.equal(api.probe("probe_end"), start + 128);
    assert.deepEqual(api.check().blocks, [{ address: start, size: 128, free: false }]);
    assert.equal(api.allocate(100), b);
    assert.equal(api.memory.buffer.byteLength, before);
    api.release(a);
    api.check();
  }
  { // a request of 0 still takes a block of capacity 32, which comes back whole
    const api = fresh();
    const start = api.firstPayload - 16;
    const zero = api.allocate(0), one = api.allocate(1);
    assert.equal(one - zero, 32);
    api.release(zero);
    assert.deepEqual(api.check().blocks, [{ address: start, size: 32, free: true }, { address: start + 32, size: 32, free: false }]);
    assert.equal(api.allocate(0), zero);
    api.release(one);
    api.release(zero);
    assert.equal(api.probe("probe_end"), start);
  }
}

// Growing a block into the free block after it: with a rest of at least 32 the rest stays free, with
// less the block takes it all; PREV_FREE of the block and of the block after are kept right; and a
// request that the neighbour cannot serve moves the block. Capacities: 32 bytes -> 48, 128 -> 144, 16 -> 32.
export function absorbCases({ fresh }) {
  const walked = fresh().check() !== null;
  { // 48 + 144 = 192; a request of 80 needs 96: the rest of 96 is a free block, which serves a request of 64 (capacity 80) whole
    const api = fresh();
    const a = api.allocate(32), b = api.allocate(128), guard = api.allocate(16);
    api.bytes().fill(0x5a, a, a + 32);
    api.release(b);
    assert.equal(api.resize(a, 32, 80), a);
    assert.ok(holds(api.bytes(), a, 32, 0x5a));
    if (walked) {
      const start = api.firstPayload - 16;
      assert.deepEqual(api.check().blocks, [{ address: start, size: 96, free: false }, { address: start + 96, size: 96, free: true }, { address: start + 192, size: 32, free: false }]);
    }
    assert.equal(api.allocate(64), a + 96);
    assert.equal(guard, a + 48 + 144);
    api.check();
  }
  { // 48 + 48 = 96 = a request of 70: nothing is left, and the block after the pair no longer follows a free block
    const api = fresh();
    const a = api.allocate(32), b = api.allocate(32);
    api.allocate(16);
    api.release(b);
    assert.equal(api.resize(a, 32, 70), a);
    api.check();
  }
  { // the block keeps PREV_FREE when the block before it is free; releasing it then merges downwards
    const api = fresh();
    const p = api.allocate(32), a = api.allocate(32), b = api.allocate(32);
    api.allocate(16);
    api.release(p);
    api.release(b);
    api.check();
    assert.equal(api.resize(a, 32, 70), a);
    api.check();
    api.release(a);
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
    if (walked) {
      const start = api.firstPayload - 16;
      assert.deepEqual(api.check().blocks, [{ address: start, size: 96, free: true }, { address: start + 96, size: 32, free: false }, { address: start + 128, size: 128, free: false }]);
    }
  }
  { // the memory above the end holds the stale header of a free block (it was merged into the top block that was given back):
    // a block that ends at the end must not read it as a neighbour
    const api = fresh();
    const p = api.allocate(32);
    api.allocate(32);
    const a = api.allocate(32), b = api.allocate(100), c = api.allocate(100);
    api.bytes().fill(0x7d, a, a + 32);
    api.release(p);
    api.release(b);
    api.release(c);
    api.check();
    if (walked) assert.equal(api.probe("probe_end"), a + 32);
    // 48 + the stale 128 = 176 would serve a request of 100 (capacity 128) if the stale header were read
    const moved = api.resize(a, 32, 100);
    assert.notEqual(moved, 0);
    assert.ok(holds(api.bytes(), moved, 32, 0x7d));
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
  const whole = api.check();
  if (whole !== null) assert.deepEqual(whole.blocks, [], "everything released: no block is left");
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
        boundaryCases(context);
        limitCases(context);
        stress(context);
        for (const run of extraCases) run(context);
      }
      console.log(`WASM heap (${target}): reuse, coalescing, top return, absorption, boundary sizes, resize, limits and a 20000-operation stress passed at O0/O3`);
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
