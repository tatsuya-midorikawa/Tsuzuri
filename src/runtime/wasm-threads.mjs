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
  const { module, memory, base, top, workerId, control, importsModule, importData } = workerData;
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
    const exports = instance.exports;
    exports.__stack_pointer.value = top;
    // Function entries trap when the stack pointer leaves these bounds.
    if (exports.tsuzuri_stack_top) {
      exports.tsuzuri_stack_base.value = base;
      exports.tsuzuri_stack_top.value = top;
    }
    exports.tsuzuri_thread_entry(workerId);
  } catch (error) {
    fail(memory, control);
    parentPort.postMessage({ error: String(error) });
    process.exitCode = 1;
  }
  parentPort.close();
}

// Returns the limits (in 64 KiB pages) of the shared env.memory import of WASM module bytes.
export function memoryLimits(bytes) {
  const data = ArrayBuffer.isView(bytes) ? new Uint8Array(bytes.buffer, bytes.byteOffset, bytes.byteLength) : new Uint8Array(bytes);
  let position = 8;
  const leb = () => {
    let result = 0, shift = 0, byte;
    do {
      byte = data[position++] ?? 0;
      result += (byte & 127) * 2 ** shift;
      shift += 7;
    } while (byte & 128);
    return result;
  };
  const name = () => {
    const length = leb();
    position += length;
    return new TextDecoder().decode(data.subarray(position - length, position));
  };
  if (data[0] === 0 && data[1] === 0x61 && data[2] === 0x73 && data[3] === 0x6d) {
    while (position < data.length) {
      const id = data[position++], end = leb() + position;
      if (id === 2) {
        for (let count = leb(); count > 0 && position < end; count--) {
          const module = name(), field = name(), kind = data[position++];
          if (kind === 2) {
            const flags = data[position++], minimum = leb(), maximum = flags & 1 ? leb() : undefined;
            if (module === "env" && field === "memory" && (flags & 3) === 3) return { flags, minimum, maximum };
          } else if (kind === 1) {
            position++;
            const flags = data[position++];
            leb();
            if (flags & 1) leb();
          } else if (kind === 3) {
            position += 2;
          } else {
            if (kind === 4) position++;
            leb();
          }
        }
        break;
      }
      position = end;
    }
  }
  throw new TypeError("WASM threads module must import a shared env.memory with a maximum; build it with --wasm-feature threads");
}

export async function createThreadPool(bytes, {
  workers = Math.min(availableParallelism(), 32) - 1,
  memory,
  importsModule,
  importData,
} = {}) {
  if (!Number.isInteger(workers) || workers < 0 || workers > 31) {
    throw new RangeError("WASM worker count must be an integer from 0 to 31");
  }
  if (memory === undefined) {
    // A compiled Module does not expose its limits, so it keeps the default 16 MiB.
    const pages = bytes instanceof WebAssembly.Module ? 256 : memoryLimits(bytes).maximum;
    try {
      memory = new WebAssembly.Memory({ initial: pages, maximum: pages, shared: true });
    } catch (cause) {
      throw new RangeError(`cannot reserve ${pages * 64} KiB of shared WebAssembly.Memory; build with a smaller --wasm-max-memory`, { cause });
    }
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
            // wasm32 pointers at or above 2 GiB arrive as negative numbers.
            const base = instance.exports.tsuzuri_thread_stack_alloc() >>> 0;
            const top = base + instance.exports.tsuzuri_thread_stack_size();
            const worker = new Worker(new URL(import.meta.url), {
              workerData: { tsuzuriThreads: true, module, memory, base, top, workerId: index + 1, control, importsModule, importData },
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
  control = instance.exports.tsuzuri_threads_control() >>> 0;
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
