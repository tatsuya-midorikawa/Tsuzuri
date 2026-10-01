import assert from "node:assert/strict";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { delimiter, dirname, join, resolve } from "node:path";
import { spawnSync } from "node:child_process";

// An empty module with one 64-bit memory.
if (!WebAssembly.validate(new Uint8Array([0, 97, 115, 109, 1, 0, 0, 0, 5, 3, 1, 4, 0]))) {
  console.error("tests/wasm64.mjs needs memory64 (Node.js 24 or newer): npx --yes --package=node@24 node tests/wasm64.mjs target/release/tsuzuri");
  process.exit(1);
}

const compiler = resolve(process.argv[2] ?? "target/release/tsuzuri");
const clang = process.env.TSUZURI_CLANG ?? "clang";
const wasmLd = process.env.TSUZURI_WASM_LD ?? "wasm-ld";
const MiB = 1024 * 1024, GiB = 1024 * MiB;
const directory = mkdtempSync(join(tmpdir(), "tsuzuri-wasm64-"));
// `tsuzuri test` starts `node` from PATH, which must also support memory64.
const env = { ...process.env, PATH: `${dirname(process.execPath)}${delimiter}${process.env.PATH}` };

function execute(program, args, success = true) {
  const result = spawnSync(program, args, { encoding: "utf8", timeout: 120000, env });
  if (success) assert.equal(result.status, 0, `${program} ${args.join(" ")}\n${result.error ?? ""}\n${result.stdout}\n${result.stderr}`);
  return result;
}
const cli = (args) => execute(compiler, args);

// Independent of the compiler: flags and page limits of the module's own memory.
function definedMemory(bytes) {
  let position = 8;
  const leb = () => { let value = 0, shift = 0, byte; do { byte = bytes[position++]; value += (byte & 127) * 2 ** shift; shift += 7; } while (byte & 128); return value; };
  while (position < bytes.length) {
    const id = bytes[position++], size = leb(), end = position + size;
    if (id === 5) {
      assert.equal(leb(), 1);
      const flags = bytes[position++], minimum = leb();
      return { flags, minimum, maximum: flags & 1 ? leb() : null };
    }
    position = end;
  }
  return null;
}

function load(path) {
  const module = new WebAssembly.Module(readFileSync(path));
  assert.deepEqual(WebAssembly.Module.imports(module), []);
  return module;
}

// Each call gets a fresh instance, so the heap starts empty right after the stack and static data.
function checkLimit(module, limit, label) {
  for (const length of [32n * BigInt(MiB), BigInt(limit - 2 * MiB)]) {
    const api = new WebAssembly.Instance(module).exports;
    assert.equal(api.tz_bytes_length(length), length, `${label}: ${length}`);
    assert.ok(api.memory.buffer.byteLength <= limit, label);
  }
  assert.throws(() => new WebAssembly.Instance(module).exports.tz_bytes_length(BigInt(limit - MiB)), WebAssembly.RuntimeError, label);
}

const heapLines = (limit) => [
  `  %fits = icmp ule i64 %size, ${limit - 32}`,
  `  %within = icmp ule i64 %end, ${limit}`,
  `  %fits = icmp ule i64 %new_size, ${limit - 32}`,
];
const occurrences = (text, line) => text.split(`${line}\n`).length - 1;

// The 64-bit runtime heap alone, with the limit substituted independently of the compiler.
function checkHeapBoundaries(limit, optimization) {
  let heap = readFileSync(resolve("src/runtime/heap-wasm64.ll"), "utf8");
  for (const [before, after] of heapLines(16 * MiB).map((line, index) => [line, heapLines(limit)[index]])) {
    assert.equal(occurrences(heap, before), 1, before);
    heap = heap.replace(`${before}\n`, `${after}\n`);
  }
  assert.doesNotMatch(heap, /16777184|16777216/);
  const input = join(directory, `heap-${limit}.ll`);
  const object = join(directory, `heap-${limit}-${optimization}.o`), output = join(directory, `heap-${limit}-${optimization}.wasm`);
  writeFileSync(input, `declare void @llvm.trap()\n${heap}\ndefine ptr @allocate(i64 %size) {\n  %value = call ptr @tz.alloc(i64 %size)\n  ret ptr %value\n}\n`);
  execute(clang, ["--target=wasm64", `-O${optimization}`, "-Wno-override-module", "-c", input, "-o", object]);
  execute(wasmLd, ["-mwasm64", object, "--no-entry", "--export=allocate", "--export=__heap_base", "--export-memory", `--max-memory=${limit}`, "-o", output]);
  const module = load(output);
  const fresh = () => new WebAssembly.Instance(module).exports;
  const api = fresh();
  const base = (api.__heap_base.value + 15n) & -16n;
  assert.equal(api.allocate(BigInt(limit) - base - 16n), base + 16n);
  assert.equal(api.memory.buffer.byteLength, limit);
  assert.throws(() => api.allocate(1n), WebAssembly.RuntimeError);
  // The first request fails the size check; the second passes it but its block ends past the limit.
  assert.throws(() => fresh().allocate(BigInt(limit - 31)), WebAssembly.RuntimeError);
  assert.throws(() => fresh().allocate(BigInt(limit - 32)), WebAssembly.RuntimeError);
}

try {
  const root = resolve("tests/fixtures/wasm_memory");
  const hostAbi = resolve("tests/fixtures/host_abi");
  for (const optimization of [0, 3]) {
    const build = (name, source, ...options) => {
      const output = join(directory, `${name}-${optimization}.wasm`);
      cli(["build", source, "--target", "wasm64", `-O${optimization}`, "--no-cache", ...options, "-o", output]);
      return output;
    };
    const omitted = build("default", root);
    const explicit = build("explicit", root, "--wasm-max-memory", "16MiB", "--wasm-stack-size", "1MiB");
    assert.deepEqual(readFileSync(explicit), readFileSync(omitted));
    assert.deepEqual(readFileSync(build("default-again", root)), readFileSync(omitted));
    const defaults = definedMemory(readFileSync(omitted));
    // Flag 4 marks a 64-bit memory; flag 1 a declared maximum.
    assert.deepEqual({ flags: defaults.flags, maximum: defaults.maximum }, { flags: 5, maximum: 256 });
    const small = new WebAssembly.Instance(load(omitted)).exports;
    assert.equal(small.tz_bytes_length(1024n), 1024n);
    assert.throws(() => small.tz_bytes_length(32n * BigInt(MiB)), WebAssembly.RuntimeError);

    const large = build("large", root, "--wasm-max-memory", "64MiB");
    assert.deepEqual(definedMemory(readFileSync(large)), { flags: 5, minimum: defaults.minimum, maximum: 1024 });
    checkLimit(load(large), 64 * MiB, `wasm64 64 MiB O${optimization}`);
    const largest = build("largest", root, "--wasm-max-memory", "16GiB", "--wasm-stack-size", "4MiB");
    assert.deepEqual(definedMemory(readFileSync(largest)), { flags: 5, minimum: defaults.minimum + 48, maximum: 262144 });
    assert.equal(new WebAssembly.Instance(load(largest)).exports.tz_bytes_length(1024n), 1024n);

    const ir = join(directory, `limit-${optimization}.ll`);
    cli(["build", root, "--target", "wasm64", "--emit", "llvm", "--wasm-max-memory", "5GiB", `-O${optimization}`, "--no-cache", "-o", ir]);
    const text = readFileSync(ir, "utf8");
    for (const line of heapLines(5 * GiB)) assert.equal(occurrences(text, line), 1, line);
    assert.ok(text.includes("call i64 @llvm.wasm.memory.size.i64(i32 0)"));
    assert.ok(!text.includes("i32 @llvm.wasm.memory") && !text.includes("@tz.stack.check"));

    const object = join(directory, `object-${optimization}.o`), linked = join(directory, `object-${optimization}.wasm`);
    cli(["build", root, "--target", "wasm64", "--emit", "object", "--wasm-max-memory", "64MiB", `-O${optimization}`, "--no-cache", "-o", object]);
    execute(wasmLd, ["-mwasm64", "--no-entry", "--stack-first", "-z", "stack-size=1048576", "--max-memory=67108864", "--export=tz_bytes_length", object, "-o", linked]);
    checkLimit(load(linked), 64 * MiB, `wasm64 object O${optimization}`);

    for (const limit of [64 * MiB, 5 * GiB, 16 * GiB]) checkHeapBoundaries(limit, optimization);

    // Host ABI pointers are i64 (BigInt in JavaScript) and work above 4 GiB.
    for (const options of [[], ["--wasm-feature", "simd128"]]) {
      const api = new WebAssembly.Instance(load(build(`abi${options.length}`, hostAbi, "--wasm-max-memory", "8GiB", ...options))).exports;
      // Bumping the heap past 4 GiB writes only a block header, so the pages stay untouched.
      api.tsuzuri_alloc(BigInt(4 * GiB));
      const input = api.tsuzuri_alloc(32n), out = api.tsuzuri_alloc(16n);
      assert.ok(input > BigInt(4 * GiB) && out > BigInt(4 * GiB), `${input} ${out}`);
      new BigInt64Array(api.memory.buffer, Number(input), 3).set([1n, 2n, 3n]);
      assert.equal(api.tz_sum(input, 3n), 6n);
      api.tz_copy_values(out, input, 3n);
      const view = new DataView(api.memory.buffer);
      const pointer = view.getBigUint64(Number(out), true);
      assert.ok(pointer > BigInt(4 * GiB));
      assert.equal(view.getBigInt64(Number(out) + 8, true), 3n);
      assert.deepEqual([...new BigInt64Array(api.memory.buffer, Number(pointer), 3)], [1n, 2n, 3n]);
      api.tsuzuri_free(pointer);
      assert.throws(() => api.tz_sum(BigInt(api.memory.buffer.byteLength) - 8n, 2n), WebAssembly.RuntimeError);
    }

    const debug = build("debug", root, "-g", "--trap-info");
    assert.equal(new WebAssembly.Instance(load(debug)).exports.tz_bytes_length(1024n), 1024n);
    assert.ok(readFileSync(`${debug}.trap.json`, "utf8").length > 0);

    const failing = execute(compiler, ["test", root, "--target", "wasm64", `-O${optimization}`], false);
    assert.equal(failing.status, 1, `${failing.stdout}\n${failing.stderr}`);
    assert.match(failing.stdout, /not ok 2 - Main allocates 32 MiB/);
    const passing = cli(["test", root, "--target", "wasm64", "--wasm-max-memory", "64MiB", `-O${optimization}`]);
    assert.match(passing.stdout, /2 passed; 0 failed; 0 ignored/);
    console.log(`WASM64 O${optimization}: 64-bit memory, 64 MiB to 16 GiB limits, IR, object, heap boundaries, host buffers above 4 GiB, debug and tests`);
  }

  const threads = execute(compiler, ["build", root, "--target", "wasm64", "--wasm-feature", "threads", "-o", join(directory, "threads.wasm")], false);
  assert.ok(threads.stderr.includes("error[E2000]: --wasm-feature threads requires wasm32 object or WASM output"), threads.stderr);
  console.log("WASM64: threads stay wasm32-only");
} finally {
  rmSync(directory, { recursive: true, force: true });
}
