// The runtime core of the bindings that `tsuzuri build --target wasm32 --emit bindings-js` generates
// (E13). The generator writes `const TABLE = {...};` before this text, the module's exports, host
// imports and records as descriptors, and `bindings.mjs` (one instance) or `bindings-threads.mjs`
// (a thread pool) after it. The runtime imports no module, fetches nothing, and keeps no global
// state; each `load` owns its compiled module and instances.

const encoder = new TextEncoder();
const decoder = new TextDecoder("utf-8", { fatal: true });
const tagOf = (value) => Object.prototype.toString.call(value);
const NO_ERROR = Symbol("no host error");
const ARRAYS = {
  i64: [BigInt64Array, 8, "[object BigInt64Array]", "a BigInt64Array"],
  f64: [Float64Array, 8, "[object Float64Array]", "a Float64Array"],
  ubyte: [Uint8Array, 1, "[object Uint8Array]", "a Uint8Array"],
};
const NARROW = { i8: [-128, 127], i16: [-32768, 32767], i32: [-2147483648, 2147483647], i8u: [0, 255], i16u: [0, 65535], i32u: [0, 4294967295] };

/** A trap or stack exhaustion inside the module. The instance that raised it is discarded. */
class TsuzuriTrap extends Error {
  constructor(trap, cause) {
    super(trap.reason === "stack" ? "stack exhausted"
      : trap.path === undefined ? `trap (site ${trap.site})`
        : `trap at ${trap.path}:${trap.line}:${trap.column} (${trap.kind})`, { cause });
    this.name = "TsuzuriTrap";
    this.trap = trap;
  }
}

// A rejected value, thrown between the converters and rethrown as a TypeError or RangeError that
// names the argument and the field.
class Mismatch {
  constructor(range, text) {
    this.range = range;
    this.text = text;
    this.path = [];
  }
}
const mismatch = (text) => new Mismatch(false, text);
const outOfRange = (type) => new Mismatch(true, type);

function located(error, at) {
  if (!(error instanceof Mismatch)) return error;
  const path = error.path.reduce((text, part) => part.startsWith("[") ? text + part : text ? `${text}.${part}` : part, "");
  const where = path ? `${at} field '${path}'` : at;
  return error.range ? new RangeError(`${where} is out of range for ${error.text}`) : new TypeError(`${where} must be ${error.text}`);
}

function inField(error, part) {
  if (error instanceof Mismatch) error.path.unshift(part);
  return error;
}

const memoryOf = (state) => state.exports.memory.buffer;
const allocate = (state, bytes) => state.exports.tsuzuri_alloc(BigInt(bytes)) >>> 0;

function decodeUnits(units) {
  let text = "";
  for (let index = 0; index < units.length; index += 4096) {
    text += String.fromCharCode.apply(null, units.subarray(index, index + 4096));
  }
  return text;
}

// Copies `length` elements at `pointer` out of the module's memory.
function readBuffer(state, kind, pointer, length) {
  if (length === 0) return kind === "string" || kind === "utf8string" ? "" : new ARRAYS[kind][0](0);
  const memory = memoryOf(state);
  if (kind === "string") return decodeUnits(new Uint16Array(memory, pointer, length));
  if (kind === "utf8string") return decoder.decode(new Uint8Array(memory, pointer, length).slice());
  return new ARRAYS[kind][0](memory, pointer, length).slice();
}

// Copies a checked value into a new allocation and returns [pointer, length].
function writeBuffer(state, kind, value) {
  const length = value.length;
  if (length === 0) return [0, 0];
  if (kind === "string") {
    const pointer = allocate(state, length * 2);
    const units = new Uint16Array(memoryOf(state), pointer, length);
    for (let index = 0; index < length; index++) units[index] = value.charCodeAt(index);
    return [pointer, length];
  }
  const [View, width] = kind === "utf8string" ? [Uint8Array, 1] : ARRAYS[kind];
  const pointer = allocate(state, length * width);
  new View(memoryOf(state), pointer, length).set(value);
  return [pointer, length];
}

// A scalar converter: `check` validates a JavaScript value, `lower` makes the WASM value, `lift`
// reads a WASM value, and `read`/`write` access a field of a normalized C record.
function scalar(type) {
  if (type in NARROW) {
    const [minimum, maximum] = NARROW[type];
    const unsigned = type.endsWith("u");
    return {
      check(value) {
        if (typeof value !== "number" || !Number.isInteger(value)) throw mismatch("an integer");
        if (value < minimum || value > maximum) throw outOfRange(type);
        return value;
      },
      lower: (value) => value,
      lift: unsigned ? (raw) => raw >>> 0 : (raw) => raw,
      read: unsigned ? (view, at) => view.getUint32(at, true) : (view, at) => view.getInt32(at, true),
      write: unsigned ? (view, at, value) => view.setUint32(at, value, true) : (view, at, value) => view.setInt32(at, value, true),
    };
  }
  switch (type) {
    case "i64":
    case "i64u": {
      const signed = type === "i64";
      return {
        check(value) {
          if (typeof value !== "bigint") throw mismatch("a bigint");
          if ((signed ? BigInt.asIntN(64, value) : BigInt.asUintN(64, value)) !== value) throw outOfRange(type);
          return value;
        },
        lower: (value) => value,
        lift: signed ? (raw) => raw : (raw) => BigInt.asUintN(64, raw),
        read: signed ? (view, at) => view.getBigInt64(at, true) : (view, at) => view.getBigUint64(at, true),
        write: signed ? (view, at, value) => view.setBigInt64(at, value, true) : (view, at, value) => view.setBigUint64(at, value, true),
      };
    }
    case "f32":
    case "f64": {
      const single = type === "f32";
      return {
        check(value) {
          if (typeof value !== "number") throw mismatch("a number");
          return value;
        },
        lower: (value) => value,
        lift: (raw) => raw,
        read: single ? (view, at) => view.getFloat32(at, true) : (view, at) => view.getFloat64(at, true),
        write: single ? (view, at, value) => view.setFloat32(at, value, true) : (view, at, value) => view.setFloat64(at, value, true),
      };
    }
    case "bool":
      return {
        check(value) {
          if (typeof value !== "boolean") throw mismatch("a boolean");
          return value;
        },
        lower: (value) => (value ? 1 : 0),
        lift: (raw) => raw !== 0,
        read: (view, at) => view.getInt32(at, true) !== 0,
        write: (view, at, value) => view.setInt32(at, value ? 1 : 0, true),
      };
    case "unit":
      return { check: (value) => value, lower: () => undefined, lift: () => undefined };
  }
  throw new Error(`unknown bindings descriptor '${type}'`);
}

function handle(name) {
  return {
    check(value) {
      if (typeof value !== "number" || !Number.isInteger(value)) throw mismatch(`a ${name} handle number`);
      if (value < 0 || value > 4294967295) throw outOfRange(name);
      return value;
    },
    lower: (value) => value,
    lift: (raw) => raw >>> 0,
  };
}

// Resolves the descriptors of the table once, so that calls never branch on descriptor strings.
function resolver() {
  const records = new Map();
  function record(name) {
    let converter = records.get(name);
    if (converter) return converter;
    const { size, fields } = TABLE.records[name];
    const parts = [];
    converter = {
      size,
      check(value) {
        if (typeof value !== "object" || value === null) throw mismatch(`a ${name} object`);
        const checked = new Array(parts.length);
        for (let index = 0; index < parts.length; index++) {
          const part = parts[index];
          try {
            checked[index] = part.converter.check(value[part.name]);
          } catch (error) {
            throw inField(error, part.name);
          }
        }
        return checked;
      },
      write(view, at, checked) {
        for (let index = 0; index < parts.length; index++) parts[index].converter.write(view, at + parts[index].offset, checked[index]);
      },
      read(view, at) {
        const value = {};
        for (const part of parts) {
          const field = part.converter.read(view, at + part.offset);
          if (part.name === "__proto__") Object.defineProperty(value, part.name, { value: field, enumerable: true, writable: true, configurable: true });
          else value[part.name] = field;
        }
        return value;
      },
    };
    records.set(name, converter);
    for (const [field, offset, type] of fields) {
      const converter = resolve(type);
      parts.push({ name: field, offset, converter: converter.record ?? converter });
    }
    return converter;
  }
  function array(element, length) {
    const stride = typeof element === "string" && /64/.test(element) ? 8 : 4;
    const converter = resolve(element);
    return {
      check(value) {
        if (!Array.isArray(value) || value.length !== length) throw mismatch(`an array of ${length} elements`);
        const checked = new Array(length);
        for (let index = 0; index < length; index++) {
          try {
            checked[index] = converter.check(value[index]);
          } catch (error) {
            throw inField(error, `[${index}]`);
          }
        }
        return checked;
      },
      write(view, at, checked) {
        for (let index = 0; index < length; index++) converter.write(view, at + index * stride, checked[index]);
      },
      read(view, at) {
        const value = new Array(length);
        for (let index = 0; index < length; index++) value[index] = converter.read(view, at + index * stride);
        return value;
      },
    };
  }
  function resolve(type) {
    if (Array.isArray(type)) {
      if (type[0] === "array") return array(type[1], type[2]);
      return { callback: true, parameters: type[1].map(resolve), result: resolve(type[2]) };
    }
    const colon = type.indexOf(":");
    if (colon < 0) return scalar(type);
    const kind = type.slice(0, colon), name = type.slice(colon + 1);
    if (kind === "record") return { record: record(name) };
    if (kind === "handle") return handle(name);
    if (kind === "slice") return { slice: name };
    if (kind === "buffer") return { buffer: name };
    throw new Error(`unknown bindings descriptor '${type}'`);
  }
  return resolve;
}

// The memory that `withBorrowed` lends to a callback. Private fields keep the pointer out of reach.
class Borrowed {
  #state;
  #kind;
  #pointer;
  #length;
  #live = true;
  constructor(state, kind, pointer, length) {
    this.#state = state;
    this.#kind = kind;
    this.#pointer = pointer;
    this.#length = length;
  }
  get length() {
    return this.#length;
  }
  view() {
    if (!Borrowed.live(this)) throw new TypeError("this Borrowed buffer was released or its instance was discarded");
    return new ARRAYS[this.#kind][0](memoryOf(this.#state), this.#pointer, this.#length);
  }
  static live(borrowed) {
    return borrowed.#live && borrowed.#state.alive && borrowed.#state.current();
  }
  static use(borrowed, kind) {
    if (borrowed.#kind !== kind) return undefined;
    if (!Borrowed.live(borrowed)) throw mismatch(`a live Borrowed<${ARRAYS[kind][0].name}>`);
    return [borrowed.#pointer, borrowed.#length];
  }
  static release(borrowed) {
    if (!Borrowed.live(borrowed)) return void (borrowed.#live = false);
    borrowed.#live = false;
    if (borrowed.#pointer !== 0) borrowed.#state.exports.tsuzuri_free(borrowed.#pointer);
  }
}

// Whether the bytes of a module declare or import a 64-bit memory.
function memory64(bytes) {
  const data = ArrayBuffer.isView(bytes) ? new Uint8Array(bytes.buffer, bytes.byteOffset, bytes.byteLength) : new Uint8Array(bytes);
  let position = 8;
  const leb = () => {
    let value = 0, shift = 0, byte;
    do {
      byte = data[position++] ?? 0;
      value += (byte & 127) * 2 ** shift;
      shift += 7;
    } while (byte & 128 && position < data.length);
    return value;
  };
  const skipName = () => { position += leb(); };
  const limits = () => {
    const flags = data[position++];
    leb();
    if (flags & 1) leb();
    return (flags & 4) !== 0;
  };
  if (data[0] !== 0 || data[1] !== 0x61 || data[2] !== 0x73 || data[3] !== 0x6d) return false;
  while (position < data.length) {
    const id = data[position++], end = leb() + position;
    if (id === 2) {
      for (let count = leb(); count > 0 && position < end; count--) {
        skipName();
        skipName();
        const kind = data[position++];
        if (kind === 0) leb();
        else if (kind === 1) { position++; limits(); }
        else if (kind === 2) { if (limits()) return true; }
        else if (kind === 3) position += 2;
        else { position++; leb(); }
      }
    } else if (id === 5) {
      for (let count = leb(); count > 0 && position < end; count--) if (limits()) return true;
    }
    position = end;
  }
  return false;
}

const UNSUPPORTED = {
  tsuzuri_io: "build a library without IO main or --debug-output",
  tsuzuri_debug: "build a library without IO main or --debug-output",
  tsuzuri_heap: "build the .wasm without --allocator host",
  wasi_snapshot_preview1: "build the .wasm without --wasm-host wasi",
};

// The exports that a module of --wasm-feature threads adds for its host.
const POOL_EXPORTS = ["__stack_pointer", "tsuzuri_thread_entry", "tsuzuri_thread_stack_alloc", "tsuzuri_thread_stack_size", "tsuzuri_threads_control", "tsuzuri_threads_init"];

// Checks the module against the table before any instance exists. `threads` bindings expect the
// shared memory and thread imports of --wasm-feature threads, and the others reject them. Returns
// the table's imports that the module declares, in the order of the table.
function inspect(module, threads) {
  const moduleImports = WebAssembly.Module.imports(module);
  const pooled = moduleImports.some((entry) => entry.module === "tsuzuri_threads");
  if (pooled && !threads) {
    throw new Error("bindings do not support modules built with --wasm-feature threads; generate the bindings with --wasm-feature threads as well");
  }
  if (!pooled && threads) {
    throw new Error("threads bindings need a module built with --wasm-feature threads; generate the bindings without --wasm-feature threads for this module");
  }
  const key = (namespace, name) => `${namespace}\u0000${name}`;
  const declared = new Set(TABLE.imports.map(([name, namespace]) => key(namespace, name)));
  const present = new Set();
  for (const { module: namespace, name, kind } of moduleImports) {
    if (threads && (namespace === "tsuzuri_threads" || (namespace === "env" && name === "memory" && kind === "memory"))) continue;
    if (kind === "function" && declared.has(key(namespace, name))) {
      present.add(key(namespace, name));
    } else if (namespace in UNSUPPORTED || namespace.startsWith("tsuzuri_")) {
      throw new Error(`bindings do not support imports from '${namespace}'; ${UNSUPPORTED[namespace] ?? "build the .wasm from the same sources without that option"}`);
    } else {
      throw new Error(`module does not match bindings: unexpected import '${namespace}.${name}'; regenerate the bindings from the same sources`);
    }
  }
  // The imports that the module declares, in the order of the table.
  const used = TABLE.imports.filter(([name, namespace]) => present.has(key(namespace, name)));
  const exported = new Set(WebAssembly.Module.exports(module).map((entry) => entry.name));
  const required = TABLE.exports.map(([name]) => `tz_${name}`);
  if (TABLE.hostAbi) required.push("memory", "tsuzuri_alloc", "tsuzuri_free");
  if (used.some(([, , parameters]) => parameters.some(Array.isArray))) required.push("__indirect_function_table");
  if (threads) required.push(...POOL_EXPORTS);
  for (const name of required) {
    if (!exported.has(name)) throw new Error(`module does not match bindings: missing export '${name}'; regenerate the bindings from the same sources`);
  }
  return used;
}

function checkHostImports(used, hostImports) {
  for (const [name] of used) {
    if (typeof hostImports?.[name] !== "function") throw new TypeError(`missing import '${name}'`);
  }
}

// Checks the source of `load` and compiles it; a 64-bit memory is refused before the engine sees it.
async function compileSource(source) {
  if (!(source instanceof WebAssembly.Module || ArrayBuffer.isView(source) || tagOf(source) === "[object ArrayBuffer]")) {
    throw new TypeError("load source must be the bytes of a .wasm file or a WebAssembly.Module");
  }
  if (source instanceof WebAssembly.Module) return source;
  if (memory64(source)) throw new Error("bindings support wasm32 modules only; build the .wasm with --target wasm32");
  return WebAssembly.compile(source);
}

// The checks of the arguments of one export, before any instance is touched. `borrow` accepts the
// Borrowed buffers of `withBorrowed` for slices.
function argumentChecker(name, parameters, borrow) {
  const count = parameters.length;
  const at = parameters.map((_, position) => `argument ${position} of '${name}'`);
  return (args) => {
    if (args.length !== count) throw new TypeError(`'${name}' expects ${count} argument${count === 1 ? "" : "s"}`);
    const checked = new Array(count);
    for (let position = 0; position < count; position++) {
      const converter = parameters[position];
      try {
        checked[position] = converter.slice !== undefined ? checkSlice(converter.slice, args[position], borrow)
          : converter.record !== undefined ? converter.record.check(args[position])
            : converter.check(args[position]);
      } catch (error) {
        throw located(error, at[position]);
      }
    }
    return checked;
  };
}

function checkBuffer(kind, value) {
  if (kind === "string") {
    if (typeof value !== "string") throw mismatch("a string");
    return value;
  }
  if (kind === "utf8string") {
    if (typeof value !== "string" || !value.isWellFormed()) throw mismatch("a well-formed string");
    return encoder.encode(value);
  }
  if (tagOf(value) !== ARRAYS[kind][2]) throw mismatch(ARRAYS[kind][3]);
  return value;
}

function checkSlice(kind, value, borrow) {
  if (borrow && value instanceof Borrowed) {
    const borrowed = Borrowed.use(value, kind);
    if (borrowed) return borrowed;
  }
  if (kind in ARRAYS && tagOf(value) !== ARRAYS[kind][2]) {
    throw mismatch(borrow ? `${ARRAYS[kind][3]} or a Borrowed<${ARRAYS[kind][0].name}>` : ARRAYS[kind][3]);
  }
  return checkBuffer(kind, value);
}

// Binds the table to one compiled module: the converters, the export functions, `withBorrowed`, and
// the rule that an exception discards the instance. `extra(owner)` adds imports beside the table's.
// With `recreate` false a discarded instance is final, as a thread pool cannot be rebuilt in place;
// `trapOf(own)` may replace the trap that a call reports.
function bind(module, used, hostImports, sites, { extra, recreate = true, trapOf = (own) => own } = {}) {
  const resolve = resolver();
  const siteTable = new Map(sites.map((site) => [site.id, site]));
  let state;
  let hostError = NO_ERROR;
  let depth = 0;
  let final;

  function describe({ reason, site }) {
    const entry = reason === "trap" && site !== 0 ? siteTable.get(site) : undefined;
    return entry ? { reason, site, kind: entry.kind, path: entry.path, line: entry.span?.line, column: entry.span?.column } : { reason, site };
  }

  // Discards the instance of `failed` and returns what the call throws: the host's own error, the
  // failure that discarded the instance before, or the classified trap.
  function fail(failed, error) {
    let thrown;
    if (error === hostError) {
      thrown = error;
    } else if (!failed.alive) {
      return failed.failure;
    } else if (error instanceof WebAssembly.RuntimeError) {
      let site = 0;
      try {
        site = (failed.exports.tsuzuri_trap_site?.() ?? 0) >>> 0;
      } catch {
        site = 0;
      }
      thrown = new TsuzuriTrap(describe(trapOf({ reason: "trap", site })), error);
    } else if (error instanceof RangeError || error?.name === "InternalError") {
      thrown = new TsuzuriTrap(describe(trapOf({ reason: "stack", site: 0 })), error);
    } else {
      thrown = error;
    }
    if (failed.alive) {
      failed.alive = false;
      failed.failure = thrown;
      if (state === failed) state = undefined;
      if (!recreate) final ??= thrown;
    }
    return thrown;
  }

  function callbackFunction(owner, converter, name, index) {
    const target = owner.exports.__indirect_function_table.get(index);
    const { parameters, result } = converter;
    const at = (position) => `argument ${position} of the callback of '${name}'`;
    let active = true;
    const callback = (...args) => {
      if (!active) throw new TypeError(`the callback of import '${name}' is only valid during that import call`);
      if (args.length !== parameters.length) throw new TypeError(`the callback of '${name}' expects ${parameters.length} argument${parameters.length === 1 ? "" : "s"}`);
      const lowered = new Array(args.length);
      for (let position = 0; position < args.length; position++) {
        try {
          lowered[position] = parameters[position].lower(parameters[position].check(args[position]));
        } catch (error) {
          throw located(error, at(position));
        }
      }
      if (!owner.alive) throw owner.failure;
      let raw;
      try {
        raw = target(...lowered);
      } catch (error) {
        throw fail(owner, error);
      }
      if (!owner.alive) throw owner.failure;
      return result.lift(raw);
    };
    return [callback, () => { active = false; }];
  }

  // The import object of one instance: each host function behind its converters.
  function wrapImports(owner) {
    const object = {};
    for (const [name, namespace, parameterTypes, resultType] of used) {
      const host = hostImports[name];
      const parameters = parameterTypes.map(resolve);
      const result = resolve(resultType);
      const out = result.buffer !== undefined || result.record !== undefined;
      const imported = function (...raw) {
        if (!owner.alive) throw owner.failure;
        let position = 0;
        const pointer = out ? raw[position++] >>> 0 : 0;
        const args = [];
        const release = [];
        let value;
        try {
          for (const converter of parameters) {
            if (converter.slice !== undefined) {
              args.push(readBuffer(owner, converter.slice, raw[position] >>> 0, Number(raw[position + 1])));
              position += 2;
            } else if (converter.record !== undefined) {
              args.push(converter.record.read(new DataView(memoryOf(owner)), raw[position++] >>> 0));
            } else if (converter.callback) {
              const [callback, deactivate] = callbackFunction(owner, converter, name, raw[position++]);
              args.push(callback);
              release.push(deactivate);
            } else {
              args.push(converter.lift(raw[position++]));
            }
          }
          value = host(...args);
        } catch (error) {
          hostError = error;
          throw error;
        } finally {
          for (const deactivate of release) deactivate();
        }
        try {
          if (!owner.alive) throw owner.failure;
          if (result.buffer !== undefined) {
            const checked = checkBuffer(result.buffer, value);
            const [address, length] = writeBuffer(owner, result.buffer, checked);
            const view = new DataView(memoryOf(owner));
            view.setUint32(pointer, address, true);
            view.setBigInt64(pointer + 8, BigInt(length), true);
            return undefined;
          }
          if (result.record !== undefined) {
            const checked = result.record.check(value);
            result.record.write(new DataView(memoryOf(owner)), pointer, checked);
            return undefined;
          }
          return result.lower(result.check(value));
        } catch (error) {
          const thrown = located(error, `result of import '${name}'`);
          hostError = thrown;
          throw thrown;
        }
      };
      (object[namespace] ??= {})[name] = imported;
    }
    return object;
  }

  function createState() {
    const created = { alive: true, failure: undefined, exports: undefined, current: () => state === created };
    return created;
  }

  function importObject(owner) {
    const object = wrapImports(owner);
    for (const [namespace, values] of Object.entries(extra?.(owner) ?? {})) object[namespace] = { ...object[namespace], ...values };
    return object;
  }

  // The instance for a call: after a failure, a new one created synchronously. A browser's main
  // thread refuses that for modules over 8 MB; `ready()` creates one asynchronously instead.
  function current() {
    if (state === undefined) {
      if (final !== undefined) throw new Error("the WASM thread pool stopped after a failure; load the module again", { cause: final });
      const created = createState();
      try {
        created.exports = new WebAssembly.Instance(module, importObject(created)).exports;
      } catch (cause) {
        throw new Error("the WASM instance could not be recreated synchronously after a failure; await ready() to recreate it asynchronously, then call again", { cause });
      }
      state = created;
    }
    return state;
  }

  let pending;
  function ready() {
    if (state !== undefined) return Promise.resolve();
    pending ??= (async () => {
      const created = createState();
      try {
        created.exports = (await WebAssembly.instantiate(module, importObject(created))).exports;
        // A call may have created an instance synchronously meanwhile; that one stays.
        state ??= created;
      } finally {
        pending = undefined;
      }
    })();
    return pending;
  }

  // Runs `body` against the current instance; any exception discards it.
  function enter(owner, body) {
    if (depth++ === 0) hostError = NO_ERROR;
    try {
      const value = body();
      if (!owner.alive) throw owner.failure;
      return value;
    } catch (error) {
      throw fail(owner, error);
    } finally {
      depth--;
    }
  }

  function exportFunction(name, parameterTypes, resultType) {
    const parameters = parameterTypes.map(resolve);
    const result = resolve(resultType);
    const target = `tz_${name}`;
    const count = parameters.length;
    const check = argumentChecker(name, parameters, true);
    return function (...args) {
      const checked = check(args);
      const owner = current();
      return enter(owner, () => {
        const lowered = [];
        const inputs = [];
        let out = 0;
        for (let position = 0; position < count; position++) {
          const converter = parameters[position];
          const value = checked[position];
          if (converter.slice !== undefined) {
            if (Array.isArray(value)) {
              lowered.push(value[0], BigInt(value[1]));
            } else {
              const [pointer, length] = writeBuffer(owner, converter.slice, value);
              if (pointer !== 0) inputs.push(pointer);
              lowered.push(pointer, BigInt(length));
            }
          } else if (converter.record !== undefined) {
            const pointer = allocate(owner, converter.record.size);
            inputs.push(pointer);
            converter.record.write(new DataView(memoryOf(owner)), pointer, value);
            lowered.push(pointer);
          } else {
            lowered.push(converter.lower(value));
          }
        }
        if (result.buffer !== undefined) out = allocate(owner, 16);
        else if (result.record !== undefined) out = allocate(owner, result.record.size);
        const exported = owner.exports[target];
        const raw = out === 0 ? exported(...lowered) : exported(out, ...lowered);
        if (!owner.alive) throw owner.failure;
        let value;
        if (result.buffer !== undefined) {
          const view = new DataView(memoryOf(owner));
          const pointer = view.getUint32(out, true);
          const length = Number(view.getBigInt64(out + 8, true));
          value = readBuffer(owner, result.buffer, pointer, length);
          owner.exports.tsuzuri_free(pointer);
        } else if (result.record !== undefined) {
          value = result.record.read(new DataView(memoryOf(owner)), out);
        } else {
          value = result.lift(raw);
        }
        if (out !== 0) owner.exports.tsuzuri_free(out);
        for (let index = inputs.length - 1; index >= 0; index--) owner.exports.tsuzuri_free(inputs[index]);
        return value;
      });
    };
  }

  const exports = {};
  for (const [name, parameters, result] of TABLE.exports) {
    Object.defineProperty(exports, name, { value: exportFunction(name, parameters, result), enumerable: true });
  }
  Object.freeze(exports);

  function withBorrowed(kind, length, callback) {
    if (!Object.hasOwn(ARRAYS, kind)) throw new TypeError('withBorrowed kind must be "i64", "f64", or "ubyte"');
    if (!Number.isSafeInteger(length) || length < 0) throw new RangeError("withBorrowed length must be a non-negative safe integer");
    if (typeof callback !== "function") throw new TypeError("withBorrowed callback must be a function");
    if (!TABLE.hostAbi) throw new TypeError("withBorrowed needs a module whose exports take buffers");
    const owner = current();
    const pointer = length === 0 ? 0 : enter(owner, () => owner.exports.tsuzuri_alloc(BigInt(length) * BigInt(ARRAYS[kind][1])) >>> 0);
    const borrowed = new Borrowed(owner, kind, pointer, length);
    let value;
    try {
      value = callback(borrowed);
    } finally {
      Borrowed.release(borrowed);
    }
    if (value !== null && (typeof value === "object" || typeof value === "function") && typeof value.then === "function") {
      throw new TypeError("withBorrowed callbacks must return synchronously; the buffer is released when the callback returns");
    }
    return value;
  }

  return {
    exports,
    withBorrowed,
    ready,
    // Creates the first instance asynchronously, as a browser's main thread requires.
    async start() {
      const first = createState();
      first.exports = (await WebAssembly.instantiate(module, importObject(first))).exports;
      state = first;
      return first;
    },
    // Whether an instance is running: an exception discards it, so a failed call leaves none.
    get alive() {
      return state !== undefined;
    },
  };
}
