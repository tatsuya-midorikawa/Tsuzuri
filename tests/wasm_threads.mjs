import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { spawnSync } from "node:child_process";
import { Worker, isMainThread, parentPort, workerData } from "node:worker_threads";
import { pathToFileURL } from "node:url";
import { createThreadPool } from "../src/runtime/wasm-threads.mjs";

const threadImports = { spawn_workers() { throw new Error("test requires the thread pool"); }, worker_ready() {} };

// Independent of memoryLimits in the host: flags and page limits of the first memory import.
function importedMemory(bytes) {
  let position = 8;
  const leb = () => { let value = 0, shift = 0, byte; do { byte = bytes[position++]; value += (byte & 127) * 2 ** shift; shift += 7; } while (byte & 128); return value; };
  const skipName = () => { const length = leb(); position += length; };
  while (position < bytes.length) {
    const id = bytes[position++], size = leb(), end = position + size;
    for (let count = id === 2 ? leb() : 0; count > 0; count--) {
      skipName();
      skipName();
      const kind = bytes[position++];
      if (kind === 2) {
        const flags = bytes[position++], minimum = leb();
        return { flags, minimum, maximum: flags & 1 ? leb() : null };
      }
      if (kind === 0) leb(); else if (kind === 3) position += 2; else throw new Error(`unexpected import kind ${kind}`);
    }
    position = end;
  }
  return null;
}

if (!isMainThread) {
  const { module, memory, base, top, name, argument } = workerData;
  const exports = new WebAssembly.Instance(module, { env: { memory }, tsuzuri_threads: threadImports }).exports;
  exports.__stack_pointer.value = top;
  exports.tsuzuri_stack_base.value = base;
  exports.tsuzuri_stack_top.value = top;
  const address = exports.tsuzuri_thread_stack_probe();
  let result;
  try {
    result = exports[name](argument);
  } catch (error) {
    result = error instanceof WebAssembly.RuntimeError ? "trap" : String(error);
  }
  parentPort.postMessage({ address, result });
} else {
  const compiler = resolve(process.argv[2] ?? "target/release/tsuzuri");
  const directory = mkdtempSync(join(tmpdir(), "tsuzuri-wasm-threads-"));
  function cli(args) {
    const result = spawnSync(compiler, args, { encoding: "utf8", timeout: 120000 });
    assert.equal(result.status, 0, `${result.error ?? ""}\n${result.stdout}\n${result.stderr}`);
  }
  const runWorker = (workerData) => new Promise((resolveWorker, reject) => {
    const worker = new Worker(new URL(import.meta.url), { workerData });
    worker.once("error", reject);
    worker.once("message", (message) => worker.once("exit", code => code === 0 ? resolveWorker(message) : reject(new Error(`worker exit ${code}`))));
  });
  try {
    mkdirSync(join(directory, "frame"));
    const source = join(directory, "frame", "Main.tz");
    writeFileSync(source, "export def frame :: i64 -> i64\nfn frame seed = { let values = new [seed, seed + 1, seed + 2]; values[2] }\n");
    // Each level keeps an 8,000-byte frame array alive across a call that is not a tail call.
    mkdirSync(join(directory, "deep"));
    const deepSource = join(directory, "deep", "Main.tz");
    writeFileSync(deepSource, `def rec deep :: i64 -> i64 = \\n ->
    let values = [${Array.from({ length: 1000 }, (_, index) => `n + ${index}`).join(", ")}]
    if n == 0 then 0 else values[deep (n - 1) % 1000]

export def deep_call :: i64 -> i64 = \\n -> deep n
`);
    const barrierDirectory = join(directory, "barrier");
    mkdirSync(barrierDirectory);
    const barrierSource = join(barrierDirectory, "Main.tz");
    writeFileSync(barrierSource, `extern def barrier :: i64 -> i64
extern def worker_failure :: i64 -> i64
export def concurrent :: i64
fn concurrent = {
    let results = Task.run (Task.parallel (new [Task<i64>](3, index -> task { barrier index })));
    results[0] * 100 + results[1] * 10 + results[2]
}
export def fail :: i64
fn fail = {
    let results = Task.run (Task.parallel (new [Task<i64>](2, index -> task { worker_failure index })));
    results[0]
}
export def bulk :: i64
fn bulk = {
    let values = Parallel.init 20000 (\\index -> index);
    let mapped = Parallel.map (\\value -> value + 1) values;
    Array.sum (&mapped)
}
`);
    const importsFile = join(directory, "imports.mjs");
    writeFileSync(importsFile, `export function createImports({ workerId, data }) {
      const counters = new Int32Array(data);
      return { tsuzuri: {
        "Main.barrier": value => {
          Atomics.or(counters, 1, 1 << workerId);
          if (Atomics.add(counters, 0, 1) === 2) {
            Atomics.store(counters, 2, 1);
            Atomics.notify(counters, 2);
          } else {
            while (!Atomics.load(counters, 2)) Atomics.wait(counters, 2, 0);
          }
          return value;
        },
        "Main.worker_failure": value => {
          if (workerId !== 0) {
            Atomics.store(counters, 3, 1);
            Atomics.notify(counters, 3);
            throw new Error("intentional worker failure");
          }
          while (!Atomics.load(counters, 3)) Atomics.wait(counters, 3, 0);
          return value;
        }
      } };
    }\n`);
    for (const optimization of [0, 3]) {
      const output = join(directory, `threads-${optimization}.wasm`);
      cli(["build", source, "--target", "wasm32", "--wasm-feature", "threads", `-O${optimization}`, "-o", output]);
      const module = new WebAssembly.Module(readFileSync(output));
      assert.ok(WebAssembly.Module.imports(module).some(entry => entry.kind === "memory"));
      const memory = new WebAssembly.Memory({ initial: 256, maximum: 256, shared: true });
      const instance = new WebAssembly.Instance(module, { env: { memory }, tsuzuri_threads: threadImports });
      const size = instance.exports.tsuzuri_thread_stack_size();
      assert.equal(size, 262144);
      const mainAddress = instance.exports.tsuzuri_thread_stack_probe();
      const ranges = Array.from({ length: 3 }, () => {
        const base = instance.exports.tsuzuri_thread_stack_alloc();
        new Uint32Array(memory.buffer, base, 1)[0] = 0x12345678;
        return { base, top: base + size };
      });
      await Promise.all(ranges.map(async ({ base, top }) => {
        const { address, result } = await runWorker({ module, memory, base, top, name: "tz_frame", argument: 40n });
        assert.equal(result, 42n);
        assert.ok(address >= base && address < top);
        assert.ok(mainAddress < base || mainAddress >= top);
        assert.equal(new Uint32Array(memory.buffer, base, 1)[0], 0x12345678);
      }));
      assert.equal(instance.exports.tz_frame(40n), 42n);
      console.log(`WASM threads O${optimization}: imported shared memory, isolated worker stacks and concurrent heap reuse`);
      // Without entry checks, deep 64 on a 256 KiB worker stack overwrote the heap block below it.
      const deep = join(directory, `deep-${optimization}.wasm`);
      cli(["build", deepSource, "--target", "wasm32", "--wasm-feature", "threads", "--trap-info", `-O${optimization}`, "-o", deep]);
      const deepModule = new WebAssembly.Module(readFileSync(deep));
      const deepMemory = new WebAssembly.Memory({ initial: 256, maximum: 256, shared: true });
      const deepApi = new WebAssembly.Instance(deepModule, { env: { memory: deepMemory }, tsuzuri_threads: threadImports }).exports;
      const neighbor = deepApi.tsuzuri_thread_stack_alloc() >>> 0;
      const stack = deepApi.tsuzuri_thread_stack_alloc() >>> 0;
      const stackTop = stack + deepApi.tsuzuri_thread_stack_size();
      assert.equal(stack, neighbor + 262144 + 16);
      new Uint8Array(deepMemory.buffer, neighbor, 262144).fill(0xa5);
      const stackData = { module: deepModule, memory: deepMemory, base: stack, top: stackTop, name: "tz_deep_call" };
      const expectedDeep = (count) => {
        let value = 0n;
        for (let level = 1n; level <= count; level++) value = level + value % 1000n;
        return value;
      };
      assert.equal((await runWorker({ ...stackData, argument: 4n })).result, expectedDeep(4n));
      assert.equal((await runWorker({ ...stackData, argument: 64n })).result, "trap");
      // The worker records its site in shared memory, where the main instance reads it.
      const deepSites = JSON.parse(readFileSync(`${deep}.trap.json`, "utf8")).sites;
      assert.equal(deepSites.find((site) => site.id === deepApi.tsuzuri_trap_site())?.kind, "stack overflow");
      assert.ok(new Uint8Array(deepMemory.buffer, neighbor, 262144).every(value => value === 0xa5));
      assert.equal(deepApi.tz_deep_call(64n), expectedDeep(64n));
      assert.throws(() => deepApi.tz_deep_call(200n), WebAssembly.RuntimeError);
      console.log(`WASM threads O${optimization}: a worker stack overflow traps before writing the heap block below it`);
      // Checked frames keep STACK_CHECK_MARGIN (4096 bytes, src/llvm.rs) for the unchecked thread runtime and i128 helpers.
      let unchecked = 0;
      for (const [language, input, flags] of [
        ["c", "src/runtime/task-wasm-threads.c", ["-std=c11", "-ffreestanding", "-fno-stack-protector", "-matomics"]],
        ["ir", "src/runtime/wasm.ll", ["-Wno-override-module"]],
      ]) {
        const object = join(directory, `unchecked-${language}-${optimization}.o`);
        const compiled = spawnSync(process.env.TSUZURI_CLANG ?? "clang", ["-x", language, "--target=wasm32-unknown-unknown", "-mbulk-memory", `-O${optimization}`, "-fstack-usage", ...flags, "-c", resolve(input), "-o", object], { encoding: "utf8", timeout: 120000 });
        assert.equal(compiled.status, 0, `${compiled.error ?? ""}\n${compiled.stderr}`);
        for (const line of readFileSync(object.replace(/\.o$/, ".su"), "utf8").trim().split("\n")) unchecked += Number(line.split("\t")[1]);
      }
      assert.ok(unchecked > 0 && unchecked <= 4096, `${unchecked}`);
      const tasks = join(directory, `tasks-${optimization}.wasm`);
      cli(["build", resolve("tests/fixtures/tasks/Main.tz"), "--target", "wasm32", "--wasm-feature", "threads", `-O${optimization}`, "-o", tasks]);
      const repeated = join(directory, `repeated-${optimization}.wasm`);
      cli(["build", resolve("tests/fixtures/tasks/Main.tz"), "--target", "wasm32", "--wasm-feature", "threads", `-O${optimization}`, "-o", repeated]);
      assert.deepEqual(readFileSync(tasks), readFileSync(repeated));
      const pool = await createThreadPool(readFileSync(tasks), { workers: 2 });
      try {
        assert.equal(pool.memory.buffer.byteLength, 16 * 1024 * 1024);
        assert.equal(pool.workerCount, 0);
        for (const count of [0n, 1n, 64n, 1024n]) {
          assert.equal(pool.call("tz_parallel_sum", count), count * (count - 1n) * (2n * count - 1n) / 6n);
        }
        assert.equal(pool.workerCount, 2);
        assert.equal(pool.call("tz_parallel_results_ok", 257n), 5625216n);
        assert.equal(pool.call("tz_parallel_results_nested"), 2016n);
        assert.equal(pool.call("tz_parallel_results_first_class"), 42n);
        assert.equal(pool.call("tz_ordered", 128n), 1);
        assert.equal(pool.call("tz_nested", 5n), 2336n);
        const reserved = 2n * (262144n + 16n);
        assert.equal(pool.call("tsuzuri_thread_heap_live_bytes"), reserved);
        for (let iteration = 0; iteration < 100; iteration++) {
          assert.equal(pool.call("tz_owned_results"), 12n);
          assert.equal(pool.call("tz_record_results"), 17n);
          assert.equal(pool.call("tz_function_results"), 42n);
          assert.equal(pool.call("tz_list_results"), 42n);
          assert.equal(pool.call("tz_parallel_results_lowest"), 2n);
          assert.equal(pool.call("tz_parallel_results_owned", 257n), 6n);
          assert.equal(pool.call("tz_parallel_results_values"), 10n);
          assert.equal(pool.call("tz_parallel_results_functions"), 42n);
          assert.equal(pool.call("tz_parallel_results_recursive", 0), 2n);
          assert.equal(pool.call("tz_parallel_results_recursive", 1), 5n);
          assert.equal(pool.call("tsuzuri_thread_heap_live_bytes"), reserved);
        }
        assert.ok(pool.memory.buffer.byteLength <= 16 * 1024 * 1024);
        console.log(`WASM tasks O${optimization}: ordered results, nested groups, 2 persistent workers and owned allocation stress`);
      } finally {
        await pool.close();
      }
      const large = join(directory, `tasks-64-${optimization}.wasm`);
      cli(["build", resolve("tests/fixtures/tasks/Main.tz"), "--target", "wasm32", "--wasm-feature", "threads", "--wasm-max-memory", "64MiB", `-O${optimization}`, "-o", large]);
      const largeBytes = readFileSync(large);
      const declared = importedMemory(largeBytes);
      assert.equal(declared.flags, 3);
      assert.equal(declared.maximum, 1024);
      assert.equal(declared.minimum, importedMemory(readFileSync(tasks)).minimum);
      const largePool = await createThreadPool(largeBytes, { workers: 2 });
      try {
        assert.equal(largePool.memory.buffer.byteLength, 67108864);
        for (const count of [0n, 1n, 64n, 1024n]) {
          assert.equal(largePool.call("tz_parallel_sum", count), count * (count - 1n) * (2n * count - 1n) / 6n);
        }
        assert.equal(largePool.call("tsuzuri_thread_heap_live_bytes"), 2n * (262144n + 16n));
      } finally {
        await largePool.close();
      }
      console.log(`WASM threads O${optimization}: --wasm-max-memory 64MiB declares 1024 shared pages and the host reserves them`);
      assert.throws(() => new WebAssembly.Instance(new WebAssembly.Module(readFileSync(tasks)), { env: { memory } }));
      const unavailable = new WebAssembly.Instance(new WebAssembly.Module(readFileSync(tasks)), { env: { memory: new WebAssembly.Memory({ initial: 256, maximum: 256, shared: true }) }, tsuzuri_threads: { spawn_workers: () => -1, worker_ready() {} } });
      unavailable.exports.tsuzuri_threads_init(2);
      assert.throws(() => unavailable.exports.tz_parallel_sum(2n), WebAssembly.RuntimeError);
      const barrierOutput = join(directory, `barrier-${optimization}.wasm`);
      cli(["build", barrierSource, "--target", "wasm32", "--wasm-feature", "threads", "--wasm-feature", "simd128", "--trap-info", "-g", `-O${optimization}`, "-o", barrierOutput]);
      const counters = new SharedArrayBuffer(16);
      const barrierPool = await createThreadPool(readFileSync(barrierOutput), { workers: 2, importsModule: pathToFileURL(importsFile).href, importData: counters });
      try {
        assert.equal(barrierPool.call("tz_concurrent"), 12n);
        assert.equal(Atomics.load(new Int32Array(counters), 1), 0b111);
        assert.equal(barrierPool.call("tz_bulk"), 200010000n);
        assert.equal(barrierPool.call("tsuzuri_thread_heap_live_bytes"), 2n * (262144n + 16n));
        assert.throws(() => barrierPool.call("tz_fail"), WebAssembly.RuntimeError);
        assert.throws(() => barrierPool.call("tz_bulk"), /failed/);
      } finally {
        await barrierPool.close();
      }
      console.log(`WASM threads O${optimization}: atomic barrier proves 3 participants; bulk callbacks, worker failure, unavailable host and SIMD/debug options passed`);
      const object = join(directory, `threads-${optimization}.o`);
      const linked = join(directory, `linked-${optimization}.wasm`);
      cli(["build", source, "--target", "wasm32", "--emit", "object", "--wasm-feature", "threads", `-O${optimization}`, "-o", object]);
      const link = spawnSync(process.env.TSUZURI_WASM_LD ?? "wasm-ld", ["--no-entry", "--shared-memory", "--import-memory", "--stack-first", "-z", "stack-size=1048576", "--max-memory=16777216", "--export-all", "--export=__stack_pointer", object, "-o", linked], { encoding: "utf8", timeout: 120000 });
      assert.equal(link.status, 0, `${link.error ?? ""}\n${link.stderr}`);
      const objectPool = await createThreadPool(readFileSync(linked), { workers: 0 });
      try { assert.equal(objectPool.call("tz_frame", 40n), 42n); } finally { await objectPool.close(); }
      const plain = join(directory, `plain-${optimization}.wasm`);
      cli(["build", source, "--target", "wasm32", `-O${optimization}`, "-o", plain]);
      assert.deepEqual(WebAssembly.Module.imports(new WebAssembly.Module(readFileSync(plain))), []);
      await assert.rejects(createThreadPool(readFileSync(plain)), TypeError);
    }
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
}
