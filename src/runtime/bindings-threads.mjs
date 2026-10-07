// The thread pool of `--emit bindings-js --wasm-feature threads` (E13 Phase 2), after the table and
// bindings-core.mjs. It follows the protocol of src/runtime/wasm-threads.mjs on Web Workers: one
// shared memory, a coordinator worker whose instance runs the exports, and helper workers that run
// `tsuzuri_thread_entry`. A browser's main thread cannot wait on atomics, so the exports run in the
// coordinator and every call returns a Promise. The helpers start before any call: a worker that a
// waiting thread creates cannot start until that thread returns to its event loop. A page that is
// not cross-origin isolated gets an error; nothing falls back to sequential execution.

const ISOLATION = "WASM threads need a cross-origin isolated page: serve it from HTTPS or localhost with 'Cross-Origin-Opener-Policy: same-origin' and 'Cross-Origin-Embedder-Policy: require-corp' (crossOriginIsolated is false)";
const ROLE = "tsuzuri-worker";
// Int32 words of the start-up record: the go flag (1 start, 2 closed), the first failure's reason
// (1 trap, 2 stack, 3 other, 4 claimed) and site, then [base, top] of each helper's stack.
const GO = 0, REASON = 1, SITE = 2, SLOTS = 3, MAX_WORKERS = 31;

// Fails the pool as createThreadPool does: the shared flag, the lock poison bits, and every waiter.
function poison(memory, address) {
  const control = new Int32Array(memory.buffer, address, 7);
  Atomics.store(control, 0, 1);
  for (const index of [2, 3, 4]) {
    Atomics.or(control, index, 0x80000000);
    Atomics.notify(control, index);
  }
  Atomics.add(control, 1, 1);
  Atomics.notify(control, 1);
}

// The page count of the shared `env.memory` that the bytes of a threads module import.
function sharedPages(bytes) {
  const data = ArrayBuffer.isView(bytes) ? new Uint8Array(bytes.buffer, bytes.byteOffset, bytes.byteLength) : new Uint8Array(bytes);
  let position = 8;
  const leb = () => {
    let result = 0, shift = 0, byte;
    do {
      byte = data[position++] ?? 0;
      result += (byte & 127) * 2 ** shift;
      shift += 7;
    } while (byte & 128 && position < data.length);
    return result;
  };
  const name = () => {
    const length = leb();
    position += length;
    return new TextDecoder().decode(data.subarray(position - length, position));
  };
  while (position < data.length) {
    const id = data[position++], end = leb() + position;
    if (id === 2) {
      for (let count = leb(); count > 0 && position < end; count--) {
        const module = name(), field = name(), kind = data[position++];
        if (kind === 2) {
          const flags = data[position++];
          leb();
          const maximum = flags & 1 ? leb() : undefined;
          if (module === "env" && field === "memory" && (flags & 3) === 3) return maximum;
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
  throw new TypeError("WASM threads module must import a shared env.memory with a maximum; build it with --wasm-feature threads");
}

async function hostImportsOf(importsModule, workerId, data) {
  if (importsModule === undefined) return {};
  const imported = await import(String(importsModule));
  if (typeof imported.createImports !== "function") throw new TypeError("the imports module must export createImports({ workerId, data })");
  const hostImports = await imported.createImports({ workerId, data });
  if (hostImports === null || typeof hostImports !== "object") throw new TypeError("createImports must return an object of host functions");
  return hostImports;
}

function describeError(error) {
  if (error instanceof TsuzuriTrap) return { type: "TsuzuriTrap", trap: error.trap };
  if (error instanceof TypeError || error instanceof RangeError) return { type: error.name, message: error.message };
  try {
    return { type: "value", value: structuredClone(error) };
  } catch {
    return { type: "Error", message: String(error) };
  }
}

function errorOf(description) {
  switch (description.type) {
    case "TsuzuriTrap": return new TsuzuriTrap(description.trap);
    case "TypeError": return new TypeError(description.message);
    case "RangeError": return new RangeError(description.message);
    case "value": return description.value;
    default: return new Error(description.message);
  }
}

function transferables(value) {
  return ArrayBuffer.isView(value) ? [value.buffer] : [];
}

// The worker side: this file again, loaded as a module worker with ?tsuzuri-worker=<role>.
const role = (() => {
  try {
    return new URL(import.meta.url).searchParams.get(ROLE);
  } catch {
    return null;
  }
})();

async function coordinator({ module, memory, bootstrap, workers, importsModule, importData, sites }) {
  const hostImports = await hostImportsOf(importsModule, 0, importData);
  const used = inspect(module, true);
  checkHostImports(used, hostImports);
  const words = new Int32Array(bootstrap);
  const slots = new Uint32Array(bootstrap);
  let control = 0;
  let poisoned = false;
  const bound = bind(module, used, hostImports, sites, {
    recreate: false,
    extra: (owner) => ({
      env: { memory },
      tsuzuri_threads: {
        spawn_workers(desired) {
          if (desired !== workers) throw new Error("invalid WASM worker request");
          for (let index = 0; index < desired; index++) {
            // wasm32 pointers at or above 2 GiB arrive as negative numbers.
            const base = owner.exports.tsuzuri_thread_stack_alloc() >>> 0;
            slots[SLOTS + 2 * index] = base;
            slots[SLOTS + 2 * index + 1] = base + owner.exports.tsuzuri_thread_stack_size();
          }
          Atomics.store(words, GO, 1);
          Atomics.notify(words, GO);
          return desired;
        },
        worker_ready() {},
      },
    }),
    // A helper's trap stops the coordinator through the pool's failure flag, without a site of its own.
    trapOf(own) {
      const reason = Atomics.load(words, REASON);
      if (own.reason !== "trap" || own.site !== 0 || (reason !== 1 && reason !== 2)) return own;
      return { reason: reason === 2 ? "stack" : "trap", site: Atomics.load(words, SITE) >>> 0 };
    },
  });
  const owner = await bound.start();
  control = owner.exports.tsuzuri_threads_control() >>> 0;
  owner.exports.tsuzuri_threads_init(workers);
  return ({ id, name, args }) => {
    try {
      const value = bound.exports[name](...args);
      postMessage({ kind: "result", id, value }, transferables(value));
    } catch (error) {
      // An argument error leaves the instance alive; anything else stops the pool.
      if (!bound.alive && !poisoned) {
        poisoned = true;
        poison(memory, control);
      }
      postMessage({ kind: "error", id, error: describeError(error), failed: !bound.alive });
    }
  };
}

async function helper({ module, memory, bootstrap, workerId, importsModule, importData }) {
  const hostImports = await hostImportsOf(importsModule, workerId, importData);
  const used = inspect(module, true);
  checkHostImports(used, hostImports);
  const bound = bind(module, used, hostImports, [], {
    recreate: false,
    extra: () => ({
      env: { memory },
      tsuzuri_threads: {
        spawn_workers() {
          throw new Error("a WASM worker cannot start workers");
        },
        worker_ready() {},
      },
    }),
  });
  const { exports } = await bound.start();
  postMessage({ kind: "ready" });
  const words = new Int32Array(bootstrap);
  const slots = new Uint32Array(bootstrap);
  while (Atomics.load(words, GO) === 0) Atomics.wait(words, GO, 0);
  if (Atomics.load(words, GO) !== 1) return;
  const base = slots[SLOTS + 2 * (workerId - 1)];
  const top = slots[SLOTS + 2 * (workerId - 1) + 1];
  exports.__stack_pointer.value = top;
  // Function entries trap when the stack pointer leaves these bounds.
  if (exports.tsuzuri_stack_top) {
    exports.tsuzuri_stack_base.value = base;
    exports.tsuzuri_stack_top.value = top;
  }
  try {
    exports.tsuzuri_thread_entry(workerId);
  } catch (error) {
    // The first failure records its reason and site for the coordinator, then stops the pool.
    if (Atomics.compareExchange(words, REASON, 0, 4) === 0) {
      let site = 0;
      try {
        site = error instanceof WebAssembly.RuntimeError ? (exports.tsuzuri_trap_site?.() ?? 0) >>> 0 : 0;
      } catch {
        site = 0;
      }
      Atomics.store(words, SITE, site);
      Atomics.store(words, REASON, error instanceof WebAssembly.RuntimeError ? 1 : error instanceof RangeError || error?.name === "InternalError" ? 2 : 3);
    }
    poison(memory, exports.tsuzuri_threads_control() >>> 0);
    postMessage({ kind: "failed", error: describeError(error) });
  }
}

if (role === "coordinator" || role === "helper") {
  const queued = [];
  let handle;
  addEventListener("message", (event) => {
    if (handle) handle(event.data);
    else queued.push(event.data);
  });
  handle = async (message) => {
    if (message?.kind !== "init") return;
    handle = (data) => queued.push(data);
    try {
      if (role === "helper") {
        await helper(message);
        return;
      }
      const call = await coordinator(message);
      handle = call;
      postMessage({ kind: "ready" });
      for (const pending of queued.splice(0)) call(pending);
    } catch (error) {
      postMessage({ kind: "failed", error: describeError(error) });
    }
  };
  if (queued.length) handle(queued.shift());
}

/** Starts the thread pool of a wasm32 module built with --wasm-feature threads from the same sources. */
export async function load(source, options = {}) {
  if (globalThis.crossOriginIsolated !== true || typeof SharedArrayBuffer !== "function") throw new Error(ISOLATION);
  if (typeof Worker !== "function") throw new Error("WASM threads bindings need module Web Workers");
  if (options === null || typeof options !== "object") throw new TypeError("load options must be an object");
  const hardware = globalThis.navigator?.hardwareConcurrency ?? 1;
  const { workers = Math.max(0, Math.min(hardware, MAX_WORKERS + 1) - 1), importsModule, importData, memory: given, sites = [] } = options;
  if (!Number.isInteger(workers) || workers < 0 || workers > MAX_WORKERS) throw new RangeError("WASM worker count must be an integer from 0 to 31");
  if (!Array.isArray(sites)) throw new TypeError("load option 'sites' must be an array of trap sites");
  if (importsModule !== undefined && typeof importsModule !== "string" && !(importsModule instanceof URL)) {
    throw new TypeError("load option 'importsModule' must be the URL of a module that exports createImports");
  }
  const module = await compileSource(source);
  const used = inspect(module, true);
  if (used.length && importsModule === undefined) {
    throw new TypeError(`missing import '${used[0][0]}': pass importsModule, the URL of a module whose createImports returns the host functions`);
  }
  let memory = given;
  if (memory === undefined) {
    // A compiled Module does not expose its limits, so it keeps the default 16 MiB.
    const pages = source instanceof WebAssembly.Module ? 256 : sharedPages(source);
    try {
      memory = new WebAssembly.Memory({ initial: pages, maximum: pages, shared: true });
    } catch (cause) {
      throw new RangeError(`cannot reserve ${pages * 64} KiB of shared WebAssembly.Memory; build with a smaller --wasm-max-memory`, { cause });
    }
  }
  if (!(memory instanceof WebAssembly.Memory) || tagOf(memory.buffer) !== "[object SharedArrayBuffer]") {
    throw new TypeError("WASM threads require a shared WebAssembly.Memory");
  }
  const bootstrap = new SharedArrayBuffer(4 * (SLOTS + 2 * MAX_WORKERS));
  const imports = importsModule === undefined ? undefined : String(importsModule);
  const started = [];
  const pending = new Map();
  let next = 0;
  let failure;
  let closed = false;

  function stop(error) {
    failure ??= error;
    for (const [, { reject }] of pending) reject(failure);
    pending.clear();
  }
  function spawn(name) {
    const url = new URL(import.meta.url);
    url.searchParams.set(ROLE, name);
    const worker = new Worker(url, { type: "module", name: `tsuzuri-${name}` });
    started.push(worker);
    return worker;
  }
  function ready(worker, init) {
    return new Promise((resolve, reject) => {
      const onError = (event) => reject(new Error(`a WASM worker failed to start: ${event?.message ?? "script error"}`));
      worker.addEventListener("error", onError);
      worker.addEventListener("message", function first(event) {
        if (event.data?.kind === "ready") resolve();
        else if (event.data?.kind === "failed") reject(errorOf(event.data.error));
        else return;
        worker.removeEventListener("message", first);
        worker.removeEventListener("error", onError);
      });
      worker.postMessage(init);
    });
  }

  let coordinatorWorker;
  try {
    const helpers = [];
    for (let workerId = 1; workerId <= workers; workerId++) {
      helpers.push(ready(spawn("helper"), { kind: "init", module, memory, bootstrap, workerId, importsModule: imports, importData }));
    }
    coordinatorWorker = spawn("coordinator");
    await Promise.all([...helpers, ready(coordinatorWorker, { kind: "init", module, memory, bootstrap, workers, importsModule: imports, importData, sites })]);
  } catch (error) {
    Atomics.store(new Int32Array(bootstrap), GO, 2);
    Atomics.notify(new Int32Array(bootstrap), GO);
    for (const worker of started) worker.terminate();
    throw error;
  }
  for (const worker of started) {
    worker.addEventListener("error", (event) => stop(new Error(`a WASM worker failed: ${event?.message ?? "script error"}`)));
    if (worker !== coordinatorWorker) {
      worker.addEventListener("message", (event) => {
        if (event.data?.kind === "failed") failure ??= new Error("a WASM worker failed; load the module again", { cause: errorOf(event.data.error) });
      });
    }
  }
  coordinatorWorker.addEventListener("message", (event) => {
    const message = event.data;
    const call = pending.get(message?.id);
    if (!call) return;
    pending.delete(message.id);
    if (message.kind === "result") {
      call.resolve(message.value);
      return;
    }
    const error = errorOf(message.error);
    if (message.failed) failure ??= error;
    call.reject(error);
  });

  const resolve = resolver();
  const exports = {};
  for (const [name, parameterTypes] of TABLE.exports) {
    const check = argumentChecker(name, parameterTypes.map(resolve), false);
    const call = (...args) => new Promise((resolveCall, reject) => {
      if (closed) throw new Error("the WASM thread pool is closed");
      if (failure !== undefined) throw new Error("the WASM thread pool stopped after a failure; load the module again", { cause: failure });
      check(args);
      const id = next++;
      pending.set(id, { resolve: resolveCall, reject });
      coordinatorWorker.postMessage({ kind: "call", id, name, args });
    });
    Object.defineProperty(exports, name, { value: call, enumerable: true });
  }
  Object.freeze(exports);
  return Object.freeze({
    exports,
    workerCount: workers,
    async close() {
      if (closed) return;
      closed = true;
      stop(new Error("the WASM thread pool is closed"));
      const words = new Int32Array(bootstrap);
      if (Atomics.load(words, GO) === 0) {
        Atomics.store(words, GO, 2);
        Atomics.notify(words, GO);
      }
      await Promise.all(started.map((worker) => worker.terminate()));
    },
  });
}

export { TsuzuriTrap };
