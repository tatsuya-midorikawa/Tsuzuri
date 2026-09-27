import { availableParallelism } from "node:os";
import { Worker, isMainThread, parentPort, workerData } from "node:worker_threads";

function fail(memory, address) {
  const control = new Int32Array(memory.buffer, address, 7);
  Atomics.store(control, 0, 1);
  for (const index of [2, 3, 4]) {
    Atomics.or(control, index, 0x80000000);
    Atomics.notify(control, index);
  }
  Atomics.add(control, 1, 1);
  Atomics.notify(control, 1);
}

async function userImports(url, memory, workerId, data) {
  return url ? (await import(url)).createImports({ memory, workerId, data }) : {};
}

if (!isMainThread && workerData?.tsuzuriThreads) {
  const { module, memory, top, workerId, control, importsModule, importData } = workerData;
  try {
    const imports = await userImports(importsModule, memory, workerId, importData);
    const instance = new WebAssembly.Instance(module, {
      ...imports,
      env: { ...imports.env, memory },
      tsuzuri_threads: {
        spawn_workers() { throw new Error("worker cannot create a second thread pool"); },
        worker_ready() {},
      },
    });
    instance.exports.__stack_pointer.value = top;
    instance.exports.tsuzuri_thread_entry(workerId);
  } catch (error) {
    fail(memory, control);
    parentPort.postMessage({ error: String(error) });
    process.exitCode = 1;
  }
  parentPort.close();
}

export async function createThreadPool(bytes, {
  workers = Math.min(availableParallelism(), 32) - 1,
  memory = new WebAssembly.Memory({ initial: 256, maximum: 256, shared: true }),
  importsModule,
  importData,
} = {}) {
  if (!Number.isInteger(workers) || workers < 0 || workers > 31) {
    throw new RangeError("WASM worker count must be an integer from 0 to 31");
  }
  if (!(memory.buffer instanceof SharedArrayBuffer)) {
    throw new TypeError("WASM threads require shared WebAssembly.Memory");
  }
  const module = bytes instanceof WebAssembly.Module ? bytes : new WebAssembly.Module(bytes);
  const imports = await userImports(importsModule, memory, 0, importData);
  const running = [];
  let closed = false;
  let instance;
  let control;
  instance = new WebAssembly.Instance(module, {
    ...imports,
    env: { ...imports.env, memory },
    tsuzuri_threads: {
      spawn_workers(desired) {
        if (running.length || desired !== workers) throw new Error("invalid WASM worker request");
        try {
          for (let index = 0; index < desired; index++) {
            const base = instance.exports.tsuzuri_thread_stack_alloc();
            const top = base + instance.exports.tsuzuri_thread_stack_size();
            const worker = new Worker(new URL(import.meta.url), {
              workerData: { tsuzuriThreads: true, module, memory, top, workerId: index + 1, control, importsModule, importData },
            });
            worker.on("error", () => fail(memory, control));
            worker.on("exit", code => { if (!closed && code !== 0) fail(memory, control); });
            running.push(worker);
          }
          return running.length;
        } catch (error) {
          fail(memory, control);
          throw error;
        }
      },
      worker_ready() {},
    },
  });
  control = instance.exports.tsuzuri_threads_control();
  instance.exports.tsuzuri_threads_init(workers);
  return {
    instance,
    memory,
    get workerCount() { return running.length; },
    call(name, ...args) {
      if (closed || Atomics.load(new Int32Array(memory.buffer, control, 7), 0)) {
        throw new Error("WASM thread pool is closed or failed");
      }
      try {
        return instance.exports[name](...args);
      } catch (error) {
        fail(memory, control);
        throw error;
      }
    },
    async close() {
      closed = true;
      await Promise.all(running.map(worker => worker.terminate()));
    },
  };
}
