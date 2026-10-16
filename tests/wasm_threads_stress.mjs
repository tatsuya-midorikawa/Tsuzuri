// PR06 Phase 2: the WASM threads pool under many groups, nested groups, failing groups, scopes and pipelines, on pools of
// 1 to 7 workers. Every check has an exact expected value from a closed form, so a child that ran twice or never, a lost
// wakeup or a wrong deadlock verdict fails whatever the schedule. A lost wakeup blocks the main thread inside the module,
// where no timer runs, so a watchdog thread ends the process.
// Usage: node tests/wasm_threads_stress.mjs [compiler] [rounds]
import assert from "node:assert/strict";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { availableParallelism, tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { spawnSync } from "node:child_process";
import { Worker } from "node:worker_threads";
import { createThreadPool } from "../src/runtime/wasm-threads.mjs";

const compiler = resolve(process.argv[2] ?? "target/release/tsuzuri");
const rounds = Number(process.argv[3] ?? 12);
const stop = new Int32Array(new SharedArrayBuffer(4));
new Worker(
  "const { workerData } = require('node:worker_threads'); const flag = new Int32Array(workerData.buffer);"
  + " if (Atomics.wait(flag, 0, 0, workerData.milliseconds) === 'timed-out') { console.error('wasm_threads_stress: timed out'); process.kill(process.pid, 'SIGKILL'); }",
  { eval: true, workerData: { buffer: stop.buffer, milliseconds: 900_000 } },
).unref();

const directory = mkdtempSync(join(tmpdir(), "tsuzuri-wasm-threads-stress-"));
const build = (name, optimization) => {
  const output = join(directory, `${name}-${optimization}.wasm`);
  const result = spawnSync(compiler, ["build", resolve(`tests/fixtures/${name}/Main.tz`), "--target", "wasm32", "--wasm-feature", "threads", `-O${optimization}`, "-o", output], { encoding: "utf8", timeout: 240_000 });
  assert.equal(result.status, 0, `${result.error ?? ""}\n${result.stdout}\n${result.stderr}`);
  return readFileSync(output);
};
const startBurners = (count) => {
  const flag = new Int32Array(new SharedArrayBuffer(4));
  const burners = Array.from({ length: count }, () => new Worker(
    "const { workerData } = require('node:worker_threads'); const flag = new Int32Array(workerData); while (Atomics.load(flag, 0) === 0) {}",
    { eval: true, workerData: flag.buffer },
  ));
  return async () => {
    Atomics.store(flag, 0, 1);
    await Promise.all(burners.map((burner) => new Promise((done) => burner.once("exit", done))));
  };
};
const sumOfSquares = (count) => count * (count - 1n) * (2n * count - 1n) / 6n;
const checksum = (count) => {
  let value = 0n;
  for (let position = 0n; position < count; position++) value = BigInt.asIntN(64, value * 31n + (3n * position + 1n) + position);
  return value;
};
const nestedSum = (parts, n) => parts * (n / 7n * 21n + (n % 7n) * ((n % 7n) - 1n) / 2n);
const pipelines = [
  ["pipeline", [3n, 1000n], 500500n, 2], ["pipeline", [1n, 257n], 257n * 258n / 2n, 2],
  ["ping_pong", [2000n], 2000002000n, 2], ["tokens_across", [200n], 200200001n, 2],
  ["three_stages", [500n], 250500n, 3], ["fan_in", [2n, 50n, 3n], 5050100n, 3], ["job_queue", [100n, 4n], 338350100n, 3],
  ["nested_then_send", [2n, 100000n], nestedSum(2n, 100000n), 1], ["nested_then_send", [4n, 100000n], nestedSum(4n, 100000n), 1],
];
const stacks = (workers) => BigInt(workers) * (262144n + 16n);

try {
  for (const optimization of [0, 3]) {
    const tasks = build("tasks", optimization), shared = build("concurrency_threads", optimization), piped = build("channel_threads", optimization);
    for (const workers of [1, 2, 3, 5, 7]) {
      const pool = await createThreadPool(tasks, { workers });
      try {
        for (let round = 0; round < rounds; round++) {
          for (const count of [0n, 1n, 2n, 3n, 63n, 64n, 257n, 1024n, 2048n]) assert.equal(pool.call("tz_parallel_sum", count), sumOfSquares(count), `parallel_sum ${count}`);
          assert.equal(pool.call("tz_parallel_results_ok", 257n), 5625216n);
          assert.equal(pool.call("tz_parallel_results_lowest"), 2n);
          assert.equal(pool.call("tz_parallel_results_owned", 2048n), 6n);
          assert.equal(pool.call("tz_parallel_results_nested"), 2016n);
          assert.equal(pool.call("tz_parallel_results_first_class"), 42n);
          assert.equal(pool.call("tz_ordered", 128n), 1);
          assert.equal(pool.call("tz_nested", 5n), 2336n);
        }
        assert.equal(pool.call("tsuzuri_thread_heap_live_bytes"), stacks(workers));
      } finally {
        await pool.close();
      }
      const scopes = await createThreadPool(shared, { workers });
      try {
        for (let round = 0; round < rounds; round++) {
          for (const count of [0n, 1n, 4n, 64n, 257n, 1000n]) {
            assert.equal(scopes.call("tz_atomic_counter", count), count * (count + 1n) / 2n);
            assert.equal(scopes.call("tz_scope_results_order", count), checksum(count));
          }
          for (const count of [0n, 1n, 3n, 64n]) {
            assert.equal(scopes.call("tz_atomic_contended", count), count * 1000n);
            assert.equal(scopes.call("tz_mutex_contended", count), count * 1000n);
          }
        }
        assert.equal(scopes.call("tz_atomic_counter", 5000n), 5000n * 5001n / 2n);
        assert.equal(scopes.call("tsuzuri_thread_heap_live_bytes"), stacks(workers));
      } finally {
        await scopes.close();
      }
      const channels = await createThreadPool(piped, { workers });
      try {
        for (let round = 0; round < Math.max(2, rounds >> 2); round++) {
          for (const [name, args, expected, needed] of pipelines) {
            if (workers + 1 >= needed) assert.equal(channels.call(`tz_${name}`, ...args), expected, `${name} on ${workers + 1} threads`);
          }
        }
        assert.equal(channels.call("tsuzuri_thread_heap_live_bytes"), stacks(workers));
      } finally {
        await channels.close();
      }
    }
    // Every CPU busy: a worker is late to its first item, and a group that is joined must still finish exactly.
    const stopBurners = startBurners(availableParallelism());
    try {
      for (const workers of [2, 3]) {
        const crowded = await createThreadPool(piped, { workers });
        try {
          for (let round = 0; round < 4; round++) {
            assert.equal(crowded.call("tz_three_stages", 500n), 250500n);
            assert.equal(crowded.call("tz_nested_then_send", BigInt(workers), 100000n), nestedSum(BigInt(workers), 100000n));
          }
        } finally {
          await crowded.close();
        }
        const busy = await createThreadPool(tasks, { workers });
        try {
          for (let round = 0; round < rounds; round++) assert.equal(busy.call("tz_parallel_sum", 1024n), sumOfSquares(1024n));
        } finally {
          await busy.close();
        }
      }
    } finally {
      await stopBurners();
    }
    console.log(`WASM threads stress O${optimization}: groups, scopes and pipelines on 1 to 7 workers are exact (${rounds} rounds)`);
  }
} finally {
  Atomics.store(stop, 0, 1);
  Atomics.notify(stop, 0);
  rmSync(directory, { recursive: true, force: true });
}
