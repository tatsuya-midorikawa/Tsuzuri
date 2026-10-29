import { readFileSync, writeSync } from "node:fs";

// `node run.mjs tests.wasm [overflow.json] index`: the side table of the overflow report (A23)
// comes between the module and the index when the runner reports the arithmetic that wraps.
if (![4, 5].includes(process.argv.length) || !/^[0-9]+$/.test(process.argv.at(-1))) process.exit(2);
const index = Number(process.argv.at(-1));
const overflow = process.argv.length === 5 ? JSON.parse(readFileSync(process.argv[3], "utf8")) : null;
const module = new WebAssembly.Module(readFileSync(process.argv[2]));
const imports = WebAssembly.Module.imports(module);
// Only a program that uses property tests imports the Debug output, for its reports.
if (imports.some(entry => entry.module !== "tsuzuri_debug" || entry.name !== "write")) throw new Error("test runner must not import host functions other than tsuzuri_debug.write");
let memory;
const host = imports.length === 0 ? {} : {
  tsuzuri_debug: {
    write(pointer, length) {
      writeSync(2, new Uint8Array(memory.buffer, Number(pointer), Number(length)));
      writeSync(2, "\n");
    },
  },
};
const instance = new WebAssembly.Instance(module, host);
memory = instance.exports.memory;
if (!Number.isSafeInteger(index) || index >= instance.exports.tsuzuri_test_count()) process.exit(2);
try {
  process.exitCode = instance.exports.tsuzuri_test_run(index);
} catch (error) {
  // A trap, or the engine's stack limit under deep recursion (`RangeError` in V8, `InternalError`
  // elsewhere), as `trap-boundary.mjs` reads them: the test fails, and still reports below.
  if (error instanceof RangeError || error?.name === "InternalError") {
    writeSync(2, `${error.name}: ${error.message}; the stack was probably exhausted by deep recursion\n`);
  } else if (!(error instanceof WebAssembly.RuntimeError)) {
    throw error;
  }
  process.exitCode = 1;
}
// The sites that wrapped, in site order, as a native runner writes them: at most 100 lines, then
// one line that says the report stops. A test that trapped keeps its failure, and still reports.
if (overflow) {
  let written = 0;
  for (let site = 0; site < instance.exports.tsuzuri_overflow_count(); site += 1) {
    if (instance.exports.tsuzuri_overflow_hit(site) !== 1) continue;
    if (written === 100) {
      writeSync(2, "warning: integer overflow reports stop after 100 sites\n");
      break;
    }
    writeSync(2, `warning: ${overflow.sites[site].message}\n`);
    written += 1;
  }
}