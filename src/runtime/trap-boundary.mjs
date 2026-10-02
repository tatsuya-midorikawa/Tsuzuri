// Calls exports of a compiled Tsuzuri module and reports traps as values (E14).
// A trap leaves the instance's heap and shadow stack half-updated, so the instance is dropped and
// the next call builds a fresh one. Build the module with --trap-info and pass the table's `sites`
// to learn where a trap happened.

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

export function createBoundary(module, { imports = {}, sites = [] } = {}) {
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
      try {
        return { ok: true, value: exports[name](...args) };
      } catch (error) {
        instance = null;
        if (error === hostError) throw error;
        if (error instanceof WebAssembly.RuntimeError) {
          return { ok: false, trap: describe("trap", exports.tsuzuri_trap_site?.() ?? 0, sites) };
        }
        if (error instanceof RangeError || error?.name === "InternalError") {
          return { ok: false, trap: { reason: "stack", site: 0 } };
        }
        throw error;
      }
    },
  };
}
