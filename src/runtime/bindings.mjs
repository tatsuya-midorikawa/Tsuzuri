// The single-instance `load` of `--emit bindings-js` (E13), after the table and bindings-core.mjs.

/** Compiles and instantiates a wasm32 module built from the same sources as these bindings. */
export async function load(source, options = {}) {
  if (options === null || typeof options !== "object") throw new TypeError("load options must be an object");
  const { imports: hostImports = {}, sites = [] } = options;
  if (hostImports === null || typeof hostImports !== "object") throw new TypeError("load option 'imports' must be an object");
  if (!Array.isArray(sites)) throw new TypeError("load option 'sites' must be an array of trap sites");
  const module = await compileSource(source);
  const used = inspect(module, false);
  checkHostImports(used, hostImports);
  const bound = bind(module, used, hostImports, sites);
  await bound.start();
  const bindings = { exports: bound.exports, ready: bound.ready };
  if (!TABLE.async?.jspi) bindings.withBorrowed = bound.withBorrowed;
  // The executor of Async.start on this event loop (B08).
  if (bound.async !== undefined) bindings.async = bound.async;
  return Object.freeze(bindings);
}

export { TsuzuriTrap };
