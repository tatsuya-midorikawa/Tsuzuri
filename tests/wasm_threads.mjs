import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { availableParallelism, tmpdir } from "node:os";
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
  // Threads that spin until stopped, so that the threads of a pool compete for the CPUs.
  const startBurners = (count) => {
    const stop = new Int32Array(new SharedArrayBuffer(4));
    const burners = Array.from({ length: count }, () => new Worker(
      "const { workerData } = require('node:worker_threads'); const flag = new Int32Array(workerData); while (Atomics.load(flag, 0) === 0) {}",
      { eval: true, workerData: stop.buffer },
    ));
    return async () => {
      Atomics.store(stop, 0, 1);
      await Promise.all(burners.map((burner) => new Promise((resolveBurner) => burner.once("exit", resolveBurner))));
    };
  };
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
      // F10: atomics and the children of a scope on real workers. A lost update or a child that runs
      // twice or never would change a total, whatever the schedule.
      const shared = join(directory, `concurrency-${optimization}.wasm`);
      cli(["build", resolve("tests/fixtures/concurrency_threads/Main.tz"), "--target", "wasm32", "--wasm-feature", "threads", `-O${optimization}`, "-o", shared]);
      const sharedPool = await createThreadPool(readFileSync(shared), { workers: 2 });
      try {
        for (const count of [0n, 1n, 4n, 64n, 257n]) {
          const total = count * (count + 1n) / 2n;
          assert.equal(sharedPool.call("tz_atomic_counter", count), total);
          assert.equal(sharedPool.call("tz_atomic_compare_exchange_loop", count), total);
          assert.equal(sharedPool.call("tz_arc_atomic_tasks", count), total * 1001n);
        }
        for (const count of [0n, 1n, 3n, 64n]) assert.equal(sharedPool.call("tz_atomic_contended", count), count * 1000n);
        // Mutex: a read-modify-write under the lock loses no update between workers, whatever the schedule.
        for (const count of [0n, 1n, 2n, 3n, 64n]) assert.equal(sharedPool.call("tz_mutex_contended", count), count * 1000n);
        for (const count of [0n, 1n, 4n, 64n, 257n]) {
          assert.equal(sharedPool.call("tz_mutex_total", count), count * count);
          assert.equal(sharedPool.call("tz_mutex_owned_array", count), count * count);
          assert.equal(sharedPool.call("tz_arc_mutex_tasks", count), count * (count + 1n) / 2n * 1000n + 1n);
          assert.equal(sharedPool.call("tz_mutex_twice", count), count * 20n);
        }
        for (const count of [0n, 1n, 2n, 1000n]) {
          let checksum = 0n;
          for (let position = 0n; position < count; position++) checksum = BigInt.asIntN(64, checksum * 31n + (3n * position + 1n) + position);
          assert.equal(sharedPool.call("tz_scope_results_order", count), checksum);
        }
        // What sleeps on the epoch waits for new work, the end of a group, a channel or a lock, so an
        // item that is done while others of its group still run wakes nobody: the epoch moves a few
        // times for a group of any size, not once for each of its items.
        const epochAddress = sharedPool.instance.exports.tsuzuri_threads_control() >>> 0;
        const epoch = () => Atomics.load(new Int32Array(sharedPool.memory.buffer, epochAddress, 7), 1);
        const startedAt = epoch();
        assert.equal(sharedPool.call("tz_atomic_counter", 5000n), 5000n * 5001n / 2n);
        assert.ok(epoch() - startedAt <= 8, `the epoch moved ${epoch() - startedAt} times for the 5,000 children of one scope`);
        assert.equal(sharedPool.call("tsuzuri_thread_heap_live_bytes"), 2n * (262144n + 16n));
      } finally {
        await sharedPool.close();
      }
      console.log(`WASM threads O${optimization}: Atomic, Mutex and Task.scope on 2 workers lose no update and run every child once`);
      // A critical section never waits, so a nested lock and parallel work inside one are refused on
      // the workers too. A trap stops the whole pool, so each case has a pool of its own.
      for (const name of ["mutex_nested", "mutex_parallel_inside"]) {
        const refused = await createThreadPool(readFileSync(shared), { workers: 2 });
        try {
          assert.throws(() => refused.call(`tz_${name}`), WebAssembly.RuntimeError, name);
        } finally {
          await refused.close();
        }
      }
      console.log(`WASM threads O${optimization}: a nested Mutex.with_lock and parallel work inside one trap`);
      // F10 Phase 2: channels between tasks that run at the same time, on 2 to 4 threads. The tasks
      // that wait on each other need a thread each, so the exports that need 3 are run from 3 up.
      const piped = join(directory, `channel-threads-${optimization}.wasm`);
      cli(["build", resolve("tests/fixtures/channel_threads/Main.tz"), "--target", "wasm32", "--wasm-feature", "threads", `-O${optimization}`, "-o", piped]);
      const pipedBytes = readFileSync(piped);
      // What nested_then_send returns: `parts` sums of index % 7 over the n indexes below n.
      const nestedSum = (parts, n) => parts * (n / 7n * 21n + (n % 7n) * ((n % 7n) - 1n) / 2n);
      const pipelines = [
        ["pipeline", [3n, 1000n], 500500n, 2], ["pipeline", [1n, 257n], 257n * 258n / 2n, 2],
        ["ping_pong", [2000n], 2000002000n, 2], ["tokens_across", [200n], 200200001n, 2],
        ["three_stages", [500n], 250500n, 3], ["fan_in", [2n, 50n, 3n], 5050100n, 3],
        ["job_queue", [100n, 4n], 338350100n, 3],
        ["nested_then_send", [2n, 100000n], nestedSum(2n, 100000n), 1], ["nested_then_send", [4n, 100000n], nestedSum(4n, 100000n), 1],
      ];
      for (const workers of [1, 2, 3]) {
        const pool = await createThreadPool(pipedBytes, { workers });
        try {
          for (let iteration = 0; iteration < 5; iteration++) {
            for (const [name, args, expected, needed] of pipelines) {
              if (workers + 1 >= needed) assert.equal(pool.call(`tz_${name}`, ...args), expected, `${name} on ${workers + 1} threads`);
            }
          }
          // Every channel block, token and Arc was freed: only the stacks of the workers are live.
          assert.equal(pool.call("tsuzuri_thread_heap_live_bytes"), BigInt(workers) * (262144n + 16n));
        } finally {
          await pool.close();
        }
      }
      // The least that a pipeline needs is a thread for each stage. A thread that waits on a channel
      // helps with an item that nobody has started only when no worker is free to take it, and the
      // thread that started the group has taken its first item under the lock that publishes it.
      // With every CPU busy, a worker can be late to its first item, which a fresh pool makes likely;
      // the third stage must not be stacked on the second then, because it would wait for the stage
      // below it.
      const stopBurners = startBurners(availableParallelism());
      try {
        for (let round = 0; round < 8; round++) {
          const crowded = await createThreadPool(pipedBytes, { workers: 2 });
          try {
            for (let call = 0; call < 4; call++) {
              assert.equal(crowded.call("tz_three_stages", 500n), 250500n, `three_stages on 3 threads on a busy machine, round ${round}`);
            }
          } finally {
            await crowded.close();
          }
        }
      } finally {
        await stopBurners();
      }
      console.log(`WASM threads O${optimization}: three stages on the 3 threads that they need complete on a busy machine`);
      // A task that starts a group of its own and then sends what the group computed to a task of the
      // outer group (nested_then_send). The thread that waits for the end of the nested group runs
      // that group's items and no others, as in the native pool, and the group's first item is taken
      // under the lock that publishes it. A thread that waits for the nested group and takes the
      // consumer of the outer group instead puts the consumer on top of the producer that must feed
      // it, and the pool reports a deadlock that the native pool does not have. Which thread is late
      // depends on the schedule, so the interleaving cannot be forced: the test runs fresh pools of 2
      // to 4 threads, and 2 threads again with a busy thread on every CPU, and is written to pass on
      // any schedule. A runtime that let the waiting thread take an item of any group trapped in some
      // pools of every size here.
      const nestedPools = async (workers, parts, pools, label) => {
        for (let round = 0; round < pools; round++) {
          const fresh = await createThreadPool(pipedBytes, { workers });
          try {
            for (let call = 0; call < 4; call++) {
              assert.equal(fresh.call("tz_nested_then_send", parts, 5000000n), nestedSum(parts, 5000000n), `${label}, pool ${round}, call ${call}`);
            }
          } finally {
            await fresh.close();
          }
        }
      };
      for (const [workers, parts] of [[1, 2n], [2, 3n], [3, 4n]]) await nestedPools(workers, parts, 10, `nested_then_send on ${workers + 1} threads`);
      const stopNestedBurners = startBurners(availableParallelism());
      try {
        await nestedPools(1, 2n, 12, "nested_then_send on 2 threads on a busy machine");
      } finally {
        await stopNestedBurners();
      }
      // One thread runs the items of a group in index order, so the producer is done before the consumer starts.
      const single = await createThreadPool(pipedBytes, { workers: 0 });
      try {
        assert.equal(single.call("tz_nested_then_send", 2n, 100000n), nestedSum(2n, 100000n));
      } finally {
        await single.close();
      }
      console.log(`WASM threads O${optimization}: a task that starts a group and then feeds a task of the outer group completes on 1 to 4 threads`);
      // With every thread waiting on a channel, no thread can ever send: a trap instead of a hang,
      // however many threads there are. The pipelines that need more threads than there are trap too.
      for (const [name, args, workers] of [["leaked_sender", [], 0], ["leaked_sender", [], 1], ["leaked_sender", [], 3],
        ["deadlock_pair", [], 0], ["deadlock_pair", [], 1], ["deadlock_pair", [], 2], ["deadlock_pair", [], 3],
        ["pipeline", [3n, 1000n], 0], ["ping_pong", [100n], 0], ["three_stages", [500n], 1]]) {
        const stuck = await createThreadPool(pipedBytes, { workers });
        try {
          assert.throws(() => stuck.call(`tz_${name}`, ...args), WebAssembly.RuntimeError, `${name} on ${workers + 1} threads`);
          assert.throws(() => stuck.call("tz_pipeline", 3n, 10n), /failed/);
        } finally {
          await stuck.close();
        }
      }
      console.log(`WASM threads O${optimization}: pipelines, fan-in, job queue and ping-pong on 2 to 4 threads, drops once, and deadlocks trap`);
      // The single-task exports of the channel suite on a pool, and the refusals inside a lock.
      const sequential = join(directory, `channel-${optimization}.wasm`);
      cli(["build", resolve("tests/fixtures/channel/Main.tz"), "--target", "wasm32", "--wasm-feature", "threads", `-O${optimization}`, "-o", sequential]);
      const sequentialBytes = readFileSync(sequential);
      const channelPool = await createThreadPool(sequentialBytes, { workers: 2 });
      try {
        for (const n of [1n, 10n, 257n]) assert.equal(channelPool.call("tz_roundtrip", n), n * (n + 1n) / 2n);
        assert.equal(channelPool.call("tz_fifo"), 123456n);
        assert.equal(channelPool.call("tz_closed"), 123000n);
        assert.equal(channelPool.call("tz_cloned"), 50n);
        assert.equal(channelPool.call("tz_text"), 54n);
        assert.equal(channelPool.call("tz_tokens", 7n), 7011n);
        assert.equal(channelPool.call("tz_nested"), 0n);
        // No group ran, so the pool never started: nothing at all is live (no leak, no worker stack).
        assert.equal(channelPool.call("tsuzuri_thread_heap_live_bytes"), 0n);
      } finally {
        await channelPool.close();
      }
      for (const name of ["bounded_zero", "bounded_negative", "full_deadlock", "empty_deadlock", "send_inside_lock", "recv_inside_lock"]) {
        const trapped = await createThreadPool(sequentialBytes, { workers: 2 });
        try {
          assert.throws(() => trapped.call(`tz_${name}`), WebAssembly.RuntimeError, name);
        } finally {
          await trapped.close();
        }
      }
      console.log(`WASM threads O${optimization}: the Channel exports of one task and the refusals inside a lock`);
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
