// Calls exports of a compiled Tsuzuri module and reports traps as values (E14).
// A trap leaves the instance's heap and shadow stack half-updated, so the instance is dropped and
// the next call builds a fresh one. Build the module with --trap-info and pass the table's `sites`
// to learn where a trap happened. With `frames: true` a trap also carries the chain of WebAssembly
// frames that led to it, innermost first (B10); a module built with -g names them.

const FRAME = /^\s*at (?:(.*?) \()?wasm:\/\/wasm\/[^:]*:wasm-function\[(\d+)\]:0x([0-9a-f]+)\)?$/;
const SUFFIX = /\.(?:\$mono\.\d+|cold\.\d+|cpu\.[a-z0-9_]+)$/;

// The name that a frame shows for a linkage name, or undefined for a frame that is not shown
// (the table of docs/language.md, "トラップ位置").
function displayName(linkage) {
  let rest;
  if (linkage.startsWith("tz.fn.")) rest = linkage.slice("tz.fn.".length);
  else if (linkage.startsWith("tz_") && linkage.length > 3) rest = linkage.slice(3);
  else return /^tz\.specialized\.\d+$/.test(linkage) ? linkage : undefined;
  while (SUFFIX.test(rest)) rest = rest.replace(SUFFIX, "");
  return rest === "" ? undefined : rest;
}

// The linkage name in a frame's name and the name to show for it, or undefined. V8 puts the module's name
// before the function's name when the name section has one (`artifact.tz.fn.Main.div`, `my.app.tz_div`), and the
// module's name may contain dots, so the linkage name is the first suffix after a dot (or the whole name) that the
// table names. An export's entry `tz_<name>` has no dot of its own besides the clone suffixes.
function linkageOf(name) {
  for (let start = 0; ; ) {
    const rest = name.slice(start);
    const shown = displayName(rest);
    if (shown !== undefined && !(rest.startsWith("tz_") && shown.includes("."))) return { linkage: rest, shown };
    const dot = name.indexOf(".", start);
    if (dot < 0) return undefined;
    start = dot + 1;
  }
}

// The frames of the stack of a WebAssembly.RuntimeError, innermost first; JavaScript frames are left out.
// Exported for hosts that catch the error themselves.
export function wasmFrames(error) {
  const frames = [];
  for (const line of String(error.stack).split("\n")) {
    const match = FRAME.exec(line);
    if (!match) continue;
    const found = match[1] === undefined ? undefined : linkageOf(match[1]);
    const linkage = found?.linkage;
    const name = found?.shown;
    const frame = { index: Number(match[2]), offset: parseInt(match[3], 16) };
    if (name === undefined) {
      frames.push(frame);
      continue;
    }
    const cold = linkage.includes(".cold.");
    const previous = frames[frames.length - 1];
    if (previous?.function === name && (cold || previous.cold)) {
      if (cold) continue;
      frames.pop();
    }
    frames.push({ function: name, ...frame, cold });
  }
  return frames.map(({ cold, ...frame }) => frame);
}

function describe(reason, site, sites) {
  const trap = { reason, site };
  const entry = site ? sites.find((candidate) => candidate.id === site) : undefined;
  if (entry) {
    trap.kind = entry.kind;
    trap.path = entry.path;
    trap.line = entry.span.line;
    trap.column = entry.span.column;
  }
  return trap;
}

export function createBoundary(module, { imports = {}, sites = [], frames = false } = {}) {
  if (WebAssembly.Module.imports(module).some(({ module: namespace }) => namespace === "tsuzuri_threads")) {
    throw new Error("createBoundary does not support modules built with --wasm-feature threads; use createThreadPool");
  }
  let instance = null;
  let hostError = null;
  const wrapped = {};
  for (const [namespace, members] of Object.entries(imports)) {
    wrapped[namespace] = {};
    for (const [name, member] of Object.entries(members)) {
      wrapped[namespace][name] = typeof member === "function"
        ? (...args) => {
          try { return member(...args); } catch (error) { hostError = error; throw error; }
        }
        : member;
    }
  }
  const current = () => instance ??= new WebAssembly.Instance(module, wrapped);
  return {
    get exports() { return current().exports; },
    call(name, ...args) {
      const exports = current().exports;
      if (typeof exports[name] !== "function") throw new TypeError(`unknown export ${name}`);
      hostError = null;
      const limit = Error.stackTraceLimit;
      if (frames) Error.stackTraceLimit = Math.max(limit, 33);
      try {
        return { ok: true, value: exports[name](...args) };
      } catch (error) {
        instance = null;
        if (error === hostError) throw error;
        if (error instanceof WebAssembly.RuntimeError) {
          const trap = describe("trap", exports.tsuzuri_trap_site?.() ?? 0, sites);
          if (frames) trap.frames = wasmFrames(error);
          return { ok: false, trap };
        }
        if (error instanceof RangeError || error?.name === "InternalError") {
          return { ok: false, trap: { reason: "stack", site: 0 } };
        }
        throw error;
      } finally {
        if (frames) Error.stackTraceLimit = limit;
      }
    },
  };
}
